use std::path::Path;

use serde::Serialize;

use crate::domain::{Action, MarketAction, MarketSnapshot, Observation, OrderSide};
use crate::history::{MarketHistoryCache, daily_market_volumes, read_market_history_dir};
use crate::money_actions::best_money_actions;
use crate::player::{PlayerExport, export_for_observation};

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
pub struct PolicyPlan {
    pub action: Action,
    pub ranked_actions: Vec<RankedAction>,
    pub order_policy: OrderPolicyRecommendation,
}

pub trait Policy {
    fn name(&self) -> &'static str;
    fn act(&self, observation: &Observation) -> anyhow::Result<Action>;
}

pub struct IdlePolicy;

impl Policy for IdlePolicy {
    fn name(&self) -> &'static str {
        "idle"
    }
    fn act(&self, _observation: &Observation) -> anyhow::Result<Action> {
        Ok(Action::default())
    }
}

pub struct HeuristicPolicy<'a> {
    player: &'a PlayerExport,
    histories: &'a std::collections::HashMap<String, MarketHistoryCache>,
    config: PlanConfig,
    include_orders: bool,
}

impl<'a> HeuristicPolicy<'a> {
    pub fn new(
        player: &'a PlayerExport,
        histories: &'a std::collections::HashMap<String, MarketHistoryCache>,
        config: PlanConfig,
        include_orders: bool,
    ) -> Self {
        Self {
            player,
            histories,
            config,
            include_orders,
        }
    }

    pub fn plan(&self, observation: &Observation) -> anyhow::Result<PolicyPlan> {
        plan_with_histories(
            self.player,
            observation,
            self.histories,
            self.config,
            self.include_orders,
        )
    }
}

impl Policy for HeuristicPolicy<'_> {
    fn name(&self) -> &'static str {
        if self.include_orders {
            "heuristic"
        } else {
            "action_only"
        }
    }
    fn act(&self, observation: &Observation) -> anyhow::Result<Action> {
        Ok(self.plan(observation)?.action)
    }
}

pub fn plan(
    player: &PlayerExport,
    market: &MarketSnapshot,
    history_dir: &Path,
    config: PlanConfig,
) -> anyhow::Result<PolicyPlan> {
    let histories = read_market_history_dir(history_dir)?;
    let observation = Observation {
        state: crate::player::state_from_export(player, market),
        market: market.clone(),
    };
    plan_with_histories(player, &observation, &histories, config, true)
}

fn plan_with_histories(
    player: &PlayerExport,
    observation: &Observation,
    histories: &std::collections::HashMap<String, MarketHistoryCache>,
    config: PlanConfig,
    include_orders: bool,
) -> anyhow::Result<PolicyPlan> {
    let player = export_for_observation(player, observation);
    let daily_volumes = daily_market_volumes(histories);
    let actions = best_money_actions(&player, &observation.market, usize::MAX);
    let ranked_actions = rank_money_actions(actions.clone(), &daily_volumes);
    let order_policy = build_order_policy(
        &player,
        &observation.market,
        &actions,
        &ranked_actions,
        &daily_volumes,
        histories,
        OrderPolicyConfig {
            max_orders: config.max_orders,
            alternatives: config.alternatives,
            tick_size: config.tick_size,
            volume_participation_rate: config.volume_participation_rate,
            daily_discount_rate: config.daily_discount_rate,
            daily_capital_cost_rate: config.daily_capital_cost_rate,
        },
    )?;

    let activity = order_policy
        .baseline_action
        .as_ref()
        .map(|candidate| candidate.action.clone());
    let market_actions = if include_orders {
        order_policy
            .recommendation
            .as_ref()
            .into_iter()
            .flat_map(|package| package.orders.iter())
            .map(|order| MarketAction::PlaceOrder {
                side: OrderSide::Buy,
                item: order.item.clone(),
                quantity: order.quantity as f64,
                limit_price: order.suggested_limit_price,
            })
            .collect()
    } else {
        Vec::new()
    };
    Ok(PolicyPlan {
        action: Action {
            activity,
            market_actions,
        },
        ranked_actions: ranked_actions
            .into_iter()
            .take(config.action_limit)
            .collect(),
        order_policy,
    })
}
