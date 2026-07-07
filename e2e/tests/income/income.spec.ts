import { test, expect } from '../../fixtures/base';
import { clearSession, registerAndLogin } from '../helpers/auth';

test.describe('Feature 5 — Income sources', () => {
  test.beforeEach(async ({ page, request }) => {
    await registerAndLogin(page, request);
    await page.goto('/income');
    await expect(page.getByRole('heading', { name: 'Income sources', level: 1 })).toBeVisible({
      timeout: 10_000,
    });
  });

  test('a new user sees the empty state', async ({ page }) => {
    await expect(page.getByText('No income sources yet.')).toBeVisible();
    await expect(page.getByRole('button', { name: 'Add income' })).toBeVisible();
  });

  test('adding monthly Social Security shows it with an annualized amount', async ({ page }) => {
    await page.getByRole('button', { name: 'Add income' }).click();

    await page.getByLabel('Name').fill('Social Security');
    await page.getByLabel('Type').selectOption('social_security');
    await page.getByLabel('Owner').selectOption('self');
    await page.getByLabel('Taxability').selectOption('partially_taxable');
    await page.getByLabel('Amount ($)').fill('2000');
    await page.getByLabel('Frequency').selectOption('monthly');
    await page.getByLabel('Start date').fill('2030-01-01');

    await page.getByRole('button', { name: 'Add income' }).click();

    // $2,000/month annualizes to $24,000 in the row's headline figure.
    await expect(page.locator('.account-name')).toHaveText('Social Security', { timeout: 10_000 });
    await expect(page.locator('.account-balance')).toHaveText('$24,000');
    await expect(page.getByText('No income sources yet.')).toBeHidden();
  });

  test('COLA and an end date are reflected in the row', async ({ page }) => {
    await page.getByRole('button', { name: 'Add income' }).click();

    await page.getByLabel('Name').fill('Consulting gig');
    await page.getByLabel('Type').selectOption('consulting');
    await page.getByLabel('Amount ($)').fill('40000');
    await page.getByLabel('Frequency').selectOption('annual');
    await page.getByLabel('Start date').fill('2026-01-01');
    await page.getByLabel('End date (optional)').fill('2029-12-31');
    await page.getByText(/cost-of-living adjustment/i).click(); // toggle COLA on

    await page.getByRole('button', { name: 'Add income' }).click();

    await expect(page.locator('.account-name')).toHaveText('Consulting gig', { timeout: 10_000 });
    await expect(page.locator('.account-balance')).toHaveText('$40,000');
    await expect(page.getByText(/COLA/)).toBeVisible();
    await expect(page.getByText(/to 2029-12-31/)).toBeVisible();
  });

  test('editing an income source updates its amount', async ({ page }) => {
    await page.getByRole('button', { name: 'Add income' }).click();
    await page.getByLabel('Name').fill('Pension');
    await page.getByLabel('Type').selectOption('pension');
    await page.getByLabel('Amount ($)').fill('1000');
    await page.getByLabel('Frequency').selectOption('monthly');
    await page.getByLabel('Start date').fill('2030-06-01');
    await page.getByRole('button', { name: 'Add income' }).click();
    await expect(page.locator('.account-balance')).toHaveText('$12,000', { timeout: 10_000 });

    await page.getByRole('button', { name: 'Edit' }).click();
    await page.getByLabel('Amount ($)').fill('1500');
    await page.getByRole('button', { name: 'Save changes' }).click();

    await expect(page.locator('.account-balance')).toHaveText('$18,000', { timeout: 10_000 });
  });

  test('deleting an income source returns to the empty state', async ({ page }) => {
    await page.getByRole('button', { name: 'Add income' }).click();
    await page.getByLabel('Name').fill('Part-time job');
    await page.getByLabel('Type').selectOption('part_time');
    await page.getByLabel('Amount ($)').fill('500');
    await page.getByLabel('Frequency').selectOption('monthly');
    await page.getByLabel('Start date').fill('2026-01-01');
    await page.getByRole('button', { name: 'Add income' }).click();
    await expect(page.getByText('Part-time job')).toBeVisible({ timeout: 10_000 });

    page.once('dialog', dialog => dialog.accept());
    await page.getByRole('button', { name: 'Delete' }).click();

    await expect(page.getByText('Part-time job')).toBeHidden({ timeout: 10_000 });
    await expect(page.getByText('No income sources yet.')).toBeVisible();
  });

  test('an added income source persists across a reload', async ({ page }) => {
    await page.getByRole('button', { name: 'Add income' }).click();
    await page.getByLabel('Name').fill('Rental property');
    await page.getByLabel('Type').selectOption('rental');
    await page.getByLabel('Amount ($)').fill('1800');
    await page.getByLabel('Frequency').selectOption('monthly');
    await page.getByLabel('Start date').fill('2026-01-01');
    await page.getByRole('button', { name: 'Add income' }).click();
    await expect(page.locator('.account-name')).toHaveText('Rental property', { timeout: 10_000 });

    await page.reload();
    await expect(page.locator('.account-name')).toHaveText('Rental property', { timeout: 10_000 });
    await expect(page.locator('.account-balance')).toHaveText('$21,600');
  });
});

test.describe('Feature 5 — Income sources: access control', () => {
  test('unauthenticated visit redirects to /login', async ({ page }) => {
    await page.goto('/');
    await clearSession(page);
    await page.goto('/income');
    await expect(page).toHaveURL(/\/login/, { timeout: 10_000 });
  });
});
