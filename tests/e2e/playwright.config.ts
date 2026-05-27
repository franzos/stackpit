import { defineConfig } from '@playwright/test';

export const STORAGE_STATE = '.auth/admin.json';

// With `--network host` the container shares the host loopback.
export default defineConfig({
  testDir: './tests/smoke',
  workers: 1,
  fullyParallel: false,
  retries: 1,
  reporter: [['list'], ['html', { open: 'never' }]],
  use: {
    baseURL: process.env.BASE_URL || 'http://localhost:3333',
    trace: 'on-first-retry',
    screenshot: 'only-on-failure',
    actionTimeout: 15_000,
    navigationTimeout: 30_000,
  },
  projects: [
    { name: 'setup', testMatch: /auth\.setup\.ts$/ },

    // login.spec.ts exercises the login flow itself, so it must run with no
    // stored session.
    { name: 'login', testMatch: /login\.spec\.ts$/ },

    {
      name: 'smoke',
      testIgnore: /login\.spec\.ts$/,
      dependencies: ['setup'],
      use: { storageState: STORAGE_STATE },
    },
  ],
});
