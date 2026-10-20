// Copied from rpg_creator src/editor/dockLayout.ts at 1f690d0263af8881a2d33ee374a2e5006355748b; see PROVENANCE.md.
import { uniqueSuffixedId } from '../domain/ids';
import { readStoredJson } from './storedJson';

/**
 * Generic, panel-id-agnostic dock-stack layout core.
 *
 * Zones (left/right/bottom) hold stacks of panels with weights; panels can
 * also float or be hidden, and `lastDock` remembers where a hidden/floating
 * panel was last docked. Every operation is pure: it takes a layout and
 * returns a new one. Consumers specialize this module with their own panel-id
 * union, storage key, version, and default layout (see workspaceLayout.ts).
 */

export const DOCK_EDGES = ['left', 'right', 'bottom'] as const;

export type DockEdge = (typeof DOCK_EDGES)[number];
export type ToolStripEdge = 'top' | 'right' | 'bottom' | 'left';

export interface DockStack<P extends string> {
  id: string;
  panels: P[];
  activePanel: P;
  weight: number;
}

export interface DockZone<P extends string> {
  size: number;
  stacks: DockStack<P>[];
}

export interface DockFloat<P extends string> {
  panelId: P;
  x: number;
  y: number;
  width: number;
  height: number;
  z: number;
}

export interface DockLayout<P extends string, V extends number = number> {
  version: V;
  zones: Record<DockEdge, DockZone<P>>;
  floating: DockFloat<P>[];
  hidden: P[];
  lastDock: Record<P, DockEdge>;
  toolStripEdge: ToolStripEdge;
}

export interface DockViewport {
  width: number;
  height: number;
}

export function isDockEdge(value: unknown): value is DockEdge {
  return value === 'left' || value === 'right' || value === 'bottom';
}

export function isToolStripEdge(value: unknown): value is ToolStripEdge {
  return value === 'top' || value === 'right' || value === 'bottom' || value === 'left';
}

function numberIn(value: unknown, fallback: number, min: number, max: number) {
  return typeof value === 'number' && Number.isFinite(value)
    ? Math.min(max, Math.max(min, value))
    : fallback;
}

function normalizeWeights<P extends string>(stacks: DockStack<P>[]): DockStack<P>[] {
  return stacks.map((stack) => ({
    ...stack,
    weight: numberIn(stack.weight, 1, 0.25, 8)
  }));
}

export function clampDockLayout<P extends string, V extends number>(
  layout: DockLayout<P, V>,
  viewport: DockViewport
): DockLayout<P, V> {
  const width = Math.max(1180, viewport.width);
  const height = Math.max(720, viewport.height);
  const rightReserve = numberIn(layout.zones.right.size, 326, 240, width);
  const leftMax = Math.max(180, width - rightReserve - 480 - 24);
  const leftSize = numberIn(layout.zones.left.size, 236, 180, leftMax);
  const rightMax = Math.max(240, width - leftSize - 480 - 24);
  const rightSize = numberIn(layout.zones.right.size, 326, 240, rightMax);
  const bottomMax = Math.max(180, height - 320 - 110);
  const bottomSize = numberIn(layout.zones.bottom.size, 260, 180, bottomMax);
  return {
    ...layout,
    zones: {
      left: {
        size: leftSize,
        stacks: normalizeWeights(layout.zones.left.stacks)
      },
      right: {
        size: rightSize,
        stacks: normalizeWeights(layout.zones.right.stacks)
      },
      bottom: {
        size: bottomSize,
        stacks: normalizeWeights(layout.zones.bottom.stacks)
      }
    },
    floating: layout.floating.map((entry) => {
      const maxWidth = Math.max(280, viewport.width - 16);
      const maxHeight = Math.max(220, viewport.height - 96);
      const widthValue = numberIn(entry.width, 360, 280, maxWidth);
      const heightValue = numberIn(entry.height, 440, 220, maxHeight);
      return {
        ...entry,
        width: widthValue,
        height: heightValue,
        x: numberIn(entry.x, 24, 8, Math.max(8, viewport.width - widthValue - 8)),
        y: numberIn(entry.y, 80, 8, Math.max(8, viewport.height - heightValue - 32)),
        z: numberIn(entry.z, 1, 1, 999)
      };
    })
  };
}

