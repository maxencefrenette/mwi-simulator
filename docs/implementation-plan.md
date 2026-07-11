# Implementation Plan: MWI Daily Wealth Optimizer

## Objective

Recommend daily MDP actions that maximize expected wealth
for one Milky Way Idle character.

Wealth is valued pessimistically as cash, cash locked in buy orders, inventory
at current bids, and items locked in sell orders at current bids.

## Current Architecture

- `player`: canonical read-only CDP player export model.
- `data`: official marketplace snapshot fetch and parsing.
- `history`: weekly cached market-history refresh and parsing.
- `domain`: shared state, observation, action, order, and event types.
- `money_actions`: noncombat activity production and cost calculations.
- `market_price`: the game's discrete market-price bins.
- `policy`: deterministic observation-to-action heuristic and planning detail.
- `world`: seeded daily transitions with stochastic output and stationary,
  circular replay of each item's historical price series.
- `simulation`: episodic comparison of idle, action-only, and full heuristic
  policies.
- `wealth`: pessimistic current wealth calculation.

## Current Commands

- `fetch-market`
- `wealth`
- `plan`
- `simulate`

## Next Steps

1. Replace the single-package selector with a contingent order portfolio that
   values the probability of unlocking at least one profitable action package.
2. Search valid discrete limit prices between the current ask and the package's
   profitability ceiling.
3. Add sell orders, cancellations, and modifications to the same portfolio.
4. Calibrate stochastic output and market-fill assumptions against observed
   results. MCTS remains out of scope.
