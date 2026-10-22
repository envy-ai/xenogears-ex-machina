import { describe, expect, it } from 'vitest';
import type { FieldGraph } from '../../src/events/model';
import { readLivePc, resolveLivePc, type LoadedField } from '../../src/game/pc';
import type { GameValue } from '../../src/game/protocol';

const bytes = [1, 2, 3, 4, 5, 6, 7, 8];
function graph(entries = [{ actor: 0, event: 0, pc: 4 }]): FieldGraph {
  return {
    schema_version: 1, source_sha256: 'a'.repeat(64), declared_size: bytes.length, stream_size: bytes.length,
    bytecode_offset: 4, variable_unsigned_bits: [], actors: [[1]], entries, skipped_zero_entries: 0,
    instructions: [{ pc: 5 } as FieldGraph['instructions'][number]], blocks: [], edges: [
      { from_block: 0, to_block: 0, from_pc: 4, to_pc: 5, kind: 'fallthrough', status: 'followed' },
    ],
    diagnostics: { overlaps: [], unpaired: [], inner: [], tables: [], into: [], outside: [], beyond_declared: [], followed_skips: 0 },
  };
}
function loaded(overrides: Partial<LoadedField> = {}): LoadedField {
  return { disc: 1, map: 1, graph: graph(), bytes, ...overrides };
}

describe('live field PC navigation', () => {
  it('masks field map gear flags, verifies context and component bytes, and resolves the tagged actor/event owner', async () => {
    const paths: string[] = [];
    const read: (command: { [key: string]: GameValue }) => Promise<GameValue> = async command => {
      const path = command.path;
      if (typeof path === 'string') paths.push(path);
      if (command.cmd === 'status') return { mode: 1, disc: 1 };
      if (command.cmd === 'read_memory') return '0102030405060708';
      switch (path) {
        case 'mode_field_map_id': return 0xc001;
        case 'field_event_actor_count': return 1;
        case 'field_current_event_actor_index': return 0;
        case 'field_view.components.descriptors[0].actor': return '0x80002000';
        case 'field_view.components.descriptors[0].actor->pc': return 5;
        case 'field_view.components.descriptors[0].actor->slot': return 0;
        case 'field_view.components.descriptors[0].actor->slots[0].tag': return 0;
        case 'field_event_package': return '0x80001000';
        case 'field_event_bytecode': return '0x80001004';
        default: throw new Error(`Unexpected fixture path: ${String(path)}`);
      }
    };
    const live = await readLivePc(read, loaded());
    expect(live).toMatchObject({ disc: 1, map: 1, actor: 0, pc: 5, event: 0, component: bytes });
    expect(paths).toContain('field_view.components.descriptors[0].actor->pc');
    expect(resolveLivePc(live, loaded())).toEqual({ actor: 0, event: 0, pc: 5 });
  });

  it('refuses wrong mode/disc/map, changed component, non-owner and aliased untagged entries', () => {
    const valid = { disc: 1, map: 1, actor: 0, pc: 5, event: 0, component: bytes, packageAddress: 0x80001000, bytecodeAddress: 0x80001004 };
    expect(() => resolveLivePc({ ...valid, disc: 2 }, loaded())).toThrow(/disc\/map differs/);
    expect(() => resolveLivePc({ ...valid, component: [...bytes, 9] }, loaded())).toThrow(/component bytes differ/);
    expect(() => resolveLivePc({ ...valid, pc: 6 }, loaded())).toThrow(/gap, inline data/);
    const aliased = loaded({ graph: graph([{ actor: 0, event: 0, pc: 4 }, { actor: 0, event: 1, pc: 4 }]) });
    expect(() => resolveLivePc({ ...valid, event: null }, aliased)).toThrow(/ambiguous/);
    expect(() => resolveLivePc({ ...valid, event: 2 }, aliased)).toThrow(/does not own/);
    expect(resolveLivePc({ ...valid, event: 1 }, aliased)).toEqual({ actor: 0, event: 1, pc: 5 });
  });

  it('bounds live actors to the loaded actor table and rejects mismatched contexts', async () => {
    const base = async (fieldCount: number, mode = 1, disc = 1, map = 1) => {
      const request: (command: { [key: string]: GameValue }) => Promise<GameValue> = async command => {
        if (command.cmd === 'status') return { mode, disc };
        if (command.path === 'mode_field_map_id') return map;
        if (command.path === 'field_event_actor_count') return fieldCount;
        return 0;
      };
      return readLivePc(request, loaded());
    };
    await expect(base(2)).rejects.toThrow(/invalid actor count/);
    await expect(base(1, 0)).rejects.toThrow(/not in field mode/);
    await expect(base(1, 1, 2)).rejects.toThrow(/differs from the game drive/);
    await expect(base(1, 1, 1, 2)).rejects.toThrow(/current map/);
  });
});
