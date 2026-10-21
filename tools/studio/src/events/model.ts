/** Version-one Rust inspection DTOs. All PCs are bytecode-relative. */
export interface OperandSpec { kind: string; offset: number; flags: number | null; bit: number | null }
export interface OpcodeForm {
  mnemonic: string; handler: string; size: number; flow: string; operands: OperandSpec[];
  effect: string; select: [number, number, number | null] | null;
  also: [number, string | null][]; inner: number[]; setup: number | null;
}
export interface Instruction {
  pc: number; opcode: number; extended: number | null; size: number; ranges: [number, number][];
  spec: OpcodeForm; operands: number[]; immediate: (boolean | null)[];
  successors: number[]; skips: number[]; inner: number[];
}
export interface Edge {
  from_block: number; to_block: number | null; from_pc: number; to_pc: number;
  kind: string; status: string;
}
export interface Block {
  start: number; end: number; kind: string; reason: string | null;
  bytes: number[]; instruction_pcs: number[];
}
export interface Diagnostics {
  overlaps: [number, number][]; unpaired: number[]; inner: [number, number][];
  tables: [number, number][]; into: [number, number][]; outside: [number, number][];
  beyond_declared: number[]; followed_skips: number;
}
export interface FieldGraph {
  schema_version: number; source_sha256: string; declared_size: number; stream_size: number;
  bytecode_offset: number; variable_unsigned_bits: number[]; actors: number[][];
  entries: { actor: number; event: number; pc: number }[]; skipped_zero_entries: number;
  instructions: Instruction[]; blocks: Block[]; edges: Edge[]; diagnostics: Diagnostics;
}
export interface OwnedInstruction { pc: number; edges: Edge[] }
export interface Sequence { nodes: StructureNode[] }
export type StructureNode =
  | { kind: 'instruction' | 'goto'; instruction: OwnedInstruction }
  | { kind: 'label'; pc: number }
  | { kind: 'if'; header: OwnedInstruction; then_branch: Sequence; else_branch: Sequence; join: number }
  | { kind: 'loop'; header_pc: number; body: Sequence; exit_pc: number | null }
  | { kind: 'switch'; header: OwnedInstruction; cases: { entry_pc: number; body: Sequence }[]; join: number | null };
export interface FoldCounts { ifs: number; loops: number; switches: number; gotos: number }
export interface EventStructure {
  schema_version: number; actor: number; event: number; entry_pc: number | null;
  skipped: boolean; body: Sequence; counts: FoldCounts;
}
export interface Selection { actor: number; event: number; pc: number | null }
export interface ActorNames { name?: string; events: Record<string, string> }
export interface MapNames { actors: Record<string, ActorNames> }
export interface NameTable {
  schema_version: number; variables: Record<string, string>;
  maps: Record<string, MapNames>; messages: Record<string, string>;
}
export interface NamesContext { actor_counts: Record<string, number> }
export interface ValidatedNames {
  table: NameTable; variable_count: number; actor_count: number; event_count: number; message_count: number;
}
export const EMPTY_NAMES: NameTable = { schema_version: 1, variables: {}, maps: {}, messages: {} };
export interface ActorSelector {
  raw: number; kind: string; direct_actor: number | null; party_slot: number | null;
  resolved_actor: number | null; unresolved: string | null;
}
export interface InstructionLocation { actor: number; event: number; pc: number }
export interface SourceEvidence { source: string; function: string; function_sha256: string; helper: string }
export interface VariableAccess {
  pc: number; operand_index: number; operand_offset: number; operand_kind: string; raw: number;
  reference: number | null; slot: number | null; odd_alias: boolean; bit: number | null;
  direction: string; resolution: string; reason: string | null; provenance: SourceEvidence[];
}
export interface EventLane {
  event: number; entry_pc: number; skipped: boolean; instruction_count: number;
  instruction_pcs: number[]; skips: { pc: number; targets: number[] }[]; variable_accesses: VariableAccess[];
}
export interface EventRequest {
  source: InstructionLocation; kind: string; actor: ActorSelector; packed_event: number;
  event: number; priority: number; target: { actor: number; event: number } | null;
}
export interface AccessLocation extends InstructionLocation { operand_index: number }
export interface VariableDependency { slot: number; writers: AccessLocation[]; readers: AccessLocation[] }
export interface MapLanes {
  schema_version: number; actors: { actor: number; events: EventLane[] }[];
  requests: EventRequest[]; variable_dependencies: VariableDependency[];
}

export const formatPc = (pc: number) => `@${pc.toString(16).padStart(4, '0')}`;
export const eventTrigger = (event: number) => ['Load', 'Idle', 'Talk', 'Touch'][event] ?? `Event ${event}`;
export const actorName = (names: NameTable, map: number, actor: number) =>
  names.maps[String(map)]?.actors[String(actor)]?.name ?? `Actor ${actor}`;
export const eventName = (names: NameTable, map: number, actor: number, event: number) =>
  names.maps[String(map)]?.actors[String(actor)]?.events[String(event)] ?? eventTrigger(event);
export const variableName = (names: NameTable, slot: number) => names.variables[String(slot)] ?? `Variable ${slot}`;

export function selectorName(selector: ActorSelector, names: NameTable, map: number): string {
  if (selector.kind === 'party') return `Party slot ${selector.party_slot} (runtime binding)`;
  if (selector.kind === 'self') return `Self (${selector.resolved_actor === null ? 'unresolved' : actorName(names, map, selector.resolved_actor)})`;
  return selector.direct_actor === null ? `Actor selector ${selector.raw}` : actorName(names, map, selector.direct_actor);
}

