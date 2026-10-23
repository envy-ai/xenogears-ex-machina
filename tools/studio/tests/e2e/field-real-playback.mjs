// Gated M6 real-disc playback of the exact service-authored package set.
// This consumes private ignored fixture output and is never a synthetic runtime pass.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve, sep } from 'node:path';
import { createInterface } from 'node:readline';
import { test } from 'node:test';
import { HEADLESS, NO_WEBGPU, ROOT, WEB, start } from '../../../../src/runtime/web/tests/support.mjs';

const enabled = process.env.XEM_STUDIO_M6_REAL_PLAYBACK === '1';
const milestoneRoot = resolve(ROOT, 'tmp/studio/milestone-6');

test('gated real M6 field edit plays through browser and native using the same package set', {
  skip: !enabled && 'set XEM_STUDIO_M6_REAL_PLAYBACK=1 to enable private M6 real-disc playback',
  timeout: 1_200_000,
}, async () => {
  const fixtureRoot = requiredPath('XEM_STUDIO_M6_FIXTURE_ROOT');
  const packageSetPath = requiredPath('XEM_STUDIO_M6_SET');
  const discPath = requiredPath('XEM_STUDIO_M6_DISC1');
  const extractRoot = requiredPath('XEM_STUDIO_EXTRACT_ROOT');
  const artifactRoot = requiredPath('XEM_STUDIO_M6_ARTIFACT_ROOT');
  for (const path of [fixtureRoot, packageSetPath, artifactRoot]) {
    assert.ok(path === milestoneRoot || path.startsWith(milestoneRoot + sep), 'private M6 inputs and outputs stay under ignored milestone storage');
  }
  assert.equal(discPath, resolve(join(extractRoot, 'discs/disc1.bin')),
    'runtime reads the same explicitly selected original disc 1 BIN as the source fixture');
  assert.ok(artifactRoot !== fixtureRoot, 'preserve the service fixture and create separate runtime evidence');
  assert.ok(existsSync(join(fixtureRoot, 'evidence.json')),
    'the gated real-source service fixture evidence exists');
  assert.ok(existsSync(packageSetPath) && existsSync(discPath) && existsSync(HEADLESS),
    'the exact candidate set, original disc, and native headless host exist');
  assert.ok(existsSync(join(WEB, 'game/game.wasm')) && existsSync(join(WEB, 'game/schema.json')),
    'the selected production browser build includes its game module and schema');
  assert.ok(existsSync(dirname(artifactRoot)) && !existsSync(artifactRoot),
    'the private output parent exists and the evidence leaf is fresh');

  const evidence = JSON.parse(readFileSync(join(fixtureRoot, 'evidence.json'), 'utf8'));
  const visibleMessage = JSON.parse(readFileSync(join(fixtureRoot, 'visible-message0.json'), 'utf8'));
  const vanillaMessage = JSON.parse(readFileSync(join(fixtureRoot, 'visible-message1.json'), 'utf8'));
  const setBytes = readFileSync(packageSetPath);
  const parsedSet = packageSetIdentity(setBytes);
  const packageDigest = parsedSet.memberHash;
  assert.ok(typeof parsedSet.manifest.id === 'string' && parsedSet.manifest.id.length > 0,
    'the selected Studio-built package has a manifest ID');
  assert.equal(visibleMessage.index, 0, 'runtime visible-text evidence identifies existing message 0');
  const expectedMessageBytes = Buffer.from(visibleMessage.bytes_hex, 'hex');
  assert.equal(expectedMessageBytes.length, visibleMessage.byte_length, 'message byte evidence length matches its terminator-inclusive encoding');
  assert.equal(sha256(expectedMessageBytes), visibleMessage.sha256, 'message evidence hash matches its private bytes');
  const expectedVanillaMessage = Buffer.from(vanillaMessage.bytes_hex, 'hex');
  assert.equal(vanillaMessage.index, 1, 'vanilla route evidence identifies the existing visible message 1');
  assert.equal(expectedVanillaMessage.length, vanillaMessage.byte_length, 'vanilla message bytes include the authoritative terminator');
  assert.equal(sha256(expectedVanillaMessage), vanillaMessage.sha256, 'vanilla message evidence hash matches its private bytes');
  assert.equal(evidence.edited_message, visibleMessage.index, 'authored event selects the message whose bytes are checked at runtime');
  assert.equal(evidence.non_event_components_equal, true, 'the fixture verified every non-event component before packaging');
  assert.ok(parsedSet.manifest.files.length >= 1, 'selected package contains its authored map delta');
  const mapEdit = parsedSet.manifest.files.find(file => file.slot === evidence.slot && file.disc === evidence.disc1.serial);
  assert.ok(mapEdit, 'package manifest contains the authored map 23 slot');
  assert.equal(mapEdit.kind, 'delta', 'authored map remains a source-validated delta');
  const untouched = parsedSet.manifest.files.find(file => file.slot === evidence.untouched_map.slot && file.disc === evidence.disc1.serial);
  assert.equal(evidence.untouched_map.source_equals_result, true, 'fixture verified the untouched source map is byte-identical');
  if (untouched) assert.equal(untouched.source_sha256, untouched.result_sha256, 'any bundled untouched map member is source-equals-result');
  assert.equal(mapEdit.source_sha256, evidence.source_bundle_sha256, 'authored delta binds to the source map bundle hash');
  assert.equal(mapEdit.result_sha256, evidence.result_bundle_sha256, 'authored delta result matches the private verified bundle');
  mkdirSync(artifactRoot, { recursive: false });

  let browser;
  try {
    browser = await start({ args: NO_WEBGPU });
    const page = await browser.page();
    const modes = {};
    for (const mode of ['absent', 'disabled', 'enabled']) {
      modes[mode] = await browserRoute(page, mode, setBytes, discPath,
        mode === 'enabled' ? expectedMessageBytes : expectedVanillaMessage);
      const screenshot = await page.locator('canvas').screenshot({ path: join(artifactRoot, `browser-${mode}.png`) });
      modes[mode].screenshot = { sha256: sha256(screenshot), path: `browser-${mode}.png` };
    }
    const nativeModes = {};
    for (const mode of ['absent', 'disabled', 'enabled']) {
      nativeModes[mode] = await nativeRoute(mode, packageSetPath, discPath, artifactRoot, evidence,
        mode === 'enabled' ? expectedMessageBytes : expectedVanillaMessage);
    }

    const untouchedBrowser = {};
    const untouchedPage = await browser.page();
    try {
      for (const mode of ['absent', 'disabled', 'enabled']) {
        untouchedBrowser[mode] = await browserRoute(untouchedPage, mode, setBytes, discPath, null, evidence.untouched_map.map);
      }
    } finally {
      await untouchedPage.close();
    }
    const untouchedNative = {};
    for (const mode of ['absent', 'disabled', 'enabled']) {
      untouchedNative[mode] = await nativeRoute(mode, packageSetPath, discPath, artifactRoot, evidence, null,
        evidence.untouched_map.map, `untouched-native-${mode}`);
    }
    for (const mode of ['disabled', 'enabled']) {
      assert.equal(untouchedBrowser[mode].memoryHash, untouchedBrowser.absent.memoryHash,
        `browser ${mode} preserves the original untouched map runtime memory`);
      assert.equal(untouchedNative[mode].memoryHash, untouchedNative.absent.memoryHash,
        `native ${mode} preserves the original untouched map runtime memory`);
      assert.equal(untouchedBrowser[mode].memoryHash, untouchedNative[mode].memoryHash,
        `untouched map ${mode} memory matches between production hosts`);
    }

    for (const mode of ['absent', 'disabled']) {
      assert.equal(modes[mode].memoryHash, modes.absent.memoryHash, `browser ${mode} memory matches package-absent vanilla`);
      assert.equal(nativeModes[mode].memoryHash, nativeModes.absent.memoryHash, `native ${mode} memory matches package-absent vanilla`);
      assert.equal(modes[mode].memoryHash, nativeModes[mode].memoryHash, `${mode} memory hash matches between hosts`);
      assert.ok(modes[mode].messageMatchesOriginal, `browser ${mode} window retains original visible message 1`);
      assert.ok(nativeModes[mode].messageMatchesOriginal, `native ${mode} window retains original visible message 1`);
      assert.equal(modes[mode].screenshot.sha256, modes.absent.screenshot.sha256, `browser ${mode} screenshot matches vanilla`);
      assert.equal(nativeModes[mode].screenshot.sha256, nativeModes.absent.screenshot.sha256, `native ${mode} screenshot matches vanilla`);
    }
    assert.equal(modes.enabled.memoryHash, nativeModes.enabled.memoryHash, 'enabled memory hash matches across hosts before input');
    assert.notEqual(modes.enabled.screenshot.sha256, modes.absent.screenshot.sha256, 'edited browser message changes the real field screenshot');
    assert.notEqual(nativeModes.enabled.screenshot.sha256, nativeModes.absent.screenshot.sha256, 'edited native message changes the real field screenshot');
    assert.ok(modes.enabled.messageMatchesEdited, 'browser active message window contains the exact existing message 0 bytes');
    assert.ok(nativeModes.enabled.messageMatchesEdited, 'native active message window contains the exact existing message 0 bytes');
    assert.equal(modes.enabled.identity.module, modes.enabled.schema.schema.module_sha256, 'browser schema is bound to the running module');
    assert.equal(nativeModes.enabled.identity.module, nativeModes.enabled.schema.schema.module_sha256, 'native schema is bound to the running module');
    const discFamily = [evidence.disc1.serial, evidence.disc2.serial].sort().join('+');
    assert.equal(modes.enabled.identity.disc, discFamily, 'browser runtime binds the verified two-disc family');
    assert.equal(nativeModes.enabled.identity.disc, discFamily, 'native runtime binds the same verified two-disc family');
    assert.equal(modes.enabled.disc.sha256, evidence.disc1.boot_sha256, 'browser disc picker identified the exact original disc 1 image');
    assert.deepEqual(modes.enabled.identity.packages, nativeModes.enabled.identity.packages, 'ordered active package identities match across hosts');
    assert.deepEqual(modes.enabled.identity.packages, [parsedSet.identity], 'active identity names the exact package set member');
    assert.deepEqual(modes.enabled.schema, nativeModes.enabled.schema, 'browser and native schema contracts agree');
    assert.deepEqual(modes.enabled.status, nativeModes.enabled.status, 'browser and native pre-input status agree');
    assert.equal(modes.enabled.status.mode, 1, 'browser reached field mode');
    assert.equal(nativeModes.enabled.status.mode, 1, 'native host reached field mode');
    assert.equal(modes.enabled.status.frames, 733, 'browser route stopped at the verified map 23 point');
    assert.equal(nativeModes.enabled.status.frames, 733, 'native route stopped at the same map 23 point');

    // Use the public input command to dismiss/advance the original message, then
    // let the authored call and return execute through ordinary game frames.
    modes.enabled.authoredMarker = await browserAdvance(page, modes.enabled.schema, evidence.authored_variable);
    assert.equal(modes.enabled.authoredMarker.value, evidence.authored_variable.marker,
      'browser original input and bounded frames execute the authored variable-setting event');
    assert.equal(nativeModes.enabled.authoredMarker.value, evidence.authored_variable.marker,
      'native original input and bounded frames execute the same authored event');
    assert.equal(modes.enabled.authoredMarker.value, nativeModes.enabled.authoredMarker.value,
      'both hosts observe the authored marker through schema-derived memory');

    writeFileSync(join(artifactRoot, 'result.json'), `${JSON.stringify({
      acceptance: 'gated real-disc M6 runtime playback through production browser and xem-headless adapters',
      fixtureEvidence: 'private source identity, package and authored variable bindings; no source bytes copied to this result',
      packageSetSha256: sha256(setBytes),
      packageMemberSha256: packageDigest,
      route: { bootMode: 0, titleFrames: 303, fieldMode: 1, map: 23, fieldFrames: 430, visibleAt: 733, postInputFrames: 600 },
      browser: summarize(modes),
      native: summarize(nativeModes),
      untouchedMap: { map: evidence.untouched_map.map, browser: summarize(untouchedBrowser), native: summarize(untouchedNative) },
      checks: {
        exactSamePackageSetBothHosts: true,
        absentAndDisabledVanillaMemoryParity: true,
        exactExistingMessageBytesInActiveWindow: true,
        originalDiscAndModuleIdentityMatch: true,
        serviceFixtureVerifiedUntouchedMapSourceEqualsResult: true,
        enabledRuntimeMatchesVanillaOnUntouchedMap: true,
        authoredVariableObservedAfterNormalInputAndFrames: true,
        browserNativeSchemaStatusAndEnabledHashParity: true,
      },
    }, null, 2)}\n`, { flag: 'wx' });
  } finally {
    if (browser) await browser.close();
  }
});

