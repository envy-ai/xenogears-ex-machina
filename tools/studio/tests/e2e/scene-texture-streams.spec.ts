import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdir, realpath, writeFile } from 'node:fs/promises';
import { basename, resolve, sep } from 'node:path';
import { expect, test, type Page } from '@playwright/test';

interface StudioTextureDriver {
  snapshot(): {
    view: string; map: number | null; sceneSelection: string | null;
    scene: {
      descriptors: { id: string; value: { flags: number; model: number } }[];
      actors: { id: string }[];
      sprite_bindings: { actor: string; node: string | null; pc: number | null; resource: number | null; bank: number | null; diagnostic: string | null }[];
      events: {
        bindings: Record<string, number>;
        graph: { instructions: SourceInstruction[] };
      };
    } | null;
    discs: { id: number; disc: number | null; mapCount: number; placeholders: number }[];
  };
  inspectScene(): {
    document: { input: { bundle: number[]; source?: { source_sha256: string; disc: number; map: number } } };
    compiled: { bundle: number[] };
  } | null;
}
interface SourceInstruction { pc: number; spec: { handler: string }; operands: number[]; immediate: (boolean | null)[] }
interface TextureStats {
  source: string;
  meshes: number;
  texturedMeshes: number;
  texturedTriangles: number;
  texturePages: number;
  spriteParts: number;
  spriteActors: { actor: string; resource: number; bank: number; frame: number; parts: number; pc: number | null; node: string | null }[];
  map_stream: null | {
    file: number; slot: number; lba: number; indexed_size: number; source_sha256: string;
    image_count: number; source_bytes_read: number; stop_reason: string; upload_count: number;
  };
}
interface Candidate { disc: 1 | 2; map: number; slot: number; frameId: string; sha256: string; indexedSize: number; images: number; strips: number }

const enabled = process.env.XEM_STUDIO_SCENE_TEXTURE_STREAMS === '1';
const candidates: Candidate[] = [
  { disc: 1, map: 1, slot: 609, frameId: 'descriptor:117', sha256: '5560c5b6ad0e35c3b9ed55edda224d76248693df7384adad1730df296c18549c', indexedSize: 526_336, images: 12, strips: 245 },
  { disc: 1, map: 2, slot: 611, frameId: 'descriptor:76', sha256: '77fbf237424b767968ed49dfacd89b1b5c62470bcb78e04560348df95b1d7259', indexedSize: 464_896, images: 12, strips: 215 },
  { disc: 2, map: 23, slot: 648, frameId: 'descriptor:25', sha256: '739b9fe57fc30c361d3bef8c05db9a9d822b961ba7f8b735574c9e74dd43e36b', indexedSize: 256_000, images: 7, strips: 118 },
];
const repoRoot = resolve(import.meta.dirname, '../../../..');

function required(name: string): string {
  const value = process.env[name];
  if (!value) throw new Error(`${name} is required for the explicit local texture-stream check`);
  return resolve(value);
}

function sha256(value: string): string {
  return createHash('sha256').update(value).digest('hex');
}

async function stats(page: Page): Promise<TextureStats> {
  return page.locator('canvas[aria-label="3D field scene viewport"]').evaluate(canvas => {
    const api = (canvas as HTMLCanvasElement & { __xemScene?: { textureStats(): TextureStats } }).__xemScene;
    if (!api) throw new Error('The test-only Scene texture inspection bridge is unavailable');
    return api.textureStats();
  });
}
async function projected(page: Page, id: string): Promise<{ x: number; y: number; z: number; visible: boolean }> {
  return page.locator('canvas[aria-label="3D field scene viewport"]').evaluate((canvas, entity) => {
    const bridge = (canvas as HTMLCanvasElement & { __xemScene?: { project(id: string): { x: number; y: number; z: number; visible: boolean } } }).__xemScene;
    if (!bridge) throw new Error('The test-only Scene projection bridge is unavailable');
    return bridge.project(entity);
  }, id);
}

