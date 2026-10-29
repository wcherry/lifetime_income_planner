# Dynamic Spending Strategies — Implementation Plan

Branch: `feature/dynamic-spending-strategies`

## What is changing and why

Add a configurable withdrawal strategy engine so the planner can recommend retirement spending using more than a static percentage rule. The first release should support three strategies:

- Fixed percentage withdrawal
- Guardrails (floor/ceiling bands)
- Guyton-Klinger style rules

This makes the planner more realistic for long retirements and gives users a way to trade off sustainability, flexibility, and spending stability.

## Scope

### In scope

- Add a strategy selection control to the plan and scenario UI
- Persist the selected strategy with the plan/scenario
- Compute withdrawal recommendations using the selected strategy
- Surface guardrail status and strategy rationale in the planner
- Support a simple override for manual adjustment

### Out of scope for v1

- Full portfolio optimization across multiple strategies
- Advanced tax-aware rebalancing logic
- Machine-learning or probabilistic strategy tuning

## Layers affected

- Backend: new strategy engine module, persistence for strategy settings, API endpoints
- Frontend: strategy picker, strategy summary panel, quarter-by-quarter withdrawal display updates
- Data: new tables for persisted strategy definitions and strategy history
- Tests: engine unit tests and UI behavior tests

## Proposed data model

### `withdrawal_strategies`

```text
id                  TEXT PK
user_id             TEXT NOT NULL
name                TEXT NOT NULL
strategy_type       TEXT NOT NULL -- fixed_percentage | guardrails | guyton_klinger
base_percentage     REAL NOT NULL
floor_percentage    REAL NULL
ceiling_percentage  REAL NULL
adjustment_band     REAL NULL
is_default          BOOLEAN NOT NULL DEFAULT 0
created_at          TIMESTAMP NOT NULL
updated_at          TIMESTAMP NOT NULL
FK user_id -> users(id) ON DELETE CASCADE
```

### `plan_withdrawal_strategy_assignments`

```text
id                  TEXT PK
plan_id             TEXT NOT NULL
scenario_id         TEXT NULL
strategy_id         TEXT NOT NULL
applied_at          TIMESTAMP NOT NULL
created_at          TIMESTAMP NOT NULL
FK plan_id -> plans(id) ON DELETE CASCADE
FK scenario_id -> scenarios(id) ON DELETE CASCADE
FK strategy_id -> withdrawal_strategies(id) ON DELETE RESTRICT
```

## Backend implementation plan

### New modules

- `backend/src/withdrawal_strategies.rs` — strategy definitions and rule evaluation
- `backend/src/handlers/withdrawal_strategies.rs` — CRUD endpoints for strategy presets
- `backend/src/handlers/plans.rs` or equivalent plan update flow — apply strategy selection to a plan/scenario

### API endpoints

- `GET /api/withdrawal-strategies` — list strategies for the authenticated user
- `POST /api/withdrawal-strategies` — create a custom strategy template
- `PUT /api/withdrawal-strategies/{id}` — update a strategy template
- `DELETE /api/withdrawal-strategies/{id}` — delete if unused
- `POST /api/plans/{id}/strategy` — assign a strategy to a plan
- `POST /api/scenarios/{id}/strategy` — assign a strategy to a scenario

### Engine behavior

1. Compute the prior-year portfolio value and current spending target.
2. Apply the selected strategy rule to derive the next withdrawal amount.
3. Clamp results to liquidity and account constraints.
4. Return the recommended amount plus a status object (`within_band`, `triggered_floor`, `triggered_ceiling`, `adjustment_applied`).

### Strategy rules

- Fixed percentage: withdraw a fixed percent of portfolio value each period.
- Guardrails: if portfolio value falls below the floor, reduce the withdrawal rate; if it rises above the ceiling, increase it within a bounded range.
- Guyton-Klinger: use a base rate, but temporarily reduce withdrawals after poor performance and increase them only when the portfolio has recovered.

## Frontend implementation plan

### UI additions

- Add a strategy selector in the plan summary and scenario detail screen
- Show a compact summary card with current strategy, next withdrawal, and trigger status
- Add a “strategy explanation” panel for plain-English reasoning

### Files

- `frontend/src/api/types.ts` — strategy-related DTOs
- `frontend/src/api/client.ts` — new strategy API methods
- `frontend/src/data/withdrawalStrategies.ts` — pure helpers for strategy evaluation
- `frontend/src/pages/PlanDetailsPage.tsx` or equivalent — strategy picker and summary UI

## Testing plan

- Backend unit tests for each strategy rule
- Integration tests for persistence and assignment to plans/scenarios
- Frontend unit tests for strategy selection and summary formatting

## Acceptance criteria

- A user can create and assign a withdrawal strategy to a plan or scenario
- Recommended withdrawal amounts change when the strategy changes
- Guardrail conditions are shown clearly to the user
- The selected strategy is persisted and reloaded correctly
- Default behavior remains stable when no strategy is explicitly selected

## Specialists needed

- `rust-developer`: strategy engine, backend endpoints, persistence
- `frontend-developer`: UI integration and strategy summary views
- `test-writer`: unit and integration coverage
