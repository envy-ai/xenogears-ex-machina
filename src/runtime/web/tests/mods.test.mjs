// Gated M5 acceptance against the user's real disc, game module, and locally
// built delta package. Synthetic overlay parity is covered separately; this
// test proves the shared package bytes traverse both production host adapters.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { createInterface } from 'node:readline';
import { dirname, join, resolve, sep } from 'node:path';
import { after, before, describe, test } from 'node:test';
import { HEADLESS, NO_WEBGPU, ROOT, WEB, decodePng, pixelStats, start } from './support.mjs';

const enabled = process.env.XEM_STUDIO_M5_REAL === '1';
const m5Root = resolve(ROOT, 'tmp/studio/milestone-5');
const fixtureRoot = process.env.XEM_STUDIO_M5_FIXTURE_ROOT && resolve(process.env.XEM_STUDIO_M5_FIXTURE_ROOT);
const packageSetPath = process.env.XEM_STUDIO_M5_SET && resolve(process.env.XEM_STUDIO_M5_SET);
const discPath = process.env.XEM_STUDIO_M5_DISC1 && resolve(process.env.XEM_STUDIO_M5_DISC1);
const artifactRoot = process.env.XEM_STUDIO_M5_ARTIFACT_ROOT && resolve(process.env.XEM_STUDIO_M5_ARTIFACT_ROOT);
const nativeClients = [];
describe('real-disc M5 package acceptance', { skip: !enabled && 'set XEM_STUDIO_M5_REAL=1 to enable private real-disc acceptance' }, () => {
  let browser;
  let page;
  let packageBytes;
  let expected;
  let originalExpected;
  let modifiedExpected;

  before(async () => {
    assert.ok(fixtureRoot && packageSetPath && discPath && artifactRoot,
      'set XEM_STUDIO_M5_FIXTURE_ROOT, XEM_STUDIO_M5_SET, XEM_STUDIO_M5_DISC1 and XEM_STUDIO_M5_ARTIFACT_ROOT');
    for (const path of [fixtureRoot, packageSetPath, artifactRoot]) {
      assert.ok(path === m5Root || path.startsWith(m5Root + sep), `private M5 path must remain under ${m5Root}: ${path}`);
    }
    assert.ok(existsSync(dirname(artifactRoot)), `artifact parent must already exist: ${dirname(artifactRoot)}`);
    assert.ok(artifactRoot !== fixtureRoot, 'acceptance output must use a separate fresh leaf');
    assert.ok(existsSync(join(fixtureRoot, 'evidence.json')), 'fixture generator evidence is required');
    assert.ok(existsSync(join(fixtureRoot, 'recipe.json')), 'fixture builder recipe is required');
    assert.ok(existsSync(packageSetPath), 'the built .xemmods package set is required');
    assert.ok(existsSync(discPath), 'the original disc 1 input is required');
    assert.ok(existsSync(HEADLESS), `native headless host is required: ${HEADLESS}`);
    assert.ok(existsSync(join(WEB, 'game/game.wasm')), `built browser game module is required in ${WEB}`);
    assert.ok(existsSync(join(WEB, 'game/schema.json')), `browser module schema is required in ${WEB}`);
    assert.ok(!existsSync(artifactRoot), `refusing to overwrite acceptance artifacts: ${artifactRoot}`);
    packageBytes = readFileSync(packageSetPath);
    const packageIdentity = packageSetIdentity(packageBytes);
    expected = JSON.parse(readFileSync(join(fixtureRoot, 'evidence.json'), 'utf8'));
    originalExpected = tokenBytes(expected.message_tokens.original);
    modifiedExpected = tokenBytes(expected.message_tokens.modified);
    assert.ok(Buffer.from(originalExpected).toString('hex') === expected.message_bytes.original_hex,
      'fixture token evidence reconstructs the original source bytes exactly');
    assert.ok(Buffer.from(modifiedExpected).toString('hex') === expected.message_bytes.modified_hex,
      'fixture token evidence reconstructs the authored result bytes exactly');
    assert.deepEqual(expected.file.only_modified_map_message_indices, [expected.component7.message_index]);
    assert.equal(expected.component7.message_capacity, 5, 'authored message has exactly five encoded bytes of storage');
    assert.equal(expected.component7.authored_text, 'MODE', 'fixture source identifies the exact authored text');
    const manifestFile = packageIdentity.manifest.files.find(file => file.disc === expected.file.disc && file.slot === 652);
    assert.ok(manifestFile, 'package manifest includes the authored map 23 slot');
    assert.equal(manifestFile.kind, 'delta', 'fixture remains a source-validated delta');
    assert.equal(manifestFile.result_size, expected.file.result_size);
    assert.equal(manifestFile.result_sha256, expected.file.result_sha256);
    mkdirSync(artifactRoot, { recursive: false });
    browser = await start({ args: NO_WEBGPU });
    page = await browser.page();
    await page.setInputFiles('#file', discPath);
    await page.waitForFunction(() => ['identified', 'failed'].includes(window.xem.status().disc?.state), null, { timeout: 120_000 });
    const disc = await page.evaluate(() => window.xem.status().disc);
    assert.equal(disc.state, 'identified');
    assert.equal(disc.identity.sha256, expected.disc_targets[0].sha256);
    assert.equal(disc.identity.serial, expected.disc_targets[0].serial);
  });

  after(async () => {
    const closeErrors = [];
    for (const client of nativeClients) {
      try { await client.close(); } catch (error) { closeErrors.push(error); }
    }
    if (browser) {
      try { await browser.close(); } catch (error) { closeErrors.push(error); }
    }
    if (closeErrors.length) throw new AggregateError(closeErrors, 'owned M5 browser/native children did not close cleanly');
  });

  test('identical package bytes render map 23 message 1 on both hosts and preserve M4 contracts', { timeout: 1_200_000 }, async () => {
    const packageDigest = sha256(packageBytes);
    const expectedPackageIdentity = packageSetIdentity(packageBytes).identity;
    const browserCaptures = {};
    const browserModes = {};
    for (const mode of ['absent', 'disabled', 'enabled']) {
      browserModes[mode] = await runBrowserRoute(page, mode, packageBytes, expected);
      await page.evaluate(() => new Promise(resolveFrame => requestAnimationFrame(() => requestAnimationFrame(resolveFrame))));
      const pngPath = join(artifactRoot, `browser-${mode}.png`);
      const png = await page.locator('canvas').screenshot({ path: pngPath });
      const textRegion = browserTextPixels(png, browserModes[mode].renderer, browserModes[mode].textMatch.geometry);
      assert.ok(textRegion.brightPixels > 0,
        `browser ${mode}: expected bright glyph pixels inside the active text area; got ${JSON.stringify(textRegion)}`);
      browserCaptures[mode] = { path: pngPath, sha256: sha256(png), pixels: pixelStats(png), textRegion };
    }

    const nativeCaptures = {};
    const nativeModes = {};
    for (const mode of ['absent', 'disabled', 'enabled']) {
      nativeModes[mode] = await runNativeRoute(mode, packageSetPath, discPath, artifactRoot, expected);
      const pngPath = join(artifactRoot, `native-${mode}.png`);
      const screenshot = await nativeModes[mode].client.command({ cmd: 'screenshot', path: pngPath });
      assert.ok(!screenshot.error, JSON.stringify(screenshot));
      const png = readFileSync(pngPath);
      const textRegion = nativeTextPixels(png, nativeModes[mode].display, nativeModes[mode].textMatch.geometry);
      assert.ok(textRegion.brightPixels > 0,
        `native ${mode}: expected bright glyph pixels inside the active text area; got ${JSON.stringify(textRegion)}`);
      nativeCaptures[mode] = { path: pngPath, sha256: sha256(png), pixels: pixelStats(png), dimensions: screenshot, textRegion };
    }

    for (const mode of ['absent', 'disabled']) {
      assert.equal(browserModes[mode].hash, browserModes.absent.hash, `browser ${mode} memory hash matches absent-package vanilla`);
      assert.equal(nativeModes[mode].hash, nativeModes.absent.hash, `native ${mode} memory hash matches absent-package vanilla`);
      assert.equal(browserModes[mode].hash, nativeModes[mode].hash, `${mode} memory hash agrees across hosts`);
      assert.ok(Buffer.from(browserModes[mode].textMatch.bytes).equals(Buffer.from(originalExpected)),
        `${mode} live window points at the original encoded message ${expected.component7.message_index}`);
      assert.ok(Buffer.from(nativeModes[mode].textMatch.bytes).equals(Buffer.from(originalExpected)),
        `${mode} native live window points at original message ${expected.component7.message_index}`);
    }
    assert.equal(browserModes.enabled.hash, nativeModes.enabled.hash, 'enabled package memory hash agrees across hosts');
    assert.deepEqual(browserModes.enabled.identity.packages, nativeModes.enabled.identity.packages, 'ordered package identities agree');
    assert.deepEqual(browserModes.enabled.identity.packages, [expectedPackageIdentity], 'runtime package identity matches exact package bytes');
    assert.ok(Buffer.from(browserModes.enabled.textMatch.bytes).equals(Buffer.from(modifiedExpected)),
      'browser active window memory matches the authored token bytes');
    assert.ok(Buffer.from(nativeModes.enabled.textMatch.bytes).equals(Buffer.from(modifiedExpected)),
      'native active window memory matches the authored token bytes');
    assert.deepEqual(browserModes.enabled.schema, nativeModes.enabled.schema, 'M4 schema contract is identical across hosts');
    assert.deepEqual(browserModes.enabled.status, nativeModes.enabled.status, 'ordered M4 runtime status is identical across hosts');
    assert.equal(browserModes.enabled.schema.schema.module_sha256, browserModes.enabled.identity.module);
    assert.equal(nativeModes.enabled.schema.schema.module_sha256, nativeModes.enabled.identity.module);
    assert.equal(browserModes.enabled.status.mode, 1, 'the real field route reached field mode');
    assert.equal(nativeModes.enabled.status.mode, 1, 'the native real field route reached field mode');
    assert.equal(browserModes.enabled.status.frames, 733);
    assert.equal(nativeModes.enabled.status.frames, 733);
    for (const captures of [browserCaptures, nativeCaptures]) {
      assert.ok(captures.absent.pixels.lit > 0.005, 'vanilla route produced a rendered game display');
      assert.ok(captures.enabled.pixels.lit > 0.005, 'modified route produced a rendered game display');
      assert.notEqual(captures.enabled.sha256, captures.absent.sha256, 'map 23 screenshot differs with the authored message');
      assert.equal(captures.disabled.sha256, captures.absent.sha256, 'disabled package screenshot matches vanilla');
    }

    const browserPersistence = await verifyBrowserM4(page, browserModes.enabled.hash);
    const nativePersistence = await verifyNativeM4(nativeModes.enabled.client, artifactRoot, nativeModes.enabled.hash);
    assert.deepEqual(
      { initial: browserPersistence.initial, changedHash: browserPersistence.changedHash,
        invalidError: browserPersistence.invalidError, invalidBefore: browserPersistence.invalidBefore,
        invalidAfter: browserPersistence.invalidAfter, restored: browserPersistence.restored },
      { initial: nativePersistence.initial, changedHash: nativePersistence.changedHash,
        invalidError: nativePersistence.invalidError, invalidBefore: nativePersistence.invalidBefore,
        invalidAfter: nativePersistence.invalidAfter, restored: nativePersistence.restored },
      'snapshot restore and rejected writes preserve identical M4 state',
    );
    const mismatch = await rejectBrowserForeignSnapshot(page, packageBytes);
    const nativeMismatch = await rejectNativeForeignSnapshot(nativeModes.disabled.client, artifactRoot);
    assert.match(mismatch.error, /identity|different|package/i);
    assert.match(nativeMismatch.error, /identity|different|package/i);
    assert.equal(mismatch.before, mismatch.after, 'browser foreign-package restore refuses before memory mutation');
    assert.equal(nativeMismatch.before, nativeMismatch.after, 'native foreign-package restore refuses before memory mutation');

    const evidence = {
      acceptance: 'real-disc M5 route through browser and xem-headless; native capture is the headless GPU display PNG, not an SDL window',
      packageSetSha256: packageDigest,
      moduleSha256: browserModes.enabled.identity.module,
      discIdentity: browserModes.enabled.identity.disc,
      fixture: expected,
      route: { start: 'boot_mode=0; frames=303', field: 'scenario mode=1, game_data.map=23; frames=430', finalFrames: 733 },
      browser: { modes: summarizeModes(browserModes), captures: browserCaptures, persistence: browserPersistence, mismatch },
      native: { modes: summarizeModes(nativeModes), captures: nativeCaptures, persistence: nativePersistence, mismatch: nativeMismatch },
      checks: {
        samePackageBytes: true,
        sameOrderedIdentityStatusAndSchema: true,
        liveWindowTextBytesMatchAuthoritativeFixture: true,
        visibleGlyphPixelsInsideActiveWindowTextArea: true,
        absentDisabledVanillaMemoryHashParity: true,
        absentDisabledScreenshotParity: true,
        snapshotRestoreAndInvalidWriteRefusal: true,
        foreignPackageSnapshotRefusedWithoutMutation: true,
      },
    };
    writeFileSync(join(artifactRoot, 'result.json'), `${JSON.stringify(evidence, null, 2)}\n`, { flag: 'wx' });
  });
});

