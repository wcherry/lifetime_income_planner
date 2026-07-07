import { spawn } from 'child_process';
import * as fs from 'fs';
import * as path from 'path';
import { BACKEND_PORT, BASE_URL, FRONTEND_PORT } from './playwright.config';

const REPO_ROOT = path.resolve(__dirname, '..');
const BACKEND_DIR = path.join(REPO_ROOT, 'backend');
const FRONTEND_DIR = path.join(REPO_ROOT, 'frontend');

const BACKEND_URL = `http://127.0.0.1:${BACKEND_PORT}`;
const POLL_INTERVAL_MS = 1000;
const BACKEND_TIMEOUT_MS = 240_000; // first `cargo run` compiles the backend
const FRONTEND_TIMEOUT_MS = 60_000;

/** Minimal KEY=VALUE .env parser (blank lines and `#` comments ignored). */
function loadEnvFile(file: string): Record<string, string> {
  const out: Record<string, string> = {};
  for (const line of fs.readFileSync(file, 'utf-8').split('\n')) {
    const trimmed = line.trim();
    if (!trimmed || trimmed.startsWith('#')) continue;
    const eq = trimmed.indexOf('=');
    if (eq === -1) continue;
    out[trimmed.slice(0, eq).trim()] = trimmed.slice(eq + 1).trim();
  }
  return out;
}

/**
 * Delete the throwaway test database so every run starts clean. Only ever
 * touches the path from `.env.test`, and refuses to run if that somehow points
 * at the dev database — the dev data must never be reset (see AGENTS/memory).
 */
function resetTestDb(testEnv: Record<string, string>): void {
  const testDb = testEnv.DATABASE_URL;
  if (!testDb) throw new Error('DATABASE_URL missing from backend/.env.test');

  const devEnv = loadEnvFile(path.join(BACKEND_DIR, '.env'));
  if (testDb === devEnv.DATABASE_URL) {
    throw new Error(
      `Refusing to run: .env.test DATABASE_URL (${testDb}) matches the dev database. ` +
        'Point .env.test at a separate file.',
    );
  }

  for (const suffix of ['', '-journal', '-wal', '-shm']) {
    const p = path.join(BACKEND_DIR, `${testDb}${suffix}`);
    if (fs.existsSync(p)) fs.rmSync(p);
  }
}

function openLog(runDir: string, name: string): number {
  const dir = path.join(runDir, 'service-logs');
  fs.mkdirSync(dir, { recursive: true });
  return fs.openSync(path.join(dir, `${name}.log`), 'a');
}

/** Spawn a long-lived child in its own process group and record its pid. */
function spawnService(
  command: string,
  args: string[],
  opts: { cwd: string; env: NodeJS.ProcessEnv; logFd: number },
): number {
  const child = spawn(command, args, {
    cwd: opts.cwd,
    env: opts.env,
    detached: true, // own process group so teardown can kill the whole tree
    stdio: ['ignore', opts.logFd, opts.logFd],
  });
  child.unref();
  if (!child.pid) throw new Error(`Failed to spawn ${command}`);
  return child.pid;
}

async function waitForUrl(url: string, timeoutMs: number, label: string): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  let lastError = '';
  while (Date.now() < deadline) {
    try {
      const res = await fetch(url);
      if (res.status < 500) {
        console.log(`${label} is ready (HTTP ${res.status})`);
        return;
      }
      lastError = `HTTP ${res.status}`;
    } catch (err) {
      lastError = String(err);
    }
    await new Promise(r => setTimeout(r, POLL_INTERVAL_MS));
  }
  throw new Error(
    `${label} did not become ready within ${timeoutMs / 1000}s. Last error: ${lastError}`,
  );
}

export default async function globalSetup(): Promise<void> {
  // Resolve the run directory (run-tests.sh sets RUN_DIR; fall back otherwise).
  let runDir = process.env.RUN_DIR;
  if (!runDir) {
    runDir = `/tmp/lip-e2e/manual_${Date.now()}`;
    process.env.RUN_DIR = runDir;
    console.log(`RUN_DIR not set — using fallback: ${runDir}`);
  }
  for (const sub of ['service-logs', 'browser-logs', 'playwright-artifacts', 'playwright-report']) {
    fs.mkdirSync(path.join(runDir, sub), { recursive: true });
  }

  fs.writeFileSync(
    path.join(runDir, '.run_meta.json'),
    JSON.stringify(
      { runId: path.basename(runDir), startedAt: new Date().toISOString(), runDir },
      null,
      2,
    ),
  );

  // SKIP_SERVERS lets you run against an already-running test stack (e.g. one you
  // started with `cargo xtask test-server` + `yarn dev`).
  if (process.env.SKIP_SERVERS) {
    console.log('SKIP_SERVERS set — assuming backend + frontend are already running.');
    await waitForUrl(`${BACKEND_URL}/api/health`, FRONTEND_TIMEOUT_MS, 'Backend');
    await waitForUrl(BASE_URL, FRONTEND_TIMEOUT_MS, 'Frontend');
    return;
  }

  const testEnv = loadEnvFile(path.join(BACKEND_DIR, '.env.test'));
  resetTestDb(testEnv);

  console.log(`Starting test backend on ${BACKEND_URL} (fresh test database)…`);
  const backendPid = spawnService('cargo', ['run', '--package', 'lifetime_income_planner'], {
    cwd: BACKEND_DIR,
    // .env.test values override anything in the process/.env so we hit test.db:8091.
    env: { ...process.env, ...testEnv },
    logFd: openLog(runDir, 'backend'),
  });

  console.log(`Starting frontend on ${BASE_URL}…`);
  const frontendPid = spawnService('yarn', ['dev', '--port', String(FRONTEND_PORT), '--strictPort'], {
    cwd: FRONTEND_DIR,
    // Point Vite's /api proxy at the test backend instead of the dev backend.
    env: { ...process.env, VITE_PROXY_TARGET: BACKEND_URL },
    logFd: openLog(runDir, 'frontend'),
  });

  // Persist pids so global-teardown (a separate module invocation) can stop them.
  fs.writeFileSync(
    path.join(runDir, '.pids.json'),
    JSON.stringify({ backendPid, frontendPid }, null, 2),
  );

  await waitForUrl(`${BACKEND_URL}/api/health`, BACKEND_TIMEOUT_MS, 'Backend');
  await waitForUrl(BASE_URL, FRONTEND_TIMEOUT_MS, 'Frontend');
}
