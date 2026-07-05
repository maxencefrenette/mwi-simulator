use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::model::MarketSnapshot;

#[derive(Debug, Clone, Deserialize)]
pub struct PlayerExport {
    #[serde(default)]
    pub derived: DerivedExport,
    #[serde(default, rename = "characterItemMap")]
    pub character_item_map: HashMap<String, CharacterItem>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct DerivedExport {
    #[serde(default, rename = "openOrders")]
    pub open_orders: Vec<ExportedOpenOrder>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CharacterItem {
    #[serde(rename = "itemHrid")]
    pub item_hrid: String,
    #[serde(default, rename = "enhancementLevel")]
    pub enhancement_level: u32,
    #[serde(default)]
    pub count: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ExportedOpenOrder {
    pub side: String,
    pub item: String,
    #[serde(default)]
    pub quantity: f64,
    #[serde(default, rename = "lockedCash")]
    pub locked_cash: f64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct WealthSummary {
    pub total: f64,
    pub cash: f64,
    pub locked_buy_order_cash: f64,
    pub inventory_bid_value: f64,
    pub sell_order_bid_value: f64,
    pub missing_bid_items: Vec<MissingBidItem>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MissingBidItem {
    pub item: String,
    pub quantity: f64,
    pub source: WealthSource,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WealthSource {
    Inventory,
    SellOrder,
}

pub fn calculate_wealth(player: &PlayerExport, market: &MarketSnapshot) -> WealthSummary {
    let cash = player
        .character_item_map
        .values()
        .filter(|item| item.item_hrid == "/items/coin")
        .map(|item| item.count)
        .sum();

    let mut missing_bid_items = Vec::new();
    let inventory_bid_value = player
        .character_item_map
        .values()
        .filter(|item| item.item_hrid != "/items/coin" && item.count > 0.0)
        .map(|item| {
            bid_value(
                &item_key_from_hrid(&item.item_hrid, item.enhancement_level),
                item.count,
                market,
                WealthSource::Inventory,
                &mut missing_bid_items,
            )
        })
        .sum();

    let locked_buy_order_cash = player
        .derived
        .open_orders
        .iter()
        .filter(|order| order.side == "buy")
        .map(|order| order.locked_cash)
        .sum();

    let sell_order_bid_value = player
        .derived
        .open_orders
        .iter()
        .filter(|order| order.side == "sell" && order.quantity > 0.0)
        .map(|order| {
            bid_value(
                &order.item,
                order.quantity,
                market,
                WealthSource::SellOrder,
                &mut missing_bid_items,
            )
        })
        .sum();

    WealthSummary {
        total: cash + locked_buy_order_cash + inventory_bid_value + sell_order_bid_value,
        cash,
        locked_buy_order_cash,
        inventory_bid_value,
        sell_order_bid_value,
        missing_bid_items,
    }
}

fn bid_value(
    item: &str,
    quantity: f64,
    market: &MarketSnapshot,
    source: WealthSource,
    missing_bid_items: &mut Vec<MissingBidItem>,
) -> f64 {
    match market.items.get(item).and_then(|quote| quote.bid) {
        Some(bid) => bid * quantity,
        None => {
            missing_bid_items.push(MissingBidItem {
                item: item.to_string(),
                quantity,
                source,
            });
            0.0
        }
    }
}

fn item_key_from_hrid(item_hrid: &str, enhancement_level: u32) -> String {
    let base = item_hrid.strip_prefix("/items/").unwrap_or(item_hrid);
    if enhancement_level == 0 {
        base.to_string()
    } else {
        format!("{base}:{enhancement_level}")
    }
}

#[cfg(test)]
mod tests {
    use crate::model::MarketQuote;

    use super::*;

    #[test]
    fn calculates_pessimistic_wealth_from_bid_prices() {
        let player = PlayerExport {
            derived: DerivedExport {
                open_orders: vec![
                    ExportedOpenOrder {
                        side: "buy".into(),
                        item: "milk".into(),
                        quantity: 10.0,
                        locked_cash: 500.0,
                    },
                    ExportedOpenOrder {
                        side: "sell".into(),
                        item: "egg".into(),
                        quantity: 25.0,
                        locked_cash: 0.0,
                    },
                ],
            },
            character_item_map: HashMap::from([
                (
                    "coin".into(),
                    CharacterItem {
                        item_hrid: "/items/coin".into(),
                        enhancement_level: 0,
                        count: 1000.0,
                    },
                ),
                (
                    "egg".into(),
                    CharacterItem {
                        item_hrid: "/items/egg".into(),
                        enhancement_level: 0,
                        count: 50.0,
                    },
                ),
            ]),
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

        let wealth = calculate_wealth(&player, &market);

        assert_eq!(wealth.cash, 1000.0);
        assert_eq!(wealth.locked_buy_order_cash, 500.0);
        assert_eq!(wealth.inventory_bid_value, 450.0);
        assert_eq!(wealth.sell_order_bid_value, 225.0);
        assert_eq!(wealth.total, 2175.0);
    }
}
