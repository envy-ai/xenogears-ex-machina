# Provenance of code taken from Saga Foundry RPG Creator

Files under `tools/studio/` that started as copies of Saga Foundry RPG Creator
(`~/rpg_creator`, licensed AGPL-3.0-only) are listed below, with their source
path and revision. Code files also name them in a header comment. Font binaries and verbatim OFL
notices use `public/fonts/README.md` and this table for attribution instead;
licence text is never modified to add a code-style header.

## License grant

The author of Saga Foundry RPG Creator (envy-ai, the owner of this fork)
granted the MIT license for the files listed below, for use in this
repository, in the Claude Code session of 2026-10-20 ("MIT is granted"). Those
code files are therefore under this repository's MIT license (`LICENSE`).
The fonts and their notices retain OFL 1.1, as stated below.

On 2026-10-21 at 03:28:46 UTC, Bart explicitly approved a bounded MIT
extension for the reusable canvas controls from `EventGraphEditor.tsx`, generic
`deltaHistory.ts` lines 1–813, and focused-control transactions from
`editorState.ts`, all at the same pinned revision below (answer: “Yes, and yes.”).
This covers only the general layers extracted for milestone three; RPG-specific
schemas, palette/inspectors, graph history, draft recovery and project reducers
are excluded. Exact retained ranges and adaptations are recorded in the rows
below. Source attribution headers and this grant record accompany the copies;
the repository MIT permission notice remains in `LICENSE`. No original
third-party notice is removed, and the OFL font terms remain unchanged.

## Copied files

Source revision: rpg_creator `1f690d0263af8881a2d33ee374a2e5006355748b`.

