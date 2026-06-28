// Showcase screenshot capture. Runs inside the Playwright container against a
// seeded, running server. Navigates dynamically (clicks real issues/events) so
// it doesn't depend on hardcoded fingerprints. Output -> /work/shots.
import { chromium } from '@playwright/test';
import { mkdirSync } from 'node:fs';

const BASE = process.env.BASE_URL || 'http://localhost:3333';
const TOK = process.env.ADMIN_TOKEN;
const OUT = '/work/shots';
const RICH_PROJECT = process.env.RICH_PROJECT || '21';
const LOGS_PROJECT = process.env.LOGS_PROJECT || '47';
mkdirSync(OUT, { recursive: true });

const settle = async (page) => {
  await page.waitForLoadState('domcontentloaded');
  await page.waitForTimeout(700);
};

async function newPage(browser, colorScheme) {
  const ctx = await browser.newContext({
    viewport: { width: 1440, height: 900 },
    deviceScaleFactor: 2,
    colorScheme,
  });
  const page = await ctx.newPage();
  await page.goto(BASE + '/web/login');
  await page.locator('input[name="token"]').fill(TOK);
  await Promise.all([
    page.waitForURL((u) => u.pathname.startsWith('/web/projects')),
    page.locator('form[action="/web/login"] button[type="submit"]').click(),
  ]);
  return page;
}

const hrefMatching = (page, re) =>
  page.evaluate((src) => {
    const r = new RegExp(src);
    const a = [...document.querySelectorAll('a')].find((a) =>
      r.test(a.getAttribute('href') || ''),
    );
    return a ? a.getAttribute('href') : null;
  }, re.source);

const browser = await chromium.launch();
const page = await newPage(browser, 'dark');
const shot = async (n) => {
  await page.waitForTimeout(300);
  await page.screenshot({ path: `${OUT}/${n}.png` });
  console.log('shot', n, '->', page.url());
};

// 1. Project list (the home dashboard)
await page.goto(BASE + '/web/projects/');
await settle(page);
await shot('01-projects');

// 2. Issue list for a busy project
await page.goto(`${BASE}/web/projects/${RICH_PROJECT}/`);
await settle(page);
await shot('02-issues');

// 3. Issue detail (click the first real issue)
const issueHref = await hrefMatching(page, /\/issues\/[^/]+\/$/);
if (issueHref) {
  await page.goto(BASE + issueHref);
  await settle(page);
  await shot('03-issue-detail');

  // 4. Event detail (events live under the "All events" tab)
  await page.goto(BASE + issueHref + '?tab=events');
  await settle(page);
  const evHref = await hrefMatching(page, /\/events\/[0-9a-fA-F]+\/$/);
  if (evHref) {
    await page.goto(BASE + evHref);
    await settle(page);
    await shot('04-event-detail');
  } else {
    console.log('no event link found on issue detail');
  }
} else {
  console.log('no issue link found for project', RICH_PROJECT);
}

// 5. Cross-project events firehose
await page.goto(BASE + '/web/events/');
await settle(page);
await shot('05-events-firehose');

// 6. Releases
await page.goto(BASE + '/web/releases/');
await settle(page);
await shot('06-releases');

// 7. Logs tab (project with the most logs)
await page.goto(`${BASE}/web/projects/${LOGS_PROJECT}/logs/`);
await settle(page);
await shot('07-logs');

// 8. Project settings
await page.goto(`${BASE}/web/projects/${RICH_PROJECT}/settings/`);
await settle(page);
await shot('08-settings');

// 9. Light-theme variant of the dashboard
const lightPage = await newPage(browser, 'light');
await lightPage.goto(BASE + '/web/projects/');
await lightPage.waitForLoadState('domcontentloaded');
await lightPage.waitForTimeout(800);
await lightPage.screenshot({ path: `${OUT}/09-projects-light.png` });
console.log('shot', '09-projects-light', '->', lightPage.url());

await browser.close();
console.log('done');
