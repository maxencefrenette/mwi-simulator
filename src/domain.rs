use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OrderSide {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct MarketSnapshot {
    pub items: HashMap<String, MarketQuote>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct MarketQuote {
    #[serde(alias = "a")]
    pub ask: Option<f64>,
    #[serde(alias = "b")]
    pub bid: Option<f64>,
    #[serde(alias = "p")]
    pub average: Option<f64>,
    #[serde(alias = "v")]
    pub volume: Option<f64>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct State {
    pub day: u32,
    pub cash: f64,
    pub inventory: HashMap<String, f64>,
    pub open_orders: Vec<OpenOrder>,
    pub fixed_wealth: f64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct OpenOrder {
    pub side: OrderSide,
    pub item: String,
    pub remaining_quantity: f64,
    pub limit_price: f64,
    pub locked_cash: f64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Observation {
    pub state: State,
    pub market: MarketSnapshot,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct Action {
    pub activity: Option<String>,
    pub market_actions: Vec<MarketAction>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MarketAction {
    PlaceOrder {
        side: OrderSide,
        item: String,
        quantity: f64,
        limit_price: f64,
    },
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    ActivityCompleted {
        action: String,
    },
    OrderPlaced {
        side: OrderSide,
        item: String,
        quantity: f64,
        limit_price: f64,
    },
    OrderFilled {
        side: OrderSide,
        item: String,
        quantity: f64,
        price: f64,
    },
    ActionRejected {
        reason: String,
    },
}

pub fn pessimistic_wealth(state: &State, market: &MarketSnapshot) -> f64 {
    let inventory_value = state
        .inventory
        .iter()
        .filter(|(item, _)| item.as_str() != "coin")
        .filter_map(|(item, quantity)| {
            market
                .items
                .get(item)
                .and_then(|quote| quote.bid)
                .map(|bid| bid * quantity)
        })
        .sum::<f64>();
    let order_value = state
        .open_orders
        .iter()
        .map(|order| match order.side {
            OrderSide::Buy => order.locked_cash,
            OrderSide::Sell => {
                market
                    .items
                    .get(&order.item)
                    .and_then(|quote| quote.bid)
                    .unwrap_or(0.0)
                    * order.remaining_quantity
            }
        })
        .sum::<f64>();

    state.cash + state.fixed_wealth + inventory_value + order_value
}
