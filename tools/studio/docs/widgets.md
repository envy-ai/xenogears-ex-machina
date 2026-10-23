<!-- Adapted from rpg_creator docs/editor-widget-library.md at 1f690d0263af8881a2d33ee374a2e5006355748b; see PROVENANCE.md. -->
# Studio widget library

Canonical registry for the studio React DOM UI. Read the relevant entry before
changing UI; import widgets from their owning module. Feature styles own layout
and placement. Shared widget classes own appearance, focus and semantic state.

The implementation starts from the pinned Saga Foundry registry and only
includes the general layers authorized for studio milestones 1 and 3. The shell uses Graphite
Teal, compact density, Inter for UI and Inconsolata for technical entry. Other
copied theme tokens remain available through `editorTheme.ts`.

All UI lives under `.editor-shell`. Every icon-only action needs an accessible
name and tooltip. Text-entry labels belong above their controls. Use native
keyboard behavior where possible; show errors persistently with their original
message and stack. Honor reduced motion. Do not create private button/control
skins, menus, dialogs, reorder mechanisms or docking systems.

| Widget or contract | Source |
| --- | --- |
| Buttons | `src/styles/editor-buttons.scss`, `src/styles/_editor-button-theme.scss` |
| Controls/fields | `src/styles/editor-controls.scss`, `src/styles/_editor-control-theme.scss` |
| Typography | `src/styles/editor-typography.scss`, `src/styles/_editor-typography-theme.scss` |
| Surfaces | `src/styles/editor-surfaces.scss` |
| Context menus and small dialogs | `src/editor/ContextMenu.tsx`, `src/editor/editorCommands.ts` |
| Dialog shell | `src/editor/EditorDialogShell.tsx` |
| Number field | `src/editor/NumberField.tsx` |
| Record select | `src/editor/RecordSelect.tsx` |
| Docking | `src/editor/DockableWorkspace.tsx`, `src/editor/dockLayout.ts`, `src/editor/workspaceLayout.ts` |
| Flat/outline reorder | `src/editor/itemReorder.tsx` |
| Docker lists and list titles | `src/editor/DockerList.tsx` |
| Native window controls | `src/editor/NativeWindowControls.tsx` |
| Render failure boundary | `src/editor/EditorRenderErrorBoundary.tsx` |

## Buttons

Every editor button carries `editor-button`. A feature class may add layout or
identify a specialized control, but it must not replace the base class.

| Class | Use |
| --- | --- |
| `editor-button` | Ordinary action |
| `editor-button--standard` | Explicit ordinary treatment when a semantic modifier is selected dynamically |
| `editor-button--primary` | Main affirmative action for a surface |
| `editor-button--warning` | Risky but normally recoverable action |
| `editor-button--danger` | Destructive or irreversible action |
| `editor-button--compact` | Dense row, node, timeline, or toolbar action |
| `editor-button--icon` | Square icon-only control |
| `editor-button--subtle` | Low-emphasis chrome action |
| `editor-button--block` | Full available width |
| `editor-button--unstyled` | Audited escape hatch for a custom interactive visualization |

Example:

```tsx
<button
  type="button"
  className="editor-button editor-button--icon editor-button--danger"
  aria-label="Delete material"
  title="Delete material"
  onClick={removeMaterial}
>
  <Trash2 size={14} aria-hidden="true" />
</button>
```

Rules:

- Icon-only buttons require both an accessible name and a tooltip.
- Use `disabled` for native buttons whose action cannot run.
- Do not use color alone to communicate selection; use `aria-pressed`,
  `aria-selected`, or the relevant native state.
- Do not add ellipsis buttons to list items that expose their actions through
  right-click. `ContextMenuButton` is reserved for discoverable panel,
  collection, or tool-strip action menus.
- A single surface normally has one primary action.

## Controls and fields

Every editor `input`, `select`, and `textarea` carries `editor-control` plus
one type modifier.

