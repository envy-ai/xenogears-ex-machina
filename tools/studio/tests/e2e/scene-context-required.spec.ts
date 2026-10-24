import { createHash, randomUUID } from 'node:crypto';
import { realpath, stat, writeFile } from 'node:fs/promises';
import { resolve, sep } from 'node:path';
import { expect, test, type Page } from '@playwright/test';
import type { SceneActor } from '../../src/backend/scenes';
import type { SourceContext, SourceIdentity } from '../../src/backend/discs';
import { findEditablePlacement } from './scene-real';

interface SceneDriver {
  snapshot(): {
    view: string;
    map: number | null;
    scene: {
      revision: number;
      actors: SceneActor[];
      descriptors: { id: string; value: { flags: number; position: number[] } }[];
    } | null;
    discs: { id: number; disc: number | null | undefined; mapCount: number; placeholders: number }[];
  };
  inspectScene(): {
    owner: number | null;
    document: {
      schema_version: number;
      input: {
        source: { disc: number; map: number; slot: number; lba: number; indexed_size: number; source_sha256: string } | null;
        context?: { module_sha256: string } | null;
      };
      actions: unknown[];
      entrance_selectors: number[];
      cursor: number;
      next_id: number;
      next_event_id: number;
    };
    compiled: { bundle_sha256: string; bundle: number[]; source_sha256: string };
  } | null;
  command(json: string): string;
  setSourceContextHandler(handler: (source: SourceIdentity) => Promise<SourceContext>): void;
}

interface SavedProjectSummary {
  project_sha256: string;
  document_sha256: string;
  schema_version: number;
  map: number;
  disc: number;
  context_present: boolean;
  source_sha256: string;
  revision: number;
}

interface PackageSummary {
  set_sha256: string;
  member_sha256: string;
  file_count: number;
  file: { kind: string; disc: string; slot: number; source_sha256: string; result_sha256: string; result_size: number };
}

const enabled = process.env.XEM_STUDIO_CONTEXT_UX_REAL === '1';
const disc1Path = process.env.XEM_STUDIO_CONTEXT_UX_DISC1_PATH;
const artifactPath = process.env.XEM_STUDIO_CONTEXT_UX_OUTPUT_DIR;
const repositoryRoot = resolve(import.meta.dirname, '../../../..');

async function inspectScene(page: Page) {
  return page.evaluate(async () => {
    const driver = (window as unknown as { __xemStudioTest?: SceneDriver }).__xemStudioTest;
    if (!driver) throw new Error('The isolated Studio test driver is unavailable');
    const state = driver.snapshot();
    const inspection = driver.inspectScene();
    if (!inspection || !state.scene) throw new Error('The original Scene owner is not open');
    const documentHash = async () => {
      const bytes = new TextEncoder().encode(JSON.stringify(inspection.document));
      const digest = await crypto.subtle.digest('SHA-256', bytes.buffer as ArrayBuffer);
      return Array.from(new Uint8Array(digest), byte => byte.toString(16).padStart(2, '0')).join('');
    };
    return {
      view: state.view,
      map: state.map,
      owner: inspection.owner,
      revision: state.scene.revision,
      actors: state.scene.actors,
      descriptors: state.scene.descriptors.map(item => ({ id: item.id, flags: item.value.flags, position: item.value.position })),
      document_sha256: await documentHash(),
      schema_version: inspection.document.schema_version,
      actions: inspection.document.actions.length,
      cursor: inspection.document.cursor,
      next_id: inspection.document.next_id,
      next_event_id: inspection.document.next_event_id,
      context_present: Boolean(inspection.document.input.context),
      source: inspection.document.input.source,
      compiled_sha256: inspection.compiled.bundle_sha256,
      compiled_source_sha256: inspection.compiled.source_sha256,
    };
  });
}