| Here | From | Changes |
| --- | --- | --- |
| `docs/widgets.md` | `docs/editor-widget-library.md` | Adapted the copied widget/class contracts to studio scope and documented centerless/layout-only/generic-owner changes. |
| `public/fonts/inconsolata/Inconsolata-Variable.ttf` | `public/fonts/inconsolata/Inconsolata-Variable.ttf` | Unchanged OFL 1.1 binary; source/revision in public/fonts/README.md (binary header exception). |
| `public/fonts/inconsolata/OFL.txt` | `public/fonts/inconsolata/OFL.txt` | Unchanged OFL 1.1 notice, byte-for-byte; source/revision in public/fonts/README.md (verbatim licence header exception). |
| `public/fonts/inter/Inter-Variable.ttf` | `public/fonts/inter/Inter-Variable.ttf` | Unchanged OFL 1.1 binary; source/revision in public/fonts/README.md (binary header exception). |
| `public/fonts/inter/OFL.txt` | `public/fonts/inter/OFL.txt` | Unchanged OFL 1.1 notice, byte-for-byte; source/revision in public/fonts/README.md (verbatim licence header exception). |
| `scripts/audit-design-system.mjs` | `scripts/audit_editor_design_system.mjs` | Adapted shared tag scanner and ownership rules to studio; feature Sass appearance audit compiles in memory; removed RPG/domain requirements. |
| `src/domain/ids.ts` | `src/domain/ids.ts` | Unchanged apart from source/revision header. |
| `src/editor/ContextMenu.tsx` | `src/editor/ContextMenu.tsx` | Guard type redirected to generic EditorOperationOwners; other behavior unchanged. |
| `src/editor/DockableWorkspace.tsx` | `src/editor/DockableWorkspace.tsx` | Studio panel labels/types; optional centerless layout with redundant separator omitted; behavior otherwise preserved. |
| `src/editor/DockerList.tsx` | `src/editor/DockerList.tsx` | Unchanged apart from source/revision header. |
| `src/editor/EditorDialogShell.tsx` | `src/editor/EditorDialogShell.tsx` | Added optional focus containment/opener restoration for names; existing Escape/portal/owner behavior retained. |
| `src/editor/EditorOperationOwners.tsx` | `src/editor/EditorOperationOwners.tsx` | Generic schema/operation/guard types; removed assistant, runtime image and domain dependencies; owner leases retained. |
| `src/editor/EditorPreferenceStore.tsx` | `src/editor/EditorPreferenceStore.tsx` | Replaced project/domain preference aggregation with synchronous studio layout-only Storage scope; failures propagate. |
| `src/editor/EditorRenderErrorBoundary.tsx` | `src/editor/EditorRenderErrorBoundary.tsx` | Unchanged apart from source/revision header. |
| `src/editor/NativeWindowControls.tsx` | `src/editor/NativeWindowControls.tsx` | Geometry import extracted; native bridge exceptions now propagate. |
| `src/editor/NumberField.tsx` | `src/editor/NumberField.tsx` | Source/revision header; nonfinite draft state renders an empty native numeric input without changing or clamping its value. |
| `src/editor/RecordSelect.tsx` | `src/editor/RecordSelect.tsx` | Unchanged apart from source/revision header. |
| `src/editor/dockLayout.ts` | `src/editor/dockLayout.ts` | Unchanged apart from source/revision header. |
| `src/editor/editorCommands.ts` | `src/editor/editorCommands.ts` | Unchanged apart from source/revision header. |
| `src/editor/editorTheme.ts` | `src/editor/editorTheme.ts` | Unchanged apart from source/revision header. |
| `src/editor/itemReorder.tsx` | `src/editor/itemReorder.tsx` | Unchanged apart from source/revision header. |
| `src/editor/storedJson.ts` | `src/editor/storedJson.ts` | Unchanged apart from source/revision header. |
| `src/editor/usePersistedPreferences.ts` | `src/editor/usePersistedPreferences.ts` | Unchanged apart from source/revision header. |
| `src/editor/workspaceLayout.ts` | `src/editor/workspaceLayout.ts` | Studio project/backend/log/live/inspector IDs, v2 schema/key and approved central event view with bottom Backend/Log default; removed RPG v3 migration and game-specific float width. |
| `src/runtime/nativeWindowGeometry.ts` | `src/runtime/nativeGameWindow.ts` | Extracted x/y/width/height shape only; omitted game-window controller. |
| `src/runtime/nativeWindowState.ts` | `src/runtime/nativeWindowState.ts` | Unchanged apart from source/revision header. |
| `src/styles/_editor-button-theme.scss` | `src/_editor-button-theme.scss` | Unchanged apart from source/revision header. |
| `src/styles/_editor-control-theme.scss` | `src/_editor-control-theme.scss` | Unchanged apart from source/revision header. |
| `src/styles/_editor-typography-theme.scss` | `src/_editor-typography-theme.scss` | Unchanged apart from source/revision header. |
| `src/styles/_widget-structure.scss` | `src/styles.css` | Selected copied-widget structural selectors and base/font/theme rules only; removed unrelated grouped branches and unused italic font face. |
| `src/styles/editor-buttons.scss` | `src/editor-buttons.scss` | Unchanged apart from source/revision header. |
| `src/styles/editor-controls.scss` | `src/editor-controls.scss` | Unchanged apart from source/revision header. |
| `src/styles/editor-surfaces.scss` | `src/editor-surfaces.scss` | Unchanged apart from source/revision header. |
| `src/styles/editor-typography.scss` | `src/editor-typography.scss` | Unchanged apart from source/revision header. |
| `src/editor/GraphCanvas.tsx` | `src/editor/EventGraphEditor.tsx` | General read-only canvas ranges detailed below; pan/zoom, viewport culling, wires/ports and focus only. Dynamic finite bounds/origin, browser-safe extent checks, vertical/right output ports and reversed wheel orientation are newly authored adaptations; domain/editor mutation code excluded. |
| `src/editor/deltaHistory.ts` | `src/editor/deltaHistory.ts` lines 1–813 | Verbatim generic module after source/grant headers; existing generic IDs import reused. |
| `src/editor/NameHistory.tsx` | `src/editor/editorState.ts` lines 3552–3559, 3561–3564, 3567–3582, 3616–3634 | Adapted focused-control labels/grouping only; newly authored validated persistence controller. No project reducer or global listeners. |


