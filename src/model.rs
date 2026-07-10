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
