import { test, expect } from '../../fixtures/base';
import { clearSession, registerAndLogin } from '../helpers/auth';

test.describe('Feature 4 — Spending assumptions', () => {
  test.beforeEach(async ({ page, request }) => {
    await registerAndLogin(page, request);
    await page.goto('/spending');
    await expect(page.getByRole('heading', { name: 'Spending plan', level: 1 })).toBeVisible({
      timeout: 10_000,
    });
  });

  test('a new user sees the empty state', async ({ page }) => {
    await expect(page.getByText('No expenses yet.')).toBeVisible();
    await expect(page.getByRole('button', { name: 'Add expense' })).toBeVisible();
  });

  test('adding a monthly essential expense shows it with an annualized total', async ({ page }) => {
    await page.getByRole('button', { name: 'Add expense' }).click();

    await page.getByLabel('Description').fill('Groceries');
    await page.getByLabel('Category').selectOption('essential');
    await page.getByLabel('Amount ($)').fill('800');
    await page.getByLabel('Frequency').selectOption('monthly');

    await page.getByRole('button', { name: 'Add expense' }).click();

    await expect(page.locator('.account-name')).toHaveText('Groceries', { timeout: 10_000 });
    await expect(page.locator('.account-balance')).toHaveText('$800');
    // $800/month annualizes to $9,600/yr in the row meta and header total.
    await expect(page.getByText('$9,600/yr').first()).toBeVisible();
    await expect(page.getByText('No expenses yet.')).toBeHidden();
  });

  test('a one-time expense is labelled as one-time', async ({ page }) => {
    await page.getByRole('button', { name: 'Add expense' }).click();

    await page.getByLabel('Description').fill('New roof');
    await page.getByLabel('Category').selectOption('home_maintenance');
    await page.getByLabel('Amount ($)').fill('18000');
    await page.getByLabel('Frequency').selectOption('one_time');

    await page.getByRole('button', { name: 'Add expense' }).click();

    await expect(page.locator('.account-name')).toHaveText('New roof', { timeout: 10_000 });
    await expect(page.getByText('one-time', { exact: true })).toBeVisible();
    // One-time expenses are excluded from the recurring total → stays at $0/yr.
    await expect(page.getByText('$0/yr recurring')).toBeVisible();
  });

  test('editing an expense updates its amount', async ({ page }) => {
    await page.getByRole('button', { name: 'Add expense' }).click();
    await page.getByLabel('Description').fill('Travel');
    await page.getByLabel('Category').selectOption('travel');
    await page.getByLabel('Amount ($)').fill('500');
    await page.getByLabel('Frequency').selectOption('monthly');
    await page.getByRole('button', { name: 'Add expense' }).click();
    await expect(page.locator('.account-balance')).toHaveText('$500', { timeout: 10_000 });

    await page.getByRole('button', { name: 'Edit' }).click();
    await page.getByLabel('Amount ($)').fill('750');
    await page.getByRole('button', { name: 'Save changes' }).click();

    await expect(page.locator('.account-balance')).toHaveText('$750', { timeout: 10_000 });
  });

  test('deleting an expense returns to the empty state', async ({ page }) => {
    await page.getByRole('button', { name: 'Add expense' }).click();
    await page.getByLabel('Description').fill('Gym membership');
    await page.getByLabel('Amount ($)').fill('60');
    await page.getByLabel('Frequency').selectOption('monthly');
    await page.getByRole('button', { name: 'Add expense' }).click();
    await expect(page.getByText('Gym membership')).toBeVisible({ timeout: 10_000 });

    page.once('dialog', dialog => dialog.accept());
    await page.getByRole('button', { name: 'Delete' }).click();

    await expect(page.getByText('Gym membership')).toBeHidden({ timeout: 10_000 });
    await expect(page.getByText('No expenses yet.')).toBeVisible();
  });

  test('an added expense persists across a reload', async ({ page }) => {
    await page.getByRole('button', { name: 'Add expense' }).click();
    await page.getByLabel('Description').fill('Healthcare premiums');
    await page.getByLabel('Category').selectOption('healthcare');
    await page.getByLabel('Amount ($)').fill('1200');
    await page.getByLabel('Frequency').selectOption('monthly');
    await page.getByRole('button', { name: 'Add expense' }).click();
    await expect(page.locator('.account-name')).toHaveText('Healthcare premiums', {
      timeout: 10_000,
    });

    await page.reload();
    await expect(page.locator('.account-name')).toHaveText('Healthcare premiums', {
      timeout: 10_000,
    });
    await expect(page.locator('.account-balance')).toHaveText('$1,200');
  });
});

test.describe('Feature 4 — Spending assumptions: access control', () => {
  test('unauthenticated visit redirects to /login', async ({ page }) => {
    await page.goto('/');
    await clearSession(page);
    await page.goto('/spending');
    await expect(page).toHaveURL(/\/login/, { timeout: 10_000 });
  });
});
