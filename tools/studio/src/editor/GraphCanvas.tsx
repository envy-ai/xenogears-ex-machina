// Adapted from the generic canvas portions of rpg_creator
// src/editor/EventGraphEditor.tsx at 1f690d0263af8881a2d33ee374a2e5006355748b.
// Exact retained ranges and the author's extended MIT grant: see PROVENANCE.md.
import { useCallback, useEffect, useId, useLayoutEffect, useMemo, useRef, useState } from 'react';
import type { CSSProperties, KeyboardEvent, PointerEvent, ReactNode } from 'react';
import '../styles/graph-canvas.scss';

export interface GraphCanvasNode {
  id: string;
  x: number;
  y: number;
  width: number;
  height: number;
  label: string;
  content?: ReactNode;
}

export interface GraphCanvasEdge {
  id: string;
  from: string;
  to: string;
  kind: string;
  status: string;
}

export interface GraphCanvasProps {
  nodes: GraphCanvasNode[];
  edges: GraphCanvasEdge[];
  selectedId: string | null;
  onSelect(id: string): void;
  onActivate?(id: string): void;
  ariaLabel: string;
}

const MIN_ZOOM = 0.1;
const MAX_ZOOM = 1.8;
const WORLD_PADDING = 1_000_000;
const CANVAS_WIDTH = 4200 + WORLD_PADDING * 2;
const CANVAS_HEIGHT = 2800 + WORLD_PADDING * 2;
// Keep every supported zoom below Chromium's layout dimension ceiling, with
// headroom. Oversized layouts fail explicitly instead of losing distant nodes.
const MAX_CSS_DIMENSION = 32_000_000;
const CULL_PADDING = 180;

type Point = { x: number; y: number };
type Viewport = { left: number; top: number; width: number; height: number };
type ZoomAnchor = Point & { localX: number; localY: number; zoom: number };

function intersects(left: number, top: number, right: number, bottom: number, viewport: Viewport) {
  return right >= viewport.left - CULL_PADDING && left <= viewport.left + viewport.width + CULL_PADDING
    && bottom >= viewport.top - CULL_PADDING && top <= viewport.top + viewport.height + CULL_PADDING;
}

