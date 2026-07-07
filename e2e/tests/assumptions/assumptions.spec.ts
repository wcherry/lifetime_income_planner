import { test, expect } from '../../fixtures/base';
import { clearSession, registerAndLogin } from '../helpers/auth';

test.describe('Feature 7 — Inflation & ROI assumptions', () => {
  test.beforeEach(async ({ page, request }) => {
    await registerAndLogin(page, request);
    await page.goto('/assumptions');
    await expect(
      page.getByRole('heading', { name: 'Inflation & ROI assumptions', level: 2 }),
    ).toBeVisible({ timeout: 10_000 });
  });

  test('a new user sees default assumptions prefilled', async ({ page }) => {
    await expect(page.getByText(/Showing default assumptions/)).toBeVisible();
    await expect(page.getByLabel('General inflation (%)')).toHaveValue('2.5');
    await expect(page.getByLabel('Investment return (%)')).toHaveValue('6');
    await expect(page.getByLabel('Healthcare inflation (%)')).toHaveValue('4.5');
    await expect(page.getByLabel('Social Security COLA (%)')).toHaveValue('2');
  });

  test('saving custom assumptions persists across a reload', async ({ page }) => {
    await page.getByLabel('General inflation (%)').fill('3.2');
    await page.getByLabel('Investment return (%)').fill('5.5');
    await page.getByLabel('Healthcare inflation (%)').fill('5');
    await page.getByLabel('Social Security COLA (%)').fill('1.8');

    await page.getByRole('button', { name: 'Save assumptions' }).click();
    await expect(page.getByText('Assumptions saved.')).toBeVisible({ timeout: 10_000 });

    await page.reload();
    await expect(page.getByLabel('General inflation (%)')).toHaveValue('3.2', {
      timeout: 10_000,
    });
    await expect(page.getByLabel('Investment return (%)')).toHaveValue('5.5');
    // Once saved, the "using defaults" banner is gone.
    await expect(page.getByText(/Showing default assumptions/)).toBeHidden();
  });

  test('an out-of-range value is rejected', async ({ page }) => {
    await page.getByLabel('Investment return (%)').fill('99');
    await page.getByRole('button', { name: 'Save assumptions' }).click();
    await expect(page.getByText(/must be between -20 and 30/)).toBeVisible({ timeout: 10_000 });
  });
});

test.describe('Feature 7 — Assumptions: access control', () => {
  test('unauthenticated visit redirects to /login', async ({ page }) => {
    await page.goto('/');
    await clearSession(page);
    await page.goto('/assumptions');
    await expect(page).toHaveURL(/\/login/, { timeout: 10_000 });
  });
});
