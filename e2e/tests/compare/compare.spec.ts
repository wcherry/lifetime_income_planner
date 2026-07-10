import { test, expect } from '../../fixtures/base';
import { clearSession } from '../helpers/auth';
import { seedFullPlan, authHeaders } from '../helpers/seed';

test.describe('Phase 4 — Scenario comparison', () => {
  test('fewer than two saved plans shows a message and a link, no compare controls', async ({
    page,
    request,
  }) => {
    await seedFullPlan(page, request);
    await page.goto('/compare');
    await expect(page.getByRole('heading', { name: 'Compare scenarios', level: 1 })).toBeVisible({
      timeout: 10_000,
    });

    await expect(page.getByText(/Save at least 2 plans/)).toBeVisible();
    await expect(page.getByRole('link', { name: 'Saved plans' })).toBeVisible();
    await expect(page.getByRole('button', { name: /Compare \d+ scenario/ })).toHaveCount(0);
  });

  test('comparing two distinct saved plans renders a full metrics table', async ({
    page,
    request,
  }) => {
    await seedFullPlan(page, request);
    const headers = await authHeaders(page);

    const plan1Res = await request.post('/api/plans', { headers, data: { name: 'Plan A' } });
    expect(plan1Res.ok(), `save Plan A failed: ${plan1Res.status()}`).toBeTruthy();

    // Change the working data before saving the second plan so the two
    // scenarios are genuinely different, not identical snapshots.
    const spendingRes = await request.post('/api/spending', {
      headers,
      data: {
        name: 'Travel',
        category: 'discretionary',
        amount: 15_000,
        frequency: 'annual',
        inflation_adjusted: true,
      },
    });
    expect(spendingRes.ok(), `seed extra spending failed: ${spendingRes.status()}`).toBeTruthy();

    const plan2Res = await request.post('/api/plans', { headers, data: { name: 'Plan B' } });
    expect(plan2Res.ok(), `save Plan B failed: ${plan2Res.status()}`).toBeTruthy();

    await page.goto('/compare');
    await expect(page.getByRole('heading', { name: 'Compare scenarios', level: 1 })).toBeVisible({
      timeout: 10_000,
    });

    await page.locator('.compare-row', { hasText: 'Plan A' }).getByRole('checkbox').check();
    await page.locator('.compare-row', { hasText: 'Plan B' }).getByRole('checkbox').check();

    await page.getByRole('button', { name: 'Compare 2 scenarios' }).click();

    const table = page.locator('table.compare-table');
    await expect(table).toBeVisible({ timeout: 15_000 });

    for (const label of [
      'Net worth today',
      'Estate (ending balance)',
      'Lifetime taxes',
      'Lifetime ACA subsidies',
      'Lifetime RMD',
      'Lifetime spending',
      'Age money depleted',
    ]) {
      await expect(table.getByRole('cell', { name: label, exact: true })).toBeVisible();
    }
    await expect(table.getByRole('columnheader', { name: 'Plan A', exact: true })).toBeVisible();
    await expect(table.getByRole('columnheader', { name: 'Plan B', exact: true })).toBeVisible();
  });
});

test.describe('Phase 4 — Compare scenarios: access control', () => {
  test('unauthenticated visit redirects to /login', async ({ page }) => {
    await page.goto('/');
    await clearSession(page);
    await page.goto('/compare');
    await expect(page).toHaveURL(/\/login/, { timeout: 10_000 });
  });
});
