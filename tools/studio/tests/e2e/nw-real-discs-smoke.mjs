import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { cp, mkdir, readFile, stat, writeFile } from 'node:fs/promises';
import { findpath } from 'nw';
import { join, resolve } from 'node:path';

const repositoryRoot = resolve(process.cwd(), '../..');
const artifactRoot = resolve(repositoryRoot, 'tmp/studio/milestone-3-ui-20261020');
const testDist = resolve(process.env.XEM_STUDIO_TEST_DIST ?? join(artifactRoot, 'test-dist'));
assert.ok(testDist.startsWith(`${artifactRoot}/`), 'test dist must remain under the fresh ignored M3 UI artifact directory');
await readFile(join(testDist, 'index.html'));
const disc1 = process.env.XEM_STUDIO_DISC1_PATH;
const disc2 = process.env.XEM_STUDIO_DISC2_PATH;
const extractRoot = process.env.XEM_STUDIO_EXTRACT_ROOT;
assert.ok(extractRoot, 'Set XEM_STUDIO_EXTRACT_ROOT to the authorized local extraction root.');
assert.ok(disc1 && disc2, 'Set both local disc image paths before running this smoke.');
const sourceDirectories = await Promise.all([stat(join(extractRoot, 'extract')), stat(join(extractRoot, 'discs'))]);
assert.ok(sourceDirectories.every(item => item.isDirectory()), 'Extraction root must contain the existing extract/ and discs/ directories.');
const discStats = await Promise.all([stat(disc1), stat(disc2)]);
assert.ok(discStats.every(item => item.isFile() && item.size > 0), 'Both selected disc paths must be nonempty regular files.');

const runDirectory = join(artifactRoot, `nw-real-discs-${process.pid}-${Date.now()}`);
await mkdir(runDirectory, { recursive: false });
const outputPath = join(runDirectory, 'result.json');
const configPath = join(runDirectory, 'config.json');
await writeFile(configPath, JSON.stringify({ paths: [disc1, disc2], output: outputPath, disc: 1, map: 2, measure: true }) + '\n', { flag: 'wx' });
await cp(testDist, join(runDirectory, 'dist'), { recursive: true, errorOnExist: true });
const manifest = {
  name: 'xem-studio-real-discs-test',
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
  const child = spawn(command, args, { cwd: repositoryRoot, detached: process.platform === 'linux', stdio: 'inherit' });
  const timeout = setTimeout(() => {
    if (process.platform === 'linux' && child.pid) process.kill(-child.pid, 'SIGTERM');
    else child.kill('SIGTERM');
    rejectPromise(new Error('NW real-disc smoke exceeded its 20 minute limit'));
  }, 20 * 60_000);
  child.once('error', error => { clearTimeout(timeout); rejectPromise(error); });
  child.once('exit', (code, signal) => {
    clearTimeout(timeout);
    if (signal) rejectPromise(new Error(`NW real-disc smoke exited on ${signal}`));
    else if (code !== 0) rejectPromise(new Error(`NW real-disc smoke exited with code ${code}`));
    else resolvePromise();
  });
});

const result = JSON.parse(await readFile(outputPath, 'utf8'));
assert.equal('error' in result, false, JSON.stringify(result.error ?? {}));
assert.deepEqual(result.discs.map(item => item.disc), [1, 2]);
assert.ok(result.discs.every(item => item.mapCount > 0));
assert.equal(result.map, 2);
assert.equal(result.instructions, 2358, 'disc 1 map 2 must match the independently cross-checked instruction count');
assert.ok(result.mapRows.some(item => item.map === 2 && !item.placeholder), 'map 2 should be a real map');
assert.deepEqual(result.renderTimes.map(item => item.view), ['list', 'graph', 'lanes']);
assert.ok(result.renderTimes.every(item => item.map === 2 && item.instructions === 2358));
console.log(`NW both-disc map 2 timings: ${JSON.stringify({ discs: result.discs, map: result.map, instructions: result.instructions, renderTimes: result.renderTimes })}`);
