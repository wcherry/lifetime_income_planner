import type { Locator, Page } from '@playwright/test';

/**
 * Locate the `.card` container for a *collapsible* card by its title — those
 * render the title as a `<button class="card-title card-title-toggle">`
 * (see `frontend/src/components/ui.tsx`'s `Card` component).
 */
export function collapsibleCard(page: Page, title: string | RegExp): Locator {
  return page.locator('.card').filter({ has: page.getByRole('button', { name: title }) });
}

/**
 * Locate the `.card` container for a *non-collapsible* card by its title —
 * those render the title as a plain `<h2 class="card-title">`.
 */
export function staticCard(page: Page, title: string | RegExp): Locator {
  return page.locator('.card').filter({ has: page.getByRole('heading', { name: title }) });
}

/**
 * Expand a collapsible card that defaults to closed (e.g. What-if / Optimize
 * on the projection page). No-ops if it's already open.
 */
export async function openCard(page: Page, title: string | RegExp): Promise<void> {
  const toggle = page.getByRole('button', { name: title });
  if ((await toggle.getAttribute('aria-expanded')) === 'false') {
    await toggle.click();
  }
}
