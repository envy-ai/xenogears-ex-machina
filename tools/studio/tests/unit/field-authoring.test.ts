import { describe, expect, it } from 'vitest';
import { FieldAuthoring, sameSceneSource } from '../../src/backend/field-authoring';
import type { DraftAction, DraftDocument } from '../../src/backend/drafts';
import type { SceneDocument, SceneInput, SceneSnapshot } from '../../src/backend/scenes';
import type { SourceIdentity } from '../../src/backend/discs';
import type { Backend } from '../../src/backend/service';

class ScriptedBackend implements Backend {
  readonly requests: { op: string; args: Record<string, unknown> }[] = [];
  readonly replies: string[] = [];
  command(json: string): string {
    this.requests.push(JSON.parse(json) as { op: string; args: Record<string, unknown> });
    const response = this.replies.shift();
    if (response === undefined) throw new Error('unexpected backend request');
    return response;
  }
  respond(value: unknown): void { this.replies.push(JSON.stringify({ ok: value })); }
  reject(kind: string, message: string): void { this.replies.push(JSON.stringify({ error: { kind, message } })); }
}

function compiledDraft(revision: number, draft = 41) {
  return {
    draft, revision, graph: { entries: [], instructions: [], edges: [], diagnostics: [] } as never,
    bindings: { 'pc:4': 4 }, can_undo: revision > 0, can_redo: false, modified: revision > 0,
  };
}
function sceneState(revision: number, actors: SceneSnapshot['actors'] = []): SceneSnapshot {
  return {
    revision, can_undo: revision > 0, can_redo: false, modified: revision > 0,
    descriptors: [], actors, zones: [], collision: {} as SceneSnapshot['collision'],
    collision_entities: { layers: [] }, entrances: [], source_sha256: 'source-hash',
    events: { bytes: [14], declared_size: 1, graph: { entries: [], instructions: [], edges: [], diagnostics: [] } as never, bindings: { 'pc:4': 4 } },
    next_event_id: 1, reference_diagnostics: [],
  };
}
function sceneReply(scene: number, state: SceneSnapshot) { return { scene, state }; }

const source: SourceIdentity = {
  disc: 1, boot_sha256: 'boot', map: 23, slot: 652, lba: 124528,
  indexed_size: 110052, source_sha256: 'bundle-source',
};
const input: SceneInput = { bundle: [3, 0, 1, 4], source, context: null, module_sha256: null };
const emptyM6: DraftDocument = {
  schema_version: 1, base: { entries: [], instructions: [], edges: [], diagnostics: [], source_sha256: 'event-source' } as never,
  actions: [], cursor: 0, allocation_starts: [], next_id: 1,
};
const sceneDocument = (sceneInput: SceneInput): SceneDocument => ({
  schema_version: 4, input: sceneInput, event_document: emptyM6, entrance_selectors: [],
  actions: [], allocation_starts: [], cursor: 0, next_id: 1,
  event_allocation_starts: [], next_event_id: 1, imported_event_actions: 0,
});

const actors: SceneSnapshot['actors'] = [
  { id: 'scene:source:actor:0', event_source_row: 0, origin: { kind: 'original', actor: 0 } },
  { id: 'scene:copy:actor:1', event_source_row: 5, origin: { kind: 'independent_copy', template: 'scene:source:actor:0' } },
  { id: 'scene:copy:actor:2', event_source_row: 9, origin: { kind: 'independent_copy', template: 'scene:source:actor:0' } },
];