/**
 * Consumer-injected persistence contract: the layout version, the closed set
 * of valid panel ids (used both for validation and for the completeness
 * check), and a factory producing a fresh default layout.
 */
export interface DockLayoutPersistenceOptions<P extends string, V extends number> {
  version: V;
  panelIds: readonly P[];
  isPanelId: (value: unknown) => value is P;
  makeDefault: () => DockLayout<P, V>;
}

export function parseDockLayout<P extends string, V extends number>(
  value: unknown,
  viewport: DockViewport,
  options: Pick<DockLayoutPersistenceOptions<P, V>, 'version' | 'panelIds' | 'isPanelId'>
): DockLayout<P, V> {
  const invalid = (message: string): never => {
    throw new TypeError(`Dock layout is invalid: ${message}.`);
  };
  if (!value || typeof value !== 'object') return invalid('expected an object');
  const candidate = value as Partial<DockLayout<P, V>>;
  if (
    candidate.version !== options.version ||
    !candidate.zones ||
    !candidate.lastDock ||
    !Array.isArray(candidate.floating) ||
    !Array.isArray(candidate.hidden) ||
    !isToolStripEdge(candidate.toolStripEdge)
  ) {
    return invalid('version, zones, floating, hidden, lastDock, or tool-strip edge');
  }

  const seen = new Set<P>();
  const zones = {} as Record<DockEdge, DockZone<P>>;
  for (const edge of DOCK_EDGES) {
    const zone = candidate.zones[edge];
    if (
      !zone ||
      !Number.isFinite(zone.size) ||
      !Array.isArray(zone.stacks)
    ) return invalid(`${edge} zone`);
    const stacks: DockStack<P>[] = [];
    for (const stackValue of zone.stacks) {
      if (!stackValue || typeof stackValue !== 'object') return invalid(`${edge} stack`);
      const stack = stackValue as DockStack<P>;
      if (
        typeof stack.id !== 'string' ||
        !Array.isArray(stack.panels) ||
        stack.panels.length === 0 ||
        !stack.panels.every(options.isPanelId) ||
        !options.isPanelId(stack.activePanel) ||
        !stack.panels.includes(stack.activePanel) ||
        !Number.isFinite(stack.weight)
      ) {
        return invalid(`${edge} stack contents`);
      }
      for (const panelId of stack.panels) {
        if (seen.has(panelId)) return invalid(`duplicate panel ${panelId}`);
        seen.add(panelId);
      }
      stacks.push({ ...stack, panels: [...stack.panels] });
    }
    zones[edge] = { size: zone.size, stacks };
  }

  const floating: DockFloat<P>[] = [];
  for (const valueEntry of candidate.floating) {
    if (!valueEntry || typeof valueEntry !== 'object') return invalid('floating panel');
    const entry = valueEntry as DockFloat<P>;
    if (
      !options.isPanelId(entry.panelId) ||
      seen.has(entry.panelId) ||
      !Number.isFinite(entry.x) ||
      !Number.isFinite(entry.y) ||
      !Number.isFinite(entry.width) ||
      !Number.isFinite(entry.height) ||
      !Number.isFinite(entry.z)
    ) {
      return invalid('floating panel ID');
    }
    seen.add(entry.panelId);
    floating.push({ ...entry });
  }
  if (!candidate.hidden.every(options.isPanelId)) return invalid('hidden panel ID');
  for (const panelId of candidate.hidden) {
    if (seen.has(panelId)) return invalid(`duplicate hidden panel ${panelId}`);
    seen.add(panelId);
  }
  if (seen.size !== options.panelIds.length) return invalid('panel set is incomplete');
  const lastDock = {} as Record<P, DockEdge>;
  for (const panelId of options.panelIds) {
    const edge = candidate.lastDock[panelId];
    if (!isDockEdge(edge)) return invalid(`last dock for ${panelId}`);
    lastDock[panelId] = edge;
  }
  return clampDockLayout(
    {
      version: options.version,
      zones,
      floating,
      hidden: [...candidate.hidden],
      lastDock,
      toolStripEdge: candidate.toolStripEdge
    },
    viewport
  );
}

