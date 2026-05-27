import { test, expect } from '@playwright/test';
import { login, adminToken } from '../../helpers/admin';

test('valid admin token lands on the project list', async ({ page }) => {
  await login(page);
  await expect(page).toHaveURL(/\/web\/projects/);
});

test('wrong token is rejected at login', async ({ page }) => {
  await page.goto('/web/login');
  await page.locator('input[name="token"]').fill('definitely-wrong');
  await page.locator('form[action="/web/login"] button[type="submit"]').click();
  await expect(page).not.toHaveURL(/\/web\/projects/);
  await expect(page.locator('input[name="token"]')).toBeVisible();
});