The font binaries and their OFL 1.1 notices retain their own license;
the MIT code grant above does not relicense the fonts. Their source/revision
is recorded in `public/fonts/README.md`. The OFL.txt notices are verbatim pinned
copies, including original whitespace, and have no added attribution header. `src/styles/editor.scss` is newly authored composition/layout glue.


## Milestone-three exact source ranges

The following hashes refer to the pinned source, before adaptation. Original
TypeScript files have no per-file copyright/licence headers to retain. The
generic source attribution and author-approved MIT grant headers accompany all
three destinations. `src/styles/graph-canvas.scss` is newly authored shared
appearance using existing tokens; no upstream stylesheet was copied.

## `src/editor/EventGraphEditor.tsx`

Whole-file SHA-256: `e2a1c662a51d321764dab9ab5bfe81230cfdb67e830854c9a0902fe90ecf9448`.

| Exact retained/adapted source lines | Range SHA-256 | Retained behavior |
| --- | --- | --- |
| 111–122 | `b2cfe660eb87c27e7470d49426e60f3d305e2f6ea84aa2b24cdc8da2868acc3c` | World/zoom constants, renamed to generic canvas constants. |
| 607–612 | `457448eb1948af502aa5cec3ea174afed5bb9570ecac39cb22a8ba26f6dfe64b` | Viewport state shape; studio stores world-relative left/top. |
| 619–625 | `490bdb7f8bd2739c7f460a927270c98bf2d76a3a77b68a9944d2ba8742c9ed10` | Pointer pan state shape. |
| 649–659 | `880bc8f40c3fc56823fdce9fc0ad56a014821627b4e36a86b02a472f41c962f7` | Canvas/zoom/anchor refs; renamed and reduced. |
| 706–722 | `657b639531362fd529ad2081a94db575f8fab5e2af1aa72ecee70fdca6f733e2` | Viewport rectangle culling; variable node dimensions and smaller overscan. |
| 726–726 | `076dbbdb3f8f4dc900779b65ecea4d0c20be4fab8e90b99bf3c406b4fef59470` | Keep selected nodes mounted. |
| 792–808 | `bf52df7080c89168253e47ea3856fb06d2f02ccfedf75c49ea3d24004408dbad` | Initialize scroll position and apply pending zoom anchors in layout effect. |
| 818–824 | `00d134d0af8fb40da47720500ca16f9551ad07c156b9bf5cdb00506d237e5b8d` | Non-passive wheel listener registration and cleanup. |
| 2637–2667 | `5446955e1b7a872e3ebed03d7e1ef9e1a1d5b25af2a1028a8015dda6b7145783` | Fit-all bounds and zoom; variable dimensions, direct anchor/layout update. |
| 2707–2765 | `1ffcf5ebea4bb081f57227085d7e8a57b4b633e9c3e2fd65383db56f7d4e5a10` | Pointer-anchored zoom and wheel normalization; same explicit 0.1–1.8 limits. |
| 2793–2835 | `6c109a4626c17198b6177cda23d10fc11b27de44a96af89e1033cf19b6c49172` | Pan begin/move and viewport conversion; pointer lifecycle made local. |
| 4149–4155 | `c6e437e18cc7c1555999453405df2948bca4379e34db8493eba9d902e65ad505` | Pointer move/up/cancel/capture and middle-click event wiring. |
| 4161–4161 | `153be9470acc65f494c313b21beb54761e24cb9bcd0182f72b70cc1e71e2a65c` | Viewport update on scroll. |
| 4195–4207 | `de0f8fa8976669dc7ba699be3a63a4bb65032f37f73660c6afab9d2806f73f65` | Scaled extent and world wrapper geometry. |
| 4225–4242 | `55270747fca2f6975bb2adbff98b8b5b2865c1d79c1ee3ba8b45b97e504767cc` | SVG connections, arrow marker and cubic bend. |
| 4256–4264 | `f5993f52f65295ebe4512ad24ec4491f22fd2f87e5ac8bb659e0d5ba82401f55` | Cubic wire path/marker rendering; domain summaries omitted. |
| 4307–4309 | `7d55e3c9b93ad1491a9c8575952c38f5c88320aeefdd275e0aa9174ffea21788` | Render only visible nodes. |
| 4348–4355 | `df76c37d881e43fcc0f041aff80226bf30c44f273c2054241a6a046cc64160bb` | Node frame/focus/selection attributes adapted to read-only button semantics. |
| 4362–4366 | `ad5bc85b25412ed19355ac8b2459c91b34e898dd28c3dc20a28b3ce53093c3b6` | Absolute node positioning using world padding; caller dimensions. |
| 4371–4381 | `219133c707fed888e9eb6c8f2a6a10ecc4c5be3ad66e5c80130d3f185226aa07` | Pointer focus behavior, stripped of graph mutation. |
| 4413–4415 | `5bdd0e545250befb54dfa36ffc67a57b8a012c8e46779c4ccf44e81887daacf7` | Node label/header composition. |