| Class | Use |
| --- | --- |
| `editor-control--text` | Text, number, search, date-like, and other boxed single-line input |
| `editor-control--select` | Native select |
| `editor-control--textarea` | Multiline input |
| `editor-control--choice` | Checkbox or radio |
| `editor-control--range` | Range slider |
| `editor-control--color` | Native color input |
| `editor-control--file` | Native file input |
| `editor-control--compact` | Dense dimensions; combine with a type |
| `editor-control--code` | Inconsolata technical entry; combine with a type |
| `editor-control--block` | Full available width |
| `editor-control--invalid` | Invalid state; pair with `aria-invalid` |
| `editor-control--warning` | Non-blocking warning state |
| `editor-control--unstyled` | Audited custom-control escape hatch |

Field composition:

| Class | Use |
| --- | --- |
| `editor-field` | Top-aligned grid for label/control/description with the shared field gap |
| `editor-field--choice` | Choice label/caption group with captions aligned under the title |
| `editor-field-list` | Grid of ordinary or choice fields with the shared section gap |
| `editor-field-label` | Visible label text and label semantics |
| `editor-field-description` | Persistent supporting description |
| `editor-field-message` | Validation message |
| `editor-field-affix` | Unit, prefix, or suffix |

Use `input-with-suffix` with `editor-field-affix` for a visually joined unit
or suffix. The composite inherits the active control height, padding, type,
surface, edge, and radius tokens; do not size or color its suffix locally.

```tsx
<span className="input-with-suffix">
  <input
    className="editor-control editor-control--text"
    aria-label="Transition duration"
    type="number"
  />
  <span className="editor-field-affix" aria-hidden="true">ms</span>
</span>
```

| Class | Use |
| --- | --- |
| `editor-field-group` | Themed `fieldset` |
| `editor-field-group--plain` | Keep fieldset semantics and edge/spacing, but let the parent dialog surface show through |
| `editor-field-legend` | Visible `legend` |

Fields and field groups align their children to the top when a caller uses grid
or flex layout. The shared compact dialog body does the same, so extra available
height does not stretch form controls or the gaps between them.
`editor-field` owns the same grid and token-based gap for simple labelled controls
and composite controls. When an adjacent action prevents wrapping a complete
field in one `<label>`, use an `editor-field` wrapper containing a linked visible
label and the control/action row. A modal does not need an alignment variant.
`editor-field-description` is a block caption with compact 1.2 line spacing,
including when it wraps to multiple lines.


Labels with a direct native `editor-control--choice` input use a shared two-column
grid. Checkbox/radio inputs occupy the first column, while title text occupies
the second; wrapped lines align with the title's first letter, including labels
with plain text children. The shared choice-size token is 15px in both densities;
boxed-control compact heights do not change native choice dimensions. Form
captions belong to an `editor-field editor-field--choice` wrapper alongside the
choice label, using `editor-field-description` and the same field gap as text
fields. Captions occupy the title column, and `aria-describedby` links them to
the input without adding them to its accessible name. `editor-field-list` and
grid field groups use `--editor-field-section-gap` between fields, so a grouped
checkbox and caption have the same outer rhythm as a text field. Native label
click, Space and Tab interactions remain available.

Text-entry labels go above the field:

```tsx
<label className="editor-field editor-field-label">
  <span>Material name</span>
  <input
    className="editor-control editor-control--text"
    value={name}
    onChange={(event) => setName(event.target.value)}
  />
  <span className="editor-field-description">
    Displayed in the Materials docker.
  </span>
</label>
```

For an editable text field with optional suggestions, keep the
`editor-control--text` input and associate a native `<datalist>` through its
`list`/`id` pair. Use a unique ID when the field can mount more than once.
Native browser behavior owns suggestion pointer and keyboard interaction;
the input remains free to accept values outside the list. The Assistant Model
field and tileset Autotile family field use this composition. The caller owns
when suggestions are populated or cleared.

Invalid state links the message:

```tsx
<label className="editor-field editor-field-label">
  <span>Width</span>
  <input
    className="editor-control editor-control--text editor-control--invalid"
    type="number"
    aria-invalid="true"
    aria-describedby="material-width-error"
  />
  <span className="editor-field-message" id="material-width-error">
    Width must be a positive integer.
  </span>
</label>
```

Readonly data remains selectable. Disabled data is unavailable. Do not use
disabled styling to represent readonly values.

## Typography

Choose the semantic HTML heading level from the document outline, then choose
its visual role independently.

Heading roles:

- `editor-heading--application`
- `editor-heading--page`
- `editor-heading--panel`
- `editor-heading--section`
- `editor-heading--subsection`
- `editor-heading--card`

