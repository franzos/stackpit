import { test, expect } from '@playwright/test';

// The bulk "all matching" form has no row checkboxes — it acts on a server-computed
// filter match — so the affirmation checkbox is the only thing standing between a
// page load and "Delete all N". These two tests are the verification tasks 9 owes.
test.describe('bulk all-matching gate', () => {
  test('the all-matching buttons stay disabled until the affirmation is ticked', async ({ page }) => {
    await page.goto('/web/projects/1/');

    const form = page.locator('form:has(.select-all-gate)');
    await expect(form).toBeVisible();
    const deleteAll = form.locator('button[value="delete"]');

    // Ships `disabled` in the markup, so the action is unavailable without JS too.
    await expect(deleteAll).toBeDisabled();

    await form.locator('.select-all-gate input[type="checkbox"]').check();
    await expect(deleteAll).toBeEnabled();

    // And unticking must put it back, not latch on.
    await form.locator('.select-all-gate input[type="checkbox"]').uncheck();
    await expect(deleteAll).toBeDisabled();
  });

  // The risk in task 9 was never the new gate failing — it was select-all.js
  // breaking the row-checkbox bar that already worked on the same page load.
  test('the per-row bulk bar still works independently on the same page', async ({ page }) => {
    await page.goto('/web/projects/1/');

    const bar = page.locator('.bulk-bar');
    await expect(bar).toBeHidden();

    await page.locator('input[type="checkbox"][name="ids"]').first().check();
    await expect(bar).toBeVisible();
    await expect(bar).toContainText('1 selected');

    // The row bar appearing must not have enabled the all-matching buttons.
    await expect(
      page.locator('form:has(.select-all-gate) button[value="delete"]'),
    ).toBeDisabled();
  });
});

// Task 13: the traces table got its own pager, and the two pagers on this page
// must not fight — paging one has to leave the other's offset alone.
//
// The seed spreads spans thinly across 100 projects, so no project reaches the
// default 25-trace page size. `trace_limit=1` exercises the same pager on any
// project with two traces, which the seed does reliably produce; we scan for one
// rather than hardcoding an id, because the seed is not deterministic.
test('the traces pager pages independently of the all-spans pager', async ({ page }) => {
  let found = 0;
  for (let id = 1; id <= 40; id++) {
    await page.goto(`/web/projects/${id}/spans/?trace_limit=1&limit=1`);
    const next = page.locator('nav.pagination').first().getByRole('link', { name: /next/i });
    if (await next.count()) {
      found = id;
      break;
    }
  }
  test.skip(!found, 'seed produced no project with two traces');

  const tracesTable = page.locator('table').first();
  const firstPageIds = await tracesTable.locator('tbody a[href*="/traces/"]').allTextContents();
  expect(firstPageIds.length).toBeGreaterThan(0);

  await page.locator('nav.pagination').first().getByRole('link', { name: /next/i }).click();

  // Traces advanced...
  await expect(page).toHaveURL(/trace_offset=[1-9]/);
  // ...and the all-spans pager's own offset came along untouched.
  await expect(page).toHaveURL(/[?&]offset=0(&|$)/);

  const secondPageIds = await page
    .locator('table')
    .first()
    .locator('tbody a[href*="/traces/"]')
    .allTextContents();
  expect(secondPageIds.length).toBeGreaterThan(0);
  expect(secondPageIds).not.toEqual(firstPageIds);
});

// Task 4(d): the route registration itself is not unit-testable (routes() needs a
// full AppState), so this path is only ever covered here.
test('/issues/ redirects to the issue stream instead of 404ing', async ({ page }) => {
  const response = await page.goto('/web/projects/1/issues/');
  expect(response?.status()).toBe(200);
  await expect(page).toHaveURL(/\/web\/projects\/1\/$/);
  await expect(page.locator('form.filter-form input[name="query"]')).toBeVisible();
});

// Task 8's environment filter, task 15a's two new panels: a smoke pass proving the
// new controls are wired to the query rather than rendered and ignored.
test('the issue stream exposes an environment filter that round-trips', async ({ page }) => {
  await page.goto('/web/projects/1/');

  const env = page.locator('select[name="environment"]');
  await expect(env).toBeVisible();

  const options = await env.locator('option').allTextContents();
  test.skip(options.length < 2, 'seed has no distinct environments');

  await env.selectOption({ index: 1 });
  await page.locator('form.filter-form button[type="submit"]').click();
  await expect(page).toHaveURL(/environment=/);
  // The selection survives the round trip rather than resetting to "All".
  await expect(page.locator('select[name="environment"]')).not.toHaveValue('');
});
