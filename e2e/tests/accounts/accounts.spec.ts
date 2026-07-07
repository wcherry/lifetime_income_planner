import { test, expect } from '../../fixtures/base';
import { clearSession, registerAndLogin } from '../helpers/auth';

test.describe('Feature 3 — Account management', () => {
  test.beforeEach(async ({ page, request }) => {
    await registerAndLogin(page, request);
    await page.goto('/accounts');
    await expect(page.getByRole('heading', { name: 'Accounts', level: 1 })).toBeVisible({
      timeout: 10_000,
    });
  });

  test('a new user sees the empty state', async ({ page }) => {
    await expect(page.getByText('No accounts yet.')).toBeVisible();
    await expect(page.getByRole('button', { name: 'Add account' })).toBeVisible();
  });

  test('adding a taxable brokerage account shows it in the list and total', async ({ page }) => {
    await page.getByRole('button', { name: 'Add account' }).click();

    await page.getByLabel('Account name').fill('Fidelity Brokerage');
    await page.getByLabel('Tax category').selectOption('taxable');
    await page.getByLabel('Account type').selectOption('brokerage');
    await page.getByLabel('Owner').selectOption('self');
    await page.getByLabel('Current balance ($)').fill('250000');
    await page.getByLabel('Expected ROI (%)').fill('6.5');

    await page.getByRole('button', { name: 'Add account' }).click();

    await expect(page.getByText('Fidelity Brokerage')).toBeVisible({ timeout: 10_000 });
    await expect(page.locator('.account-balance')).toHaveText('$250,000');
    await expect(page.getByText(/1 account\b/)).toBeVisible();
    await expect(page.getByText('No accounts yet.')).toBeHidden();
  });

  test('adding a tax-deferred IRA — category drives the available types', async ({ page }) => {
    await page.getByRole('button', { name: 'Add account' }).click();

    await page.getByLabel('Account name').fill('Vanguard IRA');
    // Switching category re-populates the type dropdown; "Traditional IRA" is a
    // tax-deferred type, so it only exists once the category is set.
    await page.getByLabel('Tax category').selectOption('tax_deferred');
    await page.getByLabel('Account type').selectOption('ira');
    await page.getByLabel('Current balance ($)').fill('500000');
    await page.getByLabel('Expected ROI (%)').fill('5');

    await page.getByRole('button', { name: 'Add account' }).click();

    await expect(page.getByText('Vanguard IRA')).toBeVisible({ timeout: 10_000 });
    await expect(page.getByText(/Traditional IRA/)).toBeVisible();
  });

  test('editing an account updates its balance', async ({ page }) => {
    await page.getByRole('button', { name: 'Add account' }).click();
    await page.getByLabel('Account name').fill('Savings');
    await page.getByLabel('Current balance ($)').fill('10000');
    await page.getByLabel('Expected ROI (%)').fill('1');
    await page.getByRole('button', { name: 'Add account' }).click();
    await expect(page.getByText('Savings')).toBeVisible({ timeout: 10_000 });

    await page.getByRole('button', { name: 'Edit' }).click();
    await page.getByLabel('Current balance ($)').fill('42000');
    await page.getByRole('button', { name: 'Save changes' }).click();

    await expect(page.locator('.account-balance')).toHaveText('$42,000', { timeout: 10_000 });
    await expect(page.getByText('$10,000')).toBeHidden();
  });

  test('deleting an account returns to the empty state', async ({ page }) => {
    await page.getByRole('button', { name: 'Add account' }).click();
    await page.getByLabel('Account name').fill('Checking');
    await page.getByLabel('Current balance ($)').fill('5000');
    await page.getByLabel('Expected ROI (%)').fill('0.5');
    await page.getByRole('button', { name: 'Add account' }).click();
    await expect(page.getByText('Checking')).toBeVisible({ timeout: 10_000 });

    // Delete triggers a window.confirm() — auto-accept it.
    page.once('dialog', dialog => dialog.accept());
    await page.getByRole('button', { name: 'Delete' }).click();

    await expect(page.getByText('Checking')).toBeHidden({ timeout: 10_000 });
    await expect(page.getByText('No accounts yet.')).toBeVisible();
  });

  test('an added account persists across a reload', async ({ page }) => {
    await page.getByRole('button', { name: 'Add account' }).click();
    await page.getByLabel('Account name').fill('Roth IRA');
    await page.getByLabel('Tax category').selectOption('tax_free');
    await page.getByLabel('Account type').selectOption('roth_ira');
    await page.getByLabel('Current balance ($)').fill('120000');
    await page.getByLabel('Expected ROI (%)').fill('7');
    await page.getByRole('button', { name: 'Add account' }).click();
    // "Roth IRA" is both the account name and the type label, so scope to the name.
    await expect(page.locator('.account-name')).toHaveText('Roth IRA', { timeout: 10_000 });

    await page.reload();
    await expect(page.locator('.account-name')).toHaveText('Roth IRA', { timeout: 10_000 });
    await expect(page.locator('.account-balance')).toHaveText('$120,000');
  });
});

test.describe('Feature 3 — Account management: access control', () => {
  test('unauthenticated visit redirects to /login', async ({ page }) => {
    await page.goto('/');
    await clearSession(page);
    await page.goto('/accounts');
    await expect(page).toHaveURL(/\/login/, { timeout: 10_000 });
  });
});
