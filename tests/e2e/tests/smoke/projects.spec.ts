import { test, expect } from '@playwright/test';

test('creating a project shows a DSN', async ({ page }) => {
  await page.goto('/web/projects/new');
  await page.locator('input[name="name"]').fill(`pw-smoke-${Date.now()}`);
  // base.html's logout form precedes page content, so scope to the create form.
  await page.locator('form[action="/web/projects/new"] button[type="submit"]').click();
  await expect(page.locator('body')).toContainText(/sentry:\/\/|https?:\/\/.+@/);
});

test('a seeded project renders its issue list', async ({ page }) => {
  await page.goto('/web/projects/1/');
  // The filter form is issue-list-specific; the table + a row link prove issues rendered.
  await expect(page.locator('form.filter-form input[name="query"]')).toBeVisible();
  await expect(
    page.locator('table thead th', { hasText: 'First Seen' }),
  ).toBeVisible();
  await expect(
    page.locator('table tbody a[href*="/issues/"]').first(),
  ).toBeVisible();
});
