# Spending Tracker — Implementation Plan

Branch: `feature/spending-tracker`

## What is changing and why

Add a transaction-level "Spending Tracker" feature: users import a month's
worth of bank/credit-card transactions from CSV, categorize each transaction,
and browse/edit that history over time. This is explicitly **separate** from
the existing "Spending" page (`SpendingPage.tsx`, `spending_items` table),
which manages planned/budgeted assumptions (essential/discretionary/etc. used
by the projection engine). Nothing in this feature touches
`backend/src/models/spending.rs`, `backend/src/handlers/spending.rs`, or the
`spending_items` table/schema.

The feature also wires into Quarterly Review: a user can jump from the Review
form to the tracker scoped to a quarter's three months and pull categorized
totals back into the Actual Income / Actual Spending fields.

## Layers affected

- **Backend (new)**: migration, models, handlers, route registration, OpenAPI
  registration, seed data for predefined categories.
- **Frontend (new)**: types, api client methods, a new page
  (`SpendingTrackerPage.tsx`), a pure-logic data module
  (`data/spendingTracker.ts`), nav entry + route in `App.tsx`, integration
  into `QuarterlyReviewPage.tsx`.
- **Design**: no new low-level primitives — reuse `Card`, `Button`, `Field`,
  `Select`, `TextInput`, `Alert`, and existing CSS classes
  (`account-list`, `account-row`, `table-scroll`, `proj-table`, `stack`,
  `page-head`, `grid-3`, `form-actions`). A small amount of new CSS may be
  needed for category chips/badges and month-coverage indicators — kept
  minimal and consistent with existing tokens.
- **Tests**: backend unit tests (CSV parsing edge cases, dedupe, category
  CRUD validation), frontend unit tests for `data/spendingTracker.ts`.

## Schema design

Three new tables, all distinct from `spending_items`:

### `spending_tracker_categories`
```
id            TEXT PK
user_id       TEXT NULL   -- NULL = predefined/global, else owned by a user
name          TEXT NOT NULL
kind          TEXT NOT NULL  -- 'income' | 'expense' | 'ignore'
is_predefined BOOLEAN NOT NULL DEFAULT 0
created_at, updated_at TIMESTAMP
FK user_id -> users(id) ON DELETE CASCADE
```
Predefined categories (Housing, Transportation, Food, Entertainment, Medical,
General Merchandise, Dependent Care, Utilities, Pets, Gifts, Other — all
`kind = expense`) are seeded once at startup with `user_id = NULL`,
`is_predefined = true`, following the existing
`seed_tax_tables_if_empty`-style idempotent seed pattern in `db.rs`. A
category list for a user is `WHERE user_id IS NULL OR user_id = :user_id`.
Custom categories are user-scoped, editable/deletable only by their owner;
predefined ones are read-only (handler rejects update/delete on
`is_predefined = true`).

### `spending_tracker_imports`
```
id               TEXT PK
user_id          TEXT NOT NULL
year             INTEGER NOT NULL
month            INTEGER NOT NULL  -- 1-12
source_filename  TEXT NULL
row_count        INTEGER NOT NULL      -- rows newly inserted by this import
duplicate_count  INTEGER NOT NULL      -- rows skipped as already-imported
skipped_count    INTEGER NOT NULL      -- unparseable rows skipped
imported_at      TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
FK user_id -> users(id) ON DELETE CASCADE
INDEX (user_id, year, month)
```
One row per CSV upload (an audit trail), not one row per month.

### `spending_tracker_transactions`
```
id             TEXT PK
user_id        TEXT NOT NULL
import_id      TEXT NOT NULL   -- the import that first created this row
year           INTEGER NOT NULL   -- target month bucket, chosen by user at import time
month          INTEGER NOT NULL
transaction_date DATE NOT NULL    -- the CSV row's own date, for display/sort
description    TEXT NOT NULL
amount         REAL NOT NULL      -- signed: negative=expense, positive=income convention
category_id    TEXT NULL          -- NULL = uncategorized
dedupe_key     TEXT NOT NULL      -- hash of (user_id, year, month, date, normalized description, amount)
created_at, updated_at TIMESTAMP
FK user_id -> users(id) ON DELETE CASCADE
FK import_id -> spending_tracker_imports(id) ON DELETE CASCADE
FK category_id -> spending_tracker_categories(id) ON DELETE SET NULL
UNIQUE INDEX (user_id, dedupe_key)
INDEX (user_id, year, month)
```

