import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { cp, mkdir, readFile, writeFile } from 'node:fs/promises';
import { findpath } from 'nw';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptDirectory = dirname(fileURLToPath(import.meta.url));
const studioRoot = resolve(scriptDirectory, '../..');
const repositoryRoot = resolve(studioRoot, '../..');
const artifactRoot = resolve(repositoryRoot, 'tmp/studio/milestone-3-ui-20261020');
const testDist = resolve(process.env.XEM_STUDIO_TEST_DIST ?? join(artifactRoot, 'test-dist'));
assert.ok(testDist.startsWith(`${artifactRoot}/`), 'test dist must remain under the fresh ignored M3 UI artifact directory');
await readFile(join(testDist, 'index.html'));

const syntheticImage = process.env.XEM_STUDIO_SYNTHETIC_DISC_IMAGE;
assert.ok(syntheticImage, 'Set XEM_STUDIO_SYNTHETIC_DISC_IMAGE to the ignored synthetic disc fixture.');
const fixturePath = resolve(syntheticImage);
assert.ok(fixturePath.startsWith(`${repositoryRoot}/tmp/studio/`), 'synthetic disc fixture must be an ignored local artifact');
await readFile(fixturePath);

const runDirectory = join(artifactRoot, `nw-events-${process.pid}-${Date.now()}`);
await mkdir(runDirectory, { recursive: false });
const outputPath = join(runDirectory, 'result.json');
const configPath = join(runDirectory, 'config.json');
await writeFile(configPath, JSON.stringify({ paths: [fixturePath], output: outputPath, map: 0, measure: false }) + '\n', { flag: 'wx' });
await cp(testDist, join(runDirectory, 'dist'), { recursive: true, errorOnExist: true });
const manifest = {
  name: 'xem-studio-events-test',
  version: '0.5.0',
  main: 'dist/index.html',
  window: { width: 1440, height: 900, min_width: 1180, min_height: 720, frame: false },
};
await writeFile(join(runDirectory, 'package.json'), `${JSON.stringify(manifest, null, 2)}\n`, { flag: 'wx' });

const binary = await findpath('nwjs', { version: '0.113.0', flavor: 'normal' });
const smokeArgs = ['--disable-gpu', '--enable-logging=stderr', runDirectory, '--studio-events-test', `--studio-events-test=${configPath}`];
const command = process.platform === 'linux' ? 'xvfb-run' : binary;
const args = process.platform === 'linux' ? ['--auto-servernum', binary, ...smokeArgs] : smokeArgs;
await new Promise((resolvePromise, rejectPromise) => {
  const child = spawn(command, args, { cwd: studioRoot, detached: process.platform === 'linux', stdio: 'inherit' });
  const timeout = setTimeout(() => {
    if (process.platform === 'linux' && child.pid) process.kill(-child.pid, 'SIGTERM');
    else child.kill('SIGTERM');
    rejectPromise(new Error('NW synthetic disc smoke exceeded its 120 second limit'));
  }, 120_000);
  child.once('error', error => { clearTimeout(timeout); rejectPromise(error); });
  child.once('exit', (code, signal) => {
    clearTimeout(timeout);
    if (signal) rejectPromise(new Error(`NW synthetic disc smoke exited on ${signal}`));
    else if (code !== 0) rejectPromise(new Error(`NW synthetic disc smoke exited with code ${code}`));
    else resolvePromise();
  });
});

const result = JSON.parse(await readFile(outputPath, 'utf8'));
assert.equal('error' in result, false, JSON.stringify(result.error ?? {}));
assert.equal(result.discs.length, 1);
assert.deepEqual(result.discs[0], { id: result.discs[0].id, disc: null, mapCount: 2, placeholders: 1 });
assert.equal(result.map, 0);
assert.deepEqual(result.mapRows, [{ map: 0, placeholder: false }, { map: 1, placeholder: true }]);
assert.deepEqual(result.visibleViews, ['Command list', 'Flow graph', 'Actor lanes']);
console.log(`NW synthetic disc smoke passed: ${JSON.stringify({ mapCount: result.discs[0].mapCount, placeholders: result.discs[0].placeholders, mapRows: result.mapRows, selectedMap: result.map })}`);
