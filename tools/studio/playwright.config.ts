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
  outputDir: process.env.XEM_STUDIO_PLAYWRIGHT_OUTPUT_DIR ?? '../../tmp/studio/playwright-results',
  use: {
    baseURL: 'http://localhost:5176',
    browserName: 'chromium',
    headless: true,
    viewport: { width: 1440, height: 900 },
    launchOptions: { executablePath: chromium, chromiumSandbox: true },
    trace: 'retain-on-failure',
  },
  webServer: {
    command: 'node tests/e2e/serve-built.mjs',
    url: 'http://localhost:5176',
    reuseExistingServer: false,
    timeout: 30_000,
  },
});
