// Capture the exact vanilla map-23 component decode context independently on
// the production browser and native hosts. Outputs remain in ignored M6 leaves.
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve, sep } from 'node:path';
import { test } from 'node:test';
import { HEADLESS, NO_WEBGPU, ROOT, WEB, start } from '../../../../src/runtime/web/tests/support.mjs';

const enabled = process.env.XEM_STUDIO_M6_CAPTURE_CONTEXT === '1';
const milestoneRoot = resolve(ROOT, 'tmp/studio/milestone-6');

test('gated native/browser map-23 capture agrees on measured original component context', {
  skip: !enabled && 'set XEM_STUDIO_M6_CAPTURE_CONTEXT=1 to use explicit private M6 inputs',
  timeout: 1_200_000,
}, async () => {
  const discPath = requiredPath('XEM_STUDIO_M6_DISC1');
  const extractRoot = requiredPath('XEM_STUDIO_EXTRACT_ROOT');
  const outputRoot = requiredPath('XEM_STUDIO_M6_CONTEXT_OUTPUT_ROOT');
  assert.ok(outputRoot.startsWith(milestoneRoot + sep));
  assert.equal(discPath, resolve(join(extractRoot, 'discs/disc1.bin')));
  assert.ok(existsSync(discPath) && existsSync(HEADLESS));
  assert.ok(existsSync(join(WEB, 'game/game.wasm')) && existsSync(join(WEB, 'game/schema.json')));
  assert.ok(existsSync(dirname(outputRoot)) && !existsSync(outputRoot), 'output leaf is fresh');
  const original = JSON.parse(execFileSync('python3', ['-c', [
    'import json, sys',
    'from pathlib import Path',
    'from tools.analysis.disc_index import Disc',
    'disc = Disc(1, Path(sys.argv[1]).parent)',
    'slot = disc.slot(4, 0, 0xB8 + 2 * 23)',
    'entry = disc.entries[slot]',
    'data = disc.data(slot)',
    'import hashlib',
    'print(json.dumps({"slot":slot,"lba":entry["lba"],"size":entry["size"],"source_sha256":hashlib.sha256(data).hexdigest(),"boot_sha256":hashlib.sha256(disc.boot).hexdigest(),"serial":"SLUS-00664"}))',
  ].join('\n'), extractRoot], { cwd: ROOT, encoding: 'utf8', timeout: 120_000 }));
  const sourceHash = original.source_sha256;
  assert.match(sourceHash, /^[0-9a-f]{64}$/);
  mkdirSync(outputRoot, { recursive: false });

  const browser = await start({ args: NO_WEBGPU });
  try {
    const page = await browser.page();
    await page.setInputFiles('#file', discPath);
    await page.waitForFunction(() => ['identified', 'failed'].includes(window.xem.status().disc?.state), null, { timeout: 120_000 });
    const disc = await page.evaluate(() => window.xem.status().disc);
    assert.equal(disc.state, 'identified');
    assert.equal(disc.identity.serial, original.serial);
    assert.equal(disc.identity.sha256, original.boot_sha256);
    await page.evaluate(async () => {
      await window.xem.boot({ run: false, rewind: false });
      const command = value => JSON.parse(window.xem.commandJson(JSON.stringify(value)));
      const boot = command({ cmd: 'boot_mode', mode: 0 });
      if (boot.error) throw new Error('browser title boot failed');
      const title = await window.xem.run({ cmd: 'frames', count: 303 });
      if (title.error) throw new Error('browser title frames failed');
      const field = command({ cmd: 'scenario', mode: 1, writes: [
        { path: 'game_data.map', value: 23 },
        { path: 'mode_kernel_menu_running', value: 0 },
        { path: 'mode_field_entered_once', value: 0 },
      ] });
      if (field.error) throw new Error('browser map-23 route failed');
      const frames = await window.xem.run({ cmd: 'frames', count: 430 });
      if (frames.error) throw new Error('browser map-23 frames failed');
    });
    const browserContext = await page.evaluate((sha) => JSON.parse(window.xem.commandJson(JSON.stringify({
      cmd: 'field_source_context', map: 23, source_sha256: sha,
    }))), sourceHash);
    assert.equal(browserContext.error, undefined, 'browser publishes measured vanilla map context');

    const commands = [
      { cmd: 'schema' },
      { cmd: 'boot_mode', mode: 0 },
      { cmd: 'frames', count: 303 },
      { cmd: 'scenario', mode: 1, writes: [
        { path: 'game_data.map', value: 23 },
        { path: 'mode_kernel_menu_running', value: 0 },
        { path: 'mode_field_entered_once', value: 0 },
      ] },
      { cmd: 'frames', count: 430 },
      { cmd: 'field_source_context', map: 23, source_sha256: sourceHash },
    ];
    const nativeLines = execFileSync(HEADLESS, ['--disc', discPath, '--control'], {
      cwd: ROOT,
      input: commands.map(command => JSON.stringify(command)).join('\n') + '\n',
      encoding: 'utf8', maxBuffer: 1 << 26, timeout: 900_000,
    }).trim().split('\n');
    assert.equal(nativeLines.length, commands.length, 'native control replied to every bounded command');
    const nativeReplies = nativeLines.map(line => JSON.parse(line));
    for (let i = 0; i < nativeReplies.length - 1; i++) {
      assert.equal(nativeReplies[i].error, undefined, `native route command ${i} succeeded`);
    }
    const nativeContext = nativeReplies.at(-1);
    assert.equal(nativeContext.error, undefined, 'native host publishes measured vanilla map context');
    const browserContextBytes = Buffer.from(JSON.stringify(browserContext, null, 2) + '\n');
    const nativeContextBytes = Buffer.from(JSON.stringify(nativeContext, null, 2) + '\n');
    const browserContextSha256 = createHash('sha256').update(browserContextBytes).digest('hex');
    const nativeContextSha256 = createHash('sha256').update(nativeContextBytes).digest('hex');
    writeFileSync(join(outputRoot, 'browser-source-context.json'), browserContextBytes, { flag: 'wx' });
    writeFileSync(join(outputRoot, 'native-source-context.json'), nativeContextBytes, { flag: 'wx' });
    assert.equal(browserContextSha256, nativeContextSha256,
      'native and browser complete measured-context digests agree; see private per-host captures');
    assert.equal(browserContext.schema_version, 1);
    assert.equal(browserContext.provenance, 'runtime_field_decompress');
    assert.equal(browserContext.identity.source_sha256, sourceHash);
    assert.equal(browserContext.identity.map, 23);
    assert.equal(browserContext.components.length, 9);

    const contextBytes = browserContextBytes;
    const contextPath = join(outputRoot, 'source-context.json');
    writeFileSync(contextPath, contextBytes, { flag: 'wx' });
    const evidence = {
      acceptance: 'gated production native/browser measured-context capture; no authored context bytes or runtime fixture bypass',
      sourceBundleSha256: sourceHash,
      sourceIdentity: { disc: 1, map: 23, slot: original.slot, lba: original.lba, indexedSize: original.size, bootSha256: original.boot_sha256 },
      moduleSha256: browserContext.module_sha256,
      contextSha256: createHash('sha256').update(contextBytes).digest('hex'),
      nativeContextSha256,
      contextPath,
      componentCount: browserContext.components.length,
      componentOutputSha256: Object.fromEntries(browserContext.components.map(component => [component.index, component.output_sha256])),
      browserAndNativeContextsEqual: true,
      route: { titleFrames: 303, map: 23, fieldFrames: 430 },
    };
    writeFileSync(join(outputRoot, 'evidence.json'), JSON.stringify(evidence, null, 2) + '\n', { flag: 'wx' });
  } finally {
    await browser.close();
  }
});

function requiredPath(name) {
  const value = process.env[name];
  assert.ok(value, `${name} must explicitly name a private M6 path`);
  return resolve(value);
}
