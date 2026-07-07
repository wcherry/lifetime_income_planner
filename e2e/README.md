# lifetime-income-planner-e2e

End-to-end tests for the Lifetime Income Planner using [Playwright](https://playwright.dev/).

Each run launches an **isolated** backend against a throwaway test database
(`backend/test.db` on port `8091`, from `backend/.env.test`) plus the Vite
frontend (port `5173`) with its `/api` proxy pointed at that test backend. The
persistent dev database (`backend/lifetime_income_planner.db`, port `8080`) is
never touched. Servers are torn down when the run finishes.

There is **no Docker** — the stack runs as local processes.

## Coverage

Phase 1 features 1–5 (see `agent_docs/road_map.md`):

| Feature | Area                        | Spec                          |
| ------- | --------------------------- | ----------------------------- |
| 1       | User accounts & auth        | `tests/auth/*.spec.ts`        |
| 2       | Retirement profile setup    | `tests/profile/profile.spec.ts` |
| 3       | Account management          | `tests/accounts/accounts.spec.ts` |
| 4       | Spending assumptions        | `tests/spending/spending.spec.ts` |
| 5       | Income sources              | `tests/income/income.spec.ts` |

## Prerequisites

- Node.js 20+
- Rust toolchain (`cargo`) and Yarn — the same tools `cargo xtask dev` needs
- Playwright's Chromium browser

## Setup

```bash
cd e2e
npm install
npx playwright install chromium
```

## Running

Full run (starts servers, runs all tests, tears everything down):

```bash
npm test
```

The first run compiles the backend via `cargo run`, so global setup allows a few
minutes before timing out.

Run a single spec:

```bash
./scripts/run-tests.sh tests/auth/login.spec.ts
```

Open the HTML report afterwards:

```bash
./scripts/run-tests.sh --report          # run then show
npm run report                            # show the last report
```

### Against an already-running stack

If you've started the stack yourself (e.g. `cargo xtask test-server` in one
terminal and `VITE_PROXY_TARGET=http://127.0.0.1:8091 yarn dev` in another),
skip the managed servers:

```bash
npm run test:no-servers
```

## Artifact layout

Every run writes a self-contained directory under `/tmp/lip-e2e/<run-id>/`:

```
<run-id>/
├── .run_meta.json          # run id + start time
├── .pids.json              # pids of the spawned backend/frontend (for teardown)
├── service-logs/           # backend.log, frontend.log
├── browser-logs/           # per-test console + network capture (JSON)
├── playwright-artifacts/   # traces, screenshots, videos
└── playwright-report/      # HTML report (open with `show-report`)
```

## Adding tests

1. Create a spec under `tests/<feature>/`.
2. Import `test` and `expect` from `../../fixtures/base` (not `@playwright/test`)
   to get automatic console + network capture.
3. Reuse the helpers in `tests/helpers/auth.ts` to register/sign in.
4. Use relative URLs — `baseURL` is the frontend (`http://localhost:5173`), and
   API calls via the `request` fixture hit the same origin (proxied to `:8091`).
