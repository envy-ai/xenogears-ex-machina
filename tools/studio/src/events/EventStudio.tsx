import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { readCommandResult, type Backend } from '../backend/service';
import { browserDiscReader, browserCueDiscReader, openNodeDiscReader, openNodeCueDiscReader, openDiscSession, type DiscSession, type DiscMap, type DiscByteReader } from '../backend/discs';
import type { FilesAdapter } from '../platform/files';
import { nativeHost } from '../platform/host';
import { listServerDiscs, openServerDiscReader, type ServerDisc, type ServerDiscs } from '../backend/remote-discs';
import { GraphCanvas } from '../editor/GraphCanvas';
import { useNameHistory } from '../editor/NameHistory';
import { DockerList, DockerListBody, DockerListRow, DockerListEmpty } from '../editor/DockerList';
import { EditorDialogShell } from '../editor/EditorDialogShell';
import { ActorTree } from './ActorTree';
import { ActorLanes } from './ActorLanes';
import { CommandList } from './CommandList';
import { MapDiagnostics } from './MapDiagnostics';
import { actorName, eventName, instructionPhrase, reachableInstructionPcs, formatPc, type FieldGraph, type EventStructure, type MapLanes, type NameTable, type Selection } from './model';
import './events.scss';
import './workspace.scss';

type View = 'list' | 'graph' | 'lanes';
interface OpenDisc { session: DiscSession; maps: DiscMap[] }
interface RenderTime { view: View; ms: number; actor: number; event: number; map: number; instructions: number }
const emptyNames = (): NameTable => ({ schema_version: 1, variables: {}, maps: {}, messages: {} });
const Context = createContext<ReturnType<typeof useStudio> | null>(null);
function useEvents() { const state = useContext(Context); if (!state) throw new Error('Missing event studio provider'); return state; }
const nextPaint = () => new Promise<void>(resolve => requestAnimationFrame(() => requestAnimationFrame(() => resolve())));

async function paintedView(view: View): Promise<void> {
  const label = { list: 'Command list', graph: 'Flow graph', lanes: 'Actor lanes' }[view];
  const selector = { list: '.event-command-view', graph: '.graph-canvas__viewport', lanes: '.event-lanes-view' }[view];
  const deadline = performance.now() + 120_000;
  for (;;) {
    await new Promise<void>(resolve => requestAnimationFrame(() => resolve()));
    const surface = document.querySelector<HTMLElement>('.event-view-surface');
    if (surface?.getAttribute('aria-label') === label && surface.querySelector(selector)) {
      // A second animation frame bounds a completed paint of the actual view,
      // rather than timing a tab change while its asynchronous data is absent.
      await new Promise<void>(resolve => requestAnimationFrame(() => resolve()));
      return;
    }
    if (performance.now() >= deadline) throw new Error(`The ${label} view did not render before its deadline`);
  }
}

