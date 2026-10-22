import { useCallback, useEffect, useRef, useState } from 'react';
import { FolderOpen, RefreshCw, File, Folder, ArrowUp } from 'lucide-react';
import { DockableWorkspace, WorkspacePanel, WorkspaceCenter, useWorkspaceLayout } from './editor/DockableWorkspace';
import { ContextMenu } from './editor/ContextMenu';
import { EditorDialogShell } from './editor/EditorDialogShell';
import { NativeWindowControls, useNativeWindowControls } from './editor/NativeWindowControls';
import { DockerList, DockerListHeader, DockerListBody, DockerListRow, DockerListEmpty } from './editor/DockerList';
import { restoreWorkspacePanel } from './editor/workspaceLayout';
import { contextMenuPointForElement, type EditorCommand, type EditorContextMenuRequest } from './editor/editorCommands';
import { loadBackend, readCommandResult, type Backend } from './backend/service';
import type { FilesAdapter, FileEntry } from './platform/files';
import { selectFolder, nativeHost } from './platform/host';
import { runNativeSmoke } from './smoke';
import { LiveGameProvider, LiveControls, SchemaInspector } from './game/LiveGame';
import { EventStudioProvider, DiscBrowser, EventViews } from './events/EventStudio';

interface LogEntry { id: number; time: string; kind: string; text: string }
const fullError = (error: unknown): string => error instanceof AggregateError
  ? `${error.stack ?? error.message}\n${error.errors.map(fullError).join('\n')}`
  : error instanceof Error
  ? `${error.stack ?? `${error.name}: ${error.message}`}${error.cause ? `\nCaused by: ${fullError(error.cause)}` : ''}`
  : String(error);