async function installOpfsProjectFolder(page: Page, folder: string): Promise<void> {
  await page.evaluate(async name => {
    if (!navigator.storage?.getDirectory) throw new Error('Chromium OPFS is unavailable on this secure test origin');
    const root = await navigator.storage.getDirectory();
    const directory = await root.getDirectoryHandle(name, { create: true });
    const sidecar = await directory.getFileHandle('names.json', { create: true });
    const writable = await sidecar.createWritable();
    await writable.write(JSON.stringify({ schema_version: 1, variables: {}, maps: {}, messages: {} }));
    await writable.close();
    // Only the OS folder chooser selection is deterministic; the normal
    // BrowserFilesAdapter performs all subsequent OPFS reads and writes.
    Object.defineProperty(window, 'showDirectoryPicker', { configurable: true, value: async () => directory });
  }, folder);
}

async function savedProjectSummary(page: Page, folder: string): Promise<SavedProjectSummary> {
  return page.evaluate(async name => {
    const root = await navigator.storage.getDirectory();
    const directory = await root.getDirectoryHandle(name);
    const file = await directory.getFileHandle('field-map-23.json');
    const text = await (await file.getFile()).text();
    const project = JSON.parse(text) as {
      schema_version: number; map: number; disc: number;
      document: {
        schema_version: number; input: { context?: unknown | null; source?: { source_sha256?: string } | null };
        actions: unknown[]; cursor: number; [key: string]: unknown;
      };
    };
    const digest = async (value: string) => {
      const bytes = new TextEncoder().encode(value);
      const result = await crypto.subtle.digest('SHA-256', bytes.buffer as ArrayBuffer);
      return Array.from(new Uint8Array(result), byte => byte.toString(16).padStart(2, '0')).join('');
    };
    return {
      project_sha256: await digest(text),
      document_sha256: await digest(JSON.stringify(project.document)),
      schema_version: project.document.schema_version,
      map: project.map,
      disc: project.disc,
      context_present: Boolean(project.document.input.context),
      source_sha256: project.document.input.source?.source_sha256 ?? '',
      revision: project.document.cursor,
    };
  }, folder);
}

async function projectPackageSummary(page: Page, folder: string, filename: string): Promise<PackageSummary> {
  return page.evaluate(async ({ folder, filename }) => {
    const root = await navigator.storage.getDirectory();
    const directory = await root.getDirectoryHandle(folder);
    const file = await directory.getFileHandle(filename);
    const bytes = new Uint8Array(await (await file.getFile()).arrayBuffer());
    const digest = async (value: Uint8Array) => {
      // Subarrays can share a larger package buffer. Hash exactly this view,
      // not the entire backing ArrayBuffer.
      const exactBytes = value.slice();
      const result = await crypto.subtle.digest('SHA-256', exactBytes.buffer as ArrayBuffer);
      return Array.from(new Uint8Array(result), byte => byte.toString(16).padStart(2, '0')).join('');
    };
    const ascii = (start: number, size: number) => new TextDecoder().decode(bytes.subarray(start, start + size));
    const word = (at: number) => new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength).getUint32(at, true);
    if (ascii(0, 8) !== 'XEMSET1\0' || word(8) !== 1) throw new Error('Build did not produce a single-member XEMSET1 package');
    const memberSize = word(12);
    if (bytes.length !== 16 + memberSize || ascii(16, 8) !== 'XEMMOD1\0') throw new Error('Built XEMSET1 framing is invalid');
    const manifestSize = word(24);
    const manifest = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes.subarray(28, 28 + manifestSize))) as {
      files: { kind: string; disc: string; slot: number; source_sha256: string; result_sha256: string; result_size: number }[];
    };
    if (!Array.isArray(manifest.files) || manifest.files.length !== 1) throw new Error('Expected one Map 23 package delta');
    const member = bytes.subarray(16);
    return {
      set_sha256: await digest(bytes),
      member_sha256: await digest(member),
      file_count: manifest.files.length,
      file: manifest.files[0]!,
    };
  }, { folder, filename });
}

