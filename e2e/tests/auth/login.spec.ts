import { test, expect } from '../../fixtures/base';
import { clearSession, registerViaApi, TOKEN_KEY } from '../helpers/auth';

test.describe('Feature 1 — Authentication: Login & sessions', () => {
  test('happy path — signs in with valid credentials', async ({ page, request }) => {
    const { email, password } = await registerViaApi(request);

    await page.goto('/login');
    await page.getByLabel('Email').fill(email);
    await page.getByLabel('Password').fill(password);
    await page.getByRole('button', { name: 'Sign in' }).click();

    // Login leaves /login for the app shell (the "/" route renders the profile).
    await expect(page).not.toHaveURL(/\/login/, { timeout: 15_000 });
    await expect(page.locator('.app-header')).toContainText(email);
    const token = await page.evaluate(key => localStorage.getItem(key), TOKEN_KEY);
    expect(token).toBeTruthy();
  });

  test('wrong password — shows an error message', async ({ page, request }) => {
    const { email } = await registerViaApi(request);

    await page.goto('/login');
    await page.getByLabel('Email').fill(email);
    await page.getByLabel('Password').fill('WrongPassword99!');
    await page.getByRole('button', { name: 'Sign in' }).click();

    await expect(page).toHaveURL(/\/login/, { timeout: 10_000 });
    await expect(page.locator('.alert-error')).toBeVisible({ timeout: 10_000 });
  });

  test('unknown email — shows an error message', async ({ page }) => {
    await page.goto('/login');
    await page.getByLabel('Email').fill('nobody_exists@example.com');
    await page.getByLabel('Password').fill('Password123!');
    await page.getByRole('button', { name: 'Sign in' }).click();

    await expect(page).toHaveURL(/\/login/, { timeout: 10_000 });
    await expect(page.locator('.alert-error')).toBeVisible({ timeout: 10_000 });
  });

  test('protected route — unauthenticated visit redirects to /login', async ({ page }) => {
    await page.goto('/');
    await clearSession(page);

    await page.goto('/accounts');
    await expect(page).toHaveURL(/\/login/, { timeout: 15_000 });
  });

  test('logout — clears the session and protected routes redirect to /login', async ({
    page,
    request,
  }) => {
    const { email, password } = await registerViaApi(request);

    await page.goto('/login');
    await page.getByLabel('Email').fill(email);
    await page.getByLabel('Password').fill(password);
    await page.getByRole('button', { name: 'Sign in' }).click();
    await expect(page.locator('.app-header')).toContainText(email, { timeout: 15_000 });

    await page.getByRole('button', { name: 'Log out' }).click();
    await expect(page).toHaveURL(/\/login/, { timeout: 10_000 });

    // The token is gone, so visiting a protected route bounces back to /login.
    const token = await page.evaluate(key => localStorage.getItem(key), TOKEN_KEY);
    expect(token).toBeNull();
    await page.goto('/profile');
    await expect(page).toHaveURL(/\/login/, { timeout: 10_000 });
  });

  test('session persists across a page reload', async ({ page, request }) => {
    const { email, password } = await registerViaApi(request);

    await page.goto('/login');
    await page.getByLabel('Email').fill(email);
    await page.getByLabel('Password').fill(password);
    await page.getByRole('button', { name: 'Sign in' }).click();
    await expect(page.locator('.app-header')).toContainText(email, { timeout: 15_000 });

    await page.reload();
    await expect(page.locator('.app-header')).toContainText(email, { timeout: 15_000 });
    await expect(page).not.toHaveURL(/\/login/);
  });

  test('sign-in page links to register', async ({ page }) => {
    await page.goto('/login');
    await page.getByRole('link', { name: 'Create one' }).click();
    await expect(page).toHaveURL(/\/register/);
    await expect(page.getByRole('heading', { name: 'Create your account' })).toBeVisible();
  });
});
