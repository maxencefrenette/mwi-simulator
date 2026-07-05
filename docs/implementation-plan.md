# Implementation Plan: MWI Daily Wealth Optimizer

## Source Spec

Notion: MWI daily wealth optimizer

## Requirements Summary

### Functional

- Recommend daily market/order/action decisions for Milky Way Idle.
- Maximize expected terminal wealth after an N-day horizon.
- Account for cash, inventory, and cancellable open orders.
- Enforce daily constraints: one continuous 24h action and at most 10 market order changes.
- Start with sell-side recommendations for current inventory and expected 24h production.
- Later add input-buy planning, brewing/crafting action choice, and full MCTS search.

### Non-Functional

- Rust CLI first.
- Deterministic local JSON inputs for the first implementation layer.
- Conservative terminal valuation so the optimizer cannot hide risk in illiquid inventory.
- Separate data adapters from optimizer logic.

## Architecture

- `model`: player state, production, open orders, and market snapshot types.
- `valuation`: conservative terminal wealth and per-item liquidation value.
- `recommend`: near-term heuristic recommendations, starting with sell orders.
- Future `data`: official marketplace snapshots, userscript exports, and hourly history adapters.
- Future `search`: Monte Carlo simulation and MCTS policy/value loop.

## Phases

### Phase 1: Sell-Side Baseline

- Parse player state, market snapshot, and expected production from JSON.
- Compute conservative terminal wealth.
- Recommend up to 10 sell orders for inventory not already covered by open orders.
- Verify with unit tests and example fixtures.

### Phase 2: Data Ingestion

- Add official `marketplace.json` adapter for `a/b/p/v`. Done for current snapshots.
- Add player-state userscript JSON schema and fixture.
- Add historical hourly window input from mooket/Q7 or self-collected snapshots.

### Phase 3: Probabilistic Sell Model

- Estimate sell-through probability from historical hourly windows.
- Include volume/liquidity penalties, stale-order penalties, and concentration risk.
- Rank sell prices by expected 24h value rather than a simple ask-minus-tick rule.

### Phase 4: Full Daily Optimizer

- Add input-buy planning.
- Add production/action simulation for brewing and crafting.
- Implement MCTS over N-day horizons with daily check-in constraints.

## Current Status

Phase 1 scaffold is implemented with local JSON inputs, conservative valuation,
sell recommendations, examples, and tests. Phase 2 has started with an official
marketplace fetcher and raw snapshot parser.