function sha256(bytes) { return createHash('sha256').update(bytes).digest('hex'); }

function packageSetIdentity(setBytes) {
  assert.equal(setBytes.toString('ascii', 0, 8), 'XEMSET1\0', 'package set uses the versioned public container');
  assert.equal(setBytes.readUInt32LE(8), 1, 'real fixture contains exactly one ordered package');
  const packageLength = setBytes.readUInt32LE(12);
  assert.equal(setBytes.length, 16 + packageLength, 'package-set framing has no trailing data');
  const packageBytes = setBytes.subarray(16);
  assert.equal(packageBytes.toString('ascii', 0, 8), 'XEMMOD1\0', 'set member is a versioned package');
  const manifestLength = packageBytes.readUInt32LE(8);
  const manifest = JSON.parse(packageBytes.toString('utf8', 12, 12 + manifestLength));
  return { manifest, identity: `${manifest.id}@${sha256(packageBytes)}` };
}

function tokenBytes(tokens) {
  const output = [];
  for (const token of tokens) {
    const code = Number.parseInt(token.code, 16);
    if (token.mnemonic === 'glyph') {
      assert.ok(token.length === 1 || token.length === 2, `unsupported glyph length ${token.length}`);
      output.push(...(token.length === 1 ? [code & 0xff] : [code >> 8, code & 0xff]));
    } else if (token.code === '0000' && token.mnemonic === 'end') {
      output.push(0);
    } else if (code === 2 && token.mnemonic === 'page') {
      output.push(2);
      if (token.length === 2) output.push(1);
    } else if (code === 0x0f00 || (code & 0xff00) === 0x0f00) {
      output.push(0x0f, code & 0xff, ...token.operands.map(operand => Number(operand.value)));
    } else if (code <= 3) {
      output.push(code);
    } else {
      throw new Error(`cannot reconstruct fixture token ${token.mnemonic}/${token.code}`);
    }
  }
  return output;
}