function requiredPath(name) {
  const value = process.env[name];
  assert.ok(value, `${name} must explicitly name a private M6 path`);
  return resolve(value);
}

function sha256(bytes) { return createHash('sha256').update(bytes).digest('hex'); }

function packageSetIdentity(bytes) {
  assert.equal(bytes.toString('ascii', 0, 8), 'XEMSET1\0', 'input is a versioned package set');
  const count = bytes.readUInt32LE(8);
  assert.equal(count, 1, 'M6 fixture contains exactly one ordered package');
  const size = bytes.readUInt32LE(12);
  assert.equal(bytes.length, 16 + size, 'package-set framing has no trailing bytes');
  const member = bytes.subarray(16);
  assert.equal(member.toString('ascii', 0, 8), 'XEMMOD1\0', 'package set contains a versioned package');
  const manifestLength = member.readUInt32LE(8);
  const manifest = JSON.parse(member.toString('utf8', 12, 12 + manifestLength));
  return { manifest, identity: `${manifest.id}@${sha256(member)}`, memberHash: sha256(member) };
}

async function browserCommand(page, request) {
  const result = await page.evaluate(value => JSON.parse(window.xem.commandJson(JSON.stringify(value))), request);
  assert.equal(result.error, undefined, `browser ${request.cmd} command completed`);
  return result;
}

