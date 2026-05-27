import { test, expect } from '@playwright/test';

test('project rename round-trips through the CSRF-guarded form', async ({ page }) => {
  await page.goto('/web/projects/1/settings/');

  const newName = `pw-renamed-${Date.now()}`;
  await page.locator('input[name="name"]').first().fill(newName);
  await page
    .locator('form[action="/web/projects/1/settings/name"] button[type="submit"]')
    .click();

  await page.goto('/web/projects/1/settings/');
  await expect(page.locator('input[name="name"]').first()).toHaveValue(newName);
});