function decodeReadMemory(reply) {
  assert.equal(typeof reply, 'string', 'read_memory returns its exact hexadecimal byte string');
  return Buffer.from(reply, 'hex');
}

async function browserRoute(page, mode, packageBytes) {
  return page.evaluate(async ({ mode, base64 }) => {
    const x = window.xem;
    if (mode !== 'absent') {
      const bytes = Uint8Array.from(atob(base64), char => char.charCodeAt(0));
      const staged = x.packages.stage(bytes, { enabled: mode === 'enabled' });
      if (mode === 'enabled' && staged.identities.length !== 1) throw new Error('expected exactly one active package identity');
      if (mode === 'disabled' && staged.identities.length !== 0) throw new Error('disabled package set must be vanilla');
    }
    await x.boot({ run: false, rewind: false });
    const schema = JSON.parse(x.commandJson('{"cmd":"schema"}'));
    const initialStatus = JSON.parse(x.commandJson('{"cmd":"status"}'));
    const bootMode = JSON.parse(x.commandJson('{"cmd":"boot_mode","mode":0}'));
    if (bootMode.error) throw new Error(bootMode.error);
    const title = await x.run({ cmd: 'frames', count: 303 });
    if (title.error || title.frames !== 303) throw new Error(`title route failed: ${JSON.stringify(title)}`);
    const fieldSetup = JSON.parse(x.commandJson(JSON.stringify({
      cmd: 'scenario', mode: 1,
      writes: [
        { path: 'game_data.map', value: 23 },
        { path: 'mode_kernel_menu_running', value: 0 },
        { path: 'mode_field_entered_once', value: 0 },
      ],
    })));
    if (fieldSetup.error) throw new Error(fieldSetup.error);
    const field = await x.run({ cmd: 'frames', count: 430 });
    if (field.error || field.frames !== 733) throw new Error(`field route failed: ${JSON.stringify(field)}`);
    const status = JSON.parse(x.commandJson('{"cmd":"status"}'));
    const hash = JSON.parse(x.commandJson('{"cmd":"memory_hash"}'));
    const fieldMap = JSON.parse(x.commandJson('{"cmd":"inspect","path":"mode_field_map_id","depth":0}'));
    const appStatus = x.status();
    const identitySchema = schema.identity;
    const read = (address, length) => {
      const value = JSON.parse(x.commandJson(JSON.stringify({ cmd: 'read_memory', address, length })));
      if (typeof value !== 'string' || value.length !== length * 2) throw new Error(`read_memory failed at ${address.toString(16)}`);
      return value;
    };
    const bytes = hex => Uint8Array.from(hex.match(/../g) ?? [], pair => Number.parseInt(pair, 16));
    const candidates = [];
    for (let index = 0; index < 4; index++) {
      const base = 0x800c2698 + index * 0x498;
      const busy = new DataView(bytes(read(base + 0x40e, 2)).buffer).getInt16(0, true);
      const age = new DataView(bytes(read(base + 0x410, 2)).buffer).getUint16(0, true);
      const pointerBytes = bytes(read(base + 0xa8, 4));
      const pointer = new DataView(pointerBytes.buffer).getUint32(0, true);
      if (busy !== 0 || age === 0xffff || pointer < 0x80000000 || pointer >= 0x80200000) continue;
      const text = read(pointer, 80);
      const geometryBytes = bytes(read(base + 0x18, 0x10));
      const geometryView = new DataView(geometryBytes.buffer, geometryBytes.byteOffset, geometryBytes.byteLength);
      candidates.push({
        index, busy, age, pointer, text,
        geometry: {
          x: geometryView.getInt16(4, true),
          y: geometryView.getInt16(6, true),
          width: geometryView.getUint16(0x0a, true) * 4,
          height: geometryView.getUint16(0x0c, true) * 14,
        },
      });
    }
    return {
      schema, initialStatus, status, hash, fieldMap, identity: identitySchema, candidates,
      renderer: { size: appStatus.renderer?.size, picture: appStatus.renderer?.picture },
    };
  }, { mode, base64: packageBytes.toString('base64') });
}