## `src/editor/deltaHistory.ts`

Whole-file SHA-256: `49bb7f25f838e09c09f6bea5b863a5551e0c9de68c19df2128dc11ec3de64c26`.

| Exact retained/adapted source lines | Range SHA-256 | Retained behavior |
| --- | --- | --- |
| 1–813 | `49bb7f25f838e09c09f6bea5b863a5551e0c9de68c19df2128dc11ec3de64c26` | Generic delta/history module copied verbatim after two source/grant header lines. Existing generic domain/ids import retained; no RPG types. |

## `src/editor/editorState.ts`

Whole-file SHA-256: `3d57fc6c2dc8ae144a012d21b65e722f79cf1bafdf7253558f24d0dfa4d18c38`.

| Exact retained/adapted source lines | Range SHA-256 | Retained behavior |
| --- | --- | --- |
| 3552–3559 | `fc91f044591ed1a381bc04a4ffefc50441c3f7b11d311c11bbe4d8f911b24818` | Field transaction label from aria-label, visible label or placeholder. |
| 3561–3564 | `2be8e5cc56e4500e25bdc2d8b9a014c6d4f4e5a1dde7f77aa38b807a41bd9bcc` | Form-control identification; caller surface scopes listeners. |
| 3567–3582 | `fa43a01987d1fe65d7d076d9018ea6d23d7c1dc8b7215d564b72327b021acd70` | Editable text-control inclusion/exclusion, with readonly/disabled guards; contentEditable omitted. |
| 3616–3634 | `d83154a83f19deb55b60ee8a8684f681b84a7f86ead8efe89cc55ad81a8e218e` | Focus/blur field transaction grouping adapted to local React capture props; no project reducer/global listeners. |

Milestone four composes these existing licensed widgets. Live-control,
transport/schema/PC models and feature SCSS are newly authored; no further RPG
Creator source was extracted. Dock panel registration and v2 layout defaults are
new Studio adaptations of the existing licensed shell.

Milestone five adds newly authored data-only package, delta and virtual-disc
code under `xem-disc::mods`, Studio package commands/CLI, and synthetic parity
fixtures. It extracts no further RPG Creator code and adds no external source
copy. The authored Form1 integrity implementation cross-references the pinned
`chd` 0.3.4 sector layout and libchdr's public CD-ROM parity description; its
lookup tables are generated from the documented polynomials, not copied.
Packages built from user-owned disc files and all real-disc acceptance inputs
and outputs remain ignored local artifacts; they are never public source.

## Milestone-six authored additions

The field-draft backend/transport and inline event authoring controls are newly
authored for this repository. They compose the already registered widget and
canvas interfaces listed above; no additional RPG Creator source is extracted
and the bounded MIT grant is not extended. Authoring templates are generated at
runtime only from `xem-studio/data/field-opcodes.json`, whose public analysis-fact
provenance and deterministic generator remain the milestone-two source of truth.
Project drafts, package outputs and real-disc verification artifacts contain
private source-derived data and remain local, outside the source allowlist.