test.skip(!enabled, 'set XEM_STUDIO_SCENE_TEXTURE_STREAMS=1 for explicit local disc texture coverage');
test('mounts streamed map textures from both original discs in the normal Scene view', async ({ page }) => {
  test.setTimeout(720_000);
  const extractRoot = await realpath(required('XEM_STUDIO_EXTRACT_ROOT'));
  const disc1 = await realpath(required('XEM_STUDIO_DISC1'));
  const disc2 = await realpath(required('XEM_STUDIO_DISC2'));
  const outputRoot = required('XEM_STUDIO_SCENE_TEXTURE_OUTPUT');
  assert.equal(basename(disc1), 'disc1.bin');
  assert.equal(basename(disc2), 'disc2.bin');
  assert.ok(disc1.startsWith(`${extractRoot}${sep}`) && disc2.startsWith(`${extractRoot}${sep}`), 'both selected images remain inside the measured extraction root');
  assert.ok(outputRoot.startsWith(`${resolve(repoRoot, 'tmp/studio')}${sep}`), 'private Scene evidence stays inside ignored tmp/studio');
  await mkdir(outputRoot, { recursive: false });

  const errors: string[] = [];
  page.on('pageerror', error => errors.push(error.message));
  await page.setViewportSize({ width: 1440, height: 900 });
  await page.goto('/');
  await expect(page.getByRole('main', { name: 'XEM Studio' })).toBeVisible();
  await expect(page.locator('[data-panel-id="backend"]')).toContainText('xem-studio 0.5.0');
  await page.getByLabel('Open disc files').setInputFiles([disc1, disc2]);
  const snapshot = () => page.evaluate(() => (window as unknown as { __xemStudioTest?: StudioTextureDriver }).__xemStudioTest?.snapshot() ?? null);
  const driver = async () => (await snapshot())?.discs ?? [];
  await expect.poll(async () => {
    const sessions = await driver();
    return [1, 2].every(number => sessions.some(session => session.disc === number));
  }, { timeout: 240_000, intervals: [1000, 2500, 5000] }).toBe(true);

  const result: unknown[] = [];
  for (const candidate of candidates) {
    await page.getByRole('tab', { name: 'Command list', exact: true }).click();
    const session = (await driver()).find(item => item.disc === candidate.disc);
    assert.ok(session, `disc ${candidate.disc} source session is open`);
    const discButton = page.getByRole('button', { name: new RegExp(`^Disc ${candidate.disc} · ${session.mapCount} maps$`) });
    if (await discButton.getAttribute('aria-pressed') !== 'true') await discButton.click();
    const mapRow = page.locator(`[data-map="${candidate.map}"]`);
    await expect(mapRow).toBeVisible();
    await mapRow.click();
    await expect(mapRow).toHaveAttribute('aria-pressed', 'true', { timeout: 120_000 });
    await expect.poll(() => page.evaluate(() => (window as unknown as { __xemStudioTest?: StudioTextureDriver }).__xemStudioTest?.snapshot().map ?? null),
      { timeout: 120_000, intervals: [250, 1000, 2500] }).toBe(candidate.map);
    await page.getByRole('tab', { name: 'Scene', exact: true }).click();
    const canvas = page.locator('canvas[aria-label="3D field scene viewport"]');
    await expect(canvas).toBeVisible({ timeout: 120_000 });
    await expect(page.getByRole('tabpanel', { name: 'Scene' })).toBeVisible();
    const scene = await page.evaluate(() => (window as unknown as { __xemStudioTest?: StudioTextureDriver }).__xemStudioTest?.inspectScene());
    assert.ok(scene, 'the Rust Scene document opened for the selected disc/map');
    assert.equal(scene.document.input.source?.disc, candidate.disc);
    assert.equal(scene.document.input.source?.map, candidate.map);

    const opened = await snapshot();
    assert.ok(opened?.scene, 'the Rust-owned scene snapshot is available');
    const descriptorIndex = opened.scene.descriptors.findIndex(item => item.id === candidate.frameId);
    assert.ok(descriptorIndex >= opened.scene.actors.length, 'the chosen frame target is a static descriptor, not an actor marker');
    const frameTarget = opened.scene.descriptors[descriptorIndex];
    assert.equal(frameTarget.value.flags & 0x40, 0, 'the static descriptor does not use the identity-marker flag');
    const filter = page.getByLabel('Filter scene entities');
    await filter.fill(candidate.frameId);
    const row = page.locator(`[data-scene-id="${candidate.frameId}"]`);
    await expect(row).toBeVisible();
    await row.click();
    await expect.poll(async () => (await snapshot())?.sceneSelection ?? null).toBe(candidate.frameId);
    for (const layer of ['Collision', 'Trigger zones', 'Entrances']) {
      const checkbox = page.getByRole('checkbox', { name: layer, exact: true });
      if (await checkbox.isChecked()) await checkbox.uncheck();
    }
    await page.getByRole('button', { name: 'Frame selection', exact: true }).click();

    const mounted = await stats(page);
    const streamed = mounted.map_stream;
    assert.ok(streamed, 'Scene uses the completed map companion returned by the DiscSession transport');
    assert.equal(streamed.file, 0xB9 + 2 * candidate.map);
    assert.equal(streamed.slot, candidate.slot);
    assert.equal(streamed.source_sha256, candidate.sha256);
    assert.equal(streamed.indexed_size, candidate.indexedSize);
    assert.equal(streamed.image_count, candidate.images);
    assert.equal(streamed.upload_count, candidate.strips);
    assert.ok(streamed.source_bytes_read <= streamed.indexed_size);

    assert.equal(mounted.source, scene.document.input.source?.source_sha256);
    assert.ok(mounted.meshes > 0, 'source geometry is mounted');
    assert.ok(mounted.texturedMeshes > 0, 'source materials use sampled VRAM pages');
    assert.ok(mounted.texturedTriangles > 0, 'at least one source face is rendered with a texture');
    assert.ok(mounted.texturePages > 0, 'the Scene cached an explicit texture page/CLUT pair');
    const overviewScreenshot = `${outputRoot}/disc${candidate.disc}-map${candidate.map}-textures-1440x900.png`;
    const overviewImage = await page.screenshot({ path: overviewScreenshot });

    const stableActors = new Set(opened.scene.actors.map(actor => actor.id));
    const qualified = opened.scene.sprite_bindings.filter(binding =>
      binding.resource !== null && binding.bank !== null && binding.pc !== null);
    assert.ok(qualified.length > 0, `disc ${candidate.disc} map ${candidate.map} has a source-qualified actor sprite binding`);
    for (const binding of qualified) {
      assert.ok(stableActors.has(binding.actor), 'sprite source binding uses a stable scene actor ID');
      const instruction: SourceInstruction | undefined = opened.scene.events.graph.instructions.find(item => item.pc === binding.pc);
      assert.ok(instruction, 'binding PC maps to an actual source instruction');
      assert.ok(['field_event_set_sprite', 'field_event_set_sprite_parameter'].includes(instruction.spec.handler));
      assert.equal(instruction.immediate[0], true, 'resource selection is a source immediate');
      assert.equal((instruction.operands[0] & 0x7fff), binding.resource);
      if (instruction.spec.handler === 'field_event_set_sprite_parameter') {
        assert.equal(instruction.immediate[1], true, 'bank selection is a source immediate');
        assert.equal((instruction.operands[1] & 0x7fff), binding.bank);
      } else {
        assert.equal(binding.bank, 0, 'primary set_sprite selects source bank zero');
      }
      if (binding.node !== null) assert.equal(opened.scene.events.bindings[binding.node], binding.pc);
    }
    assert.ok(mounted.spriteParts > 0, 'at least one original source sprite part is mounted');
    assert.ok(mounted.spriteActors.length > 0, 'the mounted actor has a real source sprite, not the unresolved identity marker');
    for (const rendered of mounted.spriteActors) {
      const binding = qualified.find(row => row.actor === rendered.actor);
      assert.ok(binding, 'every displayed sprite is backed by a qualified scene binding');
      assert.equal(rendered.resource, binding.resource);
      assert.equal(rendered.bank, binding.bank);
      assert.equal(rendered.pc, binding.pc);
      assert.equal(rendered.node, binding.node);
    }
    assert.equal(mounted.spriteParts, mounted.spriteActors.reduce((sum, actor) => sum + actor.parts, 0));

    const compiledSha = await page.evaluate(async () => {
      const value = (window as unknown as { __xemStudioTest?: StudioTextureDriver }).__xemStudioTest?.inspectScene()?.compiled.bundle;
      if (!value) return null;
      const digest = await crypto.subtle.digest('SHA-256', new Uint8Array(value));
      return [...new Uint8Array(digest)].map(byte => byte.toString(16).padStart(2, '0')).join('');
    });
    assert.equal(compiledSha, scene.document.input.source?.source_sha256, 'opening Scene without edits preserves the exact source bundle bytes');

    const renderedActor = mounted.spriteActors[0];
    await filter.fill(renderedActor.actor);
    const actorRow = page.locator(`[data-scene-id="${renderedActor.actor}"]`);
    await expect(actorRow).toBeVisible();
    await actorRow.click();
    await expect.poll(async () => (await snapshot())?.sceneSelection ?? null).toBe(renderedActor.actor);
    await page.getByRole('button', { name: 'Frame selection', exact: true }).click();
    await expect.poll(async () => (await projected(page, renderedActor.actor)).visible).toBe(true);
    const spriteFrameStats = await stats(page);
    assert.ok(spriteFrameStats.spriteActors.some(actor => actor.actor === renderedActor.actor));

    // Keep the current camera and use a normal pointer ray hit to restore this
    // sprite selection after selecting the static source geometry row.
    await filter.fill(candidate.frameId);
    const geometryRow = page.locator(`[data-scene-id="${candidate.frameId}"]`);
    await expect(geometryRow).toBeVisible();
    await geometryRow.click();
    await expect.poll(async () => (await snapshot())?.sceneSelection ?? null).toBe(candidate.frameId);
    const geometryLayer = page.getByRole('checkbox', { name: 'Geometry', exact: true });
    await geometryLayer.uncheck();
    await expect.poll(async () => (await snapshot())?.sceneSelection ?? null).toBe(candidate.frameId);
    const point = await projected(page, renderedActor.actor);
    assert.ok(point.visible, 'the source sprite intersects the current camera view after hiding occluding geometry');
    await page.mouse.click(point.x, point.y);
    await expect.poll(async () => (await snapshot())?.sceneSelection ?? null).toBe(renderedActor.actor);
    const beforeOrbit = await stats(page);
    const canvasBounds = await canvas.boundingBox();
    assert.ok(canvasBounds);
    const beforeOrbitPoint = await projected(page, candidate.frameId);
    const startX = canvasBounds.x + canvasBounds.width * 0.8;
    const startY = canvasBounds.y + canvasBounds.height * 0.7;
    assert.ok(Math.hypot(startX - beforeOrbitPoint.x, startY - beforeOrbitPoint.y) > 80,
      'orbit starts in empty canvas space, clear of the sprite and its transform gizmo');
    await page.mouse.move(startX, startY);
    await page.mouse.down();
    await page.mouse.move(startX + 34, startY + 22, { steps: 4 });
    await page.mouse.up();
    await expect.poll(() => page.evaluate(() => (window as unknown as { __xemStudioTest?: StudioTextureDriver }).__xemStudioTest?.snapshot().sceneSelection ?? null)).toBe(renderedActor.actor);
    await expect.poll(async () => {
      const after = await projected(page, candidate.frameId);
      return Math.hypot(after.x - beforeOrbitPoint.x, after.y - beforeOrbitPoint.y);
    }).toBeGreaterThan(1);
    assert.deepEqual(await stats(page), beforeOrbit, 'orbiting the source sprite does not change source bindings or rendered parts');

    const spriteScreenshot = `${outputRoot}/disc${candidate.disc}-map${candidate.map}-sprite-1440x900.png`;
    await geometryLayer.check();
    const spriteImage = await page.screenshot({ path: spriteScreenshot });
    await writeFile(`${outputRoot}/disc${candidate.disc}-map${candidate.map}-sprite.json`, `${JSON.stringify({
      disc: candidate.disc,
      map: candidate.map,
      actor: renderedActor.actor,
      resource: renderedActor.resource,
      bank: renderedActor.bank,
      source_pc: renderedActor.pc,
      parts: renderedActor.parts,
      screenshot: spriteScreenshot.slice(outputRoot.length + 1),
      screenshot_sha256: createHash('sha256').update(spriteImage).digest('hex'),
    }, null, 2)}\n`, { flag: 'wx' });

    await page.getByRole('tab', { name: 'Command list', exact: true }).click();
    await expect.poll(async () => (await snapshot())?.view ?? null).toBe('list');
    await page.getByRole('tab', { name: 'Scene', exact: true }).click();
    await expect(page.locator('canvas[aria-label="3D field scene viewport"]')).toBeVisible();
    assert.deepEqual(await stats(page), mounted, 'Scene/List/Scene retains the same mounted texture pages and uploads');
    await page.getByRole('button', { name: 'Frame selection', exact: true }).click();

    const entry = {
      disc: candidate.disc,
      map: candidate.map,
      stream: streamed,
      stream_metadata_sha256: sha256(JSON.stringify(streamed)),
      scene_source_sha256: scene.document.input.source?.source_sha256 ?? null,
      texture_stats: mounted,
      texture_overview_screenshot: overviewScreenshot.slice(outputRoot.length + 1),
      texture_overview_sha256: createHash('sha256').update(overviewImage).digest('hex'),
      sprite_screenshot: spriteScreenshot.slice(outputRoot.length + 1),
      sprite_screenshot_sha256: createHash('sha256').update(spriteImage).digest('hex'),
    };
    await writeFile(`${outputRoot}/disc${candidate.disc}-map${candidate.map}.json`, `${JSON.stringify(entry, null, 2)}\n`, { flag: 'wx' });
    result.push(entry);
  }
  assert.deepEqual(result.map((item) => (item as { map: number }).map), [1, 2, 23]);
  expect(errors).toEqual([]);
});
