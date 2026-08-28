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

// The Remove button used to sit behind the repositories card's inner scrollbar
// on a wide viewport. The seed script creates no repositories, so the test
// makes one; the viewport is set explicitly because the suite otherwise runs at
// Playwright's 1280x720 default, well below where the clipping appeared.
test('the repository Remove button is fully visible on a wide viewport', async ({ page }) => {
  await page.setViewportSize({ width: 1910, height: 900 });

  // Project ids come from the seed and are not dense, so take the first one the
  // list offers rather than assuming a fixed id exists.
  await page.goto('/web/projects/');
  const href = await page
    .locator('table tbody a[href^="/web/projects/"]')
    .first()
    .getAttribute('href');
  const projectId = href!.split('/')[3];

  await page.goto(`/web/projects/${projectId}/settings/`);

  // Each existing row also posts to the repos route, carrying the URL it was
  // rendered from as a hidden field, so both selectors have to name the add
  // form specifically rather than the shared action.
  const repoUrl = `https://github.com/pw/repo-${Date.now()}`;
  const addForm = page.locator(
    `form[action="/web/projects/${projectId}/settings/repos"]:has(#repo_url)`,
  );
  await addForm.locator('#repo_url').fill(repoUrl);
  await addForm.locator('button[type="submit"]').click();

  const row = page.locator('tr', { hasText: repoUrl });
  await expect(row).toHaveCount(1);
  await expect(row.locator('button[type="submit"]', { hasText: /remove/i })).toBeInViewport({
    ratio: 1,
  });
});
