use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::model::MarketSnapshot;

const NANOS_PER_SECOND: f64 = 1_000_000_000.0;
const SECONDS_PER_HOUR: f64 = 3600.0;
const MIN_ACTION_TIME_SECONDS: f64 = 3.0;
const DRINKS_PER_HOUR: f64 = 12.0;

const GATHERING_ACTION_TYPES: &[&str] = &[
    "/action_types/foraging",
    "/action_types/woodcutting",
    "/action_types/milking",
];
const PRODUCTION_ACTION_TYPES: &[&str] = &[
    "/action_types/brewing",
    "/action_types/cooking",
    "/action_types/cheesesmithing",
    "/action_types/crafting",
    "/action_types/tailoring",
];

#[derive(Debug, Clone, Deserialize)]
pub struct ActionPlayerExport {
    #[serde(default, rename = "characterSkillMap")]
    pub character_skill_map: HashMap<String, CharacterSkill>,
    #[serde(default, rename = "actionDetailMaps")]
    pub action_detail_maps: HashMap<String, HashMap<String, ActionDetail>>,
    #[serde(default, rename = "skillingActionTypeBuffsDict")]
    pub skilling_action_type_buffs_dict: HashMap<String, Option<Vec<Buff>>>,
    #[serde(default, rename = "skillingActionHridBuffsDict")]
    pub skilling_action_hrid_buffs_dict: HashMap<String, Option<Vec<Buff>>>,
    #[serde(default, rename = "actionTypeDrinkSlotsDict")]
    pub action_type_drink_slots_dict: HashMap<String, Vec<Option<DrinkSlot>>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CharacterSkill {
    #[serde(rename = "skillHrid")]
    pub skill_hrid: String,
    #[serde(default)]
    pub level: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ActionDetail {
    pub hrid: String,
    #[serde(default)]
    pub name: String,
    #[serde(default, rename = "baseTimeCost")]
    pub base_time_cost: f64,
    #[serde(default, rename = "levelRequirement")]
    pub level_requirement: Option<LevelRequirement>,
    #[serde(default, rename = "inputItems")]
    pub input_items: Option<Vec<FixedItem>>,
    #[serde(default, rename = "outputItems")]
    pub output_items: Option<Vec<FixedItem>>,
    #[serde(default, rename = "dropTable")]
    pub drop_table: Option<Vec<DropItem>>,
    #[serde(default, rename = "essenceDropTable")]
    pub essence_drop_table: Option<Vec<DropItem>>,
    #[serde(default, rename = "rareDropTable")]
    pub rare_drop_table: Option<Vec<DropItem>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LevelRequirement {
    #[serde(rename = "skillHrid")]
    pub skill_hrid: String,
    pub level: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FixedItem {
    #[serde(rename = "itemHrid")]
    pub item_hrid: String,
    pub count: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DropItem {
    #[serde(rename = "itemHrid")]
    pub item_hrid: String,
    #[serde(rename = "dropRate")]
    pub drop_rate: f64,
    #[serde(rename = "minCount")]
    pub min_count: f64,
    #[serde(rename = "maxCount")]
    pub max_count: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Buff {
    #[serde(rename = "typeHrid")]
    pub type_hrid: String,
    #[serde(default, rename = "flatBoost")]
    pub flat_boost: f64,
    #[serde(default, rename = "ratioBoost")]
    pub ratio_boost: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DrinkSlot {
    #[serde(rename = "itemHrid")]
    pub item_hrid: String,
    #[serde(default, rename = "isActive")]
    pub is_active: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MoneyAction {
    pub action: String,
    pub name: String,
    pub action_type: String,
    pub actions_per_hour: f64,
    pub effective_actions_per_hour: f64,
    pub action_time_seconds: f64,
    pub action_speed_bonus: f64,
    pub efficiency_bonus: f64,
    pub efficiency_multiplier: f64,
    pub artisan_bonus: f64,
    pub gourmet_bonus: f64,
    pub gathering_bonus: f64,
    pub rare_find_bonus: f64,
    pub revenue_per_hour: f64,
    pub input_cost_per_hour: f64,
    pub drink_cost_per_hour: f64,
    pub profit_per_hour: f64,
    pub profit_per_action: f64,
    pub outputs_per_hour: Vec<ActionItemRate>,
    pub missing_prices: Vec<MissingActionPrice>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ActionItemRate {
    pub item: String,
    pub quantity_per_hour: f64,
    pub bid_value_per_hour: f64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MissingActionPrice {
    pub item: String,
    pub side: PriceSide,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PriceSide {
    InputAsk,
    OutputBid,
    DrinkAsk,
}

pub fn best_money_actions(
    player: &ActionPlayerExport,
    market: &MarketSnapshot,
    max_results: usize,
) -> Vec<MoneyAction> {
    let levels = player
        .character_skill_map
        .values()
        .map(|skill| (skill.skill_hrid.as_str(), skill.level))
        .collect::<HashMap<_, _>>();

    let mut actions = player
        .action_detail_maps
        .iter()
        .filter(|(action_type, _)| is_supported_action_type(action_type))
        .flat_map(|(action_type, actions)| {
            actions
                .values()
                .filter(|action| is_unlocked(action, &levels))
                .filter(|action| has_economic_terms(action))
                .map(|action| evaluate_action(player, action_type, action, market, &levels))
        })
        .collect::<Vec<_>>();

    actions.sort_by(|a, b| {
        b.profit_per_hour
            .total_cmp(&a.profit_per_hour)
            .then_with(|| a.action.cmp(&b.action))
    });
    actions.truncate(max_results);
    actions
}

fn is_supported_action_type(action_type: &str) -> bool {
    GATHERING_ACTION_TYPES.contains(&action_type) || PRODUCTION_ACTION_TYPES.contains(&action_type)
}

fn is_unlocked(action: &ActionDetail, levels: &HashMap<&str, u32>) -> bool {
    match &action.level_requirement {
        Some(requirement) => levels
            .get(requirement.skill_hrid.as_str())
            .is_some_and(|level| *level >= requirement.level),
        None => true,
    }
}

fn has_economic_terms(action: &ActionDetail) -> bool {
    action.base_time_cost > 0.0
        && (action
            .input_items
            .as_ref()
            .is_some_and(|items| !items.is_empty())
            || action
                .output_items
                .as_ref()
                .is_some_and(|items| !items.is_empty())
            || action
                .drop_table
                .as_ref()
                .is_some_and(|items| !items.is_empty())
            || action
                .essence_drop_table
                .as_ref()
                .is_some_and(|items| !items.is_empty())
            || action
                .rare_drop_table
                .as_ref()
                .is_some_and(|items| !items.is_empty()))
}

fn evaluate_action(
    player: &ActionPlayerExport,
    action_type: &str,
    action: &ActionDetail,
    market: &MarketSnapshot,
    levels: &HashMap<&str, u32>,
) -> MoneyAction {
    let mut missing_prices = Vec::new();
    let buffs = action_buffs(player, action_type, &action.hrid);
    let modifiers = action_modifiers(action, action_type, &buffs, levels);
    let action_time_seconds =
        (action.base_time_cost / NANOS_PER_SECOND / (1.0 + modifiers.action_speed_bonus))
            .max(MIN_ACTION_TIME_SECONDS);
    let actions_per_hour = SECONDS_PER_HOUR / action_time_seconds;
    let effective_actions_per_hour = actions_per_hour * modifiers.efficiency_multiplier;
    let outputs_per_hour = output_rates(action, market, &modifiers, effective_actions_per_hour);
    let revenue_per_effective_action =
        fixed_output_value(action, market, modifiers.gourmet_bonus, &mut missing_prices)
            + drop_table_value(
                action.drop_table.as_deref(),
                market,
                modifiers.gathering_bonus,
                0.0,
                &mut missing_prices,
            )
            + drop_table_value(
                action.essence_drop_table.as_deref(),
                market,
                0.0,
                modifiers.rare_find_bonus,
                &mut missing_prices,
            )
            + drop_table_value(
                action.rare_drop_table.as_deref(),
                market,
                0.0,
                modifiers.rare_find_bonus,
                &mut missing_prices,
            );
    let input_cost_per_effective_action =
        input_cost(action, market, modifiers.artisan_bonus, &mut missing_prices);
    let drink_cost_per_hour = drink_cost(player, action_type, market, &mut missing_prices);
    let revenue_per_hour = revenue_per_effective_action * effective_actions_per_hour;
    let input_cost_per_hour = input_cost_per_effective_action * effective_actions_per_hour;
    let profit_per_hour = revenue_per_hour - input_cost_per_hour - drink_cost_per_hour;
    let profit_per_action = profit_per_hour / actions_per_hour;

    MoneyAction {
        action: action.hrid.clone(),
        name: action.name.clone(),
        action_type: action_type.to_string(),
        actions_per_hour,
        effective_actions_per_hour,
        action_time_seconds,
        action_speed_bonus: modifiers.action_speed_bonus,
        efficiency_bonus: modifiers.efficiency_bonus,
        efficiency_multiplier: modifiers.efficiency_multiplier,
        artisan_bonus: modifiers.artisan_bonus,
        gourmet_bonus: modifiers.gourmet_bonus,
        gathering_bonus: modifiers.gathering_bonus,
        rare_find_bonus: modifiers.rare_find_bonus,
        revenue_per_hour,
        input_cost_per_hour,
        drink_cost_per_hour,
        profit_per_hour,
        profit_per_action,
        outputs_per_hour,
        missing_prices,
    }
}

#[derive(Debug, Clone, Copy)]
struct ActionModifiers {
    action_speed_bonus: f64,
    efficiency_bonus: f64,
    efficiency_multiplier: f64,
    artisan_bonus: f64,
    gourmet_bonus: f64,
    gathering_bonus: f64,
    rare_find_bonus: f64,
}

fn action_buffs<'a>(
    player: &'a ActionPlayerExport,
    action_type: &str,
    action_hrid: &str,
) -> Vec<&'a Buff> {
    let mut buffs = Vec::new();

    if let Some(Some(action_type_buffs)) = player.skilling_action_type_buffs_dict.get(action_type) {
        buffs.extend(action_type_buffs);
    }
    if let Some(Some(action_buffs)) = player.skilling_action_hrid_buffs_dict.get(action_hrid) {
        buffs.extend(action_buffs);
    }

    buffs
}

fn action_modifiers(
    action: &ActionDetail,
    action_type: &str,
    buffs: &[&Buff],
    levels: &HashMap<&str, u32>,
) -> ActionModifiers {
    let action_speed_bonus = buff_sum(buffs, "/buff_types/action_speed");
    let action_level_bonus = buff_sum(buffs, "/buff_types/action_level");
    let skill_level_bonus = buff_sum_by_suffix(buffs, "_level") - action_level_bonus;
    let base_requirement = action
        .level_requirement
        .as_ref()
        .map(|requirement| requirement.level)
        .unwrap_or(1) as f64;
    let skill_level = action
        .level_requirement
        .as_ref()
        .and_then(|requirement| levels.get(requirement.skill_hrid.as_str()))
        .copied()
        .unwrap_or(base_requirement as u32) as f64;
    let effective_level = skill_level.max(base_requirement) + skill_level_bonus;
    let effective_requirement = base_requirement + action_level_bonus;
    let level_efficiency = ((effective_level - effective_requirement).max(0.0)) / 100.0;
    let efficiency_bonus = level_efficiency + buff_sum(buffs, "/buff_types/efficiency");
    let artisan_bonus = if PRODUCTION_ACTION_TYPES.contains(&action_type) {
        buff_sum(buffs, "/buff_types/artisan").clamp(0.0, 1.0)
    } else {
        0.0
    };
    let gourmet_bonus = if PRODUCTION_ACTION_TYPES.contains(&action_type) {
        buff_sum(buffs, "/buff_types/gourmet")
    } else {
        0.0
    };
    let gathering_bonus = if GATHERING_ACTION_TYPES.contains(&action_type) {
        buff_sum(buffs, "/buff_types/gathering")
    } else {
        0.0
    };
    let rare_find_bonus = buff_sum(buffs, "/buff_types/rare_find");

    ActionModifiers {
        action_speed_bonus,
        efficiency_bonus,
        efficiency_multiplier: 1.0 + efficiency_bonus,
        artisan_bonus,
        gourmet_bonus,
        gathering_bonus,
        rare_find_bonus,
    }
}

fn buff_sum(buffs: &[&Buff], type_hrid: &str) -> f64 {
    buffs
        .iter()
        .filter(|buff| buff.type_hrid == type_hrid)
        .map(|buff| buff.flat_boost + buff.ratio_boost)
        .sum()
}

fn buff_sum_by_suffix(buffs: &[&Buff], suffix: &str) -> f64 {
    buffs
        .iter()
        .filter(|buff| buff.type_hrid.ends_with(suffix))
        .map(|buff| buff.flat_boost + buff.ratio_boost)
        .sum()
}

fn drink_cost(
    player: &ActionPlayerExport,
    action_type: &str,
    market: &MarketSnapshot,
    missing_prices: &mut Vec<MissingActionPrice>,
) -> f64 {
    let Some(drinks) = player.action_type_drink_slots_dict.get(action_type) else {
        return 0.0;
    };

    drinks
        .iter()
        .flatten()
        .filter(|drink| drink.is_active)
        .map(|drink| {
            let key = item_key_from_hrid(&drink.item_hrid);
            match coin_price(&key).or_else(|| market.items.get(&key).and_then(|quote| quote.ask)) {
                Some(ask) => ask * DRINKS_PER_HOUR,
                None => {
                    missing_prices.push(MissingActionPrice {
                        item: key,
                        side: PriceSide::DrinkAsk,
                    });
                    0.0
                }
            }
        })
        .sum()
}

fn fixed_output_value(
    action: &ActionDetail,
    market: &MarketSnapshot,
    gourmet_bonus: f64,
    missing_prices: &mut Vec<MissingActionPrice>,
) -> f64 {
    action
        .output_items
        .as_deref()
        .unwrap_or_default()
        .iter()
        .map(|item| {
            output_bid_value(
                &item_key_from_hrid(&item.item_hrid),
                item.count * (1.0 + gourmet_bonus),
                market,
                missing_prices,
            )
        })
        .sum()
}

fn drop_table_value(
    drops: Option<&[DropItem]>,
    market: &MarketSnapshot,
    quantity_bonus: f64,
    drop_rate_bonus: f64,
    missing_prices: &mut Vec<MissingActionPrice>,
) -> f64 {
    drops
        .unwrap_or_default()
        .iter()
        .map(|drop| {
            let expected_count = drop.drop_rate
                * (1.0 + drop_rate_bonus)
                * ((drop.min_count + drop.max_count) / 2.0)
                * (1.0 + quantity_bonus);
            output_bid_value(
                &item_key_from_hrid(&drop.item_hrid),
                expected_count,
                market,
                missing_prices,
            )
        })
        .sum()
}

fn input_cost(
    action: &ActionDetail,
    market: &MarketSnapshot,
    artisan_bonus: f64,
    missing_prices: &mut Vec<MissingActionPrice>,
) -> f64 {
    action
        .input_items
        .as_deref()
        .unwrap_or_default()
        .iter()
        .map(|item| {
            let key = item_key_from_hrid(&item.item_hrid);
            match coin_price(&key).or_else(|| market.items.get(&key).and_then(|quote| quote.ask)) {
                Some(ask) => ask * item.count * (1.0 - artisan_bonus),
                None => {
                    missing_prices.push(MissingActionPrice {
                        item: key,
                        side: PriceSide::InputAsk,
                    });
                    0.0
                }
            }
        })
        .sum()
}

fn output_bid_value(
    item: &str,
    quantity: f64,
    market: &MarketSnapshot,
    missing_prices: &mut Vec<MissingActionPrice>,
) -> f64 {
    match coin_price(item).or_else(|| market.items.get(item).and_then(|quote| quote.bid)) {
        Some(bid) => bid * quantity,
        None => {
            missing_prices.push(MissingActionPrice {
                item: item.to_string(),
                side: PriceSide::OutputBid,
            });
            0.0
        }
    }
}

fn output_rates(
    action: &ActionDetail,
    market: &MarketSnapshot,
    modifiers: &ActionModifiers,
    effective_actions_per_hour: f64,
) -> Vec<ActionItemRate> {
    let mut rates = Vec::new();

    for item in action.output_items.as_deref().unwrap_or_default() {
        push_output_rate(
            &mut rates,
            market,
            &item_key_from_hrid(&item.item_hrid),
            item.count * (1.0 + modifiers.gourmet_bonus) * effective_actions_per_hour,
        );
    }

    push_drop_rates(
        &mut rates,
        action.drop_table.as_deref(),
        market,
        modifiers.gathering_bonus,
        0.0,
        effective_actions_per_hour,
    );
    push_drop_rates(
        &mut rates,
        action.essence_drop_table.as_deref(),
        market,
        0.0,
        modifiers.rare_find_bonus,
        effective_actions_per_hour,
    );
    push_drop_rates(
        &mut rates,
        action.rare_drop_table.as_deref(),
        market,
        0.0,
        modifiers.rare_find_bonus,
        effective_actions_per_hour,
    );

    rates.sort_by(|left, right| left.item.cmp(&right.item));
    rates
}

fn push_drop_rates(
    rates: &mut Vec<ActionItemRate>,
    drops: Option<&[DropItem]>,
    market: &MarketSnapshot,
    quantity_bonus: f64,
    drop_rate_bonus: f64,
    effective_actions_per_hour: f64,
) {
    for drop in drops.unwrap_or_default() {
        let expected_count = drop.drop_rate
            * (1.0 + drop_rate_bonus)
            * ((drop.min_count + drop.max_count) / 2.0)
            * (1.0 + quantity_bonus);
        push_output_rate(
            rates,
            market,
            &item_key_from_hrid(&drop.item_hrid),
            expected_count * effective_actions_per_hour,
        );
    }
}

fn push_output_rate(
    rates: &mut Vec<ActionItemRate>,
    market: &MarketSnapshot,
    item: &str,
    quantity_per_hour: f64,
) {
    let bid_value_per_hour = coin_price(item)
        .or_else(|| market.items.get(item).and_then(|quote| quote.bid))
        .unwrap_or(0.0)
        * quantity_per_hour;

    if let Some(existing) = rates.iter_mut().find(|rate| rate.item == item) {
        existing.quantity_per_hour += quantity_per_hour;
        existing.bid_value_per_hour += bid_value_per_hour;
    } else {
        rates.push(ActionItemRate {
            item: item.to_string(),
            quantity_per_hour,
            bid_value_per_hour,
        });
    }
}

fn coin_price(item: &str) -> Option<f64> {
    (item == "coin").then_some(1.0)
}

fn item_key_from_hrid(item_hrid: &str) -> String {
    item_hrid
        .strip_prefix("/items/")
        .unwrap_or(item_hrid)
        .to_string()
}

#[cfg(test)]
mod tests {
    use crate::model::MarketQuote;

    use super::*;

    #[test]
    fn ranks_unlocked_actions_by_bid_minus_ask_profit_per_hour() {
        let player = ActionPlayerExport {
            character_skill_map: HashMap::from([(
                "/skills/foraging".into(),
                CharacterSkill {
                    skill_hrid: "/skills/foraging".into(),
                    level: 10,
                },
            )]),
            action_detail_maps: HashMap::from([(
                "/action_types/foraging".into(),
                HashMap::from([
                    (
                        "/actions/foraging/egg".into(),
                        ActionDetail {
                            hrid: "/actions/foraging/egg".into(),
                            name: "Egg".into(),
                            base_time_cost: 6_000_000_000.0,
                            level_requirement: Some(LevelRequirement {
                                skill_hrid: "/skills/foraging".into(),
                                level: 1,
                            }),
                            input_items: None,
                            output_items: None,
                            drop_table: Some(vec![DropItem {
                                item_hrid: "/items/egg".into(),
                                drop_rate: 1.0,
                                min_count: 1.0,
                                max_count: 3.0,
                            }]),
                            essence_drop_table: None,
                            rare_drop_table: None,
                        },
                    ),
                    (
                        "/actions/foraging/locked".into(),
                        ActionDetail {
                            hrid: "/actions/foraging/locked".into(),
                            name: "Locked".into(),
                            base_time_cost: 6_000_000_000.0,
                            level_requirement: Some(LevelRequirement {
                                skill_hrid: "/skills/foraging".into(),
                                level: 99,
                            }),
                            input_items: None,
                            output_items: Some(vec![FixedItem {
                                item_hrid: "/items/diamond".into(),
                                count: 1.0,
                            }]),
                            drop_table: None,
                            essence_drop_table: None,
                            rare_drop_table: None,
                        },
                    ),
                ]),
            )]),
            skilling_action_type_buffs_dict: HashMap::new(),
            skilling_action_hrid_buffs_dict: HashMap::new(),
            action_type_drink_slots_dict: HashMap::new(),
        };
        let market = MarketSnapshot {
            items: HashMap::from([(
                "egg".into(),
                MarketQuote {
                    ask: Some(11.0),
                    bid: Some(9.0),
                    average: Some(10.0),
                    volume: Some(100.0),
                },
            )]),
        };

        let actions = best_money_actions(&player, &market, 10);

        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].action, "/actions/foraging/egg");
        assert_eq!(actions[0].efficiency_bonus, 0.09);
        assert_eq!(actions[0].profit_per_action, 19.62);
        assert_eq!(actions[0].profit_per_hour, 11_772.0);
        assert_eq!(actions[0].outputs_per_hour.len(), 1);
        assert_eq!(actions[0].outputs_per_hour[0].item, "egg");
        assert_eq!(actions[0].outputs_per_hour[0].quantity_per_hour, 1_308.0);
        assert_eq!(
            actions[0].outputs_per_hour[0].bid_value_per_hour,
            actions[0].revenue_per_hour
        );
    }

    #[test]
    fn applies_action_buffs_and_drink_costs() {
        let player = ActionPlayerExport {
            character_skill_map: HashMap::from([(
                "/skills/cooking".into(),
                CharacterSkill {
                    skill_hrid: "/skills/cooking".into(),
                    level: 1,
                },
            )]),
            action_detail_maps: HashMap::from([(
                "/action_types/cooking".into(),
                HashMap::from([(
                    "/actions/cooking/cupcake".into(),
                    ActionDetail {
                        hrid: "/actions/cooking/cupcake".into(),
                        name: "Cupcake".into(),
                        base_time_cost: 6_000_000_000.0,
                        level_requirement: Some(LevelRequirement {
                            skill_hrid: "/skills/cooking".into(),
                            level: 1,
                        }),
                        input_items: Some(vec![FixedItem {
                            item_hrid: "/items/egg".into(),
                            count: 10.0,
                        }]),
                        output_items: Some(vec![FixedItem {
                            item_hrid: "/items/cupcake".into(),
                            count: 1.0,
                        }]),
                        drop_table: None,
                        essence_drop_table: None,
                        rare_drop_table: None,
                    },
                )]),
            )]),
            skilling_action_type_buffs_dict: HashMap::from([(
                "/action_types/cooking".into(),
                Some(vec![
                    Buff {
                        type_hrid: "/buff_types/action_speed".into(),
                        flat_boost: 0.5,
                        ratio_boost: 0.0,
                    },
                    Buff {
                        type_hrid: "/buff_types/efficiency".into(),
                        flat_boost: 0.25,
                        ratio_boost: 0.0,
                    },
                    Buff {
                        type_hrid: "/buff_types/artisan".into(),
                        flat_boost: 0.1,
                        ratio_boost: 0.0,
                    },
                    Buff {
                        type_hrid: "/buff_types/gourmet".into(),
                        flat_boost: 0.2,
                        ratio_boost: 0.0,
                    },
                ]),
            )]),
            skilling_action_hrid_buffs_dict: HashMap::new(),
            action_type_drink_slots_dict: HashMap::from([(
                "/action_types/cooking".into(),
                vec![Some(DrinkSlot {
                    item_hrid: "/items/efficiency_tea".into(),
                    is_active: true,
                })],
            )]),
        };
        let market = MarketSnapshot {
            items: HashMap::from([
                (
                    "egg".into(),
                    MarketQuote {
                        ask: Some(2.0),
                        bid: Some(1.0),
                        average: None,
                        volume: None,
                    },
                ),
                (
                    "cupcake".into(),
                    MarketQuote {
                        ask: Some(100.0),
                        bid: Some(100.0),
                        average: None,
                        volume: None,
                    },
                ),
                (
                    "efficiency_tea".into(),
                    MarketQuote {
                        ask: Some(5.0),
                        bid: Some(4.0),
                        average: None,
                        volume: None,
                    },
                ),
            ]),
        };

        let actions = best_money_actions(&player, &market, 1);
        let action = &actions[0];

        assert_eq!(action.action_time_seconds, 4.0);
        assert_eq!(action.actions_per_hour, 900.0);
        assert_eq!(action.efficiency_multiplier, 1.25);
        assert_eq!(action.revenue_per_hour, 135_000.0);
        assert_eq!(action.input_cost_per_hour, 20_250.0);
        assert_eq!(action.drink_cost_per_hour, 60.0);
        assert_eq!(action.profit_per_hour, 114_690.0);
    }
}