function requireNoError(reply, label) {
  assert.equal(reply.error, undefined, `${label} completed`);
  return reply;
}

function hexBytes(value) {
  assert.equal(typeof value, 'string', 'read_memory returned hexadecimal bytes');
  return Buffer.from(value, 'hex');
}

async function browserWindows(page) {
  const candidates = await page.evaluate(() => {
    const x = window.xem;
    const readWord = (address, length, signed = false) => {
      const value = JSON.parse(x.commandJson(JSON.stringify({ cmd: 'read_memory', address, length })));
      const bytes = Uint8Array.from(value.match(/../g) ?? [], pair => Number.parseInt(pair, 16));
      const view = new DataView(bytes.buffer);
      return length === 2 ? (signed ? view.getInt16(0, true) : view.getUint16(0, true)) : view.getUint32(0, true);
    };
    const result = [];
    for (let index = 0; index < 4; index++) {
      const base = 0x800c2698 + index * 0x498;
      const busy = readWord(base + 0x40e, 2, true);
      const age = readWord(base + 0x410, 2);
      const pointer = readWord(base + 0xa8, 4);
      if (busy !== 0 || age === 0xffff || pointer < 0x80000000 || pointer >= 0x80200000) continue;
      const text = JSON.parse(x.commandJson(JSON.stringify({ cmd: 'read_memory', address: pointer, length: 80 })));
      result.push({ index, bytesHex: text });
    }
    return result;
  });
  return candidates.map(candidate => ({ index: candidate.index, bytes: Buffer.from(candidate.bytesHex, 'hex') }));
}

