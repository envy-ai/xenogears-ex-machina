import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdir, readFile, realpath, stat, writeFile } from 'node:fs/promises';
import { join, resolve, sep } from 'node:path';
import { expect, type Locator, type Page } from '@playwright/test';
import type { CompiledScene, SceneDocument, SceneSnapshot } from '../../src/backend/scenes';
import type { SourceIdentity } from '../../src/backend/discs';

const repoRoot = resolve(import.meta.dirname, '../../../..');
export const expectedModule = '613c3a747e1c07e0e0e2632710eae1f873e4b14dda880cd6ce1c556b31c152a8';
const expectedRuntime = resolve(repoRoot, 'tmp/studio/milestone-6/checkpoint-36/web');

export interface SceneInspection { owner: number | null; document: SceneDocument; compiled: CompiledScene }
interface SceneDriverSnapshot {
  view: string;
  map: number | null;
  selection: { actor: number; event: number; pc: number | null };
  scene: SceneSnapshot | null;
  discs: { id: number; disc: number | null | undefined; mapCount: number; placeholders: number }[];
}
interface SceneDriverSummary {
  view: string;
  map: number | null;
  selection: { actor: number; event: number; pc: number | null };
  scene: { revision: number; actors: SceneSnapshot['actors'] } | null;
  discs: SceneDriverSnapshot['discs'];
}
interface SceneDriver {
  snapshot(): SceneDriverSnapshot;
  inspectScene(): SceneInspection | null;
}
interface BrowserRuntime {
  boot(options: { run: boolean; rewind: boolean }): Promise<unknown>;
  commandJson(json: string): string;
  run(command: { cmd: 'frames'; count: number }): Promise<unknown>;
  status(): { running: boolean; session: { vblanks?: number | bigint } | null; page: { sessionGeneration: number } };
}
export interface CandidatePlacement {
  actorId: string;
  actorIndex: number;
  runtimeActor: number;
  placementNode: string;
  placementOption: string;
  axes: { axis: string; enabled: boolean; value: number | null }[];
}

function sha256(bytes: Uint8Array): string {
  return createHash('sha256').update(bytes).digest('hex');
}

export function requiredPath(name: string): string {
  const value = process.env[name];
  if (!value) throw new Error(`${name} is required for the gated M7 original-disc Scene discovery`);
  return resolve(value);
}

