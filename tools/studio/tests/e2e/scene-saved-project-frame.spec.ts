import assert from 'node:assert/strict';
import { createHash, randomUUID } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { join, resolve, sep } from 'node:path';
import { expect, test, type Page } from '@playwright/test';
import {
  findEditablePlacement,
  inspectInPage,
  runRealSceneStageA,
} from './scene-real';

function requiredPath(name: string): string {
  const value = process.env[name];
  if (!value) throw new Error(`${name} is required for the gated M7 saved-project frame check`);
  return resolve(value);
}

function sha256(bytes: Uint8Array): string {
  return createHash('sha256').update(bytes).digest('hex');
}

const repoRoot = resolve(import.meta.dirname, '../../../..');
const expectedProjectSha256 = '3cffb561e775730c7152a83d524729e02bdbcaeff13a57007763752a37581f9a';

async function installSavedProject(page: Page, folderName: string, projectText: string): Promise<void> {
  await page.evaluate(async ({ folderName, projectText }) => {
    if (!navigator.storage?.getDirectory) throw new Error('Chromium OPFS is unavailable on this secure test origin');
    const root = await navigator.storage.getDirectory();
    const directory = await root.getDirectoryHandle(folderName, { create: true });
    const write = async (name: string, text: string) => {
      const handle = await directory.getFileHandle(name, { create: true });
      const writable = await handle.createWritable();
      await writable.write(text);
      await writable.close();
    };
    // This is the real browser OPFS handle and real file adapter. Only the
    // operating-system folder chooser is selected deterministically by test.
    await write('field-map-23.json', projectText);
    await write('names.json', JSON.stringify({ schema_version: 1, variables: {}, maps: {}, messages: {} }));
    Object.defineProperty(window, 'showDirectoryPicker', { configurable: true, value: async () => directory });
  }, { folderName, projectText });
}

async function readPackageFromOpfs(page: Page, folderName: string, packageName: string): Promise<number[]> {
  return page.evaluate(async ({ folderName, packageName }) => {
    const root = await navigator.storage.getDirectory();
    const directory = await root.getDirectoryHandle(folderName);
    const handle = await directory.getFileHandle(packageName);
    return Array.from(new Uint8Array(await (await handle.getFile()).arrayBuffer()));
  }, { folderName, packageName });
}

async function refreshPose(page: Page): Promise<void> {
  const details = page.locator('.scene-pose-controls');
  const summary = details.locator('summary');
  if (!(await details.evaluate(element => (element as HTMLDetailsElement).open))) await summary.click();
  const live = page.getByRole('region', { name: 'Live game' });
  await page.getByRole('button', { name: 'Refresh observed actor pose', exact: true }).click();
  await expect(live.getByText(/browser · Paused · acknowledged/)).toBeVisible();
  await expect(page.getByRole('region', { name: 'Runtime pose read-only' }).getByText(/Observed VBlank \d+ · Actor \d+/)).toBeVisible();
  await summary.click();
}

test.skip(process.env.XEM_STUDIO_M7_REAL_SCENE_FRAME !== '1', 'set XEM_STUDIO_M7_REAL_SCENE_FRAME=1 for the saved original-disc M7 project frame check');