function matchingWindow(candidates, expected) {
  return candidates.some(candidate => candidate.bytes.subarray(0, expected.length).equals(expected));
}

async function browserRoute(page, mode, setBytes, discPath, expectedMessageBytes, targetMap = 23) {
  if (mode === 'absent') {
    await page.setInputFiles('#file', discPath);
    await page.waitForFunction(() => ['identified', 'failed'].includes(window.xem.status().disc?.state), null, { timeout: 120_000 });
  }
  const discStatus = await page.evaluate(() => window.xem.status().disc);
  assert.equal(discStatus.state, 'identified', `browser ${mode} disc identifies`);
  if (mode !== 'absent') {
    const staged = await page.evaluate(({ base64, mode }) => {
      const bytes = Uint8Array.from(atob(base64), char => char.charCodeAt(0));
      return window.xem.packages.stage(bytes, { enabled: mode === 'enabled' });
    }, { base64: setBytes.toString('base64'), mode });
    assert.equal(staged.identities.length, mode === 'enabled' ? 1 : 0, `browser ${mode} package staging identity count`);
  }
  await page.evaluate(() => window.xem.boot({ run: false, rewind: false }));
  const schema = await browserCommand(page, { cmd: 'schema' });
  const bootMode = await browserCommand(page, { cmd: 'boot_mode', mode: 0 });
  requireNoError(bootMode, `browser ${mode} title boot`);
  requireNoError(await page.evaluate(() => window.xem.run({ cmd: 'frames', count: 303 })), `browser ${mode} title frames`);
  requireNoError(await browserCommand(page, {
    cmd: 'scenario', mode: 1,
    writes: [
      { path: 'game_data.map', value: targetMap },
      { path: 'mode_kernel_menu_running', value: 0 },
      { path: 'mode_field_entered_once', value: 0 },
    ],
  }), `browser ${mode} field setup`);
  requireNoError(await page.evaluate(() => window.xem.run({ cmd: 'frames', count: 430 })), `browser ${mode} field frames`);
  const status = await browserCommand(page, { cmd: 'status' });
  const memoryHash = await browserCommand(page, { cmd: 'memory_hash' });
  const candidates = await browserWindows(page);
  const messageMatches = expectedMessageBytes ? matchingWindow(candidates, expectedMessageBytes) : false;
  if (expectedMessageBytes) assert.ok(messageMatches, `browser ${mode} active window matches expected existing message bytes`);
  const fieldMap = await browserCommand(page, { cmd: 'inspect', path: 'mode_field_map_id', depth: 0 });
  assert.equal(Number(fieldMap), targetMap, `browser ${mode} enters map ${targetMap}`);
  return { schema, identity: schema.identity, status, memoryHash, candidates, messageMatchesOriginal: expectedMessageBytes && mode !== 'enabled' && messageMatches,
    messageMatchesEdited: expectedMessageBytes && mode === 'enabled' && messageMatches, disc: discStatus.identity };
}

