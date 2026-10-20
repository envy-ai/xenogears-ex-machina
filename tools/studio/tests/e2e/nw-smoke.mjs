import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { copyFile, mkdtemp, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptDirectory = dirname(fileURLToPath(import.meta.url));
const studioRoot = resolve(scriptDirectory, '../..');
const repositoryRoot = resolve(studioRoot, '../..');
const temporaryRoot = resolve(repositoryRoot, 'tmp/studio');
await mkdir(temporaryRoot, { recursive: true });
const runDirectory = await mkdtemp(join(temporaryRoot, 'nw-smoke-'));
const fixtureDirectory = join(runDirectory, 'fixture');
const outputPath = join(runDirectory, 'smoke-result.json');
const preservedResult = join(temporaryRoot, 'nw-smoke-result.json');
await mkdir(join(fixtureDirectory, 'scripts'), { recursive: true });
await writeFile(join(fixtureDirectory, 'README.txt'), 'XEM Studio NW smoke fixture');
await writeFile(join(fixtureDirectory, 'scripts', 'intro.txt'), 'field script fixture');

const launcher = resolve(studioRoot, 'scripts/desktop.mjs');
const smokeFlags = [
  '--disable-gpu',
  '--enable-logging=stderr',
  `--studio-smoke=${fixtureDirectory}`,
  `--studio-smoke-output=${outputPath}`,
];
const command = process.platform === 'linux' ? 'xvfb-run' : process.execPath;
const args = process.platform === 'linux'
  ? ['--auto-servernum', process.execPath, launcher, ...smokeFlags]
  : [launcher, ...smokeFlags];

try {
  await new Promise((resolvePromise, rejectPromise) => {
    const child = spawn(command, args, {
      cwd: studioRoot,
      detached: process.platform === 'linux',
      stdio: 'inherit',
    });
    const timeout = setTimeout(() => {
      if (process.platform === 'linux' && child.pid) process.kill(-child.pid, 'SIGTERM');
      else child.kill('SIGTERM');
      rejectPromise(new Error('NW.js smoke did not finish within 90 seconds'));
    }, 90_000);
    child.once('error', (error) => {
      clearTimeout(timeout);
      rejectPromise(error);
    });
    child.once('exit', (code, signal) => {
      clearTimeout(timeout);
      if (signal) rejectPromise(new Error(`NW.js smoke exited on ${signal}`));
      else if (code !== 0) rejectPromise(new Error(`NW.js smoke exited with code ${code}`));
      else resolvePromise();
    });
  });

  const result = JSON.parse(await readFile(outputPath, 'utf8'));
  await copyFile(outputPath, preservedResult);
  assert.deepEqual(result.version, {
    version: '0.5.0',
    operations: ['version', 'echo'],
  });
  assert.equal(result.adapter, 'node');
  assert.equal(result.windowControls, true);
  assert.deepEqual(result.entries, [
    { name: 'scripts', kind: 'directory' },
    { name: 'README.txt', kind: 'file' },
  ]);
  assert.deepEqual(result.echo, result.entries);
  assert.equal('error' in result, false);
  console.log(`NW smoke passed: ${JSON.stringify(result)}`);
} finally {
  await rm(runDirectory, { recursive: true, force: true });
}
