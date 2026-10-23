import { expect, test } from '@playwright/test';
import { readFileSync } from 'node:fs';

interface TraceEntry { request: unknown; response: unknown }
interface SceneFixture { schema_version: 1; synthetic_only: true; trace: TraceEntry[] }

async function send(page: import('@playwright/test').Page, request: unknown): Promise<unknown> {
  await expect.poll(() => page.evaluate(() =>
    typeof (window as unknown as { __xemStudioTest?: { command?: unknown } }).__xemStudioTest?.command === 'function'))
    .toBe(true);
  return page.evaluate((command) => {
    const driver = (window as unknown as { __xemStudioTest: { command(json: string): string } }).__xemStudioTest;
    return JSON.parse(driver.command(JSON.stringify(command))) as unknown;
  }, request);
}

test('replays the composed scene decode protocol and malformed ranges through wasm', async ({ page }) => {
  const fixturePath = process.env.XEM_STUDIO_SCENE_PARITY_FIXTURE;
  test.skip(!fixturePath, 'the native synthetic scene command target writes this parity fixture');
  const fixture = JSON.parse(readFileSync(fixturePath!, 'utf8')) as SceneFixture;
  expect(fixture.schema_version).toBe(1);
  expect(fixture.synthetic_only).toBe(true);
  expect(fixture.trace.length).toBeGreaterThan(2);

  await page.goto('/');
  await expect(page.locator('[data-panel-id="backend"]')).toContainText('xem-studio 0.5.0');
  for (const [index, step] of fixture.trace.entries()) {
    expect(await send(page, step.request), `native/WASM command envelope at step ${index}`).toEqual(step.response);
  }
});
