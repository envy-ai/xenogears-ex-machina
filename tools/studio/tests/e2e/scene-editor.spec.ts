import { expect, test, type Page } from '@playwright/test';
import { existsSync, mkdirSync, readFileSync } from 'node:fs';
import { dirname } from 'node:path';

interface SceneTraceEntry { request_json: string; response: unknown }
interface SceneTrace { schema_version: number; synthetic_only: boolean; trace: SceneTraceEntry[] }
interface SceneSnapshot {
  revision: number;
  can_undo: boolean;
  can_redo: boolean;
  modified: boolean;
  source_sha256: string;
  reference_diagnostics: string[];
  actors: { id: string; event_source_row: number }[];
  descriptors: { id: string; value: { position: number[]; rotation: number[]; flags: number; model: number } }[];
  zones: { id: string; value: { corners: number[][] } }[];
  entrances: { selector: number; source_span: number[]; x: number; z: number; layer: number; camera_raw: number; facing_raw: number }[];
  collision_entities: { layers: { id: string; vertices: { id: string; value: number[] }[]; triangles: { id: string; value: { vertices: string[] } }[] }[] };
  events: { bytes: number[]; bindings: Record<string, number>; graph: { bytecode_offset: number; entries: { actor: number; event: number; pc: number }[]; instructions: { pc: number; operands: number[]; spec: { handler: string } }[] } };
}
interface SceneTestHook {
  loadScene(input: unknown, entrances?: number[], map?: number): Promise<void>;
  inspectScene(): {
    document: { entrance_selectors: number[]; [key: string]: unknown };
    compiled: { bundle: number[]; bundle_sha256: string; events: { bytes: number[]; bindings: Record<string, number> } };
  } | null;
  snapshot(): { view: string; scene: SceneSnapshot | null; sceneSelection: string | null; selection: { actor: number; event: number; pc: number | null } };
}
interface ProjectedPoint { x: number; y: number; z: number; visible: boolean }
interface SceneCanvasTestApi {
  project(id: string): ProjectedPoint;
  gizmo(axis: string): ProjectedPoint;
}

async function loadScene(page: Page, input: unknown, entrances: number[], map: number): Promise<void> {
  await page.evaluate(async value => {
    const driver = (window as unknown as { __xemStudioTest: SceneTestHook }).__xemStudioTest;
    await driver.loadScene(value.input, value.entrances, value.map);
  }, { input, entrances, map });
}

async function hookSnapshot(page: Page): Promise<ReturnType<SceneTestHook['snapshot']>> {
  return page.evaluate(() => (window as unknown as { __xemStudioTest: SceneTestHook }).__xemStudioTest.snapshot());
}

async function sceneSnapshot(page: Page): Promise<SceneSnapshot> {
  return page.evaluate(() => {
    const state = (window as unknown as { __xemStudioTest: SceneTestHook }).__xemStudioTest.snapshot().scene;
    if (!state) throw new Error('The synthetic scene is not open');
    return state;
  });
}

async function inspectScene(page: Page): Promise<NonNullable<ReturnType<SceneTestHook['inspectScene']>>> {
  const value = await page.evaluate(() => (window as unknown as { __xemStudioTest: SceneTestHook }).__xemStudioTest.inspectScene());
  if (!value) throw new Error('The Rust-owned Scene document is not open');
  return value;
}

async function inspectEntrance(page: Page, selector: number): Promise<void> {
  await page.getByText('Inspect existing entrance selector', { exact: true }).click();
  const picker = page.locator('.scene-entrance-picker');
  await picker.getByRole('spinbutton', { name: 'Existing entrance selector' }).fill(String(selector));
  await picker.getByRole('button', { name: 'Inspect entrance', exact: true }).click();
}

async function expectSelectedActorEntry(page: Page): Promise<string> {
  await expect.poll(async () => page.evaluate(() => {
    const value = (window as unknown as { __xemStudioTest: SceneTestHook }).__xemStudioTest.snapshot();
    const scene = value.scene;
    const actor = scene?.actors[value.selection.actor];
    const entry = scene?.events.graph.entries.find(item => item.actor === value.selection.actor && item.event === value.selection.event);
    return Boolean(actor && entry && value.sceneSelection === actor.id && value.selection.pc === entry.pc);
  })).toBe(true);
  const value = await hookSnapshot(page);
  const scene = await sceneSnapshot(page);
  const actor = scene.actors[value.selection.actor];
  expect(actor).toBeDefined();
  expect(value.sceneSelection).toBe(actor!.id);
  const entry = scene.events.graph.entries.find(item => item.actor === value.selection.actor && item.event === value.selection.event);
  expect(entry).toBeDefined();
  expect(value.selection.pc).toBe(entry!.pc);
  return actor!.id;
}

async function projected(page: Page, method: 'project' | 'gizmo', value: string): Promise<ProjectedPoint> {
  return page.locator('canvas[aria-label="3D field scene viewport"]').evaluate((canvas, input) => {
    const api = (canvas as HTMLCanvasElement & { __xemScene?: SceneCanvasTestApi }).__xemScene;
    if (!api) throw new Error('The test-only Three.js projection bridge is unavailable');
    return api[input.method](input.value);
  }, { method, value });
}

async function expectSceneLayout(page: Page, expectedInlineState: 'clean' | 'dirty' | 'invalid'): Promise<void> {
  expect(page.viewportSize()).toEqual({ width: 1440, height: 900 });
  const canvas = page.locator('canvas[aria-label="3D field scene viewport"]');
  const canvasBounds = await canvas.boundingBox();
  expect(canvasBounds, 'the central viewport remains laid out').not.toBeNull();
  expect(canvasBounds!.height, 'the 1440×900 Scene viewport has useful working height').toBeGreaterThanOrEqual(260);
  const tree = page.getByRole('tree', { name: 'Scene entities' });
  const treeBounds = await tree.boundingBox();
  expect(treeBounds, 'the source entity tree remains reachable').not.toBeNull();
  expect(treeBounds!.width).toBeGreaterThan(100);
  expect(treeBounds!.y + treeBounds!.height).toBeLessThanOrEqual(900);
  const apply = page.getByRole('button', { name: 'Apply scene fields' });
  const cancel = page.getByRole('button', { name: 'Cancel scene fields' });
  for (const control of [apply, cancel]) {
    const bounds = await control.boundingBox();
    expect(bounds, `${await control.textContent()} stays reachable`).not.toBeNull();
    expect(bounds!.y + bounds!.height).toBeLessThanOrEqual(900);
  }
  await expect(page.locator('.studio-errors')).toHaveCount(0);
  if (expectedInlineState === 'invalid') {
    await expect(apply).toBeDisabled();
    await expect(cancel).toBeEnabled();
  } else if (expectedInlineState === 'dirty') {
    await expect(apply).toBeEnabled();
    await expect(cancel).toBeEnabled();
  }
}

async function captureApprovedScreenshot(page: Page, variable: string): Promise<void> {
  const screenshotPath = process.env[variable];
  expect(screenshotPath, `${variable} points to a fresh M7 screenshot`).toBeTruthy();
  if (existsSync(screenshotPath!)) throw new Error(`Refusing to overwrite an existing screenshot: ${screenshotPath}`);
  mkdirSync(dirname(screenshotPath!), { recursive: true });
  await page.evaluate(() => document.fonts.ready.then(() => undefined));
  await page.screenshot({ path: screenshotPath!, fullPage: false });
}