test('hashes the exact package member view instead of its whole backing buffer', async ({ page }) => {
  const folder = `synthetic-package-${randomUUID()}`;
  const filename = 'fixture.xemmods';
  await page.goto('/');
  const fixture = await page.evaluate(async ({ folder, filename }) => {
    const manifest = new TextEncoder().encode(JSON.stringify({ files: [{
      kind: 'delta', disc: 'SYNTHETIC', slot: 1,
      source_sha256: 'a'.repeat(64), result_sha256: 'b'.repeat(64), result_size: 3,
    }] }));
    const member = new Uint8Array(12 + manifest.length);
    member.set(new TextEncoder().encode('XEMMOD1\0'), 0);
    new DataView(member.buffer).setUint32(8, manifest.length, true);
    member.set(manifest, 12);
    const packageBytes = new Uint8Array(16 + member.length);
    packageBytes.set(new TextEncoder().encode('XEMSET1\0'), 0);
    const view = new DataView(packageBytes.buffer);
    view.setUint32(8, 1, true);
    view.setUint32(12, member.length, true);
    packageBytes.set(member, 16);

    const root = await navigator.storage.getDirectory();
    const directory = await root.getDirectoryHandle(folder, { create: true });
    const file = await directory.getFileHandle(filename, { create: true });
    const writable = await file.createWritable();
    await writable.write(packageBytes);
    await writable.close();
    return { setBytes: Array.from(packageBytes), memberBytes: Array.from(member) };
  }, { folder, filename });

  const summary = await projectPackageSummary(page, folder, filename);
  const setSha = createHash('sha256').update(Buffer.from(fixture.setBytes)).digest('hex');
  const memberSha = createHash('sha256').update(Buffer.from(fixture.memberBytes)).digest('hex');
  expect(summary.set_sha256).toBe(setSha);
  expect(summary.member_sha256).toBe(memberSha);
  expect(summary.member_sha256).not.toBe(summary.set_sha256);
  expect(summary.file_count).toBe(1);
});