async function browserAdvance(page, schema, variable) {
  const press = await browserCommand(page, { cmd: 'press', buttons: ['cross'], frames: 1 });
  assert.equal(press.error, undefined, 'browser public cross-button input is accepted');
  requireNoError(await page.evaluate(() => window.xem.run({ cmd: 'frames', count: 600 })), 'browser bounded authored-event frames');
  const address = variableAddress(schema, variable);
  const result = await browserCommand(page, { cmd: 'read_memory', address, length: 2 });
  return { address, value: hexBytes(result).readUInt16LE(0) };
}

async function nativeRoute(mode, setPath, discPath, outputRoot, evidence, expectedMessageBytes, targetMap = 23, label = `native-${mode}`) {
  const cardPath = join(outputRoot, `cards-${label}`);
  mkdirSync(cardPath, { recursive: false });
  const args = ['--disc', discPath, '--control', '--cards', cardPath];
  if (mode !== 'absent') args.push('--packages', setPath);
  if (mode === 'disabled') args.push('--packages-disabled');
  const client = new NativeControl(args, label, outputRoot);
  try {
    const initialStatus = await client.command({ cmd: 'status' });
    requireNoError(initialStatus, `native ${mode} initial status`);
    const schema = requireNoError(await client.command({ cmd: 'schema' }), `native ${mode} schema`);
    requireNoError(await client.command({ cmd: 'boot_mode', mode: 0 }), `native ${mode} title boot`);
    requireNoError(await client.command({ cmd: 'frames', count: 303 }), `native ${mode} title frames`);
    requireNoError(await client.command({ cmd: 'scenario', mode: 1, writes: [
      { path: 'game_data.map', value: targetMap },
      { path: 'mode_kernel_menu_running', value: 0 },
      { path: 'mode_field_entered_once', value: 0 },
    ] }), `native ${mode} field setup`);
    requireNoError(await client.command({ cmd: 'frames', count: 430 }), `native ${label} field frames`);
    const status = requireNoError(await client.command({ cmd: 'status' }), `native ${mode} status`);
    const memoryHash = requireNoError(await client.command({ cmd: 'memory_hash' }), `native ${mode} memory hash`);
    const fieldMap = requireNoError(await client.command({ cmd: 'inspect', path: 'mode_field_map_id', depth: 0 }), `native ${mode} field map`);
    assert.equal(Number(fieldMap), targetMap, `native ${label} enters map ${targetMap}`);
    const candidates = await nativeWindows(client, schema);
    const messageMatches = expectedMessageBytes ? matchingWindow(candidates, expectedMessageBytes) : false;
    if (expectedMessageBytes) assert.ok(messageMatches, `native ${mode} active window matches expected existing message bytes`);
    const screenshotPath = join(outputRoot, `${label}.png`);
    if (expectedMessageBytes) requireNoError(await client.command({ cmd: 'screenshot', path: screenshotPath }), `native ${mode} screenshot`);
    let authoredMarker;
    if (mode === 'enabled' && targetMap === 23) {
      requireNoError(await client.command({ cmd: 'press', buttons: ['cross'], frames: 1 }), 'native public cross-button input');
      requireNoError(await client.command({ cmd: 'frames', count: 600 }), 'native bounded authored-event frames');
      const address = variableAddress(schema, evidence.authored_variable);
      const value = requireNoError(await client.command({ cmd: 'read_memory', address, length: 2 }), 'native schema-derived authored variable read');
      authoredMarker = { address, value: hexBytes(value).readUInt16LE(0) };
    }
    return {
      schema, identity: schema.identity, status, memoryHash, disc: schema.identity.disc,
      messageMatchesOriginal: expectedMessageBytes && mode !== 'enabled' && messageMatches,
      messageMatchesEdited: expectedMessageBytes && mode === 'enabled' && messageMatches,
      authoredMarker,
      screenshot: expectedMessageBytes ? { path: `${label}.png`, sha256: sha256(readFileSync(screenshotPath)) } : undefined,
    };
  } finally {
    await client.close();
  }
}