test('synthetic Scene editing preserves the Rust owner through tree, fields, picking, gizmo and viewport recovery', async ({ page }) => {
  test.setTimeout(90_000);
  const fixturePath = process.env.XEM_STUDIO_SCENE_UI_FIXTURE;
  test.skip(!fixturePath, 'the native scene command trace provides the synthetic-only input');
  const fixture = JSON.parse(readFileSync(fixturePath!, 'utf8')) as SceneTrace;
  expect(fixture.schema_version).toBe(1);
  expect(fixture.synthetic_only).toBe(true);
  const decoded = fixture.trace.find(step => {
    if (typeof step.request_json !== 'string') return false;
    const request = JSON.parse(step.request_json) as { op?: string };
    return request.op === 'field.scene.decode' && typeof (step.response as { ok?: unknown }).ok !== 'undefined';
  });
  expect(decoded, 'trace contains a successful synthetic field.scene.decode').toBeDefined();
  const request = JSON.parse(decoded!.request_json) as { args: { input: unknown; entrances: number[] } };

  const pageErrors: string[] = [];
  page.on('pageerror', error => pageErrors.push(error.message));
  await page.goto('/');
  await expect(page.locator('[data-panel-id="backend"]')).toContainText('xem-studio 0.5.0');
  await expect.poll(() => page.evaluate(() => typeof (window as unknown as { __xemStudioTest?: SceneTestHook }).__xemStudioTest?.loadScene)).toBe('function');
  await loadScene(page, request.args.input, request.args.entrances, 23);

  const canvas = page.locator('canvas[aria-label="3D field scene viewport"]');
  const initial = await sceneSnapshot(page);
  const owner = initial.actors[0];
  const sourceEntry = initial.events.graph.entries[0];
  expect(owner).toBeDefined();
  expect(sourceEntry).toBeDefined();
  await expect.poll(async () => (await hookSnapshot(page)).selection).toMatchObject(sourceEntry);
  await expect(page.getByRole('tree', { name: 'Scene entities' })).toBeVisible();
  await expect(page.getByRole('tabpanel', { name: 'Scene' })).toBeVisible();
  await expect(page.getByRole('region', { name: 'Authored scene inspector' })).toBeVisible();
  await expectSceneLayout(page, 'clean');
  await expect(page.getByRole('button', { name: 'Build package' })).toBeDisabled();
  await expect(page.getByRole('button', { name: 'Play', exact: true })).toBeDisabled();

  // Selecting an actor follows its Rust-owned event entry without creating a second script draft.
  await page.locator(`[data-scene-id="${owner.id}"]`).click();
  expect((await hookSnapshot(page)).selection).toMatchObject(sourceEntry);

  // Numeric scene fields guard a switch back to the script view while still inline-dirty.
  await page.locator(`[data-scene-id="${owner.id}"]`).click();
  await page.locator('[data-scene-id="zone:0"]').click();
  const originalCornerX = initial.zones[0].value.corners[0][0];
  const cornerX = page.getByRole('spinbutton', { name: 'Corner 0 X' });
  await cornerX.fill(String(originalCornerX + 17));
  await page.getByRole('tab', { name: 'Command list' }).click();
  await expect(page.getByRole('tab', { name: 'Scene', exact: true })).toHaveAttribute('aria-selected', 'true');
  expect((await hookSnapshot(page)).view).toBe('scene');
  expect(await cornerX.inputValue()).toBe(String(originalCornerX + 17));
  await expect(page.locator('.studio-errors')).toHaveCount(0);
  await expect(page.locator('.studio-log-entry').filter({ hasText: 'Apply or Cancel the inline fields before changing selection or view.' })).toBeVisible();
  await expectSceneLayout(page, 'dirty');
  await page.getByRole('button', { name: 'Cancel scene fields' }).click();
  await page.getByRole('tab', { name: 'Command list' }).click();
  await expect(page.getByRole('tab', { name: 'Command list' })).toHaveAttribute('aria-selected', 'true');
  expect((await hookSnapshot(page)).view).toBe('list');
  await page.getByRole('tab', { name: 'Flow graph' }).click();
  await expect(page.getByRole('tab', { name: 'Flow graph' })).toHaveAttribute('aria-selected', 'true');
  expect((await hookSnapshot(page)).view).toBe('graph');
  await page.getByRole('tab', { name: 'Scene', exact: true }).click();
  await expect(page.getByRole('tabpanel', { name: 'Scene' })).toBeVisible();
  await page.locator('[data-scene-id="zone:0"]').click();
  await cornerX.fill(String(originalCornerX + 17));
  await expect(page.getByRole('button', { name: 'Apply scene fields' })).toBeEnabled();
  await page.getByRole('button', { name: 'Apply scene fields' }).click();
  const applied = await sceneSnapshot(page);
  expect(applied.revision).toBe(initial.revision + 1);
  expect(applied.zones[0].value.corners[0][0]).toBe(originalCornerX + 17);
  await page.getByRole('button', { name: 'Undo', exact: true }).click();
  expect((await sceneSnapshot(page)).zones[0].value.corners[0][0]).toBe(originalCornerX);
  await page.getByRole('button', { name: 'Redo', exact: true }).click();
  expect((await sceneSnapshot(page)).zones[0].value.corners[0][0]).toBe(originalCornerX + 17);
  await page.getByRole('tab', { name: 'Command list' }).click();
  await expect(page.getByRole('tab', { name: 'Command list' })).toHaveAttribute('aria-selected', 'true');
  await page.getByRole('tab', { name: 'Flow graph' }).click();
  await expect(page.getByRole('tab', { name: 'Flow graph' })).toHaveAttribute('aria-selected', 'true');
  await page.getByRole('tab', { name: 'Scene', exact: true }).click();
  await expect(page.getByRole('tabpanel', { name: 'Scene' })).toBeVisible();
  expect((await sceneSnapshot(page)).zones[0].value.corners[0][0]).toBe(originalCornerX + 17);
  await page.locator('[data-scene-id="zone:0"]').click();
  await page.getByRole('button', { name: 'Frame selection' }).click();
  await captureApprovedScreenshot(page, 'XEM_STUDIO_SCENE_SCREENSHOT');

  // Cancel drops only the unapplied inline value; invalid signed16 fields cannot apply.
  await cornerX.fill(String(originalCornerX + 18));
  await expect(page.getByText('Unapplied fields; last valid scene retained')).toBeVisible();
  await page.getByRole('button', { name: 'Cancel scene fields' }).click();
  expect((await sceneSnapshot(page)).zones[0].value.corners[0][0]).toBe(originalCornerX + 17);
  await cornerX.fill('32768');
  await expect(page.getByRole('alert').filter({ hasText: 'whole signed16 coordinates' })).toBeVisible();
  await expect(page.getByRole('button', { name: 'Apply scene fields' })).toBeDisabled();
  await expect(page.getByRole('button', { name: 'Build package' })).toBeDisabled();
  await expect(page.getByRole('button', { name: 'Play', exact: true })).toBeDisabled();
  expect((await sceneSnapshot(page)).zones[0].value.corners[0][0]).toBe(originalCornerX + 17);
  await expect(page.locator('.studio-errors')).toHaveCount(0);
  await expect(page.locator('.studio-log-entry').filter({ hasText: 'Apply or Cancel the inline fields before changing selection or view.' })).toBeVisible();
  await expectSceneLayout(page, 'invalid');
  await captureApprovedScreenshot(page, 'XEM_STUDIO_SCENE_INVALID_SCREENSHOT');
  await page.getByRole('button', { name: 'Cancel scene fields' }).click();

  // Isolate the zone layer, frame it, then ray-pick its actual projected mesh.
  await page.getByRole('checkbox', { name: 'Geometry' }).uncheck();
  await page.getByRole('checkbox', { name: 'Actors' }).uncheck();
  await page.getByRole('checkbox', { name: 'Collision' }).uncheck();
  await page.locator('[data-scene-id="zone:0"]').click();
  await page.getByRole('button', { name: 'Frame selection' }).click();
  const entranceId = `entrance:${initial.entrances[0].selector}`;
  await expect(page.locator(`[data-scene-id="${entranceId}"]`)).toBeVisible();
  await page.locator(`[data-scene-id="${entranceId}"]`).click();
  const zonePoint = await projected(page, 'project', 'zone:0');
  expect(zonePoint.visible).toBe(true);
  const zoneBounds = await canvas.boundingBox();
  expect(zoneBounds).not.toBeNull();
  expect(zonePoint.x).toBeGreaterThan(zoneBounds!.x);
  expect(zonePoint.x).toBeLessThan(zoneBounds!.x + zoneBounds!.width);
  expect(zonePoint.y).toBeGreaterThan(zoneBounds!.y);
  expect(zonePoint.y).toBeLessThan(zoneBounds!.y + zoneBounds!.height);
  await canvas.click({ position: await canvas.evaluate((element, point) => ({ x: point.x - element.getBoundingClientRect().left, y: point.y - element.getBoundingClientRect().top }), zonePoint) });
  await expect.poll(async () => (await hookSnapshot(page)).sceneSelection).toBe('zone:0');
  for (const layer of ['Geometry', 'Actors', 'Collision', 'Entrances']) await page.getByRole('checkbox', { name: layer }).check();

  // Header rotation is qualified even for zero flags. Seed nonzero reserved high bits
  // through the ordinary field controls so the rotation drag must preserve them.
  await page.locator(`[data-scene-id="${owner.id}"]`).click();
  await page.getByRole('button', { name: 'Frame selection' }).click();
  const rotationWords = [0xa123, 0xb234, 0xc345];
  for (const [axis, value] of rotationWords.entries()) {
    await page.getByRole('spinbutton', { name: `Scene rotation ${['X', 'Y', 'Z'][axis]}` }).fill(String(value));
  }
  await page.getByRole('button', { name: 'Apply scene fields' }).click();
  const rotationSeed = await sceneSnapshot(page);
  expect(rotationSeed.descriptors.find(item => item.id === owner.id)!.value.rotation).toEqual(rotationWords);
  const unchangedExceptRotation = (state: SceneSnapshot) => {
    const copy = structuredClone(state);
    copy.revision = 0;
    copy.can_undo = false;
    copy.can_redo = false;
    copy.descriptors.find(item => item.id === owner.id)!.value.rotation = [];
    return copy;
  };

  // A measured Rotate handle previews locally; Cancel leaves the Rust owner unchanged.
  await page.getByRole('button', { name: 'Rotate' }).click();
  await page.getByRole('button', { name: 'Frame selection' }).click();
  const beforeRotateCancel = await sceneSnapshot(page);
  const handle = await projected(page, 'gizmo', 'X');
  expect(handle.visible).toBe(true);
  const gizmoBounds = await canvas.boundingBox();
  expect(gizmoBounds).not.toBeNull();
  expect(handle.x).toBeGreaterThan(gizmoBounds!.x);
  expect(handle.x).toBeLessThan(gizmoBounds!.x + gizmoBounds!.width);
  expect(handle.y).toBeGreaterThan(gizmoBounds!.y);
  expect(handle.y).toBeLessThan(gizmoBounds!.y + gizmoBounds!.height);
  const canvasBounds = await canvas.boundingBox();
  expect(canvasBounds).not.toBeNull();
  expect(handle.x).toBeGreaterThan(canvasBounds!.x);
  expect(handle.x).toBeLessThan(canvasBounds!.x + canvasBounds!.width);
  expect(handle.y).toBeGreaterThan(canvasBounds!.y);
  expect(handle.y).toBeLessThan(canvasBounds!.y + canvasBounds!.height);
  await page.mouse.move(handle.x, handle.y);
  await page.mouse.down();
  await page.mouse.move(handle.x + 24, handle.y + 16, { steps: 5 });
  await page.mouse.up();
  await expect(page.getByText('Unapplied fields; last valid scene retained')).toBeVisible();
  const previewRotationX = Number(await page.getByRole('spinbutton', { name: 'Scene rotation X' }).inputValue());
  expect(previewRotationX).not.toBe(beforeRotateCancel.descriptors.find(item => item.id === owner.id)!.value.rotation[0]);
  expect(previewRotationX & 0xf000).toBe(rotationWords[0] & 0xf000);
  expect(await sceneSnapshot(page)).toEqual(beforeRotateCancel);
  await page.getByRole('button', { name: 'Cancel scene fields' }).click();
  expect(await sceneSnapshot(page)).toEqual(beforeRotateCancel);

  // Apply a second real rotation drag; only the selected descriptor's rotation words may change.
  const applyHandle = await projected(page, 'gizmo', 'X');
  expect(applyHandle.visible).toBe(true);
  await page.mouse.move(applyHandle.x, applyHandle.y);
  await page.mouse.down();
  await page.mouse.move(applyHandle.x - 24, applyHandle.y + 16, { steps: 5 });
  await page.mouse.up();
  await expect(page.getByText('Unapplied fields; last valid scene retained')).toBeVisible();
  const appliedRotationX = Number(await page.getByRole('spinbutton', { name: 'Scene rotation X' }).inputValue());
  expect(appliedRotationX).not.toBe(beforeRotateCancel.descriptors.find(item => item.id === owner.id)!.value.rotation[0]);
  expect(appliedRotationX & 0xf000).toBe(rotationWords[0] & 0xf000);
  expect(await sceneSnapshot(page)).toEqual(beforeRotateCancel);
  await page.getByRole('button', { name: 'Apply scene fields' }).click();
  const afterRotationApply = await sceneSnapshot(page);
  expect(afterRotationApply.descriptors.find(item => item.id === owner.id)!.value.rotation[0]).toBe(appliedRotationX);
  expect(afterRotationApply.descriptors.find(item => item.id === owner.id)!.value.rotation.map(value => value & 0xf000)).toEqual(rotationWords.map(value => value & 0xf000));
  expect(afterRotationApply.descriptors.find(item => item.id === owner.id)!.value.position).toEqual(beforeRotateCancel.descriptors.find(item => item.id === owner.id)!.value.position);
  expect(unchangedExceptRotation(afterRotationApply)).toEqual(unchangedExceptRotation(beforeRotateCancel));

  // A real owner revision change while a gizmo drag is held invalidates its captured token.
  await page.getByRole('button', { name: 'Move' }).click();
  const beforeStaleDrag = await sceneSnapshot(page);
  const staleHandle = await projected(page, 'gizmo', 'X');
  expect(staleHandle.visible).toBe(true);
  await page.mouse.move(staleHandle.x, staleHandle.y);
  await page.mouse.down();
  await page.mouse.move(staleHandle.x + 16, staleHandle.y - 8, { steps: 3 });
  await page.keyboard.press('Control+z');
  const afterConcurrentUndo = await sceneSnapshot(page);
  expect(afterConcurrentUndo.revision).toBe(beforeStaleDrag.revision + 1);
  expect(afterConcurrentUndo.descriptors.find(item => item.id === owner.id)!.value.position).toEqual(beforeStaleDrag.descriptors.find(item => item.id === owner.id)!.value.position);
  await page.mouse.up();
  expect(await sceneSnapshot(page)).toEqual(afterConcurrentUndo);
  expect(Number(await page.getByRole('spinbutton', { name: 'Scene position X' }).inputValue())).toBe(afterConcurrentUndo.descriptors.find(item => item.id === owner.id)!.value.position[0]);
  expect(Number(await page.getByRole('spinbutton', { name: 'Scene rotation X' }).inputValue())).toBe(afterConcurrentUndo.descriptors.find(item => item.id === owner.id)!.value.rotation[0]);
  await expect(page.getByText('Unapplied fields; last valid scene retained')).toHaveCount(0);
  await expect(page.locator('.studio-errors')).toHaveCount(0);
  expect(pageErrors).toEqual([]);

  // A measured Move handle also previews; Cancel keeps source position and scene state intact.
  await page.getByRole('button', { name: 'Move' }).click();
  const beforeDrag = await sceneSnapshot(page);
  const actorPosition = beforeDrag.descriptors.find(item => item.id === owner.id)!.value.position;
  const moveHandle = await projected(page, 'gizmo', 'X');
  expect(moveHandle.visible).toBe(true);
  await page.mouse.move(moveHandle.x, moveHandle.y);
  await page.mouse.down();
  await page.mouse.move(moveHandle.x + 32, moveHandle.y - 10, { steps: 5 });
  await page.mouse.up();
  await expect(page.getByText('Unapplied fields; last valid scene retained')).toBeVisible();
  const previewPosition = await page.getByRole('spinbutton', { name: 'Scene position X' }).inputValue();
  expect(Number(previewPosition)).not.toBe(actorPosition[0]);
  expect(await sceneSnapshot(page)).toEqual(beforeDrag);
  await page.getByRole('button', { name: 'Cancel scene fields' }).click();
  expect(await sceneSnapshot(page)).toEqual(beforeDrag);

  // A late animation-frame renderer exception is caught locally and retryable.
  const injected = await canvas.evaluate(element => {
    const gl = (element as HTMLCanvasElement).getContext('webgl2') ?? (element as HTMLCanvasElement).getContext('webgl');
    if (!gl) return false;
    let thrown = false;
    const drawArrays = gl.drawArrays;
    const drawElements = gl.drawElements;
    const restoreAndThrow = () => {
      if (thrown) return;
      thrown = true;
      Object.defineProperty(gl, 'drawArrays', { configurable: true, value: drawArrays });
      Object.defineProperty(gl, 'drawElements', { configurable: true, value: drawElements });
      throw new Error('M7 injected late renderer failure');
    };
    for (const name of ['drawArrays', 'drawElements'] as const) {
      Object.defineProperty(gl, name, { configurable: true, value: restoreAndThrow });
    }
    return true;
  });
  expect(injected).toBe(true);
  await expect(page.locator('.field-scene .scene-canvas__error')).toContainText('M7 injected late renderer failure');
  expect(pageErrors).toEqual([]);
  expect(await sceneSnapshot(page)).toEqual(beforeDrag);
  await page.getByRole('button', { name: 'Retry 3D viewport' }).click();
  await expect(canvas).toBeVisible();
  await expect(page.locator('.scene-canvas__error')).toHaveCount(0);

  // Context loss is also local; tree/history remain usable and Retry recreates the renderer.
  const lost = await canvas.evaluate(element => {
    const gl = (element as HTMLCanvasElement).getContext('webgl2') ?? (element as HTMLCanvasElement).getContext('webgl');
    const extension = gl?.getExtension('WEBGL_lose_context');
    if (!extension) return false;
    extension.loseContext();
    return true;
  });
  expect(lost, 'pinned headless Chromium provides WEBGL_lose_context').toBe(true);
  await expect(page.locator('.field-scene .scene-canvas__error')).toContainText('WebGL context was lost');
  await expect(page.locator('[data-scene-id="zone:0"]')).toBeVisible();
  expect(await sceneSnapshot(page)).toEqual(beforeDrag);
  await page.getByRole('button', { name: 'Retry 3D viewport' }).click();
  await expect(canvas).toBeVisible();
  await expect(page.locator('.scene-canvas__error')).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Undo', exact: true })).toBeEnabled();
  await page.locator('[data-scene-id="zone:0"]').click();
  await expect.poll(async () => (await hookSnapshot(page)).sceneSelection).toBe('zone:0');
  expect(pageErrors).toEqual([]);
});

