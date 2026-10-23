import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join, resolve, sep } from 'node:path';
import { expect, test, type Page } from '@playwright/test';

const milestoneRoot = resolve('../../tmp/studio/milestone-6');
const enabled = process.env.XEM_STUDIO_M6_BUILD_PLAY === '1';

interface BuildControl {
  files(): string[];
  base64(name: string): string | null;
}

declare global {
  interface Window { __fieldBuildControl?: BuildControl }
}

test.skip(!enabled, 'Real M6 Studio build/play requires XEM_STUDIO_M6_BUILD_PLAY=1 and explicit private paths.');

test('builds the loaded real-disc field draft, retains the last build while fields are unapplied, and plays that set', async ({ page }) => {
  test.setTimeout(1_200_000);
  const fixtureRoot = requiredPath('XEM_STUDIO_M6_FIXTURE_ROOT');
  const discPath = requiredPath('XEM_STUDIO_M6_DISC1');
  const extractRoot = requiredPath('XEM_STUDIO_EXTRACT_ROOT');
  const outputRoot = requiredPath('XEM_STUDIO_M6_UI_ARTIFACT_ROOT');
  expect([fixtureRoot, outputRoot].every(path => path === milestoneRoot || path.startsWith(milestoneRoot + sep))).toBe(true);
  expect(discPath).toBe(resolve(join(extractRoot, 'discs/disc1.bin')));
  expect(outputRoot).not.toBe(fixtureRoot);
  expect(readFileSync(join(fixtureRoot, 'evidence.json'), 'utf8')).toContain('non_event_components_equal');
  const draft = JSON.parse(readFileSync(join(fixtureRoot, 'candidate-draft.json'), 'utf8')) as unknown;
  const evidence = JSON.parse(readFileSync(join(fixtureRoot, 'evidence.json'), 'utf8')) as {
    message_pc: number;
    disc1: { serial: string };
  };
  expect(evidence.message_pc).toBeGreaterThan(0);
  expect(resolve(discPath)).toBe(resolve(join(extractRoot, 'discs/disc1.bin')));
  expect(outputRoot.startsWith(milestoneRoot + sep)).toBe(true);
  expect(existsSync(join(outputRoot, '..')), 'the ignored M6 output parent exists').toBe(true);
  expect(existsSync(outputRoot), 'preserve older M6 evidence and choose a fresh output leaf').toBe(false);
  mkdirSync(outputRoot, { recursive: false });

  await installProjectFolder(page, draft);
  await page.goto('/');
  await expect(page.getByRole('main', { name: 'XEM Studio' })).toBeVisible();
  await expect(page.locator('[data-panel-id="backend"]')).toContainText('xem-studio 0.5.0');

  await page.getByLabel('Open disc files').setInputFiles(discPath);
  await expect.poll(async () => page.evaluate(() => window.__xemStudioTest?.snapshot().discs.some(item => item.disc === 1) ?? false), {
    timeout: 120_000,
    intervals: [250, 1000, 2500],
  }).toBe(true);
  const discButton = page.getByRole('button', { name: /^Disc 1 · \d+ maps$/ });
  await discButton.click();
  await page.locator('[data-map="23"]').waitFor({ state: 'visible', timeout: 120_000 });
  await page.locator('[data-map="23"]').click();
  await expect.poll(async () => page.evaluate(() => window.__xemStudioTest?.snapshot().map ?? null), {
    timeout: 120_000,
    intervals: [250, 1000, 2500],
  }).toBe(23);
  await page.getByRole('button', { name: 'Open folder…' }).click();
  await expect(page.locator('[data-studio-panel="project"]')).toContainText('m6-authored-project/');
  await page.getByRole('button', { name: 'Load project' }).click();
  await expect.poll(async () => page.evaluate(() =>
    Object.keys(window.__xemStudioTest?.snapshot().draft?.bindings ?? {}).filter(id => id.startsWith('new:')).length), {
    timeout: 30_000,
  }).toBeGreaterThan(0);

  const popupPromise = page.waitForEvent('popup');
  await page.getByRole('button', { name: 'Open game & attach' }).click();
  const gamePage = await popupPromise;
  await gamePage.waitForLoadState('domcontentloaded');
  await gamePage.waitForFunction(() => document.body.dataset.ready === 'true', undefined, { timeout: 120_000 });
  await gamePage.locator('#file').setInputFiles(discPath);
  await gamePage.waitForFunction(() => document.querySelector('#message')?.textContent?.startsWith('Xenogears disc 1 (') ?? false, undefined, { timeout: 120_000 });
  await gamePage.evaluate(async () => {
    const runtime = (window as unknown as { xem: {
      boot(options: { run: boolean; rewind: boolean }): Promise<unknown>;
      commandJson(json: string): string;
      run(command: { cmd: 'frames'; count: number }): Promise<unknown>;
    } }).xem;
    await runtime.boot({ run: false, rewind: false });
    JSON.parse(runtime.commandJson(JSON.stringify({ cmd: 'boot_mode', mode: 0 })));
    await runtime.run({ cmd: 'frames', count: 303 });
  });
  // The initial click created the popup; reconnect through the real UI after the original disc boot.
  await page.getByRole('button', { name: 'Open game & attach' }).click();
  const live = page.getByRole('region', { name: 'Live game' });
  await expect(live.getByText(/browser · Paused · acknowledged/)).toBeVisible({ timeout: 30_000 });

  const build = page.getByRole('button', { name: 'Build package' });
  const play = page.getByRole('button', { name: 'Play', exact: true });
  await expect(build).toBeEnabled();
  await expect(play).toBeDisabled();
  await build.click();
  await expect(page.locator('.event-view-surface')).toContainText(/./);
  await expect.poll(async () => page.evaluate(() => window.__fieldBuildControl?.files().some(name => name.endsWith('.xemmods')) ?? false), {
    timeout: 120_000,
    intervals: [250, 1000],
  }).toBe(true);
  const outputName = await page.evaluate(() => window.__fieldBuildControl?.files().find(name => name.endsWith('.xemmods')) ?? null);
  expect(outputName).toMatch(/^field-map-23-[0-9a-f]{64}\.xemmods$/);
  const packageBase64 = await page.evaluate(name => window.__fieldBuildControl?.base64(name) ?? null, outputName!);
  expect(packageBase64).not.toBeNull();
  const packageSet = Buffer.from(packageBase64!, 'base64');
  expect(packageSet.toString('ascii', 0, 8)).toBe('XEMSET1\0');
  const memberLength = packageSet.readUInt32LE(12);
  expect(packageSet.length).toBe(16 + memberLength);
  const member = packageSet.subarray(16);
  const manifestLength = member.readUInt32LE(8);
  const manifest = JSON.parse(member.toString('utf8', 12, 12 + manifestLength)) as { id: string; files: Array<{ kind: string }> };
  expect(manifest.files.length).toBe(1);
  expect(manifest.files[0]?.kind).toBe('delta');
  const packagePath = join(outputRoot, 'studio-built-map23.xemset');
  writeFileSync(packagePath, packageSet, { flag: 'wx' });

  await expect(page.locator('.event-workspace')).toContainText('Last successful build:');
  const relocatedMessagePc = await page.evaluate(sourcePc => {
    const bindings = window.__xemStudioTest?.snapshot().draft?.bindings ?? {};
    return bindings[`pc:${sourcePc}`] ?? null;
  }, evidence.message_pc);
  expect(relocatedMessagePc).not.toBeNull();
  const find = page.getByLabel('Find within map');
  await find.fill(`@${relocatedMessagePc!.toString(16).padStart(4, '0')}`);
  await find.press('Enter');
  const editor = page.getByRole('region', { name: 'Authored instruction editor' });
  const firstOperand = editor.getByRole('textbox', { name: 'Operand 1 · msg' });
  await expect(firstOperand).toBeVisible();
  await firstOperand.fill('4294967296');
  await expect(build).toBeDisabled();
  await expect(play).toBeDisabled();
  await expect(page.locator('.event-workspace')).toContainText('Last successful build:');
  await expect(page.locator('.event-workspace')).toContainText('retained while fields are unapplied');
  await editor.getByRole('button', { name: 'Cancel' }).click();
  await expect(build).toBeEnabled();
  await expect(play).toBeEnabled();

  await play.click();
  await expect(live.getByText(/browser · Running · acknowledged/)).toBeVisible({ timeout: 120_000 });
  const packageIdentity = await gamePage.evaluate(() => {
    const runtime = (window as unknown as { xem: { commandJson(json: string): string } }).xem;
    const schema = JSON.parse(runtime.commandJson(JSON.stringify({ cmd: 'schema' })));
    return schema.identity.packages as string[];
  });
  const memberIdentity = `${manifest.id}@${createHash('sha256').update(member).digest('hex')}`;
  expect(packageIdentity).toEqual([memberIdentity]);
  await expect(page.locator('.event-workspace')).toContainText('Last successful build:');
  await page.screenshot({ path: join(outputRoot, 'studio-build-play.png') });
  await gamePage.screenshot({ path: join(outputRoot, 'runtime-package-active.png') });
  writeFileSync(join(outputRoot, 'result.json'), `${JSON.stringify({
    acceptance: 'actual Studio load, build, dirty-field guard and same-origin browser Play',
    input: 'gated original-disc service fixture; candidate document loaded through project adapter',
    output: 'the exact XEMSET1 bytes written by Studio Build package',
    packageSetSha256: createHash('sha256').update(packageSet).digest('hex'),
    packageIdentity: memberIdentity,
    invalidInlineFieldsBlockBuildAndPlay: true,
    cancelRestoresLastGoodPlay: true,
    browserRuntimePackageIdentity: packageIdentity,
  }, null, 2)}\n`, { flag: 'wx' });
  await gamePage.close();
});