async function nativeWindows(client, schemaReply) {
  const candidates = [];
  for (let index = 0; index < 4; index++) {
    const base = 0x800c2698 + index * 0x498;
    const busy = hexBytes(await client.command({ cmd: 'read_memory', address: base + 0x40e, length: 2 })).readInt16LE(0);
    const age = hexBytes(await client.command({ cmd: 'read_memory', address: base + 0x410, length: 2 })).readUInt16LE(0);
    const pointer = hexBytes(await client.command({ cmd: 'read_memory', address: base + 0xa8, length: 4 })).readUInt32LE(0);
    if (busy !== 0 || age === 0xffff || pointer < 0x80000000 || pointer >= 0x80200000) continue;
    const text = await client.command({ cmd: 'read_memory', address: pointer, length: 80 });
    candidates.push({ index, bytes: hexBytes(text) });
  }
  assert.ok(schemaReply.identity?.module, 'native schema includes module identity');
  return candidates;
}

class NativeControl {
  constructor(args, mode, outputRoot) {
    this.child = spawn(HEADLESS, args, { cwd: ROOT, stdio: ['pipe', 'pipe', 'pipe'] });
    this.lines = createInterface({ input: this.child.stdout });
    this.stderr = '';
    this.stderrPath = join(outputRoot, `native-${mode}.stderr.log`);
    this.child.stderr.setEncoding('utf8');
    this.child.stderr.on('data', text => { this.stderr = (this.stderr + text).slice(-16_384); });
    this.lines.on('line', line => {
      const waiter = this.waiter;
      this.waiter = null;
      if (!waiter) return;
      clearTimeout(waiter.timer);
      try { waiter.resolve(JSON.parse(line)); }
      catch { waiter.reject(new Error('native control returned invalid JSON')); }
    });
    this.child.on('error', error => {
      this.spawnError = error;
      this.rejectWaiter(error);
    });
    this.child.on('close', (code, signal) => {
      this.exit = { code, signal };
      if (this.waiter) this.rejectWaiter(new Error(`native control closed before reply (${code ?? signal})`));
    });
  }

