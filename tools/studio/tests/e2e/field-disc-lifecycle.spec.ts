import { expect, test, type Page } from '@playwright/test';
import type { StudioTestSnapshot } from './studio-test-driver';

const disc2 = process.env.XEM_STUDIO_DISC2;

interface FieldDiscLifecycleControl {
  holdNextRead(): void;
  readStarted(): boolean;
  releaseRead(): void;
  writeCount(): number;
  hasSavedProject(map: number): boolean;
}

declare global {
  interface Window { __fieldDiscLifecycle?: FieldDiscLifecycleControl }
}

test.skip(process.env.VITE_XEM_STUDIO_TEST !== '1', 'Real-disc lifecycle checks require the explicit test-only Studio driver.');
test.skip(!disc2, 'Set XEM_STUDIO_DISC2 to the authorized local disc 2 browser image.');

async function snapshot(page: Page): Promise<StudioTestSnapshot> {
  return page.evaluate(() => {
    const driver = window.__xemStudioTest;
    if (!driver) throw new Error('The explicit Studio test driver is unavailable.');
    return driver.snapshot();
  });
}

async function installOwnedFixtures(page: Page): Promise<void> {
  await page.addInitScript(() => {
    const names = JSON.stringify({ schema_version: 1, variables: {}, maps: {}, messages: {} });
    const stores = new Map<string, Map<string, string>>([
      ['folder-A', new Map([['names.json', names]])],
      ['folder-B', new Map([['names.json', names]])],
    ]);
    let writeAttempts = 0;
    const handle = (folderName: string, name: string) => ({
      name,
      kind: 'file' as const,
      async getFile() { return new File([stores.get(folderName)?.get(name) ?? ''], name); },
      async createWritable() {
        writeAttempts++;
        let pending = stores.get(folderName)?.get(name) ?? '';
        return {
          async write(data: Uint8Array) { pending = new TextDecoder().decode(data); },
          async close() { stores.get(folderName)?.set(name, pending); },
          async abort() {},
        };
      },
    });
    const folder = (name: string) => ({
      name,
      kind: 'directory' as const,
      async *values() { for (const entry of stores.get(name)!.keys()) yield handle(name, entry); },
      async getDirectoryHandle() { throw new Error('The lifecycle fixture has no subdirectories'); },
      async getFileHandle(fileName: string) {
        if (fileName !== 'names.json' && !/^field-map-\d+\.json$/.test(fileName)) throw new Error('Unexpected project file name');
        if (!stores.get(name)!.has(fileName)) stores.get(name)!.set(fileName, '');
        return handle(name, fileName);
      },
    });
    const folders = [folder('folder-A'), folder('folder-B')];
    let selectedFolder = 0;
    (window as typeof window & { showDirectoryPicker: () => Promise<ReturnType<typeof folder>> }).showDirectoryPicker = async () => folders[Math.min(selectedFolder++, folders.length - 1)]!;

    let armed = false;
    let started = false;
    let release: (() => void) | null = null;
    const fileName = new URLSearchParams(location.search).get('__lifecycle_file') ?? '';
    const slice = File.prototype.slice;
    File.prototype.slice = function (start?: number, end?: number, contentType?: string): Blob {
      const blob = slice.call(this, start, end, contentType);
      if (!armed || this.name !== fileName) return blob;
      armed = false;
      const read = blob.arrayBuffer.bind(blob);
      return new Proxy(blob, {
        get(target, property) {
          if (property === 'arrayBuffer') return async () => {
            started = true;
            await new Promise<void>(resolve => { release = resolve; });
            return read();
          };
          const value = Reflect.get(target, property, target) as unknown;
          return typeof value === 'function' ? value.bind(target) : value;
        },
      });
    };
    (window as typeof window & { __fieldDiscLifecycle: FieldDiscLifecycleControl }).__fieldDiscLifecycle = {
      holdNextRead() { armed = true; started = false; release = null; },
      readStarted() { return started; },
      releaseRead() { release?.(); },
      writeCount() { return writeAttempts; },
      hasSavedProject(map) { return [...stores.values()].some(files => files.has(`field-map-${map}.json`)); },
    };
  });
}

async function openDisc2(page: Page): Promise<number> {
  await expect(page.getByRole('main', { name: 'XEM Studio' })).toBeVisible();
  await page.getByRole('button', { name: 'Open folder…' }).click();
  await expect(page.locator('[data-studio-panel="project"]')).toContainText('folder-A/');
  await page.getByLabel('Open disc files').setInputFiles(disc2!);
  await expect.poll(async () => (await snapshot(page)).discs.map(item => item.disc)).toContain(2);
  const opened = (await snapshot(page)).discs.find(item => item.disc === 2);
  if (!opened) throw new Error('Disc 2 did not produce an identified browser session.');
  await page.getByRole('button', { name: new RegExp(`^Disc 2 · ${opened.mapCount} maps$`) }).click();
  return opened.id;
}

async function mapRow(page: Page, placeholder: boolean): Promise<number> {
  const map = await page.locator('.event-map-row').evaluateAll((rows, wantsPlaceholder) => {
    const row = rows.find(button => button.textContent?.includes('Placeholder') === wantsPlaceholder);
    return row ? Number((row as HTMLButtonElement).dataset.map) : null;
  }, placeholder);
  if (map === null || !Number.isSafeInteger(map)) throw new Error(`Disc 2 has no visible ${placeholder ? 'placeholder' : 'event'} map row.`);
  return map;
}