describe('FieldAuthoring owner adapter', () => {
  it('translates NewEvent for every active runtime actor, including batches after row compaction', () => {
    const backend = new ScriptedBackend();
    backend.respond(compiledDraft(0, 12));
    const legacy = FieldAuthoring.open(backend, {} as never);
    expect(legacy.scene).toBeNull();
    expect(legacy.snapshot.draft).toBe(12);
    expect(() => legacy.editScene({ kind: 'delete_actor', id: 'scene:copy:actor:1' })).toThrow(/source-qualified Scene/);

    const sceneBackend = new ScriptedBackend();
    sceneBackend.respond(sceneReply(73, sceneState(0, actors)));
    const owner = FieldAuthoring.scene(sceneBackend, input, []);
    expect(owner.scene?.scene).toBe(73);
    expect(owner.snapshot.draft).toBe(-73);
    const action: DraftAction = {
      kind: 'batch', actions: [
        { kind: 'new_event', actor: 0, event: 1, nodes: [{ bytes: [14] }] },
        { kind: 'new_event', actor: 1, event: 2, nodes: [{ bytes: [14] }] },
        { kind: 'new_event', actor: 2, event: 3, nodes: [{ bytes: [14] }] },
        { kind: 'set_operand', node: 'pc:4', operand: 0, value: { kind: 'literal', value: 7 } },
      ],
    };
    const updated = sceneState(1, actors);
    updated.events.bindings = { 'pc:4': 4, 'new:1': 5 };
    sceneBackend.respond(sceneReply(73, updated));
    const returned = owner.edit(action);
    expect(returned).toMatchObject({ draft: -73, revision: 1, bindings: { 'pc:4': 4, 'new:1': 5 } });
    expect(owner.snapshot).toEqual(returned);
    expect(sceneBackend.requests[1]).toEqual({ op: 'field.scene.draft', args: {
      kind: 'edit', scene: 73, revision: 0, action: {
        kind: 'batch', actions: [
          { kind: 'actor_event', actor: actors[0].id, event: 1, nodes: [{ bytes: [14] }] },
          { kind: 'actor_event', actor: actors[1].id, event: 2, nodes: [{ bytes: [14] }] },
          { kind: 'actor_event', actor: actors[2].id, event: 3, nodes: [{ bytes: [14] }] },
          { kind: 'event', action: { kind: 'set_operand', node: 'pc:4', operand: 0, value: { kind: 'literal', value: 7 } } },
        ],
      },
    } });
  });

  it('derives snapshot, history and the active event graph from the sole scene owner', () => {
    const backend = new ScriptedBackend();
    const before = sceneState(3, actors);
    before.events.bindings = { 'pc:4': 4, 'new:1': 7 };
    backend.respond(sceneReply(22, before));
    const owner = FieldAuthoring.scene(backend, input, []);
    expect(owner.scene?.snapshot).toEqual(before);
    expect(owner.snapshot).toMatchObject({ draft: -22, revision: 3, bindings: { 'new:1': 7 } });

    const after = sceneState(4, actors);
    after.events.bindings = { 'pc:4': 4, 'new:1': 7, 'new:2': 8 };
    backend.respond(sceneReply(22, after));
    const applied = owner.editScene({ kind: 'set_zone', id: 'zone:0', corners: [[0, 0, 0], [1, 0, 0], [1, 0, 1], [0, 0, 1]] });
    expect(applied).toMatchObject({ draft: -22, revision: 4, bindings: after.events.bindings });
    expect(owner.scene?.snapshot.events.bindings).toEqual(after.events.bindings);
    expect(backend.requests.map(request => request.op)).toEqual(['field.scene.draft', 'field.scene.draft']);
    expect(backend.requests[1].args).toMatchObject({ kind: 'edit', scene: 22, revision: 3 });

    const undone = sceneState(5, actors);
    undone.events.bindings = before.events.bindings;
    backend.respond(sceneReply(22, undone));
    expect(owner.history('undo')).toMatchObject({ draft: -22, revision: 5, bindings: before.events.bindings });
    expect(owner.scene?.snapshot).toEqual(undone);
    expect(backend.requests[2].args).toEqual({ kind: 'undo', scene: 22, revision: 4 });
  });

  it('repackages only the exact original bundle and treats source identity as a field set', () => {
    const backend = new ScriptedBackend();
    const originalInput = { ...input, bundle: [...input.bundle] };
    backend.respond(sceneReply(18, sceneState(0)));
    const owner = FieldAuthoring.scene(backend, originalInput, []);
    const exported = sceneDocument(originalInput);
    backend.respond(exported);
    const compiled = {
      bundle: [3, 0, 9, 4], bundle_sha256: 'compiled-sha', source_sha256: 'bundle-source',
      descriptor_ids: ['descriptor:0'], zone_ids: ['zone:0'], events: sceneState(0).events,
    };
    backend.respond(compiled);
    const reorderedIdentity: SourceIdentity = {
      source_sha256: source.source_sha256, indexed_size: source.indexed_size, lba: source.lba,
      slot: source.slot, map: source.map, boot_sha256: source.boot_sha256, disc: source.disc,
    };
    expect(sameSceneSource(source, reorderedIdentity)).toBe(true);
    expect(owner.repack([...originalInput.bundle], undefined, reorderedIdentity)).toEqual({
      bytes: compiled.bundle, source_sha256: compiled.source_sha256,
    });
    expect(backend.requests.slice(1)).toEqual([
      { op: 'field.scene.draft', args: { kind: 'export', scene: 18 } },
      { op: 'field.scene.draft', args: { kind: 'compile', scene: 18 } },
    ]);

    const adopted = owner.scene?.snapshot;
    backend.respond(exported);
    expect(() => owner.repack([3, 0, 99, 4], undefined, reorderedIdentity)).toThrow(/differs from the selected original/);
    expect(owner.scene?.snapshot).toBe(adopted);
    backend.respond(exported);
    expect(() => owner.repack([...originalInput.bundle], undefined, { ...source, map: 22 })).toThrow(/differs from the selected original/);
    expect(owner.scene?.snapshot).toBe(adopted);
    expect(backend.requests).toHaveLength(5);
    expect(backend.requests.slice(-2).map(request => request.args.kind)).toEqual(['export', 'export']);
    expect(backend.requests.filter(request => request.args.kind === 'compile')).toHaveLength(1);
  });
});
