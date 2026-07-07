# Lifetime Income Planner

A retirement operating system — a living financial plan that produces quarterly
withdrawal instructions. See [agent_docs/road_map.md](agent_docs/road_map.md) for
the full product vision and phased roadmap.

## Status

**Phase 1 (Financial Foundation / MVP) complete.** Implemented:

1. **User accounts and authentication** — email/password registration and login,
   Argon2 password hashing, JWT bearer tokens.
2. **Retirement profile setup** — name, date of birth, marital status, tax filing
   status, state, planned retirement date, life expectancy, and spouse details for
   married profiles.
3. **Account management** — unlimited accounts across tax categories (taxable,
   tax-deferred, tax-free, other) with manual balances, expected ROI, dividend
   yield, cost basis, target allocation, owner, and withdrawal restrictions.
4. **Spending assumptions** — spending items by category (essential,
   discretionary, healthcare, travel, one-time, charity, …) with amount,
   frequency, inflation adjustment, and optional year bounds.
5. **Income sources** — Social Security, pensions, annuities, employment,
   consulting, etc. with owner, amount/frequency, start/end dates, growth rate,
   COLA, and taxability.
6. **Life events (basic engine)** — future events such as selling a house,
   inheritance, downsizing, Medicare start, claiming Social Security, moves, and
   large purchases, each with an event date, cash inflow/outflow, taxability,
   inflation adjustment, and optional recurrence (one-time, monthly, annual).
7. **Inflation & ROI assumptions** — per-user planning rates for general
   inflation, expected investment return, healthcare inflation, and Social
   Security COLA. New users start from sensible defaults until they save their
   own.
