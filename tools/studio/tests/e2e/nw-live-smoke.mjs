import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { mkdir, readFile, stat, writeFile, cp } from 'node:fs/promises';
import { findpath } from 'nw';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptDirectory = dirname(fileURLToPath(import.meta.url));
const studioRoot = resolve(scriptDirectory, '../..');
const repositoryRoot = resolve(studioRoot, '../..');
const artifactRoot = resolve(process.env.XEM_STUDIO_M4_ARTIFACT_ROOT ?? '');
const testDist = resolve(process.env.XEM_STUDIO_LIVE_DIST ?? '');
const executable = resolve(process.env.XEM_STUDIO_NATIVE_EXE ?? '');
const disc = resolve(process.env.XEM_STUDIO_DISC1 ?? '');
assert.ok(artifactRoot.startsWith(join(repositoryRoot, 'tmp/studio/milestone-4/')));
assert.ok(testDist.startsWith(join(repositoryRoot, 'tmp/studio/milestone-4/')));
assert.ok(executable.startsWith(join(repositoryRoot, 'src/runtime/target/release/')));
assert.match(disc, /\.(bin|chd)$/i);
for (const input of [join(testDist, 'index.html'), executable, disc]) {
  assert.ok((await stat(input)).isFile(), `required NW smoke input is missing: ${input}`);
}

const runDirectory = join(artifactRoot, `nw-live-${process.pid}-${Date.now()}`);
await mkdir(runDirectory, { recursive: false });
const settingsDirectory = join(runDirectory, 'settings');
const cardsDirectory = join(runDirectory, 'cards');
await mkdir(settingsDirectory, { mode: 0o700 });
await mkdir(cardsDirectory, { mode: 0o700 });
const displayDirectory = join(repositoryRoot, 'tmp/studio', `m4nw-${process.pid}-${Date.now()}`);
await mkdir(displayDirectory, { mode: 0o700 });
const waylandSocket = 'wl-xem-m4-nw';
const outputPath = join(runDirectory, 'result.json');
const configPath = join(runDirectory, 'config.json');
await writeFile(configPath, `${JSON.stringify({
  executable, disc, settingsDirectory, cardsDirectory, output: outputPath,
  waylandRuntimeDirectory: displayDirectory,
  waylandDisplay: waylandSocket,
  ...(process.env.XEM_LAVAPIPE_ICD ? { lavapipeIcd: process.env.XEM_LAVAPIPE_ICD } : {}),
})}\n`, { flag: 'wx' });
await cp(testDist, join(runDirectory, 'dist'), { recursive: true, errorOnExist: true });
const manifest = {
  name: 'xem-studio-live-game-test',
  version: '0.5.0',
  main: 'dist/index.html',
  window: { title: 'XEM Studio live test', width: 1440, height: 900, min_width: 1180, min_height: 720, frame: false },
};
await writeFile(join(runDirectory, 'package.json'), `${JSON.stringify(manifest, null, 2)}\n`, { flag: 'wx' });

const binary = await findpath('nwjs', { version: '0.113.0', flavor: 'normal' });
const childEnvironment = {
  ...process.env,
  XDG_RUNTIME_DIR: displayDirectory,
  WAYLAND_DISPLAY: waylandSocket,
  SDL_VIDEODRIVER: 'wayland',
  SDL_AUDIO_DRIVER: 'dummy',
  ...(process.env.XEM_LAVAPIPE_ICD ? { VK_ICD_FILENAMES: process.env.XEM_LAVAPIPE_ICD } : {}),
};
const appArguments = [
  '--disable-gpu',
  '--enable-logging=stderr',
  runDirectory,
  `--studio-live-test=${configPath}`,
];
let weston;
let westonDiagnostics = '';
try {
  if (process.platform === 'linux') {
    weston = spawn('weston', [
      '--backend=headless', '--renderer=pixman', '--width=1280', '--height=800',
      `--socket=${waylandSocket}`, '--idle-time=0',
    ], { env: childEnvironment, stdio: ['ignore', 'ignore', 'pipe'] });
    weston.stderr.setEncoding('utf8');
    weston.stderr.on('data', text => { westonDiagnostics = (westonDiagnostics + text).slice(-16_384); });
    const socketPath = join(displayDirectory, waylandSocket);
    const started = Date.now();
    while (!(await stat(socketPath).then(() => true, () => false))) {
      if (weston.exitCode !== null) throw new Error(`Test-owned Weston exited with ${weston.exitCode}${westonDiagnostics ? `: ${westonDiagnostics}` : ''}`);
      if (Date.now() - started > 20_000) throw new Error('Test-owned Weston did not create its Wayland socket in 20 seconds');
      await new Promise(resolvePromise => setTimeout(resolvePromise, 50));
    }
  }

  await new Promise((resolvePromise, rejectPromise) => {
    const command = process.platform === 'linux' ? 'xvfb-run' : binary;
    const args = process.platform === 'linux' ? ['--auto-servernum', binary, ...appArguments] : appArguments;
    const child = spawn(command, args, { cwd: studioRoot, env: childEnvironment, detached: process.platform === 'linux', stdio: 'inherit' });
    let timedOut = false;
    const timeout = setTimeout(() => {
      timedOut = true;
      if (process.platform === 'linux' && child.pid) process.kill(-child.pid, 'SIGTERM');
      else child.kill('SIGTERM');
      rejectPromise(new Error('NW live game smoke exceeded its 180-second limit'));
    }, 180_000);
    child.once('error', error => { clearTimeout(timeout); rejectPromise(error); });
    child.once('exit', (code, signal) => {
      clearTimeout(timeout);
      if (process.platform === 'linux' && child.pid && !timedOut) {
        try { process.kill(-child.pid, 'SIGTERM'); } catch (error) {
          if (!(error instanceof Error && 'code' in error && error.code === 'ESRCH')) throw error;
        }
      }
      if (timedOut) return;
      if (signal) rejectPromise(new Error(`NW live game smoke exited on ${signal}`));
      else if (code !== 0) rejectPromise(new Error(`NW live game smoke exited with code ${code}`));
      else resolvePromise();
    });
  });
} finally {
  if (weston && weston.exitCode === null && weston.signalCode === null) {
    weston.kill('SIGTERM');
    await new Promise(resolvePromise => {
      const timer = setTimeout(resolvePromise, 2_000);
      weston.once('exit', () => { clearTimeout(timer); resolvePromise(); });
    });
  }
}

const result = JSON.parse(await readFile(outputPath, 'utf8'));
assert.equal('error' in result, false, JSON.stringify(result.error ?? {}));
assert.equal(result.connected, true);
assert.equal(result.kind, 'native');
assert.match(result.module, /^[a-f0-9]{64}$/);
assert.equal(result.restored, true);
assert.equal(result.restoredValue, result.oldValue);
assert.notEqual(result.writtenValue, result.oldValue);
assert.equal(result.snapshotCaptured, true);
console.log(`NW live game smoke passed: ${JSON.stringify({ kind: result.kind, module: result.module, before: result.before, afterStep: result.afterStep, oldValue: result.oldValue, writtenValue: result.writtenValue, restoredValue: result.restoredValue, snapshotCaptured: result.snapshotCaptured })}`);
