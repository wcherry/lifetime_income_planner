import { test, expect } from '../../fixtures/base';
import { seedFullPlan, parseCurrency } from '../helpers/seed';
import { collapsibleCard, openCard } from '../helpers/cards';

test.describe('Phase 4 — Interactive scenario tools', () => {
  test.beforeEach(async ({ page, request }) => {
    await seedFullPlan(page, request);
    await page.goto('/projection');
    await expect(page.getByRole('heading', { name: 'Projection', level: 1 })).toBeVisible({
      timeout: 10_000,
    });
  });

  test('what-if: adjusting a slider shows comparison tiles, and reset hides them', async ({
    page,
  }) => {
    // What-if defaults to closed (Card defaultOpen={false}).
    await openCard(page, 'What-if');
    const card = collapsibleCard(page, 'What-if');

    await card.getByLabel('Investment return').fill('5');
    // The what-if request is debounced 400ms after the last slider change.
    await page.waitForTimeout(700);

    const compareTiles = card.locator('.what-if-compare-tile');
    await expect(compareTiles.first()).toBeVisible({ timeout: 10_000 });
    await expect(card.getByText('Estate', { exact: true })).toBeVisible();
    await expect(card.getByText('Lifetime taxes', { exact: true })).toBeVisible();
    await expect(card.getByText('Lifetime withdrawals', { exact: true })).toBeVisible();
    await expect(card.getByText('Money lasts', { exact: true })).toBeVisible();

    await card.getByRole('button', { name: 'Reset to baseline' }).click();
    await expect(compareTiles).toHaveCount(0);
  });

  test('optimize: finding the best strategy shows a recommended row that can be applied', async ({
    page,
  }) => {
    // Optimize defaults to closed (Card defaultOpen={false}).
    await openCard(page, 'Optimize');
    const card = collapsibleCard(page, 'Optimize');

    // Default goal "Minimize lifetime taxes" is fine — just run it.
    await card.getByRole('button', { name: 'Find best strategy' }).click();

    const resultTable = card.locator('table.compare-table');
    await expect(resultTable).toBeVisible({ timeout: 15_000 });
    const goodRow = resultTable.locator('tr.row-good');
    await expect(goodRow).toBeVisible();
    const applyButton = goodRow.getByRole('button', { name: 'Apply to my plan' });
    await expect(applyButton).toBeVisible();

    page.once('dialog', (dialog) => dialog.accept());
    await applyButton.click();

    await expect(card.getByText('Applied to your saved assumptions.')).toBeVisible({
      timeout: 10_000,
    });
  });

  test('monte carlo: running the default simulation shows a coherent success summary', async ({
    page,
  }) => {
    // 1,000 server-side simulations is slower than a normal round trip —
    // give this whole test more headroom than the suite default (30s).
    test.setTimeout(60_000);

    const card = collapsibleCard(page, 'Monte Carlo simulation');
    await expect(card).toBeVisible();

    await card.getByRole('button', { name: /Run 1,000 simulations/ }).click();

    await expect(card.getByText('Success rate', { exact: true })).toBeVisible({
      timeout: 45_000,
    });
    await expect(card.getByText('Median ending balance', { exact: true })).toBeVisible();
    await expect(card.getByText('Best case', { exact: true })).toBeVisible();
    await expect(card.getByText('Worst case', { exact: true })).toBeVisible();

    const tileValue = async (label: string) => {
      const tile = card.locator('.tile', { hasText: label });
      const text = (await tile.locator('.tile-value').textContent()) ?? '';
      return parseCurrency(text);
    };
    const best = await tileValue('Best case');
    const median = await tileValue('Median ending balance');
    const worst = await tileValue('Worst case');

    expect(best).toBeGreaterThanOrEqual(median);
    expect(median).toBeGreaterThanOrEqual(worst);
  });
});