test('Scene inspector keeps bounds feedback, field ownership and all corners reachable at 1440x900', async ({ page }) => {
  const fixturePath = process.env.XEM_STUDIO_SCENE_UI_FIXTURE;
  test.skip(!fixturePath, 'the native scene command trace provides the synthetic-only input');
  const fixture = JSON.parse(readFileSync(fixturePath!, 'utf8')) as SceneTrace;
  expect(fixture.synthetic_only).toBe(true);
  const decoded = fixture.trace.find(step => {
    const request = JSON.parse(step.request_json) as { op?: string };
    return request.op === 'field.scene.decode' && typeof (step.response as { ok?: unknown }).ok !== 'undefined';
  });
  expect(decoded).toBeDefined();
  const request = JSON.parse(decoded!.request_json) as { args: { input: unknown; entrances: number[] } };

  await page.goto('/');
  await expect(page.locator('[data-panel-id="backend"]')).toContainText('xem-studio 0.5.0');
  await expect.poll(() => page.evaluate(() => typeof (window as unknown as { __xemStudioTest?: SceneTestHook }).__xemStudioTest?.loadScene)).toBe('function');
  await loadScene(page, request.args.input, request.args.entrances, 23);
  const initial = await sceneSnapshot(page);
  const cornerX = page.getByRole('spinbutton', { name: 'Corner 0 X' });
  const sourceX = initial.zones[0].value.corners[0][0];
  await page.locator('[data-scene-id="zone:0"]').click();
  await page.getByRole('button', { name: 'Frame selection' }).click();
  await expectSceneLayout(page, 'clean');
  await captureApprovedScreenshot(page, 'XEM_STUDIO_SCENE_SCREENSHOT');

  // Expected selection/view guard stays in the Validation log and does not become a global stack banner.
  await cornerX.fill(String(sourceX + 1));
  await page.getByRole('tab', { name: 'Command list' }).click();
  await expect(page.getByRole('tab', { name: 'Scene', exact: true })).toHaveAttribute('aria-selected', 'true');
  expect((await hookSnapshot(page)).view).toBe('scene');
  await expect(page.locator('.studio-errors')).toHaveCount(0);
  await expect(page.locator('.studio-log-entry').filter({ hasText: 'Apply or Cancel the inline fields before changing selection or view.' })).toBeVisible();
  await expectSceneLayout(page, 'dirty');
  await page.getByRole('button', { name: 'Cancel scene fields' }).click();
  expect(await sceneSnapshot(page)).toEqual(initial);

  // Invalid signed16 input exposes its own persistent, associated reason above sticky actions.
  await cornerX.fill('32768');
  await expect(cornerX).toHaveAttribute('aria-invalid', 'true');
  const describedBy = (await cornerX.getAttribute('aria-describedby'))?.split(/\s+/).filter(Boolean) ?? [];
  expect(describedBy).toHaveLength(1);
  const reason = page.locator('.scene-property-actions [role="alert"]');
  await expect(reason).toContainText('whole signed16 coordinates');
  await expect(reason).toBeVisible();
  const describedText = await page.evaluate(id => document.getElementById(id)?.textContent ?? null, describedBy[0]);
  expect(describedText).toContain('whole signed16 coordinates');
  await expect(page.getByRole('button', { name: 'Apply scene fields' })).toBeDisabled();
  await expect(page.getByRole('button', { name: 'Cancel scene fields' })).toBeEnabled();
  expect(await sceneSnapshot(page)).toEqual(initial);

  // Use ordinary inspector scrolling to reach all four source corners while the reason/actions stay visible.
  const fieldsRegion = page.locator('.scene-inspector-fields');
  const footer = page.locator('.scene-property-actions');
  const fieldsBounds = await fieldsRegion.boundingBox();
  const footerBounds = await footer.boundingBox();
  expect(fieldsBounds).not.toBeNull();
  expect(footerBounds).not.toBeNull();
  for (let index = 0; index < 4; index++) {
    const field = page.getByRole('spinbutton', { name: `Corner ${index} X` });
    await field.scrollIntoViewIfNeeded();
    const viewport = await fieldsRegion.boundingBox();
    expect(viewport).not.toBeNull();
    await page.mouse.move(viewport!.x + viewport!.width / 2, viewport!.y + viewport!.height / 2);
    await page.mouse.wheel(0, 32);
    await expect.poll(async () => {
      const fieldBounds = await field.boundingBox();
      const currentFieldsBounds = await fieldsRegion.boundingBox();
      const currentFooterBounds = await footer.boundingBox();
      return Boolean(fieldBounds && currentFieldsBounds && currentFooterBounds
        && fieldBounds.y >= currentFieldsBounds.y
        && fieldBounds.y + fieldBounds.height <= currentFieldsBounds.y + currentFieldsBounds.height
        && fieldBounds.y + fieldBounds.height <= currentFooterBounds.y);
    }).toBe(true);
    await field.click();
    await expect(field).toBeVisible();
    const fieldBounds = await field.boundingBox();
    const currentFieldsBounds = await fieldsRegion.boundingBox();
    const currentFooterBounds = await footer.boundingBox();
    expect(fieldBounds).not.toBeNull();
    expect(currentFieldsBounds).not.toBeNull();
    expect(currentFooterBounds).not.toBeNull();
    expect(fieldBounds!.y).toBeGreaterThanOrEqual(currentFieldsBounds!.y);
    expect(fieldBounds!.y + fieldBounds!.height).toBeLessThanOrEqual(currentFieldsBounds!.y + currentFieldsBounds!.height);
    expect(fieldBounds!.y + fieldBounds!.height).toBeLessThanOrEqual(currentFooterBounds!.y);
    const hitTarget = await field.evaluate(element => {
      const bounds = element.getBoundingClientRect();
      const hit = document.elementFromPoint(bounds.x + bounds.width / 2, bounds.y + bounds.height / 2);
      return hit === element || element.contains(hit);
    });
    expect(hitTarget).toBe(true);
    await expect(reason).toBeVisible();
  }
  await cornerX.scrollIntoViewIfNeeded();
  const reasonBounds = await reason.boundingBox();
  const applyBounds = await page.getByRole('button', { name: 'Apply scene fields' }).boundingBox();
  const cancelBounds = await page.getByRole('button', { name: 'Cancel scene fields' }).boundingBox();
  expect(reasonBounds).not.toBeNull();
  expect(applyBounds).not.toBeNull();
  expect(cancelBounds).not.toBeNull();
  expect(reasonBounds!.y).toBeGreaterThanOrEqual(footerBounds!.y);
  expect(reasonBounds!.y + reasonBounds!.height).toBeLessThanOrEqual(applyBounds!.y);
  expect(cancelBounds!.y + cancelBounds!.height).toBeLessThanOrEqual(900);
  await expectSceneLayout(page, 'invalid');
  await captureApprovedScreenshot(page, 'XEM_STUDIO_SCENE_INVALID_SCREENSHOT');
  await page.getByRole('button', { name: 'Cancel scene fields' }).click();
  expect(await sceneSnapshot(page)).toEqual(initial);
});


