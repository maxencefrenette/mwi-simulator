use std::collections::HashMap;

use serde::Serialize;

use crate::market_price::{PriceBinDirection, bin_market_price};
use crate::model::{MarketSnapshot, OrderSide, PlayerState, ProductionPlan};
use crate::valuation::{ValuationConfig, conservative_unit_value};

#[derive(Debug, Clone, Copy)]
pub struct SellRecommendationConfig {
    pub max_orders: usize,
    pub tick_size: f64,
    pub valuation: ValuationConfig,
}

impl Default for SellRecommendationConfig {
    fn default() -> Self {
        Self {
            max_orders: 10,
            tick_size: 1.0,
            valuation: ValuationConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SellRecommendation {
    pub item: String,
    pub quantity: u64,
    pub suggested_limit_price: f64,
    pub conservative_unit_value: f64,
    pub conservative_total_value: f64,
    pub reason: String,
}

pub fn recommend_sells(
    state: &PlayerState,
    market: &MarketSnapshot,
    production: &ProductionPlan,
    config: SellRecommendationConfig,
) -> Vec<SellRecommendation> {
    let mut quantities = inventory_quantities(state);

    for stack in &production.items {
        *quantities.entry(stack.item.clone()).or_default() += stack.quantity;
    }

    for order in state
        .open_orders
        .iter()
        .filter(|order| order.side == OrderSide::Sell)
    {
        let remaining = quantities.entry(order.item.clone()).or_default();
        *remaining = remaining.saturating_sub(order.quantity);
    }

    let mut recommendations = quantities
        .into_iter()
        .filter(|(_, quantity)| *quantity > 0)
        .filter_map(|(item, quantity)| {
            let quote = market.items.get(&item)?;
            let conservative_unit = conservative_unit_value(quote, config.valuation)?;
            let ask = quote.ask.unwrap_or(conservative_unit);
            let desired_limit_price = (ask - config.tick_size).max(conservative_unit);
            let rounded_down = bin_market_price(desired_limit_price, PriceBinDirection::Down, 0.0);
            let suggested_limit_price = if rounded_down < conservative_unit {
                bin_market_price(conservative_unit, PriceBinDirection::Up, 0.0)
            } else {
                rounded_down
            };

            Some(SellRecommendation {
                item,
                quantity,
                suggested_limit_price,
                conservative_unit_value: conservative_unit,
                conservative_total_value: conservative_unit * quantity as f64,
                reason: "Current and expected inventory not already covered by open sell orders"
                    .into(),
            })
        })
        .collect::<Vec<_>>();

    recommendations.sort_by(|a, b| {
        b.conservative_total_value
            .total_cmp(&a.conservative_total_value)
            .then_with(|| a.item.cmp(&b.item))
    });
    recommendations.truncate(config.max_orders);
    recommendations
}

fn inventory_quantities(state: &PlayerState) -> HashMap<String, u64> {
    let mut quantities = HashMap::new();
    for stack in &state.inventory {
        *quantities.entry(stack.item.clone()).or_default() += stack.quantity;
    }
    quantities
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::model::{ItemStack, MarketQuote, OpenOrder};

    use super::*;

    #[test]
    fn subtracts_existing_sell_orders_and_keeps_top_values() {
        let state = PlayerState {
            cash: 0.0,
            inventory: vec![
                ItemStack {
                    item: "Egg".into(),
                    quantity: 100,
                },
                ItemStack {
                    item: "Milk".into(),
                    quantity: 5,
                },
            ],
            open_orders: vec![OpenOrder {
                item: "Egg".into(),
                side: OrderSide::Sell,
                quantity: 25,
                limit_price: 10.0,
            }],
        };
        let production = ProductionPlan {
            items: vec![ItemStack {
                item: "Egg".into(),
                quantity: 50,
            }],
        };
        let market = MarketSnapshot {
            items: HashMap::from([
                (
                    "Egg".into(),
                    MarketQuote {
                        ask: Some(11.0),
                        bid: Some(9.0),
                        average: None,
                        volume: Some(1000.0),
                    },
                ),
                (
                    "Milk".into(),
                    MarketQuote {
                        ask: Some(200.0),
                        bid: Some(180.0),
                        average: None,
                        volume: Some(50.0),
                    },
                ),
            ]),
        };

        let recs = recommend_sells(
            &state,
            &market,
            &production,
            SellRecommendationConfig {
                max_orders: 10,
                tick_size: 1.0,
                valuation: ValuationConfig {
                    liquidity_haircut: 0.0,
                },
            },
        );

        assert_eq!(recs.len(), 2);
        assert_eq!(recs[0].item, "Egg");
        assert_eq!(recs[0].quantity, 125);
        assert_eq!(recs[0].suggested_limit_price, 10.0);
    }
}