test('opens and builds qualified original Map 23 offline with a self-contained Scene package', async ({ page }) => {
  test.skip(!enabled, 'set XEM_STUDIO_CONTEXT_UX_REAL=1 with the explicit local Disc 1 BIN for offline Map 23 acceptance');
  test.setTimeout(10 * 60 * 1000);
  if (!disc1Path) throw new Error('XEM_STUDIO_CONTEXT_UX_DISC1_PATH must name the existing local Disc 1 BIN image');
  if (!artifactPath) throw new Error('XEM_STUDIO_CONTEXT_UX_OUTPUT_DIR must name a fresh ignored artifact directory');
  const resolvedArtifactPath = await realpath(artifactPath);
  const artifactPrefix = resolve(repositoryRoot, 'tmp/studio/context-ux') + sep;
  if (!resolvedArtifactPath.startsWith(artifactPrefix)) throw new Error('Offline Scene outputs must remain in the fresh ignored checkpoint artifact directory');
  const discInfo = await stat(disc1Path);
  expect(discInfo.isFile()).toBe(true);
  expect(discInfo.size).toBeGreaterThan(0);

  await page.goto('/');
  await expect(page.getByRole('main', { name: 'XEM Studio' })).toBeVisible();
  await expect.poll(() => page.evaluate(() => typeof (window as unknown as { __xemStudioTest?: SceneDriver }).__xemStudioTest?.inspectScene)).toBe('function');
  await page.evaluate(() => {
    const driver = (window as unknown as { __xemStudioTest?: SceneDriver }).__xemStudioTest;
    if (!driver) throw new Error('The isolated Studio test driver is unavailable');
    (window as unknown as { __sceneSourceContextCalls: number }).__sceneSourceContextCalls = 0;
    driver.setSourceContextHandler(async () => {
      const state = window as unknown as { __sceneSourceContextCalls: number };
      state.__sceneSourceContextCalls += 1;
      throw new Error('Offline Map 23 must not request a source context');
    });
  });
  const live = page.getByRole('region', { name: 'Live game' });
  await expect(live.getByRole('status')).toHaveText('Disconnected');

  await page.getByLabel('Open disc files').setInputFiles(disc1Path);
  await expect.poll(async () => page.evaluate(() => {
    const driver = (window as unknown as { __xemStudioTest?: SceneDriver }).__xemStudioTest;
    return driver?.snapshot().discs.some(item => item.disc === 1 && item.mapCount > 0) ?? false;
  }), { timeout: 6 * 60 * 1000, intervals: [1000, 5000, 15000] }).toBe(true);
  const discButton = page.getByRole('button', { name: /^Disc 1 · \d+ maps$/ });
  await expect(discButton).toBeVisible({ timeout: 120_000 });
  await discButton.click();
  await expect(page.locator('[data-map="23"]')).toBeVisible({ timeout: 120_000 });
  const map23 = page.locator('[data-map="23"]');
  await map23.click();
  await expect.poll(() => page.evaluate(() => (window as unknown as { __xemStudioTest: SceneDriver }).__xemStudioTest.snapshot().map), {
    timeout: 120_000,
  }).toBe(23);
  await expect(page.getByRole('tabpanel', { name: 'Command list' })).toBeVisible();
  const listOwner = await page.evaluate(() => {
    const driver = (window as unknown as { __xemStudioTest?: SceneDriver }).__xemStudioTest;
    return driver?.inspectScene()?.owner ?? null;
  });
  expect(listOwner).toBeNull();

  await page.getByRole('tab', { name: 'Scene', exact: true }).click();
  await expect(page.getByRole('tabpanel', { name: 'Scene' })).toBeVisible();
  await expect.poll(async () => (await inspectScene(page)).owner).not.toBeNull();
  await expect(live.getByRole('status')).toHaveText('Disconnected');
  expect(page.context().pages()).toHaveLength(1);
  const opened = await inspectScene(page);
  expect(opened.map).toBe(23);
  expect(opened.schema_version).toBe(4);
  expect(opened.context_present).toBe(false);
  expect(opened.source).toMatchObject({ disc: 1, map: 23, slot: 652, lba: 124528, indexed_size: 110052, source_sha256: '0f0a131acdf052cae070a64912fc09eb00e0b5e6a81f9d8cb4719d6416213add' });
  expect(opened.compiled_source_sha256).toBe(opened.source?.source_sha256);
  expect(opened.compiled_sha256).toBe(opened.source?.source_sha256);
  expect(opened.actions).toBe(0);
  expect(await page.evaluate(() => (window as unknown as { __sceneSourceContextCalls: number }).__sceneSourceContextCalls)).toBe(0);
  const candidate = await findEditablePlacement(page, { actors: opened.actors });
  expect(candidate.actorId).toBe('descriptor:3');
  expect(candidate.placementNode).toBe('pc:1076');
  const authoredAxes = new Map(candidate.axes.map(axis => [axis.axis, axis]));
  expect(authoredAxes.get('X')).toMatchObject({ enabled: true, value: 29 });
  expect(authoredAxes.get('Y')?.enabled).toBe(false);
  expect(authoredAxes.get('Z')).toMatchObject({ enabled: true, value: 1270 });
  const authoredY = page.getByRole('spinbutton', { name: 'Scene position Y', exact: true });
  await expect(authoredY).toBeDisabled();
  await expect(authoredY).toHaveValue('');
  const targetId = candidate.actorId;
  const originalX = authoredAxes.get('X')!.value!;
  const editedX = 61;
  const sourceHeaderX = opened.descriptors.find(item => item.id === targetId)?.position[0];
  expect(Number.isInteger(sourceHeaderX)).toBe(true);

  const opfsFolder = `m7-offline-map23-${randomUUID()}`;
  await installOpfsProjectFolder(page, opfsFolder);
  const projectFiles = page.locator('.studio-project-files');
  const projectSummary = projectFiles.locator('summary');
  if (!(await projectFiles.evaluate(element => (element as HTMLDetailsElement).open))) await projectSummary.click();
  await page.getByRole('button', { name: 'Open folder…', exact: true }).click();
  await expect(page.locator('[data-studio-panel="project"]')).toContainText(`${opfsFolder}/`);
  const save = page.getByRole('button', { name: 'Save project', exact: true });
  await expect(save).toBeEnabled();
  await save.click();
  await expect(page.locator('.event-draft-toolbar')).toContainText('Saved');
  const originalProject = await savedProjectSummary(page, opfsFolder);
  expect(originalProject).toMatchObject({ schema_version: 4, map: 23, disc: 1, context_present: false, source_sha256: opened.source?.source_sha256, revision: 0 });

  const filter = page.getByRole('textbox', { name: 'Filter scene entities' });
  await filter.fill(targetId);
  const row = page.locator(`[data-scene-id="${targetId}"]`);
  await expect(row).toBeVisible();
  await row.click();
  const x = page.getByRole('spinbutton', { name: 'Scene position X', exact: true });
  await expect(x).toHaveValue(String(originalX));
  await x.fill(String(editedX));
  const apply = page.getByRole('button', { name: 'Apply scene fields', exact: true });
  await expect(apply).toBeEnabled();
  await apply.click();
  await expect(page.locator('.event-draft-toolbar')).toContainText('Unsaved');
  const edited = await inspectScene(page);
  expect(edited.context_present).toBe(false);
  expect(edited.actions).toBe(1);
  expect(edited.cursor).toBe(1);
  expect(edited.compiled_sha256).not.toBe(opened.compiled_sha256);
  expect(edited.descriptors.find(item => item.id === targetId)?.position[0]).toBe(sourceHeaderX);
  await expect(live.getByRole('status')).toHaveText('Disconnected');
  const popupCountBeforeBuild = page.context().pages().length;

  await save.click();
  await expect(page.locator('.event-draft-toolbar')).toContainText('Saved');
  const saved = await savedProjectSummary(page, opfsFolder);
  expect(saved).toMatchObject({ schema_version: 4, map: 23, disc: 1, context_present: false, source_sha256: opened.source?.source_sha256, revision: 1 });
  expect(saved.document_sha256).toBe(await inspectScene(page).then(value => value.document_sha256));
  const beforeLoad = await inspectScene(page);
  await page.getByRole('button', { name: 'Load project', exact: true }).click();
  await expect.poll(async () => (await inspectScene(page)).owner).not.toBe(beforeLoad.owner);
  const loaded = await inspectScene(page);
  expect(loaded.document_sha256).toBe(saved.document_sha256);
  expect(loaded.compiled_sha256).toBe(edited.compiled_sha256);
  expect(loaded.context_present).toBe(false);
  await expect(live.getByRole('status')).toHaveText('Disconnected');

  const build = page.getByRole('button', { name: 'Build package', exact: true });
  await expect(build).toBeEnabled();
  await build.click();
  await expect(page.locator('.event-workspace')).toContainText(/Last successful build:/, { timeout: 180_000 });
  const outputName = await page.evaluate(async folder => {
    const root = await navigator.storage.getDirectory();
    const directory = await root.getDirectoryHandle(folder);
    const names: string[] = [];
    for await (const entry of directory.values()) names.push(entry.name);
    return names.find(name => /^field-map-23-[0-9a-f]{64}\.xemmods$/.test(name)) ?? null;
  }, opfsFolder);
  expect(outputName).not.toBeNull();
  const pkg = await projectPackageSummary(page, opfsFolder, outputName!);
  expect(pkg.file_count).toBe(1);
  expect(pkg.file).toMatchObject({ kind: 'delta', disc: 'SLUS-00664', slot: 652, source_sha256: opened.source?.source_sha256, result_sha256: loaded.compiled_sha256 });
  expect(pkg.file.result_size).toBeGreaterThan(0);
  expect(page.context().pages()).toHaveLength(popupCountBeforeBuild);
  await expect(live.getByRole('status')).toHaveText('Disconnected');
  const final = await inspectScene(page);
  expect(final.document_sha256).toBe(saved.document_sha256);
  expect(final.context_present).toBe(false);
  expect(final.descriptors.find(item => item.id === targetId)?.position[0]).toBe(sourceHeaderX);
  expect(await page.evaluate(() => (window as unknown as { __sceneSourceContextCalls: number }).__sceneSourceContextCalls)).toBe(0);
  const decodedCandidate = await page.evaluate(() => {
    const driver = (window as unknown as { __xemStudioTest?: SceneDriver }).__xemStudioTest;
    const inspection = driver?.inspectScene();
    if (!driver || !inspection) throw new Error('The offline Scene owner is unavailable for strict candidate decode');
    const raw = driver.command(JSON.stringify({ op: 'field.scene.decode', args: {
      input: { bundle: inspection.compiled.bundle, context: null, source: null, module_sha256: null },
      entrances: inspection.document.entrance_selectors,
    } }));
    const response = JSON.parse(raw) as { ok?: { components: { index: number; logical_size: number; output_size: number; output_sha256: string; optional_tail_bytes?: number }[] }; error?: { kind?: string } };
    if (!response.ok) throw new Error(`The self-contained offline candidate failed strict decode (${response.error?.kind ?? 'unknown error'})`);
    return response.ok.components;
  });
  expect(decodedCandidate).toHaveLength(9);
  expect(decodedCandidate.map(component => component.index)).toEqual([0, 1, 2, 3, 4, 5, 6, 7, 8]);
  expect(decodedCandidate[8]).toMatchObject({ logical_size: 72, output_size: 72 });
  expect(decodedCandidate[8]?.optional_tail_bytes).toBeUndefined();

  const artifactBytes = await page.evaluate(async ({ folder, filename }) => {
    const root = await navigator.storage.getDirectory();
    const directory = await root.getDirectoryHandle(folder);
    const projectFile = await directory.getFileHandle('field-map-23.json');
    const projectText = await (await projectFile.getFile()).text();
    const packageFile = await directory.getFileHandle(filename);
    const packageBytes = new Uint8Array(await (await packageFile.getFile()).arrayBuffer());
    return { projectText, packageBytes: Array.from(packageBytes) };
  }, { folder: opfsFolder, filename: outputName! });
  const projectArtifact = resolve(resolvedArtifactPath, 'offline-map23-scene.json');
  const packageArtifact = resolve(resolvedArtifactPath, 'offline-map23-edited.xemmods');
  const packageBuffer = Buffer.from(artifactBytes.packageBytes);
  await writeFile(projectArtifact, artifactBytes.projectText, { flag: 'wx' });
  await writeFile(packageArtifact, packageBuffer, { flag: 'wx' });
  const packageSha256 = createHash('sha256').update(packageBuffer).digest('hex');
  expect(packageSha256).toBe(pkg.set_sha256);
  expect(packageSha256.startsWith('d242')).toBe(false);
  const summaryArtifact = {
    schema_version: 1,
    scene_document_sha256: saved.document_sha256,
    saved_project_sha256: createHash('sha256').update(artifactBytes.projectText).digest('hex'),
    package_sha256: packageSha256,
    package_member_sha256: pkg.member_sha256,
    package_size: packageBuffer.byteLength,
    original_source_sha256: opened.source?.source_sha256,
    original_compiled_bundle_sha256: opened.compiled_sha256,
    edited_compiled_bundle_sha256: loaded.compiled_sha256,
    source_context_handler_calls: await page.evaluate(() => (window as unknown as { __sceneSourceContextCalls: number }).__sceneSourceContextCalls),
  };
  await writeFile(resolve(resolvedArtifactPath, 'offline-map23-summary.json'), `${JSON.stringify(summaryArtifact, null, 2)}\n`, { flag: 'wx' });
});
