import { test, expect } from '../../fixtures/base';
import { registerViaApi, uniqueEmail, TOKEN_KEY } from '../helpers/auth';

test.describe('Feature 1 — Authentication: Register', () => {
  test('happy path — creates account, auto-logs in, and lands on the profile', async ({ page }) => {
    const email = uniqueEmail();

    await page.goto('/register');
    await page.getByLabel('Email').fill(email);
    await page.getByLabel('Password', { exact: true }).fill('Password123!');
    await page.getByLabel('Confirm password').fill('Password123!');
    await page.getByRole('button', { name: 'Create account' }).click();

    // Registration signs the user in and navigates to the profile page.
    await expect(page).toHaveURL(/\/profile/, { timeout: 15_000 });
    await expect(page.getByRole('heading', { name: 'Retirement profile' })).toBeVisible();

    // Token is persisted and the header greets the signed-in user.
    const token = await page.evaluate(key => localStorage.getItem(key), TOKEN_KEY);
    expect(token).toBeTruthy();
    await expect(page.locator('.app-header')).toContainText(email);
  });

  test('mismatched passwords — shows an error and does not submit', async ({ page }) => {
    let registerCalled = false;
    page.on('request', req => {
      if (req.url().includes('/api/auth/register')) registerCalled = true;
    });

    await page.goto('/register');
    await page.getByLabel('Email').fill(uniqueEmail());
    await page.getByLabel('Password', { exact: true }).fill('Password123!');
    await page.getByLabel('Confirm password').fill('Different123!');
    await page.getByRole('button', { name: 'Create account' }).click();

    await expect(page.locator('.alert-error')).toContainText(/do not match/i);
    await expect(page).toHaveURL(/\/register/);
    expect(registerCalled).toBe(false);
  });

  test('duplicate email — shows an error and stays on the register page', async ({
    page,
    request,
  }) => {
    const { email } = await registerViaApi(request);

    await page.goto('/register');
    await page.getByLabel('Email').fill(email);
    await page.getByLabel('Password', { exact: true }).fill('Password123!');
    await page.getByLabel('Confirm password').fill('Password123!');
    await page.getByRole('button', { name: 'Create account' }).click();

    await expect(page).toHaveURL(/\/register/, { timeout: 10_000 });
    await expect(page.locator('.alert-error')).toBeVisible({ timeout: 10_000 });
  });

  test('password under 8 characters — browser validation blocks submission', async ({ page }) => {
    let registerCalled = false;
    page.on('request', req => {
      if (req.url().includes('/api/auth/register')) registerCalled = true;
    });

    await page.goto('/register');
    await page.getByLabel('Email').fill(uniqueEmail());
    await page.getByLabel('Password', { exact: true }).fill('Short1!'); // 7 chars < minLength 8
    await page.getByLabel('Confirm password').fill('Short1!');
    await page.getByRole('button', { name: 'Create account' }).click();

    await page.waitForTimeout(500);
    expect(registerCalled).toBe(false);
    await expect(page).toHaveURL(/\/register/);
  });

  test('register page links to sign-in', async ({ page }) => {
    await page.goto('/register');
    await page.getByRole('link', { name: 'Sign in' }).click();
    await expect(page).toHaveURL(/\/login/);
    await expect(page.getByRole('heading', { name: 'Welcome back' })).toBeVisible();
  });
});
