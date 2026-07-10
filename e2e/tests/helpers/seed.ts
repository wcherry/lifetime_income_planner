import type { APIRequestContext, Page } from '@playwright/test';
import { expect } from '../../fixtures/base';
import { registerAndLogin, TOKEN_KEY, type Credentials } from './auth';

// Minimal local mirrors of the backend DTOs we need (see
// frontend/src/api/types.ts for the authoritative shapes — not imported
// directly since the e2e package builds standalone from the frontend).

export interface SeedProfileRequest {
  first_name: string;
  last_name: string;
  date_of_birth: string;
  marital_status: string;
  filing_status: string;
  state: string;
  retirement_date: string;
  life_expectancy: number;
}

export interface SeedAccountRequest {
  name: string;
  category: string;
  account_type: string;
  owner: string;
  current_balance: number;
  expected_roi: number;
  dividend_yield: number;
  cost_basis?: number | null;
}

export interface SeedSpendingRequest {
  name: string;
  category: string;
  amount: number;
  frequency: string;
  inflation_adjusted: boolean;
}

export interface SeedOverrides {
  profile?: Partial<SeedProfileRequest>;
  ira?: Partial<SeedAccountRequest>;
  brokerage?: Partial<SeedAccountRequest>;
  spending?: Partial<SeedSpendingRequest>;
}

/**
 * Read the bearer token the UI login left in localStorage and build the
 * header set needed to call the API directly (bypassing the UI for setup
 * that would otherwise take many slow form-fills per test).
 */
export async function authHeaders(page: Page): Promise<Record<string, string>> {
  const token = await page.evaluate(key => localStorage.getItem(key), TOKEN_KEY);
  if (!token) throw new Error('No auth token found in localStorage — did registerAndLogin run?');
  return {
    Authorization: `Bearer ${token}`,
    'Content-Type': 'application/json',
  };
}

/**
 * Register + log in a fresh user, then seed a full baseline plan via the API:
 * a retirement profile (single, CA, born 1963 so the plan crosses Medicare
 * age 65 and RMD age 73 within the horizon), a large tax-deferred IRA plus a
 * taxable brokerage account, and one essential spending item with no income
 * source (forcing withdrawals — and the tax/RMD/IRMAA/ACA machinery — from
 * day one).
 *
 * Deliberately does NOT save assumptions — callers that need non-default
 * assumptions (Roth ceiling, withdrawal strategy, ACA benchmark, Medicare
 * Part B) do that through the /assumptions UI, matching how a real user
 * would drive that flow.
 */
export async function seedFullPlan(
  page: Page,
  request: APIRequestContext,
  overrides?: SeedOverrides,
): Promise<Credentials> {
  const creds = await registerAndLogin(page, request);
  const headers = await authHeaders(page);

  const profilePayload: SeedProfileRequest = {
    first_name: 'Taylor',
    last_name: 'Retiree',
    date_of_birth: '1963-01-01',
    marital_status: 'single',
    filing_status: 'single',
    state: 'CA',
    retirement_date: '2026-01-01',
    life_expectancy: 95,
    ...overrides?.profile,
  };
  const profileRes = await request.put('/api/profile', { headers, data: profilePayload });
  expect(
    profileRes.ok(),
    `seed profile failed: ${profileRes.status()} ${await profileRes.text()}`,
  ).toBeTruthy();

  const iraPayload: SeedAccountRequest = {
    name: 'Traditional IRA',
    category: 'tax_deferred',
    account_type: 'ira',
    owner: 'self',
    current_balance: 700_000,
    expected_roi: 5,
    dividend_yield: 0,
    ...overrides?.ira,
  };
  const iraRes = await request.post('/api/accounts', { headers, data: iraPayload });
  expect(
    iraRes.ok(),
    `seed IRA account failed: ${iraRes.status()} ${await iraRes.text()}`,
  ).toBeTruthy();

  const brokeragePayload: SeedAccountRequest = {
    name: 'Brokerage',
    category: 'taxable',
    account_type: 'brokerage',
    owner: 'self',
    current_balance: 300_000,
    expected_roi: 6,
    dividend_yield: 2,
    cost_basis: 150_000,
    ...overrides?.brokerage,
  };
  const brokerageRes = await request.post('/api/accounts', { headers, data: brokeragePayload });
  expect(
    brokerageRes.ok(),
    `seed brokerage account failed: ${brokerageRes.status()} ${await brokerageRes.text()}`,
  ).toBeTruthy();

  const spendingPayload: SeedSpendingRequest = {
    name: 'Living expenses',
    category: 'essential',
    amount: 70_000,
    frequency: 'annual',
    inflation_adjusted: true,
    ...overrides?.spending,
  };
  const spendingRes = await request.post('/api/spending', { headers, data: spendingPayload });
  expect(
    spendingRes.ok(),
    `seed spending failed: ${spendingRes.status()} ${await spendingRes.text()}`,
  ).toBeTruthy();

  return creds;
}

/** Strip `$`/`,`/unicode minus from a formatted currency string, e.g. "$1,234" -> 1234. */
export function parseCurrency(text: string): number {
  const cleaned = text.replace(/[$,]/g, '').replace(/−/g, '-').trim();
  if (cleaned === '' || cleaned === '—' || cleaned === '-') return 0;
  const n = Number(cleaned);
  return Number.isNaN(n) ? 0 : n;
}
