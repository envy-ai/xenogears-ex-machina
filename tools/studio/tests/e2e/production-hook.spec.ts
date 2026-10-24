import { createHash } from 'node:crypto';
import { expect, test } from '@playwright/test';

test.skip(process.env.VITE_XEM_STUDIO_TEST === '1', 'The production bundle assertion applies only to a normal build.');

test('keeps the explicit test driver absent from the normal production bundle', async ({ page }) => {
  const expectedBackendSha = process.env.XEM_STUDIO_EXPECTED_BACKEND_SHA256;
  let servedBackendSha: string | null = null;
  if (expectedBackendSha) {
    page.on('response', async response => {
      if (!/xem_studio_bg[^/]*\.wasm(?:$|\?)/.test(response.url())) return;
      servedBackendSha = createHash('sha256').update(await response.body()).digest('hex');
    });
  }
  await page.goto('/');
  await expect(page.getByRole('main', { name: 'XEM Studio' })).toBeVisible();
  expect(await page.evaluate(() => Object.hasOwn(window, '__xemStudioTest'))).toBe(false);
  if (expectedBackendSha) {
    await expect.poll(() => servedBackendSha).toBe(expectedBackendSha);
  }
});
