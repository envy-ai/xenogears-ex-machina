import { describe, expect, it } from 'vitest';
import type { FieldGraph } from '../../src/events/model';
import { ExactInteger, type GameValue } from '../../src/game/protocol';
import { readScenePose } from '../../src/game/scene-pose';
import type { LoadedField } from '../../src/game/pc';
import type { GameIdentity } from '../../src/game/schema';

const moduleHash = 'a'.repeat(64);
const component = [1, 2, 3, 4, 5, 6, 7, 8];
const identity: GameIdentity = { module: moduleHash, disc: 'disc-one', packages: ['base', 'disc-one'] };

function graph(): FieldGraph {
  return {
    schema_version: 1, source_sha256: 'b'.repeat(64), declared_size: component.length, stream_size: component.length,
    bytecode_offset: 4, variable_unsigned_bits: [], actors: [[1]], entries: [{ actor: 0, event: 0, pc: 4 }],
    skipped_zero_entries: 0, instructions: [{ pc: 5 } as FieldGraph['instructions'][number]], blocks: [],
    edges: [{ from_block: 0, to_block: 0, from_pc: 4, to_pc: 5, kind: 'fallthrough', status: 'followed' }],
    diagnostics: { overlaps: [], unpaired: [], inner: [], tables: [], into: [], outside: [], beyond_declared: [], followed_skips: 0 },
  };
}

function loaded(overrides: Partial<LoadedField> = {}): LoadedField {
  return { disc: 1, map: 1, graph: graph(), bytes: component, ...overrides };
}

interface FixtureOptions {
  runtimeIdentity?: GameIdentity;
  vblanks?: GameValue;
  memory?: string;
  inspections?: Record<string, GameValue>;
}

function fixture(options: FixtureOptions = {}) {
  const commands: { [key: string]: GameValue }[] = [];
  const request = async (command: { [key: string]: GameValue }): Promise<GameValue> => {
    commands.push(command);
    if (command.cmd === 'status') {
      const runtimeIdentity = options.runtimeIdentity ?? identity;
      return {
        mode: 1, disc: 1,
        identity: { module: runtimeIdentity.module, disc: runtimeIdentity.disc, packages: [...runtimeIdentity.packages] },
        vblanks: options.vblanks ?? 42,
      };
    }
    if (command.cmd === 'read_memory') return options.memory ?? '0102030405060708';
    if (command.cmd !== 'inspect' || typeof command.path !== 'string') throw new Error(`Unexpected request: ${String(command.cmd)}`);
    const path = command.path;
    if (options.inspections && Object.hasOwn(options.inspections, path)) return options.inspections[path]!;
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
      case 'field_view.components.descriptors[0].actor->position[0]': return -65536;
      case 'field_view.components.descriptors[0].actor->position[1]': return 32768;
      case 'field_view.components.descriptors[0].actor->position[2]': return -1;
      case 'field_view.components.descriptors[0].actor->heading': return -123;
      default: throw new Error(`Unexpected inspect path: ${path}`);
    }
  };
  return { request, commands };
}

const source = { entity: 'actor:0', actor: 0, identity, loaded: loaded() };

describe('read-only live Scene pose qualification', () => {
  it('requires exact module, disc family, and package order before reading runtime state', async () => {
    const cases: GameIdentity[] = [
      { ...identity, module: 'c'.repeat(64) },
      { ...identity, disc: 'disc-two' },
      { ...identity, packages: [...identity.packages].reverse() },
    ];
    for (const runtimeIdentity of cases) {
      const { request, commands } = fixture({ runtimeIdentity });
      await expect(readScenePose(request, source)).rejects.toThrow(/module, disc family or package order differs/);
      expect(commands).toHaveLength(1);
      expect(commands[0]?.cmd).toBe('status');
    }
  });

  it('checks exact component bytes and reports signed fixed-point position and exact vblank', async () => {
    const { request, commands } = fixture({ vblanks: new ExactInteger('9007199254740993') });
    const pose = await readScenePose(request, source);
    expect(pose).toEqual({
      entity: 'actor:0', actor: 0, map: 1, disc: 1,
      identity, vblank: '9007199254740993', raw_position: [-65536, 32768, -1],
      position: [-1, 0.5, -1 / 65536], heading_raw: -123,
    });
    expect(commands.map(command => command.cmd).every(cmd => ['status', 'inspect', 'read_memory'].includes(String(cmd)))).toBe(true);
    expect(commands.some(command => String(command.cmd).startsWith('write'))).toBe(false);

    const maximum = fixture({ vblanks: new ExactInteger('18446744073709551615') });
    await expect(readScenePose(maximum.request, source)).resolves.toMatchObject({ vblank: '18446744073709551615' });

    const changed = fixture({ memory: '01020304050607ff' });
    await expect(readScenePose(changed.request, source)).rejects.toThrow(/component bytes differ/);
  });

  it('rejects malformed signed position, heading, and vblank values', async () => {
    const signedCases: Array<[string, GameValue]> = [
      ['field_view.components.descriptors[0].actor->position[0]', -(2 ** 31) - 1],
      ['field_view.components.descriptors[0].actor->position[1]', 2 ** 31],
      ['field_view.components.descriptors[0].actor->position[2]', 0.5],
      ['field_view.components.descriptors[0].actor->heading', 2 ** 15],
      ['field_view.components.descriptors[0].actor->heading', -(2 ** 15) - 1],
    ];
    for (const [path, value] of signedCases) {
      const { request } = fixture({ inspections: { [path]: value } });
      await expect(readScenePose(request, source)).rejects.toThrow(/invalid signed/);
    }
    for (const vblanks of [-1, 0.25, new ExactInteger('-2'), new ExactInteger('18446744073709551616')]) {
      const { request } = fixture({ vblanks });
      await expect(readScenePose(request, source)).rejects.toThrow(/invalid runtime vblank/);
    }
  });

  it('rejects an unqualified source before issuing runtime requests', async () => {
    const { request, commands } = fixture();
    await expect(readScenePose(request, { ...source, identity: { ...identity, module: 'unbuilt' } })).rejects.toThrow(/identified source\/module/);
    expect(commands).toEqual([]);
  });
});
