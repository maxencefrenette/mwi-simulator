use std::collections::HashMap;

use anyhow::bail;
use serde::Serialize;

use crate::domain::{MarketSnapshot, OrderSide};
use crate::history::MarketHistoryCache;
use crate::market_price::{PriceBinDirection, bin_market_price, market_price_step};
use crate::money_actions::MoneyAction;
use crate::player::PlayerExport;

use super::rank::RankedAction;

const PACKAGE_HOURS: f64 = 24.0;
const MIN_FILL_DAYS: f64 = 0.25;
const PASSIVE_PRICE_REACH_FLOOR: f64 = 0.10;

#[derive(Debug, Clone, Copy)]
pub(super) struct OrderPolicyConfig {
    pub max_orders: usize,
    pub alternatives: usize,
    pub tick_size: f64,
    pub volume_participation_rate: f64,
    pub daily_discount_rate: f64,
    pub daily_capital_cost_rate: f64,
}

impl Default for OrderPolicyConfig {
    fn default() -> Self {
        Self {
            max_orders: 10,
            alternatives: 5,
            tick_size: 1.0,
            volume_participation_rate: 0.05,
            daily_discount_rate: 0.05,
            daily_capital_cost_rate: 0.001,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct OrderPolicyRecommendation {
    pub available_cash: f64,
    pub total_order_slots: usize,
    pub occupied_order_slots: usize,
    pub free_order_slots: usize,
    pub package_hours: f64,
    pub baseline_action: Option<BaselineAction>,
    pub recommendation: Option<ActionPackageOrders>,
    pub alternatives: Vec<ActionPackageOrders>,
    pub assumptions: OrderPolicyAssumptions,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct BaselineAction {
    pub action: String,
    pub name: String,
    pub package_profit: f64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ActionPackageOrders {
    pub action: String,
    pub name: String,
    pub action_type: String,
    pub adjusted_revenue: f64,
    pub package_profit_at_suggested_prices: f64,
    pub profit_uplift_over_baseline: f64,
    pub discounted_uplift: f64,
    pub cash_required: f64,
    pub slots_required: usize,
    pub expected_fill_days: f64,
    pub expected_slot_days: f64,
    pub score_per_slot_day: f64,
    pub input_coverage: Vec<PackageInputCoverage>,
    pub orders: Vec<MarketOrderRecommendation>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PackageInputCoverage {
    pub item: String,
    pub required_quantity: f64,
    pub inventory_covered_quantity: f64,
    pub pending_buy_covered_quantity: f64,
    pub new_order_quantity: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MarketOrderRecommendation {
    pub side: OrderSide,
    pub item: String,
    pub quantity: u64,
    pub suggested_limit_price: f64,
    pub price_bin_size: u64,
    pub maximum_profitable_price: f64,
    pub cash_required: f64,
    pub historical_daily_volume: f64,
    pub historical_price_reach_rate: f64,
    pub estimated_fill_days: f64,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct OrderPolicyAssumptions {
    pub volume_participation_rate: f64,
    pub daily_discount_rate: f64,
    pub daily_capital_cost_rate: f64,
    pub fill_model: String,
}

pub(crate) fn build_order_policy(
    player: &PlayerExport,
    market: &MarketSnapshot,
    actions: &[MoneyAction],
    ranked_actions: &[RankedAction],
    daily_volumes: &HashMap<String, f64>,
    histories: &HashMap<String, MarketHistoryCache>,
    config: OrderPolicyConfig,
) -> anyhow::Result<OrderPolicyRecommendation> {
    validate_config(config)?;
    let inventory = inventory_quantities(player);
    let pending_buys = pending_buy_orders(player);
    let vendor_prices = vendor_prices(player);
    let occupied_order_slots = player.derived.open_orders.len();
    let free_order_slots = config.max_orders.saturating_sub(occupied_order_slots);
    let ranked_by_action = ranked_actions
        .iter()
        .map(|action| (action.action.as_str(), action))
        .collect::<HashMap<_, _>>();

    let baseline_action = actions
        .iter()
        .filter_map(|action| {
            let ranked = ranked_by_action.get(action.action.as_str())?;
            feasible_package_profit(action, ranked, &inventory, market).map(|package_profit| {
                BaselineAction {
                    action: action.action.clone(),
                    name: action.name.clone(),
                    package_profit,
                }
            })
        })
        .max_by(|left, right| left.package_profit.total_cmp(&right.package_profit));
    let baseline_profit = baseline_action
        .as_ref()
        .map(|action| action.package_profit)
        .unwrap_or(0.0);

    let context = OrderPlanningContext {
        inventory: &inventory,
        pending_buys: &pending_buys,
        market,
        daily_volumes,
        histories,
        vendor_prices: &vendor_prices,
        baseline_profit,
        available_cash: player.derived.cash,
        free_order_slots,
        config,
    };
    let mut candidates = actions
        .iter()
        .filter_map(|action| {
            let ranked = ranked_by_action.get(action.action.as_str())?;
            context.build_candidate(action, ranked)
        })
        .collect::<Vec<_>>();

    candidates.sort_by(|left, right| {
        right
            .score_per_slot_day
            .total_cmp(&left.score_per_slot_day)
            .then_with(|| right.discounted_uplift.total_cmp(&left.discounted_uplift))
            .then_with(|| left.action.cmp(&right.action))
    });

    let recommendation = candidates.first().cloned();
    let alternatives = candidates
        .into_iter()
        .skip(1)
        .take(config.alternatives)
        .collect();

    Ok(OrderPolicyRecommendation {
        available_cash: player.derived.cash,
        total_order_slots: config.max_orders,
        occupied_order_slots,
        free_order_slots,
        package_hours: PACKAGE_HOURS,
        baseline_action,
        recommendation,
        alternatives,
        assumptions: OrderPolicyAssumptions {
            volume_participation_rate: config.volume_participation_rate,
            daily_discount_rate: config.daily_discount_rate,
            daily_capital_cost_rate: config.daily_capital_cost_rate,
            fill_model: "Expected fill delay uses the configured share of historical daily volume, scaled by how often historical asks reached the limit price; a conservative floor represents passive fills because queue depth is unavailable"
                .into(),
        },
    })
}

struct OrderPlanningContext<'a> {
    inventory: &'a HashMap<String, f64>,
    pending_buys: &'a HashMap<String, PendingBuy>,
    market: &'a MarketSnapshot,
    daily_volumes: &'a HashMap<String, f64>,
    histories: &'a HashMap<String, MarketHistoryCache>,
    vendor_prices: &'a HashMap<String, f64>,
    baseline_profit: f64,
    available_cash: f64,
    free_order_slots: usize,
    config: OrderPolicyConfig,
}

impl OrderPlanningContext<'_> {
    fn build_candidate(
        &self,
        action: &MoneyAction,
        ranked: &RankedAction,
    ) -> Option<ActionPackageOrders> {
        let inventory = self.inventory;
        let pending_buys = self.pending_buys;
        let market = self.market;
        let daily_volumes = self.daily_volumes;
        let histories = self.histories;
        let vendor_prices = self.vendor_prices;
        let baseline_profit = self.baseline_profit;
        let available_cash = self.available_cash;
        let free_order_slots = self.free_order_slots;
        let config = self.config;

        let requirements = package_requirements(action);
        if requirements.is_empty() {
            return None;
        }

        let mut input_coverage = Vec::new();
        let mut orders = Vec::new();
        let mut input_cost = 0.0;
        let mut expected_fill_days: f64 = 0.0;

        for (item, required_quantity) in requirements {
            let quote = market.items.get(&item)?;
            let inventory_available = inventory.get(&item).copied().unwrap_or(0.0);
            let inventory_quantity = inventory_available.min(required_quantity);
            input_cost += inventory_quantity * quote.bid?;

            let after_inventory = (required_quantity - inventory_quantity).max(0.0);
            let pending = pending_buys.get(&item);
            let pending_buy_quantity = pending
                .map(|buy| buy.quantity.min(after_inventory))
                .unwrap_or(0.0);
            if let Some(pending) = pending {
                input_cost += pending_buy_quantity * pending.average_limit_price;
                if pending_buy_quantity > 0.0 {
                    let daily_volume = *daily_volumes.get(&item)?;
                    let price_reach_rate = historical_price_reach_rate(
                        histories.get(&item),
                        pending.average_limit_price,
                    );
                    expected_fill_days = expected_fill_days.max(estimated_fill_days(
                        pending_buy_quantity,
                        daily_volume,
                        price_reach_rate,
                        config.volume_participation_rate,
                    ));
                }
            }

            let deficit = (after_inventory - pending_buy_quantity).max(0.0);
            let new_order_quantity = deficit.ceil() as u64;
            if new_order_quantity > 0 {
                let bid = quote.bid?;
                let desired_limit_price = quote
                    .ask
                    .map(|ask| (bid + config.tick_size).min(ask))
                    .unwrap_or(bid + config.tick_size);
                let vendor_price = vendor_prices.get(&item).copied().unwrap_or(0.0);
                let suggested_limit_price =
                    bin_market_price(desired_limit_price, PriceBinDirection::Up, vendor_price);
                let historical_daily_volume = *daily_volumes.get(&item)?;
                let historical_price_reach_rate =
                    historical_price_reach_rate(histories.get(&item), suggested_limit_price);
                let fill_days = estimated_fill_days(
                    new_order_quantity as f64,
                    historical_daily_volume,
                    historical_price_reach_rate,
                    config.volume_participation_rate,
                );
                let cash_required = new_order_quantity as f64 * suggested_limit_price;
                input_cost += cash_required;
                expected_fill_days = expected_fill_days.max(fill_days);
                orders.push(MarketOrderRecommendation {
                    side: OrderSide::Buy,
                    item: item.clone(),
                    quantity: new_order_quantity,
                    suggested_limit_price,
                    price_bin_size: market_price_step(suggested_limit_price as u64),
                    maximum_profitable_price: 0.0,
                    cash_required,
                    historical_daily_volume,
                    historical_price_reach_rate,
                    estimated_fill_days: fill_days,
                    reason: format!(
                        "Completes a {PACKAGE_HOURS:.0}h {} action package",
                        action.name
                    ),
                });
            }

            input_coverage.push(PackageInputCoverage {
                item,
                required_quantity,
                inventory_covered_quantity: inventory_quantity,
                pending_buy_covered_quantity: pending_buy_quantity,
                new_order_quantity,
            });
        }

        if orders.is_empty() || orders.len() > free_order_slots {
            return None;
        }

        input_coverage.sort_by(|left, right| left.item.cmp(&right.item));
        orders.sort_by(|left, right| left.item.cmp(&right.item));

        let cash_required = orders.iter().map(|order| order.cash_required).sum::<f64>();
        if cash_required > available_cash {
            return None;
        }

        let adjusted_revenue = ranked.adjusted_revenue_per_hour * PACKAGE_HOURS;
        let package_profit_at_suggested_prices = adjusted_revenue - input_cost;
        let profit_uplift_over_baseline = package_profit_at_suggested_prices - baseline_profit;
        if profit_uplift_over_baseline <= 0.0 {
            return None;
        }

        for order in &mut orders {
            let vendor_price = vendor_prices.get(&order.item).copied().unwrap_or(0.0);
            order.maximum_profitable_price = bin_market_price(
                order.suggested_limit_price + profit_uplift_over_baseline / order.quantity as f64,
                PriceBinDirection::Down,
                vendor_price,
            );
        }

        let expected_slot_days = orders
            .iter()
            .map(|order| order.estimated_fill_days)
            .sum::<f64>();
        let discount_factor = (1.0 / (1.0 + config.daily_discount_rate)).powf(expected_fill_days);
        let capital_carry_cost = orders
            .iter()
            .map(|order| {
                order.cash_required * order.estimated_fill_days * config.daily_capital_cost_rate
            })
            .sum::<f64>();
        let discounted_uplift = profit_uplift_over_baseline * discount_factor - capital_carry_cost;
        if discounted_uplift <= 0.0 || expected_slot_days <= 0.0 {
            return None;
        }

        Some(ActionPackageOrders {
            action: action.action.clone(),
            name: action.name.clone(),
            action_type: action.action_type.clone(),
            adjusted_revenue,
            package_profit_at_suggested_prices,
            profit_uplift_over_baseline,
            discounted_uplift,
            cash_required,
            slots_required: orders.len(),
            expected_fill_days,
            expected_slot_days,
            score_per_slot_day: discounted_uplift / expected_slot_days,
            input_coverage,
            orders,
        })
    }
}

fn feasible_package_profit(
    action: &MoneyAction,
    ranked: &RankedAction,
    inventory: &HashMap<String, f64>,
    market: &MarketSnapshot,
) -> Option<f64> {
    let mut input_cost = 0.0;
    for (item, required_quantity) in package_requirements(action) {
        if inventory.get(&item).copied().unwrap_or(0.0) < required_quantity {
            return None;
        }
        input_cost += required_quantity * market.items.get(&item)?.bid?;
    }

    Some(ranked.adjusted_revenue_per_hour * PACKAGE_HOURS - input_cost)
}

fn package_requirements(action: &MoneyAction) -> HashMap<String, f64> {
    let mut requirements = HashMap::new();
    for input in &action.inputs_per_hour {
        *requirements.entry(input.item.clone()).or_default() +=
            input.quantity_per_hour * PACKAGE_HOURS;
    }
    requirements
}

fn inventory_quantities(player: &PlayerExport) -> HashMap<String, f64> {
    let mut inventory = HashMap::new();
    for item in &player.derived.inventory {
        *inventory.entry(item.item.clone()).or_default() += item.quantity;
    }
    inventory
}

fn vendor_prices(player: &PlayerExport) -> HashMap<String, f64> {
    player
        .item_detail_dict
        .iter()
        .map(|(item_hrid, detail)| {
            (
                item_hrid
                    .strip_prefix("/items/")
                    .unwrap_or(item_hrid)
                    .to_string(),
                detail.sell_price,
            )
        })
        .collect()
}

#[derive(Debug, Clone, Copy)]
struct PendingBuy {
    quantity: f64,
    average_limit_price: f64,
}

fn pending_buy_orders(player: &PlayerExport) -> HashMap<String, PendingBuy> {
    let mut quantities = HashMap::<String, (f64, f64)>::new();
    for order in player
        .derived
        .open_orders
        .iter()
        .filter(|order| order.side == OrderSide::Buy && order.quantity > 0.0)
    {
        let entry = quantities.entry(order.item.clone()).or_default();
        entry.0 += order.quantity;
        entry.1 += order.quantity * order.limit_price;
    }

    quantities
        .into_iter()
        .filter_map(|(item, (quantity, total_cost))| {
            (quantity > 0.0).then_some((
                item,
                PendingBuy {
                    quantity,
                    average_limit_price: total_cost / quantity,
                },
            ))
        })
        .collect()
}

fn historical_price_reach_rate(history: Option<&MarketHistoryCache>, limit_price: f64) -> f64 {
    let Some(history) = history else {
        return PASSIVE_PRICE_REACH_FLOOR;
    };
    let asks = history.points.iter().filter_map(|point| point.ask);
    let (reached, observed) = asks.fold((0_u64, 0_u64), |(reached, observed), ask| {
        (reached + u64::from(ask <= limit_price), observed + 1)
    });
    if observed == 0 {
        return PASSIVE_PRICE_REACH_FLOOR;
    }

    (reached as f64 / observed as f64).max(PASSIVE_PRICE_REACH_FLOOR)
}

fn estimated_fill_days(
    quantity: f64,
    daily_volume: f64,
    price_reach_rate: f64,
    participation_rate: f64,
) -> f64 {
    (quantity / (daily_volume * price_reach_rate * participation_rate)).max(MIN_FILL_DAYS)
}

fn validate_config(config: OrderPolicyConfig) -> anyhow::Result<()> {
    if !config.tick_size.is_finite() || config.tick_size <= 0.0 {
        bail!("tick size must be finite and greater than 0");
    }
    if !(config.volume_participation_rate.is_finite()
        && 0.0 < config.volume_participation_rate
        && config.volume_participation_rate <= 1.0)
    {
        bail!("volume participation rate must be finite and in (0, 1]");
    }
    if !config.daily_discount_rate.is_finite()
        || !config.daily_capital_cost_rate.is_finite()
        || config.daily_discount_rate < 0.0
        || config.daily_capital_cost_rate < 0.0
    {
        bail!("daily discount and capital cost rates must be finite and nonnegative");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::rank::rank_money_actions;
    use crate::domain::MarketQuote;
    use crate::money_actions::best_money_actions;
    use crate::player::{
        ActionDetail, CharacterSkill, DerivedOpenOrder, DerivedPlayerState, FixedItem,
        LevelRequirement,
    };

    use super::*;

    #[test]
    fn recommends_complete_complementary_input_bundle() {
        let result = test_order_policy(&test_player(), OrderPolicyConfig::default());
        let recommendation = result.recommendation.expect("recommendation");

        assert_eq!(recommendation.action, "tea");
        assert_eq!(recommendation.orders.len(), 2);
        assert_eq!(recommendation.orders[0].quantity, 24);
        assert_eq!(recommendation.orders[0].item, "herb");
        assert_eq!(recommendation.orders[1].item, "water");
        assert!(recommendation.profit_uplift_over_baseline > 0.0);
    }

    #[test]
    fn does_not_recommend_an_incomplete_bundle_when_slots_are_full() {
        let mut player = test_player();
        player.derived.open_orders = vec![DerivedOpenOrder {
            side: OrderSide::Sell,
            item: "egg".into(),
            quantity: 1.0,
            limit_price: 10.0,
            locked_cash: 0.0,
        }];
        let result = test_order_policy(
            &player,
            OrderPolicyConfig {
                max_orders: 1,
                ..OrderPolicyConfig::default()
            },
        );

        assert!(result.recommendation.is_none());
    }

    fn test_order_policy(
        player: &PlayerExport,
        config: OrderPolicyConfig,
    ) -> OrderPolicyRecommendation {
        let market = test_market();
        let daily_volumes = test_daily_volumes();
        let actions = best_money_actions(player, &market, usize::MAX);
        let ranked = rank_money_actions(actions.clone(), &daily_volumes);
        build_order_policy(
            player,
            &market,
            &actions,
            &ranked,
            &daily_volumes,
            &HashMap::new(),
            config,
        )
        .unwrap()
    }

    fn test_player() -> PlayerExport {
        PlayerExport {
            derived: DerivedPlayerState {
                cash: 10_000.0,
                ..DerivedPlayerState::default()
            },
            character_item_map: HashMap::new(),
            character_skill_map: HashMap::from([
                (
                    "/skills/foraging".into(),
                    CharacterSkill {
                        skill_hrid: "/skills/foraging".into(),
                        level: 1,
                    },
                ),
                (
                    "/skills/brewing".into(),
                    CharacterSkill {
                        skill_hrid: "/skills/brewing".into(),
                        level: 1,
                    },
                ),
            ]),
            action_detail_maps: HashMap::from([
                (
                    "/action_types/foraging".into(),
                    HashMap::from([(
                        "fallback".into(),
                        action("fallback", "Fallback", "/skills/foraging", vec![], "egg"),
                    )]),
                ),
                (
                    "/action_types/brewing".into(),
                    HashMap::from([(
                        "tea".into(),
                        action(
                            "tea",
                            "Tea",
                            "/skills/brewing",
                            vec![("herb", 1.0), ("water", 1.0)],
                            "tea",
                        ),
                    )]),
                ),
            ]),
            skilling_action_type_buffs_dict: HashMap::new(),
            skilling_action_hrid_buffs_dict: HashMap::new(),
            action_type_drink_slots_dict: HashMap::new(),
            item_detail_dict: HashMap::new(),
        }
    }

    fn test_market() -> MarketSnapshot {
        MarketSnapshot {
            items: HashMap::from([
                ("egg".into(), quote(10.0, 11.0)),
                ("tea".into(), quote(1_000.0, 1_001.0)),
                ("herb".into(), quote(50.0, 100.0)),
                ("water".into(), quote(50.0, 100.0)),
            ]),
        }
    }

    fn test_daily_volumes() -> HashMap<String, f64> {
        HashMap::from([
            ("egg".into(), 10_000.0),
            ("tea".into(), 10_000.0),
            ("herb".into(), 10_000.0),
            ("water".into(), 10_000.0),
        ])
    }

    fn action(
        hrid: &str,
        name: &str,
        skill: &str,
        inputs: Vec<(&str, f64)>,
        output: &str,
    ) -> ActionDetail {
        ActionDetail {
            hrid: hrid.into(),
            name: name.into(),
            base_time_cost: 3_600_000_000_000.0,
            level_requirement: Some(LevelRequirement {
                skill_hrid: skill.into(),
                level: 1,
            }),
            input_items: (!inputs.is_empty()).then(|| {
                inputs
                    .into_iter()
                    .map(|(item, count)| FixedItem {
                        item_hrid: format!("/items/{item}"),
                        count,
                    })
                    .collect()
            }),
            output_items: Some(vec![FixedItem {
                item_hrid: format!("/items/{output}"),
                count: 1.0,
            }]),
            drop_table: None,
            essence_drop_table: None,
            rare_drop_table: None,
        }
    }

    fn quote(bid: f64, ask: f64) -> MarketQuote {
        MarketQuote {
            bid: Some(bid),
            ask: Some(ask),
            average: None,
            volume: None,
        }
    }
}
