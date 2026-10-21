// Copied from rpg_creator src/editor/workspaceLayout.ts at 1f690d0263af8881a2d33ee374a2e5006355748b; see PROVENANCE.md.
import {
  activatePanel,
  clampDockLayout,
  dockPanel,
  floatPanel,
  hidePanel,
  insertPanelAtEnd,
  insertPanelBeforeStack,
  joinStack,
  parseDockLayout,
  readDockLayout,
  restoreDefaultPanelPlacement as restoreDefaultDockPanelPlacement,
  restorePanel,
  setToolStripEdge as setDockToolStripEdge,
  type DockEdge,
  type DockFloat,
  type DockLayout,
  type DockLayoutPersistenceOptions,
  type DockStack,
  type DockViewport,
  type DockZone,
  type ToolStripEdge
} from './dockLayout';

export type { ToolStripEdge } from './dockLayout';

export const WORKSPACE_LAYOUT_STORAGE_KEY = 'xem-studio.workspace-layout.v1';

export const WORKSPACE_PANEL_IDS = ['project', 'backend', 'log'] as const;

export type WorkspacePanelId = (typeof WORKSPACE_PANEL_IDS)[number];
export type WorkspaceDockEdge = DockEdge;

export type WorkspaceStack = DockStack<WorkspacePanelId>;
export type WorkspaceDockZone = DockZone<WorkspacePanelId>;
export type WorkspaceFloat = DockFloat<WorkspacePanelId>;
export type WorkspaceLayoutV1 = DockLayout<WorkspacePanelId, 1>;
export type WorkspaceViewport = DockViewport;

export const DEFAULT_WORKSPACE_LAYOUT: WorkspaceLayoutV1 = {
  version: 1,
  zones: {
    left: { size: 300, stacks: [
      { id: 'project', panels: ['project'], activePanel: 'project', weight: 1 }
    ] },
    right: { size: 326, stacks: [] },
    bottom: { size: 220, stacks: [
      { id: 'backend', panels: ['backend'], activePanel: 'backend', weight: 2 },
      { id: 'log', panels: ['log'], activePanel: 'log', weight: 1 }
    ] }
  },
  floating: [],
  hidden: [],
  lastDock: { project: 'left', backend: 'bottom', log: 'bottom' },
  toolStripEdge: 'top'
};

function cloneDefault(): WorkspaceLayoutV1 {
  return structuredClone(DEFAULT_WORKSPACE_LAYOUT);
}

function isPanelId(value: unknown): value is WorkspacePanelId {
  return typeof value === 'string' &&
    WORKSPACE_PANEL_IDS.includes(value as WorkspacePanelId);
}

const WORKSPACE_LAYOUT_PERSISTENCE: DockLayoutPersistenceOptions<WorkspacePanelId, 1> = {
  version: 1,
  panelIds: WORKSPACE_PANEL_IDS,
  isPanelId,
  makeDefault: cloneDefault
};

export function clampWorkspaceLayout(
  layout: WorkspaceLayoutV1,
  viewport: WorkspaceViewport
): WorkspaceLayoutV1 {
  return clampDockLayout(layout, viewport);
}

export function parseWorkspaceLayout(
  value: unknown,
  viewport: WorkspaceViewport
): WorkspaceLayoutV1 {
  return parseDockLayout(value, viewport, WORKSPACE_LAYOUT_PERSISTENCE);
}

export function readWorkspaceLayout(
  storage: Pick<Storage, 'getItem'>,
  viewport: WorkspaceViewport
): WorkspaceLayoutV1 {
  return readDockLayout(storage, viewport, {
    ...WORKSPACE_LAYOUT_PERSISTENCE,
    storageKey: WORKSPACE_LAYOUT_STORAGE_KEY
  });
}

export function dockWorkspacePanel(
  layout: WorkspaceLayoutV1,
  panelId: WorkspacePanelId,
  edge: WorkspaceDockEdge
): WorkspaceLayoutV1 {
  return dockPanel(layout, panelId, edge);
}

export function insertWorkspacePanelBeforeStack(
  layout: WorkspaceLayoutV1,
  panelId: WorkspacePanelId,
  edge: WorkspaceDockEdge,
  beforeStackId: string
): WorkspaceLayoutV1 {
  return insertPanelBeforeStack(layout, panelId, edge, beforeStackId);
}

export function insertWorkspacePanelAtEnd(
  layout: WorkspaceLayoutV1,
  panelId: WorkspacePanelId,
  edge: WorkspaceDockEdge
): WorkspaceLayoutV1 {
  return insertPanelAtEnd(layout, panelId, edge);
}

export function joinWorkspaceStack(
  layout: WorkspaceLayoutV1,
  panelId: WorkspacePanelId,
  edge: WorkspaceDockEdge,
  stackId: string
): WorkspaceLayoutV1 {
  return joinStack(layout, panelId, edge, stackId);
}

export function activateWorkspacePanel(
  layout: WorkspaceLayoutV1,
  edge: WorkspaceDockEdge,
  stackId: string,
  panelId: WorkspacePanelId
): WorkspaceLayoutV1 {
  return activatePanel(layout, edge, stackId, panelId);
}

export function floatWorkspacePanel(
  layout: WorkspaceLayoutV1,
  panelId: WorkspacePanelId,
  rect?: Partial<Omit<WorkspaceFloat, 'panelId' | 'z'>>
): WorkspaceLayoutV1 {
  return floatPanel(layout, panelId, rect, () => 420);
}

export function hideWorkspacePanel(
  layout: WorkspaceLayoutV1,
  panelId: WorkspacePanelId
): WorkspaceLayoutV1 {
  return hidePanel(layout, panelId);
}

export function restoreWorkspacePanel(
  layout: WorkspaceLayoutV1,
  panelId: WorkspacePanelId
): WorkspaceLayoutV1 {
  return restorePanel(layout, panelId);
}

export function restoreDefaultPanelPlacement(
  layout: WorkspaceLayoutV1,
  panelId: WorkspacePanelId
): WorkspaceLayoutV1 {
  return restoreDefaultDockPanelPlacement(layout, panelId, DEFAULT_WORKSPACE_LAYOUT);
}

export function setToolStripEdge(
  layout: WorkspaceLayoutV1,
  edge: ToolStripEdge
): WorkspaceLayoutV1 {
  return setDockToolStripEdge(layout, edge);
}