function useStudio(backend: Backend | null, folder: FilesAdapter | null, report: (error: unknown) => void, log: (kind: string, text: string) => void) {
  const [discs, setDiscs] = useState<OpenDisc[]>([]);
  const discsRef = useRef(discs); discsRef.current = discs;
  const [activeDisc, setActiveDisc] = useState<number | null>(null);
  const [map, setMap] = useState<number | null>(null);
  const [graph, setGraph] = useState<FieldGraph | null>(null);
  const [selection, setSelection] = useState<Selection>({ actor: 0, event: 0, pc: null });
  const [structure, setStructure] = useState<EventStructure | null>(null);
  const [lanes, setLanes] = useState<MapLanes | null>(null);
  const [view, setViewState] = useState<View>('list');
  const [busy, setBusy] = useState('');
  const [filter, setFilter] = useState('');
  const [find, setFind] = useState('');
  const [rename, setRename] = useState(false);
  const [sidecarReady, setSidecarReady] = useState(false);
  const [sidecarError, setSidecarError] = useState<unknown>(null);
  const [source, setSource] = useState<{ folder: FilesAdapter; text: string } | null>(null);
  const accepted = useRef<typeof source>(null);
  const renderTimes = useRef<RenderTime[]>([]);
  const revision = useRef(0);
  const structures = useRef(new Map<string, EventStructure>());
  const navigation = useRef<{ items: Selection[]; at: number }>({ items: [], at: -1 });
  const filterRef = useRef<HTMLInputElement>(null);
  const findRef = useRef<HTMLInputElement>(null);
  const context = useMemo(() => {
    const actor_counts: Record<string, number> = {};
    for (const disc of discs) for (const row of disc.maps) if (!row.placeholder && row.actor_count !== null) {
      const previous = actor_counts[row.map];
      if (previous !== undefined && previous !== row.actor_count) throw new Error(`Conflicting actor counts for map ${row.map}`);
      actor_counts[row.map] = row.actor_count;
    }
    if (map !== null && graph) actor_counts[map] = graph.actors.length;
    return { actor_counts };
  }, [discs, graph, map]);
  const call = useCallback(<T,>(op: string, args: unknown): T => {
    if (!backend) throw new Error('The backend is not ready');
    return readCommandResult<T>(backend.command(JSON.stringify({ op, args })));
  }, [backend]);
  const history = useNameHistory<NameTable>(emptyNames(), async candidate => {
    try {
    if (!folder || !sidecarReady) throw new Error('Open a project with a valid names.json before renaming');
    const result = call<{ table: NameTable }>('names.validate', { json: JSON.stringify(candidate), context });
    await folder.writeAtomic('names.json', new TextEncoder().encode(`${JSON.stringify(result.table, null, 2)}\n`));
    log('Names', `Saved names.json in ${folder.name}`);
    } catch (error) { report(error); throw error; }
  });
  useEffect(() => {
    let alive = true;
    setSidecarReady(false); setSidecarError(null); setSource(null); accepted.current = null;
    (async () => {
      if (history.getSnapshot().pending) await new Promise<void>(resolve => {
        const unsubscribe = history.subscribe(() => { if (!history.getSnapshot().pending) { unsubscribe(); resolve(); } });
      });
      if (!alive) return;
      history.reset(emptyNames());
      if (!folder) return;
      const entries = await folder.list('');
      const exists = entries.some(entry => entry.name === 'names.json' && entry.kind === 'file');
      const text = exists ? new TextDecoder('utf-8', { fatal: true }).decode(await folder.read('names.json')) : JSON.stringify(emptyNames());
      if (alive) setSource({ folder, text });
    })().catch(error => { if (alive) { setSidecarError(error); report(error); } });
    return () => { alive = false; };
    // The history controller remains stable; only selecting a folder resets it.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [folder]);
  useEffect(() => {
    if (!source || !backend || accepted.current === source) return;
    try {
      const result = call<{ table: NameTable }>('names.validate', { json: source.text, context });
      history.reset(result.table); accepted.current = source; setSidecarReady(true); setSidecarError(null);
    } catch (error) { setSidecarReady(false); setSidecarError(error); report(error); }
    // Context becoming available retries the preserved original text, without writing it.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [source, backend, context]);

  const select = useCallback((next: Selection, remember = false) => {
    if (remember) {
      const current = navigation.current;
      if (current.at < 0) current.items.push(selection);
      else current.items = current.items.slice(0, current.at + 1);
      current.items.push(next); current.at = current.items.length - 1;
    }
    setSelection(next);
  }, [selection]);
  const install = useCallback(async (bytes: number[], declared_size: number, number: number) => {
    const started = performance.now();
    renderTimes.current = [];
    setViewState('list');
    const decoded = call<FieldGraph>('field.decode', { bytes, declared_size });
    const first = decoded.entries[0];
    structures.current.clear(); navigation.current = { items: [], at: -1 };
    setMap(number); setGraph(decoded); setLanes(null); setStructure(null);
    setSelection(first ? { actor: first.actor, event: first.event, pc: first.pc } : { actor: 0, event: 0, pc: null });
    await paintedView('list');
    renderTimes.current.push({ view: 'list', ms: performance.now() - started, actor: first?.actor ?? 0, event: first?.event ?? 0, map: number, instructions: decoded.instructions.length });
  }, [call]);
  const selectDisc = useCallback((id: number) => {
    ++revision.current; structures.current.clear(); navigation.current = { items: [], at: -1 };
    setBusy(''); setActiveDisc(id); setMap(null); setGraph(null); setStructure(null); setLanes(null);
  }, []);
  const openReader = useCallback(async (reader: DiscByteReader) => {
    if (!backend) { await reader.close(); throw new Error('The backend is not ready'); }
    setBusy(`Opening ${reader.name}`);
    let session: DiscSession | undefined;
    try {
      session = await openDiscSession(backend, reader, { onProgress: progress => setBusy(`${reader.name}: ${progress.phase ?? progress.state}`) });
      const maps = await session.maps({ onProgress: progress => setBusy(`Indexing maps ${progress.completed ?? 0}/${progress.total ?? '?'}`) });
      const opened = { session, maps };
      setDiscs(current => [...current, opened]); selectDisc(session.id);
      log('Disc', `${reader.name}: recognized disc ${session.status.identity?.disc}; ${maps.length} map slots`);
    } catch (error) { if (session) await session.close().catch(report); throw error; }
    finally { setBusy(''); }
  }, [backend, log, report, selectDisc]);
  const openFiles = useCallback(async (files: File[]) => {
    if (!backend) throw new Error('The backend is not ready');
    const claimed = new Set<string>();
    for (const cue of files.filter(file => /\.cue$/i.test(file.name))) {
      const track = call<{ file: string }>('disc.cue', { text: await cue.text() });
      const name = track.file.split(/[/\\]/).at(-1);
      const companion = files.find(file => file.name === name);
      if (!companion) throw new Error(`Select the CUE's companion ${track.file} together with ${cue.name}`);
      claimed.add(companion.name);
      await openReader(await browserCueDiscReader(backend, cue, companion));
    }
    for (const file of files.filter(file => !/\.cue$/i.test(file.name) && !claimed.has(file.name))) await openReader(browserDiscReader(file));
  }, [backend, call, openReader]);
  const openNativePath = useCallback(async (path: string) => {
    if (!nativeHost || !backend) throw new Error('Opening a disc path requires the desktop host');
    const fs = nativeHost.require('node:fs/promises');
    const reader = /\.cue$/i.test(path)
      ? await openNodeCueDiscReader(backend, path, fs, nativeHost.require('node:path'))
      : await openNodeDiscReader(path, fs);
    await openReader(reader);
  }, [backend, openReader]);
  const openServerDisc = useCallback(async (file: ServerDisc, catalogue: ServerDiscs) => {
    if (!backend) throw new Error('The backend is not ready');
    await openReader(await openServerDiscReader(backend, file, catalogue));
  }, [backend, openReader]);
  const loadMap = useCallback(async (discId: number, number: number) => {
    const disc = discsRef.current.find(disc => disc.session.id === discId);
    if (!disc) throw new Error(`Disc session ${discId} is not open`);
    const token = ++revision.current;
    setBusy(`Loading map ${number}`);
    try {
      const result = await disc.session.mapEvents(number);
      if (token !== revision.current) return;
      setActiveDisc(discId);
      if (!result.component) { setMap(number); setGraph(null); setStructure(null); setLanes(null); return; }
      await install(result.component.bytes, result.component.declared_size, number);
    } finally { if (token === revision.current) setBusy(''); }
  }, [install]);
  const closeDisc = async (id: number) => {
    const disc = discsRef.current.find(item => item.session.id === id);
    if (!disc) return;
    await disc.session.close();
    setDiscs(current => current.filter(item => item !== disc));
    if (activeDisc === id) { setBusy(''); ++revision.current; setActiveDisc(null); setGraph(null); setMap(null); setLanes(null); setStructure(null); }
  };
  useEffect(() => () => { for (const disc of discsRef.current) void disc.session.close().catch(report); }, [report]);
  useEffect(() => {
    if (!graph || graph.actors.length === 0) return;
    try {
      const key = `${selection.actor}:${selection.event}`;
      let tree = structures.current.get(key);
      if (!tree) { tree = call<EventStructure>('field.structure', { graph, actor: selection.actor, event: selection.event }); structures.current.set(key, tree); }
      setStructure(tree);
    } catch (error) { setStructure(null); report(error); }
  }, [graph, selection.actor, selection.event, call, report]);
  useEffect(() => {
    if (graph && view === 'lanes' && !lanes) {
      try { setLanes(call<MapLanes>('field.lanes', { graph })); } catch (error) { report(error); }
    }
  }, [graph, view, lanes, call, report]);
  const setView = useCallback(async (next: View) => {
    if (renderTimes.current.some(time => time.view === next && time.map === map)) { setViewState(next); await nextPaint(); return; }
    const started = performance.now(); setViewState(next);
    if (graph) await paintedView(next); else await nextPaint();
    if (graph && map !== null) renderTimes.current.push({ view: next, ms: performance.now() - started, actor: selection.actor, event: selection.event, map, instructions: graph.instructions.length });
  }, [graph, map, selection]);
  const navigate = useCallback((pc: number) => {
    if (!graph) return;
    if (!graph.instructions.some(instruction => instruction.pc === pc)) {
      report(new Error(`Target ${formatPc(pc)} has no decoded instruction; the original diagnostic remains visible`)); return;
    }
    const currentEntry = graph.entries.find(entry => entry.actor === selection.actor && entry.event === selection.event);
    const withinCurrent = currentEntry && reachableInstructionPcs(graph, currentEntry.pc).has(pc);
    const entry = withinCurrent ? currentEntry : graph.entries.find(entry => entry.pc === pc)
      ?? graph.entries.find(entry => reachableInstructionPcs(graph, entry.pc).has(pc));
    select({ actor: entry?.actor ?? selection.actor, event: entry?.event ?? selection.event, pc }, true);
  }, [graph, selection, select, report]);
  const follow = useCallback((pc: number) => {
    if (!graph) return;
    const edge = graph.edges.find(edge => edge.from_pc === pc && edge.status === 'followed' && edge.kind !== 'fallthrough');
    if (!edge) return;
    navigate(edge.to_pc);
  }, [graph, navigate]);
  useEffect(() => {
    const key = (event: KeyboardEvent) => {
      if (event.defaultPrevented) return;
      const editing = event.target instanceof HTMLElement && Boolean(event.target.closest('input,textarea,select,[contenteditable=true]'));
      if (document.querySelector('[aria-modal=true]')) return;
      if ((event.ctrlKey || event.metaKey) && ['1','2','3'].includes(event.key)) { event.preventDefault(); void setView((['list','graph','lanes'] as View[])[Number(event.key) - 1]); }
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'f') { event.preventDefault(); findRef.current?.focus(); }
      if (event.key === 'F2' && graph) { event.preventDefault(); setRename(true); }
      const interactive = event.target instanceof HTMLElement && Boolean(event.target.closest('button,a,[role=button],[role=tab],[role=menuitem]'));
      if (!editing && !interactive && event.target instanceof HTMLElement && event.target.closest('.event-view-surface') && event.key === 'Enter' && selection.pc !== null) { event.preventDefault(); follow(selection.pc); }
      if (event.altKey && ['ArrowLeft','ArrowRight'].includes(event.key)) {
        event.preventDefault(); const nav = navigation.current; const at = nav.at + (event.key === 'ArrowLeft' ? -1 : 1);
        if (at >= 0 && at < nav.items.length) { nav.at = at; setSelection(nav.items[at]); }
      }
    };
    window.addEventListener('keydown', key); return () => window.removeEventListener('keydown', key);
  }, [graph, selection, setView, follow]);
  const state = { backend, discs, activeDisc, setActiveDisc: selectDisc, map, graph, selection, select, structure, lanes, view, setView, busy, filter, setFilter, find, setFind, rename, setRename, history, sidecarReady, sidecarError, filterRef, findRef, openFiles, openNativePath, openServerDisc, loadMap, closeDisc, follow, navigate, install, report, renderTimes };
  const latest = useRef(state); latest.current = state;
  useEffect(() => {
    if (import.meta.env.VITE_XEM_STUDIO_TEST !== '1' || !backend) return;
    const hook = {
      loadComponent: async (bytes: number[], declared_size: number, number = 0) => { await latest.current.install(bytes, declared_size, number); },
      openNativePath: (path: string) => latest.current.openNativePath(path),
      selectMap: (discId: number, number: number) => latest.current.loadMap(discId, number),
      setView: (next: View) => latest.current.setView(next),
      snapshot: () => { const s = latest.current; return { view: s.view, selection: s.selection, map: s.map, instructions: s.graph?.instructions.length ?? 0, discs: s.discs.map(disc => ({ id: disc.session.id, disc: disc.session.status.identity?.disc, mapCount: disc.maps.length, placeholders: disc.maps.filter(map => map.placeholder).length })), renderTimes: [...s.renderTimes.current] }; },
    };
    (window as unknown as { __xemStudioTest: typeof hook }).__xemStudioTest = hook;
    if (nativeHost?.App.argv.some(argument => argument.startsWith('--studio-events-test='))) void import('./test-driver').then(driver => driver.runEventsTest(hook)).catch(report);
    return () => { delete (window as unknown as { __xemStudioTest?: typeof hook }).__xemStudioTest; };
  }, [backend, report]);
  return state;
}

export function EventStudioProvider({ backend, folder, report, log, children }: { backend: Backend | null; folder: FilesAdapter | null; report(error: unknown): void; log(kind: string, text: string): void; children: ReactNode }) {
  const value = useStudio(backend, folder, report, log);
  return <Context.Provider value={value}>{children}</Context.Provider>;
}

export function DiscBrowser() {
  const s = useEvents();
  const [path, setPath] = useState('');
  const [catalogue, setCatalogue] = useState<ServerDiscs | null>(null);
  const [serverId, setServerId] = useState('');
  const [serverLoading, setServerLoading] = useState(false);
  const browseServer = async () => {
    setServerLoading(true); setCatalogue(null); setServerId('');
    try {
      const result = await listServerDiscs();
      setCatalogue(result); setServerId(result.files[0]?.id ?? '');
    } catch (error) { s.report(error); }
    finally { setServerLoading(false); }
  };
  const openServer = async () => {
    if (!catalogue) return;
    const file = catalogue.files.find(file => file.id === serverId);
    if (!file) throw new Error('Select an image from the server disc catalogue');
    setServerLoading(true);
    try { await s.openServerDisc(file, catalogue); }
    finally { setServerLoading(false); }
  };
  const disc = s.discs.find(disc => disc.session.id === s.activeDisc);
  const maps = disc?.maps.filter(map => `${map.map} ${map.placeholder ? 'placeholder' : ''}`.includes(s.filter.toLowerCase())) ?? [];
  return <section className="event-disc-browser" aria-label="Disc and map browser" onKeyDown={event => {
    if (!(event.target instanceof HTMLElement) || !event.target.matches('[data-map]')) return;
    const buttons = [...event.currentTarget.querySelectorAll<HTMLButtonElement>('[data-map]')]; const at = buttons.indexOf(event.target as HTMLButtonElement);
    const next = event.key === 'ArrowDown' ? at + 1 : event.key === 'ArrowUp' ? at - 1 : event.key === 'Home' ? 0 : event.key === 'End' ? buttons.length - 1 : -1;
    if (next >= 0 && next < buttons.length) { event.preventDefault(); buttons[next].focus(); }
  }}>
    <h2 className="editor-heading editor-heading--section">Discs</h2>
    {nativeHost ? <form onSubmit={event => { event.preventDefault(); s.openNativePath(path).catch(s.report); }} className="editor-field-list"><label className="editor-field editor-field-label"><span>Disc image path</span><input className="editor-control editor-control--text" value={path} onChange={event => setPath(event.target.value)} /></label><button className="editor-button" disabled={!s.backend || Boolean(s.busy)} type="submit">Open disc path</button></form>
      : <label className="editor-field editor-field-label"><span>Open disc files</span><input className="editor-control editor-control--file" type="file" multiple accept=".chd,.bin,.cue" disabled={!s.backend || Boolean(s.busy)} onChange={event => { const files = [...event.target.files ?? []]; event.target.value = ''; s.openFiles(files).catch(s.report); }} /></label>}
    {!nativeHost && <div className="editor-field-list" aria-label="Server discs">
      <button className="editor-button" disabled={!s.backend || Boolean(s.busy) || serverLoading} onClick={() => void browseServer()}>Browse server discs</button>
      {catalogue?.enabled && <><label className="editor-field editor-field-label"><span>Server disc image</span><select className="editor-control editor-control--select" value={serverId} disabled={serverLoading || Boolean(s.busy)} onChange={event => setServerId(event.target.value)}>
        {!catalogue.files.length && <option value="">No supported images in the server folder</option>}
        {catalogue.files.map(file => <option key={file.id} value={file.id}>{file.name}</option>)}
      </select></label><button className="editor-button" disabled={!serverId || serverLoading || Boolean(s.busy)} onClick={() => openServer().catch(s.report)}>Open server disc</button></>}
      <p className="editor-text editor-text--secondary" role="status">{serverLoading ? 'Reading server discs…' : catalogue ? catalogue.enabled ? 'Server files · read only' : 'Server disc access is not configured.' : 'Open an image from the configured server folder.'}</p>
    </div>}
    <p className="editor-text" role="status">{s.busy || (s.discs.length ? `${s.discs.length} recognized disc sessions` : 'Open your own CHD or BIN/CUE image.')}</p>
    {s.discs.map(item => <div className="event-disc-row" key={item.session.id}><button className="editor-button" aria-pressed={s.activeDisc === item.session.id} onClick={() => s.setActiveDisc(item.session.id)}>Disc {item.session.status.identity?.disc ?? 'test'} · {item.maps.length} maps</button><button className="editor-button editor-button--compact" aria-label={`Close disc ${item.session.id}`} title="Close disc" onClick={() => s.closeDisc(item.session.id).catch(s.report)}>Close</button></div>)}
    <label className="editor-field editor-field-label"><span>Filter maps</span><input ref={s.filterRef} className="editor-control editor-control--text" value={s.filter} onChange={event => s.setFilter(event.target.value)} /></label>
    <DockerList><DockerListBody>{maps.map(item => <DockerListRow key={item.map} intrinsicHeight columns="1fr"><button data-map={item.map} className="editor-button event-map-row" aria-pressed={s.map === item.map && s.activeDisc === disc?.session.id} onClick={() => disc && s.loadMap(disc.session.id, item.map).catch(s.report)}><span>Map {item.map}</span><span className="editor-text editor-text--secondary">{item.placeholder ? 'Placeholder' : `${item.actor_count} actors · ${item.script_size} bytes`}</span></button></DockerListRow>)}{!maps.length && <DockerListEmpty>{disc ? 'No matching maps.' : 'No disc open.'}</DockerListEmpty>}</DockerListBody></DockerList>
    {s.graph && s.map !== null && <ActorTree graph={s.graph} names={s.history.value} map={s.map} selection={s.selection} onSelect={s.select} />}
  </section>;
}

export function EventViews() {
  const s = useEvents();
  const names = s.history.value;
  const [markerSelection, setMarkerSelection] = useState<string | null>(null);
  useEffect(() => setMarkerSelection(null), [s.graph, s.selection.pc]);
  const reachable = useMemo(() => s.graph ? reachableInstructionPcs(s.graph, s.structure?.entry_pc ?? null) : new Set<number>(), [s.graph, s.structure]);
  const graphNodes = useMemo(() => {
    if (!s.graph || s.map === null || s.view !== 'graph') return [];
    const rows = s.graph.instructions.filter(instruction => reachable.has(instruction.pc));
    // Deterministic layers along followed edges. Back edges stay wires, never relocate bytecode.
    const layer = new Map<number, number>();
    const incomingEdges = new Map<number, typeof s.graph.edges>();
    for (const edge of s.graph.edges) if (edge.status === 'followed' && edge.from_pc < edge.to_pc) {
      const list = incomingEdges.get(edge.to_pc) ?? []; list.push(edge); incomingEdges.set(edge.to_pc, list);
    }
    for (const instruction of rows) {
      const incoming = incomingEdges.get(instruction.pc) ?? [];
      layer.set(instruction.pc, incoming.reduce((maximum, edge) => Math.max(maximum, (layer.get(edge.from_pc) ?? 0) + 1), 0));
    }
    const levels = new Map<number, number>();
    const nodes = rows.map(instruction => { const x = layer.get(instruction.pc) ?? 0; const row = levels.get(x) ?? 0; levels.set(x, row + 1); return { id: String(instruction.pc), x: row * 310, y: x * 110, width: 280, height: 84, label: `${formatPc(instruction.pc)} ${instructionPhrase(instruction, names, s.map!, s.selection.actor)}` }; });
    const missing = new Map<string, { pc: number; status: string }>();
    for (const edge of s.graph.edges) if (reachable.has(edge.from_pc) && !reachable.has(edge.to_pc)) missing.set(`target:${edge.to_pc}:${edge.status}`, { pc: edge.to_pc, status: edge.status });
    let marker = 0;
    for (const [id, target] of missing) nodes.push({ id, x: -310, y: marker++ * 110, width: 280, height: 84, label: `${target.status} target ${formatPc(target.pc)} · not an instruction owner` });
    return nodes;
  }, [s.graph, s.map, reachable, names, s.view, s.selection.actor]);
  const graphEdges = useMemo(() => s.graph?.edges.filter(edge => reachable.has(edge.from_pc)).map((edge, index) => ({ id: `${index}`, from: String(edge.from_pc), to: reachable.has(edge.to_pc) ? String(edge.to_pc) : `target:${edge.to_pc}:${edge.status}`, kind: edge.kind, status: edge.status })) ?? [], [s.graph, reachable]);
  return <section className="event-workspace" aria-label="Field events" data-studio-panel="events" tabIndex={-1}>
    <header className="event-view-toolbar"><div role="tablist" aria-label="Event views">{(['list','graph','lanes'] as View[]).map((view, index) => <button className="editor-button" role="tab" aria-selected={s.view === view} key={view} onClick={() => void s.setView(view)} title={`Ctrl+${index + 1}`}>{['Command list','Flow graph','Actor lanes'][index]}</button>)}</div><button className="editor-button" disabled={!s.graph} onClick={() => s.setRename(true)} title="Rename (F2)">Rename…</button></header>
    <label className="editor-field editor-field-label event-find"><span>Find within map</span><input ref={s.findRef} className="editor-control editor-control--text" value={s.find} onChange={event => s.setFind(event.target.value)} placeholder="Mnemonic, name or PC" onKeyDown={event => { if (event.key === 'Enter' && s.graph) { const text = s.find.toLowerCase(); const found = s.graph.instructions.find(instruction => `${instructionPhrase(instruction, names, s.map ?? 0)} ${formatPc(instruction.pc)}`.toLowerCase().includes(text)); if (found) { const entry = s.graph.entries.find(entry => reachableInstructionPcs(s.graph!, entry.pc).has(found.pc)); s.select({ actor: entry?.actor ?? s.selection.actor, event: entry?.event ?? s.selection.event, pc: found.pc }, true); } } }} /></label>
    {s.graph && s.map !== null ? <><p className="editor-text event-caption">Map {s.map} · {actorName(names, s.map, s.selection.actor)} · {eventName(names, s.map, s.selection.actor, s.selection.event)} · {s.graph.instructions.length} map instructions · Read-only game data</p>
      <div className="event-view-surface" role="tabpanel" aria-label={s.view === 'list' ? 'Command list' : s.view === 'graph' ? 'Flow graph' : 'Actor lanes'}>
        {s.view === 'list' && s.graph.actors.length === 0 && <div className="event-command-view"><DockerList><DockerListEmpty>This component has no actor/event entries.</DockerListEmpty></DockerList></div>}
        {s.view === 'list' && s.structure && <CommandList graph={s.graph} structure={s.structure} names={names} map={s.map} selectedPc={s.selection.pc} onSelect={pc => s.select({ ...s.selection, pc })} onFollow={s.navigate} />}
        {s.view === 'graph' && <GraphCanvas nodes={graphNodes} edges={graphEdges} selectedId={markerSelection ?? (s.selection.pc === null ? null : String(s.selection.pc))} onSelect={id => { if (id.startsWith('target:')) setMarkerSelection(id); else { setMarkerSelection(null); s.select({ ...s.selection, pc: Number(id) }); } }} onActivate={id => { if (!id.startsWith('target:')) s.follow(Number(id)); }} ariaLabel="Event flow graph" />}
        {s.view === 'lanes' && s.lanes && <ActorLanes lanes={s.lanes} names={names} map={s.map} selection={s.selection} onSelect={next => s.select(next, true)} />}
      </div>{s.view !== 'list' && <MapDiagnostics graph={s.graph} onNavigate={s.navigate} />}</> : <p className="editor-text editor-text--secondary">{s.map !== null ? `Map ${s.map} is a placeholder.` : 'Open a disc and select a map to inspect its events.'}</p>}
    {s.rename && <NameEditor onClose={() => s.setRename(false)} />}
  </section>;
}

function NameEditor({ onClose }: { onClose(): void }) {
  const s = useEvents();
  const [scope, setScope] = useState<'actor'|'event'|'variable'|'message'>('event');
  const [id, setId] = useState('0');
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => { input.current?.focus(); }, []);
  const value = s.history.draft;
  const actor = value.maps[String(s.map)]?.actors[String(s.selection.actor)];
  const name = scope === 'actor' ? actor?.name ?? '' : scope === 'event' ? actor?.events[String(s.selection.event)] ?? '' : scope === 'variable' ? value.variables[id] ?? '' : value.messages[id] ?? '';
  const setName = (name: string) => {
    const next = structuredClone(value);
    if (scope === 'variable') next.variables[id] = name;
    else if (scope === 'message') next.messages[id] = name;
    else {
      const map = next.maps[String(s.map)] ??= { actors: {} };
      const actor = map.actors[String(s.selection.actor)] ??= { events: {} };
      if (scope === 'actor') actor.name = name; else actor.events[String(s.selection.event)] = name;
    }
    s.history.setValue(next);
  };
  return <EditorDialogShell containFocus backdropClassName="editor-dialog-backdrop" onClose={() => { if (!s.history.pending) { if (s.history.dirty) s.history.cancelDraft(); onClose(); } }}><section className="editor-small-dialog event-name-dialog" role="dialog" aria-modal="true" aria-label="Names" {...s.history.fieldProps}>
    <h2 className="editor-heading editor-heading--section">Names · metadata only</h2>
    <p className="editor-text">Names are stored only in the opened project's names.json.</p>
    {s.history.error ? <pre className="editor-text editor-text--danger" role="alert">{s.history.error instanceof Error ? s.history.error.stack ?? s.history.error.message : String(s.history.error)}</pre> : null}
    {!s.sidecarReady && <p className="editor-text editor-text--danger">{s.sidecarError instanceof Error ? s.sidecarError.message : 'Open a project folder with a valid names sidecar before renaming.'}</p>}
    <label className="editor-field editor-field-label"><span>Name scope</span><select className="editor-control editor-control--select" disabled={s.history.pending} value={scope} onChange={event => { if (s.history.getSnapshot().pending) return; s.history.cancelDraft(); setScope(event.target.value as typeof scope); }}><option value="event">Selected event</option><option value="actor">Selected actor</option><option value="variable">Variable slot</option><option value="message">Message ID</option></select></label>
    {(scope === 'variable' || scope === 'message') && <label className="editor-field editor-field-label"><span>{scope === 'variable' ? 'Variable slot' : 'Message ID'}</span><input className="editor-control editor-control--text" disabled={s.history.pending} value={id} onChange={event => { if (s.history.getSnapshot().pending) return; s.history.cancelDraft(); setId(event.target.value); }} /></label>}
    <label className="editor-field editor-field-label"><span>Name</span><input ref={input} className="editor-control editor-control--text" disabled={!s.sidecarReady || s.history.pending} value={name} onChange={event => setName(event.target.value)} /></label>
    <div className="studio-toolbar"><button className="editor-button editor-button--primary" disabled={!s.sidecarReady || s.history.pending || !s.history.dirty} onClick={() => s.history.commit().catch(s.report)}>Save name</button><button className="editor-button" disabled={!s.history.canUndo} onClick={() => s.history.undo().catch(s.report)}>Undo name</button><button className="editor-button" disabled={!s.history.canRedo} onClick={() => s.history.redo().catch(s.report)}>Redo name</button><button className="editor-button" disabled={s.history.pending} onClick={() => { if (s.history.dirty) s.history.cancelDraft(); onClose(); }}>Close names</button></div>
  </section></EditorDialogShell>;
}
