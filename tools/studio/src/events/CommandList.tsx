import { useEffect, useMemo, useState, type KeyboardEvent } from 'react';
import { DockerList, DockerListHeader, DockerListRow, DockerListSelect, DockerListEmpty } from '../editor/DockerList';
import { formatPc, instructionDiagnostics, instructionPhrase, rawInstructionBytes,
  type EventStructure, type FieldGraph, type NameTable, type OwnedInstruction, type Sequence } from './model';
import { useWindowedRows } from './useWindowedRows';
import { MapDiagnostics } from './MapDiagnostics';

export { MapDiagnostics } from './MapDiagnostics';

export interface CommandListProps {
  graph: FieldGraph; structure: EventStructure; names: NameTable; map: number;
  selectedPc: number | null; onSelect(pc: number): void; onFollow(pc: number): void;
}
interface Row { id: string; depth: number; ancestors: string[]; pc: number | null; label: string; fold: boolean; owner?: OwnedInstruction }

function commandRows(sequence: Sequence): Row[] {
  const rows: Row[] = [];
  const visit = (sequence: Sequence, depth: number, ancestors: string[], prefix: string) => {
    sequence.nodes.forEach((node, index) => {
      const id = `${prefix}/${index}`;
      const row = (label: string, pc: number | null, fold = false, owner?: OwnedInstruction) =>
        rows.push({ id, depth, ancestors, pc, label, fold, owner });
      switch (node.kind) {
        case 'instruction': case 'goto': row(node.kind === 'goto' ? 'Explicit flow' : '', node.instruction.pc, false, node.instruction); break;
        case 'label': row(`Label ${formatPc(node.pc)}`, null); break;
        case 'if':
          row('If / else', node.header.pc, true, node.header);
          visit(node.then_branch, depth + 1, [...ancestors, id], `${id}/then`);
          rows.push({ id: `${id}/else`, depth: depth + 1, ancestors: [...ancestors, id], pc: null, label: 'Else', fold: false });
          visit(node.else_branch, depth + 1, [...ancestors, id], `${id}/else-body`);
          break;
        case 'loop':
          // The container is a heading; the loop body owns its header PC.
          row(`Loop ${formatPc(node.header_pc)}${node.exit_pc === null ? '' : ` · exit ${formatPc(node.exit_pc)}`}`, null, true);
          visit(node.body, depth + 1, [...ancestors, id], `${id}/body`); break;
        case 'switch':
          row('Switch', node.header.pc, true, node.header);
          node.cases.forEach((item, caseIndex) => {
            rows.push({ id: `${id}/case-${caseIndex}`, depth: depth + 1, ancestors: [...ancestors, id], pc: null,
              label: `Case ${caseIndex} · slot ${formatPc(item.entry_pc)}`, fold: false });
            visit(item.body, depth + 2, [...ancestors, id], `${id}/case-${caseIndex}/body`);
          });
          break;
      }
    });
  };
  visit(sequence, 0, [], 'root');
  return rows;
}