Every heading also carries `editor-heading`.

Recurring text uses `editor-text` with an optional role:

- `editor-text--secondary`
- `editor-text--caption`
- `editor-text--help`
- `editor-text--empty`
- `editor-text--status`
- `editor-text--success`
- `editor-text--warning`
- `editor-text--danger`
- `editor-text--code`

`editor-kicker` is the compact uppercase category treatment used above a
heading. Inter is the editor UI family. Inconsolata is used for IDs,
coordinates, technical values, and code.

## Themed surfaces

`src/styles/editor-surfaces.scss` owns shared surface mixins:

| Mixin | Purpose |
| --- | --- |
| `editor-overlay` | Modal dimming and blur below the native title bar |
| `editor-titlebar-safe-backdrop` | Leaves native window controls reachable |
| `editor-window` | Major modal/tool-window surface |
| `editor-window-header` | Major window header |
| `editor-window-icon` | Accent icon block |
| `editor-card` | Raised card or menu |
| `editor-inset-frame` | Recessed preview or code frame |
| `editor-list-row` | Shared docker row states |

`editor-card` and `editor-inset-frame` are also neutral class compositions of
those mixins with a one-pixel border and four-pixel radius. They leave spacing
and sizing to the caller. The shell may compose these surfaces around technical output.

Feature Sass may consume these mixins when defining a new reusable widget.
Ordinary feature styles should use an existing class contract instead.

All modal backdrops begin below
`var(--editor-titlebar-height, 58px)`. Do not blur, cover, or make the native
window controls unreachable. The shared overlay also marks each dialog as a
native `no-drag` region. This lets dialogs portalled under the draggable NW.js
project launcher receive mouse input while preserving the title-bar drag strip.

`editor-dialog-layout` composes with `editor-small-dialog` for a compact modal
with a fixed header and footer and a scrolling body. Its direct children use
`editor-dialog-layout__header`, `editor-dialog-layout__body`, and
`editor-dialog-layout__footer`. The feature chooses width and owns content and
actions. Body content aligns to the top. Layer Settings and ComfyUI image
generation share this layout; both still use `EditorDialogShell` for portal
placement and Escape behavior.

## Context menus

`src/editor/ContextMenu.tsx` owns every custom `role="menu"` implementation.
`EditorCommand` and `EditorContextMenuRequest` live in
`src/editor/editorCommands.ts`. Requests specify an exact target, accessible
label, viewport coordinates, commands and optional `restoreFocus` element.
Commands support groups, checked state, danger state, disabled reasons, color
swatches and child commands. Use a disabled reason when an action cannot run.

The widget owns viewport placement, initial focus, Up/Down, Home/End,
Left/Right, Escape/Tab dismissal, outside-pointer and blur dismissal, and focus
restoration. `contextMenuPointForElement` positions button-opened menus.
`isContextMenuKey` recognizes the Context Menu key and Shift+F10.
`ContextMenuButton` is the visible ellipsis trigger for panel/collection/tool
strip actions. Ordinary named menu-bar buttons open the same `ContextMenu`.

## Dialogs

Every modal composes `src/editor/EditorDialogShell.tsx`. It portals under the
editor shell, owns the backdrop and closes only the topmost dialog on Escape.
Props are `backdropClassName`, `onClose`, optional `testId`, and `children`.
The child surface retains responsibility for `role="dialog"` or `alertdialog`,
`aria-modal`, its accessible name, focus and actions. Cancel and Escape must
produce the same outcome. Backdrops leave the titlebar and native controls
reachable.

The copied `EditorPromptDialog`, `EditorChoiceDialog` and
`EditorConfirmDialog` from `ContextMenu.tsx` implement small single decisions.
They await callbacks, block duplicate confirmation and persist callback errors
with message/stack. Prompt owns a trimmed text draft, autofocus and empty-value
blocking; Choice validates against its current options. Confirm accepts title,
message, confirmLabel, optional dangerous, onConfirm and onCancel. All accept
an optional actual `dialogRef` and owner identity. Larger composed dialogs use
`editor-small-dialog editor-dialog-layout` with direct header/body/footer
classes `editor-dialog-layout__header`, `__body`, `__footer`.