async function runBrowserRoute(page, mode, packageBytes, fixtureEvidence) {
  const result = await browserRoute(page, mode, packageBytes);
  assert.ok(result.schema.identity, `${mode}: schema includes runtime identity`);
  assert.ok(result.schema.schema.module_sha256, `${mode}: schema is build-bound`);
  assert.equal(result.status.frames, 733, `${mode}: route stops at exact frame 733`);
  assert.equal(result.status.stopped, null, `${mode}: original field route remains live`);
  assert.equal(result.status.mode, 1, `${mode}: field mode is active`);
  assert.equal(Number(result.fieldMap), 23, `${mode}: original route entered field map 23`);
  assert.equal(result.status.identity.module, result.schema.schema.module_sha256);
  if (mode === 'enabled') assert.equal(result.status.identity.packages.length, 1);
  else assert.deepEqual(result.status.identity.packages, []);
  const expectedTokens = tokenBytes(mode === 'enabled' ? fixtureEvidence.message_tokens.modified : fixtureEvidence.message_tokens.original);
  const match = findTextCandidate(result.candidates, expectedTokens);
  assert.ok(match, `${mode}: no active field dialogue message matched the authoritative token bytes; candidates=${JSON.stringify(result.candidates.map(c => ({ index: c.index, pointer: c.pointer, bytesSha256: sha256(Buffer.from(c.text, 'hex')) })))}`);
  return {
    ...result,
    textMatch: { index: match.index, pointer: match.pointer, bytes: expectedTokens, geometry: match.geometry },
  };
}

