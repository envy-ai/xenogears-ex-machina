import { spawn, type ChildProcess } from 'node:child_process';
import { mkdir, readFile, stat } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { join, resolve, sep } from 'node:path';
import { chromium } from '@playwright/test';
import { expect } from 'vitest';
import { createServer } from 'vite';
import { studioGameServer } from '../../scripts/game-server.js';
import { GameController } from '../../src/game/controller';
import { launchNativeGame } from '../../src/game/native';
import { runLiveParitySequence, type LiveParityProbe } from './live-game-sequence';

const testRoot = resolve(process.env.XEM_STUDIO_M4_ARTIFACT_ROOT ?? '');
const studioRoot = resolve(import.meta.dirname, '../..');
const repoRoot = resolve(studioRoot, '../..');
const runtimeBuildDirectory = resolve(process.env.XEM_STUDIO_RUNTIME_DIR ?? '');
const discPath = resolve(process.env.XEM_STUDIO_DISC1 ?? '');
const nativeExecutable = resolve(process.env.XEM_STUDIO_NATIVE_EXE ?? '');
const chromiumPath = process.env.XEM_CHROMIUM;
const lavapipe = process.env.XEM_LAVAPIPE_ICD;

export async function runRealGameRealParity(): Promise<void> {
  const acceptedRoots = ['milestone-4', 'milestone-5'].map(name => resolve(repoRoot, 'tmp/studio', name));
  const underAcceptedRoot = (path: string) => acceptedRoots.some(root => path === root || path.startsWith(root + sep));
  expect(underAcceptedRoot(testRoot)).toBe(true);
  expect(underAcceptedRoot(runtimeBuildDirectory)).toBe(true);
  expect(discPath).toMatch(/(?:^|\/)disc1\.bin$/);
  expect(nativeExecutable).toContain(resolve(repoRoot, 'src/runtime/target/release/xem-desktop'));
  expect(chromiumPath).toBeTruthy();
  expect(lavapipe).toBeTruthy();
  const modulePath = join(runtimeBuildDirectory, 'game/game.wasm');
  const schemaPath = join(runtimeBuildDirectory, 'game/schema.json');
  for (const path of [join(runtimeBuildDirectory, 'index.html'), join(runtimeBuildDirectory, 'pkg/xem_web.js'), modulePath, schemaPath, discPath, nativeExecutable, lavapipe!]) {
    expect((await stat(path)).isFile(), `required local input: ${path}`).toBe(true);
  }
  const schema = JSON.parse(await readFile(schemaPath, 'utf8')) as { module_sha256?: unknown };
  const moduleHash = createHash('sha256').update(await readFile(modulePath)).digest('hex');
  expect(schema.module_sha256).toBe(moduleHash);

  const runRoot = join(testRoot, `real-parity-${process.pid}-${Date.now()}`);
  await mkdir(runRoot, { recursive: false });
  // Wayland socket paths are limited to 107 bytes; keep this unique runtime
  // directory short even though the evidence directory is intentionally long.
  const runtimeDirectory = join(repoRoot, 'tmp/studio', `m4-weston-${process.pid}-${Date.now()}`);
  const settingsDirectory = join(runRoot, 'native-settings');
  const cardsDirectory = join(runRoot, 'native-cards');
  await mkdir(runtimeDirectory, { mode: 0o700 });
  await mkdir(settingsDirectory, { recursive: true });
  await mkdir(cardsDirectory, { recursive: true });

  const priorEnvironment = Object.fromEntries(['XDG_RUNTIME_DIR', 'WAYLAND_DISPLAY', 'SDL_VIDEODRIVER', 'SDL_AUDIO_DRIVER', 'VK_ICD_FILENAMES']
    .map(name => [name, process.env[name]]));
  process.env.XDG_RUNTIME_DIR = runtimeDirectory;
  process.env.WAYLAND_DISPLAY = 'wl-xem-m4';
  process.env.SDL_VIDEODRIVER = 'wayland';
  process.env.SDL_AUDIO_DRIVER = 'dummy';
  process.env.VK_ICD_FILENAMES = lavapipe;

  let weston: ChildProcess | undefined;
  let westonDiagnostics = '';
  let vite: Awaited<ReturnType<typeof createServer>> | undefined;
  let browser: Awaited<ReturnType<typeof chromium.launch>> | undefined;
  let nativeController: GameController | undefined;
  try {
    weston = spawn('weston', [
      '--backend=headless', '--renderer=pixman', '--width=1280', '--height=800',
      '--socket=wl-xem-m4', '--idle-time=0',
    ], { env: { ...process.env }, stdio: ['ignore', 'ignore', 'pipe'] });
    weston.stderr?.setEncoding('utf8');
    weston.stderr?.on('data', (text: string) => { westonDiagnostics = (westonDiagnostics + text).slice(-16_384); });
    const socket = join(runtimeDirectory, 'wl-xem-m4');
    const westonStarted = Date.now();
    while (!(await stat(socket).then(() => true, () => false))) {
      if (weston.exitCode !== null) throw new Error(`Headless Weston exited with ${weston.exitCode}${westonDiagnostics ? `:\n${westonDiagnostics}` : ''}`);
      if (Date.now() - westonStarted > 20_000) throw new Error('Headless Weston did not create its Wayland socket within 20 seconds');
      await new Promise(resolveDelay => setTimeout(resolveDelay, 50));
    }

    vite = await createServer({
      configFile: false,
      root: studioRoot,
      plugins: [studioGameServer(runtimeBuildDirectory)],
      server: { host: '127.0.0.1', port: 0, strictPort: false },
      logLevel: 'error',
    });
    await vite.listen();
    const address = vite.httpServer?.address();
    if (!address || typeof address === 'string') throw new Error('The isolated same-origin runtime server has no TCP port');
    const origin = `http://127.0.0.1:${address.port}`;

    browser = await chromium.launch({ executablePath: chromiumPath, headless: true, chromiumSandbox: true });
    const context = await browser.newContext();
    const page = await context.newPage();
    const browserErrors: string[] = [];
    page.on('pageerror', error => browserErrors.push(error.message));
    page.on('console', message => { if (message.type() === 'error') browserErrors.push(message.text()); });
    await page.goto(`${origin}/runtime/`);
    await page.waitForFunction(() => document.body.dataset.ready === 'true', undefined, { timeout: 120_000 });
    expect(await page.title()).toMatch(/Xenogears Ex Machina/);
    expect(await page.locator('#file').count()).toBe(1);
    const initialMessage = await page.locator('#message').textContent();
    await page.locator('#file').setInputFiles(discPath);
    await page.waitForFunction(initial => {
      const message = document.querySelector('#message')?.textContent;
      return Boolean(message && message !== initial && !message.startsWith('Reading '));
    }, initialMessage, { timeout: 120_000 });
    const importMessage = await page.locator('#message').textContent();
    expect(importMessage, `Disc import failed: ${importMessage}; browser errors: ${browserErrors.join(' | ')}`).toMatch(/Xenogears disc 1 \(/);
    await page.evaluate(async () => {
      const runtime = window as unknown as Window & { xem: { boot(options: { run: boolean; rewind: boolean }): Promise<unknown>; configureRewind(bytes: number): Promise<unknown> } };
      await runtime.xem.boot({ run: false, rewind: false });
      await runtime.xem.configureRewind(0);
    });

    const browserProbe = await page.evaluate(async () => {
      const modulePath = '/tests/e2e/live-game-sequence.ts';
      const load = new Function('path', 'return import(path)') as (path: string) => Promise<{ runBrowserParityPage(): Promise<unknown> }>;
      const driver = await load(modulePath);
      return driver.runBrowserParityPage();
    }) as LiveParityProbe;

    nativeController = new GameController(() => {}, () => {});
    const nativeTransport = launchNativeGame({
      executable: nativeExecutable,
      disc: discPath,
      settingsDirectory,
      cardsDirectory,
    }, spawn, process.env);
    await nativeController.connect(nativeTransport);
    await nativeController.command({ cmd: 'rewind_configure', max_bytes: 0 });
    const nativeProbe = await runLiveParitySequence(nativeController);

    expect(nativeProbe.identity).toEqual(browserProbe.identity);
    expect(nativeProbe.statusSnapshots).toEqual(browserProbe.statusSnapshots);
    expect(nativeProbe.hashes).toEqual(browserProbe.hashes);
    expect(nativeProbe.fieldMap).toBe('30');
    expect(nativeProbe.validWriteReadback).toBe('30');
    expect(nativeProbe.invalidWriteError).toMatch(/unknown path|not found|invalid/i);
    expect(browserProbe.invalidWriteError).toBe(nativeProbe.invalidWriteError);
    expect(nativeProbe.changedHash).toBe(browserProbe.changedHash);
    expect(nativeProbe.restoredHash).toBe(browserProbe.restoredHash);
    expect(nativeProbe.framesWhilePaused).toEqual(browserProbe.framesWhilePaused);

    const result = {
      moduleSha256: nativeProbe.identity.module,
      discIdentity: nativeProbe.identity.disc,
      fieldMap: nativeProbe.fieldMap,
      replayFrames: [303, 400],
      parityStatusSnapshots: nativeProbe.statusSnapshots.length,
      parityMemoryHashes: nativeProbe.hashes.length,
      invalidWriteRejectedWithoutMemoryMutation: true,
      snapshotRestoreHash: nativeProbe.restoredHash,
      pausedFrames: nativeProbe.framesWhilePaused,
      hostMetadataExclusions: ['host_running', 'renderer'],
    };
    const { writeFile } = await import('node:fs/promises');
    await writeFile(join(runRoot, 'result.json'), `${JSON.stringify(result, null, 2)}\n`, { flag: 'wx' });
    await context.close();
  } finally {
    if (nativeController) await nativeController.disconnect().catch(() => {});
    if (browser) await browser.close().catch(() => {});
    if (vite) await vite.close().catch(() => {});
    if (weston && weston.exitCode === null && weston.signalCode === null) {
      weston.kill('SIGTERM');
      await new Promise<void>(resolveExit => {
        const timer = setTimeout(resolveExit, 2_000);
        weston?.once('exit', () => { clearTimeout(timer); resolveExit(); });
      });
    }
    for (const [name, value] of Object.entries(priorEnvironment)) {
      if (value === undefined) delete process.env[name];
      else process.env[name] = value;
    }
  }

}