`src/editor/EditorOperationOwners.tsx` keeps optional mounted-owner leases
without assistant or game dependencies. `EditorOperationOwnersProvider` accepts
`registry: EditorOperationOwnerRegistry`. Operations expose a schema with
`parse(unknown)`, a kind, description and execute callback. An
`EditorExecutionGuard` provides signal/assertCurrent/isCurrent. A successful
operation marked `releasesOwner` may close its owner. Other operations must
retain a current owner throughout execution. No registry is needed for normal
pointer/keyboard widget behavior.

## Fields

`src/editor/NumberField.tsx` composes the shared label and numeric control. It
requires a visible `label`, a numeric value and onChange callback. Optional
ariaLabel overrides the accessible input name. Its default conversion uses
Number(text), permits NaN and does not clamp. Explicit options select integer
truncation/rounding, emptyAs zero/nan, fallback passthrough/minimum/ignore,
native min/max/step, and optional clamping. The caller owns these policies and
commit semantics; compact/className/inputClassName only change composition.

`src/editor/RecordSelect.tsx` renders an ordered record collection of `{id,
name}`. Props: records, value, onChange(id), optional label, ariaLabel,
noneOption, filter, disabled, compact, className, selectClassName. Omit the
visible label only in a compact composition with an accessible name. Features
do not reproduce record-option mapping.

## Dockable workspace

`src/editor/DockableWorkspace.tsx` provides `useWorkspaceLayout`,
`DockableWorkspace`, `WorkspacePanel`, `WorkspaceCenter`, and
`DockableToolStrip`. `src/editor/dockLayout.ts` owns shared transitions;
`src/editor/workspaceLayout.ts` specializes the core for project/backend/log.

`useWorkspaceLayout()` returns `[layout, setLayout, resetLayout]`, reads the
studio v1 layout, persists subsequent changes, and applies the original
viewport geometry constraints on resize. Mount a stable
`new EditorPreferenceStore({storage})` through
`<EditorPreferenceProvider store={store}>`; omitted storage means
`window.localStorage`. This layout-only store accepts
`xem-studio.workspace-layout.v2`. There are no RPG preference keys or migrations.
Read/write/parse failures propagate to the application's persistent global
error UI or render error boundary; storage denial never fabricates success.

`DockableWorkspace` takes layout/setLayout/resetLayout, optional controlsTarget,
optional `centerless` (default false), and descriptor children. Studio uses the
center for event views. Default v2 placement is Project left at 300px, Live game
and Schema inspector right at 420px, and Backend/Log in bottom stacks at 220px.
The copied centerless option remains available for consumers without a center.

Each `WorkspacePanel` has an id (`project`, `backend`, `log`, `live`, `inspector`),
optional title/className and content. All panels can be moved, stacked, floated,
hidden and restored through the shared action menu or titlebar Workspace menu.
Drag the grip/tab or use the panel menu for keyboard docking/hiding. Reset
restores the defaults above. Layout survives reload under the v2 key. Never
create an independent movable panel.

`DockableToolStrip` remains available for consumers needing docked tools; it
accepts layout/setLayout/children and owns edge placement and its action menu.
Milestone 1 has no editing tools.

## Reordering

Use `src/editor/itemReorder.tsx`. `useItemReorder` accepts optional enabled
(default true), orientation (vertical), canDrop(sourceId,targetId), and
onReorder(sourceId,targetId,beforeOrAfter). Spread targetProps on rows and
pass handleProps to `ReorderGrip`. `previewItemReorder` displays the proposed
order without mutating caller data; `reorderDestinationIndex` computes the
final index. The flat hook uses native drag-and-drop; consumers retain ownership of the
committed order and any keyboard alternatives. Hierarchical controllers use
`OutlineReorderGrip` and supply keyboard arrow handling plus
nesting/indent/outdent state.
Filtering normally disables reorder with a reason in the grip title.

## Docker lists

Use `src/editor/DockerList.tsx` for flat lists and grids in docked panels.
`DockerList`, `DockerListStack`, `DockerListHeader`, `DockerListBody`,
`DockerListRow`, `DockerListSelect`, `EditorListTitle`, `DockerListFooter` and
`DockerListEmpty` own their structural class contracts. Do not reproduce
`docker-list-*` or `docker-item-*` markup in feature components.

