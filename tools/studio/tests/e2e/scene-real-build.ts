import assert from 'node:assert/strict';
import { createHash, randomUUID } from 'node:crypto';
import { writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { expect, type Locator, type Page } from '@playwright/test';
import type { SceneInput } from '../../src/backend/scenes';
import {
  expectedModule,
  inspectInPage,
  publicRuntimeStatus,
  requiredPath,
  runRealSceneStageA,
  type RealSceneContinuation,
  type SceneInspection,
} from './scene-real';

interface SceneBuildDriver {
  inspectScene(): SceneInspection | null;
  command(request: string): string;
}

interface ComponentSummary {
  index: number;
  logical_size: number;
  output_size: number;
  source_offset: number;
  source_bytes_read: number;
  output_sha256: string;
}

interface DecodedSceneSummary {
  components: ComponentSummary[];
  descriptor25: { flags: number; model: number } | null;
  geometry_groups: number;
}

interface PackageFile {
  kind: string;
  disc: string;
  slot: number;
  source_sha256: string;
  result_sha256: string;
  result_size: number;
}

interface PackageManifest {
  id: string;
  discs: { serial: string; sha256: string }[];
  files: PackageFile[];
}

function sha256(bytes: Uint8Array): string {
  return createHash('sha256').update(bytes).digest('hex');
}

async function progress(root: string, name: string, value: Record<string, unknown>): Promise<void> {
  await writeFile(join(root, `build-progress-${name}.json`), `${JSON.stringify({ schema_version: 1, ...value }, null, 2)}\n`, { flag: 'wx' });
}

async function setPoseControls(page: Page, open: boolean): Promise<void> {
  const details = page.locator('.scene-pose-controls');
  const summary = details.locator('summary');
  await expect(summary).toHaveText('Runtime observation controls');
  const currentlyOpen = await details.evaluate(element => (element as HTMLDetailsElement).open);
  if (currentlyOpen !== open) await summary.click();
  if (open) await expect(details).toHaveAttribute('open', '');
  else await expect(details).not.toHaveAttribute('open', '');
}

async function refreshObservedPose(page: Page, live: Locator): Promise<void> {
  await setPoseControls(page, true);
  const refresh = page.getByRole('button', { name: 'Refresh observed actor pose', exact: true });
  await expect(refresh).toBeEnabled();
  await refresh.click();
  await expect(live.getByText(/browser · Paused · acknowledged/)).toBeVisible();
  await expect(page.getByText(/Observed VBlank \d+ · Actor \d+ · /)).toBeVisible({ timeout: 30_000 });
  await setPoseControls(page, false);
}

async function dismissExpectedStudioError(page: Page, pattern: RegExp, label: string): Promise<string> {
  const errors = page.getByRole('alert', { name: 'Studio errors', exact: true });
  await expect(errors, `${label} is retained in the global error log`).toHaveCount(1);
  const text = (await errors.innerText()).trim();
  assert.match(text, pattern, `${label} must match its known recovery boundary`);
  await errors.getByRole('button', { name: 'Dismiss errors', exact: true }).click();
  await expect(errors).toHaveCount(0);
  return text;
}

async function assertCenterIsUncovered(locator: Locator, label: string): Promise<void> {
  const hit = await locator.evaluate(element => {
    const rect = element.getBoundingClientRect();
    const target = document.elementFromPoint(rect.left + rect.width / 2, rect.top + rect.height / 2);
    return target === element || (target !== null && element.contains(target));
  });
  assert.equal(hit, true, `${label} center is unobscured and receives ordinary pointer input`);
}

async function assertFullyInside(locator: Locator, container: Locator, label: string): Promise<void> {
  const inner = await locator.boundingBox();
  const outer = await container.boundingBox();
  assert.ok(inner, `${label} has visible bounds`);
  assert.ok(outer, 'the authored fields region has visible bounds');
  assert.ok(inner.x >= outer.x && inner.y >= outer.y && inner.x + inner.width <= outer.x + outer.width &&
    inner.y + inner.height <= outer.y + outer.height, `${label} is fully inside the visible authored fields region`);
  await expect(locator).toBeInViewport();
}

async function decodeSceneComponents(page: Page, compiledBundle: boolean): Promise<DecodedSceneSummary> {
  return page.evaluate(useCompiled => {
    const driver = (window as unknown as { __xemStudioTest?: SceneBuildDriver }).__xemStudioTest;
    const inspection = driver?.inspectScene();
    if (!driver || !inspection) throw new Error('The explicit test-only Scene command hook is unavailable');
    const input: SceneInput = useCompiled
      ? { bundle: inspection.compiled.bundle, context: null, source: null, module_sha256: null }
      : inspection.document.input;
    const raw = driver.command(JSON.stringify({
      op: 'field.scene.decode',
      args: { input, entrances: inspection.document.entrance_selectors },
    }));
    const reply = JSON.parse(raw) as {
      ok?: {
        components: ComponentSummary[];
        header: { descriptors: { flags: number; model: number }[] };
        geometry: { groups: unknown[] };
      };
      error?: { kind?: string };
    };
    if (!reply.ok) throw new Error(`The production scene decoder rejected ${useCompiled ? 'the compiled Scene' : 'the original Scene'} (${reply.error?.kind ?? 'unknown error'})`);
    const descriptor = reply.ok.header.descriptors[25];
    return {
      components: reply.ok.components.map(component => ({
        index: component.index,
        logical_size: component.logical_size,
        output_size: component.output_size,
        source_offset: component.source_offset,
        source_bytes_read: component.source_bytes_read,
        output_sha256: component.output_sha256,
      })),
      descriptor25: descriptor ? { flags: descriptor.flags, model: descriptor.model } : null,
      geometry_groups: reply.ok.geometry.groups.length,
    };
  }, compiledBundle);
}

async function installOpfsChooser(page: Page, directoryName: string): Promise<void> {
  await page.evaluate(async name => {
    if (!navigator.storage?.getDirectory) throw new Error('Chromium OPFS is unavailable in this secure test origin');
    const root = await navigator.storage.getDirectory();
    const directory = await root.getDirectoryHandle(name, { create: true });
    Object.defineProperty(window, 'showDirectoryPicker', {
      configurable: true,
      value: async () => directory,
    });
  }, directoryName);
}

async function opfsText(page: Page, directoryName: string, fileName: string): Promise<string> {
  return page.evaluate(async ({ directoryName, fileName }) => {
    const root = await navigator.storage.getDirectory();
    const directory = await root.getDirectoryHandle(directoryName);
    const file = await directory.getFileHandle(fileName);
    return (await file.getFile()).text();
  }, { directoryName, fileName });
}

async function opfsNames(page: Page, directoryName: string): Promise<string[]> {
  return page.evaluate(async name => {
    const root = await navigator.storage.getDirectory();
    const directory = await root.getDirectoryHandle(name);
    const files: string[] = [];
    for await (const entry of directory.values()) files.push(entry.name);
    return files.sort();
  }, directoryName);
}

async function opfsPackageBase64(page: Page, directoryName: string, fileName: string): Promise<string> {
  return page.evaluate(async ({ directoryName, fileName }) => {
    const root = await navigator.storage.getDirectory();
    const directory = await root.getDirectoryHandle(directoryName);
    const file = await directory.getFileHandle(fileName);
    const bytes = new Uint8Array(await (await file.getFile()).arrayBuffer());
    let binary = '';
    for (let offset = 0; offset < bytes.length; offset += 0x8000) {
      binary += String.fromCharCode(...bytes.subarray(offset, Math.min(offset + 0x8000, bytes.length)));
    }
    return btoa(binary);
  }, { directoryName, fileName });
}

function inspectXemSet(bytes: Buffer): { member_sha256: string; identity: string; manifest: PackageManifest } {
  assert.equal(bytes.toString('ascii', 0, 8), 'XEMSET1\0', 'UI output is a versioned package set');
  assert.equal(bytes.readUInt32LE(8), 1, 'the UI output contains exactly one ordered package');
  const memberSize = bytes.readUInt32LE(12);
  assert.equal(bytes.length, 16 + memberSize, 'package-set framing has no trailing data');
  const member = bytes.subarray(16);
  assert.equal(member.toString('ascii', 0, 8), 'XEMMOD1\0', 'the package set contains a versioned package');
  const manifestSize = member.readUInt32LE(8);
  assert.ok(manifestSize > 0 && 12 + manifestSize <= member.length, 'package manifest lies within the member');
  const manifest = JSON.parse(member.toString('utf8', 12, 12 + manifestSize)) as PackageManifest;
  const memberSha256 = sha256(member);
  return { member_sha256: memberSha256, identity: `${manifest.id}@${memberSha256}`, manifest };
}

async function runtimeWord(gamePage: Page, path: string): Promise<number> {
  return gamePage.evaluate(value => {
    const runtime = (window as unknown as { xem: { commandJson(json: string): string } }).xem;
    const reply = JSON.parse(runtime.commandJson(JSON.stringify({ cmd: 'inspect', path: value, depth: 0 }))) as { error?: unknown } | number;
    if (typeof reply !== 'number') throw new Error(`Runtime inspection failed for ${value}`);
    return reply;
  }, path);
}

async function runtimeActorPosition(gamePage: Page, actor: number): Promise<[number, number, number]> {
  const base = `field_view.components.descriptors[${actor}].actor`;
  return [
    await runtimeWord(gamePage, `${base}->position[0]`),
    await runtimeWord(gamePage, `${base}->position[1]`),
    await runtimeWord(gamePage, `${base}->position[2]`),
  ];
}

async function enterFieldRouteAfterPlay(gamePage: Page): Promise<void> {
  await gamePage.evaluate(async () => {
    const runtime = (window as unknown as { xem: {
      commandJson(json: string): string;
      run(command: { cmd: 'frames'; count: number }): Promise<unknown>;
    } }).xem;
    const command = (value: Record<string, unknown>) => {
      const reply = JSON.parse(runtime.commandJson(JSON.stringify(value))) as { error?: unknown };
      if ('error' in reply) throw new Error(`Post-Play route command failed: ${String(value.cmd)}`);
    };
    command({ cmd: 'boot_mode', mode: 0 });
    const title = await runtime.run({ cmd: 'frames', count: 303 }) as { error?: unknown };
    if (title?.error) throw new Error('Post-Play title frames failed');
    command({ cmd: 'scenario', mode: 1, writes: [
      { path: 'game_data.map', value: 23 },
      { path: 'mode_kernel_menu_running', value: 0 },
      { path: 'mode_field_entered_once', value: 0 },
    ] });
    const field = await runtime.run({ cmd: 'frames', count: 430 }) as { error?: unknown };
    if (field?.error) throw new Error('Post-Play map 23 frames failed');
  });
}

async function advanceOpeningPrompt(gamePage: Page): Promise<void> {
  await gamePage.evaluate(async () => {
    const runtime = (window as unknown as { xem: {
      commandJson(json: string): string;
      run(command: { cmd: 'frames'; count: number }): Promise<unknown>;
    } }).xem;
    const input = JSON.parse(runtime.commandJson(JSON.stringify({ cmd: 'press', buttons: ['cross'], frames: 1 }))) as { error?: unknown };
    if ('error' in input) throw new Error('The public Cross input was rejected');
    const result = await runtime.run({ cmd: 'frames', count: 600 }) as { error?: unknown };
    if (result?.error) throw new Error('The bounded post-input field frames failed');
  });
}

async function runBuildAndPlay(context: RealSceneContinuation): Promise<void> {
  const { page, gamePage, live, artifactRoot, candidate } = context;
  const pageErrors: string[] = [];
  const runtimePageErrors: string[] = [];
  page.on('pageerror', error => pageErrors.push(error.message));
  gamePage.on('pageerror', error => runtimePageErrors.push(error.message));
  const original = await inspectInPage(page);
  const source = original.document.source;
  assert.ok(source, 'the original source identity survives Scene promotion');
  assert.equal(source.disc, 1);
  assert.equal(source.map, 23);
  assert.equal(source.slot, 652);
  assert.equal(original.document.context?.module_sha256, expectedModule);
  assert.deepEqual((await publicRuntimeStatus(gamePage)).identity.packages, []);

  const sourceComponents = await decodeSceneComponents(page, false);
  assert.equal(sourceComponents.components.length, 9, 'the real source decoder returns all nine original component outputs');
  assert.ok(sourceComponents.descriptor25, 'the source includes static descriptor 25 for a framed geometry view');
  assert.equal(sourceComponents.descriptor25.flags & 0x40, 0, 'descriptor 25 is a decoded static instance');
  assert.ok(sourceComponents.geometry_groups > sourceComponents.descriptor25.model, 'descriptor 25 selects a decoded source geometry group');
  await progress(artifactRoot, 'source-decoded', {
    source,
    module_sha256: expectedModule,
    actor_count: original.snapshot.scene?.actors.length ?? 0,
    descriptor25: sourceComponents.descriptor25,
    component_outputs: sourceComponents.components.map(({ index, output_size, output_sha256 }) => ({ index, output_size, output_sha256 })),
  });

  // Frame a real source model, with non-geometry overlays turned off, before the
  // actor marker and runtime gameplay screenshots are captured.
  for (const name of ['Collision', 'Trigger zones', 'Entrances']) {
    const layer = page.getByRole('checkbox', { name, exact: true });
    if (await layer.isChecked()) await layer.uncheck();
  }
  const filter = page.getByRole('textbox', { name: 'Filter scene entities' });
  await filter.fill('descriptor:25');
  const staticRow = page.locator('[data-scene-id="descriptor:25"]');
  await expect(staticRow).toBeVisible();
  await staticRow.click();
  await page.getByRole('button', { name: 'Frame selection', exact: true }).click();
  await expect(page.locator('.scene-canvas canvas')).toBeVisible();
  await page.evaluate(async () => { await document.fonts.ready; });
  const sceneShot = await page.screenshot({ path: join(artifactRoot, 'm7-scene-source-geometry.png'), fullPage: false });
  await progress(artifactRoot, 'source-geometry-framed', {
    source,
    descriptor: 25,
    descriptor_flags: sourceComponents.descriptor25.flags,
    model: sourceComponents.descriptor25.model,
    screenshot_sha256: sha256(sceneShot),
  });

  await filter.fill(candidate.actorId);
  const actorRow = page.locator(`[data-scene-id="${candidate.actorId}"]`);
  await expect(actorRow).toBeVisible();
  await actorRow.click();
  const placement = page.getByRole('combobox', { name: 'Authored placement source' });
  await placement.selectOption(candidate.placementNode);
  const x = page.getByRole('spinbutton', { name: 'Scene position X' });
  const z = page.getByRole('spinbutton', { name: 'Scene position Z' });
  const originalX = Number(await x.inputValue());
  const originalZ = Number(await z.inputValue());
  assert.equal(candidate.actorId, 'descriptor:3');
  assert.equal(candidate.runtimeActor, 3);
  assert.equal(candidate.placementNode, 'pc:1076');
  assert.equal(originalX, 29, 'the selected source instruction still has its measured vanilla X immediate');
  assert.equal(originalZ, 1270, 'the selected source instruction still has its measured vanilla Z immediate');
  const editedX = originalX + 32;
  assert.equal(editedX, 61);
  await x.fill(String(editedX));
  await expect(page.getByRole('button', { name: 'Apply scene fields', exact: true })).toBeEnabled();
  await page.getByRole('button', { name: 'Apply scene fields', exact: true }).click();
  const edited = await inspectInPage(page);
  assert.ok(edited.document.action_count > original.document.action_count, 'the real scene edit is a source-owned action');
  assert.equal(edited.document.cursor, edited.document.action_count);
  assert.notEqual(edited.compiled.event_bytes_sha256, original.compiled.event_bytes_sha256);
  assert.notEqual(edited.compiled.bundle_sha256, original.compiled.bundle_sha256);
  const editedComponents = await decodeSceneComponents(page, true);
  assert.equal(editedComponents.components.length, 9);
  for (const before of sourceComponents.components) {
    const after = editedComponents.components.find(component => component.index === before.index);
    assert.ok(after, `compiled original component ${before.index} is present`);
    if (before.index === 5) {
      assert.notEqual(after.output_sha256, before.output_sha256, 'the authored placement changes the event output');
    } else {
      assert.equal(after.output_size, before.output_size, `complete decoded component ${before.index} output size is unchanged`);
      assert.equal(after.output_sha256, before.output_sha256, `complete decoded component ${before.index} output hash is unchanged`);
    }
  }
  await expect(page.locator('.event-draft-toolbar')).toContainText('Unsaved');
  await progress(artifactRoot, 'placement-applied', {
    source,
    actor: candidate.actorId,
    runtime_actor: candidate.runtimeActor,
    placement_node: candidate.placementNode,
    original_position: [originalX, originalZ],
    edited_position: [editedX, originalZ],
    scene_revision: edited.snapshot.scene?.revision,
    action_count: edited.document.action_count,
    cursor: edited.document.cursor,
    compiled_bundle_sha256: edited.compiled.bundle_sha256,
    component_outputs: editedComponents.components.map(({ index, output_size, output_sha256 }) => ({ index, output_size, output_sha256 })),
  });

  await refreshObservedPose(page, live);
  const poseRegion = page.getByRole('region', { name: 'Runtime pose read-only' });
  const authoredX = page.getByRole('spinbutton', { name: 'Scene position X' });
  const authoredY = page.getByRole('spinbutton', { name: 'Scene position Y' });
  const authoredZ = page.getByRole('spinbutton', { name: 'Scene position Z' });
  await expect(page.getByRole('combobox', { name: 'Authored placement source' })).toHaveValue(candidate.placementNode);
  await expect(authoredX).toHaveValue(String(editedX));
  await expect(authoredX).toBeInViewport();
  await expect(authoredY).toBeInViewport();
  await expect(authoredZ).toBeInViewport();
  await expect(poseRegion).toContainText('Observed VBlank');
  const observedBeforePlay = await poseRegion.locator('.scene-pose-values dd').allTextContents();
  assert.deepEqual(observedBeforePlay.map(Number), [originalX, 0, originalZ], 'read-only vanilla pose remains separate from the authored source delta before Play');
  await expect(page.getByLabel('Current map')).toHaveText('Disc 1 · Map 23');
  await expect(page.locator('[data-map="23"][aria-pressed="true"]')).toBeVisible();
  await page.evaluate(async () => { await document.fonts.ready; });
  const authoredPoseShot = await page.screenshot({ path: join(artifactRoot, 'm7-scene-authored-and-vanilla-pose.png'), fullPage: false });
  await progress(artifactRoot, 'authored-vs-observed', {
    source,
    authored_position: [editedX, originalZ],
    observed_position: observedBeforePlay.map(Number),
    placement_node: candidate.placementNode,
    current_map: 'Disc 1 · Map 23',
    screenshot_sha256: sha256(authoredPoseShot),
  });

  const opfsName = `xem-m7-scene-${randomUUID()}`;
  await installOpfsChooser(page, opfsName);
  const projectFiles = page.locator('.studio-project-files');
  const projectFilesSummary = projectFiles.locator('summary');
  if (!(await projectFiles.evaluate(element => (element as HTMLDetailsElement).open))) await projectFilesSummary.click();
  await page.getByRole('button', { name: 'Open folder…', exact: true }).click();
  await expect(page.locator('[data-studio-panel="project"]')).toContainText(`${opfsName}/`);
  const save = page.getByRole('button', { name: 'Save project', exact: true });
  await expect(save).toBeEnabled();
  await save.click();
  await expect(page.locator('.event-draft-toolbar')).toContainText('Saved');
  const projectText = await opfsText(page, opfsName, 'field-map-23.json');
  const project = JSON.parse(projectText) as {
    schema_version: number;
    map: number;
    disc: number;
    document: {
      schema_version: number;
      input: SceneInput;
      actions: unknown[];
      cursor: number;
      next_id: number;
      next_event_id: number;
    };
    names: unknown;
  };
  assert.equal(project.schema_version, 2);
  assert.equal(project.map, 23);
  assert.equal(project.disc, 1);
  assert.equal(project.document.schema_version, 4);
  assert.equal(project.document.input.context?.components.length, 9);
  assert.equal(project.document.actions.length, edited.document.action_count);
  assert.equal(project.document.cursor, edited.document.cursor);
  const savedDocumentSha256 = sha256(Buffer.from(JSON.stringify(project.document)));
  assert.equal(savedDocumentSha256, edited.document.sha256, 'the project file stores the exact exported Scene document');
  const projectArtifact = join(artifactRoot, 'field-map-23.json');
  await writeFile(projectArtifact, projectText, { flag: 'wx' });
  const projectHash = sha256(Buffer.from(projectText));

  const beforeLoad = await inspectInPage(page);
  await page.getByRole('button', { name: 'Load project', exact: true }).click();
  await expect.poll(async () => (await inspectInPage(page)).owner).not.toBe(beforeLoad.owner);
  await expect(page.locator('.event-draft-toolbar')).toContainText('Saved');
  const reloaded = await inspectInPage(page);
  assert.notEqual(reloaded.owner, beforeLoad.owner, 'UI project load adopts a freshly imported Scene owner');
  assert.equal(reloaded.document.action_count, beforeLoad.document.action_count);
  assert.equal(reloaded.document.cursor, beforeLoad.document.cursor);
  assert.equal(reloaded.document.sha256, savedDocumentSha256, 'Scene import preserves the complete saved document and allocation history');
  assert.equal(reloaded.compiled.bundle_sha256, beforeLoad.compiled.bundle_sha256);
  assert.deepEqual(reloaded.compiled.event_bindings, beforeLoad.compiled.event_bindings);
  assert.deepEqual(reloaded.snapshot.scene?.actors.map(actor => actor.id), beforeLoad.snapshot.scene?.actors.map(actor => actor.id));
  await progress(artifactRoot, 'project-reloaded', {
    project_sha256: projectHash,
    project_schema: project.document.schema_version,
    source,
    scene_owner_before: beforeLoad.owner,
    scene_owner_after: reloaded.owner,
    action_count: reloaded.document.action_count,
    cursor: reloaded.document.cursor,
    document_sha256: reloaded.document.sha256,
    compiled_bundle_sha256: reloaded.compiled.bundle_sha256,
  });

  const build = page.getByRole('button', { name: 'Build package', exact: true });
  const play = page.getByRole('button', { name: 'Play', exact: true });
  await expect(build).toBeEnabled();
  await expect(play).toBeDisabled();
  await build.click();
  await expect(page.locator('.event-workspace')).toContainText(/Last successful build:/, { timeout: 180_000 });
  await expect(build).toBeEnabled({ timeout: 180_000 });
  const outputNames = await opfsNames(page, opfsName);
  const packageName = outputNames.find(name => /^field-map-23-[0-9a-f]{64}\.xemmods$/.test(name));
  assert.ok(packageName, 'the real BrowserFilesAdapter wrote the normal UI package build to OPFS');
  const setBytes = Buffer.from(await opfsPackageBase64(page, opfsName, packageName), 'base64');
  const parsed = inspectXemSet(setBytes);
  const packagePath = join(artifactRoot, packageName);
  await writeFile(packagePath, setBytes, { flag: 'wx' });
  assert.equal(parsed.manifest.files.length, 1);
  const file = parsed.manifest.files[0];
  assert.ok(file);
  assert.equal(file.kind, 'delta');
  assert.equal(file.disc, 'SLUS-00664', 'the sole package delta targets the selected original Disc 1');
  assert.equal(file.slot, source.slot);
  assert.equal(file.source_sha256, source.source_sha256);
  assert.equal(file.result_sha256, reloaded.compiled.bundle_sha256);
  assert.equal(file.result_size, reloaded.compiled.bundle_length);
  assert.deepEqual(parsed.manifest.discs.map(disc => disc.serial).sort(), ['SLUS-00664', 'SLUS-00669']);
  const packageHash = sha256(setBytes);
  const packageText = await page.locator('.event-workspace').innerText();
  assert.ok(packageText.includes(packageName), 'the package written to OPFS is identified in the normal Build result');
  await expect(play).toBeEnabled();
  await progress(artifactRoot, 'ui-package-built', {
    source,
    package_file: packageName,
    package_sha256: packageHash,
    member_sha256: parsed.member_sha256,
    package_identity: parsed.identity,
    manifest: {
      id: parsed.manifest.id,
      discs: parsed.manifest.discs,
      files: parsed.manifest.files.map(item => ({
        kind: item.kind,
        disc: item.disc,
        slot: item.slot,
        source_sha256: item.source_sha256,
        result_sha256: item.result_sha256,
        result_size: item.result_size,
      })),
    },
  });

  await expect(page.getByRole('tab', { name: 'Scene inspector', exact: true })).toBeVisible();
  const xField = page.getByRole('spinbutton', { name: 'Scene position X' });
  await xField.fill('32768');
  await expect(xField).toHaveAttribute('aria-invalid', 'true');
  const describedBy = await xField.getAttribute('aria-describedby');
  assert.ok(describedBy, 'the rejected coordinate points to its field-specific accessible explanation');
  const invalidReason = page.locator('[role="alert"]').filter({ hasText: /signed16|whole signed16/i }).first();
  await expect(invalidReason).toBeVisible();
  const describedText = await page.evaluate(id => document.getElementById(id)?.textContent ?? null, describedBy);
  assert.ok(describedText && /signed16|whole signed16/i.test(describedText));
  const validationFooter = page.locator('.scene-property-actions');
  await assertFullyInside(invalidReason, validationFooter, 'invalid-coordinate explanation');
  await expect(page.getByRole('button', { name: 'Apply scene fields', exact: true })).toBeDisabled();
  await expect(build).toBeDisabled();
  await expect(play).toBeDisabled();
  await expect(page.locator('.event-workspace')).toContainText(/Last successful build:/);
  const invalidReasonText = (await invalidReason.textContent())?.trim();
  assert.ok(invalidReasonText?.length, 'the invalid authored coordinate reports its signed16 source bound');
  assert.equal(sha256(Buffer.from(await opfsPackageBase64(page, opfsName, packageName), 'base64')), packageHash,
    'the last-good package file remains unchanged while invalid fields block Build and Play');
  const invalidSceneShot = await page.screenshot({ path: join(artifactRoot, 'm7-scene-invalid-coordinate-last-good-retained.png'), fullPage: false });
  await progress(artifactRoot, 'invalid-field-block', {
    invalid_x: 32768,
    invalid_reason: invalidReasonText,
    last_good_package_sha256: packageHash,
    build_disabled: await build.isDisabled(),
    play_disabled: await play.isDisabled(),
    screenshot_sha256: sha256(invalidSceneShot),
  });
  await page.getByRole('button', { name: 'Cancel scene fields', exact: true }).click();
  await expect(xField).toHaveValue(String(editedX));
  await expect(page.locator('.event-workspace')).toContainText(/Last successful build:/);
  await expect(play).toBeEnabled();

  const canvas = page.locator('.field-scene .scene-canvas canvas');
  const lost = await canvas.evaluate(element => {
    const canvas = element as HTMLCanvasElement;
    const gl = canvas.getContext('webgl2') ?? canvas.getContext('webgl');
    const extension = gl?.getExtension('WEBGL_lose_context');
    if (!extension) return false;
    extension.loseContext();
    return true;
  });
  assert.equal(lost, true, 'pinned headless Chromium exposes WEBGL_lose_context for renderer recovery proof');
  await expect(page.locator('.field-scene .scene-canvas__error')).toContainText('WebGL context was lost');
  const webglError = page.getByRole('alert', { name: 'Studio errors', exact: true });
  await expect(webglError).toHaveCount(1);
  const webglErrorText = (await webglError.innerText()).trim();
  assert.match(webglErrorText, /WebGL|context|renderer/i, 'the expected renderer failure is also retained by the normal Studio error log');
  await expect(page.locator('.event-workspace')).toContainText(/Last successful build:/);
  await expect(play).toBeEnabled();
  await page.getByRole('button', { name: 'Retry 3D viewport', exact: true }).click();
  await expect(canvas).toBeVisible();
  await expect(page.locator('.field-scene .scene-canvas__error')).toHaveCount(0);
  await expect(page.locator('.event-workspace')).toContainText(/Last successful build:/);
  await expect(play).toBeEnabled();
  await expect(webglError).toHaveCount(1);
  await webglError.getByRole('button', { name: 'Dismiss errors', exact: true }).click();
  await expect(webglError).toHaveCount(0);
  assert.equal(sha256(Buffer.from(await opfsPackageBase64(page, opfsName, packageName), 'base64')), packageHash,
    'renderer recovery does not modify the retained last-good package bytes');
  await progress(artifactRoot, 'last-good-retained', {
    invalid_x: 32768,
    invalid_reason: invalidReasonText,
    last_good_package_sha256: packageHash,
    build_blocked_for_invalid_fields: true,
    play_blocked_for_invalid_fields: true,
    last_good_retained_after_cancel: true,
    last_good_retained_after_webgl_recovery: true,
    recovered_webgl_error: webglErrorText,
  });

  await play.click();
  await expect(live.getByText(/browser · Running · acknowledged/)).toBeVisible({ timeout: 120_000 });
  const afterUiPlay = await publicRuntimeStatus(gamePage);
  assert.equal(afterUiPlay.identity.module, expectedModule);
  assert.deepEqual(afterUiPlay.identity.packages, [parsed.identity], 'UI Play stages the exact OPFS package saved for native replay');
  await progress(artifactRoot, 'ui-play-staged', {
    package_sha256: packageHash,
    package_identity: parsed.identity,
    runtime_identity: afterUiPlay.identity,
    route_before_freshboot: { mode: afterUiPlay.mode, disc: afterUiPlay.disc, vblanks: afterUiPlay.vblanks },
  });

  await live.getByRole('button', { name: 'Pause', exact: true }).click();
  await expect(live.getByText(/browser · Paused · acknowledged/)).toBeVisible();
  await gamePage.evaluate(async () => {
    const runtime = (window as unknown as { xem: { boot(options: { run: boolean; rewind: boolean }): Promise<unknown> } }).xem;
    await runtime.boot({ run: false, rewind: false });
  });
  await expect(live.getByText('Disconnected', { exact: true })).toBeVisible({ timeout: 30_000 });
  const globalErrors = page.getByRole('alert', { name: 'Studio errors', exact: true });
  await expect(globalErrors).toHaveCount(1);
  const generationError = await globalErrors.innerText();
  assert.match(generationError, /generation|session|disconnected/i, 'fresh boot invalidates the old Live session generation');
  await page.getByRole('button', { name: 'Open game & attach', exact: true }).click();
  await expect(live.getByText(/browser · Paused · acknowledged/)).toBeVisible({ timeout: 30_000 });
  const generationErrorText = await dismissExpectedStudioError(page, /generation|session|disconnected/i, 'the expected old-session diagnostic');
  const afterReconnect = await publicRuntimeStatus(gamePage);
  assert.deepEqual(afterReconnect.identity.packages, [parsed.identity], 'the same staged package survives the required fresh public boot');
  await enterFieldRouteAfterPlay(gamePage);
  let afterRoute = await publicRuntimeStatus(gamePage);
  assert.equal(afterRoute.mode, 1);
  assert.equal(afterRoute.disc, 1);
  assert.deepEqual(afterRoute.identity.packages, [parsed.identity]);
  const beforeCrossPosition = await runtimeActorPosition(gamePage, candidate.runtimeActor);
  assert.deepEqual(beforeCrossPosition, [editedX * 65536, 0, originalZ * 65536], 'the authored field placement is already active after the canonical field-entry frames and before user input');
  await progress(artifactRoot, 'authored-before-cross', {
    runtime_identity: afterRoute.identity,
    runtime_mode: afterRoute.mode,
    runtime_disc: afterRoute.disc,
    runtime_vblank: afterRoute.vblanks,
    actor: candidate.runtimeActor,
    raw_position_before_cross: beforeCrossPosition,
    canonical_route: { title_frames: 303, map: 23, field_frames: 430 },
  });
  await advanceOpeningPrompt(gamePage);
  afterRoute = await publicRuntimeStatus(gamePage);
  assert.equal(afterRoute.mode, 1);
  assert.equal(afterRoute.disc, 1);
  assert.deepEqual(afterRoute.identity.packages, [parsed.identity]);
  const [runtimeX, runtimeY, runtimeZ] = await runtimeActorPosition(gamePage, candidate.runtimeActor);
  assert.deepEqual([runtimeX, runtimeY, runtimeZ], beforeCrossPosition, 'normal Cross input leaves the already observed authored placement unchanged');
  assert.equal(runtimeY, 0, 'the source placement leaves Y unchanged');
  assert.equal(runtimeZ, originalZ * 65536, 'the source placement leaves Z unchanged');
  await expect(live.getByText(/browser · Paused · acknowledged/)).toBeVisible();

  const refresh = page.getByRole('button', { name: 'Refresh observed actor pose', exact: true });
  await setPoseControls(page, true);
  await expect(refresh).toBeEnabled();
  await refresh.click();
  await expect(poseRegion).toContainText(`Actor ${candidate.runtimeActor}`);
  const position = await poseRegion.locator('.scene-pose-values dd').allTextContents();
  assert.deepEqual(position.map(Number), [editedX, 0, originalZ]);
  const gameCanvas = gamePage.locator('canvas').first();
  await expect(gameCanvas).toBeVisible();
  await gamePage.evaluate(async () => { await document.fonts.ready; });
  const gameShot = await gameCanvas.screenshot({ path: join(artifactRoot, 'm7-game-after-cross-and-authored-placement.png') });
  const final = await inspectInPage(page);
  assert.equal(final.compiled.bundle_sha256, reloaded.compiled.bundle_sha256, 'runtime observation leaves the saved authored bundle unchanged');
  assert.equal(final.document.action_count, reloaded.document.action_count);
  assert.equal(final.document.cursor, reloaded.document.cursor);
  assert.equal(final.document.sha256, reloaded.document.sha256);
  assert.deepEqual(final.compiled.event_bindings, reloaded.compiled.event_bindings);
  await setPoseControls(page, false);
  await expect(page.getByRole('combobox', { name: 'Authored placement source' })).toHaveValue(candidate.placementNode);
  await expect(page.getByRole('spinbutton', { name: 'Scene position X' })).toBeInViewport();
  await expect(poseRegion).toContainText(`Actor ${candidate.runtimeActor}`);
  await expect(page.getByLabel('Current map')).toHaveText('Disc 1 · Map 23');
  await expect(page.locator('[data-map="23"][aria-pressed="true"]')).toBeVisible();
  const fieldsRegion = page.locator('.scene-inspector-fields');
  const placementSelector = page.getByRole('combobox', { name: 'Authored placement source' });
  await assertFullyInside(placementSelector, fieldsRegion, 'authored placement selector');
  await placementSelector.click();
  await placementSelector.press('Escape');
  await assertCenterIsUncovered(placementSelector, 'authored placement selector');
  for (const [axis, input] of [['X', page.getByRole('spinbutton', { name: 'Scene position X' })],
    ['Y', page.getByRole('spinbutton', { name: 'Scene position Y' })],
    ['Z', page.getByRole('spinbutton', { name: 'Scene position Z' })]] as const) {
    await assertFullyInside(input, fieldsRegion, `authored ${axis} input`);
    if (axis === 'Y') {
      await expect(input).toBeDisabled();
    } else {
      await input.click();
      await assertCenterIsUncovered(input, `authored ${axis} input`);
    }
  }
  const observedValues = await poseRegion.locator('.scene-pose-values dd').all();
  assert.equal(observedValues.length, 3);
  for (const item of observedValues) {
    await assertFullyInside(item, page.locator('.scene-runtime'), 'read-only observed pose value');
    await assertCenterIsUncovered(item, 'read-only observed pose value');
    assert.ok((await item.textContent())?.trim().length);
  }
  await expect(page.getByRole('alert', { name: 'Studio errors', exact: true })).toHaveCount(0);
  expect(pageErrors, `unexpected Scene Studio browser errors: ${pageErrors.join(' | ')}`).toEqual([]);
  expect(runtimePageErrors, `unexpected runtime browser errors: ${runtimePageErrors.join(' | ')}`).toEqual([]);
  await page.evaluate(async () => { await document.fonts.ready; });
  const finalSceneShot = await page.screenshot({ path: join(artifactRoot, 'm7-scene-authored-and-observed-placement.png'), fullPage: false });
  await progress(artifactRoot, 'gameplay-observed', {
    source,
    module_sha256: expectedModule,
    package_sha256: packageHash,
    package_identity: parsed.identity,
    recovered_session_error: generationErrorText,
    actor: candidate.actorId,
    runtime_actor: candidate.runtimeActor,
    placement_node: candidate.placementNode,
    expected_position: [editedX, 0, originalZ],
    runtime_position_raw: [runtimeX, runtimeY, runtimeZ],
    gameplay_route: { title_frames: 303, map: 23, field_frames: 430, cross_presses: 1, post_input_frames: 600 },
    runtime_identity: afterRoute.identity,
    runtime_mode: afterRoute.mode,
    runtime_disc: afterRoute.disc,
    runtime_vblank_after_route: afterRoute.vblanks,
    action_count: final.document.action_count,
    cursor: final.document.cursor,
    document_sha256: final.document.sha256,
    compiled_bundle_sha256: final.compiled.bundle_sha256,
    scene_screenshot_sha256: sha256(sceneShot),
    game_screenshot_sha256: sha256(gameShot),
    final_scene_screenshot_sha256: sha256(finalSceneShot),
    project_file_sha256: projectHash,
    package_file_sha256: packageHash,
    package_file: packageName,
  });
}

export async function runRealSceneBuildPlay(page: Page): Promise<void> {
  const original = await inspectInPage(page).catch(() => null);
  assert.equal(original, null, 'fresh browser page has no pre-existing Scene owner');
  await runRealSceneStageA(page, runBuildAndPlay);
}
