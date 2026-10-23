/** One authored field draft shared by the list, graph and actor lanes. */
import { useCallback, useEffect, useRef, useState } from 'react';
import { FieldDraftSession, type DraftAction, type DraftDocument, type DraftSnapshot } from '../backend/drafts';
import type { Backend } from '../backend/service';
import { readCommandResult, StudioCommandError } from '../backend/service';
import type { DiscSession, SourceContext, SourceIdentity } from '../backend/discs';
import type { FilesAdapter } from '../platform/files';
import type { FieldGraph, NameTable, NamesContext } from './model';

export interface FieldBuild { bytes: Uint8Array; identity: string; revision: number; draft: number; file: string; nativePath: string | null }
export function useFieldDraft(backend: Backend | null, folder: FilesAdapter | null, report: (error: unknown) => void, log: (kind: string, text: string) => void) {
  const current = useRef<FieldDraftSession | null>(null);
  const sourceContext=useRef<SourceContext|null>(null);
  const contextGeneration = useRef(0);
  const savedContextGeneration = useRef(0);
  const [contextDirty, setContextDirty] = useState(false);
  const sourceHandler=useRef<((source:SourceIdentity)=>Promise<SourceContext>)|null>(null);
  const ownership = useRef({ folder, backend, generation: 0 });
  if (ownership.current.folder !== folder || ownership.current.backend !== backend) ownership.current = { folder, backend, generation: ownership.current.generation + 1 };
  const sessionGeneration = useRef(0);
  const pendingRef = useRef(false);
  const inlineGeneration = useRef(0);
  const ownerToken = () => ({ ...ownership.current, session: current.current, sessionGeneration: sessionGeneration.current, revision: current.current?.snapshot.revision, inlineGeneration: inlineGeneration.current, contextGeneration: contextGeneration.current });
  const checkOwner = (token: ReturnType<typeof ownerToken>) => {
    if (ownership.current.folder !== token.folder || ownership.current.backend !== token.backend || ownership.current.generation !== token.generation || current.current !== token.session || sessionGeneration.current !== token.sessionGeneration || current.current?.snapshot.revision !== token.revision || inlineGeneration.current !== token.inlineGeneration || contextGeneration.current !== token.contextGeneration) throw new Error('The project, source map or authored revision changed during the operation; current draft and last-good build are preserved');
  };
  const beginPending = () => { if (pendingRef.current) throw new Error('A project operation is already pending'); pendingRef.current = true; setPending(true); };
  const endPending = () => { pendingRef.current = false; setPending(false); };
  useEffect(() => () => { current.current?.close(); current.current = null; }, [backend]);
  const [snapshot, setSnapshot] = useState<DraftSnapshot | null>(null);
  const [inlineDirty, setInlineDirty] = useState(false);
  const inlineRef = useRef(false);
  const [mode, setMode] = useState<'operand' | 'insert' | 'event'>('operand');
  const [editorKey, setEditorKey] = useState(0);
  const [savedRevision, setSavedRevision] = useState(0);
  const savedProjectRef = useRef(false);
  const [hasSavedProject, setHasSavedProject] = useState(false);
  useEffect(() => { setSavedRevision(-1); savedProjectRef.current = false; setHasSavedProject(false); savedContextGeneration.current = -1; setContextDirty(sourceContext.current !== null); }, [folder]);
  const [error, setError] = useState<Error | null>(null);
  const [lastBuild, setLastBuild] = useState<FieldBuild | null>(null);
  const [pending, setPending] = useState(false);
  const playHandler = useRef<((build: FieldBuild) => Promise<void>) | null>(null);
  const [canPlay, setCanPlay] = useState(false);
  const fail = useCallback((cause: unknown): never => {
    const error = cause instanceof Error ? cause : new Error(String(cause));
    setError(error); report(error); throw error;
  }, [report]);
  const dirty = useCallback((value: boolean) => { if (value || inlineRef.current !== value) inlineGeneration.current++; inlineRef.current = value; setInlineDirty(value); }, []);
  const guard = useCallback((leaving = false): boolean => {
    const value = current.current?.snapshot;
    if (pendingRef.current || inlineRef.current || (leaving && value && (((value.modified || savedProjectRef.current) && value.revision !== savedRevision) || (sourceContext.current !== null && contextGeneration.current !== savedContextGeneration.current)))) {
      const error = new Error(pendingRef.current ? 'Wait for the current project operation.' : inlineRef.current ? 'Apply or Cancel the inline instruction fields before changing selection.' : 'Save the authored project or explicitly discard its draft before changing maps.');
      setError(error); report(error); return false;
    }
    return true;
  }, [savedRevision, report]);
  const adopt = useCallback((next: FieldDraftSession, savedProject = false, context:SourceContext|null = null) => {
    sourceContext.current=context; contextGeneration.current++; savedContextGeneration.current=contextGeneration.current; setContextDirty(false);
    const previous = current.current;
    savedProjectRef.current = savedProject; setHasSavedProject(savedProject);
    sessionGeneration.current++; current.current = next; setSnapshot(next.snapshot); setSavedRevision(next.snapshot.revision);
    dirty(false); setMode('operand'); setEditorKey(value => value + 1); setError(null);
    previous?.close();
    return next.snapshot;
  }, [dirty]);
  const open = useCallback((graph: FieldGraph) => {
    if (pendingRef.current) throw new Error('Wait for the pending project operation before replacing its source map');
    if (!backend) throw new Error('The backend is not ready');
    return adopt(FieldDraftSession.open(backend, graph));
  }, [backend, adopt]);
  const edit = useCallback((action: DraftAction) => {
    try {
      if (pendingRef.current) throw new Error('Wait for the pending project operation before editing');
      if (!current.current) throw new Error('Open an event component before editing');
      const next = current.current.edit(action); setSnapshot(next); dirty(false); setMode('operand'); setEditorKey(value => value + 1); setError(null);
      log('Draft', `Applied ${action.kind}; revision ${next.revision}`); return next;
    } catch (error) { return fail(error); }
  }, [dirty, fail, log]);
  const history = useCallback((operation: 'undo' | 'redo') => {
    if (!guard()) return null;
    try { if (!current.current) throw new Error('No field draft is open'); const next = current.current.history(operation); setSnapshot(next); setEditorKey(value => value + 1); setError(null); return next; }
    catch (error) { return fail(error); }
  }, [guard, fail]);
  const cancel = useCallback(() => { dirty(false); setMode('operand'); setEditorKey(value => value + 1); setError(null); }, [dirty]);
  const begin = useCallback((mode: 'insert' | 'event') => { if (guard()) { setMode(mode); dirty(true); setEditorKey(value => value + 1); } }, [guard, dirty]);
  const clear = useCallback(() => {
    if (!guard(true)) return false;
    const previous=current.current;
    current.current=null; sourceContext.current=null; contextGeneration.current++; savedContextGeneration.current=contextGeneration.current; setContextDirty(false); sessionGeneration.current++; setSnapshot(null);
    savedProjectRef.current=false; setHasSavedProject(false); setSavedRevision(0);
    dirty(false); setMode('operand'); setEditorKey(value=>value+1); setError(null);
    previous?.close();
    return true;
  }, [guard,dirty]);
  const discard = useCallback(() => {
    const document = current.current?.export();
    if (!document) throw new Error('No field draft is open');
    return open(document.base);
  }, [open]);
  const validate = useCallback(() => {
    if (!guard()) return;
    try { if (!current.current) throw new Error('No field draft is open'); current.current.compile(); setError(null); log('Validation', 'Authored draft compiled without new control-flow diagnostics.'); }
    catch (error) { fail(error); }
  }, [guard, fail, log]);
  const save = useCallback(async (map: number, disc: number | null, names: NameTable) => {
    if (!guard()) return;
    const session = current.current; const owner = folder;
    if (!session || !owner) throw new Error('Open a project folder before saving the authored draft');
    const revision = session.snapshot.revision;
    const token = ownerToken(); beginPending();
    try {
      const document = { schema_version: 1, map, disc, document: session.export(), names, ...(sourceContext.current ? {source_context:sourceContext.current} : {}) };
      await owner.writeAtomic(`field-map-${map}.json`, new TextEncoder().encode(`${JSON.stringify(document, null, 2)}\n`));
      if (current.current !== session || ownership.current.folder !== owner) throw new Error('The project changed while saving; the previous project file was saved, current draft remains unsaved');
      checkOwner(token); savedContextGeneration.current=token.contextGeneration; setContextDirty(false); setSavedRevision(revision); savedProjectRef.current = true; setHasSavedProject(true); setError(null); log('Project', `Saved field-map-${map}.json in ${owner.name}`);
    } catch (error) { fail(error); } finally { endPending(); }
  }, [guard, folder, fail, log]);
  const load = useCallback(async (map: number, disc: number | null, context: NamesContext, sourceDisc:DiscSession|null = null): Promise<{ snapshot: DraftSnapshot; names: NameTable } | null> => {
    if (!guard(true)) return null;
    const owner = folder; const active = current.current;
    if (!owner || !backend || !active) throw new Error('Open a project and source map before loading a draft');
    const token = ownerToken(); beginPending();
    try {
      const data = await owner.read(`field-map-${map}.json`); checkOwner(token);
      const parsed = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(data)) as { schema_version: number; map: number; disc: number | null; document: DraftDocument; names: NameTable; source_context?:SourceContext };
      if (Object.keys(parsed).sort().join(',') !== (Object.hasOwn(parsed,'source_context') ? 'disc,document,map,names,schema_version,source_context' : 'disc,document,map,names,schema_version') || parsed.schema_version !== 1 || parsed.map !== map || parsed.disc !== disc || parsed.document.base.source_sha256 !== active.export().base.source_sha256) throw new Error('Saved field project does not match the selected map, disc and immutable source');
      const names = readCommandResult<{ table: NameTable }>(backend.command(JSON.stringify({ op: 'names.validate', args: { json: JSON.stringify(parsed.names), context } }))).table;
      checkOwner(token);
      let source:null|Awaited<ReturnType<DiscSession['mapBundle']>>=null;
      if (Object.hasOwn(parsed,'source_context')) {
        if (!parsed.source_context || !sourceDisc) throw new Error('Saved measured context requires its identified original disc');
        source=await sourceDisc.mapBundle(map, { onProgress: progress => log('Project', `Checking map ${map} source: ${progress.state}`) }); checkOwner(token);
        if (!source.source) throw new Error('Measured context requires an identified original map source');
      }
      const next = FieldDraftSession.import(backend, parsed.document);
      try {
        if (source) next.repack(source.bytes,parsed.source_context,source.source!);
        checkOwner(token);
        return { snapshot: adopt(next, true, parsed.source_context ?? null), names };
      } catch(error) {next.close();throw error;}
    } catch (error) { return fail(error); } finally { endPending(); }
  }, [guard, folder, backend, adopt, fail, log]);
  const build = useCallback(async (map: number, disc: DiscSession) => {
    if (!guard()) return;
    const active = current.current; const owner = folder;
    if (!active || !owner || !backend) throw new Error('Open a project folder and original disc before building a package');
    const revision = active.snapshot.revision; const draft = active.snapshot.draft;
    let token = ownerToken(); beginPending();
    try {
      const source = await disc.mapBundle(map, { onProgress: progress => log('Build', `Reading map ${map} source: ${progress.state}`) }); checkOwner(token);
      if (current.current !== active || active.snapshot.revision !== revision) throw new Error('The authored draft changed during package preparation');
      if (source.placeholder) throw new Error('A placeholder map has no editable event bundle');
      const identity = disc.status.identity;
      if (!identity?.serial || identity.test_fixture || !identity.package_discs?.length || !identity.package_discs.some(target => target.serial === identity.serial && target.sha256 === identity.boot_sha256)) throw new Error('Package builds require an identified original disc and its canonical family targets');
      let context=sourceContext.current;
      let result:{bytes:number[];source_sha256:string};
      try { result=active.repack(source.bytes,context??undefined,source.source??undefined); }
      catch(error) {
        if (!(error instanceof StudioCommandError) || error.kind!=='source_context_required') throw error;
        if (!source.source || !sourceHandler.current) throw new Error('Load this source map in an attached vanilla runtime to measure the complete original decompression context', {cause:error});
        log('Build','Reading measured source context from the attached vanilla runtime.');
        context=await sourceHandler.current(source.source); checkOwner(token);
        result=active.repack(source.bytes,context,source.source);
      }
      if (context && context !== sourceContext.current) {
        // This validated provenance is persisted with the project, independently
        // of authored instruction revisions. Acquisition must remain unsaved
        // even if a later package write fails.
        checkOwner(token); sourceContext.current=context; contextGeneration.current++;
        setContextDirty(true); token=ownerToken();
      }
      const compiled = readCommandResult<{ bytes: number[]; identity: string }>(backend.command(JSON.stringify({ op: 'package.build', args: { spec: { id: `studio-map-${map}`, discs: identity.package_discs, requires: [], overrides: [] }, files: [{ disc: identity.serial, slot: source.slot, source: source.bytes, result: result.bytes }] } })));
      const set = readCommandResult<{ bytes: number[] }>(backend.command(JSON.stringify({ op: 'package.bundle', args: { packages: [compiled.bytes] } })));
      const file = `field-map-${map}-${compiled.identity.split('@')[1]}.xemmods`; const bytes = new Uint8Array(set.bytes);
      await owner.writeAtomic(file, bytes);
      checkOwner(token);
      const nativePath = owner.nativePath ? await owner.nativePath(file) : null; checkOwner(token);
      if (current.current !== active || active.snapshot.revision !== revision) throw new Error('The draft changed while saving the package; last successful build remains selected');
      setLastBuild({ bytes, identity: compiled.identity, revision, draft, file, nativePath }); setError(null); log('Build', `Saved ${file} · ${compiled.identity}`);
    } catch (error) { fail(error); } finally { endPending(); }
  }, [guard, folder, backend, fail, log]);
  const play = useCallback(async () => {
    if (!guard()) return;
    if (!lastBuild || lastBuild.draft !== current.current?.snapshot.draft || lastBuild.revision !== current.current?.snapshot.revision) throw new Error('Build this authored revision before playing');
    if (!playHandler.current) throw new Error('No runtime launch control is available');
    const token = ownerToken(); beginPending(); try { await playHandler.current(lastBuild); checkOwner(token); } catch (error) { fail(error); } finally { endPending(); }
  }, [guard, lastBuild, fail]);
  const registerSource = useCallback((handler: ((source:SourceIdentity)=>Promise<SourceContext>)|null) => {sourceHandler.current=handler;}, []);
  const registerPlay = useCallback((handler: ((build: FieldBuild) => Promise<void>) | null) => { playHandler.current = handler; setCanPlay(Boolean(handler)); }, []);
  return { snapshot, inlineDirty, dirty, mode, editorKey, error, pending, lastBuild, canPlay, savedRevision, hasSavedProject, contextDirty, guard, clear, capture: ownerToken, assertUnchanged: checkOwner, open, edit, history, cancel, begin, discard, validate, save, load, build, play, registerPlay, registerSource, inspect: () => current.current?.compile() ?? null, nodeAt: (pc: number) => current.current?.nodeAt(pc) ?? null };
}
