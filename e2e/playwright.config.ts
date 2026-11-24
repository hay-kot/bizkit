import { defineConfig, devices } from '@playwright/test'

// No webServer: scripts/test-e2e.sh starts Mailpit and the server, then sets
// BASE_URL, MAILPIT_URL and E2E_SEED_CMD.
export default defineConfig({
  testDir: 'tests',
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: process.env.E2E_WORKERS || '50%',
  reporter: process.env.CI ? [['github'], ['html', { open: 'never' }]] : [['list']],
  use: {
    baseURL: process.env.BASE_URL,
    trace: 'on-first-retry',
    ...devices['Desktop Chrome'],
  },
  projects: [
    { name: 'smoke', testDir: 'tests/smoke' },
    { name: 'workflows', testDir: 'tests/workflows' },
    { name: 'features', testDir: 'tests/features' },
    // Tests that change shared state (Mailpit chaos, the server clock) run
    // after every other project, one at a time: fullyParallel only orders
    // the tests of one file, so one worker keeps two files apart too.
    {
      name: 'isolated',
      testDir: 'tests/isolated',
      dependencies: ['smoke', 'workflows', 'features'],
      fullyParallel: false,
      workers: 1,
    },
  ],
})
