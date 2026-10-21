import { useEffect, useMemo, useState, type KeyboardEvent } from 'react';
import { DockerList, DockerListHeader, DockerListRow, DockerListSelect, DockerListEmpty } from '../editor/DockerList';
import { actorName, eventName, eventTrigger, formatPc, type FieldGraph, type NameTable, type Selection } from './model';
import { useWindowedRows } from './useWindowedRows';

export interface ActorTreeProps {
  graph: FieldGraph; names: NameTable; map: number; selection: Selection;
  onSelect(selection: Selection): void;
}
interface TreeRow { key: string; actor: number; event: number | null; pc: number | null; skipped: boolean }

export function ActorTree({ graph, names, map, selection, onSelect }: ActorTreeProps) {
  const [expanded, setExpanded] = useState<Set<number>>(() => new Set([selection.actor]));
  const [focusKey, setFocusKey] = useState(`${selection.actor}:${selection.event}`);
  const active = useMemo(() => new Set(graph.entries.map(entry => `${entry.actor}:${entry.event}`)), [graph]);
  const rows = useMemo(() => graph.actors.flatMap((events, actor) => {
    const result: TreeRow[] = [{ key: String(actor), actor, event: null, pc: null, skipped: false }];
    if (expanded.has(actor)) events.forEach((pc, event) => result.push({ key: `${actor}:${event}`, actor, event,
      pc: active.has(`${actor}:${event}`) ? pc : null, skipped: !active.has(`${actor}:${event}`) }));
    return result;
  }), [graph, expanded, active]);
  const windowed = useWindowedRows(rows.length, 28);
  useEffect(() => { setExpanded(new Set([selection.actor])); }, [graph.source_sha256]);
  useEffect(() => {
    setExpanded(previous => previous.has(selection.actor) ? previous : new Set([...previous, selection.actor]));
    setFocusKey(`${selection.actor}:${selection.event}`);
  }, [selection.actor, selection.event]);
  useEffect(() => {
    const index = rows.findIndex(row => row.key === focusKey);
    if (index >= 0) windowed.scrollTo(index);
  }, [focusKey, rows]);
  const toggle = (actor: number, open?: boolean) => setExpanded(previous => {
    const next = new Set(previous);
    if (open ?? !next.has(actor)) next.add(actor); else next.delete(actor);
    return next;
  });
  const focus = (row: TreeRow | undefined) => {
    if (!row) return;
    setFocusKey(row.key);
    windowed.scrollTo(rows.indexOf(row));
    requestAnimationFrame(() => windowed.ref.current?.querySelector<HTMLButtonElement>(`[data-tree-key="${row.key}"]`)?.focus());
  };
  const keyboard = (event: KeyboardEvent<HTMLDivElement>) => {
    const index = rows.findIndex(row => row.key === (event.target as HTMLElement).closest('[data-tree-key]')?.getAttribute('data-tree-key'));
    const row = rows[index];
    if (!row) return;
    if (event.key === 'ArrowDown') focus(rows[index + 1]);
    else if (event.key === 'ArrowUp') focus(rows[index - 1]);
    else if (event.key === 'Home') focus(rows[0]);
    else if (event.key === 'End') focus(rows.at(-1));
    else if (event.key === 'ArrowRight' && row.event === null) {
      if (expanded.has(row.actor)) focus(rows[index + 1]); else toggle(row.actor, true);
    } else if (event.key === 'ArrowLeft') {
      if (row.event === null) toggle(row.actor, false);
      else focus(rows.find(item => item.actor === row.actor && item.event === null));
    } else return;
    event.preventDefault();
  };

  return <DockerList className="event-actor-tree">
    <DockerListHeader><span className="editor-text editor-text--caption">Actors &amp; events · {graph.actors.length} actors</span></DockerListHeader>
    {graph.actors.length === 0 ? <DockerListEmpty>This map has no actors.</DockerListEmpty> : null}
    <div ref={windowed.ref} className="event-window" role="tree" aria-label="Actors and event entries" onKeyDown={keyboard} onScroll={windowed.onScroll}>
      <div className="event-window-spacer" style={{ height: windowed.height }}>
        <div className="event-window-items" style={{ transform: `translateY(${windowed.offset}px)` }}>
          {rows.slice(windowed.start, windowed.end).map(row => {
            const selected = row.event !== null && row.actor === selection.actor && row.event === selection.event;
            const name = row.event === null ? actorName(names, map, row.actor) : eventName(names, map, row.actor, row.event);
            const caption = row.event === null ? `${expanded.has(row.actor) ? '▾' : '▸'} ${name}` : `${row.event.toString().padStart(2, '0')} · ${name}`;
            return <DockerListRow key={row.key} className="event-tree-row" selected={selected} columns="minmax(0, 1fr)" style={{ paddingLeft: row.event === null ? 4 : 20 }}>
              <DockerListSelect role="treeitem" aria-level={row.event === null ? 1 : 2} aria-selected={selected}
                aria-expanded={row.event === null ? expanded.has(row.actor) : undefined} selected={selected}
                tabIndex={focusKey === row.key ? 0 : -1} data-tree-key={row.key}
                title={row.event === null ? `Actor ${row.actor} · 32 entry slots` : `${eventTrigger(row.event)} · ${row.skipped ? 'skipped zero entry' : formatPc(row.pc!)}`}
                onFocus={() => setFocusKey(row.key)} onClick={() => {
                  if (row.event === null) toggle(row.actor);
                  else onSelect({ actor: row.actor, event: row.event, pc: row.pc });
                }}><span className="event-command-text">{caption}{row.skipped ? <span className="editor-text editor-text--secondary"> · skipped</span> : null}</span></DockerListSelect>
            </DockerListRow>;
          })}
        </div>
      </div>
    </div>
  </DockerList>;
}
