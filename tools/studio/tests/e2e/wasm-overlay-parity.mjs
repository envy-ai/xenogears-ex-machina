import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

const fixturePath = process.env.XEM_STUDIO_MOD_PARITY_OUTPUT;
const fixturePackage = process.env.XEM_STUDIO_WASM_FIXTURE_PACKAGE;
const defaultPackage = process.env.XEM_STUDIO_WASM_DEFAULT_PACKAGE;
if (!fixturePath || !fixturePackage || !defaultPackage) {
  throw new Error('Set the synthetic native output and isolated fixture/default WASM package paths.');
}

async function loadPackage(directory) {
  const gluePath = pathToFileURL(join(directory, 'xem_studio.js')).href;
  const wasm = await import(gluePath);
  wasm.initSync({ module: await readFile(join(directory, 'xem_studio_bg.wasm')) });
  return wasm;
}

const fixture = JSON.parse(await readFile(fixturePath, 'utf8'));
assert.equal(typeof fixture.image_path, 'string');
assert.equal(typeof fixture.set_path, 'string');
assert.equal(fixture.slot, 0xB8);
assert.equal(typeof fixture.expected, 'object');
const image = new Uint8Array(await readFile(fixture.image_path));
const packageSet = new Uint8Array(await readFile(fixture.set_path));

const fixtureWasm = await loadPackage(fixturePackage);
assert.equal(typeof fixtureWasm.package_fixture_overlay, 'function');
const actual = JSON.parse(
  fixtureWasm.package_fixture_overlay(image, packageSet, fixture.slot),
);
assert.deepEqual(actual, fixture.expected, 'native and fixture-WASM overlay summaries must match');

const defaultWasm = await loadPackage(defaultPackage);
assert.equal(
  typeof defaultWasm.package_fixture_overlay,
  'undefined',
  'default production WASM must not expose the test-fixture overlay helper',
);

console.log(JSON.stringify({
  native_wasm_package_overlay_parity: true,
  synthetic_image_bytes: image.length,
  package_set_bytes: packageSet.length,
  replacement_bytes: actual.replacement_bytes.length,
  missing_read_progress: actual.missing_read_progress,
  default_helper_exposed: false,
}));