export async function inspectInPage(page: Page): Promise<{
  snapshot: SceneDriverSummary;
  owner: number | null;
  document: {
    schema_version: number;
    source: SourceIdentity | null;
    context: null | {
      schema_version: number;
      provenance: string;
      module_sha256: string;
      identity: SourceIdentity;
      allocation_size: number;
      load_vblank: string;
      components: { index: number; source_offset: number; logical_size: number; packed_size: number; source_bytes_read: number; output_size: number; output_sha256: string; extra_count: number }[];
    };
    entrances: number[];
    action_count: number;
    cursor: number;
    next_id: number;
    next_event_id: number;
    sha256: string;
  };
  compiled: { bundle_sha256: string; source_sha256: string; bundle_length: number; event_bytes_sha256: string; event_bytes_length: number; event_bindings: Record<string, number> };
}> {
  return page.evaluate(async () => {
    const hook = (window as unknown as { __xemStudioTest?: SceneDriver }).__xemStudioTest;
    if (!hook) throw new Error('The explicit test-only inspection driver is unavailable');
    const current = hook.snapshot();
    const scene = current.scene;
    const inspection = hook.inspectScene();
    if (!scene || !inspection) throw new Error('The original-disc Scene owner is not open');
    const context = inspection.document.input.context ?? null;
    const digest = async (values: number[]) => {
      const bytes = new Uint8Array(values);
      const hash = await crypto.subtle.digest('SHA-256', bytes.buffer as ArrayBuffer);
      return Array.from(new Uint8Array(hash), value => value.toString(16).padStart(2, '0')).join('');
    };
    return {
      snapshot: {
        view: current.view,
        map: current.map,
        selection: current.selection,
        scene: { revision: scene.revision, actors: scene.actors.map(actor => ({
          id: actor.id, event_source_row: actor.event_source_row, origin: actor.origin,
        })) },
        discs: current.discs.map(disc => ({
          id: disc.id, disc: disc.disc, mapCount: disc.mapCount, placeholders: disc.placeholders,
        })),
      },
      owner: inspection.owner,
      document: {
        schema_version: inspection.document.schema_version,
        source: inspection.document.input.source ?? null,
        context: context && {
          schema_version: context.schema_version,
          provenance: context.provenance,
          module_sha256: context.module_sha256,
          identity: context.identity,
          allocation_size: context.allocation_size,
          load_vblank: context.load_vblank,
          components: context.components.map(component => ({
            index: component.index,
            source_offset: component.source_offset,
            logical_size: component.logical_size,
            packed_size: component.packed_size,
            source_bytes_read: component.source_bytes_read,
            output_size: component.output_size,
            output_sha256: component.output_sha256,
            extra_count: component.extra.length,
          })),
        },
        entrances: inspection.document.entrance_selectors,
        action_count: inspection.document.actions.length,
        cursor: inspection.document.cursor,
        next_id: inspection.document.next_id,
        next_event_id: inspection.document.next_event_id,
        sha256: await digest(Array.from(new TextEncoder().encode(JSON.stringify(inspection.document)))),
      },
      compiled: {
        bundle_sha256: inspection.compiled.bundle_sha256,
        source_sha256: inspection.compiled.source_sha256,
        bundle_length: inspection.compiled.bundle.length,
        event_bytes_sha256: await digest(inspection.compiled.events.bytes),
        event_bytes_length: inspection.compiled.events.bytes.length,
        event_bindings: inspection.compiled.events.bindings,
      },
    };
  });
}

export async function publicRuntimeStatus(gamePage: Page): Promise<{ identity: { module: string; disc: string; packages: string[] }; mode: number | null; disc: number | null; vblanks: number }> {
  return gamePage.evaluate(() => {
    const runtime = (window as unknown as { xem: BrowserRuntime }).xem;
    const status = JSON.parse(runtime.commandJson('{"cmd":"status"}')) as { identity: { module: string; disc: string; packages: string[] }; mode?: number; disc?: number; vblanks?: number };
    const vblanks = Number(status.vblanks);
    if (!Number.isSafeInteger(vblanks) || vblanks < 0) throw new Error('Runtime status did not return a safe nonnegative VBlank count');
    return { identity: status.identity, mode: status.mode ?? null, disc: status.disc ?? null, vblanks };
  });
}

