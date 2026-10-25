import { describe, expect, it } from 'vitest';
import { SceneDraftSession, type SceneSnapshot } from '../../src/backend/scenes';
import { StudioCommandError, type Backend } from '../../src/backend/service';
import type { DraftDocument } from '../../src/backend/drafts';

class ScriptedBackend implements Backend {
  readonly requests: unknown[] = [];
  readonly replies: string[] = [];
  command(json: string): string {
    this.requests.push(JSON.parse(json));
    const reply = this.replies.shift();
    if (reply === undefined) throw new Error('unexpected backend request');
    return reply;
  }
  respond(value: unknown): void { this.replies.push(JSON.stringify({ ok: value })); }
  reject(kind: string, message: string): void {
    this.replies.push(JSON.stringify({ error: { kind, message } }));
  }
}

function state(revision = 0): SceneSnapshot {
  return {
    revision, can_undo: false, can_redo: false, modified: false,
    descriptors: [], actors: [], zones: [], collision: {} as SceneSnapshot['collision'],
    collision_entities: { layers: [] }, entrances: [], source_sha256: 'source',
    events: { bytes: [], declared_size: 0, graph: {} as never, bindings: { 'pc:10': 10, 'new:1': 11 } },
    next_event_id: 2, reference_diagnostics: [], sprite_bindings: [],
  };
}

function reply(scene: number, snapshot: SceneSnapshot): { scene: number; state: SceneSnapshot } {
  return { scene, state: snapshot };
}

const input = { bundle: [1, 2, 3], source: null, context: null, module_sha256: null };
const eventDocument = {
  schema_version: 1, base: {} as DraftDocument['base'],
  actions: [{ kind: 'new_event', actor: 0, event: 1, nodes: [{ bytes: [14] }] }],
  cursor: 1, allocation_starts: [1], next_id: 2,
} satisfies DraftDocument;

describe('scene draft transport', () => {
  it('forwards the M6 document and typed actions, adopting only successful snapshots', () => {
    const backend = new ScriptedBackend();
    backend.respond(reply(7, state(0)));
    const session = SceneDraftSession.open(backend, input, [], eventDocument);
    const open = backend.requests[0] as { op: string; args: Record<string, unknown> };
    expect(open).toEqual({ op: 'field.scene.draft', args: {
      kind: 'open', input, entrances: [], event_document: eventDocument,
    } });

    const action = { kind: 'event', action: {
      kind: 'set_operand', node: 'pc:10', operand: 1,
      value: { kind: 'target', node: 'new:1' },
    } } as const;
    const updated = state(1);
    updated.can_undo = true;
    backend.respond(reply(7, updated));
    const adopted = session.edit(action);
    expect(adopted).toEqual(updated);
    expect(session.snapshot).toBe(adopted);
    expect(backend.requests[1]).toEqual({ op: 'field.scene.draft', args: {
      kind: 'edit', scene: 7, revision: 0, action,
    } });

    const undone = state(2);
    undone.can_redo = true;
    backend.respond(reply(7, undone));
    expect(session.history('undo')).toEqual(undone);
    expect(backend.requests[2]).toEqual({ op: 'field.scene.draft', args: {
      kind: 'undo', scene: 7, revision: 1,
    } });
  });

  it('retains the previous handle snapshot when an edit returns a protocol error', () => {
    const backend = new ScriptedBackend();
    const original = state(4);
    backend.respond(reply(9, original));
    const session = SceneDraftSession.import(backend, {} as never);
    const adopted = session.snapshot;
    expect(backend.requests[0]).toEqual({ op: 'field.scene.draft', args: { kind: 'import', document: {} } });

    backend.reject('stale_revision', 'revision changed');
    expect(() => session.edit({ kind: 'delete_zone', id: 'zone:1' })).toThrow(StudioCommandError);
    expect(session.snapshot).toBe(adopted);
    backend.respond(reply(9, state(5)));
    session.edit({ kind: 'delete_zone', id: 'zone:1' });
    expect(backend.requests[2]).toMatchObject({ op: 'field.scene.draft', args: { scene: 9, revision: 4 } });
  });

  it('keeps scene handles separate and forwards placement, compile, export and close', () => {
    const backend = new ScriptedBackend();
    const first = state(2);
    const second = state(0);
    backend.respond(reply(20, first));
    backend.respond(reply(21, second));
    const a = SceneDraftSession.open(backend, input, []);
    const b = SceneDraftSession.import(backend, {} as never);
    expect(a.scene).toBe(20);
    expect(b.scene).toBe(21);
    expect(a.nodeAt(10)).toBe('pc:10');
    expect(a.nodeAt(11)).toBe('new:1');
    expect(a.nodeAt(99)).toBeNull();

    const placement = { node: 'pc:10', pc: 10, handler: 'place', position: [{ kind: 'immediate', value: 1 }, { kind: 'immediate', value: 2 }, { kind: 'immediate', value: 3 }], owners: [], can_transform: true, diagnostics: [] };
    backend.respond(placement);
    expect(a.placement('pc:10')).toEqual(placement);
    expect(backend.requests[2]).toEqual({ op: 'field.scene.draft', args: { kind: 'placement', scene: 20, node: 'pc:10' } });

    const compiled = { bundle: [4], bundle_sha256: 'bundle', source_sha256: 'source', descriptor_ids: [], zone_ids: [], events: first.events };
    backend.respond(compiled);
    expect(a.compile()).toEqual(compiled);
    expect(backend.requests[3]).toEqual({ op: 'field.scene.draft', args: { kind: 'compile', scene: 20 } });

    const emptyRoot = { schema_version: 1, base: eventDocument.base, actions: [], cursor: 0, allocation_starts: [], next_id: 1 };
    const document = { schema_version: 4, input, event_document: emptyRoot, entrance_selectors: [], actions: [], allocation_starts: [], cursor: 0, next_id: 1, event_allocation_starts: [], next_event_id: 1, imported_event_actions: 0 };
    backend.respond(document);
    expect(a.export()).toEqual(document);
    backend.respond({ closed: true });
    a.close();
    expect(backend.requests.slice(4)).toEqual([
      { op: 'field.scene.draft', args: { kind: 'export', scene: 20 } },
      { op: 'field.scene.draft', args: { kind: 'close', scene: 20 } },
    ]);
  });
});
