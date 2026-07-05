use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Write};
use std::path::Path;

use anyhow::Context;
use serde::{Deserialize, Serialize};

use crate::model::{MarketQuote, MarketSnapshot};

pub const OFFICIAL_MARKETPLACE_URL: &str =
    "https://www.milkywayidle.com/game_data/marketplace.json";

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OfficialMarketplace {
    market_data: HashMap<String, HashMap<String, OfficialMarketQuote>>,
}

#[derive(Debug, Clone, Deserialize)]
struct OfficialMarketQuote {
    a: Option<f64>,
    b: Option<f64>,
    p: Option<f64>,
    v: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MarketSnapshotSummary {
    pub item_count: usize,
    pub quote_count: usize,
    pub quotes_with_average: usize,
    pub quotes_with_volume: usize,
}

pub fn fetch_official_marketplace() -> anyhow::Result<String> {
    let response = reqwest::blocking::get(OFFICIAL_MARKETPLACE_URL)
        .with_context(|| format!("failed to request {OFFICIAL_MARKETPLACE_URL}"))?
        .error_for_status()
        .with_context(|| format!("marketplace request failed for {OFFICIAL_MARKETPLACE_URL}"))?;

    response
        .text()
        .context("failed to read marketplace response body")
}

pub fn fetch_official_marketplace_to_path(path: &Path) -> anyhow::Result<()> {
    let body = fetch_official_marketplace()?;
    let mut file =
        File::create(path).with_context(|| format!("failed to create {}", path.display()))?;
    file.write_all(body.as_bytes())
        .with_context(|| format!("failed to write {}", path.display()))
}

pub fn read_market_snapshot(path: &Path) -> anyhow::Result<MarketSnapshot> {
    let file = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let value: serde_json::Value = serde_json::from_reader(BufReader::new(file))
        .with_context(|| format!("failed to parse {}", path.display()))?;

    parse_market_snapshot_value(value).with_context(|| format!("failed to load {}", path.display()))
}

pub fn parse_market_snapshot_str(raw: &str) -> anyhow::Result<MarketSnapshot> {
    let value = serde_json::from_str(raw).context("failed to parse market JSON")?;
    parse_market_snapshot_value(value)
}

pub fn parse_market_snapshot_value(value: serde_json::Value) -> anyhow::Result<MarketSnapshot> {
    if value.get("marketData").is_some() {
        let official: OfficialMarketplace = serde_json::from_value(value)
            .context("failed to parse official marketplace payload")?;
        Ok(official.into())
    } else {
        serde_json::from_value(value).context("failed to parse normalized market snapshot")
    }
}

pub fn summarize_market_snapshot(snapshot: &MarketSnapshot) -> MarketSnapshotSummary {
    MarketSnapshotSummary {
        item_count: snapshot
            .items
            .keys()
            .filter(|key| !key.contains(':'))
            .count(),
        quote_count: snapshot.items.len(),
        quotes_with_average: snapshot
            .items
            .values()
            .filter(|quote| quote.average.is_some())
            .count(),
        quotes_with_volume: snapshot
            .items
            .values()
            .filter(|quote| quote.volume.is_some())
            .count(),
    }
}

impl From<OfficialMarketplace> for MarketSnapshot {
    fn from(payload: OfficialMarketplace) -> Self {
        let mut items = HashMap::new();

        for (item_hrid, quotes_by_level) in payload.market_data {
            let item_key = item_key_from_hrid(&item_hrid);

            for (level, quote) in quotes_by_level {
                let key = if level == "0" {
                    item_key.clone()
                } else {
                    format!("{item_key}:{level}")
                };

                items.insert(
                    key,
                    MarketQuote {
                        ask: positive_price(quote.a),
                        bid: positive_price(quote.b),
                        average: positive_price(quote.p),
                        volume: positive_volume(quote.v),
                    },
                );
            }
        }

        Self { items }
    }
}

fn item_key_from_hrid(hrid: &str) -> String {
    hrid.strip_prefix("/items/").unwrap_or(hrid).to_string()
}

fn positive_price(value: Option<f64>) -> Option<f64> {
    value.filter(|price| *price >= 0.0)
}

fn positive_volume(value: Option<f64>) -> Option<f64> {
    value.filter(|volume| *volume > 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_official_marketplace_shape() {
        let raw = r#"
        {
          "marketData": {
            "/items/egg": {
              "0": { "a": 86, "b": 84, "p": 85, "v": 127477 }
            },
            "/items/acrobatic_hood": {
              "0": { "a": -1, "b": 62000000 },
              "10": { "a": 275000000, "b": 270000000 }
            }
          }
        }
        "#;

        let snapshot = parse_market_snapshot_str(raw).unwrap();

        assert_eq!(snapshot.items["egg"].ask, Some(86.0));
        assert_eq!(snapshot.items["egg"].volume, Some(127477.0));
        assert_eq!(snapshot.items["acrobatic_hood"].ask, None);
        assert_eq!(snapshot.items["acrobatic_hood"].bid, Some(62000000.0));
        assert_eq!(snapshot.items["acrobatic_hood:10"].ask, Some(275000000.0));
    }
}
