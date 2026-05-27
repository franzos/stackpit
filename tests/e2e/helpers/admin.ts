import type { Page } from '@playwright/test';

export function adminToken(): string {
  const t = process.env.ADMIN_TOKEN;
  if (!t) throw new Error('ADMIN_TOKEN not set (the Makefile e2e target sets it)');
  return t;
}

/** Log in and land on /web/projects/. */
export async function login(page: Page): Promise<void> {
  await page.goto('/web/login');
  await page.locator('input[name="token"]').fill(adminToken());
  await Promise.all([
    page.waitForURL((u) => u.pathname.startsWith('/web/projects')),
    page.locator('form[action="/web/login"] button[type="submit"]').click(),
  ]);
}
