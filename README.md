# MWI Simulator

MWI Simulator is organized around the standard MDP vocabulary:

- **Domain** defines observable state, market observations, actions, and events.
- **Policy** maps the current observation to one 24-hour action. The current
  heuristic ranks activities and proposes complementary market orders.
- **World model** applies actions, samples activity output, and advances each
  item's historical prices as a stationary circular time series.
- **Simulation** runs seeded episodes and compares idle, action-only, and full
  heuristic policies by terminal pessimistic wealth.

MCTS is intentionally out of scope. The policy and world interfaces leave room
for it later without coupling the current deterministic planner to search.

Rust CLI planner for Milky Way Idle daily wealth optimization using current
player state, market snapshots, and cached history.

## Run

```sh
mise run chrome-mwi
mise run export-player-state
cargo run -- fetch-market
cargo run -- wealth --player .local/exports/player-state.json --market .local/market.current.json
cargo run -- plan \
  --player .local/exports/player-state.json \
  --market .local/market.current.json \
  --history-dir .local/market-history
cargo run -- simulate \
  --player .local/exports/player-state.json \
  --market .local/market.current.json \
  --history-dir .local/market-history \
  --days 30 --episodes 100 --seed 42
```

`--market` accepts either a normalized market snapshot or the raw official MWI
marketplace payload from `https://www.milkywayidle.com/game_data/marketplace.json`.

The CDP player-state export is read-only and writes to
`.local/exports/player-state.json` by default.

`fetch-market` always refreshes `.local/market.current.json` from the official
API, then refreshes stale files under `.local/market-history/` from the
mooket/Q7 history endpoint. It refuses to reload a history file younger than
seven days unless `--force-history` is passed. History refreshes only request
base item level 0 data and skip item keys or names containing enhancement
markers like `+1`.

`plan` returns the concrete 24-hour MDP action, liquidity-adjusted activity
rankings, and persistent buy-order bundles in one result. Fill delay is
estimated from a configurable share of
historical daily volume, then the package uplift is discounted for waiting,
capital lockup, and order-slot occupancy. Suggested buy limits use the current
ask when available so input packages can start filling immediately, while the
historical ask reach is included in the estimated fill delay. Prices are
rounded to the same discrete bins as the game client before costs and fill
times are evaluated.

## Scope From Notion

- Objective: maximize expected terminal wealth after `N` days.
- Terminal wealth: `cash + conservative_market_value(inventory) + conservative_market_value(open_orders_if_cancelled)`.
- Daily constraints: one 24h action and at most 10 order placements, cancellations, or modifications.
- Current scope: policy evaluation against a deliberately simple stationary
  market world model. Enchanting and MCTS are out of scope.