test('a qualified placement binding opens its owned command and script dirt blocks switching into Scene', async ({ page }) => {
  const fixturePath = process.env.XEM_STUDIO_SCENE_UI_FIXTURE;
  test.skip(!fixturePath, 'the native scene trace provides a synthetic placement source');
  const fixture = JSON.parse(readFileSync(fixturePath!, 'utf8')) as SceneTrace;
  expect(fixture.synthetic_only).toBe(true);
  const opened = fixture.trace.find(step => {
    const request = JSON.parse(step.request_json) as { op?: string; args?: { kind?: string } };
    const response = step.response as { ok?: { state?: { events?: { graph?: { instructions?: { spec?: { handler?: string } }[] } } } } };
    return request.op === 'field.scene.draft' && request.args?.kind === 'open'
      && response.ok?.state?.events?.graph?.instructions?.some(instruction => instruction.spec?.handler?.startsWith('field_event_place')) === true;
  });
  expect(opened, 'trace has an accepted scene open with a qualified placement instruction').toBeDefined();
  const request = JSON.parse(opened!.request_json) as { args: { input: unknown; entrances: number[] } };
  const response = opened!.response as { ok: { state: SceneSnapshot } };
  const placementInstruction = response.ok.state.events.graph.instructions.find(instruction => instruction.spec.handler.startsWith('field_event_place'))!;
  const ownerEntry = response.ok.state.events.graph.entries[0];

  await page.goto('/');
  await expect(page.locator('[data-panel-id="backend"]')).toContainText('xem-studio 0.5.0');
  await expect.poll(() => page.evaluate(() => typeof (window as unknown as { __xemStudioTest?: SceneTestHook }).__xemStudioTest?.loadScene)).toBe('function');
  await loadScene(page, request.args.input, request.args.entrances, 23);
  const openedScene = await sceneSnapshot(page);
  const actor = openedScene.actors[ownerEntry.actor];
  expect(actor).toBeDefined();
  await page.locator(`[data-scene-id="${actor!.id}"]`).click();

  const binding = page.getByRole('combobox', { name: 'Authored placement source' });
  const boundOption = binding.locator('option').filter({ hasText: 'field_event_place' });
  const boundValue = await boundOption.getAttribute('value');
  expect(boundValue).toBeTruthy();
  await binding.selectOption(boundValue!);
  const boundPosition = await page.getByRole('spinbutton', { name: 'Scene position X' }).inputValue();
  await page.locator(`[data-scene-id="${actor!.id}"]`).click();
  await expect(binding).toHaveValue(boundValue!);
  expect(await page.getByRole('spinbutton', { name: 'Scene position X' }).inputValue()).toBe(boundPosition);
  const openCommand = page.getByRole('button', { name: /Open bound command/ });
  await expect(openCommand).toBeEnabled();
  await openCommand.click();
  await expect(page.getByRole('tab', { name: 'Command list' })).toHaveAttribute('aria-selected', 'true');
  await expect.poll(async () => (await hookSnapshot(page)).selection.pc).toBe(placementInstruction.pc);

  // A normal script operand edit is retained in its owner and blocks switching views.
  const operand = page.getByRole('textbox', { name: 'Operand 1 · sel' });
  const current = placementInstruction.operands[0];
  await operand.fill(String(current === 2 ? 1 : 2));
  await page.getByRole('tab', { name: 'Scene', exact: true }).click();
  await expect(page.getByRole('tab', { name: 'Command list' })).toHaveAttribute('aria-selected', 'true');
  expect((await hookSnapshot(page)).view).toBe('list');
  await expect(page.locator('.scene-inspector').getByRole('button', { name: 'Apply scene fields' })).toBeDisabled();
  await expect(page.locator('.scene-inspector').getByRole('button', { name: 'Cancel scene fields' })).toBeDisabled();
  await page.getByRole('button', { name: 'Cancel', exact: true }).click();
  await page.getByRole('tab', { name: 'Scene', exact: true }).click();
  await expect(page.getByRole('tabpanel', { name: 'Scene' })).toBeVisible();
});

