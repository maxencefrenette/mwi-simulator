use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct ItemStack {
    pub item: String,
    pub quantity: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct PlayerState {
    pub cash: f64,
    #[serde(default)]
    pub inventory: Vec<ItemStack>,
    #[serde(default)]
    pub open_orders: Vec<OpenOrder>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct ProductionPlan {
    #[serde(default)]
    pub items: Vec<ItemStack>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OrderSide {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct OpenOrder {
    pub item: String,
    pub side: OrderSide,
    pub quantity: u64,
    pub limit_price: f64,
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

impl ProductionPlan {
    pub fn empty() -> Self {
        Self { items: Vec::new() }
    }
}
