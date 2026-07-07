import type { APIRequestContext, Page } from '@playwright/test';
import { expect } from '../../fixtures/base';

// The frontend stores its bearer token here (see frontend/src/api/client.ts).
export const TOKEN_KEY = 'lip_token';

export interface Credentials {
  email: string;
  password: string;
}

export function uniqueEmail(prefix = 'e2e'): string {
  return `${prefix}_${Date.now()}_${Math.random().toString(36).slice(2, 8)}@example.com`;
}

/** Create a user directly via the API and return their credentials. */
export async function registerViaApi(
  request: APIRequestContext,
  overrides?: Partial<Credentials>,
): Promise<Credentials> {
  const creds: Credentials = {
    email: overrides?.email ?? uniqueEmail(),
    password: overrides?.password ?? 'Password123!',
  };
  const res = await request.post('/api/auth/register', {
    data: creds,
    headers: { 'Content-Type': 'application/json' },
  });
  expect(res.ok(), `API register failed: ${res.status()} ${await res.text()}`).toBeTruthy();
  return creds;
}

/** Log in through the sign-in UI and wait until the app is authenticated. */
export async function loginViaUi(page: Page, creds: Credentials): Promise<void> {
  await page.goto('/login');
  await page.getByLabel('Email').fill(creds.email);
  await page.getByLabel('Password').fill(creds.password);
  await page.getByRole('button', { name: 'Sign in' }).click();
  // A successful login leaves /login and lands on the profile (the "/" route).
  await expect(page).not.toHaveURL(/\/login/, { timeout: 15_000 });
}

/** Register a fresh user via the API, then sign in through the UI. */
export async function registerAndLogin(
  page: Page,
  request: APIRequestContext,
  overrides?: Partial<Credentials>,
): Promise<Credentials> {
  const creds = await registerViaApi(request, overrides);
  await loginViaUi(page, creds);
  return creds;
}

/** Wipe the persisted token so the app treats the session as logged out. */
export async function clearSession(page: Page): Promise<void> {
  await page.evaluate(key => localStorage.removeItem(key), TOKEN_KEY);
}
