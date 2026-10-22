// Copied from rpg_creator src/editor/DockableWorkspace.tsx at 1f690d0263af8881a2d33ee374a2e5006355748b; see PROVENANCE.md.
import {
  Children,
  isValidElement,
  useEffect,
  useMemo,
  useRef,
  useState,
  type Dispatch,
  type CSSProperties,
  type PointerEvent as ReactPointerEvent,
  type ReactElement,
  type ReactNode,
  type SetStateAction
} from 'react';
import { createPortal } from 'react-dom';
import { Grip, PanelsTopLeft } from 'lucide-react';
import { ContextMenu, ContextMenuButton } from './ContextMenu';
import { contextMenuPointForElement, type EditorCommand, type EditorContextMenuRequest } from './editorCommands';
import {
  DEFAULT_WORKSPACE_LAYOUT,
  WORKSPACE_LAYOUT_STORAGE_KEY,
  activateWorkspacePanel,
  clampWorkspaceLayout,
  dockWorkspacePanel,
  floatWorkspacePanel,
  hideWorkspacePanel,
  insertWorkspacePanelAtEnd,
  insertWorkspacePanelBeforeStack,
  joinWorkspaceStack,
  readWorkspaceLayout,
  restoreDefaultPanelPlacement,
  restoreWorkspacePanel,
  setToolStripEdge,
  type ToolStripEdge,
  type WorkspaceDockEdge,
  type WorkspaceFloat,
  type WorkspaceLayoutV2,
  type WorkspacePanelId
} from './workspaceLayout';
import { usePersistedPreferences } from './usePersistedPreferences';
import { useEditorPreferenceStore } from './EditorPreferenceStore';

const PANEL_LABELS: Record<WorkspacePanelId, string> = {
  project: 'Project', backend: 'Backend', log: 'Log', live: 'Live game', inspector: 'Schema inspector'
};

export interface WorkspacePanelProps {
  id: WorkspacePanelId;
  title?: string;
  children: ReactNode;
  className?: string;
}

/** Descriptor consumed by DockableWorkspace. */
export function WorkspacePanel(_props: WorkspacePanelProps) {
  return null;
}

export function WorkspaceCenter(_props: { children: ReactNode }) {
  return null;
}

function viewport() {
  return { width: window.innerWidth, height: window.innerHeight };
}

export function useWorkspaceLayout(): [
  WorkspaceLayoutV2,
  Dispatch<SetStateAction<WorkspaceLayoutV2>>,
  () => void
] {
  const { hostStorage } = useEditorPreferenceStore();
  const [layout, setLayout] = useState<WorkspaceLayoutV2>(() =>
    readWorkspaceLayout(hostStorage, viewport())
  );
  usePersistedPreferences(WORKSPACE_LAYOUT_STORAGE_KEY, layout, setLayout, (current) =>
    clampWorkspaceLayout(current, viewport())
  );
  return [
    layout,
    setLayout,
    () => setLayout(structuredClone(DEFAULT_WORKSPACE_LAYOUT))
  ];
}

interface DockableWorkspaceProps {
  layout: WorkspaceLayoutV2;
  setLayout: Dispatch<SetStateAction<WorkspaceLayoutV2>>;
  resetLayout: () => void;
  controlsTarget?: HTMLElement | null;
  /** Let dock zones fill the workspace when there is no fixed center canvas. */
  centerless?: boolean;
  children: ReactNode;
}

interface PanelLocation {
  edge?: WorkspaceDockEdge;
  stackId?: string;
  floating?: WorkspaceFloat;
  hidden?: boolean;
}

function workspaceDropKey(drop: HTMLElement | null): string | undefined {
  if (!drop?.dataset.workspaceDrop) return undefined;
  return [
    drop.dataset.workspaceDrop,
    drop.dataset.edge,
    drop.dataset.stackId,
    drop.dataset.beforeStackId
  ].filter(Boolean).join(':');
}

