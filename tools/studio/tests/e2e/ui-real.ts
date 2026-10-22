import { mkdir, stat } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { chromium, expect as pwExpect } from '@playwright/test';
import { expect } from 'vitest';
import { preview } from 'vite';
import { studioGameServer } from '../../scripts/game-server.js';

const repoRoot = resolve(import.meta.dirname, '../../../..');
const artifactRoot = resolve(process.env.XEM_STUDIO_M4_ARTIFACT_ROOT ?? '');
const studioDist = resolve(process.env.XEM_STUDIO_DIST ?? '');
const runtimeDirectory = resolve(process.env.XEM_STUDIO_RUNTIME_DIR ?? '');
const discPath = resolve(process.env.XEM_STUDIO_DISC1 ?? '');
const chromiumPath = process.env.XEM_CHROMIUM;

export async function runRealGameUiReal(): Promise<void> {
  expect(artifactRoot).toContain(join(repoRoot, 'tmp/studio/milestone-4'));
  expect(studioDist).toContain(join(repoRoot, 'tmp/studio/milestone-4'));
  expect(runtimeDirectory).toContain(join(repoRoot, 'tmp/studio/milestone-4'));
  expect(discPath).toMatch(/\.(bin|chd)$/i);
  expect(chromiumPath).toBeTruthy();
  for (const path of [join(studioDist, 'index.html'), join(runtimeDirectory, 'index.html'), join(runtimeDirectory, 'game/game.wasm'), join(runtimeDirectory, 'game/schema.json'), discPath]) {
    expect((await stat(path)).isFile(), `required local input: ${path}`).toBe(true);
  }

  const runRoot = join(artifactRoot, `real-ui-${process.pid}-${Date.now()}`);
  await mkdir(runRoot, { recursive: false });
  const server = await preview({
    configFile: false,
    root: resolve(repoRoot, 'tools/studio'),
    plugins: [studioGameServer(runtimeDirectory)],
    build: { outDir: studioDist },
    preview: { host: '127.0.0.1', port: 0, strictPort: false },
    logLevel: 'error',
  });
  const address = server.httpServer.address();
  if (!address || typeof address === 'string') throw new Error('The isolated Studio preview has no TCP port');
  let browser: Awaited<ReturnType<typeof chromium.launch>> | undefined;
  let gamePage: import('@playwright/test').Page | undefined;
  try {
    browser = await chromium.launch({ executablePath: chromiumPath, headless: true, chromiumSandbox: true });
    const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
    await page.goto(`http://127.0.0.1:${address.port}/`);
    await pwExpect(page.getByRole('main', { name: 'XEM Studio' })).toBeVisible();
    await pwExpect(page.locator('[data-panel-id="backend"]')).toContainText(/xem-studio 0\.5\.0/);

    const popup = page.waitForEvent('popup');
    await page.getByRole('button', { name: 'Open game & attach' }).click();
    gamePage = await popup;
    await gamePage.waitForLoadState('domcontentloaded');
    await gamePage.waitForFunction(() => document.body.dataset.ready === 'true', undefined, { timeout: 120_000 });
    await gamePage.locator('#file').setInputFiles(discPath);
    await gamePage.waitForFunction(() => document.querySelector('#message')?.textContent?.startsWith('Xenogears disc 1 (') ?? false, undefined, { timeout: 120_000 });
    await gamePage.evaluate(async () => {
      const runtime = (window as unknown as { xem: {
        boot(options: { run: boolean; rewind: boolean }): Promise<unknown>;
        commandJson(json: string): string;
        run(command: { cmd: 'frames'; count: number }): Promise<unknown>;
      } }).xem;
      await runtime.boot({ run: false, rewind: false });
      for (const command of [
        { cmd: 'rewind_configure', max_bytes: 0 },
        { cmd: 'boot_mode', mode: 0 },
      ]) JSON.parse(runtime.commandJson(JSON.stringify(command)));
      await runtime.run({ cmd: 'frames', count: 303 });
      JSON.parse(runtime.commandJson(JSON.stringify({
        cmd: 'scenario', mode: 1,
        writes: [
          { path: 'game_data.map', value: 30 },
          { path: 'mode_kernel_menu_running', value: 0 },
          { path: 'mode_field_entered_once', value: 0 },
        ],
      })));
      await runtime.run({ cmd: 'frames', count: 400 });
    });

    await page.getByLabel('Open disc files').setInputFiles(discPath);
    await page.locator('[data-map="30"]').waitFor({ state: 'visible', timeout: 120_000 });
    await page.locator('[data-map="30"]').click();
    await page.getByRole('tabpanel', { name: 'Command list' }).waitFor({ state: 'visible' });

    // The first click creates the named same-origin game window; attach again
    // after the real game session has been booted and placed in map 30.
    await page.getByRole('button', { name: 'Open game & attach' }).click();
    const live = page.getByRole('region', { name: 'Live game' });
    await pwExpect(live.getByText(/browser · Paused · acknowledged/)).toBeVisible({ timeout: 30_000 });
    if (await page.getByRole('button', { name: 'Dismiss errors' }).count()) await page.getByRole('button', { name: 'Dismiss errors' }).click();

    const beforeStep = await gamePage.evaluate(() => (window as unknown as { xem: { status(): { session: { vblanks: number } } } }).xem.status().session.vblanks);
    await live.getByRole('button', { name: 'Step', exact: true }).click();
    await pwExpect.poll(() => gamePage!.evaluate(() => (window as unknown as { xem: { status(): { session: { vblanks: number } } } }).xem.status().session.vblanks)).toBe(beforeStep + 1);
    await live.getByRole('button', { name: 'Run', exact: true }).click();
    await pwExpect(live.getByText(/browser · Running · acknowledged/)).toBeVisible();
    await live.getByRole('button', { name: 'Pause', exact: true }).click();
    await pwExpect(live.getByText(/browser · Paused · acknowledged/)).toBeVisible();

    const mapButton = page.locator('[data-map="30"]');
    await pwExpect(mapButton).toHaveAttribute('aria-pressed', 'true');
    const jump = live.getByRole('button', { name: 'Jump to node' });
    await jump.click();
    await pwExpect(live.locator('.live-pc-status')).not.toContainText('Unavailable', { timeout: 30_000 });
    await pwExpect(page.locator('.event-command-row .docker-item-select[data-selected="true"]')).toHaveCount(1, { timeout: 30_000 });

    const inspector = page.getByRole('region', { name: 'Schema inspector' });
    await inspector.getByLabel('Filter globals').fill('mode_field_map_id');
    await inspector.locator('[data-schema-node="mode_field_map_id"]').click();
    await pwExpect(inspector.getByLabel('Inspected value')).toContainText('30');

    await live.getByLabel('Snapshot name').fill('m4-ui-before-write');
    await live.getByRole('button', { name: 'Capture' }).click();
    await live.getByLabel('Session snapshots').selectOption('m4-ui-before-write');

    const scalar = inspector.getByLabel('New scalar value');
    await scalar.fill('31');
    await live.getByRole('button', { name: 'Run', exact: true }).click();
    await live.getByText(/browser · Running · acknowledged/).waitFor({ state: 'visible' });
    await pwExpect(scalar).toBeDisabled();
    await live.getByRole('button', { name: 'Pause', exact: true }).click();
    await pwExpect(live.getByText(/browser · Paused · acknowledged/)).toBeVisible();
    await pwExpect(scalar).toHaveValue('');
    await pwExpect(inspector.getByRole('button', { name: 'Write & read back' })).toBeDisabled();
    await inspector.getByRole('button', { name: 'Refresh value' }).click();
    await pwExpect(inspector.getByLabel('Inspected value')).toContainText('30');
    await scalar.fill('31');
    await inspector.getByRole('button', { name: 'Write & read back' }).click();
    await pwExpect(inspector.getByRole('status')).toContainText('Read-back: 31');
    await live.getByLabel('Session snapshots').selectOption('m4-ui-before-write');
    await live.getByRole('button', { name: 'Restore…' }).click();
    const dialog = page.getByRole('dialog', { name: 'Restore session snapshot' });
    await dialog.getByRole('button', { name: 'Restore' }).click();
    await pwExpect(inspector.getByLabel('Inspected value')).toContainText('Refresh to inspect');
    await inspector.getByRole('button', { name: 'Refresh value' }).click();
    await pwExpect(inspector.getByLabel('Inspected value')).toContainText('30');

    await page.screenshot({ path: join(runRoot, 'studio-live-map30.png'), fullPage: false });
    await gamePage.screenshot({ path: join(runRoot, 'browser-game-map30.png'), fullPage: false });
    await import('node:fs/promises').then(({ writeFile }) => writeFile(join(runRoot, 'result.json'), `${JSON.stringify({ moduleBoundRuntime: true, disc: 'disc1', map: 30, liveUiAttached: true, pcJumpSelectedInstruction: true, staleDraftInvalidated: true, snapshotRestoredMap: 30 }, null, 2)}\n`, { flag: 'wx' }));
  } finally {
    if (gamePage) await gamePage.close().catch(() => {});
    if (browser) await browser.close().catch(() => {});
    await server.httpServer.close();
  }

}
