import { test, expect } from '../../fixtures/base';
import { clearSession } from '../helpers/auth';
import { seedFullPlan, authHeaders, parseCurrency } from '../helpers/seed';
import { collapsibleCard } from '../helpers/cards';

test.describe('Phase 2 — Tax optimization', () => {
  test.beforeEach(async ({ page, request }) => {
    await seedFullPlan(page, request);
    await page.goto('/projection');
    await expect(page.getByRole('heading', { name: 'Projection', level: 1 })).toBeVisible({
      timeout: 10_000,
    });
  });

  test('tax breakdown card shows federal + state taxable-income buildups that reconcile to combined tax owed', async ({
    page,
  }) => {
    const card = collapsibleCard(page, /Tax breakdown/);
    await expect(card).toBeVisible();
    await expect(card.getByText('Federal taxable income')).toBeVisible();
    await expect(card.getByText('State taxable income')).toBeVisible();

    const row = card.locator('table.tax-compare tr', { hasText: 'Income tax' });
    const cells = row.locator('td.num');
    const federal = parseCurrency((await cells.nth(0).textContent()) ?? '');
    const state = parseCurrency((await cells.nth(1).textContent()) ?? '');
    const combined = parseCurrency((await cells.nth(2).textContent()) ?? '');

    // Rounding on each formatted cell can differ by at most a dollar or two.
    expect(Math.abs(combined - (federal + state))).toBeLessThanOrEqual(2);
  });

  test('the MAGI note is visible in the tax breakdown card', async ({ page }) => {
    const card = collapsibleCard(page, /Tax breakdown/);
    await expect(card.getByText(/Modified AGI \(MAGI\)/)).toBeVisible();
  });

  test('estimated quarterly taxes shows four installments, each with a due date', async ({
    page,
  }) => {
    // The seed has no income and $70k/yr of spending funded entirely by
    // withdrawals, so projected current-year tax is always > 0.
    const card = collapsibleCard(page, /Estimated quarterly taxes/);
    const tiles = card.locator('.estimate-card');
    await expect(tiles).toHaveCount(4);
    for (let i = 0; i < 4; i++) {
      await expect(tiles.nth(i).getByText(/^Due /)).toBeVisible();
    }
  });

  test('tax report renders a per-year table and its CSV export downloads', async ({ page }) => {
    const card = collapsibleCard(page, 'Tax report');
    const rows = card.locator('table.proj-table tbody tr');
    await expect(rows.first()).toBeVisible();
    expect(await rows.count()).toBeGreaterThan(0);

    const downloadPromise = page.waitForEvent('download');
    await card.getByRole('button', { name: 'Download CSV' }).click();
    const download = await downloadPromise;
    expect(download.suggestedFilename()).toBe('tax-summary.csv');
  });

  test('a Roth conversion ceiling surfaces a Roth conversions tile and an annual table column', async ({
    page,
    request,
  }) => {
    // Roth conversions only ever move money if a destination Roth (tax-free)
    // account exists (see backend/src/projection.rs's plan_roth_conversion,
    // which is a no-op without one) — the baseline seed intentionally has
    // none, so add one directly via the API before turning conversions on.
    const headers = await authHeaders(page);
    const rothRes = await request.post('/api/accounts', {
      headers,
      data: {
        name: 'Roth IRA',
        category: 'tax_free',
        account_type: 'roth_ira',
        owner: 'self',
        current_balance: 5_000,
        expected_roi: 5,
        dividend_yield: 0,
      },
    });
    expect(rothRes.ok(), `seed Roth account failed: ${rothRes.status()}`).toBeTruthy();

    await page.goto('/assumptions');
    await expect(
      page.getByRole('heading', { name: 'Inflation & ROI assumptions', level: 2 }),
    ).toBeVisible({ timeout: 10_000 });
    await page.getByLabel('Convert up to taxable income ($)').fill('60000');
    // Work around a pre-existing product bug: the default Medicare Part B
    // premium ($2,220/yr) isn't a multiple of that field's step="100", so the
    // browser's native HTML5 step-mismatch silently blocks the whole form's
    // submission (the <form> has no noValidate) unless it's touched into a
    // step-valid value first. Reported separately — see final test report.
    await page.getByLabel('Part B premium ($/yr)').fill('2200');
    await page.getByRole('button', { name: 'Save assumptions' }).click();
    await expect(page.getByText('Assumptions saved.')).toBeVisible({ timeout: 10_000 });

    await page.goto('/projection');
    await expect(page.getByRole('heading', { name: 'Projection', level: 1 })).toBeVisible({
      timeout: 10_000,
    });

    await expect(page.getByText('Roth conversions', { exact: true })).toBeVisible();
    const annualCard = collapsibleCard(page, 'Year-by-year projection');
    await expect(annualCard.getByRole('columnheader', { name: 'Roth conv.' })).toBeVisible();
  });

  test('switching to the tax-optimized withdrawal strategy adds an Order column to the tax report', async ({
    page,
  }) => {
    await page.goto('/assumptions');
    await expect(
      page.getByRole('heading', { name: 'Inflation & ROI assumptions', level: 2 }),
    ).toBeVisible({ timeout: 10_000 });
    await page.getByLabel('Strategy').selectOption('tax_optimized');
    // Same step-mismatch workaround as the Roth conversion test above.
    await page.getByLabel('Part B premium ($/yr)').fill('2200');
    await page.getByRole('button', { name: 'Save assumptions' }).click();
    await expect(page.getByText('Assumptions saved.')).toBeVisible({ timeout: 10_000 });

    await page.goto('/projection');
    await expect(page.getByRole('heading', { name: 'Projection', level: 1 })).toBeVisible({
      timeout: 10_000,
    });

    const reportCard = collapsibleCard(page, 'Tax report');
    await expect(reportCard.getByRole('columnheader', { name: 'Order' })).toBeVisible();
    const orderCells = reportCard.locator('tbody td', { hasText: /first$/ });
    await expect(orderCells.first()).toBeVisible();
  });
});

test.describe('Phase 2 — Projection: access control', () => {
  test('unauthenticated visit redirects to /login', async ({ page }) => {
    await page.goto('/');
    await clearSession(page);
    await page.goto('/projection');
    await expect(page).toHaveURL(/\/login/, { timeout: 10_000 });
  });
});
