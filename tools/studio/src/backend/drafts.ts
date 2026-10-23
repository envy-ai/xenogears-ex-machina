/** M6 authored draft transport. Rust owns graph edits, revisions and history. */
import { readCommandResult, type Backend } from './service';
import type { FieldGraph } from '../events/model';
import type { SourceContext, SourceIdentity } from './discs';

export type OperandValue = { kind: 'literal'; value: number; immediate?: boolean } | { kind: 'target'; node: string };
export interface NodeInput { bytes: number[]; targets?: Record<string, string> }
export type DraftAction =
  | { kind: 'set_operand'; node: string; operand: number; value: OperandValue }
  | { kind: 'insert_after'; after: string; bytes: number[]; targets?: Record<string, string> }
  | { kind: 'new_event'; actor: number; event: number; nodes: NodeInput[] }
  | { kind: 'batch'; actions: Exclude<DraftAction, { kind: 'batch' }>[] };
export interface DraftSnapshot {
  draft: number; revision: number; graph: FieldGraph; bindings: Record<string, number>;
  can_undo: boolean; can_redo: boolean; modified: boolean;
}
export interface CompiledDraft { bytes: number[]; declared_size: number; graph: FieldGraph; bindings: Record<string, number> }
export interface DraftDocument {
  schema_version: 1; base: FieldGraph; actions: DraftAction[]; cursor: number;
  allocation_starts: number[]; next_id: number;
}
export class FieldDraftSession {
  private constructor(private readonly backend: Backend, public snapshot: DraftSnapshot) {}
  private call<T>(operation: string, args: unknown): T {
    return readCommandResult<T>(this.backend.command(JSON.stringify({ op: `field.draft.${operation}`, args })));
  }
  static open(backend: Backend, graph: FieldGraph): FieldDraftSession {
    const snapshot = readCommandResult<DraftSnapshot>(backend.command(JSON.stringify({ op: 'field.draft.open', args: { graph } })));
    return new FieldDraftSession(backend, snapshot);
  }
  static import(backend: Backend, document: DraftDocument): FieldDraftSession {
    const snapshot = readCommandResult<DraftSnapshot>(backend.command(JSON.stringify({ op: 'field.draft.import', args: { document } })));
    return new FieldDraftSession(backend, snapshot);
  }
  edit(action: DraftAction): DraftSnapshot {
    const next = this.call<DraftSnapshot>('edit', { draft: this.snapshot.draft, revision: this.snapshot.revision, action });
    this.snapshot = next; return next;
  }
  history(operation: 'undo' | 'redo'): DraftSnapshot {
    const next = this.call<DraftSnapshot>(operation, { draft: this.snapshot.draft, revision: this.snapshot.revision });
    this.snapshot = next; return next;
  }
  compile(): CompiledDraft { return this.call('compile', { draft: this.snapshot.draft }); }
  export(): DraftDocument { return this.call('export', { draft: this.snapshot.draft }); }
  repack(bundle: number[], context?:SourceContext, source?:SourceIdentity): { bytes: number[]; source_sha256: string } { return this.call('repack', { draft: this.snapshot.draft, bundle, ...(context ? {context,source,module_sha256:context.module_sha256} : {}) }); }
  close(): void { this.call('close', { draft: this.snapshot.draft }); }
  nodeAt(pc: number): string | null { return Object.entries(this.snapshot.bindings).find(([, placed]) => placed === pc)?.[0] ?? null; }
}
