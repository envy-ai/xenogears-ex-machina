import { expect, test, type Page } from '@playwright/test';
import { existsSync, mkdirSync, readFileSync } from 'node:fs';
import { dirname } from 'node:path';

interface SceneTraceEntry { request_json: string; response: unknown }
interface SceneTrace { schema_version: number; synthetic_only: boolean; trace: SceneTraceEntry[] }
interface SceneSnapshot {
  revision: number;
  can_undo: boolean;
  can_redo: boolean;
  actors: { id: string; event_source_row: number }[];
  descriptors: { id: string; value: { position: number[]; rotation: number[]; flags: number; model: number } }[];
  zones: { id: string; value: { corners: number[][] } }[];
  entrances: { selector: number }[];
  events: { graph: { entries: { actor: number; event: number; pc: number }[]; instructions: { pc: number; operands: number[]; spec: { handler: string } }[] } };
}
interface SceneTestHook {
  loadScene(input: unknown, entrances?: number[], map?: number): Promise<void>;
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
