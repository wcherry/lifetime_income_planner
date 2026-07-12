import { test, expect } from '../../fixtures/base';
import { registerAndLogin } from '../helpers/auth';
import { seedFullPlan } from '../helpers/seed';

// A small, realistic bank-export CSV: standard `Date,Description,Amount`
// header, one income row (positive) and several expense rows (negative),
// matching the tolerant-parser contract in
// backend/src/models/spending_tracker.rs.
const SAMPLE_CSV = [
  'Date,Description,Amount',
  '2026-01-03,Paycheck,3000.00',
  '2026-01-05,Grocery Store,-150.25',
  '2026-01-10,Electric Bill,-95.00',
  '2026-01-15,Coffee Shop,-4.50',
].join('\n');

test.describe('Spending Tracker', () => {
  test.beforeEach(async ({ page, request }) => {
    await registerAndLogin(page, request);
  });

  test('a user navigates via the main nav and sees an empty state', async ({ page }) => {
    await page.getByRole('link', { name: 'Spending Tracker' }).click();

    await expect(page).toHaveURL(/\/spending-tracker/);
    await expect(page.getByRole('heading', { name: 'Spending Tracker', level: 1 })).toBeVisible({
      timeout: 10_000,
    });
    await expect(page.getByText(/no transactions/i)).toBeVisible();
  });

  test('uploading a CSV imports transactions into the chosen month', async ({ page }) => {
    await page.goto('/spending-tracker');
    await expect(page.getByRole('heading', { name: 'Spending Tracker', level: 1 })).toBeVisible({
      timeout: 10_000,
    });

    await page.getByLabel('Year').fill('2026');
    await page.getByLabel('Month').selectOption('1');

    await page.getByRole('button', { name: 'Import transaction' }).click();
    await page
      .getByLabel('CSV file')
      .setInputFiles({
        name: 'january-statement.csv',
        mimeType: 'text/csv',
        buffer: Buffer.from(SAMPLE_CSV),
      });
    await page.getByRole('button', { name: 'Import transactions' }).click();

    await expect(page.getByText('4 transactions imported')).toBeVisible({ timeout: 10_000 });
    await expect(page.getByText('Paycheck')).toBeVisible();
    await expect(page.getByText('Grocery Store')).toBeVisible();
    await expect(page.getByText('Electric Bill')).toBeVisible();
    await expect(page.getByText('Coffee Shop')).toBeVisible();
  });

  test('creating a custom category and assigning it to a transaction persists across a reload', async ({
    page,
  }) => {
    await page.goto('/spending-tracker');
    await expect(page.getByRole('heading', { name: 'Spending Tracker', level: 1 })).toBeVisible({
      timeout: 10_000,
    });

    // Import a month of transactions to categorize.
    await page.getByLabel('Year').fill('2026');
    await page.getByLabel('Month').selectOption('1');
    await page.getByRole('button', { name: 'Import transaction' }).click();
    await page
      .getByLabel('CSV file')
      .setInputFiles({
        name: 'january-statement.csv',
        mimeType: 'text/csv',
        buffer: Buffer.from(SAMPLE_CSV),
      });
    await page.getByRole('button', { name: 'Import transactions' }).click();
    await expect(page.getByText('4 transactions imported')).toBeVisible({ timeout: 10_000 });
    await page.getByRole('button', { name: 'Done' }).click();
    await expect(page.getByText('Coffee Shop')).toBeVisible({ timeout: 10_000 });

    // Create a custom category.
    await page.getByRole('button', { name: 'Categories' }).click();
    await page.getByRole('button', { name: 'Add category' }).click();
    await page.getByLabel('Name').fill('Coffee & Cafes');
    await page.getByLabel('Kind').selectOption('expense');
    await page.getByRole('button', { name: 'Save category' }).click();
    // .first() picks the category-list entry inside the modal: every
    // transaction row's category <select> also gets an <option> for the new
    // category, so a plain text match is ambiguous in strict mode.
    await expect(page.getByText('Coffee & Cafes').first()).toBeVisible({ timeout: 10_000 });
    await page.getByRole('button', { name: 'Close' }).click();

    // Assign it to the imported "Coffee Shop" transaction.
    await page.getByLabel('Category for Coffee Shop').selectOption({ label: 'Coffee & Cafes' });
    const assignedOption = page.getByLabel('Category for Coffee Shop').locator('option:checked');
    await expect(assignedOption).toHaveText('Coffee & Cafes');

    // Reload and re-select the month — the assignment should have persisted.
    await page.reload();
    await page.getByLabel('Year').fill('2026');
    await page.getByLabel('Month').selectOption('1');
    await expect(page.getByText('Coffee Shop')).toBeVisible({ timeout: 10_000 });
    await expect(page.getByLabel('Category for Coffee Shop')).toHaveValue(/.+/);
    const selectedOption = page
      .getByLabel('Category for Coffee Shop')
      .locator('option:checked');
    await expect(selectedOption).toHaveText('Coffee & Cafes');
  });
});

test.describe('Spending Tracker — Quarterly Review integration', () => {
  test('opening the tracker from Quarterly Review and using its totals pre-fills the review form', async ({
    page,
    request,
  }) => {
    // seedFullPlan gives a profile + accounts + spending so the current
    // year has at least one quarter due for review.
    await seedFullPlan(page, request);

    await page.goto('/quarterly-review');
    await expect(page.getByRole('heading', { name: 'Quarterly review', level: 1 })).toBeVisible({
      timeout: 10_000,
    });

    await page.getByRole('button', { name: 'Review' }).first().click();
    await expect(page.getByLabel('Actual income')).toBeVisible({ timeout: 10_000 });

    await page.getByRole('button', { name: 'Open Spending Tracker for this quarter' }).click();

    await expect(page).toHaveURL(/\/spending-tracker\?.*scopeQuarter=/);
    // Scoped to the heading specifically — "Exit quarter scope" (the
    // banner's own dismiss button) also matches a plain /quarter scope/i
    // text query, which is ambiguous in strict mode.
    await expect(page.getByRole('heading', { name: /quarter scope/i })).toBeVisible({
      timeout: 10_000,
    });

    await page.getByRole('button', { name: 'Use these totals in Review' }).click();

    await expect(page).toHaveURL(/\/quarterly-review/);
    await expect(page.getByText(/pre-filled from the Spending Tracker/i)).toBeVisible({
      timeout: 10_000,
    });
    await expect(page.getByLabel('Actual income')).not.toHaveValue('');
    await expect(page.getByLabel('Actual spending')).not.toHaveValue('');

    // The fill params should be stripped from the URL after applying, so a
    // refresh doesn't silently re-fill the form.
    await expect(page).not.toHaveURL(/fillIncome=/);
  });
});
