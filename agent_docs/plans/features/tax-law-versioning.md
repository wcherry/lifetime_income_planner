# Tax Law Versioning — Implementation Plan

Branch: `feature/tax-law-versioning`

## What is changing and why

Add the ability to model retirement plans under multiple tax law assumptions. This allows users to compare outcomes under current law, proposed legislation, or a custom tax assumption set without changing their base plan data.

The initial implementation should support:

- Current law baseline
- A user-defined custom law profile
- A simple “proposed legislation” profile for scenario comparison

This is a foundational step toward long-term tax forecasting and policy sensitivity analysis.

## Scope

### In scope

- Persist tax law versions as first-class planning data
- Apply a selected tax law version to tax calculations and projections
- Allow users to compare results across versions in a scenario or plan view
- Provide audit visibility for which version was used in a projection run

### Out of scope for v1

- Full parsing of real legislative text
- Historical tax law snapshots beyond the selected version set
- State-level dynamic law modeling beyond the existing state tax engine

## Layers affected

- Backend: tax engine integration, persistence for tax law versions, API endpoints
- Frontend: selector in assumptions and scenario views, comparison UI
- Data: new tables and plan/scenario references
- Tests: tax calculation regression tests and UI selector tests

## Proposed data model

### `tax_law_versions`

```text
id                  TEXT PK
user_id             TEXT NULL
name                TEXT NOT NULL
description         TEXT NULL
version_type        TEXT NOT NULL -- baseline | proposed | custom
is_default          BOOLEAN NOT NULL DEFAULT 0
tax_assumption_json TEXT NOT NULL
created_at          TIMESTAMP NOT NULL
updated_at          TIMESTAMP NOT NULL
FK user_id -> users(id) ON DELETE CASCADE
```

### `plan_tax_law_assignments`

```text
id                  TEXT PK
plan_id             TEXT NOT NULL
scenario_id         TEXT NULL
tax_law_version_id  TEXT NOT NULL
applied_at          TIMESTAMP NOT NULL
created_at          TIMESTAMP NOT NULL
FK plan_id -> plans(id) ON DELETE CASCADE
FK scenario_id -> scenarios(id) ON DELETE CASCADE
FK tax_law_version_id -> tax_law_versions(id) ON DELETE RESTRICT
```

## Backend implementation plan

### New modules

- `backend/src/tax_law_versioning.rs` — schema and version resolution logic
- `backend/src/handlers/tax_law_versions.rs` — CRUD and lookup endpoints
- `backend/src/tax.rs` or equivalent tax engine integration — inject the active tax law version into calculations

### API endpoints

- `GET /api/tax-law-versions` — list versions available to the current user
- `POST /api/tax-law-versions` — create a custom version profile
- `PUT /api/tax-law-versions/{id}` — update a custom version profile
- `DELETE /api/tax-law-versions/{id}` — remove if unused
- `POST /api/plans/{id}/tax-law-version` — assign a version to a plan
- `POST /api/scenarios/{id}/tax-law-version` — assign a version to a scenario

### Tax engine integration

The tax engine should read the selected version when calculating:

- federal tax
- state tax
- capital gains treatment
- qualified dividends
- Social Security taxation
- estimated quarterly taxes
- RMD-related taxation

The selected version must be visible in the output payload so the UI can display the assumptions used for the calculation.

## Frontend implementation plan

### UI additions

- Add a tax law version selector in assumptions and plan detail views
- Show the active version on the tax summary and scenario comparison page
- Add a comparison mode to contrast “current law” vs “proposed law” outcomes side by side

### Files

- `frontend/src/api/types.ts` — tax law version DTOs
- `frontend/src/api/client.ts` — new API client methods
- `frontend/src/pages/AssumptionsPage.tsx` or equivalent — selector and form controls
- `frontend/src/pages/ScenarioComparisonPage.tsx` — comparison surface

## Testing plan

- Backend regression tests for current law vs custom version calculation changes
- Tests for persistence and version assignment to plans/scenarios
- Frontend tests for selector state and comparison output formatting

## Acceptance criteria

- A user can create and assign a tax law version to a plan or scenario
- Tax outputs change when the selected version changes
- The active tax law version is visible in the UI and API responses
- The system supports at least one baseline and one custom version out of the box
- Existing tax calculations remain stable when no custom version is selected

## Specialists needed

- `rust-developer`: tax engine integration and API layer
- `frontend-developer`: selector and comparison UI
- `test-writer`: regression and behavior tests