export function App() {
  const [layout, setLayout, resetLayout] = useWorkspaceLayout();
  const controls = useNativeWindowControls();
  const [controlsTarget, setControlsTarget] = useState<HTMLDivElement | null>(null);
  const [backend, setBackend] = useState<Backend | null>(null);
  const [version, setVersion] = useState<{ version: string; operations: string[] } | null>(null);
  const [command, setCommand] = useState('{"op":"echo","args":{"message":"Hello, Studio"}}');
  const [response, setResponse] = useState('');
  const [errors, setErrors] = useState<string[]>([]);
  const [log, setLog] = useState<LogEntry[]>([]);
  const [folder, setFolder] = useState<FilesAdapter | null>(null);
  const [directory, setDirectory] = useState('');
  const [entries, setEntries] = useState<FileEntry[]>([]);
  const [about, setAbout] = useState(false);
  const [menu, setMenu] = useState<EditorContextMenuRequest | null>(null);
  const aboutFocus = useRef<HTMLElement | null>(null);
  const aboutSurface = useRef<HTMLElement | null>(null);
  const serial = useRef(0);
  const append = useCallback((kind: string, text: string) => {
    setLog(current => [...current, { id: serial.current++, time: new Date().toLocaleTimeString(), kind, text }]);
  }, []);
  const report = useCallback((error: unknown) => {
    console.error(error);
    const text = fullError(error);
    setErrors(current => [...current, text]);
    append('Error', text);
  }, [append]);

  useEffect(() => {
    const error = (event: ErrorEvent) => report(event.error ?? new Error(event.message));
    const rejection = (event: PromiseRejectionEvent) => { event.preventDefault(); report(event.reason); };
    window.addEventListener('error', error);
    window.addEventListener('unhandledrejection', rejection);
    let active = true;
    loadBackend().then(async service => {
      if (!active) return;
      if (await runNativeSmoke(service)) return;
      const request = '{"op":"version"}';
      append('Command', request);
      const reply = service.command(request);
      append('Response', reply);
      const result = readCommandResult<{ version: string; operations: string[] }>(reply);
      setVersion(result); setBackend(service);
    }).catch(report);
    return () => { active = false; window.removeEventListener('error', error); window.removeEventListener('unhandledrejection', rejection); };
  }, [append, report]);

  const browse = useCallback(async (adapter: FilesAdapter, path: string) => {
    const next = await adapter.list(path);
    setFolder(adapter); setDirectory(path); setEntries(next);
    append('Folder', `${adapter.kind}: ${adapter.name}/${path} (${next.length} entries)`);
  }, [append]);
  const open = useCallback(() => {
    selectFolder().then(adapter => browse(adapter, '')).catch(error => {
      // Cancellation is an acknowledged user action, not a failed import.
      if (error instanceof DOMException && error.name === 'AbortError') append('Folder', error.message);
      else report(error);
    });
  }, [browse, report, append]);
  const send = useCallback(() => {
    if (!backend) { report(new Error('The backend is not ready')); return; }
    append('Command', command);
    try {
      const reply = backend.command(command);
      setResponse(reply); append('Response', reply);
      setResponse(JSON.stringify(readCommandResult(reply)));
    } catch (error) { report(error); }
  }, [backend, command, append, report]);
  const openAbout = () => {
    aboutFocus.current = document.querySelector<HTMLElement>('[data-studio-menu="Help"]');
    setAbout(true);
  };
  const closeAbout = () => setAbout(false);
  useEffect(() => {
    if (!about) return;
    const surface = aboutSurface.current;
    const focus = () => surface?.querySelector<HTMLButtonElement>('button')?.focus();
    const frame = requestAnimationFrame(focus);
    const containFocus = (event: FocusEvent) => { if (!surface?.contains(event.target as Node)) focus(); };
    const tab = (event: KeyboardEvent) => { if (event.key === 'Tab') { event.preventDefault(); focus(); } };
    document.addEventListener('focusin', containFocus);
    document.addEventListener('keydown', tab, true);
    return () => {
      cancelAnimationFrame(frame);
      document.removeEventListener('focusin', containFocus);
      document.removeEventListener('keydown', tab, true);
      aboutFocus.current?.focus();
    };
  }, [about]);

  useEffect(() => {
    const key = (event: KeyboardEvent) => {
      if (document.querySelector('[aria-modal="true"]')) return;
      if (event.key === 'F10' && !event.shiftKey) { event.preventDefault(); document.querySelector<HTMLButtonElement>('[data-studio-menu="File"]')?.focus(); }
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'o') { event.preventDefault(); open(); }
      if ((event.ctrlKey || event.metaKey) && event.key === 'Enter') { event.preventDefault(); send(); }
      if (event.key === 'F6' && !event.shiftKey && !event.ctrlKey && !event.altKey && !event.defaultPrevented) {
        event.preventDefault();
        const panels = [...document.querySelectorAll<HTMLElement>('[data-studio-panel]')];
        const current = panels.findIndex(panel => panel.contains(document.activeElement));
        panels[(current + 1) % panels.length]?.focus();
      }
    };
    window.addEventListener('keydown', key); return () => window.removeEventListener('keydown', key);
  }, [open, send]);
  const menus: Record<string, EditorCommand[]> = {
    File: [{ id: 'open', label: 'Open folder…', execute: open }],
    View: [
      ...(['project', 'backend', 'log', 'live', 'inspector'] as const).map(id => ({ id: `show-${id}`, label: `Show ${id[0].toUpperCase() + id.slice(1)}`, execute: () => setLayout(current => restoreWorkspacePanel(current, id)) })),
      { id: 'reset', label: 'Reset workspace layout', execute: resetLayout },
    ],
    Help: [{ id: 'about', label: 'About XEM Studio', execute: openAbout }],
  };

  return <EventStudioProvider backend={backend} folder={folder} report={report} log={append}><LiveGameProvider report={report} log={append}><main className="editor-shell studio-shell" data-editor-theme="graphite-teal" data-editor-density="compact" aria-label="XEM Studio">
    <header className="studio-titlebar">
      <strong className="editor-text">XEM Studio</strong><span className="editor-text editor-text--secondary">Field event studio</span>
      <div className="studio-titlebar-actions" ref={setControlsTarget} />
      <NativeWindowControls bridge={controls} placement="editor" />
    </header>
    <nav className="studio-menubar" aria-label="Main menu">{Object.entries(menus).map(([label, commands]) => <button key={label} className="editor-button studio-menu-button" data-studio-menu={label} aria-haspopup="menu" aria-expanded={menu?.label === label} onClick={event => setMenu({ label, target: { type: 'panel', panelId: 'studio-menu' }, commands, ...contextMenuPointForElement(event.currentTarget), restoreFocus: event.currentTarget })} onKeyDown={event => {
      if (event.key === 'ArrowDown') { event.preventDefault(); event.currentTarget.click(); }
      const delta = event.key === 'ArrowRight' ? 1 : event.key === 'ArrowLeft' ? -1 : 0;
      if (delta) { event.preventDefault(); const buttons = [...event.currentTarget.parentElement!.querySelectorAll<HTMLButtonElement>('button')]; buttons[(buttons.indexOf(event.currentTarget) + delta + buttons.length) % buttons.length]?.focus(); }
    }}>{label}</button>)}</nav>
    <ContextMenu request={menu} onClose={() => setMenu(null)} />
    {errors.length > 0 && <section className="studio-errors" role="alert" aria-label="Studio errors"><strong className="editor-text editor-text--danger">Operation failed</strong>{errors.map((error, index) => <pre className="editor-text editor-text--danger" key={index}>{error}</pre>)}<button className="editor-button" onClick={() => setErrors([])}>Dismiss errors</button></section>}
    <DockableWorkspace layout={layout} setLayout={setLayout} resetLayout={resetLayout} controlsTarget={controlsTarget}>
      <WorkspacePanel id="project" title="Project">
        <section className="studio-project" data-studio-panel="project" aria-label="Project files" tabIndex={-1}>
          <div className="studio-toolbar"><button className="editor-button" onClick={open} title="Open folder (Ctrl+O)"><FolderOpen size={16} /> Open folder…</button><button className="editor-button editor-button--icon" aria-label="Refresh folder" disabled={!folder} onClick={() => folder && browse(folder, directory).catch(report)}><RefreshCw size={16} /></button></div>
          {folder && <div className="studio-toolbar"><button className="editor-button editor-button--icon" aria-label="Parent folder" disabled={!directory} onClick={() => browse(folder, directory.split('/').slice(0, -1).join('/')).catch(report)}><ArrowUp size={16} /></button><span className="editor-text">{folder.name}/{directory}</span></div>}
          <DockerList><DockerListHeader><span className="editor-text editor-text--secondary">{folder ? `${entries.length} entries · ${folder.kind}` : 'No folder open'}</span></DockerListHeader><DockerListBody>{entries.map(entry => <DockerListRow key={entry.name} columns="1fr"><button className="editor-button studio-file" disabled={entry.kind === 'file'} onClick={() => folder && browse(folder, [directory, entry.name].filter(Boolean).join('/')).catch(report)}>{entry.kind === 'directory' ? <Folder size={16} /> : <File size={16} />}{entry.name}</button></DockerListRow>)}{!entries.length && <DockerListEmpty>{folder ? 'This folder is empty.' : 'Open a folder to browse its files.'}</DockerListEmpty>}</DockerListBody></DockerList>
          <DiscBrowser />
        </section>
      </WorkspacePanel>
      <WorkspaceCenter><EventViews /></WorkspaceCenter>
      <WorkspacePanel id="live" title="Live game"><LiveControls report={report} /></WorkspacePanel>
      <WorkspacePanel id="inspector" title="Schema inspector"><SchemaInspector report={report} /></WorkspacePanel>
      <WorkspacePanel id="backend" title="Backend"><section className="studio-backend" data-studio-panel="backend" aria-label="Backend commands" tabIndex={-1}>
        <p className="editor-text"><span className="studio-ready" aria-hidden="true" />{version ? `xem-studio ${version.version} · Operations: ${version.operations.join(', ')}` : 'Loading backend…'}</p>
        <label className="editor-field-label"><span>Command (JSON)</span><textarea className="editor-control editor-control--textarea editor-control--code studio-command" spellCheck={false} value={command} onChange={event => setCommand(event.target.value)} /></label>
        <div className="studio-send"><button className="editor-button editor-button--primary" disabled={!backend} onClick={send}>Send command</button><span className="editor-text editor-text--secondary">Ctrl+Enter</span></div>
        <div className="editor-field"><span className="editor-text editor-text--caption">Response</span><pre className="editor-text editor-text--code editor-inset-frame studio-response" aria-label="Backend response" tabIndex={0}>{response || 'Send a command to see its reply.'}</pre></div>
      </section></WorkspacePanel>
      <WorkspacePanel id="log" title="Log"><section className="studio-log" data-studio-panel="log" aria-label="Command log" tabIndex={-1}><div className="studio-toolbar"><span className="editor-text editor-text--secondary">{log.length} entries</span><button className="editor-button" onClick={() => navigator.clipboard.writeText(log.map(entry => `${entry.time} ${entry.kind} ${entry.text}`).join('\n')).catch(report)}>Copy log</button></div><div className="studio-log-entries">{log.map(entry => <div className="studio-log-entry" key={entry.id}><time className="editor-text editor-text--secondary">{entry.time}</time><span className="editor-text">{entry.kind}</span><pre className="editor-text editor-text--code">{entry.text}</pre></div>)}</div></section></WorkspacePanel>
    </DockableWorkspace>
    <footer className="studio-status"><span className="editor-text">{backend ? 'Backend ready' : 'Backend loading'} · {nativeHost ? 'Desktop' : 'Browser'}</span><span className="editor-text editor-text--secondary">Drag tabs to dock · F6 panels · F10 menus</span></footer>
    {about && <EditorDialogShell backdropClassName="editor-dialog-backdrop" onClose={closeAbout}><section className="editor-small-dialog studio-about" role="dialog" aria-modal="true" aria-label="About XEM Studio" ref={aboutSurface}><h2 className="editor-heading editor-heading--page">XEM Studio</h2><p className="editor-text">Milestone 4 · Backend {version?.version ?? 'loading'}</p><p className="editor-text">A local modding workspace for Xenogears Ex Machina.</p><p className="editor-text editor-text--secondary">UI widgets adapted from Saga Foundry RPG Creator with permission under MIT. Inter and Inconsolata: OFL 1.1.</p><button className="editor-button" autoFocus onClick={closeAbout}>Close</button></section></EditorDialogShell>}
  </main></LiveGameProvider></EventStudioProvider>;
}