function findTextCandidate(candidates, bytes) {
  return candidates.find(candidate => Buffer.from(candidate.text, 'hex').subarray(0, bytes.length).equals(Buffer.from(bytes)));
}

class HeadlessControl {
  constructor(args, mode, output) {
    this.child = spawn(HEADLESS, args, { cwd: ROOT, stdio: ['pipe', 'pipe', 'pipe'] });
    this.diagnosticsPath = join(output, `headless-${mode}.stderr.log`);
    nativeClients.push(this);
    this.lines = createInterface({ input: this.child.stdout });
    this.waiters = [];
    this.stderr = '';
    this.exit = null;
    this.spawnError = null;
    this.child.stderr.setEncoding('utf8');
    this.child.stderr.on('data', text => { this.stderr = (this.stderr + text).slice(-16_384); });
    this.child.on('error', error => {
      this.spawnError = error;
      this.recordDiagnostics(`headless spawn error: ${error.message}`);
      for (const waiter of this.waiters.splice(0)) waiter.reject(error);
    });
    this.lines.on('line', line => {
      const waiter = this.waiters.shift();
      if (!waiter) return;
      try { waiter.resolve(JSON.parse(line)); } catch (error) { waiter.reject(new Error(`invalid native control JSON ${line}: ${error}`)); }
    });
    this.child.on('close', (code, signal) => {
      this.exit = { code, signal };
      for (const waiter of this.waiters.splice(0)) waiter.reject(new Error(`headless host closed: ${code ?? signal}; ${this.stderr}`));
    });
  }