test('synthetic Scene CRUD keeps stable IDs and event ownership while refusing a referenced vertex delete', async ({ page }) => {
  test.setTimeout(90_000);
  const fixturePath = process.env.XEM_STUDIO_SCENE_UI_FIXTURE;
  test.skip(!fixturePath, 'the native scene trace provides a synthetic-only source');
  const fixture = JSON.parse(readFileSync(fixturePath!, 'utf8')) as SceneTrace;
  expect(fixture.schema_version).toBe(1);
  expect(fixture.synthetic_only).toBe(true);
  const decoded = fixture.trace.find(step => {
    const request = JSON.parse(step.request_json) as { op?: string };
    return request.op === 'field.scene.decode' && typeof (step.response as { ok?: unknown }).ok !== 'undefined';
  });
  expect(decoded).toBeDefined();
  const request = JSON.parse(decoded!.request_json) as { args: { input: unknown; entrances: number[] } };

  await page.goto('/');
  await expect(page.locator('[data-panel-id="backend"]')).toContainText('xem-studio 0.5.0');
  await expect.poll(() => page.evaluate(() => typeof (window as unknown as { __xemStudioTest?: SceneTestHook }).__xemStudioTest?.loadScene)).toBe('function');
  await loadScene(page, request.args.input, request.args.entrances, 23);
  const initial = await sceneSnapshot(page);
  const sourceActor = initial.actors[0];
  const sourceEntry = initial.events.graph.entries.find(entry => entry.actor === 0);
  const sourceZone = initial.zones[0];
  expect(sourceActor).toBeDefined();
  expect(sourceEntry).toBeDefined();
  expect(sourceZone).toBeDefined();
  expect(initial.collision_entities.layers).toHaveLength(1);
  const collision = initial.collision_entities.layers[0]!;
  expect(collision.triangles.some(face => face.value.vertices.includes(collision.vertices[0]!.id))).toBe(true);

  await page.locator(`[data-scene-id="${sourceActor!.id}"]`).click();
  await expect.poll(async () => (await hookSnapshot(page)).selection).toMatchObject(sourceEntry!);
  const sourceActorId = sourceActor!.id;

  // Actor Add/Duplicate must activate the returned stable actor identity and its
  // actual runtime-table event entry without a compensating click on the new row.
  await page.getByRole('button', { name: 'Add', exact: true }).click();
  const afterActorAdd = await sceneSnapshot(page);
  expect(afterActorAdd.actors).toHaveLength(initial.actors.length + 1);
  const addedActor = afterActorAdd.actors.find(actor => !initial.actors.some(original => original.id === actor.id));
  expect(addedActor).toBeDefined();
  expect(await expectSelectedActorEntry(page)).toBe(addedActor!.id);
  await page.getByRole('tab', { name: 'Command list' }).click();
  await expect(page.getByRole('tab', { name: 'Command list' })).toHaveAttribute('aria-selected', 'true');
  expect((await hookSnapshot(page)).view).toBe('list');
  expect(await expectSelectedActorEntry(page)).toBe(addedActor!.id);
  await page.getByRole('tab', { name: 'Scene', exact: true }).click();
  await expect(page.getByRole('tabpanel', { name: 'Scene' })).toBeVisible();

  await page.getByRole('button', { name: 'Duplicate', exact: true }).click();
  const afterActorDuplicate = await sceneSnapshot(page);
  expect(afterActorDuplicate.actors).toHaveLength(afterActorAdd.actors.length + 1);
  const duplicatedActor = afterActorDuplicate.actors.find(actor => !afterActorAdd.actors.some(original => original.id === actor.id));
  expect(duplicatedActor).toBeDefined();
  expect(duplicatedActor!.id).not.toBe(addedActor!.id);
  expect(await expectSelectedActorEntry(page)).toBe(duplicatedActor!.id);
  await page.getByRole('tab', { name: 'Command list' }).click();
  await expect(page.getByRole('tab', { name: 'Command list' })).toHaveAttribute('aria-selected', 'true');
  expect((await hookSnapshot(page)).view).toBe('list');
  expect(await expectSelectedActorEntry(page)).toBe(duplicatedActor!.id);
  await page.getByRole('tab', { name: 'Scene', exact: true }).click();

  await page.getByRole('button', { name: 'Undo', exact: true }).click();
  const undoActorDuplicate = await sceneSnapshot(page);
  expect(undoActorDuplicate.actors.some(actor => actor.id === duplicatedActor!.id)).toBe(false);
  expect(await expectSelectedActorEntry(page)).toBeTruthy();
  await page.getByRole('button', { name: 'Redo', exact: true }).click();
  const redoActorDuplicate = await sceneSnapshot(page);
  expect(redoActorDuplicate.actors.some(actor => actor.id === duplicatedActor!.id)).toBe(true);
  expect(await expectSelectedActorEntry(page)).toBeTruthy();
  await page.locator(`[data-scene-id="${duplicatedActor!.id}"]`).click();
  await page.getByRole('button', { name: 'Delete', exact: true }).click();
  expect((await sceneSnapshot(page)).actors.some(actor => actor.id === duplicatedActor!.id)).toBe(false);
  await page.getByRole('button', { name: 'Undo', exact: true }).click();
  const undoActorDelete = await sceneSnapshot(page);
  expect(undoActorDelete.actors.some(actor => actor.id === duplicatedActor!.id)).toBe(true);
  expect(await expectSelectedActorEntry(page)).toBeTruthy();
  await page.getByRole('button', { name: 'Redo', exact: true }).click();
  expect((await sceneSnapshot(page)).actors.some(actor => actor.id === duplicatedActor!.id)).toBe(false);
  expect(await expectSelectedActorEntry(page)).toBeTruthy();
  const afterActorHistory = await sceneSnapshot(page);
  expect(afterActorHistory.actors.some(actor => actor.id === sourceActorId)).toBe(true);
  expect(afterActorHistory.events.bindings).toMatchObject(initial.events.bindings);
  expect(afterActorHistory.events.graph.entries.find(entry => entry.actor === 0 && entry.event === sourceEntry!.event)).toBeDefined();
  const sourceEventBytes = afterActorHistory.events.bytes;
  const sourceBindings = afterActorHistory.events.bindings;
  const boundScript = await hookSnapshot(page);

  // Add, duplicate, and delete use the visible hierarchy/editor controls and the Rust
  // scene transaction owner. Generated IDs are discovered from the returned snapshots.
  await page.locator(`[data-scene-id="${sourceZone!.id}"]`).click();
  await page.getByRole('button', { name: 'Add', exact: true }).click();
  const afterAdd = await sceneSnapshot(page);
  expect(afterAdd.zones).toHaveLength(initial.zones.length + 1);
  const added = afterAdd.zones.find(zone => !initial.zones.some(original => original.id === zone.id));
  expect(added).toBeDefined();
  await expect.poll(async () => (await hookSnapshot(page)).sceneSelection).toBe(added!.id);
  expect(afterAdd.events.bytes).toEqual(sourceEventBytes);
  expect(afterAdd.events.bindings).toEqual(sourceBindings);
  expect((await hookSnapshot(page)).selection).toMatchObject(boundScript.selection);

  await page.getByRole('button', { name: 'Duplicate', exact: true }).click();
  const afterDuplicate = await sceneSnapshot(page);
  expect(afterDuplicate.zones).toHaveLength(afterAdd.zones.length + 1);
  const duplicate = afterDuplicate.zones.find(zone => !afterAdd.zones.some(original => original.id === zone.id));
  expect(duplicate).toBeDefined();
  expect(duplicate!.id).not.toBe(added!.id);
  await expect.poll(async () => (await hookSnapshot(page)).sceneSelection).toBe(duplicate!.id);
  expect(afterDuplicate.events.bytes).toEqual(sourceEventBytes);
  expect(afterDuplicate.events.bindings).toEqual(sourceBindings);

  await page.getByRole('button', { name: 'Delete', exact: true }).click();
  const afterDelete = await sceneSnapshot(page);
  expect(afterDelete.zones.some(zone => zone.id === duplicate!.id)).toBe(false);
  expect(afterDelete.zones.some(zone => zone.id === added!.id)).toBe(true);
  expect(afterDelete.events.bytes).toEqual(sourceEventBytes);
  expect(afterDelete.events.bindings).toEqual(sourceBindings);
  expect((await hookSnapshot(page)).selection).toMatchObject(boundScript.selection);

  await page.getByRole('button', { name: 'Undo', exact: true }).click();
  const undoDelete = await sceneSnapshot(page);
  expect(undoDelete.zones.map(zone => zone.id)).toEqual(afterDuplicate.zones.map(zone => zone.id));
  expect(undoDelete.events).toEqual(afterDuplicate.events);
  await page.getByRole('button', { name: 'Undo', exact: true }).click();
  const undoDuplicate = await sceneSnapshot(page);
  expect(undoDuplicate.zones.map(zone => zone.id)).toEqual(afterAdd.zones.map(zone => zone.id));
  expect(undoDuplicate.events).toEqual(afterAdd.events);
  await page.getByRole('button', { name: 'Redo', exact: true }).click();
  const redoDuplicate = await sceneSnapshot(page);
  expect(redoDuplicate.zones.map(zone => zone.id)).toEqual(afterDuplicate.zones.map(zone => zone.id));
  expect(redoDuplicate.events).toEqual(afterDuplicate.events);
  await page.getByRole('button', { name: 'Redo', exact: true }).click();
  const redoDelete = await sceneSnapshot(page);
  expect(redoDelete.zones.map(zone => zone.id)).toEqual(afterDelete.zones.map(zone => zone.id));
  expect(redoDelete.events).toEqual(afterDelete.events);
  expect((await hookSnapshot(page)).selection).toMatchObject(boundScript.selection);

  // The source triangle still owns this vertex. The UI surfaces the Rust refusal and
  // adopts neither a partial mesh mutation nor a history/revision change.
  const beforeRefusedDelete = await sceneSnapshot(page);
  const referencedVertex = collision.vertices[0]!;
  await page.locator(`[data-scene-id="${referencedVertex.id}"]`).click();
  const owner = await sceneSnapshot(page);
  await page.getByRole('button', { name: 'Delete', exact: true }).click();
  const refusal = page.locator('.scene-property-actions [role="alert"]');
  await expect(refusal).toContainText('collision vertex is still referenced by a triangle');
  const afterRefusedDelete = await sceneSnapshot(page);
  expect(afterRefusedDelete).toEqual(owner);
  expect(afterRefusedDelete.collision_entities).toEqual(beforeRefusedDelete.collision_entities);
  expect(afterRefusedDelete.events).toEqual(beforeRefusedDelete.events);
  expect(afterRefusedDelete.revision).toBe(owner.revision);
});

