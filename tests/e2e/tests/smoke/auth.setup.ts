import { test as setup } from '@playwright/test';
import { login } from '../../helpers/admin';
import { STORAGE_STATE } from '../../playwright.config';

// Share one session across specs to stay under the POST /web/login rate limit (10/min/IP).
setup('authenticate', async ({ page }) => {
  await login(page);
  await page.context().storageState({ path: STORAGE_STATE });
});
