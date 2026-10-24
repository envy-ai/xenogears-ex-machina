import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { once } from 'node:events';
import { closeSync, createReadStream, existsSync, mkdirSync, openSync, readFileSync, readSync, statSync, writeFileSync } from 'node:fs';
import { basename, join, relative, resolve, sep } from 'node:path';
import { expect, test, type Page } from '@playwright/test';
import { nativeGameTransport } from '../../src/game/native';
import type { GameTransport } from '../../src/game/protocol';

type Mode = 'absent' | 'disabled' | 'enabled';
type DiscRoute = 'map23' | 'map22' | 'map0' | 'map145' | 'disc2-title';
interface PackageManifest {
  id: string;
  discs: { serial: string; sha256: string }[];
  files: { disc: string; slot: number; source_sha256?: string; result_sha256: string; result_size: number }[];
}
interface RouteResult {
  host: 'browser' | 'native'; mode: Mode; route: DiscRoute; identity: { module: string; disc: string; packages: string[] };
  active_disc: number | null; field_map: number | null; index650_sha256: string | null; index652_sha256: string | null;
  route_slot_sha256: string | null;
  scene_actor_early: number[] | null;
  actor3_before_input: number[] | null; actor3_after_cross_600: number[] | null;
  scene_actor_descriptor: number | null; scene_actor_before_input: number[] | null; scene_actor_after_cross_600: number[] | null;
  memory_hash_before_input: string; memory_hash_after_input: string;
  status_before_input: StatusSummary; status_after_input: StatusSummary;
  m7_map23_screenshot_sha256: string | null; m7_map23_screenshot_file: string | null;
  m7_scene_screenshot_sha256: string | null; m7_scene_screenshot_file: string | null;
}
interface StatusSummary { mode: number | null; vblanks: string; frames: string; stopped: string | null; fast_forward: boolean; field_map: number | null }
interface RuntimeApi {
  boot(options: { run: boolean; rewind: boolean }): Promise<unknown>;
  commandJson(json: string): string;
  run(command: { cmd: 'frames'; count: number }): Promise<unknown>;
  packages: { stage(bytes: Uint8Array, options: { enabled: boolean }): { identities: string[] } };
}

const INDEX_RAM_BASE = 0x80010004;
const MODULE_SHA256 = '613c3a747e1c07e0e0e2632710eae1f873e4b14dda880cd6ce1c556b31c152a8';
const EXPECTED_SET_SHA256 = 'd242f77413949e6dfa32db17f3815fc4adf35c7f0ede5f1d2e5d96b08e229489';
const EXPECTED_PACKAGE_IDENTITY = 'studio-map-23@54458cc80057ab7af95dc59508f9f018d9c3d5531e1fa352cca154c2a1a2612d';
const RAW_SECTOR_BYTES = 2352;
const USER_DATA_OFFSET = 24;
const FORM1_BYTES = 2048;
const INDEX_LBA = 24;
const repoRoot = resolve(import.meta.dirname, '../../../..');

function required(name: string): string {
  const value = process.env[name];
  if (!value) throw new Error(`${name} is required for the gated M7 Scene package route matrix`);
  return resolve(value);
}

function sha256(bytes: Uint8Array): string {
  return createHash('sha256').update(bytes).digest('hex');
}

async function fileSha256(path: string): Promise<string> {
  const digest = createHash('sha256');
  await new Promise<void>((resolvePromise, reject) => {
    const stream = createReadStream(path);
    stream.on('data', chunk => digest.update(chunk));
    stream.on('error', reject);
    stream.on('end', resolvePromise);
  });
  return digest.digest('hex');
}

function originalIndexRecord(discPath: string, slot: number): Buffer {
  const out = Buffer.alloc(7);
  const fd = openSync(discPath, 'r');
  try {
    let logical = slot * 7;
    let copied = 0;
    while (copied < out.length) {
      const inSector = logical % FORM1_BYTES;
      const count = Math.min(out.length - copied, FORM1_BYTES - inSector);
      const lba = INDEX_LBA + Math.floor(logical / FORM1_BYTES);
      const offset = lba * RAW_SECTOR_BYTES + USER_DATA_OFFSET + inSector;
      assert.equal(readSync(fd, out, copied, count, offset), count, 'read a complete logical index-record fragment from MODE2/Form1 user data');
      logical += count;
      copied += count;
    }
  } finally { closeSync(fd); }
  return out;
}

function hex(bytes: Uint8Array): string { return Buffer.from(bytes).toString('hex'); }

function noError<T>(value: T & { error?: unknown }, label: string): T {
  assert.equal(value.error, undefined, `${label} succeeds`);
  return value;
}

async function browserCommand(page: Page, request: Record<string, unknown>): Promise<unknown> {
  const value = await page.evaluate(input => {
    const runtime = (window as unknown as { xem: RuntimeApi }).xem;
    return JSON.parse(runtime.commandJson(JSON.stringify(input))) as unknown;
  }, request);
  if (value && typeof value === 'object' && 'error' in value) throw new Error(`Browser runtime command failed: ${String((value as { error: unknown }).error)}`);
  return value;
}

async function browserReadBytes(page: Page, address: number, length: number): Promise<Buffer> {
  const value = await browserCommand(page, { cmd: 'read_memory', address, length });
  assert.equal(typeof value, 'string', 'browser read_memory returns the protocol hexadecimal byte string');
  return Buffer.from(value as string, 'hex');
}

async function browserReadPosition(page: Page, descriptor = 3): Promise<number[]> {
  const base = `field_view.components.descriptors[${descriptor}].actor->position`;
  const words: number[] = [];
  for (const axis of [0, 1, 2]) {
    const value = await browserCommand(page, { cmd: 'inspect', path: `${base}[${axis}]`, depth: 0 });
    assert.equal(typeof value, 'number');
    words.push(value as number);
  }
  return words.map(value => value / 65536);
}

async function browserSnapshot(page: Page): Promise<{ memoryHash: string; status: StatusSummary }> {
  const status = await browserCommand(page, { cmd: 'status' }) as Record<string, unknown>;
  const memoryHash = await browserCommand(page, { cmd: 'memory_hash' });
  const map = await browserCommand(page, { cmd: 'inspect', path: 'mode_field_map_id', depth: 0 });
  assert.equal(typeof memoryHash, 'string', 'browser memory_hash is exact hexadecimal text');
  assert.equal(typeof status.vblanks, 'number', 'browser status reports the deterministic VBlank counter');
  assert.equal(typeof status.frames, 'number', 'browser status reports the deterministic frame counter');
  return { memoryHash: memoryHash as string, status: {
    mode: typeof status.mode === 'number' ? status.mode : null,
    vblanks: String(status.vblanks), frames: String(status.frames),
    stopped: typeof status.stopped === 'string' ? status.stopped : null,
    fast_forward: status.fast_forward === true,
    field_map: Number.isFinite(Number(map)) ? Number(map) : null,
  } };
}

