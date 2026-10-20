import { defineConfig } from '@playwright/test';

const chromium = process.env.XEM_CHROMIUM;
if (!chromium) throw new Error('Set XEM_CHROMIUM to the pinned Chromium executable.');

export default defineConfig({
  testDir: './tests/e2e',
  testMatch: '**/*.spec.ts',
  fullyParallel: false,
  timeout: 30_000,
  expect: { timeout: 8_000 },
  reporter: 'list',
  outputDir: '../../tmp/studio/playwright-results',
  use: {
    baseURL: 'https://localhost:5176',
    browserName: 'chromium',
    headless: true,
    ignoreHTTPSErrors: true,
    viewport: { width: 1440, height: 900 },
    launchOptions: { executablePath: chromium },
    trace: 'retain-on-failure',
  },
  webServer: {
    command: 'npm run dev -- --host 127.0.0.1',
    url: 'https://localhost:5176',
    reuseExistingServer: !process.env.CI,
    ignoreHTTPSErrors: true,
    timeout: 30_000,
  },
});
