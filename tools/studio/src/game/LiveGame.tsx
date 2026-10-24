import { createContext, useContext, useEffect, useMemo, useRef, useState, useSyncExternalStore, type ReactNode } from 'react';
import { GameController } from './controller';
import { browserGameTransport, type GameWindow } from './browser';
import { launchNativeGame, type NativeLaunch } from './native';
import { gameJson, type GameValue } from './protocol';
import { readLivePc, resolveLivePc, formatLivePc } from './pc';
import type { SchemaModel, SchemaNode } from './schema';
import { nativeHost, selectNativePath } from '../platform/host';
import { NumberField } from '../editor/NumberField';
import { RecordSelect } from '../editor/RecordSelect';
import { EditorDialogShell } from '../editor/EditorDialogShell';
import { DockerList, DockerListBody, DockerListRow, DockerListEmpty } from '../editor/DockerList';
import { useEventNavigation, useEventPackagePlay } from '../events/EventStudio';
import type { SourceContext } from '../backend/discs';
import './live-game.scss';

const Context = createContext<GameController | null>(null);
export function useLiveGame() {
  const controller = useContext(Context);
  if (!controller) throw new Error('Missing live game provider');
  const state = useSyncExternalStore(controller.subscribe, controller.getSnapshot);
  return { controller, state };
}
export function LiveGameProvider({ report, log, children }: { report(error: unknown): void; log(kind: string, text: string): void; children: ReactNode }) {
  const controller = useMemo(() => new GameController(report, log), [report, log]);
  useEffect(() => () => { void controller.disconnect().catch(report); }, [controller, report]);
  return <Context.Provider value={controller}>{children}</Context.Provider>;
}
function errorText(error: unknown) { return error instanceof Error ? error.stack ?? error.message : String(error); }

