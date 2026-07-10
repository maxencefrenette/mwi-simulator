# Implementation Plan: MWI Daily Wealth Optimizer

## Objective

Recommend daily action and market-order decisions that maximize expected wealth
for one Milky Way Idle character.

Wealth is valued pessimistically as cash, cash locked in buy orders, inventory
at current bids, and items locked in sell orders at current bids.

## Current Architecture

- `player`: canonical read-only CDP player export model.
- `data`: official marketplace snapshot fetch and parsing.
- `history`: weekly cached market-history refresh and parsing.
- `money_actions`: noncombat action production and cost calculations.
- `rank_actions`: history-aware output-liquidity adjustment.
- `market_price`: the game's discrete market-price bins.
- `recommend_orders`: persistent input-buy package recommendations.
- `wealth`: pessimistic current wealth calculation.

## Current Commands

- `fetch-market`
- `fetch-all-history`
- `wealth`
- `rank-actions`
- `recommend-orders`

## Next Steps

1. Replace the single-package selector with a contingent order portfolio that
   values the probability of unlocking at least one profitable action package.
2. Search valid discrete limit prices instead of always using a passive
   `bid + tick` target.
3. Add sell orders, cancellations, and modifications to the same portfolio.
4. Use the deterministic policy as candidate generation and value priors for
   longer-horizon MCTS.