export async function startPublicFieldRoute(gamePage: Page, discPath: string): Promise<void> {
  await gamePage.locator('#file').setInputFiles(discPath);
  await expect.poll(() => gamePage.locator('#message').textContent(), { timeout: 120_000, intervals: [250, 1000, 2500] })
    .toMatch(/^Xenogears disc 1 \(/);
  await gamePage.evaluate(async () => {
    const runtime = (window as unknown as { xem: BrowserRuntime }).xem;
    const command = (value: Record<string, unknown>) => {
      const reply = JSON.parse(runtime.commandJson(JSON.stringify(value))) as { error?: unknown };
      if (reply && typeof reply === 'object' && 'error' in reply) throw new Error(`Public scenario bootstrap failed for ${String(value.cmd)}`);
    };
    await runtime.boot({ run: false, rewind: false });
    command({ cmd: 'boot_mode', mode: 0 });
    const title = await runtime.run({ cmd: 'frames', count: 303 }) as { error?: unknown };
    if (title?.error) throw new Error('Public title frame route failed');
    command({ cmd: 'scenario', mode: 1, writes: [
      { path: 'game_data.map', value: 23 },
      { path: 'mode_kernel_menu_running', value: 0 },
      { path: 'mode_field_entered_once', value: 0 },
    ] });
    const field = await runtime.run({ cmd: 'frames', count: 430 }) as { error?: unknown };
    if (field?.error) throw new Error('Public map-23 frame route failed');
    const selected = JSON.parse(runtime.commandJson('{"cmd":"inspect","path":"mode_field_map_id","depth":0}')) as number;
    if ((Number(selected) & 0x0fff) !== 23) throw new Error('The public scenario route did not enter map 23');
  });
}

export async function findEditablePlacement(
  page: Page,
  scene: Pick<SceneSnapshot, 'actors'>,
  expected?: { actorId: string; placementNode: string },
): Promise<CandidatePlacement> {
  const filter = page.getByRole('textbox', { name: 'Filter scene entities' });
  const axes = ['X', 'Y', 'Z'];
  for (let actorIndex = 0; actorIndex < scene.actors.length; actorIndex++) {
    const actor = scene.actors[actorIndex]!;
    if (expected && actor.id !== expected.actorId) continue;
    if (actor.origin.kind !== 'original') continue;
    await filter.fill(actor.id);
    const row = page.locator(`[data-scene-id="${actor.id}"]`);
    await expect(row, `original source actor row ${actor.id}`).toBeVisible();
    await row.click();
    const binding = page.getByRole('combobox', { name: 'Authored placement source' });
    const choices = await binding.locator('option').evaluateAll(options => options.map(option => ({ value: (option as HTMLOptionElement).value, text: option.textContent ?? '' })));
    for (const choice of choices.filter(choice => choice.value !== 'header'
      && (!expected || choice.value === expected.placementNode))) {
      await binding.selectOption(choice.value);
      const openCommand = page.getByRole('button', { name: `Open bound command · ${choice.value}`, exact: true });
      if (!(await openCommand.isVisible())) continue;
      const fields = await Promise.all(axes.map(async axis => {
        const input = page.getByRole('spinbutton', { name: `Scene position ${axis}` });
        const raw = await input.inputValue();
        const value = raw.trim() === '' ? null : Number(raw);
        return { axis, enabled: await input.isEnabled(), value: value !== null && Number.isFinite(value) ? value : null };
      }));
      if (fields.some(field => field.enabled && field.value !== null)) {
        return {
          actorId: actor.id,
          actorIndex,
          runtimeActor: actor.origin.actor,
          placementNode: choice.value,
          placementOption: choice.text,
          axes: fields,
        };
      }
    }
  }
  throw new Error('The original map has no visible source actor with an immediate editable placement binding');
}

export interface RealSceneContinuation {
  page: Page;
  gamePage: Page;
  live: Locator;
  artifactRoot: string;
  disc1: string;
  disc2: string;
  candidate: CandidatePlacement;
}

export async function runRealSceneStageA(
  page: Page,
  continueAfterReadOnly?: (context: RealSceneContinuation) => Promise<void>,
): Promise<void> {
  const artifactRoot = requiredPath('XEM_STUDIO_M7_REAL_SCENE_OUTPUT');
  const extractRoot = await realpath(requiredPath('XEM_STUDIO_EXTRACT_ROOT'));
  const disc1 = await realpath(requiredPath('XEM_STUDIO_DISC1'));
  const disc2 = await realpath(requiredPath('XEM_STUDIO_DISC2'));
  const runtimeDir = await realpath(requiredPath('XEM_STUDIO_M7_RUNTIME_DIR'));
  const expectedArtifactParent = resolve(repoRoot, 'tmp/studio/milestone-7');
  assert.ok(artifactRoot.startsWith(expectedArtifactParent + sep), 'private Stage A outputs must stay in a fresh child of ignored M7 storage');
  const recordProgress = async (name: string, progress: Record<string, unknown>) => {
    const progressPath = join(artifactRoot, `progress-${name}.json`);
    await writeFile(progressPath, `${JSON.stringify({ schema_version: 1, ...progress }, null, 2)}\n`, { flag: 'wx' });
  };
  assert.equal(disc1, resolve(extractRoot, 'discs/disc1.bin'), 'Studio and game use the selected original Disc 1 BIN');
  assert.equal(disc2, resolve(extractRoot, 'discs/disc2.bin'), 'Studio opens the selected original Disc 2 BIN as well');
  assert.equal(runtimeDir, expectedRuntime, 'the original M6 runtime build is reused without rebuilding');
  assert.equal(await stat(disc1).then(info => info.isFile()), true);
  assert.equal(await stat(disc2).then(info => info.isFile()), true);
  assert.equal(await stat(join(runtimeDir, 'game/game.wasm')).then(info => info.isFile()), true);
  assert.equal(await stat(join(runtimeDir, 'game/schema.json')).then(info => info.isFile()), true);
  const schema = JSON.parse(await readFile(join(runtimeDir, 'game/schema.json'), 'utf8')) as { module_sha256?: unknown };
  const moduleSha256 = sha256(await readFile(join(runtimeDir, 'game/game.wasm')));
  assert.equal(schema.module_sha256, expectedModule, 'the reused original game module has the recorded M6 module identity');
  assert.equal(moduleSha256, expectedModule, 'the runtime module bytes match the selected schema');
  await mkdir(artifactRoot, { recursive: false });

  const pageErrors: string[] = [];
  page.on('pageerror', error => pageErrors.push(error.message));
  await page.goto('/');
  await expect(page.getByRole('main', { name: 'XEM Studio' })).toBeVisible();
  await expect(page.locator('[data-panel-id="backend"]')).toContainText('xem-studio 0.5.0');
  await expect.poll(() => page.evaluate(() => typeof (window as unknown as { __xemStudioTest?: SceneDriver }).__xemStudioTest?.inspectScene)).toBe('function');

  await page.getByLabel('Open disc files').setInputFiles([disc1, disc2]);
  await expect.poll(async () => page.evaluate(() => {
    const discs = (window as unknown as { __xemStudioTest?: SceneDriver }).__xemStudioTest?.snapshot().discs ?? [];
    return [1, 2].every(number => discs.some(item => item.disc === number));
  }), { timeout: 240_000, intervals: [1000, 2500, 5000] }).toBe(true);
  await page.getByRole('button', { name: /^Disc 1 · \d+ maps$/ }).click();
  const map23 = page.locator('[data-map="23"]');
  await expect(map23).toBeVisible({ timeout: 120_000 });
  await map23.click();
  await expect.poll(() => page.evaluate(() => (window as unknown as { __xemStudioTest?: SceneDriver }).__xemStudioTest?.snapshot().map ?? null), {
    timeout: 120_000, intervals: [250, 1000, 2500],
  }).toBe(23);
  const discBrowser = page.getByRole('region', { name: 'Disc and map browser' });
  const discLoadStatus = discBrowser.locator(':scope > p[role="status"]');
  await expect(discLoadStatus).not.toContainText('Loading map 23', { timeout: 120_000 });
  await expect(discLoadStatus).toHaveText('2 recognized disc sessions');
  await recordProgress('map-loaded', {
    stage: 'disc-map-load-complete',
    map: 23,
    disc_sessions: await page.evaluate(() => (window as unknown as { __xemStudioTest?: SceneDriver }).__xemStudioTest?.snapshot().discs.map(disc => ({
      disc: disc.disc, map_count: disc.mapCount, placeholders: disc.placeholders,
    })) ?? []),
  });

  const popupPromise = page.waitForEvent('popup');
  let initialAttachDiagnostic: string | null = null;
  await page.getByRole('button', { name: 'Open game & attach' }).click();
  const gamePage = await popupPromise;
  const gamePageErrors: string[] = [];
  gamePage.on('pageerror', error => gamePageErrors.push(error.message));
  await gamePage.waitForLoadState('domcontentloaded');
  await gamePage.waitForFunction(() => document.body.dataset.ready === 'true', undefined, { timeout: 120_000 });
  assert.equal(new URL(gamePage.url()).origin, new URL(page.url()).origin, 'the runtime popup remains same-origin with Studio');
  await expect(gamePage).toHaveTitle(/Xenogears Ex Machina/);
  await startPublicFieldRoute(gamePage, disc1);

  // Reconnect through the ordinary Live control after booting the real browser game.
  const globalErrors = page.getByRole('alert', { name: 'Studio errors', exact: true });
  if (await globalErrors.count()) {
    const text = (await globalErrors.textContent()) ?? '';
    assert.match(text, /Cannot attach to browser game|no acknowledged booted session generation/i,
      'only the expected pre-boot attach diagnostic may be dismissed');
    initialAttachDiagnostic = text.split('\n')[1]?.trim() ?? text.trim();
    await globalErrors.getByRole('button', { name: 'Dismiss errors', exact: true }).click();
    await expect(globalErrors).toHaveCount(0);
  }
  await page.getByRole('button', { name: 'Open game & attach' }).click();
  const live = page.getByRole('region', { name: 'Live game' });
  await expect(live.getByText(/browser · Paused · acknowledged/)).toBeVisible({ timeout: 30_000 });
  const runtimeBeforeScene = await publicRuntimeStatus(gamePage);
  assert.equal(runtimeBeforeScene.identity.module, expectedModule);
  assert.deepEqual(runtimeBeforeScene.identity.packages, []);
  assert.equal(runtimeBeforeScene.identity.disc, 'SLUS-00664+SLUS-00669');
  assert.equal(runtimeBeforeScene.disc, 1);
  assert.equal(runtimeBeforeScene.mode, 1);

  await page.getByRole('tab', { name: 'Scene', exact: true }).click();
  await expect(page.getByRole('tabpanel', { name: 'Scene' })).toBeVisible({ timeout: 120_000 });
  await expect(page.getByRole('tree', { name: 'Scene entities' })).toBeVisible();
  await expect(discLoadStatus).toHaveText('2 recognized disc sessions');
  const firstScene = await inspectInPage(page);
  assert.ok(firstScene.document.source, 'the actual DiscSession mapBundle supplies source identity');
  assert.notEqual(firstScene.owner, null, 'the actual Scene has an owning Rust scene handle');
  assert.equal(firstScene.document.source.disc, 1);
  assert.equal(firstScene.document.source.map, 23);
  assert.equal(firstScene.document.schema_version, 4);
  assert.equal(firstScene.compiled.source_sha256, firstScene.document.source.source_sha256);
  assert.equal(firstScene.snapshot.map, 23);
  await expect(page.getByLabel('Current map')).toHaveText('Disc 1 · Map 23');
  await expect(page.locator('[data-map="23"][aria-pressed="true"]')).toBeVisible();

  const scene = firstScene.snapshot.scene;
  assert.ok(scene, 'the actual original-disc Scene is open');
  const initialContext = firstScene.document.context;
  assert.ok(initialContext, 'opening the original map acquires the measured context needed for its complete packed component');
  assert.equal(initialContext.provenance, 'runtime_field_decompress');
  assert.equal(initialContext.module_sha256, runtimeBeforeScene.identity.module);
  assert.equal(initialContext.components.length, 9);
  await recordProgress('scene-open', {
    phase: 'original-scene-open',
    source: firstScene.document.source,
    runtime_module_sha256: runtimeBeforeScene.identity.module,
    runtime_packages: runtimeBeforeScene.identity.packages,
    map: firstScene.snapshot.map,
    scene_owner: firstScene.owner,
    scene_revision: scene.revision,
    context_components: initialContext.components.map(component => ({
      index: component.index,
      source_offset: component.source_offset,
      logical_size: component.logical_size,
      packed_size: component.packed_size,
      source_bytes_read: component.source_bytes_read,
      output_size: component.output_size,
      output_sha256: component.output_sha256,
    })),
  });
  const candidate = await findEditablePlacement(page, scene);
  const selectedActor = scene.actors[candidate.actorIndex]!;
  assert.equal(selectedActor.id, candidate.actorId);
  assert.equal(selectedActor.origin.kind, 'original');
  assert.equal(selectedActor.origin.actor, candidate.runtimeActor);
  await expect(page.getByRole('button', { name: `Open bound command · ${candidate.placementNode}`, exact: true })).toBeVisible();
  await expect(page.getByRole('spinbutton', { name: /Scene position [XYZ]/ }).first()).toBeVisible();

  const bind = page.getByRole('button', { name: 'Bind original runtime source context', exact: true });
  const runtimeControls = page.locator('.scene-pose-controls');
  const runtimeControlsSummary = runtimeControls.locator('summary');
  await expect(runtimeControlsSummary).toHaveText('Runtime observation controls');
  if (!(await runtimeControls.evaluate(element => (element as HTMLDetailsElement).open))) await runtimeControlsSummary.click();
  await expect(runtimeControls).toHaveAttribute('open', '');
  const bindDisabledReason = await bind.getAttribute('title');
  await recordProgress('candidate', {
    phase: 'qualified-placement-before-context-bind',
    source: firstScene.document.source,
    runtime_module_sha256: runtimeBeforeScene.identity.module,
    scene_owner: firstScene.owner,
    scene_revision: scene.revision,
    candidate,
    context_components: initialContext.components.map(component => ({
      index: component.index,
      output_size: component.output_size,
      output_sha256: component.output_sha256,
    })),
    context_bind: { disabled: await bind.isDisabled(), disabled_reason: bindDisabledReason },
  });
  await expect(bind, `context-bind control state: ${JSON.stringify({ disabled: await bind.isDisabled(), disabled_reason: bindDisabledReason })}`)
    .toBeEnabled();
  await live.getByRole('button', { name: 'Run', exact: true }).click();
  await expect(live.getByText(/browser · Running · acknowledged/)).toBeVisible();
  const beforeBindVblank = (await publicRuntimeStatus(gamePage)).vblanks;
  await bind.click();
  await expect(live.getByText(/browser · Paused · acknowledged/)).toBeVisible();
  const afterBindVblank = (await publicRuntimeStatus(gamePage)).vblanks;
  await page.waitForTimeout(350);
  assert.equal((await publicRuntimeStatus(gamePage)).vblanks, afterBindVblank, 'source-context binding leaves the runtime paused');
  assert.ok(afterBindVblank >= beforeBindVblank);

  await expect.poll(async () => (await inspectInPage(page)).owner).not.toBe(firstScene.owner);
  const afterBind = await inspectInPage(page);
  const context = afterBind.document.context;
  assert.ok(context, 'measured original source context was adopted through the normal bind control');
  assert.notEqual(afterBind.owner, firstScene.owner, 'binding adopts a newly validated Scene owner');
  await expect(bind).toBeEnabled();
  await expect(page.getByLabel('Authored draft controls')).toContainText('Unsaved');
  await expect(globalErrors, 'context adoption must not leave an operation error').toHaveCount(0);
  await expect(page.getByRole('region', { name: 'Runtime pose read-only' }).getByRole('alert')).toHaveCount(0);
  assert.equal(context.provenance, 'runtime_field_decompress');
  assert.equal(context.module_sha256, runtimeBeforeScene.identity.module);
  assert.deepEqual(context.identity, afterBind.document.source);
  assert.equal(context.components.length, 9);
  assert.deepEqual(context.components.map(component => component.index), [0, 1, 2, 3, 4, 5, 6, 7, 8]);
  assert.deepEqual(context.components.map(component => [component.index, component.source_offset, component.logical_size,
    component.packed_size, component.source_bytes_read, component.output_size, component.output_sha256]),
  initialContext.components.map(component => [component.index, component.source_offset, component.logical_size,
    component.packed_size, component.source_bytes_read, component.output_size, component.output_sha256]));
  assert.equal(afterBind.compiled.bundle_sha256, firstScene.compiled.bundle_sha256, 'binding read-only provenance leaves the compiled author bundle unchanged');
  assert.deepEqual(afterBind.compiled.event_bindings, firstScene.compiled.event_bindings);
  assert.equal(afterBind.document.action_count, firstScene.document.action_count);
  assert.equal(afterBind.document.cursor, firstScene.document.cursor);
  assert.equal(afterBind.snapshot.scene!.revision, firstScene.snapshot.scene!.revision);
  await recordProgress('context-bound', {
    phase: 'runtime-context-bound',
    source: afterBind.document.source,
    runtime_module_sha256: context.module_sha256,
    scene_owner_before: firstScene.owner,
    scene_owner_after: afterBind.owner,
    scene_revision: afterBind.snapshot.scene!.revision,
    context_components: context.components.map(component => ({
      index: component.index,
      output_size: component.output_size,
      output_sha256: component.output_sha256,
    })),
    compiled_bundle_sha256: afterBind.compiled.bundle_sha256,
    event_bytes_sha256: afterBind.compiled.event_bytes_sha256,
    event_bindings: afterBind.compiled.event_bindings,
  });

  await live.getByRole('button', { name: 'Run', exact: true }).click();
  await expect(live.getByText(/browser · Running · acknowledged/)).toBeVisible();
  const beforeRefreshVblank = (await publicRuntimeStatus(gamePage)).vblanks;
  const refresh = page.getByRole('button', { name: 'Refresh observed actor pose', exact: true });
  await expect(refresh).toBeEnabled();
  await refresh.click();
  await expect(live.getByText(/browser · Paused · acknowledged/)).toBeVisible();
  await expect(page.getByText(/Observed VBlank \d+ · Actor \d+ · /)).toBeVisible({ timeout: 30_000 });
  const afterRefreshVblank = (await publicRuntimeStatus(gamePage)).vblanks;
  await page.waitForTimeout(350);
  assert.equal((await publicRuntimeStatus(gamePage)).vblanks, afterRefreshVblank, 'pose refresh leaves the runtime paused');
  assert.ok(afterRefreshVblank >= beforeRefreshVblank);

  const runtimePosition = await gamePage.evaluate(async actor => {
    const runtime = (window as unknown as { xem: BrowserRuntime }).xem;
    const inspect = (path: string) => JSON.parse(runtime.commandJson(JSON.stringify({ cmd: 'inspect', path, depth: 0 }))) as number;
    const base = `field_view.components.descriptors[${actor}].actor`;
    return {
      actor,
      rawPosition: [0, 1, 2].map(axis => inspect(`${base}->position[${axis}]`)),
      heading: inspect(`${base}->heading`),
    };
  }, candidate.runtimeActor);
  const poseRegion = page.getByRole('region', { name: 'Runtime pose read-only' });
  const displayedPosition = await poseRegion.locator('.scene-pose-values dd').allTextContents();
  assert.equal(displayedPosition.length, 3);
  assert.deepEqual(displayedPosition.map(Number), runtimePosition.rawPosition.map(value => value / 65536));
  await expect(poseRegion).toContainText(`Signed heading word: ${runtimePosition.heading}`);
  await recordProgress('pose-refreshed', {
    stage: 'read-only-runtime-pose-refreshed',
    actor: candidate.runtimeActor,
    vblank: afterRefreshVblank,
    raw_position: runtimePosition.rawPosition,
    heading_raw: runtimePosition.heading,
    paused_after_refresh: true,
  });

  const final = await inspectInPage(page);
  assert.equal(final.compiled.bundle_sha256, afterBind.compiled.bundle_sha256, 'read-only pose observation does not alter authored bundle bytes');
  assert.equal(final.document.action_count, afterBind.document.action_count);
  assert.equal(final.document.cursor, afterBind.document.cursor);
  assert.equal(final.snapshot.scene!.revision, afterBind.snapshot.scene!.revision);
  assert.deepEqual(final.compiled.event_bindings, afterBind.compiled.event_bindings);
  expect(pageErrors, `unexpected browser errors: ${pageErrors.join(' | ')}`).toEqual([]);
  expect(gamePageErrors, `unexpected runtime browser errors: ${gamePageErrors.join(' | ')}`).toEqual([]);

  if (await runtimeControls.evaluate(element => (element as HTMLDetailsElement).open)) await runtimeControlsSummary.click();
  await expect(runtimeControls).not.toHaveAttribute('open', '');

  const scenePath = join(artifactRoot, 'm7-real-scene.png');
  const gamePath = join(artifactRoot, 'm7-real-game.png');
  await page.evaluate(async () => { await document.fonts.ready; });
  await gamePage.evaluate(async () => { await document.fonts.ready; });
  const scenePng = await page.screenshot({ path: scenePath, fullPage: false });
  const gameCanvas = gamePage.locator('canvas').first();
  await expect(gameCanvas).toBeVisible();
  const gamePng = await gameCanvas.screenshot({ path: gamePath });
  const result = {
    schema_version: 1,
    stage: 'M7 original-disc Scene and read-only runtime pose discovery',
    source: final.document.source,
    module_sha256: context.module_sha256,
      scene_owner_replaced_on_bind: afterBind.owner !== firstScene.owner,
      initial_preboot_attach_diagnostic: initialAttachDiagnostic,
    runtime_identity: runtimeBeforeScene.identity,
    opened_discs: final.snapshot.discs.map(item => ({ disc: item.disc, maps: item.mapCount, placeholders: item.placeholders })),
    map: final.snapshot.map,
    measured_context: {
      provenance: context.provenance,
      load_vblank: context.load_vblank,
      allocation_size: context.allocation_size,
      component_count: context.components.length,
      output_sha256: context.components.map(component => ({ index: component.index, size: component.output_size, sha256: component.output_sha256 })),
    },
    authored: {
      revision_before: firstScene.snapshot.scene!.revision,
      revision_after: final.snapshot.scene!.revision,
      action_count: final.document.action_count,
      cursor: final.document.cursor,
      compiled_bundle_sha256_before: firstScene.compiled.bundle_sha256,
      compiled_bundle_sha256_after: final.compiled.bundle_sha256,
      source_event_sha256: final.compiled.event_bytes_sha256,
    },
    editable_candidate: candidate,
    runtime_pose: {
      actor: candidate.runtimeActor,
      vblank: afterRefreshVblank,
      raw_position: runtimePosition.rawPosition,
      source_position: displayedPosition.map(Number),
      heading_raw: runtimePosition.heading,
      paused_after_refresh: true,
      paused_vblank_stable: true,
    },
    scenario_route: { title_frames: 303, map: 23, field_frames: 430 },
    project_picker: 'not used in Stage A; no project file or sidecar was created',
    screenshots: {
      scene: { file: 'm7-real-scene.png', sha256: sha256(scenePng) },
      game: { file: 'm7-real-game.png', sha256: sha256(gamePng) },
    },
  };
  await writeFile(join(artifactRoot, 'stage-a-summary.json'), `${JSON.stringify(result, null, 2)}\n`, { flag: 'wx' });
  if (continueAfterReadOnly) await continueAfterReadOnly({ page, gamePage, live, artifactRoot, disc1, disc2, candidate });
}