/** Read-only canvas. The caller owns layout, node content and selection. */
export function GraphCanvas({ nodes, edges, selectedId, onSelect, onActivate, ariaLabel }: GraphCanvasProps) {
  const canvasRef = useRef<HTMLDivElement>(null);
  const nodesRef = useRef(new Map<string, HTMLElement>());
  const zoomRef = useRef(1);
  const [zoom, setZoom] = useState(1);
  const [viewport, setViewport] = useState<Viewport>({ left: 0, top: 0, width: 900, height: 600 });
  const initialized = useRef(false);
  const previousOrigin = useRef<Point | null>(null);
  const worldViewport = useRef({ left: 0, top: 0 });
  const pendingAnchor = useRef<ZoomAnchor | null>(null);
  const pan = useRef<{ pointerId: number; startX: number; startY: number; scrollLeft: number; scrollTop: number } | null>(null);
  const [panning, setPanning] = useState(false);
  const [spaceHeld, setSpaceHeld] = useState(false);
  const markerId = useId().replaceAll(':', '_');
  const hintId = useId();
  const { byId, canvas } = useMemo(() => {
    const result = new Map<string, GraphCanvasNode>();
    let minX = 0, minY = 0, maxX = 0, maxY = 0;
    for (const node of nodes) {
      if (!node.id || result.has(node.id)) throw new Error(`Duplicate or empty graph node id: ${node.id}`);
      if (![node.x, node.y, node.width, node.height, node.x + node.width, node.y + node.height].every(Number.isFinite)
        || node.width <= 0 || node.height <= 0
        || node.x + node.width <= node.x || node.y + node.height <= node.y) {
        throw new Error(`Graph node ${node.id} has invalid or unrepresentable bounds.`);
      }
      minX = Math.min(minX, node.x); minY = Math.min(minY, node.y);
      maxX = Math.max(maxX, node.x + node.width); maxY = Math.max(maxY, node.y + node.height);
      result.set(node.id, node);
    }
    // Caller coordinates remain unchanged. A canvas origin translates negative
    // coordinates into the padded scroll extent, which grows with the layout.
    const width = Math.max(CANVAS_WIDTH, maxX - minX + WORLD_PADDING * 2);
    const height = Math.max(CANVAS_HEIGHT, maxY - minY + WORLD_PADDING * 2);
    if (![width, height].every(Number.isFinite)
      || width * MAX_ZOOM > MAX_CSS_DIMENSION || height * MAX_ZOOM > MAX_CSS_DIMENSION) {
      throw new Error(`Graph layout exceeds the supported browser canvas extent at ${MAX_ZOOM}× zoom.`);
    }
    const edgeIds = new Set<string>();
    for (const edge of edges) {
      if (!edge.id || edgeIds.has(edge.id)) throw new Error(`Duplicate or empty graph edge id: ${edge.id}`);
      if (!result.has(edge.from) || !result.has(edge.to)) throw new Error(`Graph edge ${edge.id} has an unknown endpoint.`);
      edgeIds.add(edge.id);
    }
    return { byId: result, canvas: { width, height,
      originX: WORLD_PADDING - minX, originY: WORLD_PADDING - minY } };
  }, [nodes, edges]);

  const updateViewport = useCallback((activeZoom = zoomRef.current) => {
    const element = canvasRef.current;
    if (!element) return;
    const next = { left: element.scrollLeft / activeZoom - canvas.originX,
      top: element.scrollTop / activeZoom - canvas.originY,
      width: element.clientWidth / activeZoom, height: element.clientHeight / activeZoom };
    worldViewport.current = { left: next.left, top: next.top };
    setViewport(next);
  }, [canvas.originX, canvas.originY]);

  const zoomAroundPoint = useCallback((nextZoom: number, localPoint?: Point) => {
    const element = canvasRef.current;
    if (!element) return;
    const currentZoom = zoomRef.current;
    // These are the explicit original canvas zoom limits.
    const boundedZoom = Math.max(MIN_ZOOM, Math.min(MAX_ZOOM, nextZoom));
    if (Math.abs(boundedZoom - currentZoom) < 0.0001) return;
    const localX = localPoint?.x ?? element.clientWidth / 2;
    const localY = localPoint?.y ?? element.clientHeight / 2;
    const pending = pendingAnchor.current;
    const left = pending && Math.abs(pending.zoom - currentZoom) < 0.0001
      ? pending.x * currentZoom - pending.localX : element.scrollLeft;
    const top = pending && Math.abs(pending.zoom - currentZoom) < 0.0001
      ? pending.y * currentZoom - pending.localY : element.scrollTop;
    pendingAnchor.current = { x: (left + localX) / currentZoom, y: (top + localY) / currentZoom,
      localX, localY, zoom: boundedZoom };
    zoomRef.current = boundedZoom;
    setZoom(boundedZoom);
  }, []);

  const fitAll = useCallback(() => {
    const element = canvasRef.current;
    if (!element || !nodes.length || !element.clientWidth || !element.clientHeight) return;
    let minX = Infinity, minY = Infinity, maxX = -Infinity, maxY = -Infinity;
    for (const node of nodes) {
      minX = Math.min(minX, node.x); minY = Math.min(minY, node.y);
      maxX = Math.max(maxX, node.x + node.width); maxY = Math.max(maxY, node.y + node.height);
    }
    const previousZoom = zoomRef.current;
    const nextZoom = Math.max(MIN_ZOOM, Math.min(1.25,
      element.clientWidth / (maxX - minX + 120), element.clientHeight / (maxY - minY + 120)));
    pendingAnchor.current = { x: minX + canvas.originX - 60, y: minY + canvas.originY - 60,
      localX: 0, localY: 0, zoom: nextZoom };
    zoomRef.current = nextZoom;
    setZoom(nextZoom);
    // A fit can keep the current zoom, in which case React has no zoom update.
    element.scrollLeft = pendingAnchor.current.x * nextZoom;
    element.scrollTop = pendingAnchor.current.y * nextZoom;
    if (Math.abs(previousZoom - nextZoom) < 0.0001) pendingAnchor.current = null;
    updateViewport(nextZoom);
  }, [nodes, canvas.originX, canvas.originY, updateViewport]);

  useLayoutEffect(() => {
    const element = canvasRef.current;
    if (!element) return;
    if (!initialized.current) {
      element.scrollLeft = canvas.originX * zoom;
      element.scrollTop = canvas.originY * zoom;
      initialized.current = true;
    } else if (previousOrigin.current && (previousOrigin.current.x !== canvas.originX
      || previousOrigin.current.y !== canvas.originY)) {
      const anchor = pendingAnchor.current;
      if (anchor) {
        anchor.x += canvas.originX - previousOrigin.current.x;
        anchor.y += canvas.originY - previousOrigin.current.y;
      } else {
        element.scrollLeft = (worldViewport.current.left + canvas.originX) * zoom;
        element.scrollTop = (worldViewport.current.top + canvas.originY) * zoom;
      }
    }
    previousOrigin.current = { x: canvas.originX, y: canvas.originY };
    const anchor = pendingAnchor.current;
    if (anchor && Math.abs(anchor.zoom - zoom) < 0.0001) {
      element.scrollLeft = anchor.x * zoom - anchor.localX;
      element.scrollTop = anchor.y * zoom - anchor.localY;
      pendingAnchor.current = null;
    }
    updateViewport(zoom);
  }, [zoom, canvas.width, canvas.height, canvas.originX, canvas.originY, updateViewport]);

  useLayoutEffect(() => {
    const element = canvasRef.current;
    if (!element) return;
    const observer = new ResizeObserver(() => updateViewport());
    observer.observe(element);
    return () => observer.disconnect();
  }, [updateViewport]);

  useEffect(() => {
    const element = canvasRef.current;
    if (!element) return;
    const wheel = (event: WheelEvent) => {
      event.preventDefault();
      const bounds = element.getBoundingClientRect();
      const modeScale = event.deltaMode === WheelEvent.DOM_DELTA_LINE ? 16
        : event.deltaMode === WheelEvent.DOM_DELTA_PAGE ? element.clientHeight : 1;
      const normalizedDelta = Math.max(-70, Math.min(70, event.deltaY * modeScale));
      zoomAroundPoint(zoomRef.current * Math.exp(normalizedDelta * 0.0022), {
        x: event.clientX - bounds.left, y: event.clientY - bounds.top,
      });
    };
    element.addEventListener('wheel', wheel, { passive: false });
    return () => element.removeEventListener('wheel', wheel);
  }, [zoomAroundPoint]);

  // External selection (for example from a command row) reveals the same node,
  // without taking keyboard focus away from the user's current view.
  useLayoutEffect(() => {
    let node = selectedId === null ? undefined : byId.get(selectedId);
    const element = canvasRef.current;
    if (!element) return;
    const current = { left: element.scrollLeft / zoomRef.current - canvas.originX,
      top: element.scrollTop / zoomRef.current - canvas.originY,
      width: element.clientWidth / zoomRef.current, height: element.clientHeight / zoomRef.current };
    // A newly loaded layout may have no selected node yet and occupy a wholly
    // different part of the world. Reveal its first node without selecting it.
    if (!node && selectedId === null && !nodes.some(candidate => intersects(candidate.x,
      candidate.y, candidate.x + candidate.width, candidate.y + candidate.height, current))) node = nodes[0];
    if (!node) return;
    if (node.x < current.left || node.y < current.top
      || node.x + node.width > current.left + current.width
      || node.y + node.height > current.top + current.height) {
      element.scrollLeft = (node.x + canvas.originX + node.width / 2) * zoomRef.current - element.clientWidth / 2;
      element.scrollTop = (node.y + canvas.originY + node.height / 2) * zoomRef.current - element.clientHeight / 2;
      updateViewport();
    }
  }, [byId, nodes, selectedId, canvas.originX, canvas.originY, updateViewport]);

  const visibleNodes = nodes.filter(node => node.id === selectedId
    || intersects(node.x, node.y, node.x + node.width, node.y + node.height, viewport));
  const visibleEdges = useMemo(() => edges.flatMap(edge => {
    const from = byId.get(edge.from)!;
    const to = byId.get(edge.to)!;
    const start = { x: from.x + from.width, y: from.y + from.height / 2 };
    const end = { x: to.x, y: to.y + to.height / 2 };
    const bend = Math.max(70, (end.x - start.x) * 0.5);
    // Include the Bezier's control hull: a wire may cross the viewport even
    // while both endpoint nodes are off screen.
    if (!intersects(Math.min(start.x, end.x - bend), Math.min(start.y, end.y),
      Math.max(end.x, start.x + bend), Math.max(start.y, end.y), viewport)) return [];
    const a = { x: start.x + canvas.originX, y: start.y + canvas.originY };
    const b = { x: end.x + canvas.originX, y: end.y + canvas.originY };
    return [{ edge, d: `M ${a.x} ${a.y} C ${a.x + bend} ${a.y}, ${b.x - bend} ${b.y}, ${b.x} ${b.y}` }];
  }), [edges, byId, viewport, canvas.originX, canvas.originY]);

  function beginPan(event: PointerEvent<HTMLDivElement>) {
    if (event.button !== 1 && !(event.button === 0 && (spaceHeld
      || !(event.target as Element).closest('[data-graph-node]')))) return;
    event.preventDefault();
    const element = event.currentTarget;
    element.focus({ preventScroll: true });
    element.setPointerCapture(event.pointerId);
    pan.current = { pointerId: event.pointerId, startX: event.clientX, startY: event.clientY,
      scrollLeft: element.scrollLeft, scrollTop: element.scrollTop };
    setPanning(true);
  }
  function movePan(event: PointerEvent<HTMLDivElement>) {
    const active = pan.current;
    if (!active || active.pointerId !== event.pointerId) return;
    event.currentTarget.scrollLeft = active.scrollLeft - (event.clientX - active.startX);
    event.currentTarget.scrollTop = active.scrollTop - (event.clientY - active.startY);
    updateViewport();
  }
  function endPan(event: PointerEvent<HTMLDivElement>) {
    if (pan.current?.pointerId !== event.pointerId) return;
    pan.current = null;
    setPanning(false);
    if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId);
  }
  function selectAndFocus(node: GraphCanvasNode) {
    onSelect(node.id);
    // Selection ensures the node is rendered even if it was previously culled.
    requestAnimationFrame(() => nodesRef.current.get(node.id)?.focus({ preventScroll: true }));
  }
  function keyDown(event: KeyboardEvent<HTMLDivElement>) {
    if (event.altKey || event.ctrlKey || event.metaKey) return;
    const selected = selectedId === null ? undefined : byId.get(selectedId);
    if (event.key === ' ') { event.preventDefault(); setSpaceHeld(true); }
    else if (event.key === '+' || event.key === '=') { event.preventDefault(); zoomAroundPoint(zoomRef.current * 1.2); }
    else if (event.key === '-') { event.preventDefault(); zoomAroundPoint(zoomRef.current / 1.2); }
    else if (event.key === '0' || event.key.toLowerCase() === 'f') { event.preventDefault(); fitAll(); }
    else if (event.key === 'Home' || event.key === 'End') {
      event.preventDefault();
      const node = event.key === 'Home' ? nodes[0] : nodes.at(-1);
      if (node) selectAndFocus(node);
    } else if (event.key === 'Enter' && selected) {
      event.preventDefault(); onActivate?.(selected.id);
    } else if (event.key.startsWith('Arrow')) {
      event.preventDefault();
      if (!selected) { if (nodes[0]) selectAndFocus(nodes[0]); return; }
      const dx = event.key === 'ArrowRight' ? 1 : event.key === 'ArrowLeft' ? -1 : 0;
      const dy = event.key === 'ArrowDown' ? 1 : event.key === 'ArrowUp' ? -1 : 0;
      let nearest: GraphCanvasNode | null = null;
      let score = Infinity;
      for (const node of nodes) {
        const x = node.x + node.width / 2 - selected.x - selected.width / 2;
        const y = node.y + node.height / 2 - selected.y - selected.height / 2;
        const forward = x * dx + y * dy;
        if (node.id === selected.id || forward <= 0) continue;
        const distance = forward + Math.abs(x * dy - y * dx) * 3;
        if (distance < score) { nearest = node; score = distance; }
      }
      if (nearest) selectAndFocus(nearest);
    }
  }

  return <section className="graph-canvas" aria-label={ariaLabel}>
    <div className="graph-canvas__toolbar" role="toolbar" aria-label="Graph view controls">
      <button className="editor-button editor-button--compact" type="button" onClick={fitAll} title="Fit graph (F or 0)">Fit graph</button>
      <button className="editor-button editor-button--compact" type="button" onClick={() => zoomAroundPoint(zoomRef.current / 1.2)} aria-label="Zoom out" title="Zoom out (-)">−</button>
      <span className="editor-text editor-text--code" aria-live="polite">{Math.round(zoom * 100)}%</span>
      <button className="editor-button editor-button--compact" type="button" onClick={() => zoomAroundPoint(zoomRef.current * 1.2)} aria-label="Zoom in" title="Zoom in (+)">+</button>
      <span className="editor-text editor-text--caption" id={hintId}>Drag background to pan · Wheel to zoom · Arrows select · Enter follows</span>
    </div>
    <div ref={canvasRef} className="graph-canvas__viewport" tabIndex={0} role="group"
      aria-label={`${ariaLabel} canvas`} aria-describedby={hintId}
      data-panning={panning} data-pan-mode={spaceHeld} onKeyDown={keyDown}
      onKeyUp={event => { if (event.key === ' ') setSpaceHeld(false); }}
      onBlur={event => { if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setSpaceHeld(false); }}
      onPointerDown={beginPan} onPointerMove={movePan} onPointerUp={endPan}
      onPointerCancel={endPan} onLostPointerCapture={endPan}
      onAuxClick={event => { if (event.button === 1) event.preventDefault(); }} onScroll={() => updateViewport()}>
      <div className="graph-canvas__extent" style={{ width: canvas.width * zoom, height: canvas.height * zoom }}>
        <div className="graph-canvas__world" style={{ width: canvas.width, height: canvas.height,
          transform: `scale(${zoom})` } as CSSProperties}>
          <svg className="graph-canvas__wires" width={canvas.width} height={canvas.height}
            aria-label="Flow connections">
            <defs><marker id={markerId} markerWidth="5" markerHeight="5" refX="4.5" refY="2.5" orient="auto">
              <path d="M0,0 L5,2.5 L0,5 z" />
            </marker></defs>
            {visibleEdges.map(({ edge, d }) => <path key={edge.id} className="graph-canvas__wire"
              d={d} data-kind={edge.kind} data-status={edge.status}
              data-selected={edge.from === selectedId || edge.to === selectedId}
              markerEnd={`url(#${markerId})`}><title>{edge.kind} · {edge.status}</title></path>)}
          </svg>
          {visibleNodes.map(node => <article key={node.id} data-graph-node={node.id}
            className="graph-canvas__node" role="button" aria-label={node.label}
            aria-pressed={node.id === selectedId} tabIndex={node.id === selectedId || (!selectedId && node === nodes[0]) ? 0 : -1}
            data-selected={node.id === selectedId}
            ref={element => { if (element) nodesRef.current.set(node.id, element); else nodesRef.current.delete(node.id); }}
            style={{ left: node.x + canvas.originX, top: node.y + canvas.originY, width: node.width, height: node.height }}
            onPointerDown={event => {
              if (event.button === 0 && !spaceHeld) event.currentTarget.focus({ preventScroll: true });
            }}
            onClick={() => { if (!spaceHeld) onSelect(node.id); }}
            onFocus={() => { if (!spaceHeld && node.id !== selectedId) onSelect(node.id); }}
            onDoubleClick={() => onActivate?.(node.id)}>
            <span className="graph-canvas__port graph-canvas__port--input" aria-hidden="true" />
            <strong className="graph-canvas__label">{node.label}</strong>
            {node.content === undefined ? null : <div className="graph-canvas__content">{node.content}</div>}
            <span className="graph-canvas__port graph-canvas__port--output" aria-hidden="true" />
          </article>)}
        </div>
      </div>
    </div>
  </section>;
}
