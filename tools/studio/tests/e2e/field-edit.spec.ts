import { expect, test, type Page } from '@playwright/test';
import type { StudioTestSnapshot } from './studio-test-driver';

interface FolderRaceControl {
  holdNextWrite(): void;
  writeStarted(): boolean;
  releaseWrite(): void;
  holdNextRead(): void;
  readStarted(): boolean;
  releaseRead(): void;
}

declare global {
  interface Window {
    __fieldEditFolderRace?: FolderRaceControl;
  }
}

function syntheticComponent(): { bytes: number[]; declaredSize: number } {
  const bytecodeOffset = 132 + 64;
  const code = [0x03, 0, 0, 1, 0x03, 0, 0, 2, 0]; // message_box, message_box, end_slot
  const bytes = new Array<number>(bytecodeOffset + code.length).fill(0);
  bytes[128] = 1;
  bytes[132 + 4 * 2] = 8; // event slot 4 is terminal-only
  bytes.splice(bytecodeOffset, code.length, ...code);
  return { bytes, declaredSize: bytes.length };
}

async function loadSyntheticComponent(page: Page): Promise<void> {
  await page.evaluate(async ({ bytes, declaredSize }) => {
    const driver = window.__xemStudioTest;
    if (!driver) throw new Error('The explicit synthetic Studio test driver is unavailable.');
    await driver.loadComponent(bytes, declaredSize, 0);
  }, syntheticComponent());
}

async function snapshot(page: Page): Promise<StudioTestSnapshot> {
  return page.evaluate(() => {
    const driver = window.__xemStudioTest;
    if (!driver) throw new Error('The explicit synthetic Studio test driver is unavailable.');
    return driver.snapshot();
  });
}

async function selectPc(page: Page, pc: number): Promise<void> {
  await page.locator(`.event-command-row[data-pc="${pc}"] button`).click();
}

async function expectOperands(page: Page, values: string): Promise<void> {
  const detail = page.locator('.event-instruction-detail');
  const toggle = detail.getByRole('button', { name: /Show raw bytes|Hide raw bytes/ });
  if ((await toggle.textContent())?.startsWith('Show')) await toggle.click();
  await expect(detail.locator('pre')).toContainText(`Operands: ${values}`);
}

async function ready(page: Page): Promise<void> {
  await expect(page.getByRole('main', { name: 'XEM Studio' })).toBeVisible();
  await expect(page.locator('[data-panel-id="backend"]')).toContainText('xem-studio 0.5.0');
}

test.skip(process.env.VITE_XEM_STUDIO_TEST !== '1', 'Authored field editor journeys require the explicit synthetic Studio test driver.');

test.beforeEach(async ({ page }) => {
  await page.goto('/');
  await ready(page);
  await loadSyntheticComponent(page);
});

test('edits inline fields, guards selection, and keeps list, graph, insert and history synchronized', async ({ page }) => {
  const editor = page.getByRole('region', { name: 'Authored instruction editor' });
  const operand = editor.getByRole('textbox', { name: 'Operand 2 · u8' });
  await expect(operand).toHaveValue('1');

  await operand.fill('9');
  await selectPc(page, 4);
  expect((await snapshot(page)).selection.pc).toBe(0);
  await expect(page.locator('.event-draft-diagnostic')).toContainText('Apply or Cancel the inline fields before changing selection or view');
  await editor.getByRole('button', { name: 'Cancel' }).click();
  await expect(operand).toHaveValue('1');

  await operand.fill('7');
  await editor.getByRole('button', { name: 'Apply' }).click();
  await expect.poll(async () => (await snapshot(page)).selection.pc).toBe(0);
  await expectOperands(page, '[0,7]');
  await expect(page.getByRole('button', { name: 'Undo', exact: true })).toBeEnabled();
  await page.getByRole('button', { name: 'Undo', exact: true }).click();
  await expectOperands(page, '[0,1]');
  await expect(page.getByRole('button', { name: 'Redo', exact: true })).toBeEnabled();
  await page.getByRole('button', { name: 'Redo', exact: true }).click();
  await expectOperands(page, '[0,7]');

  await editor.getByRole('textbox', { name: 'Operand 2 · u8' }).fill('256');
  await editor.getByRole('button', { name: 'Apply' }).click();
  await expect(editor.getByRole('alert')).toContainText('operand exceeds its encoded width');
  await expectOperands(page, '[0,7]');
  await editor.getByRole('button', { name: 'Cancel' }).click();

  await page.getByRole('tab', { name: 'Flow graph' }).click();
  await expect(page.locator('[data-graph-node="0"][data-selected="true"]')).toBeVisible();
  expect((await snapshot(page)).selection.pc).toBe(0);
  await page.getByRole('tab', { name: 'Command list' }).click();
  await selectPc(page, 4);
  const beforeInsert = await snapshot(page);
  await page.getByRole('button', { name: 'Insert node' }).click();
  const insertionEditor = page.getByRole('region', { name: 'Authored instruction editor' });
  await expect(insertionEditor.getByLabel('Opcode · supported standalone form')).toBeVisible();
  await insertionEditor.getByRole('button', { name: 'Apply' }).click();
  const afterInsert = await snapshot(page);
  const insertedId = Object.keys(afterInsert.draft?.bindings ?? {}).find(id =>
    id.startsWith('new:') && beforeInsert.draft?.bindings[id] === undefined);
  expect(insertedId, 'the insertion creates a stable authored node identity').toBeDefined();
  const insertedPc = afterInsert.draft!.bindings[insertedId!];
  expect(afterInsert.draft!.graph.instructions.find(instruction => instruction.pc === insertedPc)?.spec.mnemonic)
    .toBe('nop');
  await page.getByRole('button', { name: 'Undo', exact: true }).click();
  await expect.poll(async () => (await snapshot(page)).draft?.bytes).toEqual(beforeInsert.draft?.bytes);
  expect((await snapshot(page)).draft?.graph).toEqual(beforeInsert.draft?.graph);
  expect((await snapshot(page)).draft?.bindings).toEqual(beforeInsert.draft?.bindings);
  await page.getByRole('button', { name: 'Redo', exact: true }).click();
  await expect.poll(async () => (await snapshot(page)).draft?.bytes).toEqual(afterInsert.draft?.bytes);
  expect((await snapshot(page)).draft?.graph).toEqual(afterInsert.draft?.graph);
  expect((await snapshot(page)).draft?.bindings).toEqual(afterInsert.draft?.bindings);
});