  command(command) {
    if (this.spawnError) return Promise.reject(this.spawnError);
    if (this.exit) return Promise.reject(new Error(`headless host already exited: ${JSON.stringify(this.exit)}`));
    return new Promise((resolveReply, rejectReply) => {
      const timeout = setTimeout(() => {
        const error = new Error(`native command timed out after 120 seconds: ${JSON.stringify(command)}; stderr: ${this.stderr}`);
        this.recordDiagnostics(error.message);
        rejectReply(error);
        this.child.kill('SIGTERM');
        const killTimer = setTimeout(() => this.child.kill('SIGKILL'), 5_000);
        killTimer.unref();
      }, 120_000);
      this.waiters.push({
        resolve(value) { clearTimeout(timeout); resolveReply(value); },
        reject(error) { clearTimeout(timeout); rejectReply(error); },
      });
      this.child.stdin.write(`${JSON.stringify(command)}\n`, error => {
        if (error) {
          clearTimeout(timeout);
          rejectReply(error);
        }
      });
    });
  }

  async close() {
    if (this.spawnError && !this.exit) {
      this.recordDiagnostics(`headless did not spawn: ${this.spawnError.message}`);
      this.lines.close();
      return;
    }
    if (!this.exit) {
      this.child.stdin.end();
      await new Promise((resolveExit, rejectExit) => {
        const timeout = setTimeout(() => {
          const error = new Error(`headless child did not exit after control EOF; stderr: ${this.stderr}`);
          this.recordDiagnostics(error.message);
          this.child.kill('SIGTERM');
          rejectExit(error);
        }, 30_000);
        this.child.once('close', (code, signal) => {
          clearTimeout(timeout);
          this.exit = { code, signal };
          if (code === 0) {
            this.recordDiagnostics(`headless exited successfully; stderr: ${this.stderr}`);
            resolveExit();
          }
          else {
            const error = new Error(`headless host exited ${code ?? signal}; ${this.stderr}`);
            this.recordDiagnostics(error.message);
            rejectExit(error);
          }
        });
        this.child.once('error', error => {
          clearTimeout(timeout);
          this.recordDiagnostics(`headless close error: ${error.message}`);
          rejectExit(error);
        });
      });
    }
    if (this.exit?.code === 0) this.recordDiagnostics(`headless exited successfully; stderr: ${this.stderr}`);
    this.lines.close();
  }

  recordDiagnostics(reason) {
    try {
      writeFileSync(this.diagnosticsPath, `${reason}\n\n${this.stderr}\n`, { flag: 'wx' });
    } catch (error) {
      if (error.code !== 'EEXIST') throw error;
    }
  }
}

async function nativeRead(client, address, length) {
  return decodeReadMemory(await client.command({ cmd: 'read_memory', address, length }));
}

async function nativeLiveText(client) {
  const candidates = [];
  for (let index = 0; index < 4; index++) {
    const base = 0x800c2698 + index * 0x498;
    const busy = (await nativeRead(client, base + 0x40e, 2)).readInt16LE(0);
    const age = (await nativeRead(client, base + 0x410, 2)).readUInt16LE(0);
    const pointer = (await nativeRead(client, base + 0xa8, 4)).readUInt32LE(0);
    if (busy !== 0 || age === 0xffff || pointer < 0x80000000 || pointer >= 0x80200000) continue;
    const bytes = await nativeRead(client, pointer, 80);
    const geometryBytes = await nativeRead(client, base + 0x18, 0x10);
    candidates.push({ index, busy, age, pointer, bytes, geometry: decodeWindowGeometry(geometryBytes) });
  }
  return candidates;
}

function decodeWindowGeometry(bytes) {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  return {
    x: view.getInt16(4, true),
    y: view.getInt16(6, true),
    width: view.getUint16(0x0a, true) * 4,
    height: view.getUint16(0x0c, true) * 14,
  };
}

function findNativeTextCandidate(candidates, expectedBytes) {
  return candidates.find(candidate => candidate.bytes.subarray(0, expectedBytes.length).equals(Buffer.from(expectedBytes)));
}

function browserTextPixels(png, renderer, geometry) {
  const image = decodePng(png);
  const picture = renderer?.picture;
  const [surfaceWidth, surfaceHeight] = renderer?.size ?? [];
  assert.ok(picture && surfaceWidth > 0 && surfaceHeight > 0, 'browser display geometry is available');
  const [displayWidth, displayHeight] = picture.display;
  assert.ok(displayWidth > 0 && displayHeight > 0, 'browser picture has a display size');
  return countBrightTextPixels(image, {
    x: (picture.x + geometry.x * picture.width / displayWidth) * image.width / surfaceWidth,
    y: (picture.y + geometry.y * picture.height / displayHeight) * image.height / surfaceHeight,
    width: geometry.width * picture.width / displayWidth * image.width / surfaceWidth,
    height: geometry.height * picture.height / displayHeight * image.height / surfaceHeight,
  });
}

