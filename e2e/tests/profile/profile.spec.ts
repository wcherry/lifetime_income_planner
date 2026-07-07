import { test, expect } from '../../fixtures/base';
import { clearSession, registerAndLogin } from '../helpers/auth';

test.describe('Feature 2 — Retirement profile setup', () => {
  test.beforeEach(async ({ page, request }) => {
    await registerAndLogin(page, request);
    await page.goto('/profile');
    await expect(page.getByRole('heading', { name: 'Retirement profile' })).toBeVisible({
      timeout: 10_000,
    });
  });

  test('renders the core profile fields', async ({ page }) => {
    await expect(page.getByLabel('First name')).toBeVisible();
    await expect(page.getByLabel('Last name')).toBeVisible();
    await expect(page.getByLabel('Date of birth')).toBeVisible();
    await expect(page.getByLabel('State')).toBeVisible();
    await expect(page.getByLabel('Marital status')).toBeVisible();
    await expect(page.getByLabel('Tax filing status')).toBeVisible();
    await expect(page.getByLabel('Planned retirement date')).toBeVisible();
    await expect(page.getByLabel('Life expectancy (age)', { exact: true })).toBeVisible();
  });

  test('filling and saving a profile shows a success confirmation', async ({ page }) => {
    await page.getByLabel('First name').fill('Ada');
    await page.getByLabel('Last name').fill('Lovelace');
    await page.getByLabel('Date of birth').fill('1960-12-10');
    await page.getByLabel('State').selectOption('CA');
    await page.getByLabel('Marital status').selectOption('single');
    await page.getByLabel('Tax filing status').selectOption('single');
    await page.getByLabel('Planned retirement date').fill('2028-01-01');
    await page.getByLabel('Life expectancy (age)', { exact: true }).fill('95');

    await page.getByRole('button', { name: 'Save profile' }).click();
    await expect(page.locator('.alert-success')).toContainText(/saved/i, { timeout: 10_000 });
  });

  test('saved profile persists across a reload', async ({ page }) => {
    await page.getByLabel('First name').fill('Grace');
    await page.getByLabel('Last name').fill('Hopper');
    await page.getByLabel('Date of birth').fill('1955-06-15');
    await page.getByLabel('State').selectOption('NY');
    await page.getByLabel('Planned retirement date').fill('2027-07-01');
    await page.getByLabel('Life expectancy (age)', { exact: true }).fill('92');
    await page.getByRole('button', { name: 'Save profile' }).click();
    await expect(page.locator('.alert-success')).toBeVisible({ timeout: 10_000 });

    await page.reload();
    await expect(page.getByLabel('First name')).toHaveValue('Grace', { timeout: 10_000 });
    await expect(page.getByLabel('Last name')).toHaveValue('Hopper');
    await expect(page.getByLabel('Date of birth')).toHaveValue('1955-06-15');
    await expect(page.getByLabel('State')).toHaveValue('NY');
    await expect(page.getByLabel('Life expectancy (age)', { exact: true })).toHaveValue('92');
  });

  test('choosing "Married" reveals the spouse detail fields', async ({ page }) => {
    await expect(page.getByLabel('Spouse first name')).toBeHidden();

    await page.getByLabel('Marital status').selectOption('married');

    await expect(page.getByLabel('Spouse first name')).toBeVisible();
    await expect(page.getByLabel('Spouse last name')).toBeVisible();
    await expect(page.getByLabel('Spouse date of birth')).toBeVisible();
    await expect(page.getByLabel('Spouse life expectancy (age)')).toBeVisible();
  });

  test('a married profile with spouse details persists across a reload', async ({ page }) => {
    await page.getByLabel('First name').fill('John');
    await page.getByLabel('Last name').fill('Smith');
    await page.getByLabel('Date of birth').fill('1958-03-03');
    await page.getByLabel('State').selectOption('TX');
    await page.getByLabel('Marital status').selectOption('married');
    await page.getByLabel('Tax filing status').selectOption('married_filing_jointly');
    await page.getByLabel('Planned retirement date').fill('2026-12-31');
    await page.getByLabel('Life expectancy (age)', { exact: true }).fill('90');

    await page.getByLabel('Spouse first name').fill('Jane');
    await page.getByLabel('Spouse last name').fill('Smith');
    await page.getByLabel('Spouse date of birth').fill('1960-05-05');
    await page.getByLabel('Spouse life expectancy (age)').fill('93');

    await page.getByRole('button', { name: 'Save profile' }).click();
    await expect(page.locator('.alert-success')).toBeVisible({ timeout: 10_000 });

    await page.reload();
    await expect(page.getByLabel('Marital status')).toHaveValue('married', { timeout: 10_000 });
    await expect(page.getByLabel('Spouse first name')).toHaveValue('Jane');
    await expect(page.getByLabel('Spouse life expectancy (age)')).toHaveValue('93');
  });
});

test.describe('Feature 2 — Retirement profile: access control', () => {
  test('unauthenticated visit redirects to /login', async ({ page }) => {
    await page.goto('/');
    await clearSession(page);
    await page.goto('/profile');
    await expect(page).toHaveURL(/\/login/, { timeout: 10_000 });
  });
});
