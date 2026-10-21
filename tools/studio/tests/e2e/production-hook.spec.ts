import { expect, test } from '@playwright/test';

test.skip(process.env.VITE_XEM_STUDIO_TEST === '1', 'The production bundle assertion applies only to a normal build.');

test('keeps the explicit test driver absent from the normal production bundle', async ({ page }) => {
  await page.goto('/');
  await expect(page.getByRole('main', { name: 'XEM Studio' })).toBeVisible();
  expect(await page.evaluate(() => Object.hasOwn(window, '__xemStudioTest'))).toBe(false);
});
