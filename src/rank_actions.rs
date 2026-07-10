use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use anyhow::Context;
use serde::Serialize;

use crate::history::{MarketHistoryCache, summarize_market_history};
use crate::model::MarketSnapshot;
use crate::money_actions::{ActionPlayerExport, MoneyAction, best_money_actions};

const HOURS_PER_DAY: f64 = 24.0;

#[derive(Debug, Clone, Copy)]
pub struct RankActionsConfig {
    pub limit: usize,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RankedAction {
    pub rank: usize,
    pub action: String,
    pub name: String,
    pub action_type: String,
    pub raw_profit_per_hour: f64,
    pub adjusted_profit_per_hour: f64,
    pub raw_revenue_per_hour: f64,
    pub adjusted_revenue_per_hour: f64,
    pub input_cost_per_hour: f64,
    pub drink_cost_per_hour: f64,
    pub actions_per_hour: f64,
    pub effective_actions_per_hour: f64,
    pub output_liquidity: Vec<OutputLiquidity>,
    pub missing_prices: Vec<crate::money_actions::MissingActionPrice>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct OutputLiquidity {
    pub item: String,
    pub quantity_per_hour: f64,
    pub expected_24h_quantity: f64,
    pub bid_value_per_hour: f64,
    pub adjusted_bid_value_per_hour: f64,
    pub historical_daily_volume: Option<f64>,
    pub sellable_fraction: f64,
}

pub fn rank_actions(
    player: &ActionPlayerExport,
    market: &MarketSnapshot,
    history_dir: &Path,
    config: RankActionsConfig,
) -> anyhow::Result<Vec<RankedAction>> {
    let histories = read_history_dir(history_dir)?;
    let raw_actions = best_money_actions(player, market, usize::MAX);
    let mut ranked = raw_actions
        .into_iter()
        .map(|action| rank_action(action, &histories))
        .collect::<Vec<_>>();

    ranked.sort_by(|left, right| {
        right
            .adjusted_profit_per_hour
            .total_cmp(&left.adjusted_profit_per_hour)
            .then_with(|| {
                right
                    .raw_profit_per_hour
                    .total_cmp(&left.raw_profit_per_hour)
            })
            .then_with(|| left.action.cmp(&right.action))
    });
    ranked.truncate(config.limit);

    for (index, action) in ranked.iter_mut().enumerate() {
        action.rank = index + 1;
    }

    Ok(ranked)
}

fn rank_action(
    action: MoneyAction,
    histories: &HashMap<String, MarketHistoryCache>,
) -> RankedAction {
    let output_liquidity = action
        .outputs_per_hour
        .iter()
        .map(|output| {
            let expected_24h_quantity = output.quantity_per_hour * HOURS_PER_DAY;
            let historical_daily_volume = histories
                .get(&output.item)
                .map(summarize_market_history)
                .and_then(|summary| {
                    (summary.days > 0).then_some(summary.total_volume / f64::from(summary.days))
                });
            let sellable_fraction =
                sellable_fraction(historical_daily_volume, expected_24h_quantity);
            OutputLiquidity {
                item: output.item.clone(),
                quantity_per_hour: output.quantity_per_hour,
                expected_24h_quantity,
                bid_value_per_hour: output.bid_value_per_hour,
                adjusted_bid_value_per_hour: output.bid_value_per_hour * sellable_fraction,
                historical_daily_volume,
                sellable_fraction,
            }
        })
        .collect::<Vec<_>>();

    let adjusted_revenue_per_hour = output_liquidity
        .iter()
        .map(|output| output.adjusted_bid_value_per_hour)
        .sum::<f64>();
    let adjusted_profit_per_hour =
        adjusted_revenue_per_hour - action.input_cost_per_hour - action.drink_cost_per_hour;

    RankedAction {
        rank: 0,
        action: action.action,
        name: action.name,
        action_type: action.action_type,
        raw_profit_per_hour: action.profit_per_hour,
        adjusted_profit_per_hour,
        raw_revenue_per_hour: action.revenue_per_hour,
        adjusted_revenue_per_hour,
        input_cost_per_hour: action.input_cost_per_hour,
        drink_cost_per_hour: action.drink_cost_per_hour,
        actions_per_hour: action.actions_per_hour,
        effective_actions_per_hour: action.effective_actions_per_hour,
        output_liquidity,
        missing_prices: action.missing_prices,
    }
}

fn sellable_fraction(historical_daily_volume: Option<f64>, expected_24h_quantity: f64) -> f64 {
    if expected_24h_quantity <= 0.0 {
        return 1.0;
    }

    historical_daily_volume
        .filter(|volume| volume.is_finite() && *volume > 0.0)
        .map(|volume| (volume / expected_24h_quantity).clamp(0.0, 1.0))
        .unwrap_or(0.0)
}

fn read_history_dir(history_dir: &Path) -> anyhow::Result<HashMap<String, MarketHistoryCache>> {
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

        let file =
            File::open(&path).with_context(|| format!("failed to open {}", path.display()))?;
        let history: MarketHistoryCache = serde_json::from_reader(BufReader::new(file))
            .with_context(|| format!("failed to parse {}", path.display()))?;
        histories.insert(history.item.clone(), history);
    }

    Ok(histories)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sellable_fraction_caps_at_historical_daily_volume() {
        assert_eq!(sellable_fraction(Some(50.0), 100.0), 0.5);
        assert_eq!(sellable_fraction(Some(500.0), 100.0), 1.0);
        assert_eq!(sellable_fraction(None, 100.0), 0.0);
    }
}
