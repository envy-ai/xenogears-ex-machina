import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { createHash, X509Certificate } from 'node:crypto';
import { get as httpsGet } from 'node:https';
import { copyFile, cp, mkdtemp, mkdir, readFile, readdir, rm, symlink, writeFile } from 'node:fs/promises';
import { constants as fsConstants, existsSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { studioTlsDirectory } from '../../scripts/tls-directory.js';

const scriptDirectory = dirname(fileURLToPath(import.meta.url));
const studioRoot = resolve(scriptDirectory, '../..');
const repositoryRoot = resolve(studioRoot, '../..');
const temporaryRoot = resolve(repositoryRoot, 'tmp/studio');
const devMode = process.argv.includes('--dev');
const sceneMode = process.argv.includes('--scene');
const sceneVulkanMode = process.argv.includes('--scene-vulkan');
const builtMode = process.argv.includes('--built');
const keepArtifacts = process.argv.includes('--keep-artifacts');
assert.ok(Number(devMode) + Number(sceneMode) + Number(builtMode) <= 1, 'choose at most one of --dev, --built, or --scene');
assert.ok(!sceneVulkanMode || sceneMode, '--scene-vulkan is valid only with --scene');
assert.ok(!(sceneMode || builtMode) || keepArtifacts, 'M7 NW modes require --keep-artifacts');
const artifactSuffix = process.env.XEM_NW_SMOKE_SUFFIX ? `-${process.env.XEM_NW_SMOKE_SUFFIX}` : '';
const artifactParent = sceneMode || builtMode || keepArtifacts ? resolve(temporaryRoot, 'milestone-7') : temporaryRoot;
await mkdir(artifactParent, { recursive: true });
const runDirectory = await mkdtemp(join(artifactParent, `${sceneMode ? 'nw-scene-smoke' : builtMode ? 'nw-built-smoke' : 'nw-smoke'}${artifactSuffix}-`));
const fixtureDirectory = join(runDirectory, 'fixture');
const outputPath = join(runDirectory, 'smoke-result.json');
const preservedResult = join(temporaryRoot, devMode ? `nw-dev${artifactSuffix}-smoke-result.json` : `nw${artifactSuffix}-smoke-result.json`);
const devManifestPath = join(temporaryRoot, 'nw-launch', 'package.json');
const preservedDevManifest = join(temporaryRoot, `nw-dev-package${artifactSuffix}.json`);
await mkdir(join(fixtureDirectory, 'scripts'), { recursive: true });
await writeFile(join(fixtureDirectory, 'README.txt'), 'XEM Studio NW smoke fixture');
await writeFile(join(fixtureDirectory, 'scripts', 'intro.txt'), 'field script fixture');

const launcher = resolve(studioRoot, 'scripts/desktop.mjs');
let selectedLauncher = launcher;
let smokeFlags;
if (sceneMode || builtMode) {
  const dist = resolve(requiredPath(sceneMode ? 'XEM_NW_SMOKE_TEST_DIST' : 'XEM_NW_SMOKE_PRODUCTION_DIST'));
  const m7Root = resolve(temporaryRoot, 'milestone-7');
  assert.ok(dist.startsWith(m7Root + '/'), 'NW test/production dist stays inside ignored M7 storage');
  assert.ok(existsSync(join(dist, 'index.html')), 'NW dist has an index');
  const builtApp = join(runDirectory, sceneMode ? 'scene-test-app' : 'built-app');
  const scripts = join(builtApp, 'scripts');
  await mkdir(builtApp, { recursive: false });
  await mkdir(scripts, { recursive: false });
  const packageBytes = await readFile(join(studioRoot, 'package.json'));
  const packageManifest = JSON.parse(packageBytes);
  assert.equal(packageManifest.main, 'dist/index.html');
  assert.equal(Object.hasOwn(packageManifest, 'node-remote'), false);
  assert.equal(Object.hasOwn(packageManifest, 'additional_trust_anchors'), false);
  await copyFile(join(studioRoot, 'package.json'), join(builtApp, 'package.json'), fsConstants.COPYFILE_EXCL);
  await cp(dist, join(builtApp, 'dist'), { recursive: true, errorOnExist: true, force: false });
  await symlink(join(studioRoot, 'node_modules'), join(builtApp, 'node_modules'), 'dir');
  await copyFile(launcher, join(scripts, 'desktop.mjs'), fsConstants.COPYFILE_EXCL);
  await copyFile(join(studioRoot, 'scripts/tls-directory.js'), join(scripts, 'tls-directory.js'), fsConstants.COPYFILE_EXCL);
  assert.ok((await readFile(join(builtApp, 'package.json'))).equals(packageBytes), 'isolated NW app keeps the canonical package manifest byte-for-byte');
  assert.ok((await readFile(join(scripts, 'desktop.mjs'))).equals(await readFile(launcher)), 'isolated NW app uses the canonical desktop launcher byte-for-byte');
  assert.ok((await readFile(join(scripts, 'tls-directory.js'))).equals(await readFile(join(studioRoot, 'scripts/tls-directory.js'))), 'isolated NW app uses the canonical TLS helper byte-for-byte');
  const sourceAssets = await assetInventory(dist);
  const appAssets = await assetInventory(join(builtApp, 'dist'));
  assert.deepEqual(appAssets, sourceAssets, 'all isolated NW assets are copied byte-for-byte into the app root');
  const bundles = (await Promise.all(sourceAssets.filter(asset => asset.path.endsWith('.js'))
    .map(asset => readFile(join(dist, asset.path), 'utf8')))).join('\n');
  const threeNotice = sourceAssets.find(asset => asset.path === 'licenses/three/LICENSE');
  assert.ok(threeNotice, 'NW app carries the Three.js MIT notice');
  assert.equal(threeNotice.sha256, '8b378ebe60e2fe500158cb0ac71cb5e8b7d92953c2abcc63a0eb90499653b5bc',
    'NW app carries the exact pinned Three.js license text');
  if (sceneMode) {
    assert.ok(bundles.includes('__xemStudioTest'), 'isolated Scene app contains its test-only command hook');
  } else {
    assert.equal(bundles.includes('__xemStudioTest'), false, 'production NW app excludes the test-only command hook');
    assert.equal(bundles.includes('VITE_XEM_STUDIO_TEST'), false, 'production NW app excludes the test build flag');
  }
  await writeFile(join(runDirectory, 'nw-app-identity.json'), `${JSON.stringify({
    mode: sceneMode ? 'isolated-test' : 'production', package_manifest_sha256: sha256(packageBytes),
    desktop_launcher_sha256: sha256(await readFile(join(scripts, 'desktop.mjs'))),
    tls_helper_sha256: sha256(await readFile(join(scripts, 'tls-directory.js'))),
    assets: sourceAssets, every_asset_copied_byte_identically: true,
  }, null, 2)}\n`, { flag: 'wx' });
  selectedLauncher = join(scripts, 'desktop.mjs');
  if (!sceneMode) {
    smokeFlags = [
      '--disable-gpu', '--enable-logging=stderr',
      `--studio-smoke=${fixtureDirectory}`, `--studio-smoke-output=${outputPath}`,
    ];
  }
}
if (sceneMode) {
  const disc1 = requiredPath('XEM_STUDIO_DISC1');
  const disc2 = requiredPath('XEM_STUDIO_DISC2');
  const seedProject = requiredPath('XEM_STUDIO_M7_SAVED_PROJECT');
  assert.equal(seedProject, resolve(repositoryRoot, 'tmp/studio/milestone-7/checkpoint-86/private-stage-b/field-map-23.json'), 'Scene NW smoke uses the exact reviewed M7 saved project');
  const folder = join(runDirectory, 'project');
  const sceneConfigPath = join(runDirectory, 'scene-config.json');
  const sceneConfig = {
    paths: [disc1, disc2], output: outputPath, disc: 1, map: 23,
    scene: {
      folder, seedProject, seedProjectSha256: '3cffb561e775730c7152a83d524729e02bdbcaeff13a57007763752a37581f9a',
      actorId: 'descriptor:3', placementNode: 'pc:1076', sourceHeaderX: 29, initialX: 61, editedX: 62,
      expectedSourceSha256: '0f0a131acdf052cae070a64912fc09eb00e0b5e6a81f9d8cb4719d6416213add',
    },
  };
  await writeFile(sceneConfigPath, `${JSON.stringify(sceneConfig, null, 2)}\n`, { flag: 'wx' });
  smokeFlags = [
    ...(sceneVulkanMode ? ['--use-gl=angle', '--use-angle=vulkan'] : []),
    `--studio-events-test=${sceneConfigPath}`,
  ];
} else if (!builtMode) {
  smokeFlags = [
    '--disable-gpu',
    '--enable-logging=stderr',
    `--studio-smoke=${fixtureDirectory}`,
    `--studio-smoke-output=${outputPath}`,
  ];
}
const launcherFlags = devMode ? ['--dev'] : [];
const command = process.platform === 'linux' ? 'xvfb-run' : process.execPath;
const args = process.platform === 'linux'
  ? ['--auto-servernum', process.execPath, selectedLauncher, ...launcherFlags, ...smokeFlags]
  : [selectedLauncher, ...launcherFlags, ...smokeFlags];

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
    const packageManifest = JSON.parse(await readFile(join(studioRoot, 'package.json'), 'utf8'));
    const expectedManifest = Buffer.from(JSON.stringify({
      name: packageManifest.name,
      version: packageManifest.version,
      main: 'https://localhost:5176/',
      'node-remote': 'https://localhost:5176/*',
      additional_trust_anchors: [selectedCertificate.toString()],
      window: packageManifest.window,
    }, null, 2));
    await writeFile(join(runDirectory, 'expected-dev-package.json'), expectedManifest, { flag: 'wx' });
    try {
      const existingManifest = await readFile(devManifestPath);
      assert.ok(existingManifest.equals(expectedManifest), 'existing NW development manifest must match the canonical public-cert launcher output byte-for-byte');
      await writeFile(join(runDirectory, 'preexisting-dev-package.json'), existingManifest, { flag: 'wx' });
      await writeFile(join(runDirectory, 'preexisting-dev-package.sha256'), `${sha256(existingManifest)}\n`, { flag: 'wx' });
    } catch (error) {
      if (!(error instanceof Error && 'code' in error && error.code === 'ENOENT')) throw error;
      await writeFile(join(runDirectory, 'preexisting-dev-package.absent'), 'canonical launcher may create the expected manifest\n', { flag: 'wx' });
    }
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
      rejectPromise(new Error(`NW.js ${sceneMode ? 'Scene ' : ''}smoke did not finish within ${sceneMode ? '600 seconds' : '90 seconds'}`));
    }, sceneMode ? 600_000 : 90_000);
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
  if (result.error) {
    const detail = typeof result.error === 'object' ? result.error : { message: String(result.error) };
    const error = new Error(`NW smoke driver failed: ${detail.message ?? 'no diagnostic message'}`);
    if (typeof detail.stack === 'string') error.stack = detail.stack;
    throw error;
  }
  if (sceneMode) {
    assert.equal(result.kind, 'scene');
    assert.equal(result.adapter, 'node');
    assert.equal(result.disc, 1);
    assert.equal(result.map, 23);
    assert.equal(result.source_sha256, '0f0a131acdf052cae070a64912fc09eb00e0b5e6a81f9d8cb4719d6416213add');
    assert.deepEqual(result.source_identity, {
      disc: 1, map: 23, slot: 652, lba: 124528, indexed_size: 110052,
      boot_sha256: 'dc0b2dd786203d4cce5927c5a3fc85a18f39a3f7406078860076ebb0bbae7119',
    });
    assert.equal(result.module_sha256, '613c3a747e1c07e0e0e2632710eae1f873e4b14dda880cd6ce1c556b31c152a8');
    assert.equal(result.actor_id, 'descriptor:3');
    assert.equal(result.placement_node, 'pc:1076');
    assert.equal(result.source_header_x, 29);
    assert.equal(result.saved_x, 62);
    assert.equal(result.owner_changed_on_reload, true);
    assert.ok(result.compiled_bundle_sha256 && /^[0-9a-f]{64}$/.test(result.compiled_bundle_sha256));
    assert.ok(result.canvas.width > 0 && result.canvas.height > 0, 'Scene canvas is rendered with normal NW WebGL');
    if (sceneVulkanMode) assert.ok(result.canvas.renderer && result.canvas.version, 'Scene reports its live WebGL renderer and version');
    assert.deepEqual(result.filesystem, ['field-map-23.json', 'names.json']);
    console.log(`NW Scene smoke passed: ${JSON.stringify(result)}`);
  } else {
    if (!keepArtifacts) await copyFile(outputPath, preservedResult);
  }
  if (devMode) {
    const expectedManifest = await readFile(join(runDirectory, 'expected-dev-package.json'));
    const actualManifest = await readFile(devManifestPath);
    assert.ok(actualManifest.equals(expectedManifest), 'canonical NW development launcher preserved the pre-existing byte-identical manifest');
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
  if (!sceneMode) {
    assert.deepEqual(result.version, {
    version: '0.5.0',
    operations: [
      'version', 'echo', 'field.decode', 'field.scene.decode', 'field.scene.draft', 'field.encode', 'disc.open', 'disc.poll',
      'disc.missing', 'disc.close', 'disc.maps', 'field.map_events', 'field.map_bundle', 'field.map_textures',
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
  }
} finally {
  if (devMode && !keepArtifacts) {
    try {
      await copyFile(devManifestPath, preservedDevManifest);
    } catch (error) {
      if (!(error instanceof Error && 'code' in error && error.code === 'ENOENT')) throw error;
    }
  }
  if (keepArtifacts) console.log(`NW smoke artifacts retained: ${runDirectory}`);
  else await rm(runDirectory, { recursive: true, force: true });
}

function requiredPath(name) {
  const value = process.env[name];
  assert.ok(value, `${name} must name an explicit local input`);
  return resolve(value);
}

function sha256(value) { return createHash('sha256').update(value).digest('hex'); }

async function assetInventory(root) {
  const entries = [];
  async function visit(directory, prefix = '') {
    for (const entry of (await readdir(directory, { withFileTypes: true })).sort((left, right) => left.name.localeCompare(right.name))) {
      const relative = join(prefix, entry.name).split(/[\\/]/).join('/');
      const path = join(directory, entry.name);
      if (entry.isDirectory()) await visit(path, relative);
      else if (entry.isFile()) {
        const bytes = await readFile(path);
        entries.push({ path: relative, size: bytes.length, sha256: sha256(bytes) });
      } else assert.fail(`unexpected non-file NW test asset: ${relative}`);
    }
  }
  await visit(root);
  return entries;
}