8. **Projection engine** — a pure, unit-tested cash-flow engine
   (`backend/src/projection.rs`) that projects year by year from the current
   year to the end of the plan (the last survivor's life expectancy). Each year
   it grows accounts by their expected ROI, applies inflation-adjusted spending
   (healthcare tracks healthcare inflation), income with growth/COLA, and life
   events, then draws from accounts to cover shortfalls — taxable first, then
   tax-deferred, then tax-free — reinvesting any surplus. It reports net worth,
   projected estate, lifetime totals, and the year (if any) funds run short.
9. **Quarterly withdrawal schedule** — the near-term, actionable output: the
   current year's recommended withdrawals broken into four quarters with
   per-account amounts, following the same taxable-first sequencing. Served
   together with the projection at `GET /api/projection` and shown on the
   **Plan** page.
10. **Net worth projection charts** — a dependency-free, responsive inline-SVG
    area/line chart on the **Plan** page showing projected net worth (each
    year's ending balance) across the whole plan, with a crosshair-and-tooltip
    hover layer, rounded axis ticks, an endpoint value label, the shortfall
    year marked when funds run out, and a "$" marker on each year with life
    events (green for a net inflow, red for an outflow) whose hover tooltip
    lists each event and amount. A top lane of "⚑" milestone flags marks the
    age/regulatory events the engine derives from the birthdate(s) — penalty-free
    withdrawals (59½), Social Security eligibility (62), Medicare (65), full
    retirement age, maximum Social Security (70), the start of RMDs (73/75 per
    SECURE 2.0), and the planned retirement date — each with a hover tooltip,
    and for couples spouse milestones are included and labelled. The same green/
    red event badges and milestone flags appear in the year-by-year table.
11. **Save/load retirement plans** — snapshot the entire working set (profile,
    assumptions, accounts, income, spending, life events) as a named plan on the
    **Saved** page, then load any snapshot back (replacing the working set),
    rename, or delete it. Snapshots are stored as a self-contained JSON document
    (`plans` table) so a saved plan is independent of later edits.

## Tech stack

- **Backend:** Rust · Actix Web · Diesel + SQLite · JWT · Argon2 · Utoipa (OpenAPI)
- **Frontend:** TypeScript · React · Vite · React Router · Jest
- **End-to-end:** Playwright (see [`e2e/`](e2e/))

## Running locally

### Everything at once (recommended)

From the repository root:

```bash
cargo xtask dev
```

This runs the backend and frontend together, installing frontend dependencies on
first run. Press `Ctrl-C` to stop both — if either process exits, the other is
shut down automatically. (`cargo xtask install` installs frontend deps only.)

> **Port note:** the backend defaults to `8080`. If another local service is
> already using it, set `PORT` in `backend/.env` and point the frontend at the
> new port with `VITE_PROXY_TARGET` (e.g.
> `PORT=8091 cargo run` and `VITE_PROXY_TARGET=http://127.0.0.1:8091 yarn dev`).

You can also run each side on its own:

### Backend (`backend/`)

```bash
cd backend
cp .env.example .env          # then edit JWT_SECRET
diesel migration run          # or let the app auto-migrate on startup
cargo run
```

The API listens on `http://127.0.0.1:8080`. Interactive OpenAPI docs are at
`http://127.0.0.1:8080/docs/`.

### Frontend (`frontend/`)

```bash
cd frontend
yarn install
yarn dev
```

The app runs on `http://localhost:5173` and proxies `/api` to the backend.

## Databases: dev vs. test

There are two separate SQLite databases so testing never disturbs your manual data:

| Purpose            | File                          | Config          | Port   |
| ------------------ | ----------------------------- | --------------- | ------ |
| Dev / manual       | `backend/lifetime_income_planner.db` | `backend/.env`      | `8080` |
| Automated / test   | `backend/test.db`             | `backend/.env.test` | `8091` |

- `cargo xtask dev` uses the **dev** database — it persists across restarts, so
  data you enter while manually testing stays put.
- `cargo xtask test-server` runs the backend against a **fresh** test database
  (recreated on every launch) on port 8091, fully isolated from your dev data.
- `cargo xtask test-reset` deletes the test database. Neither test task can touch
  the dev database (the task refuses to run if the two point at the same file).

## API

| Method | Path                 | Auth   | Description                          |
| ------ | -------------------- | ------ | ------------------------------------ |
| GET    | `/api/health`        | —      | Liveness probe                       |
| POST   | `/api/auth/register` | —      | Create an account, returns a token   |
| POST   | `/api/auth/login`    | —      | Authenticate, returns a token        |
| GET    | `/api/auth/me`       | Bearer | Current user                         |
| GET    | `/api/profile`       | Bearer | Fetch the retirement profile         |
| PUT    | `/api/profile`       | Bearer | Create or replace the profile        |
| GET    | `/api/accounts`      | Bearer | List all accounts                    |
| POST   | `/api/accounts`      | Bearer | Create an account                    |
| GET    | `/api/accounts/{id}` | Bearer | Fetch a single account               |
| PUT    | `/api/accounts/{id}` | Bearer | Update an account                    |
| DELETE | `/api/accounts/{id}` | Bearer | Delete an account                    |
| GET    | `/api/spending`      | Bearer | List spending items                  |
| POST   | `/api/spending`      | Bearer | Create a spending item               |
| PUT    | `/api/spending/{id}` | Bearer | Update a spending item               |
| DELETE | `/api/spending/{id}` | Bearer | Delete a spending item               |
| GET    | `/api/income`        | Bearer | List income sources                  |
| POST   | `/api/income`        | Bearer | Create an income source              |
| PUT    | `/api/income/{id}`   | Bearer | Update an income source              |
| DELETE | `/api/income/{id}`   | Bearer | Delete an income source              |
| GET    | `/api/life-events`      | Bearer | List life events                  |
| POST   | `/api/life-events`      | Bearer | Create a life event               |
| PUT    | `/api/life-events/{id}` | Bearer | Update a life event               |
| DELETE | `/api/life-events/{id}` | Bearer | Delete a life event               |
| GET    | `/api/assumptions`   | Bearer | Fetch assumptions (or defaults)      |
| PUT    | `/api/assumptions`   | Bearer | Create or replace assumptions        |

## Tests

### Unit tests

```bash
cd backend  && cargo test   # auth unit tests
cd frontend && yarn jest    # api client tests
```

### End-to-end tests

Browser-driven [Playwright](https://playwright.dev/) tests live in [`e2e/`](e2e/)
and cover Phase 1 features 1–7 (auth, profile, accounts, spending, income, life
events, and assumptions). Each
run launches an **isolated** backend against the fresh `test.db` on port 8091 plus
the Vite frontend — your dev data is never touched, and no Docker is required.

```bash
# one-time: install the e2e deps and browser
cd e2e && npm install && npx playwright install chromium

# from the repo root — starts the stack, runs the suite, tears it down
cargo xtask e2e

# extra args pass straight through:
cargo xtask e2e tests/auth/login.spec.ts   # a single spec
cargo xtask e2e --report                    # open the HTML report afterward
```

See [`e2e/README.md`](e2e/README.md) for the artifact layout and how to run
against an already-running stack.

For quick manual API checks, `cargo xtask test-server` runs the same isolated test
backend on its own so you can exercise `http://127.0.0.1:8091/api/...` directly.
