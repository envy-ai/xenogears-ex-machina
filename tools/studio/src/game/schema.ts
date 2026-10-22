import { ExactInteger, record, type GameValue } from './protocol';

export interface GameIdentity { module: string; disc: string; packages: string[] }
interface Member { name: string | null; offset: number; type: number | null; bit_size?: number | null }
export interface SchemaType {
  kind: string; name?: string | null; size?: number; encoding?: string;
  type?: number | null; to?: number | string | null; of?: number | null; count?: number;
  members?: Member[]; values?: Record<string, number>;
}
export interface GameSchema {
  module_sha256: string;
  globals: Record<string, { address: number; image?: string | null; type: number | null }>;
  types: SchemaType[]; records: Record<string, number>;
}
export interface SchemaNode {
  path: string; label: string; type: number | null; depth: number;
  unsupported?: string; range?: { start: number; end: number; element: number | null };
}
const MAX_DEPTH = 12;
const PAGE_SIZE = 64;
export class SchemaModel {
  readonly identity: GameIdentity;
  readonly schema: GameSchema;
  constructor(reply: GameValue, expectedModule?: string) {
    const envelope = record(reply, 'Session schema');
    const identity = record(envelope.identity, 'Session identity');
    if (typeof identity.module !== 'string' || !/^[a-f0-9]{64}$/.test(identity.module) || typeof identity.disc !== 'string' ||
        !Array.isArray(identity.packages) || !identity.packages.every(item => typeof item === 'string')) throw new Error('Invalid session content identity');
    if (expectedModule && identity.module !== expectedModule) throw new Error('Schema belongs to another game module');
    this.identity = identity as unknown as GameIdentity;
    const schema = record(envelope.schema, 'Loaded session schema');
    if (schema.module_sha256 !== identity.module) throw new Error('Schema has no matching game-module build binding');
    if (!Array.isArray(schema.types) || !schema.types.every(type => type !== null && typeof type === 'object' && typeof (type as Record<string, unknown>).kind === 'string')) throw new Error('Session schema has no valid type table');
    record(schema.globals, 'Schema globals'); record(schema.records, 'Schema records');
    this.schema = schema as unknown as GameSchema;
    const validType = (type: unknown) => type === null || (Number.isSafeInteger(type) && (type as number) >= 0 && (type as number) < this.schema.types.length);
    for (const [name, global] of Object.entries(this.schema.globals)) {
      if (!/^[A-Za-z_]\w*$/.test(name) || !global || !Number.isSafeInteger(global.address) || global.address < 0 || global.address > 0xffff_ffff || !validType(global.type)) throw new Error(`Invalid schema global ${name}`);
    }
    for (const [index, type] of this.schema.types.entries()) {
      for (const key of ['type', 'of'] as const) if (key in type && !validType(type[key])) throw new Error(`Invalid schema type reference ${index}.${key}`);
      if (type.kind === 'array' && (!Number.isSafeInteger(type.count) || type.count! < 0)) throw new Error(`Invalid array count at type ${index}`);
      if (type.members && (!Array.isArray(type.members) || type.members.some(member => !validType(member.type)))) throw new Error(`Invalid members at type ${index}`);
    }
  }
  private node(node: SchemaNode): SchemaNode {
    const type = this.resolve(node.type);
    if (!node.unsupported && type?.kind === 'base' &&
      (!['signed', 'unsigned', 'bool'].includes(type.encoding ?? '') || ![1, 2, 4, 8].includes(type.size ?? 0))) {
      return { ...node, unsupported: `${type.name ?? 'base'} (${type.encoding ?? 'unknown'} ${String(type.size)}-byte): value inspection/write unsupported by runtime` };
    }
    return node;
  }
  roots(filter = ''): SchemaNode[] {
    return Object.entries(this.schema.globals).filter(([name]) => name.toLowerCase().includes(filter.toLowerCase())).sort(([a], [b]) => a.localeCompare(b))
      .map(([name, global]) => this.node({ path: name, label: name, type: global.type, depth: 0 }));
  }
  resolve(id: number | null): SchemaType | null {
    const seen = new Set<number>();
    while (id !== null) {
      if (seen.has(id)) throw new Error('Cyclic schema typedef');
      seen.add(id);
      const type = this.schema.types[id];
      if (!type) throw new Error(`Unknown schema type ${id}`);
      if (type.kind !== 'typedef') return type;
      id = type.type ?? null;
    }
    return null;
  }
  describe(node: SchemaNode): string {
    if (node.range) return `Elements ${node.range.start}–${node.range.end - 1}`;
    if (node.unsupported) return node.unsupported;
    const type = this.resolve(node.type);
    if (!type) return 'untyped · read-only';
    return `${type.name ?? type.kind}${type.kind === 'array' ? `[${type.count}]` : ''}${type.encoding ? ` · ${type.encoding} ${type.size! * 8}-bit` : ''}`;
  }
  scalar(node: SchemaNode): boolean {
    if (node.unsupported || node.range) return false;
    const type = this.resolve(node.type);
    return !!type && (type.kind === 'enum' || type.kind === 'pointer' ||
      (type.kind === 'base' && ['signed', 'unsigned', 'bool'].includes(type.encoding ?? '') && [1, 2, 4, 8].includes(type.size ?? 0)));
  }
  value(node: SchemaNode, text: string): GameValue {
    if (!this.scalar(node)) throw new Error(`Scalar writes unsupported for ${this.describe(node)} at ${node.path}`);
    const type = this.resolve(node.type)!;
    if (type.kind === 'enum' && !/^-?\d+$/.test(text)) return text;
    if (!/^-?(?:0|[1-9]\d*)$/.test(text)) throw new Error('Enter an exact decimal integer (enum names are also accepted for enums)');
    const value = BigInt(text);
    const bits = BigInt((type.kind === 'pointer' ? 4 : type.size ?? 0) * 8);
    if (bits <= 0n || bits > 64n) throw new Error('Unsupported runtime scalar width');
    const signed = type.kind === 'enum' || type.encoding === 'signed';
    const minimum = signed ? -(1n << (bits - 1n)) : 0n;
    const maximum = type.encoding === 'bool' ? 1n : signed ? (1n << (bits - 1n)) - 1n : (1n << bits) - 1n;
    if (value < minimum || value > maximum) throw new Error(`Value is outside ${minimum}…${maximum} for ${this.describe(node)}`);
    return new ExactInteger(text);
  }
  children(node: SchemaNode): SchemaNode[] {
    if (node.unsupported) return [];
    if (node.depth >= MAX_DEPTH) return [{ ...node, label: 'Expansion limit', unsupported: `Maximum schema depth ${MAX_DEPTH} reached` }];
    const child = (path: string, label: string, type: number | null, unsupported?: string): SchemaNode => this.node({ path, label, type, depth: node.depth + 1, ...(unsupported ? { unsupported } : {}) });
    const array = (start: number, end: number, element: number | null): SchemaNode[] => {
      if (end - start <= PAGE_SIZE) return Array.from({ length: end - start }, (_, i) => child(`${node.path}[${start + i}]`, `[${start + i}]`, element));
      const stride = PAGE_SIZE ** Math.ceil(Math.log(end - start) / Math.log(PAGE_SIZE) - 1);
      const ranges: SchemaNode[] = [];
      for (let index = start; index < end; index += stride) ranges.push({ path: node.path, label: `[${index}…${Math.min(index + stride, end) - 1}]`, type: element, depth: node.depth + 1, range: { start: index, end: Math.min(index + stride, end), element } });
      return ranges;
    };
    if (node.range) return array(node.range.start, node.range.end, node.range.element);
    const type = this.resolve(node.type);
    if (!type) return [];
    const members = (items: Member[], pointer: boolean) => items.map((member, i) => member.name
      ? child(`${node.path}${pointer ? '->' : '.'}${member.name}`, member.name, member.type, member.bit_size != null ? 'bit-field path inspection/write unsupported by runtime' : undefined)
      : child(node.path, `<anonymous ${i}>`, member.type, 'Anonymous member has no runtime path'));
    if (type.kind === 'struct' || type.kind === 'union') return members(type.members ?? [], false);
    if (type.kind === 'array') return array(0, type.count!, type.of ?? null);
    if (type.kind === 'pointer') {
      const to = typeof type.to === 'number' ? type.to : typeof type.to === 'string' && type.to.startsWith('record ') ? this.schema.records[type.to.slice(7)] : null;
      if (to == null) return [child(node.path, 'Unknown pointee', null, 'Pointer has no typed target')];
      const target = this.resolve(to);
      if (target?.kind === 'struct' || target?.kind === 'union') return members(target.members ?? [], true);
      return [child(`${node.path}[0]`, '[0]', to)];
    }
    return [];
  }
}