export function CommandList({ graph, structure, names, map, selectedPc, onSelect, onFollow }: CommandListProps) {
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const [showRaw, setShowRaw] = useState(false);
  const [showPcs, setShowPcs] = useState(false);
  const allRows = useMemo(() => commandRows(structure.body), [structure]);
  const instructions = useMemo(() => new Map(graph.instructions.map(item => [item.pc, item])), [graph]);
  const diagnostics = useMemo(() => instructionDiagnostics(graph), [graph]);
  const rows = useMemo(() => allRows.filter(row => !row.ancestors.some(id => collapsed.has(id))), [allRows, collapsed]);
  const windowed = useWindowedRows(rows.length, 32);
  const selected = selectedPc === null ? undefined : instructions.get(selectedPc);
  const selectedOwner = allRows.find(row => row.pc === selectedPc)?.owner;

  useEffect(() => { setCollapsed(new Set()); setShowRaw(false); }, [graph.source_sha256, structure.actor, structure.event]);
  useEffect(() => {
    const row = allRows.find(row => row.pc === selectedPc);
    if (!row) return;
    setCollapsed(previous => {
      if (!row.ancestors.some(id => previous.has(id))) return previous;
      const next = new Set(previous); row.ancestors.forEach(id => next.delete(id)); return next;
    });
  }, [selectedPc, allRows]);
  useEffect(() => {
    const index = rows.findIndex(row => row.pc === selectedPc);
    if (index >= 0) windowed.scrollTo(index);
  }, [selectedPc, rows]);

  const toggle = (id: string) => setCollapsed(previous => {
    const next = new Set(previous); if (next.has(id)) next.delete(id); else next.add(id); return next;
  });
  const follow = () => {
    const edge = selectedOwner?.edges.find(edge => edge.status === 'followed' && edge.kind !== 'fallthrough')
      ?? selectedOwner?.edges.find(edge => edge.status === 'followed');
    if (edge) onFollow(edge.to_pc);
  };
  const keyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if ((event.target as HTMLElement).closest('input, [data-command-fold]')) return;
    const selectable = rows.filter(row => row.pc !== null);
    const selectedIndex = selectable.findIndex(row => row.pc === selectedPc);
    let next: Row | undefined;
    if (event.key === 'ArrowDown') next = selectable[Math.min(selectable.length - 1, selectedIndex + 1)];
    else if (event.key === 'ArrowUp') next = selectable[Math.max(0, selectedIndex - 1)];
    else if (event.key === 'Home') next = selectable[0];
    else if (event.key === 'End') next = selectable.at(-1);
    else if (event.key === 'Enter') { event.preventDefault(); follow(); }
    else return;
    if (next?.pc !== null && next?.pc !== undefined) {
      event.preventDefault(); onSelect(next.pc); windowed.ref.current?.focus();
    }
  };

  return <section className="event-command-view" aria-label="Command list">
    <DockerList className="event-command-list">
      <DockerListHeader>
        <span className="editor-text editor-text--caption">{allRows.filter(row => row.pc !== null).length} instructions · read only</span>
        <label className="editor-field-label event-inline-choice"><input type="checkbox" className="editor-control editor-control--choice"
          checked={showPcs} onChange={event => setShowPcs(event.target.checked)} />Show PCs</label>
        <button type="button" className="editor-button editor-button--compact editor-button--subtle"
          onClick={() => setCollapsed(new Set())}>Expand all</button>
      </DockerListHeader>
      {structure.skipped ? <DockerListEmpty>Skipped zero entry in the map-record table.</DockerListEmpty> : null}
      <div ref={windowed.ref} className="event-window event-command-window" tabIndex={0} role="list" aria-label="Event commands"
        onScroll={windowed.onScroll} onKeyDown={keyDown} onCopy={event => {
          if (window.getSelection()?.toString() || !selected) return;
          event.clipboardData.setData('text/plain', `${formatPc(selected.pc)} ${instructionPhrase(selected, names, map, structure.actor)}\n${selected.spec.mnemonic}`);
          event.preventDefault();
        }}>
        <div className="event-window-spacer" style={{ height: windowed.height }}>
          <div className="event-window-items" style={{ transform: `translateY(${windowed.offset}px)` }}>
            {rows.slice(windowed.start, windowed.end).map(row => {
              const instruction = row.pc === null ? undefined : instructions.get(row.pc);
              const issues = row.pc === null ? [] : diagnostics.get(row.pc) ?? [];
              const phrase = instruction ? instructionPhrase(instruction, names, map, structure.actor) : row.label;
              return <DockerListRow key={row.id} role="listitem" className="event-command-row" selected={row.pc !== null && row.pc === selectedPc}
                columns="26px minmax(0, 1fr) auto" style={{ paddingLeft: row.depth * 18 + 4 }} data-pc={row.pc ?? undefined}>
                {row.fold ? <button type="button" className="editor-button editor-button--icon editor-button--compact editor-button--subtle"
                  aria-label={`${collapsed.has(row.id) ? 'Expand' : 'Collapse'} ${row.label}`} title={`${collapsed.has(row.id) ? 'Expand' : 'Collapse'} ${row.label}`}
                  data-command-fold="true" aria-expanded={!collapsed.has(row.id)} onClick={() => toggle(row.id)}>{collapsed.has(row.id) ? '▸' : '▾'}</button> : <span />}
                {row.pc !== null ? <DockerListSelect selected={row.pc === selectedPc} title={`${row.label ? `${row.label}: ` : ''}${phrase} · ${formatPc(row.pc)}`}
                  aria-label={`${phrase} at ${formatPc(row.pc)}`} onClick={() => onSelect(row.pc!)} onDoubleClick={() => {
                    const edge = row.owner?.edges.find(edge => edge.status === 'followed' && edge.kind !== 'fallthrough')
                      ?? row.owner?.edges.find(edge => edge.status === 'followed');
                    if (edge) onFollow(edge.to_pc);
                  }}><span className="event-command-text">{showPcs ? <span className="editor-text editor-text--code">{formatPc(row.pc)} </span> : null}
                    {row.fold || row.label ? <span className="editor-text editor-text--secondary">{row.label} · </span> : null}{phrase}</span></DockerListSelect>
                  : <span className="editor-text editor-text--secondary event-command-text">{row.label}</span>}
                {issues.length ? <span className="editor-text editor-text--warning" title={issues.join('\n')} aria-label={issues.join('; ')}>⚠ {issues.length}</span> : null}
              </DockerListRow>;
            })}
          </div>
        </div>
      </div>
    </DockerList>
    <MapDiagnostics graph={graph} onNavigate={onFollow} />
    {selected ? <div className="event-instruction-detail editor-inset-frame" aria-label="Selected instruction">
      <div className="event-detail-heading"><span className="editor-text editor-text--code">{formatPc(selected.pc)} · {selected.spec.mnemonic}</span>
        <button type="button" className="editor-button editor-button--compact" aria-expanded={showRaw} onClick={() => setShowRaw(value => !value)}>{showRaw ? 'Hide' : 'Show'} raw bytes</button></div>
      <p className="editor-text editor-text--caption">{selected.spec.effect}</p>
      {(diagnostics.get(selected.pc) ?? []).map((text, index) => <p key={index} className="editor-text editor-text--warning">{text}</p>)}
      {showRaw ? <pre className="editor-text editor-text--code">{rawInstructionBytes(graph, selected)}{'\n'}Operands: {JSON.stringify(selected.operands)}{'\n'}Ranges: {JSON.stringify(selected.ranges)} · advance {selected.size}</pre> : null}
      <div className="event-edge-actions">{selectedOwner?.edges.map((edge, index) => edge.status === 'followed'
        ? <button key={index} type="button" className="editor-button editor-button--compact editor-button--subtle"
          onClick={() => onFollow(edge.to_pc)}>{edge.kind} → {formatPc(edge.to_pc)}</button>
        : <span key={index} className="editor-text editor-text--warning">{edge.kind} → {formatPc(edge.to_pc)} · {edge.status}</span>)}</div>
    </div> : null}
  </section>;
}