type ResizeState =
  | {
      type: 'zone';
      edge: WorkspaceDockEdge;
      startX: number;
      startY: number;
      initial: WorkspaceLayoutV2;
    }
  | {
      type: 'stack';
      edge: WorkspaceDockEdge;
      index: number;
      nextIndex: number;
      startX: number;
      startY: number;
      initial: WorkspaceLayoutV2;
    }
  | {
      type: 'float';
      panelId: WorkspacePanelId;
      startX: number;
      startY: number;
      initial: WorkspaceLayoutV2;
    };

export function DockableWorkspace({
  layout,
  setLayout,
  resetLayout,
  controlsTarget,
  centerless = false,
  children
}: DockableWorkspaceProps) {
  const descriptors = useMemo(() => {
    const map = new Map<WorkspacePanelId, ReactElement<WorkspacePanelProps>>();
    Children.forEach(children, (child) => {
      if (isValidElement<WorkspacePanelProps>(child) && child.type === WorkspacePanel) {
        map.set(child.props.id, child);
      }
    });
    return map;
  }, [children]);
  const availablePanelIds = useMemo(
    () => new Set(descriptors.keys()),
    [descriptors]
  );
  const visibleStacks = (edge: WorkspaceDockEdge) =>
    layout.zones[edge].stacks.flatMap((stack, layoutIndex) => {
      const panels = stack.panels.filter((panelId) =>
        availablePanelIds.has(panelId)
      );
      if (panels.length === 0) return [];
      return [
        {
          stack: {
            ...stack,
            panels,
            activePanel: panels.includes(stack.activePanel)
              ? stack.activePanel
              : panels[0]
          },
          layoutIndex
        }
      ];
    });
  const center = useMemo(() => {
    let content: ReactNode = null;
    Children.forEach(children, (child) => {
      if (
        isValidElement<{ children: ReactNode }>(child) &&
        child.type === WorkspaceCenter
      ) {
        content = child.props.children;
      }
    });
    return content;
  }, [children]);
  const [menu, setMenu] = useState<EditorContextMenuRequest | null>(null);
  const [drag, setDrag] = useState<{
    panelId: WorkspacePanelId;
    pointerId: number;
    active: boolean;
    startX: number;
    startY: number;
    currentX: number;
    currentY: number;
    float?: WorkspaceFloat;
    hoveredDrop?: string;
  } | null>(null);
  const suppressPanelTabClickRef = useRef<WorkspacePanelId | null>(null);
  const [resize, setResize] = useState<ResizeState | null>(null);

  const locations = useMemo(() => {
    const result = new Map<WorkspacePanelId, PanelLocation>();
    for (const edge of ['left', 'right', 'bottom'] as const) {
      for (const stack of layout.zones[edge].stacks) {
        for (const panelId of stack.panels) {
          result.set(panelId, { edge, stackId: stack.id });
        }
      }
    }
    for (const entry of layout.floating) {
      result.set(entry.panelId, { floating: entry });
    }
    for (const panelId of layout.hidden) result.set(panelId, { hidden: true });
    return result;
  }, [layout]);

  useEffect(() => {
    if (!resize) return;
    const onMove = (event: PointerEvent) => {
      const deltaX = event.clientX - resize.startX;
      const deltaY = event.clientY - resize.startY;
      if (resize.type === 'zone') {
        const sign = resize.edge === 'right' ? -1 : 1;
        const delta = resize.edge === 'bottom' ? -deltaY : deltaX * sign;
        setLayout(
          clampWorkspaceLayout(
            {
              ...resize.initial,
              zones: {
                ...resize.initial.zones,
                [resize.edge]: {
                  ...resize.initial.zones[resize.edge],
                  size: resize.initial.zones[resize.edge].size + delta
                }
              }
            },
            viewport()
          )
        );
        return;
      }
      if (resize.type === 'stack') {
        const delta = resize.edge === 'bottom' ? deltaX : deltaY;
        const zone = resize.initial.zones[resize.edge];
        const span = resize.edge === 'bottom'
          ? Math.max(1, window.innerWidth)
          : Math.max(1, window.innerHeight - 110);
        const amount = (delta / span) * zone.stacks.reduce((sum, stack) => sum + stack.weight, 0);
        setLayout({
          ...resize.initial,
          zones: {
            ...resize.initial.zones,
            [resize.edge]: {
              ...zone,
              stacks: zone.stacks.map((stack, index) =>
                index === resize.index
                  ? { ...stack, weight: Math.max(0.25, stack.weight + amount) }
                  : index === resize.nextIndex
                    ? { ...stack, weight: Math.max(0.25, stack.weight - amount) }
                    : stack
              )
            }
          }
        });
        return;
      }
      const initialFloat = resize.initial.floating.find(
        (entry) => entry.panelId === resize.panelId
      );
      if (!initialFloat) return;
      setLayout(
        clampWorkspaceLayout(
          {
            ...resize.initial,
            floating: resize.initial.floating.map((entry) =>
              entry.panelId === resize.panelId
                ? {
                    ...entry,
                    width: initialFloat.width + deltaX,
                    height: initialFloat.height + deltaY
                  }
                : entry
            )
          },
          viewport()
        )
      );
    };
    const onUp = () => setResize(null);
    window.addEventListener('pointermove', onMove);
    window.addEventListener('pointerup', onUp, { once: true });
    return () => {
      window.removeEventListener('pointermove', onMove);
      window.removeEventListener('pointerup', onUp);
    };
  }, [resize, setLayout]);

  function panelCommands(panelId: WorkspacePanelId): EditorCommand[] {
    const location = locations.get(panelId);
    const commands: EditorCommand[] = (['left', 'right', 'bottom'] as const).map(
      (edge) => ({
        id: `panel.dock.${edge}`,
        label: `Dock ${edge}`,
        group: 'dock',
        checked: location?.edge === edge,
        execute: () => setLayout((current) => dockWorkspacePanel(current, panelId, edge))
      })
    );
    for (const edge of ['left', 'right', 'bottom'] as const) {
      for (const stack of layout.zones[edge].stacks) {
        const availableStackPanels = stack.panels.filter((id) =>
          availablePanelIds.has(id)
        );
        if (
          availableStackPanels.length === 0 ||
          availableStackPanels.includes(panelId)
        ) {
          continue;
        }
        const label = availableStackPanels
          .map(
            (id) =>
              descriptors.get(id)?.props.title ?? PANEL_LABELS[id]
          )
          .join(' / ');
        commands.push({
          id: `panel.stack.${edge}.${stack.id}`,
          label: `Join stack with ${label}`,
          group: 'stack',
          execute: () =>
            setLayout((current) =>
              joinWorkspaceStack(current, panelId, edge, stack.id)
            )
        });
      }
    }
    commands.push(
      {
        id: 'panel.float',
        label: 'Float panel',
        group: 'placement',
        checked: Boolean(location?.floating),
        execute: () =>
          setLayout((current) =>
            clampWorkspaceLayout(floatWorkspacePanel(current, panelId), viewport())
          )
      },
      {
        id: 'panel.reset-size',
        label: 'Reset size',
        group: 'placement',
        execute: () => {
          const defaults = DEFAULT_WORKSPACE_LAYOUT;
          setLayout((current) => {
            const currentLocation = locations.get(panelId);
            if (currentLocation?.floating) {
              return clampWorkspaceLayout(
                floatWorkspacePanel(current, panelId),
                viewport()
              );
            }
            const edge = currentLocation?.edge ?? defaults.lastDock[panelId];
            return {
              ...current,
              zones: {
                ...current.zones,
                [edge]: {
                  ...current.zones[edge],
                  size: defaults.zones[edge].size,
                  stacks: current.zones[edge].stacks.map((stack) =>
                    stack.id === currentLocation?.stackId
                      ? { ...stack, weight: 1 }
                      : stack
                  )
                }
              }
            };
          });
        }
      },
      {
        id: 'panel.default',
        label: 'Restore default placement',
        group: 'placement',
        execute: () =>
          setLayout((current) => restoreDefaultPanelPlacement(current, panelId))
      },
      {
        id: 'panel.hide',
        label: 'Hide panel',
        group: 'visibility',
        dangerous: false,
        execute: () => setLayout((current) => hideWorkspacePanel(current, panelId))
      }
    );
    return commands;
  }

  function openPanelMenu(
    panelId: WorkspacePanelId,
    element: HTMLElement,
    point = contextMenuPointForElement(element)
  ) {
    setMenu({
      target: { type: 'panel', panelId },
      label: `${PANEL_LABELS[panelId]} panel`,
      ...point,
      restoreFocus: element,
      commands: panelCommands(panelId)
    });
  }

  function beginPanelDrag(
    event: ReactPointerEvent<HTMLButtonElement>,
    panelId: WorkspacePanelId
  ) {
    if (event.button !== 0) return;
    event.currentTarget.setPointerCapture(event.pointerId);
    setDrag({
      panelId,
      pointerId: event.pointerId,
      active: false,
      startX: event.clientX,
      startY: event.clientY,
      currentX: event.clientX,
      currentY: event.clientY,
      float: locations.get(panelId)?.floating
    });
  }

  function updatePanelDrag(event: ReactPointerEvent<HTMLButtonElement>) {
    if (!drag || drag.pointerId !== event.pointerId) return;
    const moved =
      drag.active ||
      Math.hypot(
        event.clientX - drag.startX,
        event.clientY - drag.startY
      ) >= 5;
    const hit = moved
      ? document.elementFromPoint(event.clientX, event.clientY) as HTMLElement | null
      : null;
    const drop = hit?.closest<HTMLElement>('[data-workspace-drop]') ?? null;
    setDrag((current) =>
      current
        ? {
            ...current,
            active: moved,
            currentX: event.clientX,
            currentY: event.clientY,
            hoveredDrop: moved ? workspaceDropKey(drop) : undefined
          }
        : null
    );
  }

  function finishPanelDrag(event: ReactPointerEvent<HTMLButtonElement>) {
    if (!drag || drag.pointerId !== event.pointerId) return;
    if (!drag.active) {
      setDrag(null);
      return;
    }
    suppressPanelTabClickRef.current = drag.panelId;
    window.setTimeout(() => {
      if (suppressPanelTabClickRef.current === drag.panelId) {
        suppressPanelTabClickRef.current = null;
      }
    }, 0);
    const target = document.elementFromPoint(event.clientX, event.clientY) as HTMLElement | null;
    const drop = target?.closest<HTMLElement>('[data-workspace-drop]');
    if (drop?.dataset.workspaceDrop === 'edge') {
      const edge = drop.dataset.edge as WorkspaceDockEdge;
      setLayout((current) => dockWorkspacePanel(current, drag.panelId, edge));
    } else if (drop?.dataset.workspaceDrop === 'insert') {
      const edge = drop.dataset.edge as WorkspaceDockEdge;
      const beforeStackId = drop.dataset.beforeStackId!;
      setLayout((current) =>
        insertWorkspacePanelBeforeStack(
          current,
          drag.panelId,
          edge,
          beforeStackId
        )
      );
    } else if (drop?.dataset.workspaceDrop === 'insert-end') {
      const edge = drop.dataset.edge as WorkspaceDockEdge;
      setLayout((current) =>
        insertWorkspacePanelAtEnd(current, drag.panelId, edge)
      );
    } else if (drop?.dataset.workspaceDrop === 'stack') {
      const edge = drop.dataset.edge as WorkspaceDockEdge;
      const stackId = drop.dataset.stackId!;
      setLayout((current) =>
        joinWorkspaceStack(current, drag.panelId, edge, stackId)
      );
    } else if (drop?.dataset.workspaceDrop === 'float') {
      setLayout((current) =>
        clampWorkspaceLayout(
          floatWorkspacePanel(current, drag.panelId, {
            x: Math.max(8, event.clientX - 170),
            y: Math.max(8, event.clientY - 22)
          }),
          viewport()
        )
      );
    } else if (drag.float) {
      const deltaX = event.clientX - drag.startX;
      const deltaY = event.clientY - drag.startY;
      setLayout((current) =>
        clampWorkspaceLayout(
          {
            ...current,
            floating: current.floating.map((entry) =>
              entry.panelId === drag.panelId
                ? { ...entry, x: drag.float!.x + deltaX, y: drag.float!.y + deltaY }
                : entry
            )
          },
          viewport()
        )
      );
    }
    setDrag(null);
  }

  function renderPanelChrome(
    panelId: WorkspacePanelId,
    edge: WorkspaceDockEdge | undefined,
    stackId: string | undefined,
    floating = false
  ) {
    const descriptor = descriptors.get(panelId);
    if (!descriptor) return null;
    const stackDrop = edge && stackId ? `stack:${edge}:${stackId}` : undefined;
    const dropTargetActive = Boolean(
      drag && stackDrop && drag.hoveredDrop === stackDrop
    );
    const stack = edge && stackId
      ? layout.zones[edge].stacks.find((entry) => entry.id === stackId)
      : undefined;
    const stackPanels = (stack?.panels ?? [panelId]).filter((id) =>
      availablePanelIds.has(id)
    );
    return (
      <section
        className={`workspace-panel ${floating ? 'floating' : 'docked'} ${dropTargetActive ? 'drop-target-active' : ''} ${descriptor.props.className ?? ''}`}
        data-panel-id={panelId}
        aria-label={`${descriptor.props.title ?? PANEL_LABELS[panelId]} panel`}
        onPointerDown={() => {
          if (!floating) return;
          const topZ = Math.max(0, ...layout.floating.map((entry) => entry.z)) + 1;
          setLayout((current) => ({
            ...current,
            floating: current.floating.map((entry) =>
              entry.panelId === panelId ? { ...entry, z: topZ } : entry
            )
          }));
        }}
      >
        <header
          className="workspace-panel-titlebar"
          data-workspace-drop={drag?.active && stackId ? 'stack' : undefined}
          data-edge={edge}
          data-stack-id={stackId}
          onContextMenu={(event) => {
            event.preventDefault();
            openPanelMenu(panelId, event.currentTarget, {
              x: event.clientX,
              y: event.clientY
            });
          }}
        >
          <button
            type="button"
            className="editor-button workspace-panel-grip"
            aria-label={`Drag ${PANEL_LABELS[panelId]} panel`}
            title={`Drag ${PANEL_LABELS[panelId]} panel`}
            onPointerDown={(event) => beginPanelDrag(event, panelId)}
            onPointerMove={updatePanelDrag}
            onPointerUp={finishPanelDrag}
            onPointerCancel={() => setDrag(null)}
          >
            <Grip size={13} />
          </button>
          <div className="workspace-panel-tabs" role="tablist" aria-label={`${PANEL_LABELS[panelId]} stack`}>
            {stackPanels.map((tabId) => (
              <button
                className="editor-button"
                type="button"
                role="tab"
                aria-selected={tabId === panelId}
                title={`Drag ${PANEL_LABELS[tabId]} panel or click to activate`}
                key={tabId}
                onPointerDown={(event) => beginPanelDrag(event, tabId)}
                onPointerMove={updatePanelDrag}
                onPointerUp={finishPanelDrag}
                onPointerCancel={() => setDrag(null)}
                onClick={(event) => {
                  if (suppressPanelTabClickRef.current === tabId) {
                    suppressPanelTabClickRef.current = null;
                    event.preventDefault();
                    return;
                  }
                  if (edge && stackId) {
                    setLayout((current) =>
                      activateWorkspacePanel(current, edge, stackId, tabId)
                    );
                  }
                }}
              >
                {descriptors.get(tabId)?.props.title ?? PANEL_LABELS[tabId]}
              </button>
            ))}
          </div>
          <ContextMenuButton
            label={`${PANEL_LABELS[panelId]} panel actions`}
            onOpen={(event) => openPanelMenu(panelId, event.currentTarget)}
          />
        </header>
        <div className="workspace-panel-body">{descriptor.props.children}</div>
        {floating && (
          <button
            type="button"
            className="editor-button workspace-float-resize"
            aria-label={`Resize ${PANEL_LABELS[panelId]} panel`}
            title={`Resize ${PANEL_LABELS[panelId]} panel`}
            onPointerDown={(event) => {
              event.preventDefault();
              setResize({
                type: 'float',
                panelId,
                startX: event.clientX,
                startY: event.clientY,
                initial: structuredClone(layout)
              });
            }}
          />
        )}
      </section>
    );
  }

  function renderZone(edge: WorkspaceDockEdge) {
    const stacks = visibleStacks(edge);
    if (stacks.length === 0) return null;
    return (
      <aside
        className="workspace-dock-zone"
        data-edge={edge}
        aria-label={`${edge} dock zone`}
      >
        {drag?.active && (
          <div
            className="workspace-stack-edge-insertion start"
            aria-label={`Insert panel at start of ${edge} dock`}
            data-workspace-drop="insert"
            data-edge={edge}
            data-before-stack-id={stacks[0]!.stack.id}
            data-drop-active={
              drag.hoveredDrop ===
                `insert:${edge}:${stacks[0]!.stack.id}` ||
              undefined
            }
          />
        )}
        {stacks.map(({ stack, layoutIndex }, visibleIndex) => (
          <div
            className="workspace-dock-stack"
            key={stack.id}
            style={{ flexGrow: stack.weight, flexBasis: 0 }}
          >
            {renderPanelChrome(stack.activePanel, edge, stack.id)}
            {visibleIndex < stacks.length - 1 && (
              <div
                className="workspace-stack-separator"
                role="separator"
                tabIndex={0}
                aria-orientation={edge === 'bottom' ? 'vertical' : 'horizontal'}
                aria-label={`Resize ${edge} panel split ${visibleIndex + 1}`}
                aria-valuemin={25}
                aria-valuemax={800}
                aria-valuenow={Math.round(stack.weight * 100)}
                data-workspace-drop={drag?.active ? 'insert' : undefined}
                data-edge={drag?.active ? edge : undefined}
                data-before-stack-id={
                  drag?.active
                    ? stacks[visibleIndex + 1]!.stack.id
                    : undefined
                }
                data-drop-active={
                  drag?.hoveredDrop ===
                    `insert:${edge}:${stacks[visibleIndex + 1]!.stack.id}` ||
                  undefined
                }
                onPointerDown={(event) => {
                  if (drag?.active) return;
                  event.preventDefault();
                  setResize({
                    type: 'stack',
                    edge,
                    index: layoutIndex,
                    nextIndex: stacks[visibleIndex + 1]!.layoutIndex,
                    startX: event.clientX,
                    startY: event.clientY,
                    initial: structuredClone(layout)
                  });
                }}
                onKeyDown={(event) => {
                  const delta =
                    event.key === 'ArrowDown' || event.key === 'ArrowRight'
                      ? 0.1
                      : event.key === 'ArrowUp' || event.key === 'ArrowLeft'
                        ? -0.1
                        : 0;
                  if (!delta) return;
                  event.preventDefault();
                  setLayout((current) => ({
                    ...current,
                    zones: {
                      ...current.zones,
                      [edge]: {
                        ...current.zones[edge],
                        stacks: current.zones[edge].stacks.map((entry, itemIndex) =>
                          itemIndex === layoutIndex
                            ? { ...entry, weight: Math.max(0.25, entry.weight + delta) }
                            : itemIndex === stacks[visibleIndex + 1]!.layoutIndex
                              ? { ...entry, weight: Math.max(0.25, entry.weight - delta) }
                              : entry
                        )
                      }
                    }
                  }));
                }}
              />
            )}
          </div>
        ))}
        {drag?.active && (
          <div
            className="workspace-stack-edge-insertion end"
            aria-label={`Insert panel at end of ${edge} dock`}
            data-workspace-drop="insert-end"
            data-edge={edge}
            data-drop-active={
              drag.hoveredDrop === `insert-end:${edge}` || undefined
            }
          />
        )}
      </aside>
    );
  }

  const leftStacks = visibleStacks('left');
  const rightStacks = visibleStacks('right');
  const bottomStacks = visibleStacks('bottom');
  const gridStyle = {
    '--workspace-left-size': leftStacks.length
      ? `${layout.zones.left.size}px`
      : '0px',
    '--workspace-right-size': rightStacks.length
      ? `${layout.zones.right.size}px`
      : '0px',
    '--workspace-bottom-size': bottomStacks.length
      ? `${layout.zones.bottom.size}px`
      : '0px'
  } as CSSProperties;

  return (
    <div
      className="dockable-workspace"
      style={gridStyle}
      data-dragging={Boolean(drag?.active)}
      data-centerless={centerless || undefined}
      data-has-left={leftStacks.length > 0 || undefined}
      data-has-right={rightStacks.length > 0 || undefined}
    >
      {renderZone('left')}
      {leftStacks.length > 0 && (
        <div
          className="workspace-zone-separator"
          data-edge="left"
          role="separator"
          tabIndex={0}
          aria-orientation="vertical"
          aria-label="Resize left dock"
          aria-valuemin={180}
          aria-valuemax={700}
          aria-valuenow={Math.round(layout.zones.left.size)}
          onPointerDown={(event) => setResize({
            type: 'zone', edge: 'left', startX: event.clientX, startY: event.clientY,
            initial: structuredClone(layout)
          })}
          onKeyDown={(event) => {
            const delta = event.key === 'ArrowRight' ? 10 : event.key === 'ArrowLeft' ? -10 : 0;
            if (!delta) return;
            event.preventDefault();
            setLayout((current) => clampWorkspaceLayout({
              ...current,
              zones: { ...current.zones, left: { ...current.zones.left, size: current.zones.left.size + delta } }
            }, viewport()));
          }}
        />
      )}
      {!centerless && <div className="workspace-center">{center}</div>}
      {rightStacks.length > 0 && !centerless && (
        <div
          className="workspace-zone-separator"
          data-edge="right"
          role="separator"
          tabIndex={0}
          aria-orientation="vertical"
          aria-label="Resize right dock"
          aria-valuemin={240}
          aria-valuemax={700}
          aria-valuenow={Math.round(layout.zones.right.size)}
          onPointerDown={(event) => setResize({
            type: 'zone', edge: 'right', startX: event.clientX, startY: event.clientY,
            initial: structuredClone(layout)
          })}
          onKeyDown={(event) => {
            const delta = event.key === 'ArrowLeft' ? 10 : event.key === 'ArrowRight' ? -10 : 0;
            if (!delta) return;
            event.preventDefault();
            setLayout((current) => clampWorkspaceLayout({
              ...current,
              zones: { ...current.zones, right: { ...current.zones.right, size: current.zones.right.size + delta } }
            }, viewport()));
          }}
        />
      )}
      {renderZone('right')}
      {bottomStacks.length > 0 && (
        <div
          className="workspace-zone-separator"
          data-edge="bottom"
          role="separator"
          tabIndex={0}
          aria-orientation="horizontal"
          aria-label="Resize bottom dock"
          aria-valuemin={180}
          aria-valuemax={500}
          aria-valuenow={Math.round(layout.zones.bottom.size)}
          onPointerDown={(event) => setResize({
            type: 'zone', edge: 'bottom', startX: event.clientX, startY: event.clientY,
            initial: structuredClone(layout)
          })}
        />
      )}
      {renderZone('bottom')}

      {layout.floating
        .filter((entry) => availablePanelIds.has(entry.panelId))
        .map((entry) => (
        <div
          className="workspace-floating-slot"
          key={entry.panelId}
          style={{
            left: entry.x,
            top: entry.y,
            width: entry.width,
            height: entry.height,
            zIndex: entry.z
          }}
        >
          {renderPanelChrome(entry.panelId, undefined, undefined, true)}
        </div>
        ))}

      {controlsTarget
        ? createPortal(
        <button
          className="editor-button editor-button--icon icon-button workspace-titlebar-button"
          type="button"
          aria-label="Workspace layout actions"
          title="Workspace layout actions"
          onClick={(event) => {
            const commands: EditorCommand[] = layout.hidden
              .filter((panelId) => availablePanelIds.has(panelId))
              .map((panelId) => ({
              id: `workspace.show.${panelId}`,
              label: `Show ${PANEL_LABELS[panelId]}`,
              group: 'show',
              execute: () => setLayout((current) => restoreWorkspacePanel(current, panelId))
            }));
            commands.push({
              id: 'workspace.reset',
              label: 'Reset workspace layout',
              group: 'reset',
              execute: resetLayout
            });
            setMenu({
              target: { type: 'panel', panelId: 'workspace' },
              label: 'Workspace layout',
              ...contextMenuPointForElement(event.currentTarget),
              restoreFocus: event.currentTarget,
              commands
            });
          }}
        >
          <PanelsTopLeft size={17} />
        </button>,
        controlsTarget
        )
        : null}

      {drag?.active && (
        <div className="workspace-dock-preview" aria-hidden="true">
          {(['left', 'right', 'bottom'] as const).map((edge) => (
            <div
              key={edge}
              className="workspace-drop-target edge"
              data-workspace-drop="edge"
              data-edge={edge}
              data-drop-active={drag.hoveredDrop === `edge:${edge}` || undefined}
            >
              Dock {edge}
            </div>
          ))}
          <div
            className="workspace-drop-target float"
            data-workspace-drop="float"
            data-drop-active={drag.hoveredDrop === 'float' || undefined}
          >
            Float
          </div>
        </div>
      )}
      <ContextMenu request={menu} onClose={() => setMenu(null)} />
    </div>
  );
}

