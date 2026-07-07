import { test, expect } from '../../fixtures/base';
import { clearSession, registerAndLogin } from '../helpers/auth';

test.describe('Feature 6 — Life events', () => {
  test.beforeEach(async ({ page, request }) => {
    await registerAndLogin(page, request);
    await page.goto('/life-events');
    await expect(page.getByRole('heading', { name: 'Life events', level: 1 })).toBeVisible({
      timeout: 10_000,
    });
  });

  test('a new user sees the empty state', async ({ page }) => {
    await expect(page.getByText('No life events yet.')).toBeVisible();
    await expect(page.getByRole('button', { name: 'Add event' })).toBeVisible();
  });

  test('adding an inflow event shows a positive amount', async ({ page }) => {
    await page.getByRole('button', { name: 'Add event' }).click();

    await page.getByLabel('Name').fill('Sell the lake house');
    await page.getByLabel('Event type').selectOption('sell_house');
    await page.getByLabel('Event date', { exact: true }).fill('2032-06-01');
    await page.getByLabel('Direction').selectOption('inflow');
    await page.getByLabel('Amount ($)').fill('350000');

    await page.getByRole('button', { name: 'Add event' }).click();

    await expect(page.locator('.account-name')).toHaveText('Sell the lake house', {
      timeout: 10_000,
    });
    await expect(page.locator('.account-balance')).toHaveText('$350,000');
    await expect(page.getByText('No life events yet.')).toBeHidden();
  });

  test('an outflow event shows a negative amount', async ({ page }) => {
    await page.getByRole('button', { name: 'Add event' }).click();

    await page.getByLabel('Name').fill('Buy an RV');
    await page.getByLabel('Event type').selectOption('large_purchase');
    await page.getByLabel('Event date', { exact: true }).fill('2030-03-15');
    await page.getByLabel('Direction').selectOption('outflow');
    await page.getByLabel('Amount ($)').fill('120000');

    await page.getByRole('button', { name: 'Add event' }).click();

    await expect(page.locator('.account-name')).toHaveText('Buy an RV', { timeout: 10_000 });
    await expect(page.locator('.account-balance')).toHaveText('-$120,000');
  });

  test('a recurring event exposes a repeat-until date', async ({ page }) => {
    await page.getByRole('button', { name: 'Add event' }).click();

    await page.getByLabel('Name').fill('College gift');
    await page.getByLabel('Event type').selectOption('gift');
    await page.getByLabel('Event date', { exact: true }).fill('2028-09-01');
    await page.getByLabel('Direction').selectOption('outflow');
    await page.getByLabel('Amount ($)').fill('20000');
    await page.getByLabel('Repeat', { exact: true }).selectOption('annual');
    // The repeat-until field only appears once a recurrence is chosen.
    await page.getByLabel('Repeat until (optional)').fill('2031-09-01');

    await page.getByRole('button', { name: 'Add event' }).click();

    await expect(page.locator('.account-name')).toHaveText('College gift', { timeout: 10_000 });
    await expect(page.getByText(/Annual/)).toBeVisible();
    await expect(page.getByText(/to 2031-09-01/)).toBeVisible();
    await expect(page.getByText('per year')).toBeVisible();
  });

  test('editing an event updates its amount', async ({ page }) => {
    await page.getByRole('button', { name: 'Add event' }).click();
    await page.getByLabel('Name').fill('Inheritance');
    await page.getByLabel('Event type').selectOption('inheritance');
    await page.getByLabel('Event date', { exact: true }).fill('2035-01-01');
    await page.getByLabel('Direction').selectOption('inflow');
    await page.getByLabel('Amount ($)').fill('100000');
    await page.getByRole('button', { name: 'Add event' }).click();
    await expect(page.locator('.account-balance')).toHaveText('$100,000', { timeout: 10_000 });

    await page.getByRole('button', { name: 'Edit' }).click();
    await page.getByLabel('Amount ($)').fill('150000');
    await page.getByRole('button', { name: 'Save changes' }).click();

    await expect(page.locator('.account-balance')).toHaveText('$150,000', { timeout: 10_000 });
  });

  test('deleting an event returns to the empty state', async ({ page }) => {
    await page.getByRole('button', { name: 'Add event' }).click();
    await page.getByLabel('Name').fill('Downsize the home');
    await page.getByLabel('Event type').selectOption('downsize');
    await page.getByLabel('Event date', { exact: true }).fill('2033-05-01');
    await page.getByLabel('Direction').selectOption('inflow');
    await page.getByLabel('Amount ($)').fill('80000');
    await page.getByRole('button', { name: 'Add event' }).click();
    await expect(page.getByText('Downsize the home')).toBeVisible({ timeout: 10_000 });

    page.once('dialog', dialog => dialog.accept());
    await page.getByRole('button', { name: 'Delete' }).click();

    await expect(page.getByText('Downsize the home')).toBeHidden({ timeout: 10_000 });
    await expect(page.getByText('No life events yet.')).toBeVisible();
  });

  test('an added event persists across a reload', async ({ page }) => {
    await page.getByRole('button', { name: 'Add event' }).click();
    await page.getByLabel('Name').fill('Pay off mortgage');
    await page.getByLabel('Event type').selectOption('pay_off_mortgage');
    await page.getByLabel('Event date', { exact: true }).fill('2029-12-01');
    await page.getByLabel('Direction').selectOption('outflow');
    await page.getByLabel('Amount ($)').fill('60000');
    await page.getByRole('button', { name: 'Add event' }).click();
    await expect(page.locator('.account-name')).toHaveText('Pay off mortgage', {
      timeout: 10_000,
    });

    await page.reload();
    await expect(page.locator('.account-name')).toHaveText('Pay off mortgage', {
      timeout: 10_000,
    });
    await expect(page.locator('.account-balance')).toHaveText('-$60,000');
  });
});

test.describe('Feature 6 — Life events: access control', () => {
  test('unauthenticated visit redirects to /login', async ({ page }) => {
    await page.goto('/');
    await clearSession(page);
    await page.goto('/life-events');
    await expect(page).toHaveURL(/\/login/, { timeout: 10_000 });
  });
});
