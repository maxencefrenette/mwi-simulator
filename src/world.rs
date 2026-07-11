use std::collections::HashMap;

use rand::{RngExt, SeedableRng, rngs::StdRng};
use serde::Serialize;

use crate::domain::{
    Action, Event, MarketAction, MarketQuote, MarketSnapshot, Observation, OpenOrder, OrderSide,
    State, pessimistic_wealth,
};
use crate::history::MarketHistoryCache;
use crate::money_actions::best_money_actions;
use crate::player::{PlayerExport, export_for_observation};

#[derive(Debug, Clone, Copy)]
pub struct WorldConfig {
    pub hours_per_step: f64,
    pub max_orders: usize,
    pub volume_participation_rate: f64,
    pub passive_fill_probability: f64,
}

impl Default for WorldConfig {
    fn default() -> Self {
        Self {
            hours_per_step: 24.0,
            max_orders: 10,
            volume_participation_rate: 0.05,
            passive_fill_probability: 0.10,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Transition {
    pub observation: Observation,
    pub events: Vec<Event>,
    pub wealth: f64,
}

struct Series {
    points: Vec<MarketQuote>,
    cursor: usize,
    stride: usize,
}

pub struct World<'a> {
    state: State,
    market: MarketSnapshot,
    series: HashMap<String, Series>,
    player: &'a PlayerExport,
    rng: StdRng,
    config: WorldConfig,
}

impl<'a> World<'a> {
    pub fn new(
        state: State,
        market: MarketSnapshot,
        histories: &HashMap<String, MarketHistoryCache>,
        player: &'a PlayerExport,
        seed: u64,
        config: WorldConfig,
    ) -> Self {
        let mut rng = StdRng::seed_from_u64(seed);
        let series = histories
            .iter()
            .filter_map(|(item, history)| {
                if history.points.is_empty() {
                    return None;
                }
                let days = history.days.max(1) as usize;
                let points = history
                    .points
                    .iter()
                    .map(|point| MarketQuote {
                        ask: point.ask,
                        bid: point.bid,
                        average: point.average,
                        volume: point.volume,
                    })
                    .collect::<Vec<_>>();
                let cursor = rng.random_range(0..points.len());
                let stride = (points.len() / days).max(1);
                Some((
                    item.clone(),
                    Series {
                        points,
                        cursor,
                        stride,
                    },
                ))
            })
            .collect();
        Self {
            state,
            market,
            series,
            player,
            rng,
            config,
        }
    }

    pub fn observation(&self) -> Observation {
        Observation {
            state: self.state.clone(),
            market: self.market.clone(),
        }
    }

    pub fn step(&mut self, action: &Action) -> Transition {
        let mut events = Vec::new();
        self.place_orders(&action.market_actions, &mut events);
        self.run_activity(action.activity.as_deref(), &mut events);
        self.advance_market();
        self.fill_orders(&mut events);
        self.state.day += 1;
        let observation = self.observation();
        let wealth = pessimistic_wealth(&observation.state, &observation.market);
        Transition {
            observation,
            events,
            wealth,
        }
    }

    fn place_orders(&mut self, actions: &[MarketAction], events: &mut Vec<Event>) {
        for action in actions {
            let MarketAction::PlaceOrder {
                side,
                item,
                quantity,
                limit_price,
            } = action;
            if *quantity <= 0.0
                || *limit_price <= 0.0
                || self.state.open_orders.len() >= self.config.max_orders
            {
                events.push(Event::ActionRejected {
                    reason: format!("invalid or unavailable order slot for {item}"),
                });
                continue;
            }
            match side {
                OrderSide::Buy => {
                    let cost = quantity * limit_price;
                    if self.state.cash < cost {
                        events.push(Event::ActionRejected {
                            reason: format!("insufficient cash for {item}"),
                        });
                        continue;
                    }
                    self.state.cash -= cost;
                    self.state.open_orders.push(OpenOrder {
                        side: *side,
                        item: item.clone(),
                        remaining_quantity: *quantity,
                        limit_price: *limit_price,
                        locked_cash: cost,
                    });
                }
                OrderSide::Sell => {
                    let available = self.state.inventory.get(item).copied().unwrap_or(0.0);
                    if available < *quantity {
                        events.push(Event::ActionRejected {
                            reason: format!("insufficient inventory for {item}"),
                        });
                        continue;
                    }
                    *self.state.inventory.entry(item.clone()).or_default() -= quantity;
                    self.state.open_orders.push(OpenOrder {
                        side: *side,
                        item: item.clone(),
                        remaining_quantity: *quantity,
                        limit_price: *limit_price,
                        locked_cash: 0.0,
                    });
                }
            }
            events.push(Event::OrderPlaced {
                side: *side,
                item: item.clone(),
                quantity: *quantity,
                limit_price: *limit_price,
            });
        }
    }

    fn run_activity(&mut self, activity: Option<&str>, events: &mut Vec<Event>) {
        let Some(activity) = activity else {
            return;
        };
        let observation = self.observation();
        let player = export_for_observation(self.player, &observation);
        let Some(evaluation) = best_money_actions(&player, &self.market, usize::MAX)
            .into_iter()
            .find(|entry| entry.action == activity)
        else {
            events.push(Event::ActionRejected {
                reason: format!("unknown or unavailable activity {activity}"),
            });
            return;
        };
        let requirements = evaluation
            .inputs_per_hour
            .iter()
            .map(|input| {
                (
                    input.item.clone(),
                    input.quantity_per_hour * self.config.hours_per_step,
                )
            })
            .collect::<Vec<_>>();
        if requirements.iter().any(|(item, quantity)| {
            self.state.inventory.get(item).copied().unwrap_or(0.0) < *quantity
        }) {
            events.push(Event::ActionRejected {
                reason: format!("insufficient inputs for {activity}"),
            });
            return;
        }
        for (item, quantity) in requirements {
            *self.state.inventory.entry(item).or_default() -= quantity;
        }
        for output in evaluation.outputs_per_hour {
            let quantity = sample_count(
                &mut self.rng,
                output.quantity_per_hour * self.config.hours_per_step,
            );
            *self.state.inventory.entry(output.item).or_default() += quantity;
        }
        events.push(Event::ActivityCompleted {
            action: activity.to_string(),
        });
    }

    fn advance_market(&mut self) {
        for (item, series) in &mut self.series {
            series.cursor = (series.cursor + series.stride) % series.points.len();
            self.market
                .items
                .insert(item.clone(), series.points[series.cursor].clone());
        }
    }

    fn fill_orders(&mut self, events: &mut Vec<Event>) {
        let mut remaining = Vec::new();
        for mut order in self.state.open_orders.drain(..) {
            let Some(quote) = self.market.items.get(&order.item) else {
                remaining.push(order);
                continue;
            };
            let crosses = match order.side {
                OrderSide::Buy => quote.ask.is_some_and(|price| price <= order.limit_price),
                OrderSide::Sell => quote.bid.is_some_and(|price| price >= order.limit_price),
            };
            if !crosses && self.rng.random::<f64>() >= self.config.passive_fill_probability {
                remaining.push(order);
                continue;
            }
            let capacity = quote.volume.unwrap_or(order.remaining_quantity).max(1.0)
                * self.config.volume_participation_rate;
            let quantity = order.remaining_quantity.min(capacity.max(1.0));
            match order.side {
                OrderSide::Buy => {
                    *self.state.inventory.entry(order.item.clone()).or_default() += quantity;
                    order.locked_cash -= quantity * order.limit_price;
                }
                OrderSide::Sell => self.state.cash += quantity * order.limit_price,
            }
            order.remaining_quantity -= quantity;
            events.push(Event::OrderFilled {
                side: order.side,
                item: order.item.clone(),
                quantity,
                price: order.limit_price,
            });
            if order.remaining_quantity > 1e-9 {
                remaining.push(order);
            }
        }
        self.state.open_orders = remaining;
    }
}

fn sample_count(rng: &mut StdRng, mean: f64) -> f64 {
    if mean <= 0.0 {
        return 0.0;
    }
    if mean < 30.0 {
        let threshold = (-mean).exp();
        let mut product = 1.0;
        let mut count: f64 = 0.0;
        while product > threshold {
            product *= rng.random::<f64>();
            count += 1.0;
        }
        (count - 1.0).max(0.0)
    } else {
        let normal = (0..12).map(|_| rng.random::<f64>()).sum::<f64>() - 6.0;
        (mean + mean.sqrt() * normal).round().max(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::MarketHistoryPoint;

    #[test]
    fn seeded_world_replays_prices_and_fills_crossing_orders_deterministically() {
        let market = MarketSnapshot {
            items: HashMap::from([(
                "tea_leaf".into(),
                MarketQuote {
                    ask: Some(12.0),
                    bid: Some(10.0),
                    average: Some(11.0),
                    volume: Some(100.0),
                },
            )]),
        };
        let history = MarketHistoryCache {
            fetched_at_unix: 0,
            source_url: String::new(),
            item: "tea_leaf".into(),
            item_hrid: "/items/tea_leaf".into(),
            level: 0,
            days: 2,
            points: vec![
                MarketHistoryPoint {
                    time: 1,
                    ask: Some(9.0),
                    bid: Some(8.0),
                    average: Some(8.5),
                    volume: Some(100.0),
                },
                MarketHistoryPoint {
                    time: 2,
                    ask: Some(11.0),
                    bid: Some(10.0),
                    average: Some(10.5),
                    volume: Some(100.0),
                },
            ],
        };
        let histories = HashMap::from([("tea_leaf".into(), history)]);
        let state = State {
            day: 0,
            cash: 100.0,
            inventory: HashMap::new(),
            open_orders: Vec::new(),
            fixed_wealth: 0.0,
        };
        let player = PlayerExport::default();
        let action = Action {
            activity: None,
            market_actions: vec![MarketAction::PlaceOrder {
                side: OrderSide::Buy,
                item: "tea_leaf".into(),
                quantity: 5.0,
                limit_price: 12.0,
            }],
        };
        let mut left = World::new(
            state.clone(),
            market.clone(),
            &histories,
            &player,
            7,
            WorldConfig::default(),
        );
        let mut right = World::new(
            state,
            market,
            &histories,
            &player,
            7,
            WorldConfig::default(),
        );

        let left_transition = left.step(&action);
        let right_transition = right.step(&action);

        assert_eq!(left_transition, right_transition);
        assert_eq!(left_transition.observation.state.inventory["tea_leaf"], 5.0);
        assert!(
            left_transition.events.iter().any(
                |event| matches!(event, Event::OrderFilled { item, .. } if item == "tea_leaf")
            )
        );
    }
}