export function readDockLayout<P extends string, V extends number>(
  storage: Pick<Storage, 'getItem'>,
  viewport: DockViewport,
  options: DockLayoutPersistenceOptions<P, V> & { storageKey: string }
): DockLayout<P, V> {
  return readStoredJson(
    options.storageKey,
    (raw) => parseDockLayout(raw, viewport, options),
    options.makeDefault(),
    storage
  );
}

/**
 * Removes a panel from wherever it currently lives (zone stack, floating, or
 * hidden), dropping stacks that become empty and repairing their active
 * panel. Shared removal step for every relocation operation.
 */
export function removePanel<P extends string, V extends number>(
  layout: DockLayout<P, V>,
  panelId: P
): DockLayout<P, V> {
  return {
    ...layout,
    zones: Object.fromEntries(
      DOCK_EDGES.map((edge) => [
        edge,
        {
          ...layout.zones[edge],
          stacks: layout.zones[edge].stacks
            .map((stack) => {
              const panels = stack.panels.filter((id) => id !== panelId);
              return {
                ...stack,
                panels,
                activePanel:
                  stack.activePanel === panelId
                    ? panels[0] ?? stack.activePanel
                    : stack.activePanel
              };
            })
            .filter((stack) => stack.panels.length > 0)
        }
      ])
    ) as Record<DockEdge, DockZone<P>>,
    floating: layout.floating.filter((entry) => entry.panelId !== panelId),
    hidden: layout.hidden.filter((id) => id !== panelId)
  };
}

function uniqueStackId<P extends string>(stacks: DockStack<P>[], preferredId: string) {
  return uniqueSuffixedId(stacks.map((stack) => stack.id), preferredId);
}

export function dockPanel<P extends string, V extends number>(
  layout: DockLayout<P, V>,
  panelId: P,
  edge: DockEdge
): DockLayout<P, V> {
  const next = removePanel(layout, panelId);
  const stackId = uniqueStackId(
    next.zones[edge].stacks,
    `${edge}-${panelId}`
  );
  return {
    ...next,
    zones: {
      ...next.zones,
      [edge]: {
        ...next.zones[edge],
        stacks: [
          ...next.zones[edge].stacks,
          { id: stackId, panels: [panelId], activePanel: panelId, weight: 1 }
        ]
      }
    },
    lastDock: { ...next.lastDock, [panelId]: edge }
  };
}

export function insertPanelBeforeStack<P extends string, V extends number>(
  layout: DockLayout<P, V>,
  panelId: P,
  edge: DockEdge,
  beforeStackId: string
): DockLayout<P, V> {
  const sourceStack = Object.values(layout.zones)
    .flatMap((zone) => zone.stacks)
    .find((stack) => stack.panels.includes(panelId));
  const next = removePanel(layout, panelId);
  const beforeIndex = next.zones[edge].stacks.findIndex(
    (stack) => stack.id === beforeStackId
  );
  // Dropping a one-panel stack on the border immediately before itself does
  // not change its position, and removing it also removes the target ID.
  if (beforeIndex < 0) return layout;

  const previousStack = next.zones[edge].stacks[beforeIndex - 1];
  const beforeStack = next.zones[edge].stacks[beforeIndex]!;
  const neighboringWeight =
    previousStack === undefined
      ? beforeStack.weight
      : (previousStack.weight + beforeStack.weight) / 2;
  const weight =
    sourceStack?.panels.length === 1
      ? sourceStack.weight
      : neighboringWeight;
  const stack: DockStack<P> = {
    id: uniqueStackId(next.zones[edge].stacks, `${edge}-${panelId}`),
    panels: [panelId],
    activePanel: panelId,
    weight
  };

  return {
    ...next,
    zones: {
      ...next.zones,
      [edge]: {
        ...next.zones[edge],
        stacks: [
          ...next.zones[edge].stacks.slice(0, beforeIndex),
          stack,
          ...next.zones[edge].stacks.slice(beforeIndex)
        ]
      }
    },
    lastDock: { ...next.lastDock, [panelId]: edge }
  };
}

