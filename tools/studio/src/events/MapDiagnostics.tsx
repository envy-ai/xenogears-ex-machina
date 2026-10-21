import { useMemo, useState } from 'react';
import { formatPc, instructionDiagnostics, type FieldGraph } from './model';

export interface MapDiagnosticsProps { graph: FieldGraph; onNavigate(pc: number): void }

/** The same original diagnostic references remain available in every event view. */
export function MapDiagnostics({ graph, onNavigate }: MapDiagnosticsProps) {
  const [expanded, setExpanded] = useState(false);
  const diagnostics = useMemo(() => instructionDiagnostics(graph), [graph]);
  const pinned = useMemo(() => graph.blocks.filter(block => block.kind === 'pinned'), [graph]);
  const count = graph.diagnostics.overlaps.length + graph.diagnostics.unpaired.length + graph.diagnostics.inner.length
    + graph.diagnostics.tables.length + graph.diagnostics.into.length + graph.diagnostics.outside.length
    + graph.diagnostics.beyond_declared.length + pinned.length;

  return <details className="event-diagnostics editor-card" open={expanded} onToggle={event => setExpanded(event.currentTarget.open)}>
    <summary className="editor-text editor-text--warning">Map diagnostics · {count} references · {pinned.length} pinned storage blocks · {graph.diagnostics.followed_skips} followed skips</summary>
    {expanded ? <div className="event-diagnostic-content">
      {[...diagnostics].map(([pc, issues]) => <div key={pc} className="event-diagnostic-line">
        <button type="button" className="editor-button editor-button--compact editor-button--subtle" onClick={() => onNavigate(pc)}>{formatPc(pc)}</button>
        <span className="editor-text editor-text--caption">{issues.join('; ')}</span>
      </div>)}
      {pinned.filter(block => block.instruction_pcs.length === 0).map(block => <p key={block.start} className="editor-text editor-text--caption">
        Pinned storage {formatPc(block.start)}–{formatPc(block.end)} · {block.reason}</p>)}
    </div> : null}
  </details>;
}