async function browserRoute(
  testPage: Page, mode: Mode, route: DiscRoute, discPath: string, packageBytes: Buffer,
  artifactRoot: string, routeIndex: number,
): Promise<RouteResult> {
  const sceneDescriptor = route === 'map0' ? 18 : route === 'map145' ? 11 : route === 'map23' ? 3 : null;
  const routeSlot = route === 'map0' ? 606 : route === 'map145' ? 896 : null;
  const routeRoot = join(artifactRoot, `browser-${routeIndex}-${mode}-${route}`);
  assert.equal(existsSync(routeRoot), false, 'browser route output is fresh');
  mkdirSync(routeRoot);
  const gamePage = await testPage.context().newPage();
  try {
    await gamePage.goto(new URL('/runtime/', testPage.url()).href);
    await gamePage.waitForFunction(() => document.body.dataset.ready === 'true', undefined, { timeout: 120_000 });
    if (mode !== 'absent') {
      const staged = await gamePage.evaluate(({ base64, enabled }) => {
        const bytes = Uint8Array.from(atob(base64), character => character.charCodeAt(0));
        return (window as unknown as { xem: RuntimeApi }).xem.packages.stage(bytes, { enabled });
      }, { base64: packageBytes.toString('base64'), enabled: mode === 'enabled' });
      assert.deepEqual(staged.identities, mode === 'enabled' ? [PACKAGE_IDENTITY] : [], `browser ${mode} staging identity`);
    }
    if (route !== 'disc2-title') {
      await gamePage.locator('#file').setInputFiles(discPath);
      await expect(gamePage.locator('#message')).toHaveText(/^Xenogears disc 1 \(/, { timeout: 120_000 });
      await expect.poll(() => gamePage.evaluate(() => (window as unknown as { xem: { status(): { disc?: { state?: string } } } }).xem.status().disc?.state),
        { timeout: 120_000, intervals: [500, 1500] }).toBe('identified');
      await gamePage.evaluate(() => (window as unknown as { xem: RuntimeApi }).xem.boot({ run: false, rewind: false }));
      noError(await browserCommand(gamePage, { cmd: 'boot_mode', mode: 0 }) as { error?: unknown }, 'browser title boot');
      noError(await gamePage.evaluate(() => (window as unknown as { xem: RuntimeApi }).xem.run({ cmd: 'frames', count: 303 })) as { error?: unknown }, 'browser title route');
      const map = route === 'map23' ? 23 : route === 'map145' ? 145 : route === 'map0' ? 0 : 22;
      noError(await browserCommand(gamePage, { cmd: 'scenario', mode: 1, writes: [
        { path: 'game_data.map', value: map },
        { path: 'mode_kernel_menu_running', value: 0 },
        { path: 'mode_field_entered_once', value: 0 },
      ] }) as { error?: unknown }, `browser map ${map} scenario`);
      const fieldFrames = route === 'map145' ? 300 : 430;
      noError(await gamePage.evaluate((count) => (window as unknown as { xem: RuntimeApi }).xem.run({ cmd: 'frames', count }), fieldFrames) as { error?: unknown }, `browser map ${map} field route`);
    } else {
      await gamePage.locator('#file').setInputFiles(discPath);
      await expect(gamePage.locator('#message')).toHaveText(/^Xenogears disc 2 \(/, { timeout: 120_000 });
      await expect.poll(() => gamePage.evaluate(() => (window as unknown as { xem: { status(): { disc?: { state?: string } } } }).xem.status().disc?.state),
        { timeout: 120_000, intervals: [500, 1500] }).toBe('identified');
      await gamePage.evaluate(() => (window as unknown as { xem: RuntimeApi }).xem.boot({ run: false, rewind: false }));
      noError(await browserCommand(gamePage, { cmd: 'boot_mode', mode: 0 }) as { error?: unknown }, 'browser disc 2 title boot');
      noError(await gamePage.evaluate(() => (window as unknown as { xem: RuntimeApi }).xem.run({ cmd: 'frames', count: 303 })) as { error?: unknown }, 'browser disc 2 title frames');
    }
    const schema = await browserCommand(gamePage, { cmd: 'schema' }) as { identity: RouteResult['identity'] };
    const status = await browserCommand(gamePage, { cmd: 'status' }) as { identity?: RouteResult['identity']; disc?: number };
    const identity = status.identity ?? schema.identity;
    assert.equal(identity.module, MODULE_SHA256);
    assert.equal(identity.disc, 'SLUS-00664+SLUS-00669');
    assert.deepEqual(identity.packages, mode === 'enabled' ? [PACKAGE_IDENTITY] : []);
    assert.equal(status.disc, route === 'disc2-title' ? 2 : 1);
    const selected = route === 'disc2-title' ? null : Number(await browserCommand(gamePage, { cmd: 'inspect', path: 'mode_field_map_id', depth: 0 }));
    if (route === 'map23') assert.equal(selected, 23);
    if (route === 'map22') assert.equal(selected, 22);
    if (route === 'map0') assert.equal(selected, 0);
    if (route === 'map145') assert.equal(selected, 145);
    const index650_sha256 = route === 'disc2-title' ? null : sha256(await browserReadBytes(gamePage, INDEX_RAM_BASE + 650 * 7, 7));
    const index652_sha256 = route === 'disc2-title' ? null : sha256(await browserReadBytes(gamePage, INDEX_RAM_BASE + 652 * 7, 7));
    const route_slot_sha256 = routeSlot === null ? null : sha256(await browserReadBytes(gamePage, INDEX_RAM_BASE + routeSlot * 7, 7));
    let early: number[] | null = null;
    let before: number[] | null = null;
    if (route === 'map145' && sceneDescriptor !== null) {
      early = await browserReadPosition(gamePage, sceneDescriptor);
    }
    if (route === 'map145') {
      noError(await gamePage.evaluate(() => (window as unknown as { xem: RuntimeApi }).xem.run({ cmd: 'frames', count: 130 })) as { error?: unknown }, 'browser remaining Map 145 field route frames');
    }
    if (sceneDescriptor !== null) before = await browserReadPosition(gamePage, sceneDescriptor);
    const beforeSnapshot = await browserSnapshot(gamePage);
    noError(await browserCommand(gamePage, { cmd: 'press', buttons: ['cross'], frames: 1 }) as { error?: unknown }, 'browser public Cross input');
    noError(await gamePage.evaluate(() => (window as unknown as { xem: RuntimeApi }).xem.run({ cmd: 'frames', count: 600 })) as { error?: unknown }, 'browser post-input frames');
    const afterSnapshot = await browserSnapshot(gamePage);
    const after = sceneDescriptor !== null ? await browserReadPosition(gamePage, sceneDescriptor) : null;
    let screenshotHash: string | null = null;
    let screenshotFile: string | null = null;
    let sceneScreenshotHash: string | null = null;
    let sceneScreenshotFile: string | null = null;
    if (sceneDescriptor !== null && mode !== 'disabled') {
      const screenshotPath = join(routeRoot, `${route}-after-cross.png`);
      assert.equal(existsSync(screenshotPath), false, 'new M7 browser gameplay screenshot does not overwrite an older image');
      const screenshot = await gamePage.locator('canvas').screenshot({ path: screenshotPath });
      screenshotHash = sha256(screenshot);
      screenshotFile = relative(artifactRoot, screenshotPath);
      sceneScreenshotHash = screenshotHash;
      sceneScreenshotFile = screenshotFile;
    }
    const result: RouteResult = { host: 'browser', mode, route, identity, active_disc: status.disc ?? null, field_map: selected, index650_sha256, index652_sha256, route_slot_sha256,
      actor3_before_input: route === 'map23' ? before : null, actor3_after_cross_600: route === 'map23' ? after : null,
      scene_actor_descriptor: sceneDescriptor, scene_actor_early: early, scene_actor_before_input: before, scene_actor_after_cross_600: after,
      memory_hash_before_input: beforeSnapshot.memoryHash, memory_hash_after_input: afterSnapshot.memoryHash,
      status_before_input: beforeSnapshot.status, status_after_input: afterSnapshot.status,
      m7_map23_screenshot_sha256: route === 'map23' ? screenshotHash : null, m7_map23_screenshot_file: route === 'map23' ? screenshotFile : null,
      m7_scene_screenshot_sha256: sceneScreenshotHash, m7_scene_screenshot_file: sceneScreenshotFile };
    writeFileSync(join(routeRoot, 'route-result.json'), `${JSON.stringify(result, null, 2)}\n`, { flag: 'wx' });
    return result;
  } finally {
    await gamePage.close();
  }
}

function parseReply(text: string, label: string): unknown {
  const value = JSON.parse(text) as unknown;
  if (value && typeof value === 'object' && 'error' in value) throw new Error(`${label}: ${String((value as { error: unknown }).error)}`);
  return value;
}

async function nativeReadBytes(transport: GameTransport, address: number, length: number): Promise<Buffer> {
  const value = parseReply(await transport.request(JSON.stringify({ cmd: 'read_memory', address, length })), 'native read_memory');
  assert.equal(typeof value, 'string');
  return Buffer.from(value as string, 'hex');
}

async function nativeReadPosition(transport: GameTransport, descriptor = 3): Promise<number[]> {
  const base = `field_view.components.descriptors[${descriptor}].actor->position`;
  const words: number[] = [];
  for (const axis of [0, 1, 2]) {
    const value = parseReply(await transport.request(JSON.stringify({ cmd: 'inspect', path: `${base}[${axis}]`, depth: 0 })), 'native actor position');
    assert.equal(typeof value, 'number');
    words.push(value as number);
  }
  return words.map(value => value / 65536);
}

async function nativeSnapshot(transport: GameTransport): Promise<{ memoryHash: string; status: StatusSummary }> {
  const status = parseReply(await transport.request(JSON.stringify({ cmd: 'status' })), 'native runtime status') as Record<string, unknown>;
  const memoryHash = parseReply(await transport.request(JSON.stringify({ cmd: 'memory_hash' })), 'native memory hash');
  const map = parseReply(await transport.request(JSON.stringify({ cmd: 'inspect', path: 'mode_field_map_id', depth: 0 })), 'native selected map');
  assert.equal(typeof memoryHash, 'string', 'native memory_hash is exact hexadecimal text');
  assert.equal(typeof status.vblanks, 'number', 'native status reports the deterministic VBlank counter');
  assert.equal(typeof status.frames, 'number', 'native status reports the deterministic frame counter');
  return { memoryHash: memoryHash as string, status: {
    mode: typeof status.mode === 'number' ? status.mode : null,
    vblanks: String(status.vblanks), frames: String(status.frames),
    stopped: typeof status.stopped === 'string' ? status.stopped : null,
    fast_forward: status.fast_forward === true,
    field_map: Number.isFinite(Number(map)) ? Number(map) : null,
  } };
}

async function nativeRoute(
  mode: Mode, route: DiscRoute, discPath: string, packagePath: string,
  headless: string, artifactRoot: string, routeIndex: number,
): Promise<RouteResult> {
  const sceneDescriptor = route === 'map0' ? 18 : route === 'map145' ? 11 : route === 'map23' ? 3 : null;
  const routeSlot = route === 'map0' ? 606 : route === 'map145' ? 896 : null;
  const routeRoot = join(artifactRoot, `native-${routeIndex}-${mode}-${route}`);
  assert.equal(existsSync(routeRoot), false, 'native route output is fresh');
  mkdirSync(routeRoot);
  const cards = join(routeRoot, 'cards');
  const data = join(routeRoot, 'data');
  mkdirSync(cards);
  mkdirSync(data);
  const args = ['--disc', discPath, '--cards', cards, '--control'];
  if (mode !== 'absent') args.push('--packages', packagePath);
  if (mode === 'disabled') args.push('--packages-disabled');
  const child = spawn(headless, args, { cwd: repoRoot, env: { ...process.env, XEM_DATA_DIR: data }, stdio: ['pipe', 'pipe', 'pipe'] });
  let stderr = '';
  child.stderr.setEncoding('utf8');
  child.stderr.on('data', text => { stderr = (stderr + text).slice(-32_768); });
  const closed = once(child, 'close');
  const transport = nativeGameTransport(child);
  const request = async (value: Record<string, unknown>, label: string) => parseReply(await transport.request(JSON.stringify(value)), label);
  let result: RouteResult | null = null;
  let operationError: unknown;
  let cleanupError: unknown;
  try {
    const schema = await request({ cmd: 'schema' }, 'native schema') as { identity: RouteResult['identity'] };
    if (route !== 'disc2-title') {
      parseReply(await transport.request(JSON.stringify({ cmd: 'boot_mode', mode: 0 })), 'native title boot');
      parseReply(await transport.request(JSON.stringify({ cmd: 'frames', count: 303 })), 'native title frames');
      const map = route === 'map23' ? 23 : route === 'map145' ? 145 : route === 'map0' ? 0 : 22;
      parseReply(await transport.request(JSON.stringify({ cmd: 'scenario', mode: 1, writes: [
        { path: 'game_data.map', value: map },
        { path: 'mode_kernel_menu_running', value: 0 },
        { path: 'mode_field_entered_once', value: 0 },
      ] })), `native map ${map} scenario`);
      const fieldFrames = route === 'map145' ? 300 : 430;
      parseReply(await transport.request(JSON.stringify({ cmd: 'frames', count: fieldFrames })), `native map ${map} field route`);
    } else {
      parseReply(await transport.request(JSON.stringify({ cmd: 'boot_mode', mode: 0 })), 'native disc 2 title boot');
      parseReply(await transport.request(JSON.stringify({ cmd: 'frames', count: 303 })), 'native disc 2 title frames');
    }
    const status = await request({ cmd: 'status' }, 'native status') as { identity?: RouteResult['identity']; disc?: number };
    const identity = status.identity ?? schema.identity;
    assert.equal(identity.module, MODULE_SHA256);
    assert.equal(identity.disc, 'SLUS-00664+SLUS-00669');
    assert.deepEqual(identity.packages, mode === 'enabled' ? [PACKAGE_IDENTITY] : []);
    assert.equal(status.disc, route === 'disc2-title' ? 2 : 1);
    const selected = route === 'disc2-title' ? null : Number(await request({ cmd: 'inspect', path: 'mode_field_map_id', depth: 0 }, 'native selected map'));
    if (route === 'map23') assert.equal(selected, 23);
    if (route === 'map22') assert.equal(selected, 22);
    if (route === 'map0') assert.equal(selected, 0);
    if (route === 'map145') assert.equal(selected, 145);
    const index650_sha256 = route === 'disc2-title' ? null : sha256(await nativeReadBytes(transport, INDEX_RAM_BASE + 650 * 7, 7));
    const index652_sha256 = route === 'disc2-title' ? null : sha256(await nativeReadBytes(transport, INDEX_RAM_BASE + 652 * 7, 7));
    const route_slot_sha256 = routeSlot === null ? null : sha256(await nativeReadBytes(transport, INDEX_RAM_BASE + routeSlot * 7, 7));
    let early: number[] | null = null;
    let before: number[] | null = null;
    if (route === 'map145' && sceneDescriptor !== null) {
      early = await nativeReadPosition(transport, sceneDescriptor);
    }
    if (route === 'map145') {
      parseReply(await transport.request(JSON.stringify({ cmd: 'frames', count: 130 })), 'native remaining Map 145 field route frames');
    }
    if (sceneDescriptor !== null) before = await nativeReadPosition(transport, sceneDescriptor);
    const beforeSnapshot = await nativeSnapshot(transport);
    parseReply(await transport.request(JSON.stringify({ cmd: 'press', buttons: ['cross'], frames: 1 })), 'native Cross input');
    parseReply(await transport.request(JSON.stringify({ cmd: 'frames', count: 600 })), 'native post-input frames');
    const afterSnapshot = await nativeSnapshot(transport);
    const after = sceneDescriptor !== null ? await nativeReadPosition(transport, sceneDescriptor) : null;
    let screenshotHash: string | null = null;
    let screenshotFile: string | null = null;
    let sceneScreenshotHash: string | null = null;
    let sceneScreenshotFile: string | null = null;
    if (sceneDescriptor !== null && mode !== 'disabled') {
      const screenshotPath = join(routeRoot, `${route}-after-cross.png`);
      assert.equal(existsSync(screenshotPath), false, 'new M7 native gameplay screenshot does not overwrite an older image');
      parseReply(await transport.request(JSON.stringify({ cmd: 'screenshot', path: screenshotPath })), 'native M7 gameplay screenshot');
      screenshotHash = sha256(readFileSync(screenshotPath));
      screenshotFile = relative(artifactRoot, screenshotPath);
      sceneScreenshotHash = screenshotHash;
      sceneScreenshotFile = screenshotFile;
    }
    result = { host: 'native', mode, route, identity, active_disc: status.disc ?? null, field_map: selected, index650_sha256, index652_sha256, route_slot_sha256,
      actor3_before_input: route === 'map23' ? before : null, actor3_after_cross_600: route === 'map23' ? after : null,
      scene_actor_descriptor: sceneDescriptor, scene_actor_early: early, scene_actor_before_input: before, scene_actor_after_cross_600: after,
      memory_hash_before_input: beforeSnapshot.memoryHash, memory_hash_after_input: afterSnapshot.memoryHash,
      status_before_input: beforeSnapshot.status, status_after_input: afterSnapshot.status,
      m7_map23_screenshot_sha256: route === 'map23' ? screenshotHash : null, m7_map23_screenshot_file: route === 'map23' ? screenshotFile : null,
      m7_scene_screenshot_sha256: sceneScreenshotHash, m7_scene_screenshot_file: sceneScreenshotFile };
    writeFileSync(join(routeRoot, 'route-result.json'), `${JSON.stringify(result, null, 2)}\n`, { flag: 'wx' });
  } catch (error) {
    operationError = error;
  } finally {
    child.stdin.end();
    let timeout: ReturnType<typeof setTimeout> | undefined;
    try {
      const [code, signal] = await Promise.race([
        closed,
        new Promise<never>((_, reject) => { timeout = setTimeout(() => reject(new Error('native control child did not exit after stdin EOF')), 30_000); }),
      ]);
      writeFileSync(join(routeRoot, 'native.stderr.log'), `${stderr}\nexit=${code ?? 'null'} signal=${signal ?? 'none'}`, { flag: 'wx' });
      assert.equal(code, 0, `native control exits normally after EOF (${signal ?? 'no signal'})`);
    } catch (error) {
      cleanupError = error;
      writeFileSync(join(routeRoot, 'native.cleanup-error.log'), `${error instanceof Error ? error.stack ?? error.message : String(error)}\n`, { flag: 'wx' });
    } finally {
      if (timeout) clearTimeout(timeout);
      try {
        await transport.close();
      } catch (error) {
        if (cleanupError === undefined) cleanupError = error;
      }
    }
  }
  if (operationError !== undefined && cleanupError !== undefined) {
    throw new AggregateError([operationError, cleanupError], 'Native route failed and cleanup also failed');
  }
  if (operationError !== undefined) throw operationError;
  if (cleanupError !== undefined) throw cleanupError;
  assert.ok(result, 'native host route completed');
  return result;
}

function parsePackage(bytes: Buffer): { identity: string; manifest: PackageManifest } {
  assert.equal(bytes.toString('ascii', 0, 8), 'XEMSET1\0');
  assert.equal(bytes.readUInt32LE(8), 1);
  const member = bytes.subarray(16, 16 + bytes.readUInt32LE(12));
  assert.equal(member.toString('ascii', 0, 8), 'XEMMOD1\0');
  const length = member.readUInt32LE(8);
  const manifest = JSON.parse(member.toString('utf8', 12, 12 + length)) as PackageManifest;
  return { identity: `${manifest.id}@${sha256(member)}`, manifest };
}

let PACKAGE_IDENTITY = '';

test('sector-aware index reader preserves the seven-byte record across Form1 boundaries', () => {
  const raw = Buffer.alloc((INDEX_LBA + 3) * RAW_SECTOR_BYTES, 0xa5);
  const logical = Buffer.alloc(FORM1_BYTES * 3);
  const expected = new Map<number, Buffer>();
  for (const slot of [292, 652]) {
    const record = Buffer.alloc(7);
    record.writeUInt32LE(0x12340 + slot, 0);
    record.writeUIntBE(0x102030 + slot, 4, 3);
    record.copy(logical, slot * 7);
    expected.set(slot, record);
  }
  for (let sector = 0; sector < 3; sector++) {
    logical.subarray(sector * FORM1_BYTES, (sector + 1) * FORM1_BYTES)
      .copy(raw, (INDEX_LBA + sector) * RAW_SECTOR_BYTES + USER_DATA_OFFSET);
  }
  for (const [slot, record] of expected) {
    const observed = Buffer.alloc(7);
    let offset = slot * 7;
    let copied = 0;
    while (copied < observed.length) {
      const inSector = offset % FORM1_BYTES;
      const count = Math.min(observed.length - copied, FORM1_BYTES - inSector);
      const lba = INDEX_LBA + Math.floor(offset / FORM1_BYTES);
      raw.copy(observed, copied, lba * RAW_SECTOR_BYTES + USER_DATA_OFFSET + inSector,
        lba * RAW_SECTOR_BYTES + USER_DATA_OFFSET + inSector + count);
      offset += count;
      copied += count;
    }
    assert.deepEqual(observed, record, `slot ${slot} maps through MODE2/Form1 sector headers`);
  }
});

test('gated M7 exact Scene package matches browser and native routes on both original discs', async ({ page }) => {
  test.setTimeout(1_000_000);
  test.skip(process.env.XEM_STUDIO_M7_PACKAGE_HOSTS !== '1', 'set XEM_STUDIO_M7_PACKAGE_HOSTS=1 for explicit original-disc M7 host parity');
  const root = repoRoot;
  const packagePath = required('XEM_STUDIO_M7_SET');
  const disc1 = required('XEM_STUDIO_DISC1');
  const disc2 = required('XEM_STUDIO_DISC2');
  const headless = required('XEM_HEADLESS');
  const artifactRoot = required('XEM_STUDIO_M7_HOSTS_OUTPUT_ROOT');
  assert.ok(artifactRoot.startsWith(join(root, 'tmp/studio/milestone-7') + sep));
  assert.equal(packagePath, join(root, 'tmp/studio/milestone-7/checkpoint-86/private-stage-b/field-map-23-54458cc80057ab7af95dc59508f9f018d9c3d5531e1fa352cca154c2a1a2612d.xemmods'));
  assert.equal(disc1, '/home/bart/xenogears_toolkit/.local/discs/disc1.bin');
  assert.equal(disc2, '/home/bart/xenogears_toolkit/.local/discs/disc2.bin');
  assert.equal(headless, join(root, 'tmp/studio/milestone-6/checkpoint-36/cargo-target/release/xem-headless'));
  assert.equal(process.env.XEM_STUDIO_GAME_DIR, join(root, 'tmp/studio/milestone-6/checkpoint-36/web'));
  assert.equal(existsSync(artifactRoot), false, 'private M7 host matrix leaf must be fresh');
  mkdirSync(artifactRoot);
  assert.ok(existsSync(headless));
  assert.ok(existsSync(disc1) && existsSync(disc2));
  assert.equal(process.env.XEM_STUDIO_GAME_DIR, resolve(root, 'tmp/studio/milestone-6/checkpoint-36/web'));
  const bytes = readFileSync(packagePath);
  assert.equal(sha256(bytes), EXPECTED_SET_SHA256, 'matrix consumes the exact package set produced by checkpoint86');
  const pkg = parsePackage(bytes);
  PACKAGE_IDENTITY = pkg.identity;
  assert.equal(pkg.identity, EXPECTED_PACKAGE_IDENTITY, 'package identity matches the saved checkpoint86 UI build');
  assert.deepEqual(pkg.manifest.discs.map(item => item.serial).sort(), ['SLUS-00664', 'SLUS-00669']);
  assert.equal(pkg.manifest.files.length, 1);
  assert.deepEqual(pkg.manifest.files.map(file => [file.disc, file.slot]), [['SLUS-00664', 652]]);
  const source650 = sha256(originalIndexRecord(disc1, 650));
  const source652 = sha256(originalIndexRecord(disc1, 652));
  const trackBytes = statSync(disc1).size;
  assert.equal(trackBytes % RAW_SECTOR_BYTES, 0, 'Disc 1 original raw track has complete sectors');
  const sectors = Math.floor(trackBytes / RAW_SECTOR_BYTES);
  const expected652 = Buffer.alloc(7);
  expected652.writeUIntLE(sectors, 0, 3);
  expected652.writeInt32LE(pkg.manifest.files[0]!.result_size, 3);
  const expected652Hash = sha256(expected652);
  await page.goto('/');
  await expect(page.locator('[data-panel-id="backend"]')).toContainText('xem-studio 0.5.0');

  const browser: RouteResult[] = [];
  const native: RouteResult[] = [];
  let routeIndex = 0;
  for (const route of ['map23', 'map22'] as const) {
    for (const mode of ['absent', 'disabled', 'enabled'] as const) {
      browser.push(await browserRoute(page, mode, route, disc1, bytes, artifactRoot, routeIndex));
      native.push(await nativeRoute(mode, route, disc1, packagePath, headless, artifactRoot, routeIndex++));
    }
  }
  for (const mode of ['absent', 'disabled', 'enabled'] as const) {
    browser.push(await browserRoute(page, mode, 'disc2-title', disc2, bytes, artifactRoot, routeIndex));
    native.push(await nativeRoute(mode, 'disc2-title', disc2, packagePath, headless, artifactRoot, routeIndex++));
  }

  for (const result of [...browser, ...native]) {
    if (result.route === 'map23') {
      assert.equal(result.index650_sha256, source650, 'Disc 1 map22 index-record hash remains the original while map23 is loaded');
      if (result.mode !== 'enabled') assert.equal(result.index652_sha256, source652, 'vanilla/disabled map23 index-record hash remains original');
      assert.deepEqual(result.actor3_before_input, result.mode === 'enabled' ? [61, 0, 1270] : [29, 0, 1270],
        'map23 enters with the package-authored placement only when enabled');
      assert.deepEqual(result.actor3_after_cross_600, result.mode === 'enabled' ? [61, 0, 1270] : [29, 0, 1270],
        'only the enabled exact package changes the authored placement route after ordinary Cross and frames');
      if (result.mode === 'enabled') assert.equal(result.index652_sha256, expected652Hash, 'enabled map23 index-record hash matches the appended package extent');
    } else if (result.route === 'map22') {
      assert.equal(result.field_map, 22, 'the real runtime scenario selected map22');
      assert.equal(result.index650_sha256, source650, 'map22 index-record hash remains original');
      if (result.mode === 'enabled') assert.equal(result.index652_sha256, expected652Hash, 'the global map23 slot has the package virtual record while map22 runs');
    } else {
      assert.equal(result.active_disc, 2, 'runtime is booted from original Disc 2');
      assert.equal(result.identity.disc, 'SLUS-00664+SLUS-00669', 'verified two-disc package family remains applicable');
      assert.equal(result.index650_sha256, null);
    }
  }
  for (const route of ['map23', 'map22', 'disc2-title'] as const) {
    for (const mode of ['absent', 'disabled', 'enabled'] as const) {
      const b = browser.find(item => item.route === route && item.mode === mode)!;
      const n = native.find(item => item.route === route && item.mode === mode)!;
      assert.equal(b.identity.module, n.identity.module);
      assert.equal(b.identity.disc, n.identity.disc);
      assert.equal(b.active_disc, n.active_disc);
      assert.deepEqual(b.identity.packages, n.identity.packages);
      assert.equal(b.field_map, n.field_map);
      assert.equal(b.index650_sha256, n.index650_sha256);
      assert.equal(b.index652_sha256, n.index652_sha256);
      assert.deepEqual(b.actor3_before_input, n.actor3_before_input);
      assert.deepEqual(b.actor3_after_cross_600, n.actor3_after_cross_600);
      assert.equal(b.memory_hash_before_input, n.memory_hash_before_input);
      assert.equal(b.memory_hash_after_input, n.memory_hash_after_input);
      assert.deepEqual(b.status_before_input, n.status_before_input);
      assert.deepEqual(b.status_after_input, n.status_after_input);
    }
  }
  for (const host of [browser, native]) {
    for (const route of ['map23', 'map22', 'disc2-title'] as const) {
      const absent = host.find(item => item.route === route && item.mode === 'absent')!;
      const disabled = host.find(item => item.route === route && item.mode === 'disabled')!;
      assert.equal(disabled.index650_sha256, absent.index650_sha256, `${route}: disabled package preserves map22 index evidence`);
      assert.equal(disabled.index652_sha256, absent.index652_sha256, `${route}: disabled package preserves map23 index evidence`);
      assert.equal(disabled.memory_hash_before_input, absent.memory_hash_before_input, `${route}: disabled memory equals vanilla before input`);
      assert.equal(disabled.memory_hash_after_input, absent.memory_hash_after_input, `${route}: disabled memory equals vanilla after input`);
      assert.deepEqual(disabled.status_before_input, absent.status_before_input, `${route}: disabled status equals vanilla before input`);
      assert.deepEqual(disabled.status_after_input, absent.status_after_input, `${route}: disabled status equals vanilla after input`);
      assert.equal(disabled.identity.module, absent.identity.module);
      assert.equal(disabled.active_disc, absent.active_disc);
      if (route === 'map23') {
        assert.deepEqual(disabled.actor3_before_input, absent.actor3_before_input);
        assert.deepEqual(disabled.actor3_after_cross_600, absent.actor3_after_cross_600);
      }
    }
    const disc2Absent = host.find(item => item.route === 'disc2-title' && item.mode === 'absent')!;
    for (const mode of ['disabled', 'enabled'] as const) {
      const current = host.find(item => item.route === 'disc2-title' && item.mode === mode)!;
      assert.equal(current.memory_hash_before_input, disc2Absent.memory_hash_before_input, `Disc 2 ${mode} has no package file effects before input`);
      assert.equal(current.memory_hash_after_input, disc2Absent.memory_hash_after_input, `Disc 2 ${mode} has no package file effects after input`);
      assert.deepEqual(current.status_before_input, disc2Absent.status_before_input, `Disc 2 ${mode} status matches package-absent route before input`);
      assert.deepEqual(current.status_after_input, disc2Absent.status_after_input, `Disc 2 ${mode} status matches package-absent route after input`);
    }
  }
  for (const result of [...browser, ...native].filter(item => item.route === 'disc2-title')) {
    assert.equal(result.active_disc, 2);
    assert.equal(result.identity.disc, 'SLUS-00664+SLUS-00669');
    assert.equal(result.identity.packages.includes(pkg.identity), result.mode === 'enabled');
  }
  for (const host of [browser, native]) {
    for (const mode of ['absent', 'enabled'] as const) {
      const result = host.find(item => item.route === 'map23' && item.mode === mode)!;
      assert.match(result.m7_map23_screenshot_sha256 ?? '', /^[0-9a-f]{64}$/, `${host === browser ? 'browser' : 'native'} ${mode} M7 gameplay screenshot was captured after Cross+600`);
    }
    const disabled = host.find(item => item.route === 'map23' && item.mode === 'disabled')!;
    assert.equal(disabled.m7_map23_screenshot_sha256, null, 'disabled route does not add an unnecessary screenshot');
  }
  const output = { schema_version: 1, package_set_sha256: sha256(bytes), package_identity: pkg.identity,
    module_sha256: MODULE_SHA256, source_index_record_sha256: { disc1_map22_650: source650, disc1_map23_652: source652 }, browser, native,
    checks: { map22_actual_route_both_hosts: true, map22_full_file_preservation: 'verified by field_m7_scene_package gated Rust test',
      map23_only_enabled_position_change: true, disc2_package_family_passthrough: true, hostParity: true } };
  writeFileSync(join(artifactRoot, 'host-routes.json'), `${JSON.stringify(output, null, 2)}\n`, { flag: 'wx' });
});

test('gated offline Map 23 Scene package runs identically in fresh native and browser hosts', async ({ page }) => {
  test.setTimeout(1_200_000);
  test.skip(process.env.XEM_STUDIO_CONTEXT_UX_HOSTS !== '1', 'set XEM_STUDIO_CONTEXT_UX_HOSTS=1 for the fresh offline Scene package host check');
  const root = repoRoot;
  const packagePath = required('XEM_STUDIO_CONTEXT_UX_SET');
  const disc1 = required('XEM_STUDIO_CONTEXT_UX_DISC1');
  const headless = required('XEM_STUDIO_CONTEXT_UX_HEADLESS');
  const runtimeWeb = required('XEM_STUDIO_CONTEXT_UX_GAME_DIR');
  const artifactRoot = required('XEM_STUDIO_CONTEXT_UX_HOSTS_OUTPUT');
  const expectedSetSha256 = '5ba108ec9d59316dcb3e55072d7826c16b527cdd7db55b269d78d089f6defa2a';
  const expectedBundleSha256 = 'd9506735b94efcc4cb41f33a8be721f46319353fcf3a26db4b5cadd21007bbbc';
  assert.equal(basename(packagePath), 'offline-map23-edited.xemmods');
  assert.ok(packagePath.startsWith(join(root, 'tmp/studio/context-ux') + sep), 'package input stays in isolated ignored context-ux storage');
  assert.equal(basename(disc1), 'disc1.bin');
  assert.equal(basename(headless), 'xem-headless');
  assert.equal(basename(runtimeWeb), 'web');
  assert.equal(process.env.XEM_STUDIO_GAME_DIR, runtimeWeb, 'browser loads the accepted read-only M6 game build');
  assert.ok(artifactRoot.startsWith(join(root, 'tmp/studio/context-ux') + sep), 'all new route data stays under isolated ignored context-ux storage');
  assert.equal(existsSync(artifactRoot), false, 'fresh package-host output leaf must not overwrite prior evidence');
  mkdirSync(artifactRoot);
  assert.ok(existsSync(headless) && existsSync(disc1));
  assert.equal(await fileSha256(disc1), '39c547a9afc6da15d847ef81a2c6cea1a6516bdfa562cf13b0999b04e8598bda');
  assert.equal(await fileSha256(headless), 'dabe1826f54dc379fefc2a28899ec86eccb79990bdd5c03ed307c94f952e14cb');

  const modulePath = join(runtimeWeb, 'game/game.wasm');
  const schemaPath = join(runtimeWeb, 'game/schema.json');
  assert.ok(existsSync(modulePath) && existsSync(schemaPath));
  const schema = JSON.parse(readFileSync(schemaPath, 'utf8')) as { module_sha256?: string };
  assert.equal(schema.module_sha256, MODULE_SHA256);
  assert.equal(sha256(readFileSync(modulePath)), MODULE_SHA256);
  const packageBytes = readFileSync(packagePath);
  assert.equal(sha256(packageBytes), expectedSetSha256, 'consume the exact set saved by the new offline UI build');
  assert.notEqual(sha256(packageBytes), EXPECTED_SET_SHA256, 'do not reuse the prior context-backed M7 package');
  const pkg = parsePackage(packageBytes);
  PACKAGE_IDENTITY = pkg.identity;
  assert.equal(pkg.manifest.files.length, 1);
  assert.deepEqual(pkg.manifest.files.map(file => [file.disc, file.slot, file.source_sha256, file.result_sha256]), [
    ['SLUS-00664', 652, '0f0a131acdf052cae070a64912fc09eb00e0b5e6a81f9d8cb4719d6416213add', expectedBundleSha256],
  ]);
  assert.equal(pkg.manifest.discs.map(disc => disc.serial).sort().join(','), 'SLUS-00664,SLUS-00669');
  const original650 = sha256(originalIndexRecord(disc1, 650));
  const original652 = sha256(originalIndexRecord(disc1, 652));
  const sectorCount = Math.floor(statSync(disc1).size / RAW_SECTOR_BYTES);
  const expected652 = Buffer.alloc(7);
  expected652.writeUIntLE(sectorCount, 0, 3);
  expected652.writeInt32LE(pkg.manifest.files[0]!.result_size, 3);
  const expected652Hash = sha256(expected652);
  const resultFile = join(artifactRoot, 'offline-map23-hosts.json');
  assert.equal(existsSync(resultFile), false);

  await page.goto('/');
  await expect(page.locator('[data-panel-id="backend"]')).toContainText('xem-studio 0.5.0');
  const browser: RouteResult[] = [];
  const native: RouteResult[] = [];
  let routeIndex = 0;
  for (const mode of ['absent', 'disabled', 'enabled'] as const) {
    browser.push(await browserRoute(page, mode, 'map23', disc1, packageBytes, artifactRoot, routeIndex));
    native.push(await nativeRoute(mode, 'map23', disc1, packagePath, headless, artifactRoot, routeIndex));
    routeIndex += 1;
  }

  for (const result of [...browser, ...native]) {
    assert.equal(result.identity.module, MODULE_SHA256);
    assert.equal(result.identity.disc, 'SLUS-00664+SLUS-00669');
    assert.equal(result.identity.packages.join(','), result.mode === 'enabled' ? PACKAGE_IDENTITY : '');
    assert.equal(result.active_disc, 1);
    assert.equal(result.field_map, 23);
    assert.equal(result.index650_sha256, original650, 'map22 index slot650 is untouched');
    if (result.mode === 'enabled') {
      assert.equal(result.index652_sha256, expected652Hash);
      assert.deepEqual(result.actor3_before_input, [61, 0, 1270]);
      assert.deepEqual(result.actor3_after_cross_600, [61, 0, 1270]);
    } else {
      assert.equal(result.index652_sha256, original652);
      assert.deepEqual(result.actor3_before_input, [29, 0, 1270]);
      assert.deepEqual(result.actor3_after_cross_600, [29, 0, 1270]);
    }
    assert.notEqual(result.memory_hash_before_input, '');
    assert.notEqual(result.memory_hash_after_input, '');
    assert.equal(result.status_after_input.field_map, 23);
  }

  for (const mode of ['absent', 'disabled', 'enabled'] as const) {
    const browserResult = browser.find(item => item.mode === mode)!;
    const nativeResult = native.find(item => item.mode === mode)!;
    assert.equal(browserResult.identity.module, nativeResult.identity.module);
    assert.equal(browserResult.identity.disc, nativeResult.identity.disc);
    assert.deepEqual(browserResult.identity.packages, nativeResult.identity.packages);
    assert.equal(browserResult.index650_sha256, nativeResult.index650_sha256);
    assert.equal(browserResult.index652_sha256, nativeResult.index652_sha256);
    assert.deepEqual(browserResult.actor3_before_input, nativeResult.actor3_before_input);
    assert.deepEqual(browserResult.actor3_after_cross_600, nativeResult.actor3_after_cross_600);
    assert.equal(browserResult.memory_hash_before_input, nativeResult.memory_hash_before_input);
    assert.equal(browserResult.memory_hash_after_input, nativeResult.memory_hash_after_input);
    assert.deepEqual(browserResult.status_before_input, nativeResult.status_before_input);
    assert.deepEqual(browserResult.status_after_input, nativeResult.status_after_input);
  }
  for (const host of [browser, native]) {
    const absent = host.find(item => item.mode === 'absent')!;
    const disabled = host.find(item => item.mode === 'disabled')!;
    assert.equal(disabled.memory_hash_before_input, absent.memory_hash_before_input);
    assert.equal(disabled.memory_hash_after_input, absent.memory_hash_after_input);
    assert.deepEqual(disabled.status_before_input, absent.status_before_input);
    assert.deepEqual(disabled.status_after_input, absent.status_after_input);
    assert.deepEqual(disabled.actor3_before_input, absent.actor3_before_input);
    assert.deepEqual(disabled.actor3_after_cross_600, absent.actor3_after_cross_600);
  }
  assert.ok(browser.some(item => item.mode === 'absent' && item.m7_map23_screenshot_sha256));
  assert.ok(browser.some(item => item.mode === 'enabled' && item.m7_map23_screenshot_sha256));
  assert.ok(native.some(item => item.mode === 'absent' && item.m7_map23_screenshot_sha256));
  assert.ok(native.some(item => item.mode === 'enabled' && item.m7_map23_screenshot_sha256));
  writeFileSync(resultFile, `${JSON.stringify({
    schema_version: 1,
    module_sha256: MODULE_SHA256,
    original_bundle_sha256: '0f0a131acdf052cae070a64912fc09eb00e0b5e6a81f9d8cb4719d6416213add',
    compiled_candidate_sha256: expectedBundleSha256,
    package_set_sha256: expectedSetSha256,
    package_identity: PACKAGE_IDENTITY,
    routes: [...browser, ...native],
  }, null, 2)}\n`, { flag: 'wx' });
});

test('gated offline Map 0 and Map 145 Scene packages preserve their exact edited routes across native and browser', async ({ page }) => {
  test.setTimeout(1_200_000);
  test.skip(process.env.XEM_STUDIO_M7_OFFLINE_MAP_HOSTS !== '1', 'set XEM_STUDIO_M7_OFFLINE_MAP_HOSTS=1 for the two offline Scene host matrices');
  const root = repoRoot;
  const disc1 = required('XEM_STUDIO_DISC1');
  const headless = required('XEM_HEADLESS');
  const runtimeWeb = required('XEM_STUDIO_GAME_DIR');
  const artifactRoot = required('XEM_STUDIO_M7_OFFLINE_MAP_HOSTS_OUTPUT');
  assert.ok(artifactRoot.startsWith(join(root, 'tmp/studio/context-ux') + sep), 'all route evidence remains in isolated ignored context-ux storage');
  assert.equal(basename(disc1), 'disc1.bin');
  assert.equal(basename(headless), 'xem-headless');
  assert.equal(basename(runtimeWeb), 'web');
  assert.equal(process.env.XEM_STUDIO_GAME_DIR, runtimeWeb, 'browser uses the accepted read-only M6 runtime build');
  assert.equal(existsSync(artifactRoot), false, 'fresh two-map route output leaf must not overwrite previous evidence');
  mkdirSync(artifactRoot);
  assert.ok(existsSync(disc1) && existsSync(headless));
  assert.equal(await fileSha256(disc1), '39c547a9afc6da15d847ef81a2c6cea1a6516bdfa562cf13b0999b04e8598bda');
  assert.equal(await fileSha256(headless), 'dabe1826f54dc379fefc2a28899ec86eccb79990bdd5c03ed307c94f952e14cb');
  const modulePath = join(runtimeWeb, 'game/game.wasm');
  const schemaPath = join(runtimeWeb, 'game/schema.json');
  const runtimeSchema = JSON.parse(readFileSync(schemaPath, 'utf8')) as { module_sha256?: string };
  assert.equal(runtimeSchema.module_sha256, MODULE_SHA256);
  assert.equal(sha256(readFileSync(modulePath)), MODULE_SHA256);
  await page.goto('/');
  await expect(page.locator('[data-panel-id="backend"]')).toContainText('xem-studio 0.5.0');

  const allMaps = [
    { map: 0, route: 'map0' as const, slot: 606, descriptor: 18, source: 'c7cdbefd834e1d0405482d95204c822c8f4ef35249eee62c54a3af09cfd52921', originalX: 0, editedX: 32, candidate: '8818c8454e507b9fd64cd9651b5d8edce8caa32c312fa0dd48bd3f382384259c' },
    { map: 145, route: 'map145' as const, slot: 896, descriptor: 11, source: 'faa4ff0fa1754af3180a48f7a786d6086dc4f5d9e926ca91b69f703a98506619', originalX: 382, editedX: 366, candidate: '0c4742046506af9226525468d0171d235d7f1ef40de0efd9d61c02712dcdf4b2' },
  ];
  const maps = process.env.XEM_STUDIO_M7_MAP145_ONLY === '1'
    ? allMaps.filter(fixture => fixture.map === 145)
    : allMaps;
  assert.ok(maps.length > 0, 'at least one approved map route is selected');
  const summary: Record<string, unknown> = { schema_version: 1, module_sha256: MODULE_SHA256, routes: [] as RouteResult[] };
  const allResults = summary.routes as RouteResult[];
  let routeIndex = 0;
  for (const fixture of maps) {
    const packagePath = required(`XEM_STUDIO_M7_MAP${fixture.map}_SET`);
    const expectedSetSha = process.env[`XEM_STUDIO_M7_MAP${fixture.map}_SET_SHA256`];
    assert.ok(expectedSetSha && /^[0-9a-f]{64}$/.test(expectedSetSha), `Map ${fixture.map} set SHA-256 must be supplied as hexadecimal metadata`);
    const packageRoot = join(root, 'tmp/studio/context-ux') + sep;
    assert.ok(packagePath.startsWith(packageRoot), `Map ${fixture.map} set stays in ignored context-ux artifacts`);
    assert.equal(basename(packagePath), `offline-map${fixture.map}-edited.xemmods`);
    const bytes = readFileSync(packagePath);
    assert.equal(sha256(bytes), expectedSetSha, `Map ${fixture.map} host route consumes exact UI-built package bytes`);
    const pkg = parsePackage(bytes);
    PACKAGE_IDENTITY = pkg.identity;
    assert.equal(pkg.manifest.files.length, 1);
    assert.deepEqual(pkg.manifest.files.map(file => [file.disc, file.slot, file.source_sha256]), [
      ['SLUS-00664', fixture.slot, fixture.source],
    ]);
    const target = pkg.manifest.files[0]!;
    assert.notEqual(target.result_sha256, fixture.source, `Map ${fixture.map} package contains an actual edited result`);
    assert.equal(target.result_sha256, fixture.candidate, `Map ${fixture.map} package contains the exact Scene build candidate`);
    const originalTargetIndex = sha256(originalIndexRecord(disc1, fixture.slot));
    const originalMap22Index = sha256(originalIndexRecord(disc1, 650));
    const sectorCount = Math.floor(statSync(disc1).size / RAW_SECTOR_BYTES);
    const virtualRecord = Buffer.alloc(7);
    virtualRecord.writeUIntLE(sectorCount, 0, 3);
    virtualRecord.writeInt32LE(target.result_size, 3);
    const expectedTargetIndex = sha256(virtualRecord);
    const packageIdentity = pkg.identity;
    const results: RouteResult[] = [];
    const mapArtifactRoot = join(artifactRoot, `map-${fixture.map}`);
    mkdirSync(mapArtifactRoot);
    for (const mode of ['absent', 'disabled', 'enabled'] as const) {
      const browserResult = await browserRoute(page, mode, fixture.route, disc1, bytes, mapArtifactRoot, routeIndex);
      const nativeResult = await nativeRoute(mode, fixture.route, disc1, packagePath, headless, mapArtifactRoot, routeIndex++);
      results.push(browserResult, nativeResult);
      for (const result of [browserResult, nativeResult]) {
        assert.equal(result.field_map, fixture.map);
        assert.equal(result.identity.packages.join(','), mode === 'enabled' ? packageIdentity : '');
        assert.equal(result.scene_actor_descriptor, fixture.descriptor);
        assert.equal(result.route_slot_sha256, mode === 'enabled' ? expectedTargetIndex : originalTargetIndex);
        assert.equal(result.index650_sha256, originalMap22Index, 'unrelated map22 source index record remains unchanged');
        const position = result.scene_actor_before_input;
        assert.ok(position, `Map ${fixture.map} source actor position is readable before input`);
        if (fixture.map === 145) {
          // Source PC 124 initializes X=382 (edited to 366); event 1 patrols
          // between X=262 and X=382. Check the observed trajectory within
          // those source-derived bounds without assuming its phase.
          const patrolLow = 262;
          const patrolHigh = fixture.originalX;
          const early = result.scene_actor_early;
          const after = result.scene_actor_after_cross_600;
          assert.ok(early && early[0] >= patrolLow && early[0] <= patrolHigh,
            `Map 145 ${mode} X stays within its source-derived patrol bounds at the early checkpoint`);
          assert.ok(position[0] >= patrolLow && position[0] <= patrolHigh,
            `Map 145 ${mode} X stays within its source-derived patrol bounds at the first checkpoint`);
          assert.ok(after && after[0] >= patrolLow && after[0] <= patrolHigh,
            `Map 145 ${mode} X stays within its source-derived patrol bounds after Cross+600`);
          assert.notEqual(early[0], position[0],
            `Map 145 ${mode} source placement moves along the authored patrol before frame 733`);
          assert.equal(early[2], 322);
          assert.equal(position[2], 322);
          assert.equal(after[2], 322);
        } else {
          assert.equal(position[0], mode === 'enabled' ? fixture.editedX : fixture.originalX,
            `Map ${fixture.map} enabled package changes only the selected source X coordinate`);
          assert.deepEqual(result.scene_actor_after_cross_600, result.scene_actor_before_input,
            `Map ${fixture.map} selected source position stays stable through Cross+600`);
        }
        assert.match(result.memory_hash_before_input, /^[0-9a-f]{16}$/, 'runtime memory_hash is the exact 64-bit hexadecimal contract');
        assert.match(result.memory_hash_after_input, /^[0-9a-f]{16}$/, 'runtime memory_hash is the exact 64-bit hexadecimal contract');
        assert.equal(result.status_after_input.field_map, fixture.map);
        if (mode === 'disabled') {
          assert.equal(result.m7_scene_screenshot_sha256, null);
        } else {
          assert.match(result.m7_scene_screenshot_sha256 ?? '', /^[0-9a-f]{64}$/);
        }
      }
      assert.deepEqual(browserResult.scene_actor_before_input, nativeResult.scene_actor_before_input, 'browser and native observe identical raw source pose');
      assert.deepEqual(browserResult.scene_actor_early, nativeResult.scene_actor_early, 'browser and native observe identical early source pose');
      assert.deepEqual(browserResult.scene_actor_after_cross_600, nativeResult.scene_actor_after_cross_600);
      assert.equal(browserResult.memory_hash_before_input, nativeResult.memory_hash_before_input);
      assert.equal(browserResult.memory_hash_after_input, nativeResult.memory_hash_after_input);
      assert.deepEqual(browserResult.status_before_input, nativeResult.status_before_input);
      assert.deepEqual(browserResult.status_after_input, nativeResult.status_after_input);
    }
    const hostPairs = [
      results.filter(item => item.host === 'browser'),
      results.filter(item => item.host === 'native'),
    ];
    for (const host of hostPairs) {
      const absent = host.find(item => item.mode === 'absent')!;
      const disabled = host.find(item => item.mode === 'disabled')!;
      const enabled = host.find(item => item.mode === 'enabled')!;
      assert.equal(disabled.memory_hash_before_input, absent.memory_hash_before_input);
      assert.equal(disabled.memory_hash_after_input, absent.memory_hash_after_input);
      assert.deepEqual(disabled.status_before_input, absent.status_before_input);
      assert.deepEqual(disabled.status_after_input, absent.status_after_input);
      assert.deepEqual(disabled.scene_actor_before_input, absent.scene_actor_before_input);
      assert.deepEqual(disabled.scene_actor_after_cross_600, absent.scene_actor_after_cross_600);
      assert.deepEqual(disabled.scene_actor_early, absent.scene_actor_early);
      assert.deepEqual(enabled.scene_actor_before_input?.slice(1), absent.scene_actor_before_input?.slice(1), 'package edit preserves the other source axes');
      assert.deepEqual(enabled.scene_actor_after_cross_600?.slice(1), absent.scene_actor_after_cross_600?.slice(1));
      if (fixture.map === 145) {
        assert.deepEqual(disabled.scene_actor_before_input, absent.scene_actor_before_input,
          'Map 145 disabled package preserves the vanilla patrol position before input');
        assert.deepEqual(disabled.scene_actor_after_cross_600, absent.scene_actor_after_cross_600,
          'Map 145 disabled package preserves the vanilla patrol position after Cross+600');
        assert.deepEqual(disabled.scene_actor_early, absent.scene_actor_early,
          'Map 145 disabled package preserves the vanilla patrol position at the early checkpoint');
        assert.notEqual(enabled.scene_actor_early?.[0], absent.scene_actor_early?.[0],
          'Map 145 enabled initial placement changes the early patrol position');
        assert.notEqual(enabled.scene_actor_before_input?.[0], absent.scene_actor_before_input?.[0],
          'Map 145 enabled initial placement changes the first observed patrol position');
        assert.notEqual(enabled.scene_actor_after_cross_600?.[0], absent.scene_actor_after_cross_600?.[0],
          'Map 145 enabled initial placement changes the later observed patrol position');
      }
    }
    allResults.push(...results);
    (summary as Record<string, unknown>)[`map${fixture.map}`] = {
      package_sha256: expectedSetSha,
      package_identity: packageIdentity,
      source_sha256: fixture.source,
      result_sha256: target.result_sha256,
      original_index_sha256: originalTargetIndex,
      virtual_index_sha256: expectedTargetIndex,
    };
  }
  writeFileSync(join(artifactRoot, 'offline-map-routes.json'), `${JSON.stringify(summary, null, 2)}\n`, { flag: 'wx' });
});