export function insertPanelAtEnd<P extends string, V extends number>(
  layout: DockLayout<P, V>,
  panelId: P,
  edge: DockEdge
): DockLayout<P, V> {
  const sourceStack = Object.values(layout.zones)
    .flatMap((zone) => zone.stacks)
    .find((stack) => stack.panels.includes(panelId));
  const next = removePanel(layout, panelId);
  const lastStack = next.zones[edge].stacks.at(-1);
  const stack: DockStack<P> = {
    id: uniqueStackId(next.zones[edge].stacks, `${edge}-${panelId}`),
    panels: [panelId],
    activePanel: panelId,
    weight:
      sourceStack?.panels.length === 1
        ? sourceStack.weight
        : lastStack?.weight ?? 1
  };
  return {
    ...next,
    zones: {
      ...next.zones,
      [edge]: {
        ...next.zones[edge],
        stacks: [...next.zones[edge].stacks, stack]
      }
    },
    lastDock: { ...next.lastDock, [panelId]: edge }
  };
}

export function joinStack<P extends string, V extends number>(
  layout: DockLayout<P, V>,
  panelId: P,
  edge: DockEdge,
  stackId: string
): DockLayout<P, V> {
  return insertPanelIntoStack(layout, panelId, edge, stackId);
}

/**
 * Moves a panel into an existing stack at an explicit tab index (clamped to
 * the stack bounds; defaults to the end) and activates it. Returns the
 * original layout when the target stack does not exist.
 */
export function insertPanelIntoStack<P extends string, V extends number>(
  layout: DockLayout<P, V>,
  panelId: P,
  edge: DockEdge,
  stackId: string,
  index?: number
): DockLayout<P, V> {
  const next = removePanel(layout, panelId);
  if (!next.zones[edge].stacks.some((stack) => stack.id === stackId)) {
    return layout;
  }
  return {
    ...next,
    zones: {
      ...next.zones,
      [edge]: {
        ...next.zones[edge],
        stacks: next.zones[edge].stacks.map((stack) => {
          if (stack.id !== stackId) return stack;
          const insertAt =
            index === undefined
              ? stack.panels.length
              : Math.max(0, Math.min(index, stack.panels.length));
          const panels = [...stack.panels];
          panels.splice(insertAt, 0, panelId);
          return { ...stack, panels, activePanel: panelId };
        })
      }
    },
    lastDock: { ...next.lastDock, [panelId]: edge }
  };
}

export interface InsertPanelStackOptions {
  /** Explicit stack id; defaults to a unique `${edge}-${panelId}` id. */
  stackId?: string;
  /** Stack weight; defaults to 1. */
  weight?: number;
}

/**
 * Moves a panel into a brand-new single-panel stack inserted at an explicit
 * index within a zone (clamped to the zone bounds). Unlike
 * `insertPanelBeforeStack`, the caller controls the stack id and weight.
 */
