import { defineConfig, devices } from '@playwright/test';
import * as path from 'path';

// The frontend (Vite dev server) is the entry point the browser hits; it proxies
// /api to the isolated test backend on port 8091 (see global-setup.ts).
export const FRONTEND_PORT = 5173;
export const BACKEND_PORT = 8091;
export const BASE_URL = `http://localhost:${FRONTEND_PORT}`;

const runDir = process.env.RUN_DIR ?? '/tmp/lip-e2e/default';

export default defineConfig({
  testDir: './tests',
  outputDir: path.join(runDir, 'playwright-artifacts'),

  // Run serially — all tests share a single backend + frontend.
  workers: 1,
  fullyParallel: false,

  // Retry once on CI to smooth over transient flakiness.
  retries: process.env.CI ? 1 : 0,

  // The backend compiles on first launch, so give setup plenty of headroom.
  timeout: 30_000,

  reporter: [
    ['list'],
    ['html', { outputFolder: path.join(runDir, 'playwright-report'), open: 'never' }],
  ],

  use: {
    baseURL: BASE_URL,
    trace: 'on',
    screenshot: 'only-on-failure',
    video: 'on-first-retry',
  },

  globalSetup: './global-setup.ts',
  globalTeardown: './global-teardown.ts',

  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] },
    },
  ],
});
