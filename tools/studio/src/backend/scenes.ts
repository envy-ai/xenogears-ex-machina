/** M7 nonvisual scene transport. Rust owns source identity, edits and history. */
import { readCommandResult, type Backend } from './service';
import type { SourceContext, SourceIdentity } from './discs';
import type { CompiledDraft, DraftAction, DraftDocument, NodeInput } from './drafts';

export type SceneVector = [number, number, number];
export type SceneCorners = [SceneVector, SceneVector, SceneVector, SceneVector];
export interface SceneInput {
  bundle: number[];
  context?: SourceContext | null;
  source?: SourceIdentity | null;
  module_sha256?: string | null;
}
export interface SceneEntity<T> { id: string; value: T }
export interface SceneDescriptor {
  flags: number; rotation: SceneVector; position: SceneVector; model: number;
}
export interface SceneActor {
  id: string;
  /** Compiler source provenance only, never a runtime selection or edit index. */
  event_source_row: number;
  origin: { kind: 'original'; actor: number } | { kind: 'independent_copy'; template: string };
}
export interface SceneEntrance {
  selector: number; source_span: [number, number]; x: number; z: number;
  layer: number; camera_raw: number; facing_raw: number;
}
export interface SceneFace {
  vertices: [string, string, string]; neighbors: [string | null, string | null, string | null];
  attribute: number; height_bump_raw: number;
}
export interface SceneCollision {
  source_sha256: string; logical_size: number; attributes_offset: number;
  attributes: number[]; reserved_triangle_sizes: number[];
  layers: {
    triangles_offset: number; vertices_offset: number; missing_neighbor_attribute: number;
    vertices: [number, number, number, number][];
    triangles: { vertices: SceneVector; neighbors: SceneVector; attribute: number; height_bump_raw: number }[];
  }[];
}
export interface SceneSnapshot {
  revision: number; can_undo: boolean; can_redo: boolean; modified: boolean;
  descriptors: SceneEntity<SceneDescriptor>[]; actors: SceneActor[];
  zones: SceneEntity<{ corners: SceneCorners }>[];
  collision: SceneCollision;
  collision_entities: { layers: {
    id: string; vertices: SceneEntity<[number, number, number, number]>[];
    triangles: SceneEntity<SceneFace>[];
  }[] };
  entrances: SceneEntrance[]; source_sha256: string;
  events: CompiledDraft; next_event_id: number; reference_diagnostics: string[];
}
export type PlacementCoordinate =
  | { kind: 'immediate'; value: number }
  | { kind: 'variable'; id: number }
  | { kind: 'floor'; explicit_layer: number | null };
export interface ScenePlacement {
  node: string; pc: number; handler: string;
  position: [PlacementCoordinate, PlacementCoordinate, PlacementCoordinate];
  owners: { actor: string; event: number }[]; can_transform: boolean; diagnostics: string[];
}
export interface ScenePlacementChange {
  node: string; actor: string; event: number;
  position: [number | null, number | null, number | null]; layer: number | null;
}
export type SceneAction =
  | { kind: 'batch'; actions: Exclude<SceneAction, { kind: 'batch' }>[] }
  | { kind: 'event'; action: DraftAction }
  | { kind: 'add_actor'; template: string; position: SceneVector; rotation: SceneVector }
  | { kind: 'duplicate_actor' | 'delete_actor'; id: string }
  | { kind: 'actor_event'; actor: string; event: number; nodes: NodeInput[] }
  | { kind: 'set_placement'; change: ScenePlacementChange }
  | { kind: 'set_descriptor'; id: string; position: SceneVector; rotation: SceneVector }
  | { kind: 'add_instance'; template: string; model: number; position: SceneVector; rotation: SceneVector }
  | { kind: 'duplicate_instance' | 'delete_instance'; id: string }
  | { kind: 'set_zone'; id: string; corners: SceneCorners }
  | { kind: 'add_zone'; corners: SceneCorners }
  | { kind: 'duplicate_zone' | 'delete_zone'; id: string }
  | { kind: 'set_collision_vertex'; id: string; position: SceneVector }
  | { kind: 'add_collision_vertex'; layer: string; position: SceneVector }
  | { kind: 'duplicate_collision_vertex' | 'delete_collision_vertex'; id: string }
  | { kind: 'add_collision_triangle'; layer: string; vertices: [string, string, string]; attribute: number; height_bump_raw: number }
  | { kind: 'duplicate_collision_triangle' | 'delete_collision_triangle'; id: string }
  | { kind: 'set_entrance'; value: SceneEntrance };
export interface SceneDocument {
  schema_version: 4; input: SceneInput;
  /** Empty original M6 root; imported actions live on the scene timeline. */
  event_document: DraftDocument;
  entrance_selectors: number[]; actions: SceneAction[]; allocation_starts: number[];
  cursor: number; next_id: number; event_allocation_starts: number[];
  next_event_id: number; imported_event_actions: number;
}
export interface CompiledScene {
  bundle: number[]; bundle_sha256: string; source_sha256: string;
  descriptor_ids: string[]; zone_ids: string[]; events: CompiledDraft;
}
interface SceneReply { scene: number; state: SceneSnapshot }

/** A scene handle is separate from an M6 draft handle; no parallel editable copy. */
export class SceneDraftSession {
  private constructor(private readonly backend: Backend, private value: SceneReply) {}
  private static request<T>(backend: Backend, args: unknown): T {
    return readCommandResult<T>(backend.command(JSON.stringify({ op: 'field.scene.draft', args })));
  }
  private call<T>(kind: string, args: Record<string, unknown> = {}): T {
    return SceneDraftSession.request<T>(this.backend, { kind, scene: this.value.scene, ...args });
  }
  static open(backend: Backend, input: SceneInput, entrances: number[], eventDocument?: DraftDocument): SceneDraftSession {
    const value = SceneDraftSession.request<SceneReply>(backend, {
      kind: 'open', input, entrances, ...(eventDocument ? { event_document: eventDocument } : {}),
    });
    return new SceneDraftSession(backend, value);
  }
  static import(backend: Backend, document: SceneDocument): SceneDraftSession {
    return new SceneDraftSession(backend, SceneDraftSession.request<SceneReply>(backend, { kind: 'import', document }));
  }
  get scene(): number { return this.value.scene; }
  get snapshot(): SceneSnapshot { return this.value.state; }
  edit(action: SceneAction): SceneSnapshot {
    const next = this.call<SceneReply>('edit', { revision: this.snapshot.revision, action });
    this.value = next; return next.state;
  }
  history(kind: 'undo' | 'redo'): SceneSnapshot {
    const next = this.call<SceneReply>(kind, { revision: this.snapshot.revision });
    this.value = next; return next.state;
  }
  placement(node: string): ScenePlacement { return this.call('placement', { node }); }
  compile(): CompiledScene { return this.call('compile'); }
  export(): SceneDocument { return this.call('export'); }
  close(): void { this.call('close'); }
  nodeAt(pc: number): string | null {
    return Object.entries(this.snapshot.events.bindings).find(([, placed]) => placed === pc)?.[0] ?? null;
  }
}