Rows accept selected/intrinsicHeight/columns and optional reorder props
(label, handleProps, targetProps, enabled, surface). Use `EditorListTitle`
for ellipsized names with a tooltip and `DockerListEmpty` for an empty list.
Feature code owns row data, selection, actions and mutation. Nested trees
retain their distinct outline semantics.

## Native controls and failures

`src/editor/NativeWindowControls.tsx` exports `useNativeWindowControls` and
`NativeWindowControls`. The hook returns null in a browser; under NW.js it
tracks native maximize/minimize/restore events and normal client geometry.
The component provides named and titled minimize, maximize/restore and close
buttons. Native bridge exceptions propagate. Geometry lives in
`src/runtime/nativeWindowGeometry.ts`; state tracking in
`src/runtime/nativeWindowState.ts`. No game-window controller is copied.

`src/editor/EditorRenderErrorBoundary.tsx` preserves a caller-supplied failure
surface after a rendering/effect failure and logs the original error. The
application supplies persistent message/stack and recovery controls.

## Styles and enforcement

Import `src/styles/editor.scss` once. It composes the authoritative pinned
button/control/typography/surface Sass and the selected copied-widget/base/theme
rules in `_widget-structure.scss`. No generated CSS is committed. Font files
and their unchanged OFL licenses live under `public/fonts/`.

Run `npm run audit:design-system` after UI changes. The audit inspects actual
studio TSX tags for button/control/field/text contracts, prevents custom menus
outside ContextMenu and copied DockerList markup outside its owner, verifies
registry sources, and compiles feature Sass in memory to detect ordinary
control/text appearance overrides. It complements typechecking and rendered
interaction tests; it does not claim accessibility or pixel-perfect rendering.


## Read-only graph canvas

`src/editor/GraphCanvas.tsx` and `src/styles/graph-canvas.scss` own the shared
canvas contract. Callers provide nodes with explicit world position/dimensions,
labels/content, exact edge kind/status and selection; they own automatic layout
and domain navigation. The canvas owns readonly node frames/ports, cubic wires,
selection focus, viewport culling, pan and zoom. It has no palette, inspector,
node editing, project schema, draft recovery or graph history.

Middle-drag or Space+drag pans; wheel up zooms in and wheel down zooms out,
anchored under the pointer. Inputs enter at the top center and main outputs
leave at the bottom center. An edge may set `fromPort: "right"` for an alternate
output; the canvas shows a right-side port only on nodes using it. Omitted
`fromPort` means `"bottom"`. The event view assigns branch and table-entry edges
to the right port. All destinations retain their top input. Backward and self edges detour around
the frames while retaining those port directions; culling includes both arcs.
The copied zoom limits are 0.1–1.8. Home/Fit fits the supplied bounds; +/- zooms.
Arrow keys move focus/selection; Enter activates the selected node through the
caller. External selection is revealed even when initially outside the viewport.
Edges retain `data-kind`/`data-status`; diagnostic targets are labelled
non-executable markers, not invented decoded instructions. Source ranges and the
bounded MIT grant are in PROVENANCE. New canvas appearance is shared Sass;
ordinary controls retain the registered button/control classes.

## Names history and modal fields

`src/editor/deltaHistory.ts` supplies the copied generic reversible JSON delta
module. `src/editor/NameHistory.tsx` owns the name transaction controller/hook:
`value` is the last successfully saved table, `draft` is editable, and commit,
undo and redo validate/persist through the caller before changing committed
state or history. A rejected save preserves the draft and original error; a
rejected undo/redo preserves both stacks. Reset loads already validated data
without saving it. Focus/blur groups a field edit; Enter commits, Escape cancels
a draft, and Ctrl/Cmd+Z / Shift+Z / Y use the same metadata history.

The caller must report persistence failures to the persistent application error
surface and display the hook's error; invalid sidecars must not be replaced.
Only the names editor uses this history. Read-only game graphs never enter it.
`EditorDialogShell`'s `containFocus` option traps focus in the topmost modal form
and restores its connected opener. The names editor exercises it; its fields
retain visible labels above native shared controls.

## Event inspection widgets

