import { reachableInstructionPcs, type FieldGraph, type Selection } from '../events/model';
import { gameJson, record, type GameValue } from './protocol';

export interface LoadedField {
  disc: number | null; map: number | null; graph: FieldGraph | null; bytes: readonly number[] | null;
}
export interface LivePc {
  disc: number; map: number; actor: number; pc: number; event: number | null;
  component: number[]; packageAddress: number; bytecodeAddress: number;
}
export function integer(value: GameValue, label: string, maximum = 0xffff_ffff): number {
  if (typeof value !== 'number' || !Number.isSafeInteger(value) || value < 0 || value > maximum) throw new Error(`Unavailable: invalid ${label}`);
  return value;
}
function pointer(value: GameValue, label: string): number {
  if (typeof value !== 'string' || !/^0x[0-9a-f]{8}$/i.test(value)) throw new Error(`Unavailable: invalid ${label}`);
  const address = Number.parseInt(value.slice(2), 16);
  if (address < 0x8000_0000 || address >= 0x8020_0000) throw new Error(`Unavailable: ${label} is not in game RAM`);
  return address;
}
/** Reads use authoritative schema paths, never a separately maintained actor layout. */
export async function readLivePc(request: (command: { [key: string]: GameValue }) => Promise<GameValue>, loaded: LoadedField, actor?: number): Promise<LivePc> {
  if (!loaded.graph || loaded.map === null || loaded.disc === null || !loaded.bytes) throw new Error('Unavailable: load a recognized disc map and event component first');
  const status = record(await request({ cmd: 'status' }), 'Live status');
  if (status.mode !== 1) throw new Error('Unavailable: the attached game is not in field mode');
  const disc = integer(status.disc, 'inserted disc', 2);
  if (disc !== loaded.disc) throw new Error('Unavailable: loaded Studio disc differs from the game drive');
  const inspect = (path: string) => request({ cmd: 'inspect', path, depth: 0 });
  // The original loader indexes the low 12 bits; 0xC000 marks gear transitions.
  const map = integer(await inspect('mode_field_map_id'), 'field map flags', 0xffff) & 0x0fff;
  if (map !== loaded.map) throw new Error('Unavailable: load the attached game’s current map in Studio');
  const count = integer(await inspect('field_event_actor_count'), 'actor count', loaded.graph.actors.length);
  if (count !== loaded.graph.actors.length) throw new Error('Unavailable: live actor count differs from the decoded component');
  const index = actor ?? integer(await inspect('field_current_event_actor_index'), 'current actor', count - 1);
  if (!Number.isSafeInteger(index) || index < 0 || index >= count) throw new Error('Unavailable: actor is outside the active field');
  const base = `field_view.components.descriptors[${index}].actor`;
  pointer(await inspect(base), 'actor pointer');
  const pc = integer(await inspect(`${base}->pc`), 'actor PC', 0xffff);
  const slot = integer(await inspect(`${base}->slot`), 'actor script slot', 7);
  const tag = integer(await inspect(`${base}->slots[${slot}].tag`), 'event tag', 0xff);
  const packageAddress = pointer(await inspect('field_event_package'), 'event package');
  const bytecodeAddress = pointer(await inspect('field_event_bytecode'), 'event bytecode');
  if (packageAddress + loaded.graph.bytecode_offset !== bytecodeAddress) throw new Error('Unavailable: live event header/bytecode boundary differs from loaded component');
  const component: number[] = [];
  const length = loaded.graph.declared_size;
  if (packageAddress + length > 0x8020_0000) throw new Error('Unavailable: live component extends outside game RAM');
  for (let offset = 0; offset < length; offset += 0x10000) {
    const hex = await request({ cmd: 'read_memory', address: packageAddress + offset, length: Math.min(0x10000, length - offset) });
    if (typeof hex !== 'string' || !/^(?:[0-9a-f]{2})*$/i.test(hex) || hex.length !== Math.min(0x10000, length - offset) * 2) throw new Error('Unavailable: incomplete live component read');
    for (let i = 0; i < hex.length; i += 2) component.push(Number.parseInt(hex.slice(i, i + 2), 16));
  }
  return { disc, map, actor: index, pc, event: tag < 32 ? tag : null, component, packageAddress, bytecodeAddress };
}
/** Shared source qualification; pose reads do not require an active script event. */
export function assertLiveComponent(live: LivePc, loaded: LoadedField): FieldGraph {
  const graph = loaded.graph;
  if (!graph || !loaded.bytes || loaded.map === null || loaded.disc === null) throw new Error('Unavailable: no decoded event component is loaded');
  if (live.disc !== loaded.disc || live.map !== loaded.map) throw new Error('Unavailable: live disc/map differs from the loaded component');
  if (live.component.length !== graph.declared_size || live.component.some((byte, index) => byte !== loaded.bytes![index])) throw new Error('Unavailable: live event component bytes differ from the loaded map');
  return graph;
}
export function resolveLivePc(live: LivePc, loaded: LoadedField): Selection {
  const graph = assertLiveComponent(live, loaded);
  if (!graph.instructions.some(instruction => instruction.pc === live.pc)) throw new Error('Unavailable: live PC is in a gap, inline data, or an undecoded instruction');
  const owners = graph.entries.filter(entry => entry.actor === live.actor && reachableInstructionPcs(graph, entry.pc).has(live.pc));
  if (live.event !== null) {
    if (!owners.some(owner => owner.event === live.event)) throw new Error('Unavailable: live event tag does not own this decoded instruction');
    return { actor: live.actor, event: live.event, pc: live.pc };
  }
  if (owners.length !== 1) throw new Error(`Unavailable: event ownership is ${owners.length ? `ambiguous (${owners.map(owner => owner.event).join(', ')})` : 'unresolved'}; the runtime slot has no event tag`);
  return { actor: live.actor, event: owners[0].event, pc: live.pc };
}
export const formatLivePc = (live: LivePc) => `Disc ${live.disc} · Map ${live.map} · Actor ${live.actor} · ${live.event === null ? 'untagged event' : `Event ${live.event}`} · PC ${gameJson(live.pc)}`;