export function LiveControls({ report }: { report(error: unknown): void }) {
  const { controller, state } = useLiveGame();
  const navigation = useEventNavigation();
  const eventPackage = useEventPackagePlay();
  const [url, setUrl] = useState('/runtime/');
  const target = useRef<Window | null>(null);
  const testStarted = useRef(false);
  useEffect(() => {
    if (import.meta.env.VITE_XEM_STUDIO_TEST !== '1' || !nativeHost || testStarted.current
      || !nativeHost.App.argv.some(value => value.startsWith('--studio-live-test='))) return;
    testStarted.current = true;
    void import('../../tests/e2e/live-nw-driver').then(driver => driver.runLiveNwTest(controller)).catch(report);
  }, [controller, report]);
  const [launch, setLaunch] = useState<NativeLaunch>({ executable: '', disc: '', settingsDirectory: '', cardsDirectory: '' });
  const [attaching, setAttaching] = useState(false);
  const [count, setCount] = useState(1);
  const [name, setName] = useState('Before change');
  const [snapshot, setSnapshot] = useState('');
  const [restore, setRestore] = useState(false);
  const [pcStatus, setPcStatus] = useState('Refresh a live actor to resolve its exact instruction.');
  const [actor, setActor] = useState('');
  const busy = Boolean(state.busy) || attaching;
  const run = (operation: () => Promise<unknown>) => { void Promise.resolve().then(operation).catch(report); };
  useEffect(() => { setSnapshot(''); setRestore(false); }, [state.schema]);
  useEffect(() => { setPcStatus('Refresh a live actor to resolve its exact instruction.'); }, [state.revision, navigation.graph]);
  useEffect(() => {
    const key = (event: KeyboardEvent) => {
      if (event.defaultPrevented || !state.connected || busy || document.querySelector('[aria-modal=true]')) return;
      if (event.target instanceof HTMLElement && event.target.closest('input,textarea,select,[contenteditable=true]')) return;
      if (event.key === 'F7' && !event.shiftKey && !event.ctrlKey && !event.altKey) { event.preventDefault(); run(() => controller.step(count)); }
      if (event.key === 'F6' && event.shiftKey && !event.ctrlKey && !event.altKey) { event.preventDefault(); run(() => controller.setRunning(!state.running)); }
    };
    window.addEventListener('keydown', key); return () => window.removeEventListener('keydown', key);
  }, [controller, state.connected, state.running, busy, count]);
  const attach = async () => {
    setAttaching(true);
    try {
      if (nativeHost) await controller.connect(launchNativeGame(launch, nativeHost.require('node:child_process').spawn, nativeHost.require('node:process').env));
      else {
        const address = new URL(url, location.href);
        if (address.origin !== location.origin || !['http:', 'https:'].includes(address.protocol)) throw new Error('Choose a same-origin game page');
        if (!target.current || target.current.closed) target.current = window.open(address.href, 'xem-studio-live-game');
        if (!target.current) throw new Error('The browser blocked the game window; open it using this button');
        const deadline = performance.now() + 30_000;
        while (target.current.location.href === 'about:blank') {
          if (target.current.closed) throw new Error('The game window closed before navigation completed');
          if (performance.now() >= deadline) throw new Error('The game window did not finish its initial navigation');
          await new Promise(resolve => setTimeout(resolve, 25));
        }
        await controller.connect(await browserGameTransport(target.current as unknown as GameWindow, location.origin));
      }
    } finally { setAttaching(false); }
  };
  useEffect(() => {
    eventPackage.registerSource(async source => {
      if (busy) throw new Error('Wait for the current game operation before reading source context');
      const context=await controller.command({cmd:'field_source_context',map:source.map,source_sha256:source.source_sha256}) as unknown as SourceContext;
      if (context.module_sha256!==controller.getSnapshot().schema?.identity.module) throw new Error('Runtime source context module changed during capture');
      return context;
    });
    return () => eventPackage.registerSource(null);
  },[eventPackage.registerSource,controller,busy]);
  useEffect(() => {
    eventPackage.register(async build => {
      if (busy) throw new Error('Wait for the current game operation before launching a package');
      setAttaching(true);
      try {
        if (nativeHost) {
          if (!build.nativePath) throw new Error('Build this package in a native project folder before native playback');
          await controller.disconnect();
          await controller.connect(launchNativeGame({ ...launch, packages: build.nativePath }, nativeHost.require('node:child_process').spawn, nativeHost.require('node:process').env));
        } else {
          const game = target.current;
          if (!game || game.closed || game.location.origin !== location.origin) throw new Error('Open the same-origin game using Live game, import its original disc and boot it before package playback');
          const api = (game as unknown as GameWindow).xem;
          if (!api?.packages || !api.boot) throw new Error('This game page does not expose package staging and fresh boot');
          api.setRunning(false);
          await controller.disconnect();
          // The runtime validates Uint8Array in its own window realm.
          const bytes = new (game as Window & typeof globalThis).Uint8Array(build.bytes);
          api.packages.stage(bytes, { enabled: true });
          await api.boot({ run: false, rewind: false });
          await controller.connect(await browserGameTransport(game as unknown as GameWindow, location.origin));
        }
        await controller.setRunning(true);
      } finally { setAttaching(false); }
    });
    return () => eventPackage.register(null);
  }, [eventPackage.register, controller, launch, busy]);
  const jump = async () => {
    const loaded = { disc: navigation.disc, map: navigation.map, graph: navigation.graph, bytes: navigation.bytes };
    try {
      const live = await controller.paused(request => readLivePc(request, loaded, actor === '' ? undefined : Number(actor)));
      // The loaded map may have changed while live reads were pending.
      if (navigation.graph !== loaded.graph) throw new Error('The loaded map changed during live PC resolution');
      const selection = resolveLivePc(live, loaded);
      navigation.reveal(selection);
      setPcStatus(`${formatLivePc(live)} · selected Event ${selection.event}`);
    } catch (error) { setPcStatus(errorText(error)); throw error; }
  };
  return <section className="live-game" data-studio-panel="live" aria-label="Live game" tabIndex={-1}>
    <p className="editor-text" role="status">{attaching ? 'Connecting…' : state.connected ? `${state.kind} · ${busy ? 'Busy' : state.running ? 'Running · acknowledged' : 'Paused · acknowledged'}` : 'Disconnected'}</p>
    {!state.connected && <div className="editor-field-list">
      {nativeHost ? (Object.entries({ executable: 'Runtime executable', disc: 'Game disc path', settingsDirectory: 'Isolated settings directory', cardsDirectory: 'Isolated cards directory' }) as [keyof NativeLaunch, string][]).map(([key, label]) => <div className="live-path" key={key}><label className="editor-field editor-field-label"><span>{label}</span><input className="editor-control editor-control--text" value={launch[key]} onChange={event => setLaunch(current => ({ ...current, [key]: event.target.value }))} /></label><button className="editor-button" disabled={busy} onClick={() => run(async () => { const path = await selectNativePath(key.endsWith('Directory')); setLaunch(current => ({ ...current, [key]: path })); })}>Browse…</button></div>)
        : <label className="editor-field editor-field-label"><span>Same-origin game page</span><input className="editor-control editor-control--text" value={url} onChange={event => setUrl(event.target.value)} /></label>}
      <button className="editor-button editor-button--primary" disabled={busy} onClick={() => run(attach)}>{nativeHost ? 'Launch & connect' : 'Open game & attach'}</button>
      {!nativeHost && <p className="editor-text editor-text--secondary">Import your disc and boot in the game window, then attach again. Reuses that window without navigating it.</p>}
    </div>}
    {state.connected && <>
      <div className="live-toolbar"><button className="editor-button" disabled={busy} title="Run/pause (Shift+F6)" onClick={() => run(() => controller.setRunning(!state.running))}>{state.running ? 'Pause' : 'Run'}</button><NumberField label="Frames" value={count} onChange={setCount} min={1} max={10000} emptyAs="nan" disabled={busy} /><button className="editor-button" disabled={busy} title="Step (F7)" onClick={() => run(() => controller.step(count))}>Step</button></div>
      <pre className="editor-text editor-text--code live-status" aria-label="Live runtime status">{state.status === null ? '' : gameJson(state.status)}</pre>
      <div className="live-toolbar"><button className="editor-button" disabled={busy} onClick={() => run(() => controller.refresh())}>Refresh status</button><button className="editor-button" disabled={busy} onClick={() => run(() => controller.disconnect())}>Disconnect</button></div>
      <div className="live-toolbar"><label className="editor-field editor-field-label"><span>Snapshot name</span><input className="editor-control editor-control--text" value={name} onChange={event => setName(event.target.value)} /></label><button className="editor-button" disabled={busy} onClick={() => run(async () => { await controller.snapshot(name); setSnapshot(name); })}>Capture</button></div>
      <div className="live-toolbar"><RecordSelect label="Session snapshots" records={state.snapshots.map(name => ({ id: name, name }))} value={snapshot} onChange={setSnapshot} noneOption="Select snapshot" disabled={busy} /><button className="editor-button" disabled={busy || !snapshot} onClick={() => setRestore(true)}>Restore…</button></div>
      <div className="live-toolbar"><label className="editor-field editor-field-label"><span>Live actor (blank = current)</span><input className="editor-control editor-control--text" inputMode="numeric" value={actor} onChange={event => setActor(event.target.value)} /></label><button className="editor-button" disabled={busy} onClick={() => run(jump)}>Jump to node</button></div>
      <pre className="editor-text editor-text--code live-pc-status" role="status">{pcStatus}</pre>
    </>}
    {!state.connected && state.kind !== null && <button className="editor-button" onClick={() => run(() => controller.disconnect())}>{state.kind === 'native' ? 'Close owned game' : 'Detach lost game'}</button>}
    {state.error && <pre className="editor-text editor-text--danger" role="alert">{errorText(state.error)}</pre>}
    {restore && <EditorDialogShell onClose={() => setRestore(false)} backdropClassName="editor-dialog-backdrop" containFocus><section className="editor-small-dialog" role="dialog" aria-modal="true" aria-label="Restore session snapshot"><h2 className="editor-heading editor-heading--section">Restore snapshot</h2><p className="editor-text">Restore “{snapshot}” and replace the current game state?</p><div className="live-toolbar"><button className="editor-button" onClick={() => setRestore(false)}>Cancel</button><button className="editor-button editor-button--primary" disabled={busy} onClick={() => run(async () => { await controller.restore(snapshot); setRestore(false); })}>Restore</button></div></section></EditorDialogShell>}
  </section>;
}

