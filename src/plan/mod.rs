use std::path::Path;

use serde::Serialize;

use crate::history::{daily_market_volumes, read_market_history_dir};
use crate::model::MarketSnapshot;
use crate::money_actions::best_money_actions;
use crate::player::PlayerExport;

mod orders;
mod rank;

pub use orders::OrderPolicyRecommendation;
pub use rank::RankedAction;

use orders::{OrderPolicyConfig, build_order_policy};
use rank::rank_money_actions;

#[derive(Debug, Clone, Copy)]
pub struct PlanConfig {
    pub action_limit: usize,
    pub max_orders: usize,
    pub alternatives: usize,
    pub tick_size: f64,
    pub volume_participation_rate: f64,
    pub daily_discount_rate: f64,
    pub daily_capital_cost_rate: f64,
}

impl Default for PlanConfig {
    fn default() -> Self {
        let orders = OrderPolicyConfig::default();
        Self {
            action_limit: 25,
            max_orders: orders.max_orders,
            alternatives: orders.alternatives,
            tick_size: orders.tick_size,
            volume_participation_rate: orders.volume_participation_rate,
            daily_discount_rate: orders.daily_discount_rate,
            daily_capital_cost_rate: orders.daily_capital_cost_rate,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DailyPlan {
    pub ranked_actions: Vec<RankedAction>,
    pub order_policy: OrderPolicyRecommendation,
}

pub fn plan(
    player: &PlayerExport,
    market: &MarketSnapshot,
    history_dir: &Path,
    config: PlanConfig,
) -> anyhow::Result<DailyPlan> {
    let histories = read_market_history_dir(history_dir)?;
    let daily_volumes = daily_market_volumes(&histories);
    let actions = best_money_actions(player, market, usize::MAX);
    let ranked_actions = rank_money_actions(actions.clone(), &daily_volumes);
    let order_policy = build_order_policy(
        player,
        market,
        &actions,
        &ranked_actions,
        &daily_volumes,
        &histories,
        OrderPolicyConfig {
            max_orders: config.max_orders,
            alternatives: config.alternatives,
            tick_size: config.tick_size,
            volume_participation_rate: config.volume_participation_rate,
            daily_discount_rate: config.daily_discount_rate,
            daily_capital_cost_rate: config.daily_capital_cost_rate,
        },
    )?;

    Ok(DailyPlan {
        ranked_actions: ranked_actions
            .into_iter()
            .take(config.action_limit)
            .collect(),
        order_policy,
    })
}
