/** One authored owner: M6 event-only or source-owned M7 scene, never both. */
import type { Backend } from './service';
import { FieldDraftSession, type DraftAction, type DraftDocument, type DraftSnapshot, type CompiledDraft } from './drafts';
import { SceneDraftSession, type SceneDocument, type SceneInput, type SceneAction } from './scenes';
import type { SourceContext, SourceIdentity } from './discs';
import type { FieldGraph } from '../events/model';

export class FieldAuthoring {
  private constructor(private readonly owner: FieldDraftSession | SceneDraftSession) {}
  static open(backend: Backend, graph: FieldGraph): FieldAuthoring { return new FieldAuthoring(FieldDraftSession.open(backend, graph)); }
  static importEvents(backend: Backend, document: DraftDocument): FieldAuthoring { return new FieldAuthoring(FieldDraftSession.import(backend, document)); }
  static importScene(backend: Backend, document: SceneDocument): FieldAuthoring { return new FieldAuthoring(SceneDraftSession.import(backend, document)); }
  static scene(backend: Backend, input: SceneInput, entrances: number[], events?: DraftDocument): FieldAuthoring {
    return new FieldAuthoring(SceneDraftSession.open(backend, input, entrances, events));
  }
  get scene(): SceneDraftSession | null { return this.owner instanceof SceneDraftSession ? this.owner : null; }
  get snapshot(): DraftSnapshot {
    if (this.owner instanceof FieldDraftSession) return this.owner.snapshot;
    const state = this.owner.snapshot;
    return { draft: -this.owner.scene, revision: state.revision, can_undo: state.can_undo, can_redo: state.can_redo, modified: state.modified, graph: state.events.graph, bindings: state.events.bindings };
  }
  get original(): FieldGraph {
    const document = this.export();
    return 'base' in document ? document.base : document.event_document.base;
  }
  edit(action: DraftAction): DraftSnapshot {
    if (this.owner instanceof FieldDraftSession) return this.owner.edit(action);
    const actors = this.owner.snapshot.actors;
    const translate = (action: DraftAction): SceneAction => {
      if (action.kind === 'batch') return { kind: 'batch', actions: action.actions.map(action => translate(action) as Exclude<SceneAction, {kind:'batch'}>) };
      if (action.kind === 'new_event') {
        const actor = actors[action.actor];
        if (!actor) throw new Error('The selected scene actor no longer exists');
        return { kind: 'actor_event', actor: actor.id, event: action.event, nodes: action.nodes };
      }
      return { kind: 'event', action };
    };
    this.owner.edit(translate(action)); return this.snapshot;
  }
  editScene(action: SceneAction): DraftSnapshot {
    if (!(this.owner instanceof SceneDraftSession)) throw new Error('Open the source-qualified Scene before editing its entities');
    this.owner.edit(action); return this.snapshot;
  }
  history(operation: 'undo'|'redo'): DraftSnapshot { this.owner.history(operation); return this.snapshot; }
  compile(): CompiledDraft { return this.owner instanceof FieldDraftSession ? this.owner.compile() : this.owner.compile().events; }
  export(): DraftDocument | SceneDocument { return this.owner.export(); }
  repack(bundle: number[], context?: SourceContext, source?: SourceIdentity): {bytes:number[];source_sha256:string} {
    if (this.owner instanceof FieldDraftSession) return this.owner.repack(bundle, context, source);
    const original = this.owner.export().input;
    if (original.bundle.length !== bundle.length || original.bundle.some((byte,index) => byte !== bundle[index])
      || !sameSceneSource(original.source??null,source??null)) {
      throw new Error('Scene compilation source differs from the selected original disc/map bundle');
    }
    const compiled = this.owner.compile(); return { bytes: compiled.bundle, source_sha256: compiled.source_sha256 };
  }
  close(): void { this.owner.close(); }
  nodeAt(pc: number): string | null { return this.owner.nodeAt(pc); }
}

export function sameSceneSource(a:SourceIdentity|null,b:SourceIdentity|null):boolean {
  if(!a||!b)return a===b;
  return (['disc','boot_sha256','map','slot','lba','indexed_size','source_sha256'] as const).every(key=>a[key]===b[key]);
}
