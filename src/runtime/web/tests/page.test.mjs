// The browser host in headless Chromium: rendering on both backends, the Slint
// settings panel, settings persistence, disc import and, over a game module
// fixture, the automation API, the game's sound, input, memory cards in
// IndexedDB and snapshot saves. Needs no private inputs.

import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { after, before, describe, test } from 'node:test';
import { runInNewContext } from 'node:vm';
import { NO_WEBGPU, WEBGPU, pixelStats, settingsSection, shot, start, status, syntheticDisc } from './support.mjs';
import { createAutomation } from '../xem.js';

function deferred() {
  let resolve;
  const promise = new Promise(done => { resolve = done; });
  return { promise, resolve };
}

function blockedAutomation() {
  const commands = [];
  const fill = deferred();
  const started = deferred();
  let generation = 1;
  let operationGeneration = 0;
  let booting = false;
  const xem = createAutomation({
    app: {
      status() { return JSON.stringify({ session: {} }); },
      command(text) {
        commands.push(JSON.parse(text));
        return JSON.stringify({ blocked: true, frames: 0 });
      },
    },
    pageStatus: () => ({ sessionGeneration: generation, operationGeneration, booting }),
    async fillDisc(check) { started.resolve(); await fill.promise; check(); },
    async flushCards() {},
  });
  return {
    xem, commands, fill, started,
    replaceSession() { generation += 1; },
    beginBoot() { operationGeneration += 1; booting = true; },
    endBoot() { booting = false; },
  };
}

test('a replacement session stops async frame continuation after a deferred disc fill', async () => {
  const probe = blockedAutomation();
  const running = probe.xem.run({ cmd: 'frames', count: 3 });
  await probe.started.promise;
  probe.replaceSession();
  probe.fill.resolve();
  await assert.rejects(running, /session changed during asynchronous frame work/);
  assert.deepEqual(probe.commands, [{ cmd: 'frames', count: 3 }], 'no later frame command reaches the replacement session');
});

test('a failed boot operation invalidates deferred frame continuation without publishing a session', async () => {
  const probe = blockedAutomation();
  const generation = probe.xem.status().page.sessionGeneration;
  const running = probe.xem.run({ cmd: 'frames', count: 3 });
  await probe.started.promise;
  probe.beginBoot();
  assert.deepEqual(probe.xem.command({ cmd: 'status' }), { error: 'a session boot is in progress' });
  probe.endBoot();
  probe.fill.resolve();
  await assert.rejects(running, /boot operation changed during asynchronous frame work/);
  assert.equal(probe.xem.status().page.sessionGeneration, generation, 'failed boot leaves the publication generation unchanged');
  assert.deepEqual(probe.commands, [{ cmd: 'frames', count: 3 }], 'no later frame command reaches the retained session');
});

test('setRunning refuses to call the app during a boot operation', () => {
  const probe = blockedAutomation();
  probe.beginBoot();
  assert.throws(() => probe.xem.setRunning(true), /a session boot is in progress/);
  assert.deepEqual(probe.commands, [], 'the blocked request never reaches the app');
});

test('cancellation stops async frame continuation after a deferred disc fill', async () => {
  const probe = blockedAutomation();
  const cancellation = new AbortController();
  const running = probe.xem.run({ cmd: 'frames', count: 3 }, { signal: cancellation.signal });
  await probe.started.promise;
  cancellation.abort(new Error('owned game link closed'));
  probe.fill.resolve();
  await assert.rejects(running, /owned game link closed/);
  assert.deepEqual(probe.commands, [{ cmd: 'frames', count: 3 }], 'no later frame command reaches the closed session');
});

async function canvasShot(page, name) {
  const png = await page.locator('canvas').screenshot({ path: shot(name) });
  return pixelStats(png);
}

test('audio worklet keeps consecutive samples through animation scheduling jitter', () => {
  let Output;
  runInNewContext(readFileSync(new URL('../audio-worklet.js', import.meta.url), 'utf8'), {
    AudioWorkletProcessor: class { constructor() { this.port = { postMessage() {} }; } },
    registerProcessor(_name, processor) { Output = processor; },
  });
  const output = new Output();
  const sample = (at) => Math.fround(Math.sin(at * 0.07) * 0.5);
  let produced = 0;
  const push = (count) => {
    const samples = new Float32Array(count * 2);
    for (let i = 0; i < count; i++) {
      samples[i * 2] = sample(produced + i);
      samples[i * 2 + 1] = -samples[i * 2];
    }
    produced += count;
    output.push(samples);
  };
  const process = () => {
    const left = new Float32Array(128), right = new Float32Array(128);
    output.process([], [[left, right]]);
    return [left, right];
  };
  push(735);
  assert.ok(process()[0].every((value) => value === 0));
  assert.equal(output.buffered,735,'one animation frame is retained until playback has a cushion');
  push(1470);
  let consumed = 0;
  // Audio runs every 128 samples. A busy page alternates between 11.6 and
  // 23.2 ms batches; it supplies exactly elapsed audio time, as FF does.
  let nextPush = 512, lastPush = 0, interval = 0;
  const underruns = output.underruns;
  for (let elapsed = 0; elapsed < 44100; elapsed += 128) {
    if (elapsed >= nextPush) {
      push(elapsed - lastPush);
      lastPush = elapsed;
      nextPush += ++interval % 2 ? 1024 : 512;
    }
    const [left, right] = process();
    for (let i = 0; i < left.length; i++, consumed++) {
      assert.equal(left[i],sample(consumed));
      assert.equal(right[i],-sample(consumed));
    }
  }
  assert.equal(output.underruns,underruns,'jitter must not insert silence into the stream');
  assert.equal(output.overflows,0);
  output.port.onmessage({ data: { reset: true } });
  push(735);
  assert.ok(process()[0].every((value) => value === 0));
  assert.equal(output.buffered,735,'a restored timeline primes from its new audio');
});

