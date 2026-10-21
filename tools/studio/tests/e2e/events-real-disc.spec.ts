import { expect, test, type Page } from '@playwright/test';
import type { RenderTime, StudioTestSnapshot } from './studio-test-driver';

const disc1 = process.env.XEM_STUDIO_DISC1_PATH;
const disc2 = process.env.XEM_STUDIO_DISC2_PATH;
const extractRoot = process.env.XEM_STUDIO_EXTRACT_ROOT;

async function readSnapshot(page: Page): Promise<StudioTestSnapshot> {
  return page.evaluate(() => {
    const driver = window.__xemStudioTest;
    if (!driver) throw new Error('The explicit Studio test driver is unavailable.');
    return driver.snapshot();
  });
}

test('opens both local discs and measures the first real map 2 render in all views', async ({ page }) => {
  test.skip(process.env.VITE_XEM_STUDIO_TEST !== '1', 'Real-disc event views require the explicit test-only Studio driver.');
  test.skip(!extractRoot, 'Set XEM_STUDIO_EXTRACT_ROOT to the authorized local extraction root.');
  test.skip(!disc1 || !disc2, 'Set XEM_STUDIO_DISC1_PATH and XEM_STUDIO_DISC2_PATH to the existing local disc images.');
  test.setTimeout(20 * 60 * 1000);
  await page.goto('/');
  await expect(page.getByRole('main', { name: 'XEM Studio' })).toBeVisible();
  await page.getByLabel('Open disc files').setInputFiles([disc1!, disc2!]);

  await expect.poll(async () => (await readSnapshot(page)).discs.map(item => item.disc), { timeout: 15 * 60 * 1000, intervals: [1000, 5000, 15000] }).toEqual([1, 2]);
  let state = await readSnapshot(page);
  expect(state.discs.every(item => item.mapCount > 0)).toBe(true);
  await page.getByRole('button', { name: /^Disc 1 ·/ }).click();
  await expect(page.getByLabel('Filter maps')).toBeVisible();
  await page.locator('.event-map-row[data-map="2"]').click();
  await expect.poll(async () => (await readSnapshot(page)).map, { timeout: 5 * 60 * 1000, intervals: [500, 2000, 5000] }).toBe(2);

  state = await readSnapshot(page);
  expect(state.instructions).toBe(2358);
  const list = page.getByRole('tabpanel', { name: 'Command list' });
  await expect(list).toBeVisible();
  await expect(list.locator('.event-command-row').first()).toBeVisible();
  await expect(page.getByText(/Map 2 · .*\d+ map instructions/)).toBeVisible();

  const firstPaint: Partial<Record<RenderTime['view'], RenderTime>> = {};
  let listTiming: RenderTime | undefined;
  await expect.poll(async () => {
    state = await readSnapshot(page);
    listTiming = state.renderTimes.filter(item => item.view === 'list' && item.map === 2).at(-1);
    return listTiming;
  }, { timeout: 10_000 }).toBeTruthy();
  firstPaint.list = listTiming!;

  for (const view of ['graph', 'lanes'] as const) {
    await page.getByRole('tab', { name: view === 'graph' ? 'Flow graph' : 'Actor lanes' }).click();
    const panelName = view === 'graph' ? 'Flow graph' : 'Actor lanes';
    const panel = page.getByRole('tabpanel', { name: panelName });
    await expect(panel).toBeVisible();
    await expect(view === 'graph' ? panel.locator('[data-graph-node]').first() : panel.locator('.event-lane-row').first()).toBeVisible();
    let timing: RenderTime | undefined;
    await expect.poll(async () => {
      state = await readSnapshot(page);
      timing = state.renderTimes.filter(item => item.view === view && item.map === 2).at(-1);
      return timing;
    }, { timeout: 10_000 }).toBeTruthy();
    firstPaint[view] = timing!;
  }

  expect(firstPaint.graph?.instructions).toBe(2358);
  expect(firstPaint.lanes?.instructions).toBe(2358);
  console.log(`Map 2 first-render timings (disc 1): ${JSON.stringify(firstPaint)}`);
  expect(firstPaint.list!.ms).toBeLessThan(1000);
  expect(firstPaint.graph!.ms).toBeLessThan(2000);
  expect(firstPaint.lanes!.ms).toBeLessThan(1000);
});
