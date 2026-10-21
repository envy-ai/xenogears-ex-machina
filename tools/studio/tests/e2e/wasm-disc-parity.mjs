import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { pathToFileURL } from 'node:url';

const expectedPath = process.env.XEM_STUDIO_WASM_DISC_PARITY_FIXTURE;
const fixturePackage = process.env.XEM_STUDIO_WASM_FIXTURE_PACKAGE;
const defaultPackage = process.env.XEM_STUDIO_WASM_DEFAULT_PACKAGE;
if (!expectedPath || !fixturePackage || !defaultPackage) {
  throw new Error('Set the native parity fixture and isolated WASM package paths.');
}

async function loadStudio(packageDirectory) {
  const javascript = await import(pathToFileURL(join(packageDirectory, 'xem_studio.js')).href);
  javascript.initSync({ module: await readFile(join(packageDirectory, 'xem_studio_bg.wasm')) });
  return new javascript.Studio();
}

function command(studio, op, args) {
  const response = JSON.parse(studio.command(JSON.stringify({ op, args })));
  assert.deepEqual(Object.keys(response), ['ok'], `${op} should return a success envelope`);
  return response.ok;
}

const expected = JSON.parse(await readFile(expectedPath, 'utf8'));
const image = new Uint8Array(await readFile(expected.image_path));
const fixtureStudio = await loadStudio(fixturePackage);

const opening = command(fixtureStudio, 'disc.open', {
  kind: 'pull',
  name: 'invented-fixture.bin',
  bytes: image.length,
});
assert.equal(opening.state, 'pending');
const filled = JSON.parse(fixtureStudio.disc_fill(opening.session, 0, image));
assert.deepEqual(filled, { ok: { session: opening.session, filled: image.length } });
const status = command(fixtureStudio, 'disc.poll', { session: opening.session });
const nativeStatus = expected.native_status;
assert.equal(status.session, opening.session);
assert.equal(status.state, 'ready');
assert.equal(nativeStatus.state, 'ready');
assert.equal(status.phase, nativeStatus.phase);
assert.deepEqual(status.identity, nativeStatus.identity);
assert.equal(status.bytes, nativeStatus.bytes);
assert.deepEqual(status.missing, nativeStatus.missing);
for (const field of ['chunk_bytes', 'chunk_budget', 'hunk_budget']) {
  assert.equal(status[field], nativeStatus[field], `transport policy ${field}`);
}
// The session ID, source name, and resident cache differ by transport. Assert
// each mode's values separately while comparing all disc identity/status data.
assert.equal(status.name, 'invented-fixture.bin');
assert.equal(nativeStatus.name, expected.image_path);
assert.equal(nativeStatus.resident_bytes, 0);
assert.ok(status.resident_bytes > 0 && status.resident_bytes <= status.chunk_budget);

const maps = command(fixtureStudio, 'disc.maps', { session: opening.session });
assert.equal(maps.session, opening.session);
assert.equal(maps.state, 'ready');
assert.equal(expected.maps.session, nativeStatus.session);
assert.equal(expected.maps.state, 'ready');
assert.deepEqual(maps.maps, expected.maps.maps);
const events = command(fixtureStudio, 'field.map_events', { session: opening.session, map: 0 });
assert.deepEqual(events.component, expected.component);
const placeholder = command(fixtureStudio, 'field.map_events', { session: opening.session, map: 1 });
assert.equal(placeholder.placeholder, true);
assert.equal(placeholder.component, expected.placeholder_component);

const graph = command(fixtureStudio, 'field.decode', {
  bytes: expected.component.bytes,
  declared_size: expected.component.declared_size,
});
assert.deepEqual(graph, expected.graph);
assert.deepEqual(command(fixtureStudio, 'field.encode', { graph }), expected.component.bytes);
assert.deepEqual(
  command(fixtureStudio, 'field.structure', { graph, actor: 0, event: 0 }),
  expected.structure,
);
assert.deepEqual(command(fixtureStudio, 'field.lanes', { graph }), expected.lanes);
assert.deepEqual(command(fixtureStudio, 'names.validate', {
  json: '{"schema_version":1,"variables":{"0":"HP"}}',
}), expected.names);
command(fixtureStudio, 'disc.close', { session: opening.session });
fixtureStudio.free();

const defaultStudio = await loadStudio(defaultPackage);
const defaultOpen = command(defaultStudio, 'disc.open', {
  kind: 'pull',
  name: 'invented-fixture.bin',
  bytes: image.length,
});
JSON.parse(defaultStudio.disc_fill(defaultOpen.session, 0, image));
const defaultPoll = JSON.parse(defaultStudio.command(JSON.stringify({
  op: 'disc.poll',
  args: { session: defaultOpen.session },
})));
assert.equal(defaultPoll.error?.kind, 'unknown_disc');
assert.equal(typeof defaultPoll.error?.message, 'string');
assert.ok(defaultPoll.error.message.length > 0);
defaultStudio.free();

console.log(JSON.stringify({
  native_wasm_pull_parity: true,
  binary_fill_bytes: image.length,
  maps: maps.maps.length,
  component_bytes: expected.component.bytes.length,
  declared_size: expected.component.declared_size,
  decoded_instructions: graph.instructions.length,
  event_lanes: expected.lanes.actors[0].events.length,
  default_fixture_rejected: defaultPoll.error.kind,
}));