**Design choice — the user picks the target year/month at import time**
(rather than deriving it per-row from each transaction's date). A bank
statement's date range can spill a day or two into an adjacent month; forcing
every row from one CSV into the month the user says they're importing avoids
fragmenting a single statement across two month buckets. `transaction_date`
is still stored per row for accurate display and chronological sort.

### Re-import / dedupe decision

**Append with content-based dedupe, not replace.** Each transaction gets a
`dedupe_key` = a hash of `(user_id, year, month, transaction_date,
normalized description, amount)`. Importing a CSV inserts only rows whose
dedupe key isn't already present for that user (enforced with a DB unique
index plus `ON CONFLICT DO NOTHING` at the Diesel/SQLite level, so it's
correct even under concurrent imports). This means:
- Re-importing the exact same file into the same month is a safe no-op for
  already-present rows (they're reported as `duplicate_count`, not
  duplicated in the table) — so a user can't lose their categorization work
  by re-uploading the same statement.
- Importing a *second, different* CSV into the same month (e.g. a checking
  account export and a credit card export for the same calendar month)
  appends its rows alongside the first import's rows rather than wiping them
  out.
- If a bank re-exports a statement with slightly different wording, rows
  will not dedupe (this is a known limitation of content-hash dedupe without
  a stable external transaction id — most exports don't have one). This is
  documented in the API/handler doc comment.

This is safer than a wholesale "replace the month" strategy, which would
silently discard any manual re-categorization a user had already done for
that month.

## API endpoints (new file-per-feature: `handlers/spending_tracker.rs`, `models/spending_tracker.rs`)

- `GET /api/spending-tracker/categories` — predefined + caller's custom categories
- `POST /api/spending-tracker/categories` — create a custom category `{ name, kind }`
- `PUT /api/spending-tracker/categories/{id}` — rename/re-kind a custom category (own only)
- `DELETE /api/spending-tracker/categories/{id}` — delete a custom category (own only); referencing transactions become uncategorized via `ON DELETE SET NULL`
- `POST /api/spending-tracker/import` — `{ year, month, csv_content, source_filename }`, tolerant header-detection CSV parse, returns `{ import_id, imported_count, duplicate_count, skipped_rows: [{ row_number, reason }] }`
- `GET /api/spending-tracker/months` — per-user list of `{ year, month, transaction_count, last_imported_at }`, drives the month picker and quarter-coverage display
- `GET /api/spending-tracker/transactions?year=&month=` — transactions for one month, with category name/kind denormalized into the response
- `PATCH /api/spending-tracker/transactions/{id}` — set `category_id` (assign/change one transaction's category)
- `POST /api/spending-tracker/transactions/bulk-categorize` — `{ transaction_ids: [...], category_id }`
- `GET /api/spending-tracker/quarter-summary?year=&quarter=` — for the Quarterly Review integration: per-month coverage (`has_data`) plus summed `income_total` / `expense_total` (ignore-kind and uncategorized transactions excluded) across whichever of the quarter's 3 months have data

CSV parsing (`parse_spending_transactions_csv`, pure function, unit tested
like `parse_box_data_csv`): case-insensitive header detection for
- date: `date`, `transaction date`, `posting date`, `post date`
- description: `description`, `memo`, `payee`, `name`
- amount: `amount`, `transaction amount` — falls back to combining separate
  `debit`/`credit` columns (debit treated as negative, credit as positive)
  if no single amount column is found
Rows that fail to parse (bad date, no usable amount) are skipped and
reported back in `skipped_rows`, not a hard failure of the whole import —
mirrors `tax_document.rs`'s tolerant-skip approach.

All handlers scope every query by `auth.user_id` (see `quarterly_review.rs`
pattern) via the `AuthUser` extractor.

## Frontend

- `frontend/src/api/types.ts` — `SpendingTrackerCategory`,
  `SpendingTrackerCategoryKind`, `SpendingTrackerTransaction`,
  `SpendingTrackerImportResult`, `SpendingTrackerMonth`,
  `SpendingTrackerQuarterSummary`, plus request DTOs.
- `frontend/src/api/client.ts` — matching `api.*` methods.
- `frontend/src/data/spendingTracker.ts` (+ `.test.ts`) — pure helpers:
  `monthsOfQuarter(quarter)`, `computeCategorizedTotals(transactions,
  categories)`, coverage/formatting helpers — mirrors the
  `data/quarterlyReview.ts` pattern so QuarterlyReviewPage's live preview
  logic is unit-testable without hitting the API.
- `frontend/src/pages/SpendingTrackerPage.tsx` — month picker, CSV upload
  (file input, tolerant to any file the user drags in), category management
  (list predefined read-only + CRUD for custom), transaction table for the
  selected month with a per-row category `<Select>` and bulk-assign
  (checkboxes + "Apply category to selected").
- `frontend/src/App.tsx` — new nav entry "Spending Tracker" (distinct label
  from "Spending") and route `/spending-tracker`.

### Quarterly Review integration UX

No modal primitive exists in `components/ui.tsx` (confirmed), and the repo's
existing pattern for "go do a related task and come back" is plain
navigation, not an overlay. Chosen approach: **query-param scoped
navigation**, per the task's suggested option.

- `ReviewForm` in `QuarterlyReviewPage.tsx` gets an icon button next to the
  Actual income / Actual spending fields that navigates to
  `/spending-tracker?scopeYear=<year>&scopeQuarter=<quarter>`.
- `SpendingTrackerPage`, when those params are present, shows a "Quarter
  scope" banner restricting the month picker to the quarter's 3 months, a
  coverage row (which months have imports, which don't), and a running
  quarter-totals panel from `GET /spending-tracker/quarter-summary`. A
  "Use these totals in Review" button navigates back to
  `/quarterly-review?fillIncome=<n>&fillSpending=<n>&fillYear=<y>&fillQuarter=<q>`.
- `QuarterlyReviewPage` reads those fill params on mount, selects the
  matching due period if not already selected, pre-fills
  `actualIncome`/`actualSpending`, shows a success `Alert`, then
  `navigate(..., { replace: true })` to strip the params so a page refresh
  or back-navigation doesn't re-fill unexpectedly.
- Partial data: `quarter-summary` reports `has_data` per month and only sums
  months that have any imported transactions; the banner clearly shows
  which of the 3 months are missing so totals aren't mistaken for complete.

## Risks / edge cases

- Ambiguous CSV headers (e.g. a column literally named "Amount Owed") —
  matching is exact-ish against a known variant list, not fuzzy, to avoid
  false positives; unmatched files return a clear 400.
- Negative vs. positive convention differs by bank (some export card
  spending as positive) — categorization is still user-editable per row
  regardless of sign, so a wrong default categorization is a one-click fix,
  not a data problem.
- Deleting a custom category that's in use — handled via `ON DELETE SET
  NULL`, transactions fall back to "uncategorized" rather than being deleted
  or blocking the category deletion.
- Quarter spanning a year boundary (Q4 2025 → doesn't happen; quarters never
  cross calendar years) — not an issue given `quarter_of_month`'s existing
  semantics.
- Large CSV files — parsed synchronously in a `web::block`, consistent with
  every other handler in this codebase; no explicit size cap added beyond
  what Actix's default payload limit already enforces.

## Acceptance criteria

- Migration creates the three tables; `schema.rs` regenerated via `diesel
  migration run` against the test DB.
- Predefined categories seeded once, visible to every user, not
  editable/deletable.
- Custom categories are per-user CRUD, validated (`name` required,
  `kind` one of the three).
- CSV import tolerates header variants, skips bad rows without failing the
  whole import, reports counts, and is idempotent on exact re-import.
- Transactions are browsable/editable by month indefinitely (no
  transient/one-shot state).
- Quarterly Review can open the tracker scoped to a quarter and pull back
  auto-filled Actual Income/Spending, correctly excluding `ignore`-kind and
  uncategorized transactions, and gracefully handling months with no data.
- `cargo test`, `yarn test`, `yarn lint`/typecheck all pass; no regressions
  in existing suites.

## Specialists needed

- `rust-developer`: migration, models, handlers, route + OpenAPI
  registration, seed data.
- `frontend-developer`: types, api client, `data/spendingTracker.ts`, page
  component, App.tsx wiring, QuarterlyReviewPage integration.
- `ui-designer`: category chip/badge styling, month-coverage indicator,
  quarter-scope banner, empty/loading states for the new page — reusing
  existing tokens/classes wherever possible.
- `test-writer`: backend unit tests for CSV parsing edge cases and
  category/transaction validation logic; frontend unit tests for
  `data/spendingTracker.ts`.