function nativeTextPixels(png, display, geometry) {
  const image = decodePng(png);
  assert.ok(display.width > 0 && display.height > 0, 'native display has a size');
  return countBrightTextPixels(image, {
    x: geometry.x * image.width / display.width,
    y: geometry.y * image.height / display.height,
    width: geometry.width * image.width / display.width,
    height: geometry.height * image.height / display.height,
  });
}

function countBrightTextPixels(image, rect) {
  assert.ok(Object.values(rect).every(Number.isFinite), 'active text rectangle coordinates are finite');
  assert.ok(rect.width > 0 && rect.height > 0, 'active text rectangle has positive dimensions');
  const left = Math.floor(rect.x);
  const top = Math.floor(rect.y);
  const right = Math.ceil(rect.x + rect.width);
  const bottom = Math.ceil(rect.y + rect.height);
  assert.ok(left >= 0 && top >= 0 && right <= image.width && bottom <= image.height,
    'active text rectangle is fully inside the captured display');
  let brightPixels = 0;
  let nonBlackPixels = 0;
  for (let y = top; y < bottom; y++) {
    for (let x = left; x < right; x++) {
      const offset = (y * image.width + x) * 4;
      const brightness = image.rgba[offset] + image.rgba[offset + 1] + image.rgba[offset + 2];
      if (brightness > 30) nonBlackPixels++;
      if (brightness >= 450) brightPixels++;
    }
  }
  return {
    rect: { x: left, y: top, width: right - left, height: bottom - top },
    brightPixels,
    nonBlackPixels,
  };
}

async function runNativeRouteRaw(mode, setPath, disc, output) {
  const cards = join(output, `cards-${mode}`);
  mkdirSync(cards, { recursive: true });
  const args = ['--disc', disc, '--control', '--cards', cards];
  if (mode !== 'absent') args.push('--packages', setPath);
  if (mode === 'disabled') args.push('--packages-disabled');
  const client = new HeadlessControl(args, mode, output);
  try {
    const bootStatus = await client.command({ cmd: 'status' });
    const schema = await client.command({ cmd: 'schema' });
    const bootMode = await client.command({ cmd: 'boot_mode', mode: 0 });
    assert.ok(!bootMode.error, JSON.stringify(bootMode));
    const title = await client.command({ cmd: 'frames', count: 303 });
    assert.ok(!title.error, JSON.stringify(title));
    assert.equal(title.frames, 303);
    const setup = await client.command({
      cmd: 'scenario', mode: 1,
      writes: [
        { path: 'game_data.map', value: 23 },
        { path: 'mode_kernel_menu_running', value: 0 },
        { path: 'mode_field_entered_once', value: 0 },
      ],
    });
    assert.ok(!setup.error, JSON.stringify(setup));
    const field = await client.command({ cmd: 'frames', count: 430 });
    assert.ok(!field.error, JSON.stringify(field));
    assert.equal(field.frames, 733);
    const status = await client.command({ cmd: 'status' });
    const hash = await client.command({ cmd: 'memory_hash' });
    const fieldMap = await client.command({ cmd: 'inspect', path: 'mode_field_map_id', depth: 0 });
    const display = await client.command({ cmd: 'display' });
    assert.ok(!display.error, 'native display geometry is available');
    const candidates = await nativeLiveText(client);
    return {
      bootStatus, schema, status, hash, fieldMap, display, candidates, client,
      identity: schema.identity,
      textMatch: candidates,
    };
  } catch (error) {
    await client.close().catch(closeError => {
      client.recordDiagnostics(`route failed: ${error.message}; close failed: ${closeError.message}`);
    });
    throw error;
  }
}

async function runNativeRoute(mode, setPath, disc, output, fixtureEvidence) {
  const result = await runNativeRouteRaw(mode, setPath, disc, output);
  assert.equal(result.status.frames, 733);
  assert.equal(result.status.stopped, null);
  assert.equal(result.status.mode, 1);
  assert.equal(Number(result.fieldMap), 23);
  assert.equal(result.status.identity.module, result.schema.schema.module_sha256);
  const expectedTokens = tokenBytes(mode === 'enabled' ? fixtureEvidence.message_tokens.modified : fixtureEvidence.message_tokens.original);
  const match = findNativeTextCandidate(result.candidates, expectedTokens);
  assert.ok(match, `${mode}: no active native dialogue window matched original token bytes; candidates=${JSON.stringify(result.candidates.map(c => ({ index: c.index, pointer: c.pointer, sha256: sha256(c.bytes) })))}`);
  result.textMatch = { index: match.index, pointer: match.pointer, bytes: expectedTokens, geometry: match.geometry };
  return result;
}