`src/events/CommandList.tsx`, `ActorTree.tsx`, `ActorLanes.tsx`,
`MapDiagnostics.tsx` and `useWindowedRows.ts` compose registered DockerList,
button, surface and typography contracts. Feature Sass owns layout only.
Command rows retain exact PCs and original diagnostics, expose raw storage on
demand, copy/select text, and fold only backend-proven structured blocks.
List/tree/lanes bound mounted rows with the shared row window helper. Tree
arrows/Home/End navigate the 32 event slots per actor. Lanes preserve symbolic
party selectors and grouped possible variable dependencies; links navigate to
the source instruction. Map-wide diagnostic references remain available in all
three views. Ctrl+1/2/3 switches views, Ctrl+F finds within the map, F2 opens names,
Enter follows an explicit target, and Alt+left/right navigates jump history.


### Server disc picker

The browser Disc panel composes the existing `editor-field-list`, above-label
native select, ordinary buttons and persistent status text. Browse explicitly
loads the configured server catalogue; native arrows/type-ahead select an image
and Enter activates a focused button. Loading disables competing server actions.
Errors use the existing persistent Studio error surface. This composes the
registered field/button pattern without a separate skin or copied interaction.


## Live controls and schema inspector

The inspector composes a body-only DockerList using its `className` slot. Its
feature layout supplies one bounded 180px body row; DockerListBody owns scrolling,
so selecting details preserves the tree viewport and keyboard navigation. Schema
rows use the existing `intrinsicHeight` option to fit their name/type lines.

`src/game/LiveGame.tsx` composes DockerList, NumberField, RecordSelect and
EditorDialogShell in the Live game and Schema inspector docks. New layout
preferences use `xem-studio.workspace-layout.v2` with both right-hand panels;
previous layout storage is left intact. F6 cycles panels, F7 steps, Shift+F6
runs/pauses, and focused Enter activates a button or confirms a scalar form.
Shortcuts yield to text entry and modal dialogs. Restore requires an explicit
confirmation. Run state is acknowledged by the runtime.

The schema tree expands lazily, pages roots and large arrays, and follows typed
pointer paths. Native tree buttons retain Tab/Enter and Up/Down/Home/End focus
navigation. Type/path and original errors remain visible. Scalar entry uses a
text control to preserve exact 64-bit decimal integers; NumberField is reserved
for bounded frame counts without clamping. Unsupported/bit-field/untyped nodes
are explicit and cannot be written. The tree invalidates reads on session,
selection and timeline changes. Details remain inline in the persistent inspector.
An unapplied scalar draft holds a requested node switch: Write & read back applies
and reads the requested node, Cancel draft explicitly discards and switches, and
Keep editing clears the pending switch without losing text. A rejected write
retains the draft, selection and pending node. Refresh is disabled while dirty.
Timeline/connection changes retain dirty text for reference, disable writing it,
and require explicit cancellation before a new inspection. Feature Sass owns
layout only.

NumberField renders nonfinite draft state as an empty input while preserving the
nonfinite model value for explicit validation. The live frame field exercises
this existing passthrough/emptyAs behavior: clearing it cannot silently step a
frame or introduce a React NaN-value warning.

## Authored field drafts

`src/events/DraftEditor.tsx` composes registered above-label native text/select
controls, buttons, typography and inset surfaces below the active event view.
It adds no copied widget or modal. Selected operands remain inline. Apply uses
one shared-Rust batch; Cancel restores the selected instruction's fields. Text
entry retains native undo, while Ctrl+Enter/Escape apply/cancel. Insert and New
event compose the same fields over schema-derived templates; unsupported forms
are not invented. Event endings are the schema's End slot or Return forms.

`src/events/useFieldDraft.ts` owns service-handle lifetime and UI operation
ownership. Rust owns authored transactions and history. Pending map/project I/O
disables fields, and source/project/session/revision/inline generations reject
stale completion before adopting state. Changing selection with unapplied fields
keeps the fields and asks for Apply/Cancel in the persistent diagnostic surface.
Save/load, validation and builds use existing file/disc adapters. The toolbar
retains last-good build identity while invalid fields block new build/play.
Authored graphs are distinct from the live inspector; source-PC navigation maps
through stable IDs after validating the immutable live source component.

The event workspace keeps a minimum 200px view surface and scrolls as a whole
when toolbars, persistent diagnostics and the inline editor exceed the available
dock height. Its child controls do not shrink over the command viewport; list
rows retain normal pointer and keyboard access while errors remain visible.