describe('rendering', () => {
  for (const [label, args, expected] of [['webgpu', WEBGPU, 'webgpu'], ['webgl2 (WebGPU disabled)', NO_WEBGPU, 'webgl2']]) {
    test(`renders the test scene on ${label}`, async () => {
      const browser = await start({ args });
      try {
        const page = await browser.page();
        await page.waitForFunction(() => window.xem.status().renderer?.frames > 30);
        const { renderer, page: host } = await status(page);
        assert.equal(renderer.backend, expected);
        assert.equal(renderer.lost, null);
        assert.deepEqual(host.deviceLosses, []);
        const pixels = await canvasShot(page, `scene-${expected}.png`);
        assert.ok(pixels.colours > 200, `only ${pixels.colours} colours`);
        assert.ok(pixels.lit > 0.4, `only ${pixels.lit} of the canvas is lit`);

        // The Slint panel: shown by Escape (as a player would) and by automation.
        assert.equal(renderer.panel, 'slint');
        await page.keyboard.press('Escape');
        assert.equal(await page.evaluate(() => window.xem.panel.visible()), true);
        await page.waitForTimeout(300);
        const withPanel = await canvasShot(page, `panel-${expected}.png`);
        assert.ok(withPanel.colours > 200);
        await page.keyboard.press('Escape');
        assert.equal(await page.evaluate(() => window.xem.panel.visible()), false);

        // Show FPS must draw a counter on the canvas even with Settings closed.
        const box=await page.locator('canvas').boundingBox();
        const counter=()=>page.screenshot({clip:{x:box.x+8,y:box.y+8,width:96,height:28}});
        assert.equal(pixelStats(await counter()).lit,0,'the corner starts black');
        await page.evaluate(()=>window.xem.settings.apply({ShowFps:true}));
        await page.waitForFunction(()=>/^\d+ FPS$/.test(window.xem.status().renderer.fps));
        assert.ok(pixelStats(await counter()).lit>0.1,'the FPS counter is visible');
        await page.evaluate(()=>window.xem.settings.apply({ShowFps:false}));
        await page.waitForTimeout(100);
        assert.equal(pixelStats(await counter()).lit,0,'disabling Show FPS removes the counter');
        assert.deepEqual(page.errors, []);
      } finally {
        await browser.close();
      }
    });
  }

  test('the WebGL2 path can be chosen explicitly', async () => {
    const browser = await start({ args: WEBGPU });
    try {
      const page = await browser.page('?backend=webgl2');
      assert.equal((await status(page)).renderer.backend, 'webgl2');
    } finally {
      await browser.close();
    }
  });
});

describe('page services', () => {
  let browser;
  before(async () => {
    browser = await start({ args: NO_WEBGPU });
  });
  after(() => browser.close());

  test('settings go through the settings service and persist across reload', async () => {
    const page = await browser.page();
    const applied = await page.evaluate(() => [
      window.xem.settings.apply({ MasterVolume: 37 }),
      window.xem.settings.apply({ Scale: 'Integer' }),
      window.xem.settings.apply({ MasterVolume: 101 }),
    ]);
    assert.equal(applied[0].ok, true);
    assert.equal(applied[1].settings.presentation.scale, 'Integer');
    assert.deepEqual([applied[2].ok, applied[2].rejected], [false, 'invalid']);
    await page.reload();
    await page.waitForFunction(() => document.body.dataset.ready === 'true');
    const { settings } = await page.evaluate(() => window.xem.settings.get());
    assert.equal(settings.master_volume, 37);
    assert.equal(settings.presentation.scale, 'Integer');
    await page.close();
  });

  test('settings can be changed through the Slint panel', async () => {
    const page = await browser.page();
    await page.evaluate(() => window.xem.panel.show(true));
    await page.waitForTimeout(300);
    const panel=await settingsSection(page,1);
    const before = (await page.evaluate(() => window.xem.settings.get())).settings.presentation.show_fps;
    // The Show FPS switch, at the end of the Picture card's last row.
    await page.mouse.click(panel.left+586,panel.top+262);
    await page.waitForTimeout(200);
    await page.locator('canvas').screenshot({ path: shot('panel-click.png') });
    const after = (await page.evaluate(() => window.xem.settings.get())).settings.presentation.show_fps;
    assert.equal(after, !before);
    await page.close();
  });

  test('audio starts from a user gesture at the SPU rate', async () => {
    const page = await browser.page();
    assert.equal((await status(page)).page.audio.state, 'not started');
    await page.click('#audio');
    await page.waitForFunction(() => window.xem.status().page.audio.state === 'running');
    const { audio, page: host } = await status(page);
    assert.equal(host.audio.sampleRate, 44100);
    assert.equal(audio.sampleRate, 44100);
    // Without a session nothing is produced: the worklet plays silence.
    assert.equal(audio.frames, 0);
    await page.close();
  });

  test('saving needs a session', async () => {
    const page = await browser.page();
    await assert.rejects(page.evaluate(() => window.xem.save('slot-test')), /no game session/);
    await page.close();
  });

  test('a synthetic disc image is identified in place with a bounded cache', async () => {
    const page = await browser.page();
    const requests = [];
    page.on('request', (request) => requests.push(request.method()));
    const { image, exeSha256 } = syntheticDisc();
    await page.setInputFiles('#file', { name: 'synthetic.bin', mimeType: 'application/octet-stream', buffer: image });
    await page.waitForFunction(() => ['identified', 'failed'].includes(window.xem.status().disc?.state));
    const { disc } = await status(page);
    assert.equal(disc.state, 'identified');
    assert.equal(disc.kind, 'bin');
    assert.equal(disc.identity.disc, null);
    assert.equal(disc.identity.boot_path, 'SLUS_999.99;1');
    assert.equal(disc.identity.sha256, exeSha256);
    assert.ok(disc.peak_resident_bytes <= disc.chunk_budget);
    const sector = await page.evaluate(async () => Array.from((await window.xem.readSectors(16, 1)).subarray(24, 30)));
    assert.deepEqual(sector, [1, ...Buffer.from('CD001')]);
    assert.match(await page.textContent('#message'), /not a known Xenogears disc/);
    // Booting needs a known disc.
    await assert.rejects(page.evaluate(() => window.xem.boot()), /not a known Xenogears disc/);
    assert.ok(requests.every((method) => method === 'GET'), 'the page only fetched its own files');
    await page.close();
  });

  test('the page keeps working without a game module', async () => {
    const page = await browser.page('?game=missing/game.wasm');
    const s = await status(page);
    assert.equal(s.page.game.available, false);
    assert.ok(s.renderer.frames >= 0);
    assert.equal(await page.isDisabled('#boot'), true);
    await assert.rejects(page.evaluate(() => window.xem.boot({ executable: false })), /no game module/);
    assert.equal(page.errors.length, 0, page.errors.join('\n'));
    await page.close();
  });
});