async function selectMap(page: Page, map: number): Promise<void> {
  await page.locator(`.event-map-row[data-map="${map}"]`).click();
  await expect.poll(async () => (await snapshot(page)).map, { timeout: 120_000, intervals: [250, 1000, 2500] }).toBe(map);
}

async function lifecycleControl(page: Page, operation: keyof FieldDiscLifecycleControl, map?: number): Promise<number | boolean> {
  return page.evaluate(({ operation, map }) => {
    const control = window.__fieldDiscLifecycle;
    if (!control) throw new Error('The owned browser lifecycle fixture is unavailable.');
    if (operation === 'hasSavedProject') return control.hasSavedProject(map!);
    if (operation === 'writeCount') return control.writeCount();
    throw new Error(`Unsupported lifecycle observation: ${operation}`);
  }, { operation, map });
}

test('clears the active draft for placeholders and close; Ctrl+S cannot rewrite the prior map', async ({ page }) => {
  await installOwnedFixtures(page);
  await page.goto('/');
  const discId = await openDisc2(page);
  const sourceMap = await mapRow(page, false);
  await selectMap(page, sourceMap);
  expect((await snapshot(page)).draft).not.toBeNull();
  await page.getByRole('button', { name: 'Save project' }).click();
  await expect(page.locator('.event-draft-toolbar')).toContainText('Saved');
  expect(await lifecycleControl(page, 'hasSavedProject', sourceMap)).toBe(true);
  const writesAfterSave = await lifecycleControl(page, 'writeCount') as number;

  const placeholder = await mapRow(page, true);
  await selectMap(page, placeholder);
  const placeholderState = await snapshot(page);
  expect(placeholderState.draft).toBeNull();
  expect(placeholderState.instructions).toBe(0);
  await page.locator('[data-studio-panel="events"]').press('Control+s');
  await page.waitForTimeout(250);
  expect(await lifecycleControl(page, 'writeCount')).toBe(writesAfterSave);
  expect(await lifecycleControl(page, 'hasSavedProject', sourceMap)).toBe(true);

  await selectMap(page, sourceMap);
  await page.getByRole('button', { name: `Close disc ${discId}` }).click();
  await expect.poll(async () => (await snapshot(page)).map).toBeNull();
  const closedState = await snapshot(page);
  expect(closedState.draft).toBeNull();
  expect(closedState.instructions).toBe(0);
  await page.locator('[data-studio-panel="events"]').press('Control+s');
  await page.waitForTimeout(250);
  expect(await lifecycleControl(page, 'writeCount')).toBe(writesAfterSave);
});

test('a delayed real map read cannot adopt into a project folder selected while loading', async ({ page }) => {
  await installOwnedFixtures(page);
  await page.goto(`/?__lifecycle_file=${encodeURIComponent(disc2!.split(/[\\/]/).at(-1)!)}`);
  const discId = await openDisc2(page);
  const originalMap = await mapRow(page, false);
  await selectMap(page, originalMap);
  const alternateMap = await page.locator('.event-map-row').evaluateAll((rows, current) => {
    const row = rows.find(button => !button.textContent?.includes('Placeholder') && Number((button as HTMLButtonElement).dataset.map) !== current);
    return row ? Number((row as HTMLButtonElement).dataset.map) : null;
  }, originalMap);
  if (alternateMap === null) throw new Error('Disc 2 needs a second real map to exercise delayed loading.');
  const before = await snapshot(page);
  expect(before.draft).not.toBeNull();

  await page.evaluate(() => window.__fieldDiscLifecycle?.holdNextRead());
  await page.locator(`.event-map-row[data-map="${alternateMap}"]`).click();
  await expect.poll(() => page.evaluate(() => window.__fieldDiscLifecycle?.readStarted() ?? false), { timeout: 30_000, intervals: [100, 250, 500] }).toBe(true);
  await expect(page.getByRole('button', { name: 'Insert node' })).toBeDisabled();
  await expect(page.getByRole('button', { name: 'Save project' })).toBeDisabled();
  const editor = page.getByRole('region', { name: 'Authored instruction editor' });
  await expect(editor).toBeVisible();
  await expect(editor.getByRole('button', { name: 'Cancel' })).toBeDisabled();
  const inlineFields = editor.locator('input,select');
  for (let index = 0; index < await inlineFields.count(); index++) await expect(inlineFields.nth(index)).toBeDisabled();

  await page.getByRole('button', { name: 'Open folder…' }).click();
  await expect(page.locator('[data-studio-panel="project"]')).toContainText('folder-B/');
  await page.evaluate(() => window.__fieldDiscLifecycle?.releaseRead());
  await expect(page.locator('.studio-errors[role="alert"]')).toContainText('project, source map or authored revision changed during the operation');

  const after = await snapshot(page);
  expect(after.map).toBe(originalMap);
  expect(after.instructions).toBe(before.instructions);
  expect(after.draft?.revision).toBe(before.draft?.revision);
  expect(JSON.stringify(after.draft?.bytes) === JSON.stringify(before.draft?.bytes), 'the retained source draft bytes remain unchanged').toBe(true);
  expect(after.discs.find(item => item.id === discId)?.disc).toBe(2);
});
