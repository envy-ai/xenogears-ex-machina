import { expect, test } from '@playwright/test';
import { basename, dirname, resolve } from 'node:path';
import { readFileSync } from 'node:fs';
import type { StudioTestSnapshot } from './studio-test-driver';

const fixturePath = process.env.XEM_STUDIO_WASM_DISC_PARITY_FIXTURE;
const discDirectory = process.env.XEM_STUDIO_DISC_DIR;
const fixture = fixturePath ? JSON.parse(readFileSync(fixturePath, 'utf8')) as {
  image_path: string;
  maps: { maps: Array<{ map: number; placeholder: boolean }> };
  graph: { instructions: unknown[] };
} : undefined;

async function snapshot(page: import('@playwright/test').Page): Promise<StudioTestSnapshot> {
  return page.evaluate(() => {
    const driver = window.__xemStudioTest;
    if (!driver) throw new Error('The explicit Studio test driver is unavailable.');
    return driver.snapshot();
  });
}

test('shows a clear status when server disc access is not configured', async ({ page }) => {
  test.skip(Boolean(discDirectory), 'This status is covered by the configured server fixture run.');
  await page.goto('/');
  await page.getByRole('button', { name: 'Browse server discs' }).click();
  await expect(page.getByLabel('Server discs')).toContainText('Server disc access is not configured.');
});

test('opens a synthetic disc through the configured HTTP range server and real wasm backend', async ({ page }, testInfo) => {
  test.skip(process.env.VITE_XEM_STUDIO_TEST !== '1', 'The test-only fixture WASM build is required.');
  test.skip(!discDirectory || !fixture, 'Set XEM_STUDIO_DISC_DIR and XEM_STUDIO_WASM_DISC_PARITY_FIXTURE to the synthetic fixture folder and manifest.');
  expect(resolve(discDirectory!)).toBe(resolve(dirname(fixture!.image_path)));
  const imageName = basename(fixture!.image_path);
  const expectedMap = fixture!.maps.maps.find(item => !item.placeholder);
  test.skip(!expectedMap, 'The synthetic fixture must include a non-placeholder map.');

  const listed = page.waitForResponse(response => response.url().endsWith('/__studio/discs') && response.status() === 200);
  await page.goto('/');
  await expect(page.getByRole('main', { name: 'XEM Studio' })).toBeVisible();
  await page.getByRole('button', { name: 'Browse server discs' }).click();
  await listed;
  await expect(page.getByLabel('Server discs')).toContainText('Server files · read only');
  await page.getByLabel('Server disc image').selectOption({ label: imageName });

  const rangeResponses: Array<{ status: number; range: string | undefined }> = [];
  page.on('response', response => {
    if (!response.url().includes('/__studio/discs/')) return;
    rangeResponses.push({ status: response.status(), range: response.request().headers().range });
  });
  await page.getByRole('button', { name: 'Open server disc' }).click();
  await expect.poll(async () => (await snapshot(page)).discs.length).toBe(1);
  expect((await snapshot(page)).discs[0]?.mapCount).toBe(fixture!.maps.maps.length);

  await page.locator(`.event-map-row[data-map="${expectedMap!.map}"]`).click();
  await expect.poll(async () => (await snapshot(page)).map).toBe(expectedMap!.map);
  await expect.poll(async () => (await snapshot(page)).instructions).toBe(fixture!.graph.instructions.length);
  await expect(page.getByRole('list', { name: 'Event commands' })).toBeVisible();
  const rowGeometry = await page.locator('.event-map-row').evaluateAll(rows => rows.slice(0, 2).map(row => {
    const rect = row.getBoundingClientRect();
    const children = [...row.querySelectorAll('span')].map(child => child.getBoundingClientRect());
    return { top: rect.top, bottom: rect.bottom, childrenInside: children.every(child =>
      child.top >= rect.top && child.bottom <= rect.bottom && child.left >= rect.left && child.right <= rect.right) };
  }));
  expect(rowGeometry).toHaveLength(2);
  expect(rowGeometry[0]!.bottom).toBeLessThanOrEqual(rowGeometry[1]!.top);
  expect(rowGeometry.every(row => row.childrenInside)).toBe(true);
  await page.screenshot({ path: testInfo.outputPath('remote-disc-synthetic.png'), fullPage: false });
  expect(rangeResponses.length).toBeGreaterThan(0);
  expect(rangeResponses.every(item => item.status === 206 && /^bytes=\d+-\d+$/.test(item.range ?? ''))).toBe(true);
  expect(rangeResponses.some(item => item.range === undefined)).toBe(false);
});

test('opens both authorized local discs through bounded HTTP ranges and the Rust CUE parser', async ({ page }) => {
  const extractRoot = process.env.XEM_STUDIO_EXTRACT_ROOT;
  test.skip(process.env.VITE_XEM_STUDIO_TEST !== '1', 'The explicit test-only Studio driver is required.');
  test.skip(process.env.XEM_STUDIO_REMOTE_REAL_DISC_CHECK !== '1', 'Real local inputs are opt-in for this focused transport check.');
  test.skip(!extractRoot || !discDirectory, 'Set the authorized extraction root and exact configured server disc directory.');
  expect(resolve(discDirectory!)).toBe(resolve(extractRoot!, 'discs'));
  test.setTimeout(120_000);

  await page.goto('/');
  await expect(page.getByRole('main', { name: 'XEM Studio' })).toBeVisible();
  await page.getByRole('button', { name: 'Browse server discs' }).click();
  await expect(page.getByLabel('Server discs')).toContainText('Server files · read only');

  for (const [name, expectedDisc] of [['disc1.bin', 1], ['disc2.cue', 2]] as const) {
    await page.getByLabel('Server disc image').selectOption({ label: name });
    await page.getByRole('button', { name: 'Open server disc' }).click();
    await expect.poll(async () => (await snapshot(page)).discs.length, { timeout: 60_000, intervals: [500, 1_000, 2_000, 5_000] }).toBe(expectedDisc);
    const opened = (await snapshot(page)).discs[expectedDisc - 1];
    expect(opened?.disc).toBe(expectedDisc);
    expect(opened?.mapCount).toBe(730);
  }
  const state = await snapshot(page);
  expect(state.discs.reduce((sum, disc) => sum + disc.placeholders, 0)).toBe(525);

  await page.getByRole('button', { name: /^Disc 1 · 730 maps$/ }).click();
  await page.locator('.event-map-row[data-map="2"]').click();
  await expect.poll(async () => (await snapshot(page)).map, { timeout: 60_000, intervals: [500, 1_000, 2_000, 5_000] }).toBe(2);
  await expect.poll(async () => (await snapshot(page)).instructions, { timeout: 60_000, intervals: [500, 1_000, 2_000, 5_000] }).toBe(2_358);
  await expect(page.getByRole('list', { name: 'Event commands' })).toBeVisible();
});