test('saved M7 Scene project reloads, builds the same package, and frames authored source beside vanilla pose', async ({ page }) => {
  test.setTimeout(600_000);
  const projectPath = requiredPath('XEM_STUDIO_M7_SAVED_PROJECT');
  const packagePath = requiredPath('XEM_STUDIO_M7_SET');
  const artifactRoot = requiredPath('XEM_STUDIO_M7_REAL_SCENE_OUTPUT');
  const milestoneRoot = resolve(repoRoot, 'tmp/studio/milestone-7');
  assert.ok(artifactRoot.startsWith(milestoneRoot + sep), 'private Stage A artifacts stay under ignored M7 storage');
  assert.equal(projectPath, join(milestoneRoot, 'checkpoint-86/private-stage-b/field-map-23.json'));
  assert.equal(packagePath, join(milestoneRoot, 'checkpoint-86/private-stage-b/field-map-23-54458cc80057ab7af95dc59508f9f018d9c3d5531e1fa352cca154c2a1a2612d.xemmods'));
  const projectText = readFileSync(projectPath, 'utf8');
  assert.equal(sha256(Buffer.from(projectText)), expectedProjectSha256, 'the imported project is the exact Stage B UI save');
  const savedSet = readFileSync(packagePath);
  const savedSetHash = sha256(savedSet);
  const project = JSON.parse(projectText) as { schema_version: number; map: number; disc: number; document: { schema_version: number; input: { source?: { map?: number } }; [key: string]: unknown } };
  assert.equal(project.schema_version, 2);
  assert.equal(project.map, 23);
  assert.equal(project.disc, 1);
  assert.equal(project.document.schema_version, 4);
  assert.equal(project.document.input.source?.map, 23);
  const projectDocumentHash = sha256(Buffer.from(JSON.stringify(project.document)));
  assert.equal(savedSet.toString('ascii', 0, 8), 'XEMSET1\0');
  assert.equal(savedSet.readUInt32LE(8), 1);
  const member = savedSet.subarray(16, 16 + savedSet.readUInt32LE(12));
  assert.equal(member.toString('ascii', 0, 8), 'XEMMOD1\0');
  const manifestLength = member.readUInt32LE(8);
  const manifest = JSON.parse(member.toString('utf8', 12, 12 + manifestLength)) as { files: { slot: number; result_sha256: string }[] };
  assert.equal(manifest.files.length, 1);
  assert.equal(manifest.files[0]?.slot, 652);

  await runRealSceneStageA(page, async context => {
    const { page: studioPage, candidate } = context;
    const folderName = `m7-saved-scene-${randomUUID()}`;
    await installSavedProject(studioPage, folderName, projectText);
    const projectFiles = studioPage.locator('.studio-project-files');
    const summary = projectFiles.locator('summary');
    if (!(await projectFiles.evaluate(element => (element as HTMLDetailsElement).open))) await summary.click();
    await studioPage.getByRole('button', { name: 'Open folder…', exact: true }).click();
    await expect(studioPage.locator('[data-studio-panel="project"]')).toContainText(`${folderName}/`);
    // Stage A measures a fresh source context. Explicitly discard that unsaved
    // acquisition through the normal UI before replacing the owner with the
    // seeded project; never overwrite the checkpoint86 project to clear guard.
    await studioPage.getByRole('button', { name: 'Discard draft…', exact: true }).click();
    await studioPage.getByRole('button', { name: 'Confirm discard draft', exact: true }).click();
    const draftControls = studioPage.locator('.event-draft-toolbar');
    await expect(draftControls).toContainText('Unmodified source');
    const loadProject = studioPage.getByRole('button', { name: 'Load project', exact: true });
    await expect(loadProject).toBeEnabled();
    const beforeLoad = await inspectInPage(studioPage);
    await loadProject.click();
    await expect.poll(async () => (await inspectInPage(studioPage)).owner).not.toBe(beforeLoad.owner);
    await expect(studioPage.locator('.event-draft-toolbar')).toContainText('Saved');
    const loaded = await inspectInPage(studioPage);
    assert.notEqual(loaded.owner, beforeLoad.owner, 'the saved project adopts a new Rust owner');
    assert.equal(loaded.document.sha256, projectDocumentHash, 'normal Load imports the complete saved Scene document');
    assert.equal(loaded.document.source?.map, 23);
    assert.equal(loaded.compiled.bundle_sha256, manifest.files[0]!.result_sha256, 'loaded Scene compilation matches the package manifest result');
    assert.deepEqual(loaded.compiled.event_bindings, beforeLoad.compiled.event_bindings);

    const build = studioPage.getByRole('button', { name: 'Build package', exact: true });
    await expect(build).toBeEnabled();
    await build.click();
    await expect(studioPage.locator('.event-workspace')).toContainText(/Last successful build:/, { timeout: 180_000 });
    const outputs = await studioPage.evaluate(async name => {
      const root = await navigator.storage.getDirectory();
      const directory = await root.getDirectoryHandle(name);
      const files: string[] = [];
      for await (const item of directory.values()) files.push(item.name);
      return files;
    }, folderName);
    const outputName = outputs.find(name => /^field-map-23-[0-9a-f]{64}\.xemmods$/.test(name));
    assert.ok(outputName, 'normal UI Build writes a package set through the real OPFS adapter');
    const builtSet = Buffer.from(await readPackageFromOpfs(studioPage, folderName, outputName));
    assert.equal(sha256(builtSet), savedSetHash, 'reloading the exact saved project deterministically rebuilds the exact Stage B set');

    await studioPage.getByRole('tab', { name: 'Scene inspector', exact: true }).click();
    const currentScene = loaded.snapshot.scene;
    assert.ok(currentScene, 'saved project has an owned Scene snapshot');
    const framedCandidate = await findEditablePlacement(studioPage, currentScene);
    assert.equal(framedCandidate.actorId, candidate.actorId, 'the saved project retains the qualified original actor identity');
    assert.equal(framedCandidate.placementNode, candidate.placementNode);
    const actorRow = studioPage.locator(`[data-scene-id="${framedCandidate.actorId}"]`);
    await actorRow.click();
    await studioPage.getByRole('button', { name: 'Frame selection', exact: true }).click();
    const fields = studioPage.locator('.scene-inspector-fields');
    const pose = studioPage.getByRole('region', { name: 'Runtime pose read-only' });
    await refreshPose(studioPage);
    const x = studioPage.getByRole('spinbutton', { name: 'Scene position X' });
    const y = studioPage.getByRole('spinbutton', { name: 'Scene position Y' });
    const z = studioPage.getByRole('spinbutton', { name: 'Scene position Z' });
    await expect(x).toHaveValue('61');
    await expect(y).toHaveValue('');
    await expect(z).toHaveValue('1270');
    await expect(y).toBeDisabled();
    const binding = studioPage.getByRole('combobox', { name: 'Authored placement source' });
    await expect(binding).toHaveValue(framedCandidate.placementNode);
    for (const input of [binding, x, y, z]) {
      const bounds = await input.boundingBox();
      const fieldsBounds = await fields.boundingBox();
      assert.ok(bounds && fieldsBounds && bounds.y >= fieldsBounds.y && bounds.y + bounds.height <= fieldsBounds.y + fieldsBounds.height,
        'authored binding/position fields are fully visible in the scrollable inspector');
    }
    const observed = await pose.locator('.scene-pose-values dd').allTextContents();
    assert.deepEqual(observed.map(Number), [29, 0, 1270], 'live pose remains vanilla and distinct from the authored source values');
    const poseBounds = await pose.boundingBox();
    for (const value of await pose.locator('.scene-pose-values dd').all()) {
      const bounds = await value.boundingBox();
      assert.ok(bounds && poseBounds && bounds.y >= poseBounds.y && bounds.y + bounds.height <= poseBounds.y + poseBounds.height,
        'read-only observed coordinates are visible beside the authored fields');
    }
    await expect(studioPage.getByLabel('Current map')).toHaveText('Disc 1 · Map 23');
    await expect(studioPage.locator('[data-map="23"][aria-pressed="true"]')).toBeVisible();
    await studioPage.evaluate(async () => { await document.fonts.ready; });
    await studioPage.screenshot({ path: join(artifactRoot, 'm7-saved-scene-framed.png'), fullPage: false });
  });
});
