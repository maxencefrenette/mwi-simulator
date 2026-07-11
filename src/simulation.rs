use std::collections::HashMap;

use serde::Serialize;

use crate::domain::{Event, MarketSnapshot};
use crate::history::MarketHistoryCache;
use crate::player::{PlayerExport, state_from_export};
use crate::policy::{HeuristicPolicy, IdlePolicy, PlanConfig, Policy};
use crate::world::{World, WorldConfig};

#[derive(Debug, Clone, Copy)]
pub struct SimulationConfig {
    pub days: u32,
    pub episodes: u32,
    pub seed: u64,
}

impl Default for SimulationConfig {
    fn default() -> Self {
        Self {
            days: 30,
            episodes: 100,
            seed: 42,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SimulationReport {
    pub days: u32,
    pub episodes: u32,
    pub policies: Vec<PolicyReport>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PolicyReport {
    pub policy: String,
    pub starting_wealth: f64,
    pub mean_terminal_wealth: f64,
    pub mean_gain: f64,
    pub p10_terminal_wealth: f64,
    pub p50_terminal_wealth: f64,
    pub p90_terminal_wealth: f64,
    pub min_terminal_wealth: f64,
    pub max_terminal_wealth: f64,
    pub mean_action_rejections: f64,
    pub mean_order_fills: f64,
}

pub fn simulate(
    player: &PlayerExport,
    market: &MarketSnapshot,
    histories: &HashMap<String, MarketHistoryCache>,
    plan_config: PlanConfig,
    config: SimulationConfig,
) -> anyhow::Result<SimulationReport> {
    anyhow::ensure!(config.days > 0, "days must be positive");
    anyhow::ensure!(config.episodes > 0, "episodes must be positive");
    let idle = IdlePolicy;
    let action_only = HeuristicPolicy::new(player, histories, plan_config, false);
    let heuristic = HeuristicPolicy::new(player, histories, plan_config, true);
    let policies: [&dyn Policy; 3] = [&idle, &action_only, &heuristic];
    let initial_state = state_from_export(player, market);
    let starting_wealth = crate::domain::pessimistic_wealth(&initial_state, market);
    let mut reports = Vec::new();

    for policy in policies {
        let mut terminal = Vec::new();
        let mut rejection_count = 0usize;
        let mut fill_count = 0usize;
        for episode in 0..config.episodes {
            let mut world = World::new(
                initial_state.clone(),
                market.clone(),
                histories,
                player,
                config.seed.wrapping_add(u64::from(episode)),
                WorldConfig {
                    max_orders: plan_config.max_orders,
                    volume_participation_rate: plan_config.volume_participation_rate,
                    ..WorldConfig::default()
                },
            );
            let mut wealth = starting_wealth;
            for _ in 0..config.days {
                let action = policy.act(&world.observation())?;
                let transition = world.step(&action);
                rejection_count += transition
                    .events
                    .iter()
                    .filter(|event| matches!(event, Event::ActionRejected { .. }))
                    .count();
                fill_count += transition
                    .events
                    .iter()
                    .filter(|event| matches!(event, Event::OrderFilled { .. }))
                    .count();
                wealth = transition.wealth;
            }
            terminal.push(wealth);
        }
        terminal.sort_by(f64::total_cmp);
        let mean = terminal.iter().sum::<f64>() / f64::from(config.episodes);
        reports.push(PolicyReport {
            policy: policy.name().to_string(),
            starting_wealth,
            mean_terminal_wealth: mean,
            mean_gain: mean - starting_wealth,
            p10_terminal_wealth: percentile(&terminal, 0.10),
            p50_terminal_wealth: percentile(&terminal, 0.50),
            p90_terminal_wealth: percentile(&terminal, 0.90),
            min_terminal_wealth: terminal[0],
            max_terminal_wealth: *terminal.last().unwrap(),
            mean_action_rejections: rejection_count as f64 / f64::from(config.episodes),
            mean_order_fills: fill_count as f64 / f64::from(config.episodes),
        });
    }
    Ok(SimulationReport {
        days: config.days,
        episodes: config.episodes,
        policies: reports,
    })
}

fn percentile(values: &[f64], percentile: f64) -> f64 {
    let index = ((values.len() - 1) as f64 * percentile).round() as usize;
    values[index]
}

#[cfg(test)]
mod tests {
    use super::percentile;
    #[test]
    fn percentile_uses_sorted_nearest_rank() {
        assert_eq!(percentile(&[1.0, 2.0, 3.0, 4.0, 5.0], 0.5), 3.0);
    }
}
