# Web ↔ desktop parity

What it would take to bring the web version (this repo: Rust/Actix backend, React frontend) to feature
parity with the macOS desktop app (`lifetime_income_planner_macos`: Swift, SwiftUI, SwiftData).

**How this was written.** It's a survey of both code bases as of 2026-10-31, after the desktop ACA-2026 work
(macOS PRs #11–#15) and this repo's PR #3 merged. Nothing was run in a browser, and the frontend
`node_modules` weren't installed, so frontend claims come from reading the code. Treat each "gap" as a
pointer to check, not a verified defect. Sizes are rough: **S** about a day, **M** several days,
**L** a week or more.

## 1. Where the two stand

| Area | Desktop | Web | Gap |
|---|---|---|---|
| Projection engine: taxes, RMDs, Medicare, IRMAA, ACA | yes | yes (bit-level differences, see 2.1) | none |
| ACA 2026 rules, ACA-aware Roth mode | yes | yes (#3) | none |
| IRMAA-aware Roth mode, Medicare enrollment age, regulatory milestones | yes | yes | none |
| Mid-year income proration | yes | yes | none |
| What-if, optimizer, Monte Carlo, scenarios, compare | yes | yes | check details (2.3) |
| **Cash management** (reserve, drawdown, refill, goals, alerts, CD/HSA rules) | yes | **no** | **L** |
| **Holdings** (symbol, shares, basis, live quotes) feeding account balance, basis, yield, allocation | yes | **no** | **L** |
| **Stock sales tool** (tax on selling lots to raise cash) | yes | **no** | **M** |
| Projection overview: year picker, chart marker, per-year cards | yes | no (first year only) | **M** |
| MAGI as % of poverty line on the tax breakdown | yes | no | **S** |
| Budget wizard (build spending lines from tracker history) | yes | no | **M** |
| Social Security claiming-age recommendation | yes | no | **S** |
| Social Security statement PDF import | yes | manual entry | **M** |
| Setup checklist and guided onboarding | yes | no | **M** |
| Reference-table editor, import and update from a URL or file | yes | no (tables seeded in the DB, no UI) | **M** |
| Insights with explanations | template, Claude, on-device | template, Claude | none (on-device is desktop-only) |
| Quarterly review, tasks, annual wizard, spending tracker, tax documents | yes | yes | check details (2.3) |
| Reports: CSV, PDF, print | yes | CSV, PDF | **S** (print styles) |
| Reminders | macOS notifications | none | **M**, needs a design |
| Accounts, sharing, aggregation | single user, local file | multi-user, collaborators, Plaid | web-only (section 5) |

## 2. Engine (`backend/src`)

### 2.1 Already matching
These came over in #3 and the work before it:
- `AcaRules` (enhanced credits through 2025, the original 2026 schedule after, the 400% cutoff, a one-year
  poverty-guideline lag), stored in `aca_rules` and `aca_standard_applicable_percentages`.
- `roth_conversion_mode` (`fixed_ceiling`, `irmaa_aware`, `aca_aware`), `medicare_enrollment_age`,
  `Milestone.is_regulatory`.
- A test-only replay of the desktop's golden fixtures: `fixture_parity.rs`
  (`cargo test fixture_parity -- --ignored`). All 31 match to 1e-9. Bit-for-bit equality holds for only
  4 of them on the machine this was run on, and that is also true of this repo's own exporter. The
  fixtures were presumably generated on a Mac, so check bit equality there before treating the last-digit
  differences as a defect.

Still open in this area:
- **The ACA-rules fixtures live in two places.** `fixture_parity.rs::export_aca_rules_fixtures` writes the
  `projection_rules/` scenarios, while `fixture_export.rs` writes the original `projection/` set. Decide
  which owns fixture generation, and make `cargo xtask export-fixtures` write both.
- **Rules beyond 2026.** The standard-schedule percentages are held flat after 2026 (the 2027 table is in
  Rev. Proc. 2026-26 and isn't loaded). Also verify the 2026 values against Rev. Proc. 2025-25. They were
  taken from secondary summaries on both sides.

### 2.2 Missing engine features (all desktop-only today)
Port each from `Sources/PlannerEngine` and `Sources/PlannerStore`, with the Swift tests as the spec.

1. **Cash management engine (L).** Swift: `Cash/CashPlan.swift`, `Cash/CashAnalysis.swift`, and the
   changes in `ProjectionEngine.swift` and `WithdrawalPlanning.swift`.
   - `CashPolicy`: reserve target (months of spending and/or dollars, the larger applies), drawdown position
     (with the portfolio, first, last), refill rule (surplus only, always, only when the portfolio isn't
     down), CD early-withdrawal haircut (months of interest).
   - What counts as cash: savings, checking, money market, CD and HSA accounts, plus the CASH holdings of
     other accounts (needs holdings, 2.2.2). Cash in retirement accounts counts at 59½; an HSA counts at any
     age. An HSA is never topped up from surplus or reordered as spendable cash.
   - Engine hooks: reserve top-up from surplus before reinvesting, refill planning
     (`plan_withdrawals_with_refill`), cash positioning in the drawdown order.
   - `CashAnalysis`: summary (cash on hand, funded %, months covered, CD haircut), goal progress, alerts.
   - Off by default and adds no output fields, so existing fixtures stay valid.
   - Note: the year solve is now a retry loop (for ACA-aware conversions). The cash hooks sit inside it, as
     they do in Swift's `runPass`. Port against the current structure, not the older one.
2. **Holdings (L).** `HoldingsSummary` (balance, basis, yield and stock/bond/cash allocation computed from the
   holdings, weighted by value), `FundType`, and the exact cash dollars per account (`cash_holdings`), which
   the engine prefers over the rounded `allocation_cash_pct`.
3. **Stock sale tax planner (M).** `Tax/StockSaleTaxPlanner.swift`: given a cash target or shares to sell,
   picks lots (losers and low-gain first) and computes federal, state and 3.8% NIIT on the net gain. Needs
   holdings first.
4. **Social Security claiming recommendation (S).** `ClaimingRanking.recommend` in
   `SocialSecurityClaiming.swift`: projects the plan at ages 62, 67 and 70 and ranks them.
5. **Budget wizard (M).** `BudgetWizard.swift`: turns spending-tracker history into proposed budget lines
   (monthly average, COLA period, end year). Pure logic over data this repo already stores.
6. **Setup checklist logic (S).** `SetupChecklist.swift`: which inputs exist, which page to open next.
7. **Statement parser (M).** `SocialSecurityStatementParser.swift` reads the text of an SSA "my Social
   Security" PDF. The web would need server-side PDF text extraction (a crate such as `pdf-extract` or
   `lopdf`), then the same parsing rules.

### 2.3 Behaviors to compare (not yet checked)
These exist on both sides. Run each against the same inputs and diff the outputs before calling them done:
- Monte Carlo: Swift runs trials in parallel from pre-drawn, seeded shocks; the fixture is a statistical
  match, not exact (`monte_carlo.json`).
- What-if overrides and the optimizer: Swift pulled these into `Scenarios/WhatIf.swift` and
  `Scenarios/Optimizer.swift` (originally Rust handler code). Confirm the Rust handlers still agree.
- Scenario history: Rust keeps plan versions and clones; the desktop has refresh, restore and compare of up
  to 10 scenarios. Confirm the compare limits and the "depletion age" figure match.
- Insights: both have the same nine categories. Compare thresholds and message wording.
- Quarterly review, tasks, annual wizard, tax documents: same data, different persistence. Compare the
  derived tasks (`expected_tasks` in the fixtures covers part of this).
- Spending tracker: the desktop has a compare-months dialog with a budget column, and learned
  category mappings. Check the web page for both.

## 3. Data model and API (`backend/migrations`, `models`, `handlers`)

New tables and endpoints, following the existing pattern (Diesel migration, `schema.rs`, model, handler,
OpenAPI, plan-snapshot support).

| Table | Purpose |
|---|---|
| `holdings` | `account_id`, `sort_index`, `symbol`, `shares`, `price`, `price_date`, `cost_basis`, `dividend_yield`, `fund_type` |
| `cash_plans` | one row per user (or per plan): reserve months and dollars, drawdown position, refill rule, CD penalty months, goals, CD maturity dates. The desktop stores this as one JSON document, which also works here |

- Plan snapshots (`models/plan.rs`) must carry the cash plan and the holdings-derived account figures so saved
  scenarios, compare and Monte Carlo reproduce them. Default the new fields for plans saved earlier.
- Endpoints: `GET/PUT /api/cash-plan`, `GET /api/cash-analysis`; `GET/PUT /api/accounts/{id}/holdings`;
  `POST /api/stock-sales/plan`; `POST /api/budget-wizard/propose`; `GET /api/social-security/claiming`;
  `GET /api/quotes/{symbol}`.
- **Quote lookups must go through the backend.** The desktop calls a quote service directly. A browser would
  hit CORS and expose any key. Add a small cached proxy with rate limiting and a clear error when the
  provider is down. Pick the provider deliberately (what `QuoteClient.swift` uses is the starting point, but
  check its terms for server-side use).
- Keep assumption additions backward compatible: `serde(default)`, a column default, and a snapshot default.

## 4. Frontend (`frontend/src`)

Pages and components to add or change, in rough order of value.

1. **Projection overview: year picker (M).**
   - A year dropdown, a vertical marker on `NetWorthChart.tsx`, and the withdrawal schedule, estimated
     taxes, tax breakdown, ACA and IRMAA cards following the selected year.
   - Two extra tiles: balance at the end of the year and that year's taxes.
   - The first-year-only schedule is currently baked into the response. Either compute the per-year
     quarterly schedule and estimated taxes in the backend (the desktop made `Schedules.quarterly` and
     `estimatedTaxes` public for this) or send enough per-year data to compute them in the browser.
   - Click on the chart selects a year; a "Details" button opens the full year.
2. **MAGI as % of poverty line (S).** On the tax breakdown: "MAGI … · 331% of the federal poverty line
   ($30,120)", shown only for years where the ACA figures exist (benchmark premium set and someone under 65).
3. **Cash page (L).** Cash on hand with funded % and months covered, alerts, reserve, drawdown and refill
   settings, savings goals with progress, CD maturity dates and value-if-cashed, and editable balances for
   cash accounts and cash funds. The desktop's `CashView.swift` is the layout reference. It also needs the
   holdings editor's CASH row to be a single "Amount" field.
4. **Holdings editor in the account form (L).** A holdings list per account with symbol lookup (price, fund
   type and yield filled in), shares, cost basis per share, and a computed balance, basis, yield and
   allocation. A CASH symbol is one editable amount.
5. **Stock sales page (M).** Spending-cash target or shares to sell, lot choice, federal, state and NIIT.
6. **Social Security page (S + M).** Claiming recommendation (S); PDF import of the statement (M, needs 2.2.7).
7. **Budget wizard on the Spending page (M).**
8. **Setup checklist and onboarding (M).** Today the app drops a new user on empty pages. Add a "what to
   enter first" flow that locks planning pages until profile, accounts, income and spending exist.
9. **Reference tables page (M).** View and edit tax, state, ACA (including the new rules) and IRMAA tables;
   import from a file or HTTPS address with a diff before applying; reset to built-in. Needs endpoints that
   validate the way `ReferenceTableSet.issues` does on the desktop, an admin or owner role rather than every
   user, and care about per-user versus global tables (the web tables are global).
10. **Assumptions page (S).** Already has the ACA-aware option; add anything the cash plan needs (CD
    penalty months).
11. **Reports (S).** Print-friendly styles for the annual plan report; keep CSV and PDF.
12. **Reminders (M).** Quarterly-review and task reminders. The desktop schedules macOS notifications. On the
    web that means email or web push, which brings in a mail provider, unsubscribe handling and a job runner.
    Decide whether it's wanted before building.

## 5. Not parity targets

**Desktop-only by nature** (the web has its own answer, or none is needed):
- Plan files with PIN encryption, and the legacy store import: the web has accounts, server storage and
  `auth.rs`. Confirm backups and data export exist (CSV and a full JSON export would be the equivalent).
- App lock, Spotlight indexing, menu bar, Sparkle updates, crash-report handling.
- On-device Apple Intelligence explanations. The Claude and template providers cover the rest.

**Web-only, which the desktop lacks** (optional to backport):
- Multi-user accounts and auth, collaborators and shared plans, Plaid account aggregation.

## 6. Cross-cutting

- **Shared fixtures are the contract.** For each ported engine feature, add scenarios to
  `fixture_export.rs`, regenerate into the desktop's `Tests/PlannerEngineTests/Fixtures`, and keep both
  repos' replay tests green. Cash management is desktop-only today; once ported, give it
  `projection_cash/` scenarios the same way `projection_rules/` works.
- **Which engine is the source of truth?** Right now both are edited independently and cross-checked by
  fixtures. Pick one process: change one engine, regenerate fixtures, and port the change in the same
  sprint. The ACA work showed what happens otherwise: the Rust copy fell behind in several ways at once.
- **Machine differences.** Fixtures generated on arm64 and replayed on x86_64 differ in the last digit.
  Either generate fixtures in CI on one platform or compare with a tolerance (the desktop test for
  `projection_rules` uses 1e-9 for this reason).
- **Build tooling.** `utoipa-swagger-ui` downloads a zip from GitHub at build time. Environments without
  that access cannot build the backend. Vendoring it (the `vendored` feature) or caching the zip removes
  the dependency.
- **Validation.** Mirror the desktop's `Validation.swift` rules in the new request models
  (`validator` ranges and cross-field checks) so both apps reject the same inputs.
- **Docs.** Update `README.md` and `agent_docs/road_map.md` as each item ships.

## 7. Suggested order

1. Year picker, MAGI percent, and the Social Security claiming recommendation. All S or M, mostly frontend,
   and no new tables.
2. Holdings (data model, endpoints, quote proxy, editor). Everything cash-related depends on it.
3. Cash management engine, then the Cash page, then fixtures for it.
4. Stock sales, budget wizard, setup checklist.
5. Reference-table editor and PDF statement import.
6. Reminders, if wanted.

Items 1 and 2 can run in parallel. Item 3 needs 2.