test('Alt+Left/Right keep navigation history and inline fields owned until Apply or Cancel', async ({ page }) => {
  const find = page.getByLabel('Find within map');
  await find.fill('@0004');
  await find.press('Enter');
  await expect.poll(async () => (await snapshot(page)).selection.pc).toBe(4);

  const editor = page.getByRole('region', { name: 'Authored instruction editor' });
  const operand = editor.getByRole('textbox', { name: 'Operand 2 · u8' });
  await expect(operand).toHaveValue('2');
  await operand.fill('9');
  await page.keyboard.press('Alt+ArrowLeft');
  expect((await snapshot(page)).selection.pc).toBe(4);
  await expect(operand).toHaveValue('9');
  await expect(page.locator('.event-draft-diagnostic')).toContainText('Apply or Cancel the inline fields before changing selection or view');

  await editor.getByRole('button', { name: 'Cancel' }).click();
  await page.keyboard.press('Alt+ArrowLeft');
  await expect.poll(async () => (await snapshot(page)).selection.pc).toBe(0);

  const firstOperand = editor.getByRole('textbox', { name: 'Operand 2 · u8' });
  await expect(firstOperand).toHaveValue('1');
  await firstOperand.fill('7');
  await page.keyboard.press('Alt+ArrowRight');
  expect((await snapshot(page)).selection.pc).toBe(0);
  await expect(firstOperand).toHaveValue('7');
  await expect(page.locator('.event-draft-diagnostic')).toContainText('Apply or Cancel the inline fields before changing selection or view');

  await editor.getByRole('button', { name: 'Cancel' }).click();
  await page.keyboard.press('Alt+ArrowRight');
  await expect.poll(async () => (await snapshot(page)).selection.pc).toBe(4);
  await expectOperands(page, '[0,2]');
});

test('creates an event in an existing terminal-only actor slot', async ({ page }) => {
  const before = await snapshot(page);
  await page.getByRole('button', { name: 'New event' }).click();
  const editor = page.getByRole('region', { name: 'Authored instruction editor' });
  await editor.getByRole('textbox', { name: 'Unused or terminal-only event slot (0–31)' }).fill('4');
  await editor.getByRole('button', { name: 'Apply' }).click();
  const after = await snapshot(page);
  expect(Object.keys(after.draft?.bindings ?? {}).some(id => id.startsWith('new:'))).toBe(true);
  expect(after.draft?.graph.entries).toContainEqual(expect.objectContaining({ actor: 0, event: 4 }));
  expect(after.draft?.bytes).not.toEqual(before.draft?.bytes);

  const event = page.getByRole('treeitem', { name: /^04 ·/ });
  await expect(event).toBeVisible();
  await event.click();
  expect((await snapshot(page)).selection.event).toBe(4);
  await expect(page.getByRole('list', { name: 'Event commands' })).toContainText(/nop/i);
});

