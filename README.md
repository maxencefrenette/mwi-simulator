# MWI Simulator

Rust CLI planner for Milky Way Idle daily wealth optimization using current
player state, market snapshots, and cached history.

## Run

```sh
mise run chrome-mwi
mise run export-player-state
cargo run -- fetch-market
cargo run -- wealth --player .local/exports/player-state.json --market .local/market.current.json
cargo run -- rank-actions \
  --player .local/exports/player-state.json \
  --market .local/market.current.json \
  --history-dir .local/market-history
cargo run -- recommend-orders \
  --player .local/exports/player-state.json \
  --market .local/market.current.json \
  --history-dir .local/market-history
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

`recommend-orders` values persistent buy-order bundles by the 24-hour action
packages they unlock. Fill delay is estimated from a configurable share of
historical daily volume, then the package uplift is discounted for waiting,
capital lockup, and order-slot occupancy. The output includes the selected
bundle, reservation prices, and ranked alternatives. Suggested limits use a
passive `current bid + tick` policy, capped at the current ask; historical ask
reach is included in the estimated fill delay. Prices are rounded to the same
discrete bins as the game client before costs and fill times are evaluated.

## Scope From Notion

- Objective: maximize expected terminal wealth after `N` days.
- Terminal wealth: `cash + conservative_market_value(inventory) + conservative_market_value(open_orders_if_cancelled)`.
- Daily constraints: one 24h action and at most 10 order placements, cancellations, or modifications.
- Current scope: wealth, liquidity-aware action ranking, and persistent input-buy planning before full MCTS.
