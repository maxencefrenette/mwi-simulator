use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Write};
use std::path::Path;
use std::thread::sleep;
use std::time::{Duration, SystemTime};

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};

use crate::domain::MarketSnapshot;

pub const MOOKET_HISTORY_URL: &str = "https://q7.nainai.eu.org/api/market/history";
const CACHE_TTL: Duration = Duration::from_secs(7 * 24 * 60 * 60);

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct MarketHistoryCache {
    pub fetched_at_unix: u64,
    pub source_url: String,
    pub item: String,
    pub item_hrid: String,
    pub level: u32,
    pub days: u32,
    pub points: Vec<MarketHistoryPoint>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct MarketHistoryPoint {
    pub time: u64,
    #[serde(alias = "a")]
    pub ask: Option<f64>,
    #[serde(alias = "b")]
    pub bid: Option<f64>,
    #[serde(alias = "p")]
    pub average: Option<f64>,
    #[serde(alias = "v")]
    pub volume: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FetchHistoryOutcome {
    Fetched,
    Cached,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct FetchAllHistoryReport {
    pub requested: usize,
    pub fetched: usize,
    pub cached: usize,
    pub failed: usize,
    pub failures: Vec<FetchHistoryFailure>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct FetchHistoryFailure {
    pub item: String,
    pub level: u32,
    pub error: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketHistoryTarget {
    pub item: String,
    pub level: u32,
}

fn fetch_market_history_to_path(
    item: &str,
    level: u32,
    days: u32,
    output: &Path,
    force: bool,
) -> anyhow::Result<FetchHistoryOutcome> {
    if !force && is_fresh_cache(output)? {
        return Ok(FetchHistoryOutcome::Cached);
    }

    let item_key = item_key_from_input(item);
    let item_hrid = item_hrid_from_key(&item_key);
    let points = fetch_market_history_points(&item_hrid, level, days)?;
    let cache = MarketHistoryCache {
        fetched_at_unix: unix_now()?,
        source_url: source_url(&item_hrid, level, days),
        item: item_key,
        item_hrid,
        level,
        days,
        points,
    };

    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let mut file =
        File::create(output).with_context(|| format!("failed to create {}", output.display()))?;
    serde_json::to_writer_pretty(&mut file, &cache)
        .with_context(|| format!("failed to serialize {}", output.display()))?;
    file.write_all(b"\n")
        .with_context(|| format!("failed to write {}", output.display()))?;

    Ok(FetchHistoryOutcome::Fetched)
}

pub fn fetch_all_market_history(
    snapshot: &MarketSnapshot,
    output_dir: &Path,
    days: u32,
    delay: Duration,
    force: bool,
) -> FetchAllHistoryReport {
    let targets = history_targets(snapshot);
    let mut report = FetchAllHistoryReport {
        requested: targets.len(),
        fetched: 0,
        cached: 0,
        failed: 0,
        failures: Vec::new(),
    };

    for target in targets {
        let output = history_cache_path(output_dir, &target.item, target.level);
        match fetch_market_history_to_path(&target.item, target.level, days, &output, force) {
            Ok(FetchHistoryOutcome::Fetched) => {
                report.fetched += 1;
                eprintln!(
                    "Fetched {}/{} {}:{}",
                    report.fetched + report.cached + report.failed,
                    report.requested,
                    target.item,
                    target.level
                );
                sleep(delay);
            }
            Ok(FetchHistoryOutcome::Cached) => {
                report.cached += 1;
                eprintln!(
                    "Cached {}/{} {}:{}",
                    report.fetched + report.cached + report.failed,
                    report.requested,
                    target.item,
                    target.level
                );
            }
            Err(error) => {
                report.failed += 1;
                report.failures.push(FetchHistoryFailure {
                    item: target.item.clone(),
                    level: target.level,
                    error: error.to_string(),
                });
                eprintln!(
                    "Failed {}/{} {}:{}: {error:#}",
                    report.fetched + report.cached + report.failed,
                    report.requested,
                    target.item,
                    target.level
                );
                sleep(delay);
            }
        }
    }

    report
}

fn fetch_market_history_points(
    item_hrid: &str,
    level: u32,
    days: u32,
) -> anyhow::Result<Vec<MarketHistoryPoint>> {
    let response = reqwest::blocking::Client::new()
        .get(MOOKET_HISTORY_URL)
        .query(&[
            ("item_id", item_hrid.to_string()),
            ("variant", level.to_string()),
            ("days", days.to_string()),
        ])
        .send()
        .with_context(|| format!("failed to request {}", source_url(item_hrid, level, days)))?
        .error_for_status()
        .with_context(|| format!("history request failed for {item_hrid}:{level}"))?;

    let raw_points: Vec<RawMarketHistoryPoint> = response
        .json()
        .with_context(|| format!("failed to parse history response for {item_hrid}:{level}"))?;

    Ok(raw_points
        .into_iter()
        .map(MarketHistoryPoint::from)
        .collect())
}

fn read_market_history_cache(path: &Path) -> anyhow::Result<MarketHistoryCache> {
    let file = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    serde_json::from_reader(BufReader::new(file))
        .with_context(|| format!("failed to parse {}", path.display()))
}

pub fn read_market_history_dir(
    history_dir: &Path,
) -> anyhow::Result<HashMap<String, MarketHistoryCache>> {
    let mut histories = HashMap::new();

    for entry in std::fs::read_dir(history_dir)
        .with_context(|| format!("failed to read {}", history_dir.display()))?
    {
        let entry =
            entry.with_context(|| format!("failed to read entry in {}", history_dir.display()))?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }

        let history = read_market_history_cache(&path)?;
        histories.insert(history.item.clone(), history);
    }

    Ok(histories)
}

pub(crate) fn daily_market_volumes(
    histories: &HashMap<String, MarketHistoryCache>,
) -> HashMap<String, f64> {
    histories
        .iter()
        .filter(|(_, history)| history.days > 0)
        .map(|(item, history)| {
            let total_volume = history
                .points
                .iter()
                .filter_map(|point| point.volume)
                .sum::<f64>();
            (item.clone(), total_volume / f64::from(history.days))
        })
        .collect()
}

#[derive(Debug, Clone, Deserialize)]
struct RawMarketHistoryPoint {
    time: u64,
    a: Option<f64>,
    b: Option<f64>,
    p: Option<f64>,
    v: Option<f64>,
}

impl From<RawMarketHistoryPoint> for MarketHistoryPoint {
    fn from(point: RawMarketHistoryPoint) -> Self {
        Self {
            time: point.time,
            ask: positive_price(point.a),
            bid: positive_price(point.b),
            average: positive_price(point.p),
            volume: positive_volume(point.v),
        }
    }
}

fn is_fresh_cache(path: &Path) -> anyhow::Result<bool> {
    let Ok(metadata) = std::fs::metadata(path) else {
        return Ok(false);
    };

    let modified = metadata
        .modified()
        .with_context(|| format!("failed to read modification time for {}", path.display()))?;
    let age = SystemTime::now()
        .duration_since(modified)
        .unwrap_or(Duration::ZERO);

    Ok(age < CACHE_TTL)
}

fn item_key_from_input(item: &str) -> String {
    item.trim()
        .strip_prefix("/items/")
        .unwrap_or(item.trim())
        .to_string()
}

fn item_hrid_from_key(item: &str) -> String {
    format!("/items/{item}")
}

fn source_url(item_hrid: &str, level: u32, days: u32) -> String {
    let encoded_item = item_hrid.replace('/', "%2F");
    format!("{MOOKET_HISTORY_URL}?item_id={encoded_item}&variant={level}&days={days}")
}

fn positive_price(value: Option<f64>) -> Option<f64> {
    value.filter(|price| price.is_finite() && *price >= 0.0)
}

fn positive_volume(value: Option<f64>) -> Option<f64> {
    value.filter(|volume| volume.is_finite() && *volume > 0.0)
}

fn unix_now() -> anyhow::Result<u64> {
    Ok(SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .context("system clock is before unix epoch")?
        .as_secs())
}

pub fn validate_history_request(days: u32) -> anyhow::Result<()> {
    if days == 0 {
        bail!("days must be greater than 0");
    }

    Ok(())
}

fn history_targets(snapshot: &MarketSnapshot) -> Vec<MarketHistoryTarget> {
    let mut targets: Vec<_> = snapshot
        .items
        .keys()
        .filter_map(|key| parse_market_key(key))
        .collect();

    targets.sort_by(|left, right| {
        left.item
            .cmp(&right.item)
            .then_with(|| left.level.cmp(&right.level))
    });
    targets.dedup_by(|left, right| left.item == right.item && left.level == right.level);
    targets
}

fn parse_market_key(key: &str) -> Option<MarketHistoryTarget> {
    let (item, level) = match key.rsplit_once(':') {
        Some((item, level)) => (item, level.parse().ok()?),
        None => (key, 0),
    };

    if level != 0 || has_plus_level(item) {
        return None;
    }

    Some(MarketHistoryTarget {
        item: item.to_string(),
        level,
    })
}

fn has_plus_level(item: &str) -> bool {
    item.as_bytes()
        .windows(2)
        .any(|chars| chars[0] == b'+' && chars[1].is_ascii_digit())
}

fn history_cache_path(output_dir: &Path, item: &str, level: u32) -> std::path::PathBuf {
    if level == 0 {
        output_dir.join(format!("{item}.json"))
    } else {
        output_dir.join(format!("{item}__plus{level}.json"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_history_points() {
        let raw = RawMarketHistoryPoint {
            time: 123,
            a: Some(-1.0),
            b: Some(84.0),
            p: Some(85.0),
            v: Some(127_477.0),
        };

        let point = MarketHistoryPoint::from(raw);

        assert_eq!(point.ask, None);
        assert_eq!(point.bid, Some(84.0));
        assert_eq!(point.average, Some(85.0));
        assert_eq!(point.volume, Some(127_477.0));
    }

    #[test]
    fn derives_base_history_targets_from_snapshot() {
        let snapshot = MarketSnapshot {
            items: [
                (
                    "egg".to_string(),
                    crate::domain::MarketQuote {
                        ask: Some(1.0),
                        bid: None,
                        average: None,
                        volume: None,
                    },
                ),
                (
                    "acrobatic_hood:10".to_string(),
                    crate::domain::MarketQuote {
                        ask: Some(1.0),
                        bid: None,
                        average: None,
                        volume: None,
                    },
                ),
                (
                    "azure_enhancer".to_string(),
                    crate::domain::MarketQuote {
                        ask: Some(1.0),
                        bid: None,
                        average: None,
                        volume: None,
                    },
                ),
                (
                    "advanced_enhancing_charm".to_string(),
                    crate::domain::MarketQuote {
                        ask: Some(1.0),
                        bid: None,
                        average: None,
                        volume: None,
                    },
                ),
                (
                    "enchanted_essence".to_string(),
                    crate::domain::MarketQuote {
                        ask: Some(1.0),
                        bid: None,
                        average: None,
                        volume: None,
                    },
                ),
                (
                    "test_item+1".to_string(),
                    crate::domain::MarketQuote {
                        ask: Some(1.0),
                        bid: None,
                        average: None,
                        volume: None,
                    },
                ),
                (
                    "acrobatic_hood".to_string(),
                    crate::domain::MarketQuote {
                        ask: Some(1.0),
                        bid: None,
                        average: None,
                        volume: None,
                    },
                ),
            ]
            .into_iter()
            .collect(),
        };

        let targets = history_targets(&snapshot);

        assert_eq!(
            targets,
            vec![
                MarketHistoryTarget {
                    item: "acrobatic_hood".to_string(),
                    level: 0,
                },
                MarketHistoryTarget {
                    item: "advanced_enhancing_charm".to_string(),
                    level: 0,
                },
                MarketHistoryTarget {
                    item: "azure_enhancer".to_string(),
                    level: 0,
                },
                MarketHistoryTarget {
                    item: "egg".to_string(),
                    level: 0,
                },
                MarketHistoryTarget {
                    item: "enchanted_essence".to_string(),
                    level: 0,
                },
            ]
        );
    }
}