test('synthetic Scene inspects only selected entrance records without changing bytes, then edits X/Z transactionally', async ({ page }) => {
  test.setTimeout(60_000);
  const fixturePath = process.env.XEM_STUDIO_SCENE_UI_FIXTURE;
  test.skip(!fixturePath, 'the native scene trace provides explicit synthetic entrance selectors');
  const fixture = JSON.parse(readFileSync(fixturePath!, 'utf8')) as SceneTrace;
  expect(fixture.schema_version).toBe(1);
  expect(fixture.synthetic_only).toBe(true);
  const opened = fixture.trace.find(step => {
    const request = JSON.parse(step.request_json) as { op?: string; args?: { kind?: string; entrances?: number[] } };
    return request.op === 'field.scene.draft' && request.args?.kind === 'open'
      && request.args.entrances?.length === 2 && typeof (step.response as { ok?: unknown }).ok !== 'undefined';
  });
  expect(opened, 'fixture records an explicitly selected pair of existing selectors').toBeDefined();
  const request = JSON.parse(opened!.request_json) as { args: { input: unknown; entrances: number[] } };
  expect(request.args.entrances).toEqual([0, 1]);
  const expected = (opened!.response as { ok: { state: SceneSnapshot } }).ok.state.entrances;
  expect(expected.map(entry => entry.selector)).toEqual([0, 1]);

  await page.goto('/');
  await expect(page.locator('[data-panel-id="backend"]')).toContainText('xem-studio 0.5.0');
  await expect.poll(() => page.evaluate(() => typeof (window as unknown as { __xemStudioTest?: SceneTestHook }).__xemStudioTest?.inspectScene)).toBe('function');
  await loadScene(page, request.args.input, [], 23);
  const initial = await sceneSnapshot(page);
  expect(initial.entrances).toEqual([]);
  expect(initial.can_undo).toBe(false);
  const sourceScene = await inspectScene(page);
  expect(sourceScene.document.entrance_selectors).toEqual([]);

  for (const selector of request.args.entrances) {
    const before = await sceneSnapshot(page);
    const beforeCompiled = await inspectScene(page);
    await inspectEntrance(page, selector);
    await expect(page.locator(`[data-scene-id="entrance:${selector}"]`)).toBeVisible();
    const after = await sceneSnapshot(page);
    const inspected = after.entrances.find(entry => entry.selector === selector);
    const expectedRecord = expected.find(entry => entry.selector === selector);
    expect(inspected).toEqual(expectedRecord);
    expect(after.entrances.map(entry => entry.selector)).toEqual(request.args.entrances.slice(0, selector + 1));
    expect(after.revision).toBe(before.revision);
    expect(after.can_undo).toBe(false);
    expect(after.can_redo).toBe(false);
    expect(after.modified).toBe(false);
    expect(after.events.bytes).toEqual(before.events.bytes);
    expect(after.events.bindings).toEqual(before.events.bindings);
    const afterCompiled = await inspectScene(page);
    expect(afterCompiled.document.entrance_selectors).toEqual(request.args.entrances.slice(0, selector + 1));
    const { entrance_selectors: _selectors, ...beforeDocument } = beforeCompiled.document;
    const { entrance_selectors: _afterSelectors, ...afterDocument } = afterCompiled.document;
    expect(afterDocument).toEqual(beforeDocument);
    expect(afterCompiled.compiled.bundle_sha256).toBe(beforeCompiled.compiled.bundle_sha256);
    expect(afterCompiled.compiled.bundle.length).toBe(beforeCompiled.compiled.bundle.length);
    expect(afterCompiled.compiled.bundle.every((byte, index) => byte === beforeCompiled.compiled.bundle[index])).toBe(true);
    expect(afterCompiled.compiled.events.bytes).toEqual(beforeCompiled.compiled.events.bytes);
    expect(afterCompiled.compiled.events.bindings).toEqual(beforeCompiled.compiled.events.bindings);
    expect(after.source_sha256).toBe(initial.source_sha256);
    expect(after.reference_diagnostics).toEqual(initial.reference_diagnostics);
  }

  const selectorOne = expected[1]!;
  await expect(page.getByText(`Source selector ${selectorOne.selector}; layer ${selectorOne.layer}. Raw camera/facing bytes ${selectorOne.camera_raw}/${selectorOne.facing_raw}. Height is evaluated by the game.`, { exact: true })).toBeVisible();
  const entranceFooter = page.locator('.scene-property-actions');
  await expect(entranceFooter.getByText('Entrance structural CRUD has no proven count/reference closure.', { exact: true })).toBeVisible();
  for (const action of ['Add', 'Duplicate', 'Delete']) {
    const button = page.getByRole('button', { name: action, exact: true });
    await expect(button).toBeDisabled();
    await expect(button).toHaveAttribute('title', 'Entrance structural CRUD is unavailable: no proven count/reference closure');
  }
  const x = page.getByRole('spinbutton', { name: 'Scene position X' });
  const y = page.getByRole('spinbutton', { name: 'Scene position Y' });
  const z = page.getByRole('spinbutton', { name: 'Scene position Z' });
  await expect(x).toBeEnabled();
  await expect(y).toBeDisabled();
  await expect(z).toBeEnabled();
  await expect(page.getByRole('spinbutton', { name: 'Layer' })).toHaveCount(0);
  await expect(page.getByRole('spinbutton', { name: 'Camera' })).toHaveCount(0);
  await expect(page.getByRole('spinbutton', { name: 'Facing' })).toHaveCount(0);

  const beforeEdit = await sceneSnapshot(page);
  const beforeEditCompiled = await inspectScene(page);
  const original = beforeEdit.entrances.find(entry => entry.selector === selectorOne.selector)!;
  const editedX = original.x < 32767 ? original.x + 1 : original.x - 1;
  const editedZ = original.z < 32767 ? original.z + 1 : original.z - 1;
  await x.fill(String(editedX));
  await z.fill(String(editedZ));
  await page.getByRole('button', { name: 'Apply scene fields' }).click();
  const afterEdit = await sceneSnapshot(page);
  expect(afterEdit.revision).toBe(beforeEdit.revision + 1);
  expect(afterEdit.entrances.find(entry => entry.selector === selectorOne.selector)).toEqual({ ...original, x: editedX, z: editedZ });
  expect(afterEdit.entrances.find(entry => entry.selector === 0)).toEqual(beforeEdit.entrances.find(entry => entry.selector === 0));
  const entranceWordStart = beforeEdit.events.graph.bytecode_offset + original.source_span[0];
  const changedEventBytes = afterEdit.events.bytes.flatMap((value, offset) => value === beforeEdit.events.bytes[offset] ? [] : [offset]);
  expect(changedEventBytes.length).toBeGreaterThan(0);
  expect(changedEventBytes.every(offset => offset >= entranceWordStart && offset < entranceWordStart + 4)).toBe(true);
  expect(afterEdit.events.bindings).toEqual(beforeEdit.events.bindings);
  expect(afterEdit.events.graph.instructions).toEqual(beforeEdit.events.graph.instructions);
  const editedCompiled = await inspectScene(page);
  expect(editedCompiled.compiled.bundle_sha256).not.toBe(beforeEditCompiled.compiled.bundle_sha256);

  await page.getByRole('button', { name: 'Undo', exact: true }).click();
  expect((await sceneSnapshot(page)).entrances).toEqual(beforeEdit.entrances);
  expect((await sceneSnapshot(page)).events.bytes).toEqual(beforeEdit.events.bytes);
  expect((await sceneSnapshot(page)).events.bindings).toEqual(beforeEdit.events.bindings);
  expect((await inspectScene(page)).compiled.bundle_sha256).toBe(beforeEditCompiled.compiled.bundle_sha256);
  await page.getByRole('button', { name: 'Redo', exact: true }).click();
  expect((await sceneSnapshot(page)).entrances).toEqual(afterEdit.entrances);
  expect((await sceneSnapshot(page)).events.bytes).toEqual(afterEdit.events.bytes);
  expect((await sceneSnapshot(page)).events.bindings).toEqual(afterEdit.events.bindings);
  expect((await inspectScene(page)).compiled.bundle_sha256).toBe(editedCompiled.compiled.bundle_sha256);

  const beforeInvalid = await sceneSnapshot(page);
  const invalidDocument = await inspectScene(page);
  await inspectEntrance(page, 65535);
  const diagnostic = page.locator('.scene-entrance-picker [role="alert"]');
  await expect(diagnostic).toContainText('entrance selector list exceeds bounded prefix capacity');
  expect(await sceneSnapshot(page)).toEqual(beforeInvalid);
  const afterInvalidDocument = await inspectScene(page);
  expect(afterInvalidDocument.document).toEqual(invalidDocument.document);
  expect(afterInvalidDocument.compiled.bundle_sha256).toBe(invalidDocument.compiled.bundle_sha256);
  expect(afterInvalidDocument.compiled.bundle.length).toBe(invalidDocument.compiled.bundle.length);
  expect(afterInvalidDocument.compiled.bundle.every((byte, index) => byte === invalidDocument.compiled.bundle[index])).toBe(true);
});