function variableReference(raw: number, names: NameTable): string {
  if (raw >= 2048) return `Invalid variable reference 0x${raw.toString(16)}`;
  return `${variableName(names, raw >>> 1)}${raw & 1 ? ' (odd byte alias)' : ''}`;
}

export function formatOperand(instruction: Instruction, index: number, names: NameTable, map: number, sourceActor?: number): string {
  const descriptor = instruction.spec.operands[index];
  const value = instruction.operands[index];
  const immediate = instruction.immediate[index];
  if (descriptor.kind === 'u8' && descriptor.offset === 2
    && ['field_event_request_event_started', 'field_event_request_event_finished'].includes(instruction.spec.handler)) {
    const actorIndex = instruction.spec.operands.findIndex(operand => operand.kind === 'actor' && operand.offset === 1);
    const actor = instruction.operands[actorIndex];
    const resolved = actor === 0xfb ? sourceActor : actor < 0xfd ? actor : undefined;
    const event = value & 31;
    return `${resolved === undefined ? eventTrigger(event) : eventName(names, map, resolved, event)} (event ${event}, priority ${value >>> 5})`;
  }
  if ((descriptor.kind === 'iv' || descriptor.kind === 'sel') && immediate !== null) {
    return immediate
      ? String(descriptor.kind === 'iv' ? value & 0x7fff : value >= 0x8000 ? value - 0x10000 : value)
      : variableReference(value, names);
  }
  switch (descriptor.kind) {
    case 'var': return variableReference(value, names);
    case 'bit': return `${variableReference(value >>> 4, names)} · bit ${value & 15}`;
    case 'actor':
      if (value >= 0xfd) return `Party slot ${0xff - value}`;
      if (value === 0xfb) return 'Self';
      return actorName(names, map, value);
    case 'addr': return formatPc(value);
    case 'data': return `Data ${formatPc(value)}`;
    case 'msg': return names.messages[String(value)] ?? `msg${value.toString(16).padStart(4, '0')}`;
    case 's16': return String(value >= 0x8000 ? value - 0x10000 : value);
    case 'cond': return `Condition ${value & 15} (0x${value.toString(16).padStart(2, '0')})`;
    case 'flags': return `Flags 0x${value.toString(16).padStart(2, '0')}`;
    default: return String(value);
  }
}

export function instructionPhrase(instruction: Instruction, names: NameTable, map: number, sourceActor?: number): string {
  const phrase = instruction.spec.mnemonic.replaceAll('_', ' ');
  const operands = instruction.spec.operands.map((_, index) => formatOperand(instruction, index, names, map, sourceActor));
  return `${phrase.charAt(0).toUpperCase()}${phrase.slice(1)}${operands.length ? `: ${operands.join(', ')}` : ''}`;
}

/** Build once per graph, preserving every diagnostic category and storage reason. */
export function instructionDiagnostics(graph: FieldGraph): Map<number, string[]> {
  const result = new Map<number, string[]>();
  const add = (pc: number, text: string) => result.set(pc, [...(result.get(pc) ?? []), text]);
  for (const [pc, other] of graph.diagnostics.overlaps) {
    add(pc, `Overlap with ${formatPc(other)}`); add(other, `Overlapped by ${formatPc(pc)}`);
  }
  for (const pc of graph.diagnostics.unpaired) add(pc, 'Unpaired setup/step');
  for (const [pc, target] of graph.diagnostics.inner) add(pc, `Inner continuation ${formatPc(target)} (not followed)`);
  for (const [pc, count] of graph.diagnostics.tables) add(pc, `Inline table: ${count} slots`);
  for (const [pc, target] of graph.diagnostics.into) add(pc, `Skip enters an instruction at ${formatPc(target)} (not followed)`);
  for (const [pc, target] of graph.diagnostics.outside) add(pc, `Outside stream: ${formatPc(target)}`);
  for (const pc of graph.diagnostics.beyond_declared) add(pc, 'Beyond declared component boundary');
  for (const block of graph.blocks) if (block.kind !== 'code') {
    for (const pc of block.instruction_pcs) add(pc, `${block.kind} storage ${formatPc(block.start)}–${formatPc(block.end)}: ${block.reason ?? 'no reason supplied'}`);
  }
  return result;
}

export function rawInstructionBytes(graph: FieldGraph, instruction: Instruction): string {
  return instruction.ranges.map(([start, end]) => {
    const bytes: string[] = [];
    for (let pc = start; pc < end; pc += 1) {
      const block = graph.blocks.find(block => block.start <= pc && pc < block.end);
      if (!block) throw new Error(`Instruction ${formatPc(instruction.pc)} has missing storage at ${formatPc(pc)}`);
      bytes.push(block.bytes[pc - block.start].toString(16).padStart(2, '0'));
    }
    return bytes.join(' ');
  }).join(' | ');
}

export function reachableInstructionPcs(graph: FieldGraph, entry: number | null): Set<number> {
  const next = new Map<number, number[]>();
  for (const edge of graph.edges) if (edge.status === 'followed') {
    const targets = next.get(edge.from_pc) ?? []; targets.push(edge.to_pc); next.set(edge.from_pc, targets);
  }
  const seen = new Set<number>();
  const pending = entry === null ? [] : [entry];
  while (pending.length) {
    const pc = pending.pop()!;
    if (seen.has(pc)) continue;
    seen.add(pc); pending.push(...(next.get(pc) ?? []));
  }
  return seen;
}
