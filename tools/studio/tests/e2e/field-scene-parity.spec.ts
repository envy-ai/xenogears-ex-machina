import { expect, test } from '@playwright/test';
import { readFileSync } from 'node:fs';

interface TraceEntry { request: unknown; request_json: string; response: unknown }
interface SceneFixture { schema_version: 1; synthetic_only: true; trace: TraceEntry[] }

async function send(page: import('@playwright/test').Page, requestJson: string): Promise<unknown> {
  await expect.poll(() => page.evaluate(() =>
    typeof (window as unknown as { __xemStudioTest?: { command?: unknown } }).__xemStudioTest?.command === 'function'))
    .toBe(true);
  return page.evaluate((raw) => {
    const driver = (window as unknown as { __xemStudioTest: { command(json: string): string } }).__xemStudioTest;
    return JSON.parse(driver.command(raw)) as unknown;
  }, requestJson);
}

test('replays the composed scene decode protocol and malformed ranges through wasm', async ({ page }) => {
  const fixturePath = process.env.XEM_STUDIO_SCENE_PARITY_FIXTURE;
  test.skip(!fixturePath, 'the native synthetic scene command target writes this parity fixture');
  const fixture = JSON.parse(readFileSync(fixturePath!, 'utf8')) as SceneFixture;
  expect(fixture.schema_version).toBe(1);
  expect(fixture.synthetic_only).toBe(true);
  expect(fixture.trace.length).toBeGreaterThan(2);

  await page.goto('/');
  const backend = page.locator('[data-panel-id="backend"]');
  await expect(backend).toContainText('xem-studio 0.5.0');
  await expect(backend).toContainText(
    'Operations: version, echo, field.decode, field.scene.decode, field.scene.draft, field.encode, disc.open, disc.poll, disc.missing, disc.close, disc.maps, field.map_events, field.map_bundle, field.map_textures, field.structure, names.validate, field.lanes, disc.cue, package.build, package.inspect, package.bundle, field.draft.open, field.templates, field.instruction, field.draft.edit, field.draft.undo, field.draft.redo, field.draft.compile, field.draft.export, field.draft.import, field.draft.close, field.draft.snapshot, field.draft.repack',
  );
  for (const [index, step] of fixture.trace.entries()) {
    expect(typeof step.request_json).toBe('string');
    expect(await send(page, step.request_json), `native/WASM command envelope at step ${index}`).toEqual(step.response);
  }
});