  rejectWaiter(error) {
    if (!this.waiter) return;
    clearTimeout(this.waiter.timer);
    this.waiter.reject(error);
    this.waiter = null;
  }

  command(request) {
    if (this.spawnError) return Promise.reject(this.spawnError);
    if (this.exit) return Promise.reject(new Error('native control process is already closed'));
    if (this.waiter) return Promise.reject(new Error('native control commands must be sequential'));
    return new Promise((resolveReply, rejectReply) => {
      const timer = setTimeout(() => {
        this.recordDiagnostics('native command exceeded the bounded 120 second wait');
        this.child.kill('SIGTERM');
        rejectReply(new Error('native control command timed out'));
        this.waiter = null;
      }, 120_000);
      this.waiter = { resolve: resolveReply, reject: rejectReply, timer };
      this.child.stdin.write(`${JSON.stringify(request)}\n`, error => {
        if (error) this.rejectWaiter(error);
      });
    });
  }

  async close() {
    if (!this.exit && !this.spawnError) {
      this.child.stdin.end();
      await new Promise((resolveClose, rejectClose) => {
        const timeout = setTimeout(() => rejectClose(new Error('native control child did not close after EOF')), 30_000);
        this.child.once('close', (code, signal) => {
          clearTimeout(timeout);
          this.exit = { code, signal };
          if (code === 0) resolveClose();
          else rejectClose(new Error(`native control exited ${code ?? signal}`));
        });
        this.child.once('error', error => { clearTimeout(timeout); rejectClose(error); });
      });
    }
    this.lines.close();
    this.recordDiagnostics(`native control exit=${this.exit?.code ?? this.exit?.signal ?? 'spawn-error'}`);
    assert.equal(this.exit?.code, 0, 'owned native control child exited successfully');
  }

  recordDiagnostics(reason) {
    try { writeFileSync(this.stderrPath, `${reason}\n${this.stderr}\n`, { flag: 'wx' }); }
    catch (error) { if (error.code !== 'EEXIST') throw error; }
  }
}

function variableAddress(schema, variable) {
  const base = schema.schema?.globals?.field_event_variables?.address;
  assert.ok(Number.isInteger(base), 'running module schema exposes the field_event_variables global address');
  assert.ok(Number.isInteger(variable.operand_reference) && variable.operand_reference >= 0,
    'candidate evidence exposes the typed field-bank byte reference');
  const address = base + variable.operand_reference;
  assert.ok(address <= 0xffffffff && address >= base, 'schema-derived authored-variable address remains in range');
  return address;
}

function summarize(modes) {
  return Object.fromEntries(Object.entries(modes).map(([mode, value]) => [mode, {
    identity: value.identity ?? value.schema.identity,
    status: value.status,
    memoryHash: value.memoryHash,
    messageMatchesOriginal: value.messageMatchesOriginal,
    messageMatchesEdited: value.messageMatchesEdited,
    authoredMarker: value.authoredMarker,
    screenshot: value.screenshot,
  }]));
}
