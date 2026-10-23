import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { X509Certificate } from 'node:crypto';
import { get as httpsGet } from 'node:https';
import { copyFile, mkdtemp, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { studioTlsDirectory } from '../../scripts/tls-directory.js';

const scriptDirectory = dirname(fileURLToPath(import.meta.url));
const studioRoot = resolve(scriptDirectory, '../..');
const repositoryRoot = resolve(studioRoot, '../..');
const temporaryRoot = resolve(repositoryRoot, 'tmp/studio');
const devMode = process.argv.includes('--dev');
const artifactSuffix = process.env.XEM_NW_SMOKE_SUFFIX ? `-${process.env.XEM_NW_SMOKE_SUFFIX}` : '';
await mkdir(temporaryRoot, { recursive: true });
const runDirectory = await mkdtemp(join(temporaryRoot, 'nw-smoke-'));
const fixtureDirectory = join(runDirectory, 'fixture');
const outputPath = join(runDirectory, 'smoke-result.json');
const preservedResult = join(temporaryRoot, devMode ? `nw-dev${artifactSuffix}-smoke-result.json` : `nw${artifactSuffix}-smoke-result.json`);
const devManifestPath = join(temporaryRoot, 'nw-launch', 'package.json');
const preservedDevManifest = join(temporaryRoot, `nw-dev-package${artifactSuffix}.json`);
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
const launcherFlags = devMode ? ['--dev'] : [];
const command = process.platform === 'linux' ? 'xvfb-run' : process.execPath;
const args = process.platform === 'linux'
  ? ['--auto-servernum', process.execPath, launcher, ...launcherFlags, ...smokeFlags]
  : [launcher, ...launcherFlags, ...smokeFlags];

async function assertDevServerTls(selectedCertificate) {
  const certificatePath = join(studioTlsDirectory(), 'cert.pem');
  const certificatePem = await readFile(certificatePath);
  await new Promise((resolvePromise, rejectPromise) => {
    const request = httpsGet('https://localhost:5176/', {
      ca: certificatePem,
      rejectUnauthorized: true,
      servername: 'localhost',
    }, (response) => {
      const peerCertificate = new X509Certificate(response.socket.getPeerCertificate().raw);
      try {
        assert.equal(response.statusCode, 200);
        assert.equal(response.socket.authorized, true);
        assert.equal(response.headers['strict-transport-security'], undefined);
        assert.equal(peerCertificate.fingerprint256, selectedCertificate.fingerprint256);
      } catch (error) {
        response.resume();
        rejectPromise(error);
        return;
      }
      response.resume();
      response.once('end', resolvePromise);
    });
    request.setTimeout(5_000, () => request.destroy(new Error('TLS development server check timed out')));
    request.once('error', rejectPromise);
  });
}

try {
  if (devMode) {
    const selectedCertificate = new X509Certificate(await readFile(join(studioTlsDirectory(), 'cert.pem')));
    await assertDevServerTls(selectedCertificate);
  }
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
  if (devMode) {
    const temporaryManifest = JSON.parse(await readFile(join(temporaryRoot, 'nw-launch', 'package.json'), 'utf8'));
    const packagedManifest = JSON.parse(await readFile(join(studioRoot, 'package.json'), 'utf8'));
    assert.equal(temporaryManifest.main, 'https://localhost:5176/');
    assert.equal(temporaryManifest['node-remote'], 'https://localhost:5176/*');
    const trustAnchors = temporaryManifest.additional_trust_anchors;
    assert.ok(Array.isArray(trustAnchors));
    assert.equal(trustAnchors.length, 1);
    const selectedCertificate = new X509Certificate(await readFile(join(studioTlsDirectory(), 'cert.pem')));
    const configuredAnchor = new X509Certificate(trustAnchors[0]);
    assert.equal(configuredAnchor.fingerprint256, selectedCertificate.fingerprint256);
    assert.equal(configuredAnchor.raw.toString('hex'), selectedCertificate.raw.toString('hex'));
    assert.equal(Object.hasOwn(packagedManifest, 'node-remote'), false);
    assert.equal(Object.hasOwn(packagedManifest, 'additional_trust_anchors'), false);
  }
  assert.deepEqual(result.version, {
    version: '0.5.0',
    operations: [
      'version', 'echo', 'field.decode', 'field.scene.decode', 'field.encode', 'disc.open', 'disc.poll',
      'disc.missing', 'disc.close', 'disc.maps', 'field.map_events', 'field.map_bundle',
      'field.structure', 'names.validate', 'field.lanes', 'disc.cue', 'package.build',
      'package.inspect', 'package.bundle', 'field.draft.open', 'field.templates', 'field.instruction',
      'field.draft.edit', 'field.draft.undo', 'field.draft.redo', 'field.draft.compile',
      'field.draft.export', 'field.draft.import', 'field.draft.close', 'field.draft.snapshot',
      'field.draft.repack',
    ],
  });
  assert.deepEqual(result.versionResponse, { ok: result.version });
  assert.equal(result.adapter, 'node');
  assert.equal(result.windowControls, true);
  assert.deepEqual(result.entries, [
    { name: 'scripts', kind: 'directory' },
    { name: 'README.txt', kind: 'file' },
  ]);
  assert.deepEqual(result.echo, result.entries);
  assert.deepEqual(result.echoResponse, { ok: result.entries });
  assert.equal('error' in result, false);
  console.log(`NW ${devMode ? 'dev ' : ''}smoke passed: ${JSON.stringify(result)}`);
} finally {
  if (devMode) {
    try {
      await copyFile(devManifestPath, preservedDevManifest);
    } catch (error) {
      if (!(error instanceof Error && 'code' in error && error.code === 'ENOENT')) throw error;
    }
  }
  await rm(runDirectory, { recursive: true, force: true });
}
