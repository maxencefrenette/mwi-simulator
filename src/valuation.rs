use crate::model::{MarketQuote, MarketSnapshot, OpenOrder, OrderSide, PlayerState};
use serde::Serialize;

#[derive(Debug, Clone, Copy)]
pub struct ValuationConfig {
    pub liquidity_haircut: f64,
}

impl Default for ValuationConfig {
    fn default() -> Self {
        Self {
            liquidity_haircut: 0.15,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ConservativeValuation {
    pub cash: f64,
    pub inventory_value: f64,
    pub cancellable_order_value: f64,
    pub total: f64,
}

pub fn conservative_unit_value(quote: &MarketQuote, config: ValuationConfig) -> Option<f64> {
    let basis = quote.bid.or(quote.average).or(quote.ask)?;
    Some((basis * (1.0 - config.liquidity_haircut)).max(0.0))
}

pub fn conservative_terminal_wealth(
    state: &PlayerState,
    market: &MarketSnapshot,
    config: ValuationConfig,
) -> ConservativeValuation {
    let inventory_value = state
        .inventory
        .iter()
        .filter_map(|stack| {
            market
                .items
                .get(&stack.item)
                .and_then(|quote| conservative_unit_value(quote, config))
                .map(|unit| unit * stack.quantity as f64)
        })
        .sum();

    let cancellable_order_value = state
        .open_orders
        .iter()
        .map(|order| cancellable_order_value(order, market, config))
        .sum();

    ConservativeValuation {
        cash: state.cash,
        inventory_value,
        cancellable_order_value,
        total: state.cash + inventory_value + cancellable_order_value,
    }
}

fn cancellable_order_value(
    order: &OpenOrder,
    market: &MarketSnapshot,
    config: ValuationConfig,
) -> f64 {
    match order.side {
        OrderSide::Buy => order.quantity as f64 * order.limit_price,
        OrderSide::Sell => market
            .items
            .get(&order.item)
            .and_then(|quote| conservative_unit_value(quote, config))
            .map(|unit| unit * order.quantity as f64)
            .unwrap_or(0.0),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::model::{ItemStack, MarketQuote};

    use super::*;

    #[test]
    fn values_inventory_with_bid_before_average_or_ask() {
        let state = PlayerState {
            cash: 100.0,
            inventory: vec![ItemStack {
                item: "Egg".into(),
                quantity: 10,
            }],
            open_orders: vec![],
        };
        let market = MarketSnapshot {
            items: HashMap::from([(
                "Egg".into(),
                MarketQuote {
                    ask: Some(110.0),
                    bid: Some(90.0),
                    average: Some(100.0),
                    volume: Some(1000.0),
                },
            )]),
        };

        let value = conservative_terminal_wealth(
            &state,
            &market,
            ValuationConfig {
                liquidity_haircut: 0.10,
            },
        );

        assert_eq!(value.inventory_value, 810.0);
        assert_eq!(value.total, 910.0);
    }
}
