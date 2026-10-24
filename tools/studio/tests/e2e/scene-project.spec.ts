import { expect, test, type Page } from '@playwright/test';
import { readFileSync } from 'node:fs';
import type { CompiledDraft } from '../../src/backend/drafts';
import type { SceneInput, SceneSnapshot } from '../../src/backend/scenes';

interface TraceEntry {
  request_json: string;
  response: { ok?: { state?: { events?: CompiledDraft } } };
}
interface SceneTrace { schema_version: number; synthetic_only: boolean; trace: TraceEntry[] }
interface SceneProjectSeed { input: SceneInput; entrances: number[]; events: CompiledDraft }
interface SceneProjectSnapshot {
  view: string;
  map: number | null;
  selection: { actor: number; event: number; pc: number | null };
  scene: SceneSnapshot | null;
  draft: { revision: number; bindings: Record<string, number>; graph: unknown; bytes: number[] } | null;
}
interface SceneProjectDriver {
  command(json: string): string;
  loadComponent(bytes: number[], declaredSize: number, map?: number): Promise<void>;
  loadScene(input: SceneInput, entrances?: number[], map?: number, promoteExistingEvents?: boolean): Promise<void>;
  snapshot(): SceneProjectSnapshot;
}
interface SceneProjectFolderControl {
  holdNextWrite(): void;
  writeStarted(): boolean;
  releaseWrite(): void;
  holdNextRead(): void;
  readStarted(): boolean;
  releaseRead(): void;
  readFile(folder: string, name: string): string | null;
}

declare global {
  interface Window { __sceneProjectFolderControl?: SceneProjectFolderControl }
}

function sceneProjectSeed(): SceneProjectSeed {
  const fixturePath = process.env.XEM_STUDIO_SCENE_UI_FIXTURE;
  if (!fixturePath) throw new Error('XEM_STUDIO_SCENE_UI_FIXTURE is required for the synthetic Scene project test');
  const trace = JSON.parse(readFileSync(fixturePath, 'utf8')) as SceneTrace;
  if (trace.schema_version !== 1 || trace.synthetic_only !== true) throw new Error('Scene project fixture must be the synthetic native scene trace');
  for (const step of trace.trace) {
    const request = JSON.parse(step.request_json) as { op?: string; args?: { kind?: string; input?: SceneInput; entrances?: number[] } };
    if (request.op !== 'field.scene.draft' || request.args?.kind !== 'open' || !request.args.input) continue;
    const events = step.response.ok?.state?.events;
    if (!events) continue;
    return { input: request.args.input, entrances: request.args.entrances ?? [], events };
  }
  throw new Error('Synthetic trace has no successful Scene open with a source event component');
}

async function snapshot(page: Page): Promise<SceneProjectSnapshot> {
  return page.evaluate(() => {
    const hook = (window as unknown as { __xemStudioTest?: SceneProjectDriver }).__xemStudioTest;
    if (!hook) throw new Error('The explicit synthetic Studio test driver is unavailable');
    return hook.snapshot();
  });
}

async function sceneSnapshot(page: Page): Promise<SceneSnapshot> {
  const value = await snapshot(page);
  if (!value.scene) throw new Error('The synthetic source-owned Scene is not open');
  return value.scene;
}

async function waitReady(page: Page): Promise<void> {
  await page.goto('/');
  await expect(page.getByRole('main', { name: 'XEM Studio' })).toBeVisible();
  await expect(page.locator('[data-panel-id="backend"]')).toContainText('xem-studio 0.5.0');
  await expect.poll(() => page.evaluate(() => typeof (window as Window & { __xemStudioTest?: SceneProjectDriver }).__xemStudioTest?.loadScene)).toBe('function');
}

async function loadM6AndScene(page: Page, seed: SceneProjectSeed, promoteExistingEvents: boolean): Promise<void> {
  await page.evaluate(async ({ bytes, declaredSize, input, entrances, promote }) => {
    const hook = (window as Window & { __xemStudioTest?: SceneProjectDriver }).__xemStudioTest;
    if (!hook) throw new Error('The explicit synthetic Studio test driver is unavailable');
    await hook.loadComponent(bytes, declaredSize, 23);
    await hook.loadScene(input, entrances, 23, promote);
  }, { bytes: seed.events.bytes, declaredSize: seed.events.declared_size, input: seed.input, entrances: seed.entrances, promote: promoteExistingEvents });
  await expect(page.getByRole('tabpanel', { name: 'Scene' })).toBeVisible();
}