test('discards delayed project reads and writes after the selected folder changes', async ({ page }) => {
  await page.addInitScript(() => {
    const emptyNames = JSON.stringify({ schema_version: 1, variables: {}, maps: {}, messages: {} });
    const stores = new Map<string, Map<string, string>>([
      ['folder-A', new Map([['names.json', emptyNames]])],
      ['folder-B', new Map([['names.json', emptyNames]])],
    ]);
    const control = {
      delayWrite: false, writeIsStarted: false, writeRelease: null as (() => void) | null,
      delayRead: false, readIsStarted: false, readRelease: null as (() => void) | null,
    };
    const handle = (folder: string, name: string) => ({
      name, kind: 'file' as const,
      async getFile() {
        if (name === 'field-map-0.json' && control.delayRead) {
          control.readIsStarted = true;
          await new Promise<void>(resolve => { control.readRelease = resolve; });
          control.delayRead = false;
        }
        return new File([stores.get(folder)?.get(name) ?? ''], name);
      },
      async createWritable() {
        let content = '';
        return {
          async write(data: Uint8Array) { content = new TextDecoder().decode(data); },
          async close() {
            if (name === 'field-map-0.json' && control.delayWrite) {
              control.writeIsStarted = true;
              await new Promise<void>(resolve => { control.writeRelease = resolve; });
              control.delayWrite = false;
            }
            stores.get(folder)?.set(name, content);
          },
          async abort() {},
        };
      },
    });
    const directory = (name: string) => ({
      name, kind: 'directory' as const,
      async *values() { yield handle(name, 'names.json'); },
      async getDirectoryHandle() { throw new Error('No synthetic subdirectory'); },
      async getFileHandle(fileName: string) { return handle(name, fileName); },
    });
    const selected = [directory('folder-A'), directory('folder-B'), directory('folder-A')];
    let next = 0;
    (window as typeof window & { showDirectoryPicker: () => Promise<ReturnType<typeof directory>> }).showDirectoryPicker = async () => selected[Math.min(next++, selected.length - 1)]!;
    (window as typeof window & { __fieldEditFolderRace: unknown }).__fieldEditFolderRace = {
      holdNextWrite() { control.delayWrite = true; control.writeIsStarted = false; },
      writeStarted() { return control.writeIsStarted; },
      releaseWrite() { control.writeRelease?.(); },
      holdNextRead() { control.delayRead = true; control.readIsStarted = false; },
      readStarted() { return control.readIsStarted; },
      releaseRead() { control.readRelease?.(); },
    };
  });
  await page.reload();
  await ready(page);
  await loadSyntheticComponent(page);
  const folderPanel = page.locator('[data-studio-panel="project"]');
  const openFolder = page.getByRole('button', { name: 'Open folder…' });
  await openFolder.click();
  await expect(folderPanel).toContainText('folder-A/');

  const editor = page.getByRole('region', { name: 'Authored instruction editor' });
  await editor.getByRole('textbox', { name: 'Operand 2 · u8' }).fill('7');
  await editor.getByRole('button', { name: 'Apply' }).click();
  await page.evaluate(() => window.__fieldEditFolderRace?.holdNextWrite());
  await page.getByRole('button', { name: 'Save project' }).click();
  await page.waitForFunction(() => window.__fieldEditFolderRace?.writeStarted());
  await openFolder.click();
  await expect(folderPanel).toContainText('folder-B/');
  await page.evaluate(() => window.__fieldEditFolderRace?.releaseWrite());
  await expect(page.locator('.event-draft-diagnostic')).toContainText('The project changed while saving; the previous project file was saved, current draft remains unsaved');
  await expectOperands(page, '[0,7]');

  await page.getByRole('button', { name: 'Save project' }).click();
  await expect(page.locator('.event-draft-toolbar')).toContainText('Saved');
  await page.getByRole('button', { name: 'Undo', exact: true }).click();
  await expectOperands(page, '[0,1]');
  const reverted = await snapshot(page);
  await page.getByRole('button', { name: 'Load project' }).click();
  await expect(page.locator('.event-draft-diagnostic')).toContainText('Save the authored project or explicitly discard its draft before changing maps');
  expect((await snapshot(page)).draft?.bytes).toEqual(reverted.draft?.bytes);
  expect((await snapshot(page)).draft?.graph).toEqual(reverted.draft?.graph);
  await page.getByRole('button', { name: 'Save project' }).click();
  await expect(page.locator('.event-draft-toolbar')).toContainText('Saved');
  await page.evaluate(() => window.__fieldEditFolderRace?.holdNextRead());
  await page.getByRole('button', { name: 'Load project' }).click();
  await page.waitForFunction(() => window.__fieldEditFolderRace?.readStarted());
  await openFolder.click();
  await expect(folderPanel).toContainText('folder-A/');
  await page.evaluate(() => window.__fieldEditFolderRace?.releaseRead());
  await expect(page.locator('.event-draft-diagnostic')).toContainText('project, source map or authored revision changed during the operation');
  await expectOperands(page, '[0,1]');
  expect((await snapshot(page)).selection.pc).toBe(0);
});