export function insertPanelStackAt<P extends string, V extends number>(
  layout: DockLayout<P, V>,
  panelId: P,
  edge: DockEdge,
  index: number,
  options?: InsertPanelStackOptions
): DockLayout<P, V> {
  const next = removePanel(layout, panelId);
  const stacks = next.zones[edge].stacks;
  const insertAt = Math.max(0, Math.min(index, stacks.length));
  const stack: DockStack<P> = {
    id: options?.stackId ?? uniqueStackId(stacks, `${edge}-${panelId}`),
    panels: [panelId],
    activePanel: panelId,
    weight: options?.weight ?? 1
  };
  return {
    ...next,
    zones: {
      ...next.zones,
      [edge]: {
        ...next.zones[edge],
        stacks: [
          ...stacks.slice(0, insertAt),
          stack,
          ...stacks.slice(insertAt)
        ]
      }
    },
    lastDock: { ...next.lastDock, [panelId]: edge }
  };
}

export function activatePanel<P extends string, V extends number>(
  layout: DockLayout<P, V>,
  edge: DockEdge,
  stackId: string,
  panelId: P
): DockLayout<P, V> {
  return {
    ...layout,
    zones: {
      ...layout.zones,
      [edge]: {
        ...layout.zones[edge],
        stacks: layout.zones[edge].stacks.map((stack) =>
          stack.id === stackId && stack.panels.includes(panelId)
            ? { ...stack, activePanel: panelId }
            : stack
        )
      }
    }
  };
}

export function floatPanel<P extends string, V extends number>(
  layout: DockLayout<P, V>,
  panelId: P,
  rect?: Partial<Omit<DockFloat<P>, 'panelId' | 'z'>>,
  defaultWidth: (panelId: P) => number = () => 340
): DockLayout<P, V> {
  const next = removePanel(layout, panelId);
  const topZ = Math.max(0, ...layout.floating.map((entry) => entry.z)) + 1;
  return {
    ...next,
    floating: [
      ...next.floating,
      {
        panelId,
        x: rect?.x ?? 48,
        y: rect?.y ?? 96,
        width: rect?.width ?? defaultWidth(panelId),
        height: rect?.height ?? 460,
        z: topZ
      }
    ]
  };
}

export function hidePanel<P extends string, V extends number>(
  layout: DockLayout<P, V>,
  panelId: P
): DockLayout<P, V> {
  const next = removePanel(layout, panelId);
  return { ...next, hidden: [...next.hidden, panelId] };
}

export function restorePanel<P extends string, V extends number>(
  layout: DockLayout<P, V>,
  panelId: P
): DockLayout<P, V> {
  return dockPanel(layout, panelId, layout.lastDock[panelId]);
}

export function restoreDefaultPanelPlacement<P extends string, V extends number>(
  layout: DockLayout<P, V>,
  panelId: P,
  defaultLayout: DockLayout<P, V>
): DockLayout<P, V> {
  const defaultEdge = defaultLayout.lastDock[panelId];
  const defaultStack = defaultLayout.zones[defaultEdge].stacks.find(
    (stack) => stack.panels.includes(panelId)
  );
  if (!defaultStack) return layout;
  const next = removePanel(layout, panelId);
  const existingStack = next.zones[defaultEdge].stacks.find(
    (stack) => stack.id === defaultStack.id
  );
  if (existingStack) {
    return joinStack(next, panelId, defaultEdge, existingStack.id);
  }
  return {
    ...next,
    zones: {
      ...next.zones,
      [defaultEdge]: {
        ...next.zones[defaultEdge],
        stacks: [
          ...next.zones[defaultEdge].stacks,
          {
            ...structuredClone(defaultStack),
            panels: [panelId],
            activePanel: panelId
          }
        ]
      }
    },
    lastDock: { ...next.lastDock, [panelId]: defaultEdge }
  };
}

export function setToolStripEdge<P extends string, V extends number>(
  layout: DockLayout<P, V>,
  edge: ToolStripEdge
): DockLayout<P, V> {
  return { ...layout, toolStripEdge: edge };
}