describe('automation over a game module fixture', () => {
  let browser;
  let page;
  before(async () => {
    browser = await start({ args: NO_WEBGPU, fixtures: true });
    page = await browser.page('?game=/fixtures/fixture.wasm');
  });
  after(() => browser.close());

  const STOP = 'host stopped the game: called FixtureStub, which the port does not define';

  test('control commands: frames, run_until, memory, snapshots', async () => {
    const result = await page.evaluate(async () => {
      const xem = window.xem;
      // This baseline compares the complete command replies before/after a
      // restore. Rewind history is deliberately reset when loading a state;
      // held rewind and its retained allocations have their own tests.
      xem.settings.apply({ RewindEnabled: false });
      await xem.boot({ executable: false });
      const first = xem.step(1);
      const until = xem.runUntil({ maxFrames: 10, until: { frames: 3 } });
      const word = xem.command({ cmd: 'read_memory', address: 0x80000000, length: 4 });
      const snapshot = xem.snapshot('three');
      const halt = xem.step(1000);
      const end = { status: xem.command({ cmd: 'status' }), hash: xem.memoryHash() };
      const restored = xem.restore('three');
      const again = xem.step(1000);
      const end2 = { status: xem.command({ cmd: 'status' }), hash: xem.memoryHash() };
      const tooFar = xem.runUntil({ maxFrames: 1e9, until: { vblanks: 1e9 } });
      const unbounded = xem.command({ cmd: 'run_until', until: { vblanks: 1e9 } });
      return { first, until, word, snapshot, halt, end, restored, again, end2, tooFar, unbounded };
    });
    assert.deepEqual([result.first.frames, result.first.stopped], [1, null]);
    assert.deepEqual([result.until.met, result.until.frames, result.until.status.frames], [true, 2, 3]);
    assert.equal(result.word, '02000000');
    assert.equal(result.snapshot.name, 'three');
    assert.deepEqual(result.halt, { error: STOP });
    // Three frame waits, a hundred polls (one per frame), then the stop.
    assert.deepEqual([result.end.status.vblanks, result.end.status.stopped], [103, STOP]);
    assert.deepEqual([result.restored.frames, result.restored.stopped], [3, null]);
    assert.deepEqual(result.again, result.halt);
    assert.deepEqual(result.end2, result.end);
    assert.match(result.tooFar.error, /at most 10000 frames/);
    assert.match(result.unbounded.error, /needs max_frames/);
  });

  test('Run and automation stay blocked while a candidate boot is held, then the retained session resumes after failure', async () => {
    const previous = await page.evaluate(async () => {
      const x = window.xem;
      await x.boot({ executable: false });
      x.setRunning(true);
      await new Promise((resolve, reject) => {
        const deadline = performance.now() + 5000;
        const poll = () => {
          const s = x.status();
          if (s.session.frames >= 2) return resolve();
          if (performance.now() > deadline) return reject(new Error('fixture did not advance before the boot-race test'));
          setTimeout(poll, 10);
        };
        poll();
      });
      x.setRunning(false);
      return { generation: x.status().page.sessionGeneration, status: x.status().session, hash: x.memoryHash() };
    });

    await page.evaluate(() => {
      const original = WebAssembly.instantiate.bind(WebAssembly);
      let release;
      const gate = new Promise((resolve) => { release = resolve; });
      const race = {
        entered: false,
        pending: null,
        release(fail) { this.fail = fail; release(); },
        restore() { WebAssembly.instantiate = original; },
        fail: false,
      };
      WebAssembly.instantiate = async (...args) => {
        race.entered = true;
        await gate;
        if (race.fail) throw new Error('synthetic instantiate rejection');
        return original(...args);
      };
      race.pending = window.xem.boot({ executable: false }).then(
        () => ({ ok: true }),
        (error) => ({ error: String(error.message ?? error) }),
      );
      window.__bootRace = race;
    });
    try {
      await page.waitForFunction(() => window.__bootRace?.entered === true);
      const held = await page.evaluate(async () => {
        const x = window.xem;
        const waitFrames = () => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
        const before = { status: x.status(), hash: x.memoryHash() };
        await waitFrames();
        const afterFrames = { status: x.status(), hash: x.memoryHash() };
        let apiError;
        try { x.setRunning(true); }
        catch (error) { apiError = String(error.message ?? error); }
        let snapshotError;
        try { x.importSnapshot(new Uint8Array()); }
        catch (error) { snapshotError = String(error.message ?? error); }
        let operationError;
        try { await x.appOperation({ Action: 'Reset' }); }
        catch (error) { operationError = String(error.message ?? error); }
        return { before, afterFrames, apiError, snapshotError, operationError };
      });
      assert.equal(held.before.status.page.booting, true);
      assert.equal(held.before.status.running, false);
      assert.deepEqual(held.afterFrames.status.session.frames, held.before.status.session.frames,
        'the retained session takes no game frames while replacement boot is pending');
      assert.deepEqual(held.before.hash, { error: 'a session boot is in progress' }, 'memory commands are guarded during boot');
      assert.deepEqual(held.afterFrames.hash, held.before.hash);
      assert.equal(held.apiError, 'a session boot is in progress');
      assert.equal(held.snapshotError, 'a session boot is in progress');
      assert.equal(held.operationError, 'a session boot is in progress');

      await page.click('#run');
      assert.match(await page.locator('#message').textContent(), /session boot is in progress/);
      const afterRunClick = await page.evaluate(() => ({ status: window.xem.status(), hash: window.xem.memoryHash() }));
      assert.equal(afterRunClick.status.running, false, 'the Run button cannot restart the old session during candidate boot');
      assert.deepEqual(afterRunClick.status.session.frames, held.before.status.session.frames);
      assert.deepEqual(afterRunClick.hash, { error: 'a session boot is in progress' });

      await page.evaluate(() => window.__bootRace.release(true));
      const bootResult = await page.evaluate(() => window.__bootRace.pending);
      assert.equal(bootResult.error, 'synthetic instantiate rejection');
      const retained = await page.evaluate(() => ({ status: window.xem.status(), hash: window.xem.memoryHash() }));
      assert.equal(retained.status.page.booting, false);
      assert.equal(retained.status.page.sessionGeneration, previous.generation);
      assert.deepEqual(retained.status.session.frames, previous.status.frames);
      assert.deepEqual(retained.hash, previous.hash);

      await page.click('#run');
      await page.waitForFunction((frames) => window.xem.status().session.frames > frames, previous.status.frames);
      const resumed = await page.evaluate(() => window.xem.status());
      assert.equal(resumed.page.sessionGeneration, previous.generation, 'failed candidate boot keeps the published session');
      assert.ok(resumed.session.frames > previous.status.frames, 'Run advances the retained session after boot failure');
    } finally {
      await page.evaluate(async () => {
        if (window.__bootRace) {
          window.__bootRace.release(true);
          const pending = window.__bootRace.pending;
          window.__bootRace.restore();
          delete window.__bootRace;
          await pending;
        }
      });
    }
  });

  test('package sets stage through the browser API, reject truncation, and take effect only on a fresh boot', async () => {
    const result = await page.evaluate(async () => {
      const x = window.xem;
      const empty = Uint8Array.of(88, 69, 77, 83, 69, 84, 49, 0, 0, 0, 0, 0); // XEMSET1\0 + u32 package count 0
      const encoder = new TextEncoder();
      const u32 = (value) => Uint8Array.of(value & 255, (value >>> 8) & 255, (value >>> 16) & 255, (value >>> 24) & 255);
      const concat = (...parts) => {
        const out = new Uint8Array(parts.reduce((size, part) => size + part.length, 0));
        let at = 0;
        for (const part of parts) { out.set(part, at); at += part.length; }
        return out;
      };
      const digest = async (bytes) => new Uint8Array(await crypto.subtle.digest('SHA-256', bytes));
      const hex = (bytes) => Array.from(bytes, (byte) => byte.toString(16).padStart(2, '0')).join('');
      const payload = encoder.encode('synthetic browser authored test payload');
      const manifest = encoder.encode(JSON.stringify({
        id: 'browser-synthetic', discs: [{ serial: 'SYNTH-1', sha256: 'a'.repeat(64) }], requires: [], overrides: [],
        files: [{ disc: 'SYNTH-1', slot: 1, kind: 'authored', source_sha256: null,
          result_sha256: hex(await digest(payload)), result_size: payload.length, offset: 0, length: payload.length }],
      }));
      const packagePrefix = concat(encoder.encode('XEMMOD1\0'), u32(manifest.length), manifest, payload);
      const packageBytes = concat(packagePrefix, await digest(packagePrefix));
      const nonempty = concat(encoder.encode('XEMSET1\0'), u32(1), u32(packageBytes.length), packageBytes);
      await x.boot({ executable: false });
      const initialHash = x.memoryHash();
      x.step(1);
      const before = { identity: x.status().session.identity, generation: x.status().page.sessionGeneration,
        frames: x.status().session.frames, hash: x.memoryHash() };
      const enabled = x.packages.stage(empty, { enabled: true });
      const afterEnabled = { identity: x.status().session.identity, generation: x.status().page.sessionGeneration,
        frames: x.status().session.frames, hash: x.memoryHash() };
      let malformed, truncated;
      try { x.packages.stage(Uint8Array.of(0, 1, 2), { enabled: true }); }
      catch (error) { malformed = String(error.message ?? error); }
      try { x.packages.stage(Uint8Array.of(88, 69, 77, 83, 69, 84, 49, 0), { enabled: true }); }
      catch (error) { truncated = String(error.message ?? error); }
      const afterInvalid = { identity: x.status().session.identity, generation: x.status().page.sessionGeneration,
        frames: x.status().session.frames, hash: x.memoryHash() };
      const enabledNonempty = x.packages.stage(nonempty, { enabled: true });
      let invalidPackageBoot;
      try { await x.boot({ executable: false }); }
      catch (error) { invalidPackageBoot = String(error.message ?? error); }
      const afterRejectedPackageBoot = { identity: x.status().session.identity,
        generation: x.status().page.sessionGeneration, frames: x.status().session.frames,
        hash: x.memoryHash() };
      const retainedStep = x.step(1);
      const afterRetainedStep = { identity: x.status().session.identity,
        generation: x.status().page.sessionGeneration, frames: x.status().session.frames,
        hash: x.memoryHash() };
      const disabledNonempty = x.packages.stage(nonempty, { enabled: false });
      const disabledEmpty = x.packages.stage(empty, { enabled: false });
      const afterDisabled = { identity: x.status().session.identity, generation: x.status().page.sessionGeneration,
        frames: x.status().session.frames, hash: x.memoryHash() };
      await x.boot({ executable: false });
      return { before, enabled, afterEnabled, malformed, truncated, enabledNonempty, invalidPackageBoot,
        afterRejectedPackageBoot, retainedStep, afterRetainedStep, disabledNonempty, disabledEmpty, afterInvalid, afterDisabled,
        initialHash, fresh: { identity: x.status().session.identity, generation: x.status().page.sessionGeneration,
          frames: x.status().session.frames, hash: x.memoryHash() } };
    });
    assert.deepEqual(result.enabled.identities, []);
    assert.equal(result.enabled.pending_boot, true);
    assert.deepEqual(result.afterEnabled, result.before, 'staging does not alter the active session');
    assert.match(result.malformed, /mod package-set|truncated/i);
    assert.match(result.truncated, /truncated/i);
    assert.equal(result.enabledNonempty.identities.length, 1, 'synthetic package bytes pass structural validation while only staged');
    assert.match(result.invalidPackageBoot, /packages require an identified original disc/i);
    assert.deepEqual(result.afterRejectedPackageBoot, result.before, 'an invalid nonempty package set does not publish a new fixture session');
    assert.equal(result.retainedStep.frames, result.before.frames + 1, 'the retained old fixture still accepts a normal frame');
    assert.equal(result.retainedStep.stopped, null);
    assert.equal(result.afterRetainedStep.generation, result.before.generation);
    assert.equal(result.afterRetainedStep.frames, result.before.frames + 1);
    assert.deepEqual(result.disabledNonempty.identities, [], 'disabled sets are validated, then staged as vanilla');
    assert.deepEqual(result.disabledEmpty.identities, []);
    assert.deepEqual(result.afterInvalid, result.before, 'invalid input leaves the active fixture session intact');
    assert.deepEqual(result.afterDisabled, result.afterRetainedStep, 'disabled staging also waits for a fresh boot');
    assert.equal(result.fresh.generation, result.afterRetainedStep.generation + 1);
    assert.equal(result.fresh.frames, 0);
    assert.deepEqual(result.fresh.identity.packages, []);
    assert.equal(result.fresh.hash, result.initialHash, 'fresh deterministic boot restores the initial memory hash');
  });

  test('browser card validation refuses malformed wrappers before IndexedDB or the active card changes', async () => {
    const result = await page.evaluate(async () => {
      const x = window.xem;
      await x.boot({ executable: false });
      const image = Array.from(x.cards.image(0));
      const stored = await x.cards.stored(0);
      const hash = x.memoryHash();
      let error;
      try { await x.cards.store(0, new Uint8Array(0x20001)); }
      catch (caught) { error = String(caught.message ?? caught); }
      return { image, stored: stored ? Array.from(stored) : null, current: Array.from(x.cards.image(0)),
        afterStored: await x.cards.stored(0), hash, afterHash: x.memoryHash(), error };
    });
    assert.match(result.error, /128 KiB|package-bound card/i);
    assert.deepEqual(result.current, result.image, 'the rejected image never reaches the live card');
    assert.deepEqual(result.afterStored ? Array.from(result.afterStored) : null, result.stored, 'validation runs before IndexedDB put');
    assert.deepEqual(result.afterHash, result.hash);
  });

  test('browser card storage rejects a foreign package identity before changing memory or IndexedDB', async () => {
    const result = await page.evaluate(async () => {
      const x = window.xem;
      await x.packages.stage(Uint8Array.of(88, 69, 77, 83, 69, 84, 49, 0, 0, 0, 0, 0), { enabled: true });
      await x.boot({ executable: false });
      const before = { generation: x.status().page.sessionGeneration, identity: x.status().session.identity,
        hash: x.memoryHash(), frames: x.status().session.frames, image: Array.from(x.cards.image(0)) };
      const storedBefore = await x.cards.stored(0);

      // Synthetic save wrapper bound to a different package set. The public card
      // API must reject it before updating either its in-memory image or IndexedDB.
      const encoder = new TextEncoder();
      const raw = new Uint8Array(x.cards.image(0));
      const metadata = encoder.encode(JSON.stringify({ disc: '', packages: ['foreign-synthetic@hash'] }));
      const prefix = new Uint8Array(12 + metadata.length + raw.length);
      prefix.set(encoder.encode('XEMSAVE1'), 0);
      new DataView(prefix.buffer).setUint32(8, metadata.length, true);
      prefix.set(metadata, 12);
      prefix.set(raw, 12 + metadata.length);
      const checksum = new Uint8Array(await crypto.subtle.digest('SHA-256', prefix));
      const archive = new Uint8Array(prefix.length + checksum.length);
      archive.set(prefix);
      archive.set(checksum, prefix.length);
      let error;
      try { await x.cards.store(0, archive); }
      catch (caught) { error = String(caught.message ?? caught); }
      const storedAfter = await x.cards.stored(0);
      const after = { generation: x.status().page.sessionGeneration, identity: x.status().session.identity,
        hash: x.memoryHash(), frames: x.status().session.frames, image: Array.from(x.cards.image(0)) };
      const retainedStep = x.step(1);
      const afterStep = { generation: x.status().page.sessionGeneration,
        frames: x.status().session.frames, hash: x.memoryHash() };
      await x.boot({ executable: false });
      const fresh = { generation: x.status().page.sessionGeneration,
        identity: x.status().session.identity, frames: x.status().session.frames };
      return { error, before, after, storedBefore: storedBefore ? Array.from(storedBefore) : null,
        storedAfter: storedAfter ? Array.from(storedAfter) : null, retainedStep, afterStep, fresh };
    });
    assert.match(result.error, /card package identity mismatch/i);
    assert.deepEqual(result.after, result.before, 'foreign binding leaves the active fixture session and image untouched');
    assert.deepEqual(result.storedAfter, result.storedBefore, 'foreign binding is refused before the IndexedDB write');
    assert.equal(result.retainedStep.frames, 1);
    assert.equal(result.retainedStep.stopped, null);
    assert.equal(result.afterStep.generation, result.before.generation);
    assert.equal(result.afterStep.frames, result.before.frames + 1);
    assert.equal(result.fresh.generation, result.before.generation + 1);
    assert.deepEqual(result.fresh.identity.packages, []);
    assert.equal(result.fresh.frames, 0);

    const metadata = Buffer.from(JSON.stringify({
      disc: '',
      packages: Array.from({ length: 32 }, (_, index) => `foreign-${index}-`.padEnd(128, 'x')),
    }));
    const raw = Buffer.from(result.before.image);
    const prefix = Buffer.alloc(12 + metadata.length + raw.length);
    prefix.write('XEMSAVE1', 0, 'ascii');
    prefix.writeUInt32LE(metadata.length, 8);
    metadata.copy(prefix, 12);
    raw.copy(prefix, 12 + metadata.length);
    const archive = Buffer.concat([prefix, createHash('sha256').update(prefix).digest()]);
    assert.ok(archive.length > 0xf40 + 0x20000, 'fixture archive exceeds the old raw card-file limit');
    assert.ok(archive.length < 0x20000 + (16 << 10) + 44, 'fixture archive remains within the shared package-card limit');
    await page.setInputFiles('#import-card', {
      name: 'foreign-large.xemcard', mimeType: 'application/octet-stream', buffer: archive,
    });
    await page.waitForFunction(() => /card package identity mismatch/i.test(window.xem.status().page.operationStatus ?? ''));
    const imported = await page.evaluate(async () => ({
      status: window.xem.status(), hash: window.xem.memoryHash(), image: Array.from(window.xem.cards.image(0)),
      stored: await window.xem.cards.stored(0),
    }));
    assert.match(imported.status.page.operationStatus, /card package identity mismatch/i,
      'a large valid wrapper reaches binding validation instead of the legacy raw-file size guard');
    assert.deepEqual(imported.image, result.before.image, 'foreign import leaves the live card unchanged');
    assert.deepEqual(imported.stored ? Array.from(imported.stored) : null, result.storedBefore,
      'foreign import is rejected before its IndexedDB write');
    assert.deepEqual(imported.hash, result.before.hash, 'foreign import leaves session memory unchanged');
  });

  test('a card replacement waits for its IndexedDB write before a fresh boot can replace the session', async () => {
    const result = await page.evaluate(async () => {
      const x = window.xem;
      await x.boot({ executable: false });
      const generation = x.status().page.sessionGeneration;
      const storage = await import('/storage.js');
      const priorStored = await storage.get('cards', 'card1');
      const db = await new Promise((resolve, reject) => {
        const request = indexedDB.open('xem', 2);
        request.onsuccess = () => resolve(request.result);
        request.onerror = () => reject(request.error);
      });
      const blocker = db.transaction('cards', 'readwrite');
      const objectStore = blocker.objectStore('cards');
      let holding = true;
      const keepAlive = () => {
        if (!holding) return;
        const request = objectStore.get('__synthetic-lock__');
        request.onsuccess = keepAlive;
        request.onerror = keepAlive;
      };
      keepAlive();
      const replacement = new Uint8Array(x.cards.image(0));
      replacement[0x2000] ^= 0x5a;
      let replacementDone = false, bootDone = false;
      const replacing = x.cards.store(0, replacement).then(() => { replacementDone = true; });
      await new Promise((resolve) => setTimeout(resolve, 25));
      const booting = x.boot({ executable: false }).then(() => { bootDone = true; });
      await new Promise((resolve) => setTimeout(resolve, 50));
      const waited = !bootDone && !replacementDone && x.status().page.sessionGeneration === generation;
      holding = false;
      blocker.abort();
      try {
        await Promise.all([replacing, booting]);
        const result = { waited, generation, nextGeneration: x.status().page.sessionGeneration,
          image: Array.from(x.cards.image(0)), stored: Array.from(await x.cards.stored(0)) };
        if (priorStored === undefined) await storage.remove('cards', 'card1');
        else await storage.put('cards', 'card1', priorStored);
        return result;
      } finally { db.close(); }
    });
    assert.equal(result.waited, true, 'boot remains queued behind the in-flight card replacement');
    assert.equal(result.nextGeneration, result.generation + 1);
    assert.deepEqual(result.image, result.stored, 'the completed replacement is what the new session loads');
  });

  test('the animation loop runs due frames in bounded batches and drops missed time', async () => {
    const loop = await page.evaluate(async () => {
      const status = () => window.xem.status();
      const waitFor = (done) => new Promise((resolve) => {
        const check = () => (done() ? resolve() : setTimeout(check, 10));
        check();
      });
      await window.xem.boot({ executable: false, run: true });
      const before = status().loop;
      await waitFor(() => status().session.vblanks >= 10);
      // A one-second stall of the page (as a throttled or frozen tab sees).
      const stalled = status().session.vblanks;
      const until = performance.now() + 1000;
      while (performance.now() < until);
      await new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
      const afterStall = status().session.vblanks - stalled;
      await waitFor(() => status().session.stopped);
      const s = status();
      return { ...s.loop, before, afterStall, running: s.running, session: s.session };
    });
    assert.ok(loop.maxBatch <= loop.maxFramesPerAnimation, `batch of ${loop.maxBatch}`);
    assert.ok(loop.afterStall <= 2 * loop.maxFramesPerAnimation, `${loop.afterStall} frames after the stall`);
    assert.ok(loop.dropped - loop.before.dropped >= 50, `${loop.dropped - loop.before.dropped} dropped`);
    assert.equal(loop.gameFrames - loop.before.gameFrames, 104);
    assert.equal(loop.running, false);
    assert.deepEqual([loop.session.vblanks, loop.session.stopped], [103, STOP]);
  });

  test('the game\'s sound reaches the AudioWorklet', async () => {
    await page.click('#audio');
    const audio = await page.evaluate(async () => {
      await window.xem.boot({ executable: false, run: true });
      await new Promise((resolve) => {
        const check = () => (window.xem.status().session.stopped ? resolve() : setTimeout(check, 10));
        check();
      });
      await new Promise((resolve) => setTimeout(resolve, 300));
      const s = window.xem.status();
      return { runtime: s.audio, page: s.page.audio, vblanks: s.session.vblanks };
    });
    // 735 samples per frame at 44.1 kHz and 60 frames a second.
    assert.ok(audio.runtime.frames > 735 * 60, JSON.stringify(audio));
    assert.ok(audio.page.posted > 735 * 60, JSON.stringify(audio));
    assert.ok(audio.page.played > 4800, JSON.stringify(audio));
  });

  test('the card device\'s writes reach IndexedDB and survive a reload', async () => {
    const hex = (bytes) => Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join('');
    const text = (bytes, from, length) => String.fromCharCode(...bytes.subarray(from, from + length)).replace(/\0.*$/, '');
    const saved = await page.evaluate(async () => {
      const xem = window.xem;
      await xem.boot({ executable: false });
      // The fixture saves a file on card 1 through the BIOS file calls.
      xem.command({ cmd: 'write_memory', address: 0x80000100, hex: '01000000' });
      await xem.runFrames(2);
      const result = xem.command({ cmd: 'read_memory', address: 0x80000104, length: 8 });
      const image = xem.cards.image(0);
      await xem.cards.flush();
      const s = xem.status();
      return { result, image, stored: await xem.cards.stored(0), other: await xem.cards.stored(1),
               unsaved: s.cards.unsaved, errors: s.cards.errors, page: s.page.cards };
    });
    // Descriptor 2, 128 bytes written.
    assert.equal(saved.result, '0200000080000000');
    const image = Uint8Array.from(Object.values(saved.image));
    const stored = Uint8Array.from(Object.values(saved.stored));
    assert.equal(image.length, 128 * 1024);
    assert.equal(hex(stored), hex(image), 'IndexedDB holds the device\'s image');
    assert.equal(text(image, 0, 2), 'MC');
    // Directory frame 1: a file's first block named XEMTEST; its data in block 1.
    assert.equal(image[0x80], 0x51);
    assert.equal(text(image, 0x8a, 20), 'XEMTEST');
    assert.equal(text(image, 0x2000, 128), 'Saved by the browser host\'s card test.');
    assert.equal(saved.other, undefined, 'card 2 was never written');
    assert.deepEqual([saved.unsaved, saved.errors, saved.page.pending, saved.page.error], [false, [], 0, null]);

    await page.reload();
    await page.waitForFunction(() => document.body.dataset.ready === 'true');
    const reloaded = await page.evaluate(async () => {
      const xem = window.xem;
      const stored = await xem.cards.stored(0);
      await xem.boot({ executable: false });
      const image = xem.cards.image(0);
      // Creating the file again fails: the card the device loaded has it.
      xem.command({ cmd: 'write_memory', address: 0x80000100, hex: '01000000' });
      await xem.runFrames(1);
      return { stored, image, result: xem.command({ cmd: 'read_memory', address: 0x80000104, length: 4 }) };
    });
    assert.equal(hex(Uint8Array.from(Object.values(reloaded.stored))), hex(image));
    assert.equal(hex(Uint8Array.from(Object.values(reloaded.image))), hex(image));
    assert.equal(reloaded.result, 'ffffffff');
  });

  test('pad 1 follows the keyboard while the panel is hidden, and automation sets pads directly', async () => {
    const pads = await page.evaluate(async () => {
      await window.xem.boot({ executable: false });
      // The fixture's card path, which then waits for VSync forever (its file
      // exists by now, so it saves nothing).
      window.xem.command({ cmd: 'write_memory', address: 0x80000100, hex: '01000000' });
      window.xem.setRunning(true);
      const frames = () => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
      const buttons = () => window.xem.command({ cmd: 'status' }).pads[0].buttons;
      const out = {};
      window.xem.command({ cmd: 'pad', port: 0, buttons: ['triangle'] });
      await frames();
      out.automation = buttons();
      return out;
    });
    assert.deepEqual(pads.automation, ['triangle'], 'the player\'s idle input leaves an automation pad alone');
    await page.keyboard.down('KeyE');
    await page.keyboard.down('Enter');
    await page.waitForTimeout(100);
    const held = await page.evaluate(() => window.xem.command({ cmd: 'status' }).pads[0].buttons);
    await page.keyboard.up('KeyE');
    await page.keyboard.up('Enter');
    await page.waitForTimeout(100);
    const released = await page.evaluate(() => window.xem.command({ cmd: 'status' }).pads[0].buttons);
    await page.evaluate(() => window.xem.panel.show(true));
    await page.keyboard.down('ArrowUp');
    await page.waitForTimeout(100);
    const withPanel = await page.evaluate(() => window.xem.command({ cmd: 'status' }).pads[0].buttons);
    await page.keyboard.up('ArrowUp');
    await page.evaluate(() => window.xem.panel.show(false));
    assert.deepEqual(held, ['start', 'cross']);
    assert.deepEqual(released, []);
    assert.deepEqual(withPanel, [], 'the panel has the input');
  });

  test('fingers play the touch controls beside the picture and let go when lifted', async () => {
    const touch = await browser.page('?game=/fixtures/fixture.wasm');
    try {
      await touch.evaluate(async () => {
        await window.xem.boot({ executable: false });
        window.xem.command({ cmd: 'write_memory', address: 0x80000100, hex: '01000000' });
        window.xem.setRunning(true);
        for (let i = 0; i < 4; i++) await new Promise(requestAnimationFrame);
      });
      const finger = (type, id, control) => touch.evaluate(async ({ type, id, control }) => {
        const canvas = document.querySelector('canvas');
        const scale = window.devicePixelRatio || 1;
        const c = window.xem.status().touch.controls.find((k) => k.control === control);
        const box = canvas.getBoundingClientRect();
        const x = box.left + (c ? c.x + c.width / 2 : canvas.width / 2) / scale;
        const y = box.top + (c ? c.y + c.height / 2 : canvas.height / 2) / scale;
        canvas.dispatchEvent(new PointerEvent(type, { pointerType: 'touch', pointerId: id, clientX: x, clientY: y, bubbles: true, isPrimary: id === 1 }));
        for (let i = 0; i < 3; i++) await new Promise(requestAnimationFrame);
        const s = window.xem.status();
        return { held: s.touch.held, buttons: window.xem.command({ cmd: 'status' }).pads[0].buttons.sort(), touched: s.touch.touched };
      }, { type, id, control });
      // The first touch shows the deck (laid out from the picture's place).
      await finger('pointerdown', 9, null);
      await finger('pointerup', 9, null);
      const controls = await touch.evaluate(() => window.xem.status().touch.controls.map((c) => c.control));
      assert.ok(controls.includes('Up') && controls.includes('Confirm') && controls.includes('Settings') && controls.includes('Select'), JSON.stringify(controls));
      assert.deepEqual(await finger('pointerdown', 1, 'Up'), { held: ['Up'], buttons: ['up'], touched: true });
      assert.deepEqual(await finger('pointerdown', 2, 'Confirm'), { held: ['Up', 'Confirm'], buttons: ['cross', 'up'], touched: true });
      assert.deepEqual(await finger('pointermove', 1, 'Right'), { held: ['Right', 'Confirm'], buttons: ['cross', 'right'], touched: true }, 'a finger slides across the D-pad');
      assert.deepEqual(await finger('pointerup', 2, 'Confirm'), { held: ['Right'], buttons: ['right'], touched: true });
      assert.deepEqual(await finger('pointercancel', 1, 'Right'), { held: [], buttons: [], touched: true }, 'a cancelled touch lets go');
      // SELECT is the game's Select outside a field (in one it toggles
      // first-person inspection where the game's shows nothing).
      assert.deepEqual(await finger('pointerdown', 4, 'Select'), { held: ['Select'], buttons: ['select'], touched: true });
      assert.deepEqual(await finger('pointerup', 4, 'Select'), { held: [], buttons: [], touched: true });
      assert.equal((await touch.evaluate(() => window.xem.status().renderer.firstPerson)), false);
      // Settings is a touch away (not only Escape).
      await finger('pointerdown', 3, 'Settings');
      assert.equal(await touch.evaluate(() => window.xem.panel.visible()), true);
      assert.deepEqual(touch.errors, []);
    } finally {
      await touch.close();
    }
  });

  test('AUTO on the touch deck in a field scene turns auto-advance on and off through the settings', async () => {
    const touch = await browser.page('?game=/fixtures/fixture.wasm');
    try {
      await touch.evaluate(async () => {
        await window.xem.boot({ executable: false });
        // The fixture reports the field's mode; with no menu requested and
        // no window open, the field's context is a scene's.
        window.xem.command({ cmd: 'write_memory', address: 0x80000110, hex: '01000000' });
        window.xem.command({ cmd: 'write_memory', address: 0x800ADB64, hex: 'ff' });
        window.xem.setRunning(true);
        for (let i = 0; i < 4; i++) await new Promise(requestAnimationFrame);
      });
      const tap = (id, control) => touch.evaluate(async ({ id, control }) => {
        const canvas = document.querySelector('canvas');
        const scale = window.devicePixelRatio || 1;
        const c = window.xem.status().touch.controls.find((k) => k.control === control);
        const box = canvas.getBoundingClientRect();
        const x = box.left + (c ? c.x + c.width / 2 : canvas.width / 2) / scale;
        const y = box.top + (c ? c.y + c.height / 2 : canvas.height / 2) / scale;
        for (const type of ['pointerdown', 'pointerup']) {
          canvas.dispatchEvent(new PointerEvent(type, { pointerType: 'touch', pointerId: id, clientX: x, clientY: y, bubbles: true, isPrimary: true }));
          for (let i = 0; i < 3; i++) await new Promise(requestAnimationFrame);
        }
      }, { id, control });
      const enabled = (on) => touch.waitForFunction((on) => window.xem.settings.get().settings.auto_advance.enabled === on, on);
      // The first touch shows the deck, with AUTO in the field's scene.
      await tap(1, null);
      const controls = await touch.evaluate(() => window.xem.status().touch.controls.map((c) => c.control));
      assert.ok(controls.includes('Auto') && controls.includes('Confirm'), JSON.stringify(controls));
      await tap(2, 'Auto');
      await enabled(true);
      await touch.waitForFunction(() => window.xem.status().session.auto_advance === 120);
      assert.deepEqual(await touch.evaluate(() => window.xem.command({ cmd: 'status' }).pads[0].buttons), [], 'no press of the game\'s');
      await tap(3, 'Auto');
      await enabled(false);
      await touch.waitForFunction(() => window.xem.status().session.auto_advance === 0);
      assert.deepEqual(touch.errors, []);
    } finally {
      await touch.close();
    }
  });

  test('gamepad sticks drive movement and camera, and Back+Start owns the settings toggle', async () => {
    const controls = await browser.page('?game=/fixtures/fixture.wasm');
    try {
      await controls.evaluate(async () => {
        await window.xem.boot({ executable: false });
        window.xem.command({ cmd: 'write_memory', address: 0x80000100, hex: '01000000' });
        window.testPad = { id: 'Test gamepad', connected: true, mapping: 'standard',
          buttons: Array.from({ length: 17 }, () => ({ pressed: false })), axes: [0, 0, 0, 0] };
        Object.defineProperty(navigator, 'getGamepads', { configurable: true, value: () => [window.testPad] });
        window.xem.setRunning(true);
      });
      const sample = (buttons = [], axes = [0, 0, 0, 0], connected = true) => controls.evaluate(async ({ buttons, axes, connected }) => {
        window.testPad.buttons.forEach((button, i) => { button.pressed = buttons.includes(i); });
        window.testPad.axes = axes;
        window.testPad.connected = connected;
        for (let i = 0; i < 3; i++) await new Promise(requestAnimationFrame);
        return { buttons: window.xem.command({ cmd: 'status' }).pads[0].buttons.sort(),
          panel: window.xem.panel.visible() };
      }, { buttons, axes, connected });
      assert.deepEqual(await sample([0, 13], [-0.8, 0, 0.8, 0]),
        { buttons: ['cross', 'down', 'left', 'r1'], panel: false });
      assert.deepEqual(await sample([], [0.8, 0.8, -0.8, 0]),
        { buttons: ['down', 'l1', 'right'], panel: false });
      assert.deepEqual(await sample([], [-0.8, -0.8, 0, 0]),
        { buttons: ['left', 'up'], panel: false });
      assert.deepEqual(await sample([], [0.2, -0.2, 0.2, 1]), { buttons: [], panel: false });
      assert.deepEqual(await sample([16]), { buttons: [], panel: false }, 'Guide has no app binding');
      assert.deepEqual(await sample([8]), { buttons: ['select'], panel: false });
      assert.deepEqual(await sample([9]), { buttons: ['start'], panel: false });
      assert.deepEqual(await sample([8, 9]), { buttons: [], panel: true });
      assert.deepEqual(await sample([8, 9]), { buttons: [], panel: true }, 'holding the chord does not repeat');
      assert.deepEqual(await sample([8]), { buttons: [], panel: true });
      assert.deepEqual(await sample(), { buttons: [], panel: true });
      assert.deepEqual(await sample([8, 9]), { buttons: [], panel: false });
      assert.deepEqual(await sample([9]), { buttons: [], panel: false }, 'releasing Back first does not send Start');
      assert.deepEqual(await sample(), { buttons: [], panel: false });
      assert.deepEqual(await sample([9]), { buttons: ['start'], panel: false }, 'Start works again after releasing the chord');
      assert.deepEqual(await sample([], [0.8, 0, 0.8, 0]), { buttons: ['r1', 'right'], panel: false });
      assert.deepEqual(await sample([], [0.8, 0, 0.8, 0], false), { buttons: [], panel: false }, 'disconnect releases the sticks');
      assert.deepEqual(controls.errors, []);
    } finally {
      await controls.close();
    }
  });

  test('a snapshot save survives a reload byte for byte', async () => {
    const saved = await page.evaluate(async () => {
      await window.xem.boot({ executable: false });
      window.xem.step(2);
      return window.xem.save('fixture');
    });
    assert.equal(saved.kind, 'snapshot');
    await page.reload();
    await page.waitForFunction(() => document.body.dataset.ready === 'true');
    const loaded = await page.evaluate(async () => {
      const xem = window.xem;
      const record = await xem.load('fixture');
      await xem.boot({ executable: false });
      xem.importSnapshot(record.data);
      const at = xem.command({ cmd: 'status' }).frames;
      const next = xem.step(1).frames;
      const word = xem.command({ cmd: 'read_memory', address: 0x80000000, length: 4 });
      return { verified: record.verified, sha256: record.sha256, at, next, word };
    });
    assert.equal(loaded.verified, true);
    assert.equal(loaded.sha256, saved.sha256);
    assert.deepEqual([loaded.at, loaded.next, loaded.word], [2, 3, '02000000']);
  });
});
