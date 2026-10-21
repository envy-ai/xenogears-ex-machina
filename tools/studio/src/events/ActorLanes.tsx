import { useEffect, useMemo, useState } from 'react';
import { DockerList, DockerListHeader, DockerListRow, DockerListSelect, DockerListEmpty } from '../editor/DockerList';
import { actorName, eventName, eventTrigger, formatPc, selectorName, variableName,
  type AccessLocation, type EventRequest, type MapLanes, type NameTable, type Selection, type VariableDependency } from './model';
import { useWindowedRows } from './useWindowedRows';

export interface ActorLanesProps { lanes: MapLanes; names: NameTable; map: number; selection: Selection; onSelect(selection: Selection): void }
type LinkRow = { kind: 'request'; request: EventRequest } | { kind: 'variable'; group: VariableDependency };

export function ActorLanes({ lanes, names, map, selection, onSelect }: ActorLanesProps) {
  const [group, setGroup] = useState<VariableDependency | null>(null);
  const [showAccesses, setShowAccesses] = useState(false);
  const actorWindow = useWindowedRows(lanes.actors.length, 106);
  const links = useMemo(() => {
    const result: LinkRow[] = [];
    for (const request of lanes.requests) if (request.source.actor === selection.actor && request.source.event === selection.event) {
      result.push({ kind: 'request', request });
    }
    for (const group of lanes.variable_dependencies) if (group.writers.some(item => item.actor === selection.actor && item.event === selection.event)
      || group.readers.some(item => item.actor === selection.actor && item.event === selection.event)) {
      result.push({ kind: 'variable', group });
    }
    return result;
  }, [lanes, selection.actor, selection.event]);
  const linkWindow = useWindowedRows(links.length, 32);
  const references = useMemo(() => group ? [
    ...group.writers.map(location => ({ role: 'Write', location })),
    ...group.readers.map(location => ({ role: 'Read', location }))
  ] : [], [group]);
  const referenceWindow = useWindowedRows(references.length, 32);
  const event = lanes.actors.find(actor => actor.actor === selection.actor)?.events.find(event => event.event === selection.event);
  const unresolved = event?.variable_accesses.filter(access => access.resolution !== 'resolved') ?? [];
  useEffect(() => {
    const index = lanes.actors.findIndex(actor => actor.actor === selection.actor);
    if (index >= 0) actorWindow.scrollTo(index);
    setGroup(null); setShowAccesses(false);
  }, [lanes, selection.actor, selection.event]);
  const selectSource = (source: AccessLocation | EventRequest['source']) => onSelect({ actor: source.actor, event: source.event, pc: source.pc });

  return <section className="event-lanes-view" aria-label="Actor lanes">
    <div className="event-lanes-caption editor-text editor-text--caption">Event membership and potential links · columns are event slots, not elapsed time. Party bindings remain symbolic.</div>
    <div ref={actorWindow.ref} className="event-window event-lanes-grid" onScroll={actorWindow.onScroll} aria-label="Actor event spans">
      <div className="event-window-spacer event-lane-width" style={{ height: actorWindow.height }}>
        <div className="event-window-items" style={{ transform: `translateY(${actorWindow.offset}px)` }}>
          {lanes.actors.slice(actorWindow.start, actorWindow.end).map(actor => <div key={actor.actor} className="event-lane-row editor-card">
            <div className="event-lane-title"><span className="editor-text">{actorName(names, map, actor.actor)}</span><span className="editor-text editor-text--caption">Actor {actor.actor}</span></div>
            <div className="event-lane-slots">{actor.events.map(item => <button key={item.event} type="button"
              className={`editor-button editor-button--compact ${actor.actor === selection.actor && item.event === selection.event ? 'editor-button--primary' : 'editor-button--subtle'}`}
              aria-pressed={actor.actor === selection.actor && item.event === selection.event}
              aria-label={`${actorName(names, map, actor.actor)} ${eventName(names, map, actor.actor, item.event)} event ${item.event}`}
              title={`${eventName(names, map, actor.actor, item.event)} · ${item.skipped ? 'skipped zero entry' : `${item.instruction_count} instructions · ${formatPc(item.entry_pc)}`}`}
              onClick={() => onSelect({ actor: actor.actor, event: item.event, pc: item.skipped ? null : item.entry_pc })}>
              <span>{item.event}</span><span className="editor-text editor-text--caption">{item.skipped ? 'skip' : item.instruction_count}</span>
            </button>)}</div>
          </div>)}
        </div>
      </div>
    </div>
    <div className="event-lane-lower">
      <DockerList className="event-lane-links">
        <DockerListHeader><span className="editor-text">{actorName(names, map, selection.actor)} · {eventName(names, map, selection.actor, selection.event)} · links</span></DockerListHeader>
        {!links.length ? <DockerListEmpty>No requests or shared variable groups for this event.</DockerListEmpty> : null}
        <div ref={linkWindow.ref} className="event-window" onScroll={linkWindow.onScroll} role="list" aria-label="Event links">
          <div className="event-window-spacer" style={{ height: linkWindow.height }}><div className="event-window-items" style={{ transform: `translateY(${linkWindow.offset}px)` }}>
            {links.slice(linkWindow.start, linkWindow.end).map((link, index) => <DockerListRow key={linkWindow.start + index} className="event-link-row" role="listitem" columns="minmax(0, 1fr)">
              {link.kind === 'request' ? <DockerListSelect title={`${link.request.kind} · ${link.request.actor.unresolved ?? 'resolved actor'} · priority ${link.request.priority}`}
                onClick={() => selectSource(link.request.source)}><span className="event-command-text">{formatPc(link.request.source.pc)} · {link.request.kind.replaceAll('_', ' ')} → {selectorName(link.request.actor, names, map)} / {link.request.target ? eventName(names, map, link.request.target.actor, link.request.event) : eventTrigger(link.request.event)} ({link.request.event})</span></DockerListSelect>
                : <DockerListSelect selected={group === link.group} onClick={() => setGroup(link.group)}><span className="event-command-text">{variableName(names, link.group.slot)} · {link.group.writers.length} writes / {link.group.readers.length} reads · inspect references</span></DockerListSelect>}
            </DockerListRow>)}
          </div></div>
        </div>
      </DockerList>
      <DockerList className="event-lane-references">
        <DockerListHeader><span className="editor-text editor-text--caption">{group ? `${variableName(names, group.slot)} · potential sharing, no causality asserted` : 'Select a variable group to inspect its sources'}</span></DockerListHeader>
        <div ref={referenceWindow.ref} className="event-window" onScroll={referenceWindow.onScroll} role="list" aria-label="Variable source references">
          <div className="event-window-spacer" style={{ height: referenceWindow.height }}><div className="event-window-items" style={{ transform: `translateY(${referenceWindow.offset}px)` }}>
            {references.slice(referenceWindow.start, referenceWindow.end).map(({ role, location }, index) => <DockerListRow key={referenceWindow.start + index} className="event-link-row" role="listitem" columns="minmax(0, 1fr)">
              <DockerListSelect onClick={() => selectSource(location)}><span className="event-command-text">{role} · {actorName(names, map, location.actor)} / {eventName(names, map, location.actor, location.event)} · {formatPc(location.pc)} · operand {location.operand_index}</span></DockerListSelect>
            </DockerListRow>)}
          </div></div>
        </div>
      </DockerList>
    </div>
    <details className="event-lane-accesses editor-inset-frame" open={showAccesses} onToggle={event => setShowAccesses(event.currentTarget.open)}>
      <summary className={`editor-text ${unresolved.length ? 'editor-text--warning' : 'editor-text--caption'}`}>{event?.variable_accesses.length ?? 0} variable accesses · {unresolved.length} unresolved roles/references · inspect original descriptors</summary>
      {showAccesses ? <div>{event?.variable_accesses.map((access, index) => <div key={index} className="event-diagnostic-line">
        <button type="button" className="editor-button editor-button--compact editor-button--subtle"
          onClick={() => onSelect({ ...selection, pc: access.pc })}>{formatPc(access.pc)}</button>
        <span className="editor-text editor-text--caption">{access.direction} · {access.slot === null ? access.resolution : variableName(names, access.slot)}{access.bit === null ? '' : ` · bit ${access.bit}`}{access.odd_alias ? ' · odd byte alias' : ''} · {access.operand_kind}@{access.operand_offset} · {access.resolution}{access.reason ? ` · ${access.reason}` : ''}</span>
      </div>)}</div> : null}
    </details>
  </section>;
}