async function installProjectFolder(page: Page, document: unknown): Promise<void> {
  const initialProject = JSON.stringify({
    schema_version: 1,
    map: 23,
    disc: 1,
    document,
    names: { schema_version: 1, variables: {}, maps: {}, messages: {} },
  });
  await page.addInitScript(({ initialProject }) => {
    const storage = new Map<string, number[]>([
      ['names.json', [...new TextEncoder().encode(JSON.stringify({ schema_version: 1, variables: {}, maps: {}, messages: {} }))]],
      ['field-map-23.json', [...new TextEncoder().encode(initialProject)]],
    ]);
    const handle = (name: string) => ({
      name,
      kind: 'file' as const,
      async getFile() {
        const bytes = new Uint8Array(storage.get(name) ?? []);
        return new File([bytes.buffer], name);
      },
      async createWritable() {
        let pending = new Uint8Array();
        return {
          async write(data: Uint8Array) { pending = new Uint8Array(data); },
          async close() { storage.set(name, [...pending]); },
          async abort() {},
        };
      },
    });
    const directory = {
      name: 'm6-authored-project',
      kind: 'directory' as const,
      async *values() { for (const name of storage.keys()) yield handle(name); },
      async getDirectoryHandle() { throw new Error('M6 project fixture has no subdirectories'); },
      async getFileHandle(name: string) {
        if (name !== 'names.json' && !/^field-map-\d+(?:-[0-9a-f]{64})?\.xemmods$/.test(name) && !/^field-map-\d+\.json$/.test(name)) {
          throw new Error('Unexpected project adapter file name');
        }
        if (!storage.has(name)) storage.set(name, []);
        return handle(name);
      },
    };
    (window as typeof window & { showDirectoryPicker: () => Promise<typeof directory> }).showDirectoryPicker = async () => directory;
    (window as typeof window & { __fieldBuildControl: BuildControl }).__fieldBuildControl = {
      files() { return [...storage.keys()]; },
      base64(name) {
        const bytes = storage.get(name);
        if (!bytes) return null;
        let result = '';
        for (let at = 0; at < bytes.length; at += 0x8000) result += String.fromCharCode(...bytes.slice(at, at + 0x8000));
        return btoa(result);
      },
    };
  }, { initialProject });
}

function requiredPath(name: string): string {
  const value = process.env[name];
  if (!value) throw new Error(`${name} must explicitly name a private M6 path`);
  return resolve(value);
}