async function loadM6Component(page: Page, seed: SceneProjectSeed): Promise<void> {
  await page.evaluate(async ({ bytes, declaredSize }) => {
    const hook = (window as Window & { __xemStudioTest?: SceneProjectDriver }).__xemStudioTest;
    if (!hook) throw new Error('The explicit synthetic Studio test driver is unavailable');
    await hook.loadComponent(bytes, declaredSize, 23);
  }, { bytes: seed.events.bytes, declaredSize: seed.events.declared_size });
}

async function openFolder(page: Page, expected: string): Promise<void> {
  await page.getByRole('button', { name: 'Open folder…' }).click();
  await expect(page.locator('[data-studio-panel="project"]')).toContainText(`${expected}/`);
}

async function readProject(page: Page, folder: string): Promise<Record<string, unknown> | null> {
  return page.evaluate(name => {
    const control = (window as Window & { __sceneProjectFolderControl?: SceneProjectFolderControl }).__sceneProjectFolderControl;
    const raw = control?.readFile(name, 'field-map-23.json');
    return raw === null || raw === undefined ? null : JSON.parse(raw) as Record<string, unknown>;
  }, folder);
}

async function waitForSaved(page: Page): Promise<void> {
  await expect(page.locator('.event-draft-toolbar')).toContainText('Saved');
}

async function installProjectFolders(page: Page, pickerOrder: string[]): Promise<void> {
  await page.addInitScript(order => {
    const filesKey = 'm7-scene-project-files-v1';
    const pickerKey = 'm7-scene-project-picker-order-v1';
    const pickerIndexKey = 'm7-scene-project-picker-index-v1';
    const names = JSON.stringify({ schema_version: 1, variables: {}, maps: {}, messages: {} });
    if (localStorage.getItem(filesKey) === null) {
      localStorage.setItem(filesKey, JSON.stringify(Object.fromEntries(order.map(folder => [folder, { 'names.json': names }]))));
    }
    if (localStorage.getItem(pickerKey) === null) localStorage.setItem(pickerKey, JSON.stringify(order));
    if (localStorage.getItem(pickerIndexKey) === null) localStorage.setItem(pickerIndexKey, '0');

    const control = {
      delayWrite: false, writeIsStarted: false, writeRelease: null as (() => void) | null,
      delayRead: false, readIsStarted: false, readRelease: null as (() => void) | null,
    };
    const readStore = () => JSON.parse(localStorage.getItem(filesKey) ?? '{}') as Record<string, Record<string, string>>;
    const writeStore = (store: Record<string, Record<string, string>>) => localStorage.setItem(filesKey, JSON.stringify(store));
    const fileHandle = (folder: string, name: string) => ({
      name, kind: 'file' as const,
      async getFile() {
        if (name === 'field-map-23.json' && control.delayRead) {
          control.readIsStarted = true;
          await new Promise<void>(resolve => { control.readRelease = resolve; });
          control.delayRead = false;
        }
        return new File([readStore()[folder]?.[name] ?? ''], name);
      },
      async createWritable() {
        let content = '';
        return {
          async write(data: Uint8Array) { content = new TextDecoder().decode(data); },
          async close() {
            if (name === 'field-map-23.json' && control.delayWrite) {
              control.writeIsStarted = true;
              await new Promise<void>(resolve => { control.writeRelease = resolve; });
              control.delayWrite = false;
            }
            const store = readStore();
            store[folder] ??= {};
            store[folder]![name] = content;
            writeStore(store);
          },
          async abort() {},
        };
      },
    });
    const directoryHandle = (name: string) => ({
      name, kind: 'directory' as const,
      async *values() {
        for (const fileName of Object.keys(readStore()[name] ?? {})) yield fileHandle(name, fileName);
      },
      async getDirectoryHandle() { throw new Error('The synthetic Scene project has no subdirectories'); },
      async getFileHandle(fileName: string, options?: { create?: boolean }) {
        const store = readStore();
        if (!store[name]?.[fileName] && !options?.create) throw new Error(`Synthetic project file not found: ${fileName}`);
        return fileHandle(name, fileName);
      },
    });
    Object.defineProperty(window, 'showDirectoryPicker', {
      configurable: true,
      value: async () => {
        const selected = JSON.parse(localStorage.getItem(pickerKey) ?? '[]') as string[];
        const index = Number(localStorage.getItem(pickerIndexKey) ?? '0');
        localStorage.setItem(pickerIndexKey, String(index + 1));
        return directoryHandle(selected[Math.min(index, selected.length - 1)] ?? order[0]!);
      },
    });
    (window as Window & { __sceneProjectFolderControl?: SceneProjectFolderControl }).__sceneProjectFolderControl = {
      holdNextWrite() { control.delayWrite = true; control.writeIsStarted = false; },
      writeStarted() { return control.writeIsStarted; },
      releaseWrite() { control.writeRelease?.(); },
      holdNextRead() { control.delayRead = true; control.readIsStarted = false; },
      readStarted() { return control.readIsStarted; },
      releaseRead() { control.readRelease?.(); },
      readFile(folder: string, fileName: string) { return readStore()[folder]?.[fileName] ?? null; },
    };
  }, pickerOrder);
}