async function verifyBrowserM4(page, expectedHash) {
  return page.evaluate(async expectedHash => {
    const x = window.xem;
    const initial = JSON.parse(x.commandJson('{"cmd":"memory_hash"}'));
    if (initial !== expectedHash) throw new Error('enabled M5 state changed before M4 snapshot checks');
    const snapshot = JSON.parse(x.commandJson('{"cmd":"snapshot","name":"m5-boundary"}'));
    if (snapshot.error) throw new Error(snapshot.error);
    const changed = JSON.parse(x.commandJson('{"cmd":"write","path":"game_data.map","value":24}'));
    if (changed.error) throw new Error(changed.error);
    const changedHash = JSON.parse(x.commandJson('{"cmd":"memory_hash"}'));
    if (changedHash === initial) throw new Error('valid schema scalar write did not change memory');
    const invalidBefore = JSON.parse(x.commandJson('{"cmd":"memory_hash"}'));
    const invalid = JSON.parse(x.commandJson('{"cmd":"write","path":"__m5_invalid_scalar__","value":1}'));
    const invalidAfter = JSON.parse(x.commandJson('{"cmd":"memory_hash"}'));
    if (!invalid.error || invalidBefore !== invalidAfter) throw new Error('invalid schema write was not refused atomically');
    const restore = JSON.parse(x.commandJson('{"cmd":"restore","name":"m5-boundary"}'));
    if (restore.error) throw new Error(restore.error);
    const restored = JSON.parse(x.commandJson('{"cmd":"memory_hash"}'));
    if (restored !== initial) throw new Error('matching snapshot restore did not restore the original hash');
    window.__m5PackageSnapshot = new Uint8Array(x.exportSnapshot());
    return { initial, changedHash, invalidError: invalid.error, invalidBefore, invalidAfter, restored };
  }, expectedHash);
}

async function verifyNativeM4(client, output, expectedHash) {
    const initial = await client.command({ cmd: 'memory_hash' });
    assert.equal(initial, expectedHash);
    assert.ok(!(await client.command({ cmd: 'snapshot', name: 'm5-boundary' })).error);
    const changed = await client.command({ cmd: 'write', path: 'game_data.map', value: 24 });
    assert.ok(!changed.error, JSON.stringify(changed));
    const changedHash = await client.command({ cmd: 'memory_hash' });
    assert.notEqual(changedHash, initial);
    const invalidBefore = await client.command({ cmd: 'memory_hash' });
    const invalid = await client.command({ cmd: 'write', path: '__m5_invalid_scalar__', value: 1 });
    const invalidAfter = await client.command({ cmd: 'memory_hash' });
    assert.ok(invalid.error);
    assert.equal(invalidBefore, invalidAfter);
    assert.ok(!(await client.command({ cmd: 'restore', name: 'm5-boundary' })).error);
    const restored = await client.command({ cmd: 'memory_hash' });
    assert.equal(restored, initial);
    const statePath = join(output, 'native-enabled.state');
    const saved = await client.command({ cmd: 'state_save', path: statePath });
    assert.equal(saved.saved, true);
    return { initial, changedHash, invalidError: invalid.error, invalidBefore, invalidAfter, restored, statePath };
}

async function rejectBrowserForeignSnapshot(page, packageBytes) {
  return page.evaluate(async base64 => {
    const x = window.xem;
    x.packages.stage(Uint8Array.from(atob(base64), c => c.charCodeAt(0)), { enabled: false });
    await x.boot({ run: false, rewind: false });
    const before = JSON.parse(x.commandJson('{"cmd":"memory_hash"}'));
    let error = '';
    try { x.importSnapshot(window.__m5PackageSnapshot); }
    catch (caught) { error = String(caught.message ?? caught); }
    const after = JSON.parse(x.commandJson('{"cmd":"memory_hash"}'));
    return { error, before, after };
  }, packageBytes.toString('base64'));
}

async function rejectNativeForeignSnapshot(client, output) {
    const before = await client.command({ cmd: 'memory_hash' });
    const refusal = await client.command({ cmd: 'state_load', path: join(output, 'native-enabled.state') });
    const after = await client.command({ cmd: 'memory_hash' });
    return { error: refusal.error ?? '', before, after };
}

function summarizeModes(modes) {
  return Object.fromEntries(Object.entries(modes).map(([name, value]) => [name, {
    identity: value.identity,
    status: value.status,
    memoryHash: value.hash,
    activeTextWindow: value.textMatch ? { window: value.textMatch.index, pointer: value.textMatch.pointer } : null,
  }]));
}
