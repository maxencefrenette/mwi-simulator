# MWI Simulator

Rust CLI planner for Milky Way Idle daily wealth optimization.

The first milestone is intentionally narrow: load exported player state and a
market snapshot, value inventory conservatively, and recommend sell-side market
orders for current inventory plus expected 24h production.

## Run

```sh
mise run chrome-mwi
mise run export-player-state
cargo run -- fetch-market --output market.current.json
cargo run -- summarize-market --market market.current.json
cargo run -- recommend-sells \
  --state examples/player_state.json \
  --market examples/market_snapshot.json \
  --production examples/production_24h.json
```

`--market` accepts either the small normalized fixture shape used in
`examples/market_snapshot.json` or the raw official MWI marketplace payload from
`https://www.milkywayidle.com/game_data/marketplace.json`.

The CDP player-state export is read-only and writes to
`.local/exports/player-state.json` by default.

## Scope From Notion

- Objective: maximize expected terminal wealth after `N` days.
- Terminal wealth: `cash + conservative_market_value(inventory) + conservative_market_value(open_orders_if_cancelled)`.
- Daily constraints: one 24h action and at most 10 order placements, cancellations, or modifications.
- Starting scope: sell-side recommendations before input-buy planning, action choice, and full MCTS.