export function SchemaInspector({ report }: { report(error: unknown): void }) {
  const { controller, state } = useLiveGame();
  const [filter, setFilter] = useState('');
  const [limit, setLimit] = useState(128);
  const [expanded, setExpanded] = useState(new Set<string>());
  const [selected, setSelected] = useState<SchemaNode | null>(null);
  const [value, setValue] = useState<{ revision: number; data: GameValue } | undefined>(undefined);
  const [draft, setDraft] = useState<{ path: string; revision: number; text: string; original: string; schema: SchemaModel | null } | null>(null);
  const [pendingNode, setPendingNode] = useState<SchemaNode | null>(null);
  const [confirmed, setConfirmed] = useState<{ revision: number; text: string } | null>(null);
  const request = useRef(0);
  const schema = state.schema;
  const dirty = draft !== null && draft.text !== draft.original;
  const currentDraft = draft?.schema === schema && draft?.revision === state.revision;
  useEffect(() => {
    ++request.current; setSelected(null); setValue(undefined); setConfirmed(null); setExpanded(new Set()); setPendingNode(null);
    // A disconnected draft is retained for reference, never replayed on another schema.
    setDraft(current => current && current.text !== current.original ? current : null);
  }, [schema]);
  const scalarText = (node: SchemaNode, next: GameValue) => typeof next === 'string'
    ? schema?.resolve(node.type)?.kind === 'pointer' && /^0x[0-9a-f]+$/i.test(next) ? BigInt(next).toString() : next
    : gameJson(next);
  const read = async (node: SchemaNode) => {
    const token = ++request.current;
    setSelected(node); setValue(undefined); setDraft(null); setConfirmed(null); setPendingNode(null);
    if (node.unsupported || node.range) return;
    const revision = controller.getSnapshot().revision;
    const next = await controller.inspect(node);
    if (token !== request.current || controller.getSnapshot().schema !== schema || controller.getSnapshot().revision !== revision) return;
    const text = scalarText(node, next);
    setValue({ revision, data: next }); setDraft({ path: node.path, revision, text, original: text, schema });
  };
  const write = async () => {
    if (!selected || !draft || draft.path !== selected.path || draft.schema !== controller.getSnapshot().schema || draft.revision !== controller.getSnapshot().revision) throw new Error('Enter a scalar draft for the selected path and current timeline');
    const token = ++request.current;
    const next = await controller.write(selected, draft.text);
    if (token !== request.current || controller.getSnapshot().schema !== schema) return;
    const revision = controller.getSnapshot().revision;
    const text = scalarText(selected, next);
    setValue({ revision, data: next }); setDraft({ ...draft, revision, text, original: text }); setConfirmed({ revision, text: `Read-back: ${gameJson(next)} · confirmed` });
    if (pendingNode) await read(pendingNode);
  };
  const choose = async (node: SchemaNode) => {
    if (dirty) { if (node.path !== selected?.path || node.range) setPendingNode(node); return; }
    await read(node);
  };
  const cancelDraft = async () => {
    const target = pendingNode ?? (draft?.schema === schema ? selected : null);
    setDraft(null); setPendingNode(null); setConfirmed(null);
    if (target) await read(target);
  };
  const roots = schema?.roots(filter) ?? [];
  const rows: { node: SchemaNode; key: string }[] = [];
  const add = (nodes: SchemaNode[], prefix: string) => { nodes.forEach((node, i) => { const key = `${prefix}/${node.label}:${i}`; rows.push({ node, key }); if (expanded.has(key)) add(schema!.children(node), key); }); };
  add(roots.slice(0, limit), 'root');
  return <section className="schema-inspector" data-studio-panel="inspector" aria-label="Schema inspector" tabIndex={-1}>
    <p className="editor-text editor-text--secondary">{schema ? `Bound module ${schema.identity.module}` : 'Connect to a game with its matching build schema.'}</p>
    <label className="editor-field editor-field-label"><span>Filter globals</span><input className="editor-control editor-control--text" type="search" value={filter} onChange={event => { setFilter(event.target.value); setLimit(128); setExpanded(new Set()); }} disabled={!schema} /></label>
    <DockerList className="schema-list"><DockerListBody><div className="schema-tree" role="tree" aria-label="Game schema tree" onKeyDown={event => {
      if (!['ArrowDown', 'ArrowUp', 'Home', 'End'].includes(event.key)) return;
      const buttons = [...event.currentTarget.querySelectorAll<HTMLButtonElement>('[data-schema-node]')]; const at = buttons.indexOf(event.target as HTMLButtonElement);
      if (at < 0) return;
      const index = event.key === 'Home' ? 0 : event.key === 'End' ? buttons.length - 1 : at + (event.key === 'ArrowDown' ? 1 : -1);
      if (index >= 0 && index < buttons.length) { event.preventDefault(); buttons[index].focus(); }
    }}>{rows.map(({ node, key }) => { const children = schema!.children(node); return <DockerListRow key={key} intrinsicHeight selected={selected?.path === node.path && !node.range} columns="auto 1fr"><button className="editor-button editor-button--compact" aria-label={`${expanded.has(key) ? 'Collapse' : 'Expand'} ${node.label}`} title={`${expanded.has(key) ? 'Collapse' : 'Expand'} ${node.label}`} disabled={!children.length} aria-expanded={children.length ? expanded.has(key) : undefined} onClick={() => setExpanded(current => { const next = new Set(current); if (next.has(key)) next.delete(key); else next.add(key); return next; })}>{expanded.has(key) ? '−' : '+'}</button><button className="editor-button editor-button--compact editor-button--subtle schema-node" role="treeitem" aria-level={node.depth + 1} aria-selected={selected?.path === node.path && !node.range} data-schema-node={node.path} disabled={Boolean(state.busy)} style={{ paddingInlineStart: `${node.depth * 10}px` }} title={`${node.path} · ${schema!.describe(node)}`} onClick={() => { void choose(node).catch(report); }}><span className="editor-text">{node.label}</span><span className="editor-text editor-text--secondary">{schema!.describe(node)}</span></button></DockerListRow>; })}</div>{!rows.length && <DockerListEmpty>{schema ? 'No matching globals.' : 'No attached schema.'}</DockerListEmpty>}</DockerListBody></DockerList>
    {limit < roots.length && <button className="editor-button" onClick={() => setLimit(current => current + 128)}>Show more globals ({limit}/{roots.length})</button>}
    {dirty && <div className="editor-field-list schema-draft-notice">
      <p className="editor-text" role="status">{currentDraft ? `Unapplied draft for ${draft.path}. Write & read back to apply, or Cancel draft to discard.` : `The game timeline or connection changed. Unapplied draft for ${draft.path} is retained for reference and cannot be written. Cancel draft before refreshing or switching.`}{pendingNode && ` Waiting to select ${pendingNode.path}.`}</p>
      {!selected && <pre className="editor-text editor-text--code" aria-label="Retained scalar draft">{draft.text}</pre>}
      <div className="live-toolbar"><button className="editor-button" disabled={Boolean(state.busy)} onClick={() => { void cancelDraft().catch(report); }}>Cancel draft</button>{pendingNode && <button className="editor-button" disabled={Boolean(state.busy)} onClick={() => setPendingNode(null)}>Keep editing</button>}</div>
    </div>}
    {selected && <div className="editor-field-list"><label className="editor-field editor-field-label"><span>Selected path</span><input className="editor-control editor-control--text editor-control--code" value={selected.path} readOnly /></label><p className="editor-text">Type: {schema?.describe(selected)}</p><pre className="editor-text editor-text--code" aria-label="Inspected value">{value === undefined || value.revision !== state.revision || state.running ? selected.unsupported ?? 'Refresh to inspect the current state.' : gameJson(value.data)}</pre><button className="editor-button" disabled={Boolean(state.busy) || dirty || !!selected.unsupported || !!selected.range} onClick={() => { void read(selected).catch(report); }}>Refresh value</button><form className="live-toolbar" onSubmit={event => { event.preventDefault(); void write().catch(report); }}><label className="editor-field editor-field-label"><span>New scalar value</span><input className="editor-control editor-control--text editor-control--code" value={draft?.path === selected.path ? draft.text : ''} onChange={event => { const text = event.target.value; if (text === draft?.original) setPendingNode(null); setDraft(current => current && current.schema === schema && current.revision === state.revision ? { ...current, text } : current); }} disabled={!schema?.scalar(selected) || Boolean(state.busy) || Boolean(state.running) || !currentDraft} /></label><button className="editor-button" type="submit" disabled={!schema?.scalar(selected) || Boolean(state.busy) || Boolean(state.running) || draft?.path !== selected.path || !currentDraft}>Write & read back</button></form>{confirmed?.revision === state.revision && !state.running && <p className="editor-text" role="status">{confirmed.text}</p>}</div>}
  </section>;
}