async function createM6Event(page: Page, event: number): Promise<void> {
  await page.getByRole('button', { name: 'New event' }).click();
  const editor = page.getByRole('region', { name: 'Authored instruction editor' });
  await editor.getByRole('textbox', { name: 'Unused or terminal-only event slot (0–31)' }).fill(String(event));
  await expect(editor.getByRole('button', { name: 'Apply' })).toBeEnabled();
  await editor.getByRole('button', { name: 'Apply' }).click();
}

function sceneWithoutRevision(scene: SceneSnapshot): Omit<SceneSnapshot, 'revision'> {
  const { revision: _revision, ...rest } = scene;
  return rest;
}

test.skip(process.env.VITE_XEM_STUDIO_TEST !== '1', 'Scene project lifecycle requires the explicit synthetic Studio test driver.');

test('Scene projects save imported M6 history, preserve redo and allocator IDs, and reload the same owner', async ({ page }) => {
  test.setTimeout(90_000);
  const seed = sceneProjectSeed();
  await installProjectFolders(page, ['scene-project-A', 'scene-project-A', 'scene-project-A']);
  await waitReady(page);
  await loadM6Component(page, seed);

  // Start from the exact event bytes and size carried by the synthetic Scene source.
  // The real M6 editor creates two history actions; undo leaves the second action redoable.
  await page.getByRole('tab', { name: 'Command list' }).click();
  await createM6Event(page, 1);
  await createM6Event(page, 2);
  await page.getByRole('button', { name: 'Undo', exact: true }).click();
  const m6BeforePromotion = await snapshot(page);
  expect(m6BeforePromotion.draft?.bindings).toHaveProperty('new:1');
  expect(m6BeforePromotion.draft?.bindings).toHaveProperty('new:2');
  expect(m6BeforePromotion.draft?.bindings).not.toHaveProperty('new:3');
  expect(m6BeforePromotion.draft?.bindings).not.toHaveProperty('new:4');

  await page.evaluate(async ({ input, entrances }) => {
    const hook = (window as Window & { __xemStudioTest?: SceneProjectDriver }).__xemStudioTest;
    if (!hook) throw new Error('The explicit synthetic Studio test driver is unavailable');
    await hook.loadScene(input, entrances, 23, true);
  }, { input: seed.input, entrances: seed.entrances });
  const promoted = await sceneSnapshot(page);
  expect(promoted.next_event_id).toBe(5);
  expect(promoted.events.bindings).toHaveProperty('new:1');
  expect(promoted.events.bindings).toHaveProperty('new:2');
  expect(promoted.events.bindings).not.toHaveProperty('new:3');
  expect(promoted.can_redo).toBe(true);

  await openFolder(page, 'scene-project-A');
  await page.getByRole('button', { name: 'Save project' }).click();
  await waitForSaved(page);
  const firstSaved = await readProject(page, 'scene-project-A') as {
    schema_version: number; map: number; document: Record<string, unknown>;
  } | null;
  expect(firstSaved).not.toBeNull();
  expect(firstSaved!.schema_version).toBe(2);
  expect(firstSaved!.map).toBe(23);
  const firstDocument = firstSaved!.document as {
    schema_version: number; event_document: { schema_version: number; actions: unknown[]; cursor: number; next_id: number; allocation_starts: number[] };
    actions: unknown[]; cursor: number; imported_event_actions: number; next_event_id: number;
  };
  expect(firstDocument.schema_version).toBe(4);
  expect(firstDocument.event_document).toMatchObject({ schema_version: 1, actions: [], cursor: 0, allocation_starts: [], next_id: 1 });
  expect(firstDocument.imported_event_actions).toBe(2);
  expect(firstDocument.actions).toHaveLength(2);
  expect(firstDocument.cursor).toBe(1);
  expect(firstDocument.next_event_id).toBe(5);

  // A fresh application owner imports the project through the visible Load control.
  await page.reload();
  await waitReady(page);
  await loadM6AndScene(page, seed, false);
  await openFolder(page, 'scene-project-A');
  await page.getByRole('button', { name: 'Load project' }).click();
  await expect(page.locator('.event-draft-toolbar')).toContainText('Saved');
  const loadedAtCursor = await sceneSnapshot(page);
  expect(loadedAtCursor.events.bytes).toEqual(promoted.events.bytes);
  expect(loadedAtCursor.events.bindings).toEqual(promoted.events.bindings);
  expect(loadedAtCursor.next_event_id).toBe(5);
  expect(loadedAtCursor.can_redo).toBe(true);

  // Undo/redo traverses the imported shared M6 prefix, including its retained redo action.
  await page.getByRole('button', { name: 'Undo', exact: true }).click();
  const undoneImported = await sceneSnapshot(page);
  expect(undoneImported.events.bindings).not.toHaveProperty('new:1');
  expect(undoneImported.events.bindings).not.toHaveProperty('new:2');
  expect(undoneImported.can_redo).toBe(true);
  await page.getByRole('button', { name: 'Redo', exact: true }).click();
  const redonePrefix = await sceneSnapshot(page);
  expect(redonePrefix.events.bytes).toEqual(loadedAtCursor.events.bytes);
  expect(redonePrefix.events.bindings).toEqual(loadedAtCursor.events.bindings);
  await page.getByRole('button', { name: 'Redo', exact: true }).click();
  const redoneImported = await sceneSnapshot(page);
  expect(redoneImported.events.bindings).toHaveProperty('new:3');
  expect(redoneImported.events.bindings).toHaveProperty('new:4');
  expect(redoneImported.next_event_id).toBe(5);

  // Undo to the saved cursor and branch through the actual script editor. The abandoned
  // imported identities remain reserved, so the alternate instruction must be new:5.
  await page.getByRole('button', { name: 'Undo', exact: true }).click();
  await page.getByRole('tab', { name: 'Command list' }).click();
  await page.getByRole('treeitem', { name: /^01 ·/ }).click();
  const branchBase = await snapshot(page);
  const insertionAnchor = branchBase.scene?.events.bindings['new:1'];
  expect(insertionAnchor).toBeDefined();
  await page.locator(`.event-command-row[data-pc="${insertionAnchor}"] button`).click();
  await page.getByRole('button', { name: 'Insert node' }).click();
  const insertion = page.getByRole('region', { name: 'Authored instruction editor' });
  await expect(insertion.getByRole('button', { name: 'Apply' })).toBeEnabled();
  await insertion.getByRole('button', { name: 'Apply' }).click();
  const alternate = await sceneSnapshot(page);
  expect(alternate.events.bindings).toHaveProperty('new:1');
  expect(alternate.events.bindings).toHaveProperty('new:2');
  expect(alternate.events.bindings).toHaveProperty('new:5');
  expect(alternate.events.bindings).not.toHaveProperty('new:3');
  expect(alternate.events.bindings).not.toHaveProperty('new:4');
  expect(alternate.next_event_id).toBe(6);

  // Canceling inline fields leaves the adopted Scene owner untouched; Apply changes only
  // the selected zone while shared event history and stable IDs remain intact.
  await page.getByRole('tab', { name: 'Scene', exact: true }).click();
  await page.locator('[data-scene-id="zone:0"]').click();
  const beforeInline = await sceneSnapshot(page);
  const cornerX = page.getByRole('spinbutton', { name: 'Corner 0 X' });
  const sourceX = beforeInline.zones[0]!.value.corners[0]![0]!;
  await cornerX.fill(String(sourceX + 1));
  await page.getByRole('button', { name: 'Cancel scene fields' }).click();
  expect(await sceneSnapshot(page)).toEqual(beforeInline);
  await cornerX.fill(String(sourceX + 2));
  await page.getByRole('button', { name: 'Apply scene fields' }).click();
  const afterInline = await sceneSnapshot(page);
  expect(afterInline.zones[0]!.value.corners[0]![0]).toBe(sourceX + 2);
  expect(afterInline.events).toEqual(beforeInline.events);
  expect(afterInline.next_event_id).toBe(beforeInline.next_event_id);

  await page.getByRole('button', { name: 'Save project' }).click();
  await waitForSaved(page);
  const savedScene = await sceneSnapshot(page);
  const finalSaved = await readProject(page, 'scene-project-A') as {
    schema_version: number; document: { schema_version: number; event_document: { actions: unknown[]; cursor: number; next_id: number }; actions: unknown[]; cursor: number; next_event_id: number };
  } | null;
  expect(finalSaved?.schema_version).toBe(2);
  expect(finalSaved?.document.schema_version).toBe(4);
  expect(finalSaved?.document.event_document).toMatchObject({ actions: [], cursor: 0, next_id: 1 });
  expect(finalSaved?.document.next_event_id).toBe(6);

  await page.reload();
  await waitReady(page);
  await loadM6AndScene(page, seed, false);
  await openFolder(page, 'scene-project-A');
  await page.getByRole('button', { name: 'Load project' }).click();
  const reloaded = await sceneSnapshot(page);
  expect(sceneWithoutRevision(reloaded)).toEqual(sceneWithoutRevision(savedScene));
  expect(reloaded.events.bindings).toHaveProperty('new:5');
  expect(reloaded.events.bindings).not.toHaveProperty('new:3');
  expect(reloaded.events.bindings).not.toHaveProperty('new:4');
});

