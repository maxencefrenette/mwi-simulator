use std::collections::HashMap;
use std::path::Path;

use anyhow::Context;
use serde::Serialize;

use crate::history::{MarketHistoryCache, read_market_history_cache};
use crate::model::MarketSnapshot;
use crate::money_actions::{
    ActionPlayerExport, MissingActionPrice, MoneyAction, best_money_actions,
};

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
    pub missing_prices: Vec<MissingActionPrice>,
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
    let daily_volumes = read_daily_volumes(history_dir)?;
    let raw_actions = best_money_actions(player, market, usize::MAX);
    let mut ranked = raw_actions
        .into_iter()
        .map(|action| rank_action(action, &daily_volumes))
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

fn rank_action(action: MoneyAction, daily_volumes: &HashMap<String, f64>) -> RankedAction {
    let output_liquidity = action
        .outputs_per_hour
        .iter()
        .map(|output| {
            let expected_24h_quantity = output.quantity_per_hour * HOURS_PER_DAY;
            let historical_daily_volume = daily_volumes.get(&output.item).copied();
            let sellable_fraction =
                sellable_fraction(&output.item, historical_daily_volume, expected_24h_quantity);
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

fn historical_daily_volume(history: &MarketHistoryCache) -> Option<f64> {
    (history.days > 0).then(|| {
        history
            .points
            .iter()
            .filter_map(|point| point.volume)
            .sum::<f64>()
            / f64::from(history.days)
    })
}

fn sellable_fraction(
    item: &str,
    historical_daily_volume: Option<f64>,
    expected_24h_quantity: f64,
) -> f64 {
    if item == "coin" {
        return 1.0;
    }

    if expected_24h_quantity <= 0.0 {
        return 1.0;
    }

    historical_daily_volume
        .filter(|volume| volume.is_finite() && *volume > 0.0)
        .map(|volume| (volume / expected_24h_quantity).clamp(0.0, 1.0))
        .unwrap_or(0.0)
}

fn read_daily_volumes(history_dir: &Path) -> anyhow::Result<HashMap<String, f64>> {
    let mut daily_volumes = HashMap::new();

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
        if let Some(volume) = historical_daily_volume(&history) {
            daily_volumes.insert(history.item, volume);
        }
    }

    Ok(daily_volumes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sellable_fraction_caps_at_historical_daily_volume() {
        assert_eq!(sellable_fraction("egg", Some(50.0), 100.0), 0.5);
        assert_eq!(sellable_fraction("egg", Some(500.0), 100.0), 1.0);
        assert_eq!(sellable_fraction("egg", None, 100.0), 0.0);
    }

    #[test]
    fn coins_are_fully_liquid_without_market_history() {
        assert_eq!(sellable_fraction("coin", None, 100.0), 1.0);
    }
}