export function DockableToolStrip({
  layout,
  setLayout,
  children
}: {
  layout: WorkspaceLayoutV2;
  setLayout: Dispatch<SetStateAction<WorkspaceLayoutV2>>;
  children: ReactNode;
}) {
  const [menu, setMenu] = useState<EditorContextMenuRequest | null>(null);
  const [dragBounds, setDragBounds] = useState<{
    top: number;
    right: number;
    bottom: number;
    left: number;
  } | null>(null);
  const dragging = Boolean(dragBounds);

  function commands(): EditorCommand[] {
    return (['top', 'right', 'bottom', 'left'] as const).map((edge) => ({
      id: `toolstrip.${edge}`,
      label: `Dock ${edge}`,
      checked: layout.toolStripEdge === edge,
      group: 'dock',
      execute: () => setLayout((current) => setToolStripEdge(current, edge))
    }));
  }

  useEffect(() => {
    if (!dragBounds) return;
    const finishDrag = (event: PointerEvent) => {
      const distances: Array<[ToolStripEdge, number]> = [
        ['top', Math.abs(event.clientY - dragBounds.top)],
        ['bottom', Math.abs(dragBounds.bottom - event.clientY)],
        ['left', Math.abs(event.clientX - dragBounds.left)],
        ['right', Math.abs(dragBounds.right - event.clientX)]
      ];
      distances.sort((a, b) => a[1] - b[1]);
      setLayout((current) => setToolStripEdge(current, distances[0][0]));
      setDragBounds(null);
    };
    const cancelDrag = () => setDragBounds(null);
    window.addEventListener('pointerup', finishDrag, { once: true });
    window.addEventListener('pointercancel', cancelDrag, { once: true });
    return () => {
      window.removeEventListener('pointerup', finishDrag);
      window.removeEventListener('pointercancel', cancelDrag);
    };
  }, [dragBounds, setLayout]);

  return (
    <div className="map-toolbar" data-edge={layout.toolStripEdge} data-dragging={dragging}>
      <div className="toolstrip-chrome">
        <button
          type="button"
          className="editor-button toolstrip-grip"
          aria-label="Drag map tool strip"
          title="Drag map tool strip"
          onPointerDown={(event) => {
            if (event.button !== 0) return;
            const bounds = event.currentTarget
              .closest<HTMLElement>('.map-workbench')
              ?.getBoundingClientRect();
            if (bounds) {
              setDragBounds({
                top: bounds.top,
                right: bounds.right,
                bottom: bounds.bottom,
                left: bounds.left
              });
            }
          }}
        >
          <Grip size={13} />
        </button>
        <ContextMenuButton
          label="Map tool strip actions"
          onOpen={(event) => setMenu({
            target: { type: 'toolstrip', toolstripId: 'map-tools' },
            label: 'Map tool strip',
            ...contextMenuPointForElement(event.currentTarget),
            restoreFocus: event.currentTarget,
            commands: commands()
          })}
        />
      </div>
      <div className="toolstrip-content">{children}</div>
      <ContextMenu request={menu} onClose={() => setMenu(null)} />
    </div>
  );
}
