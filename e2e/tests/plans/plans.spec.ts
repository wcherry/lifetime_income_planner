import { test, expect } from '../../fixtures/base';
import { clearSession } from '../helpers/auth';
import { seedFullPlan } from '../helpers/seed';

// The baseline seed (profile + a tax-deferred IRA + a taxable brokerage +
// one spending item, no assumptions saved) reads as this summary line — see
// frontend/src/data/plans.ts's planSummary().
const BASELINE_SUMMARY = 'Profile · 2 accounts · 1 spending item';

test.describe('Phase 4 — Saved plan lifecycle', () => {
  test.beforeEach(async ({ page, request }) => {
    // seedFullPlan populates the user's *working* profile/accounts/spending
    // via the API — it never saves a Plan snapshot, so every test still
    // starts from an empty saved-plans list.
    await seedFullPlan(page, request);
    await page.goto('/plans');
    await expect(page.getByRole('heading', { name: 'Saved plans', level: 1 })).toBeVisible({
      timeout: 10_000,
    });
  });

  test('a new user sees the empty state', async ({ page }) => {
    await expect(page.getByText('No saved plans yet.')).toBeVisible();
  });

  test('saving a plan shows it with the correct name, summary, and saved date', async ({
    page,
  }) => {
    await page.getByLabel('Plan name').fill('Baseline');
    await page.getByRole('button', { name: 'Save as new plan' }).click();

    await expect(page.getByText('Baseline', { exact: true })).toBeVisible({ timeout: 10_000 });
    await expect(page.getByText(BASELINE_SUMMARY)).toBeVisible();
    await expect(page.locator('.account-meta', { hasText: /^Saved / })).toBeVisible();
    await expect(page.getByText('No saved plans yet.')).toBeHidden();
  });

  test('loading a plan shows a success notice', async ({ page }) => {
    await page.getByLabel('Plan name').fill('Baseline');
    await page.getByRole('button', { name: 'Save as new plan' }).click();
    await expect(page.getByText('Baseline', { exact: true })).toBeVisible({ timeout: 10_000 });

    page.once('dialog', (dialog) => dialog.accept());
    await page.getByRole('button', { name: 'Load' }).click();

    await expect(page.getByText('Loaded "Baseline" into your working plan.')).toBeVisible({
      timeout: 10_000,
    });
  });

  test('cloning a plan creates a new row noting its parent', async ({ page }) => {
    await page.getByLabel('Plan name').fill('Baseline');
    await page.getByRole('button', { name: 'Save as new plan' }).click();
    await expect(page.getByText('Baseline', { exact: true })).toBeVisible({ timeout: 10_000 });

    page.once('dialog', (dialog) => dialog.accept('Baseline clone'));
    await page.getByRole('button', { name: 'Clone' }).click();

    await expect(page.getByText('Baseline clone', { exact: true })).toBeVisible({
      timeout: 10_000,
    });
    await expect(page.getByText('Cloned from Baseline')).toBeVisible();
  });

  test('renaming a plan updates its name', async ({ page }) => {
    await page.getByLabel('Plan name').fill('Baseline');
    await page.getByRole('button', { name: 'Save as new plan' }).click();
    await expect(page.getByText('Baseline', { exact: true })).toBeVisible({ timeout: 10_000 });

    page.once('dialog', (dialog) => dialog.accept('Retire early'));
    await page.getByRole('button', { name: 'Rename' }).click();

    await expect(page.getByText('Retire early', { exact: true })).toBeVisible({
      timeout: 10_000,
    });
    await expect(page.getByText('Baseline', { exact: true })).toBeHidden();
  });

  test('updating a snapshot then restoring a version from history shows success notices', async ({
    page,
  }) => {
    await page.getByLabel('Plan name').fill('Baseline');
    await page.getByRole('button', { name: 'Save as new plan' }).click();
    await expect(page.getByText('Baseline', { exact: true })).toBeVisible({ timeout: 10_000 });

    page.once('dialog', (dialog) => dialog.accept());
    await page.getByRole('button', { name: 'Update' }).click();
    await expect(page.getByText('Updated "Baseline" with your current data.')).toBeVisible({
      timeout: 10_000,
    });

    await page.getByRole('button', { name: 'History' }).click();
    const restoreButton = page.getByRole('button', { name: 'Restore' });
    await expect(restoreButton).toBeVisible({ timeout: 10_000 });

    page.once('dialog', (dialog) => dialog.accept());
    await restoreButton.click();

    await expect(page.getByText(/Restored "Baseline" to its .+ version\./)).toBeVisible({
      timeout: 10_000,
    });
  });

  test('deleting a plan removes it from the list', async ({ page }) => {
    await page.getByLabel('Plan name').fill('Baseline');
    await page.getByRole('button', { name: 'Save as new plan' }).click();
    await expect(page.getByText('Baseline', { exact: true })).toBeVisible({ timeout: 10_000 });

    page.once('dialog', (dialog) => dialog.accept());
    await page.getByRole('button', { name: 'Delete' }).click();

    await expect(page.getByText('Baseline', { exact: true })).toBeHidden({ timeout: 10_000 });
    await expect(page.getByText('No saved plans yet.')).toBeVisible();
  });
});

test.describe('Phase 4 — Saved plans: access control', () => {
  test('unauthenticated visit redirects to /login', async ({ page }) => {
    await page.goto('/');
    await clearSession(page);
    await page.goto('/plans');
    await expect(page).toHaveURL(/\/login/, { timeout: 10_000 });
  });
});
