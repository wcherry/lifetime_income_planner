import { test, expect } from '../../fixtures/base';
import { seedFullPlan, parseCurrency } from '../helpers/seed';
import { collapsibleCard, staticCard } from '../helpers/cards';

test.describe('Phase 3 — Healthcare & regulatory', () => {
  test.beforeEach(async ({ page, request }) => {
    // A much bigger IRA than the baseline seed: the ~$106k single-filer
    // IRMAA threshold (see backend/src/irmaa.rs) inflates every year from
    // its 2025 base at the same general-inflation rate as this household's
    // MAGI grows from spending-driven withdrawals, so a $700k-$2M IRA's MAGI
    // never actually overtakes it within the plan horizon — confirmed by
    // probing /api/projection directly. RMDs alone (age 73+) push a $5M IRA's
    // ordinary income well past the threshold by ~2040, which also
    // comfortably exceeds that year's spending for the RMD warning badge.
    await seedFullPlan(page, request, { ira: { current_balance: 5_000_000 } });
    await page.goto('/projection');
    await expect(page.getByRole('heading', { name: 'Projection', level: 1 })).toBeVisible({
      timeout: 10_000,
    });
  });

  test('an ACA benchmark premium renders the subsidy card without erroring', async ({ page }) => {
    await page.goto('/assumptions');
    await expect(
      page.getByRole('heading', { name: 'Inflation & ROI assumptions', level: 2 }),
    ).toBeVisible({ timeout: 10_000 });
    await page.getByLabel('Benchmark silver premium ($/yr)').fill('12000');
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

    const card = staticCard(page, /ACA health insurance subsidy/);
    await expect(card).toBeVisible();
    // Either the household is eligible (a `dl` of MAGI/FPL/subsidy lines) or
    // it isn't (a short explanatory note) — either is a valid render, this
    // just asserts the card resolved to one of them rather than erroring.
    const eligibleLines = card.locator('dl.aca-lines');
    const notEligibleNote = card.locator('p.muted.center');
    await expect(eligibleLines.or(notEligibleNote).first()).toBeVisible();
  });

  test('Medicare Part B premiums show a table column and a positive lifetime tile', async ({
    page,
  }) => {
    // Medicare Part B defaults to a non-zero premium ($2,220/yr) and this
    // household's plan horizon (birth 1963, life expectancy 95) runs from
    // 2026 through 2058, well past age 65 — no assumption change needed.
    const tile = page.locator('.tile', { hasText: 'Medicare Part B' });
    await expect(tile).toBeVisible();
    const tileValue = parseCurrency((await tile.locator('.tile-value').textContent()) ?? '');
    expect(tileValue).toBeGreaterThan(0);

    const annualCard = collapsibleCard(page, 'Year-by-year projection');
    await expect(annualCard.getByRole('columnheader', { name: 'Medicare' })).toBeVisible();

    const nonDashCount = await annualCard.locator('table.proj-table').evaluate((table) => {
      const headers = Array.from(table.querySelectorAll('thead th')).map((th) =>
        th.textContent?.trim(),
      );
      const idx = headers.indexOf('Medicare');
      if (idx === -1) return -1;
      const rows = Array.from(table.querySelectorAll('tbody tr'));
      return rows.filter((r) => r.children[idx]?.textContent?.trim() !== '—').length;
    });
    expect(nonDashCount).toBeGreaterThan(0);
  });

  test('IRMAA surcharges are reflected as a positive lifetime tile, consistent with the current-year card', async ({
    page,
  }) => {
    // Confirmed by probing /api/projection directly against this exact seed:
    // this $5M-IRA household's MAGI crosses the (also-inflating) IRMAA
    // threshold by ~2040, well within the plan horizon, so the lifetime tile
    // is always present here.
    const tile = page.locator('.tile', { hasText: 'IRMAA surcharges' });
    await expect(tile).toBeVisible();

    const tileValue = parseCurrency((await tile.locator('.tile-value').textContent()) ?? '');
    expect(tileValue).toBeGreaterThan(0);

    const card = staticCard(page, /Medicare IRMAA surcharge/);
    await expect(card).toBeVisible();

    // The card only ever describes the *current* (first) plan year, which
    // may not be the year(s) that actually drove the lifetime tile above
    // (IRMAA needs two prior years of MAGI, so it can never apply in the
    // plan's first two years) — assert whichever of the three valid states
    // rendered is internally consistent, not one specific state.
    const totalLine = card.locator('.tax-line-strong', { hasText: 'Total IRMAA surcharge' });
    if (await totalLine.count() > 0) {
      const value = parseCurrency((await totalLine.locator('dd').textContent()) ?? '');
      expect(value).toBeGreaterThanOrEqual(0);
    } else {
      await expect(
        card.getByText(/MAGI isn't modeled|pays only the standard premiums/),
      ).toBeVisible();
    }
  });

  test('a year whose RMD exceeds that year\'s spending shows the RMD warning badge', async ({
    page,
  }) => {
    const annualCard = collapsibleCard(page, 'Year-by-year projection');
    await expect(annualCard).toBeVisible();
    await expect(annualCard.locator('.rmd-badge').first()).toBeVisible({ timeout: 10_000 });
  });
});