test('Scene project delayed reads and writes cannot adopt across folder or source ownership changes', async ({ page }) => {
  test.setTimeout(60_000);
  const seed = sceneProjectSeed();
  await installProjectFolders(page, ['scene-project-A', 'scene-project-B', 'scene-project-A']);
  await waitReady(page);
  await loadM6AndScene(page, seed, false);
  await openFolder(page, 'scene-project-A');

  await page.locator('[data-scene-id="zone:0"]').click();
  const cornerX = page.getByRole('spinbutton', { name: 'Corner 0 X' });
  const sourceX = (await sceneSnapshot(page)).zones[0]!.value.corners[0]![0]!;
  await cornerX.fill(String(sourceX + 9));
  await page.getByRole('button', { name: 'Apply scene fields' }).click();
  const unsavedOwner = await sceneSnapshot(page);
  await page.evaluate(() => window.__sceneProjectFolderControl?.holdNextWrite());
  await page.getByRole('button', { name: 'Save project' }).click();
  await page.waitForFunction(() => window.__sceneProjectFolderControl?.writeStarted());

  // Changing the selected browser folder while its previous write is pending must not
  // mark that old-folder write as the current Scene project's saved revision.
  await openFolder(page, 'scene-project-B');
  await page.evaluate(() => window.__sceneProjectFolderControl?.releaseWrite());
  await expect(page.locator('.studio-errors')).toContainText('The project changed while saving; the previous project file was saved, current draft remains unsaved');
  expect(await sceneSnapshot(page)).toEqual(unsavedOwner);
  expect(await readProject(page, 'scene-project-A')).not.toBeNull();
  expect(await readProject(page, 'scene-project-B')).toBeNull();

  await page.getByRole('button', { name: 'Save project' }).click();
  await waitForSaved(page);
  const savedInB = await sceneSnapshot(page);
  expect((await readProject(page, 'scene-project-B'))?.schema_version).toBe(2);

  await page.evaluate(() => window.__sceneProjectFolderControl?.holdNextRead());
  await page.getByRole('button', { name: 'Load project' }).click();
  await page.waitForFunction(() => window.__sceneProjectFolderControl?.readStarted());

  // The test-only source replacement call must meet the normal pending-operation guard;
  // no Scene handle or map is adopted while the visible project read is outstanding.
  const blockedSourceChange = await page.evaluate(async input => {
    const hook = (window as Window & { __xemStudioTest?: SceneProjectDriver }).__xemStudioTest;
    if (!hook) throw new Error('The explicit synthetic Studio test driver is unavailable');
    try { await hook.loadScene(input, [], 24, false); return null; }
    catch (error) { return error instanceof Error ? error.message : String(error); }
  }, seed.input);
  expect(blockedSourceChange).toContain('The current project cannot be replaced');
  expect((await snapshot(page)).map).toBe(23);
  expect(await sceneSnapshot(page)).toEqual(savedInB);

  // Switching to A invalidates the delayed B read. Releasing it must preserve the
  // current Scene snapshot rather than importing data owned by the prior folder.
  await openFolder(page, 'scene-project-A');
  await page.evaluate(() => window.__sceneProjectFolderControl?.releaseRead());
  await expect(page.locator('.studio-errors')).toContainText('The project, source map or authored revision changed during the operation');
  expect((await snapshot(page)).map).toBe(23);
  expect(await sceneSnapshot(page)).toEqual(savedInB);
});
