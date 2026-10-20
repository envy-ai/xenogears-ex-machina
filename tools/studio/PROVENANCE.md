# Provenance of code taken from Saga Foundry RPG Creator

Files under `tools/studio/` that started as copies of Saga Foundry RPG Creator
(`~/rpg_creator`, licensed AGPL-3.0-only) are listed below, with their source
path and revision. Each also names them in a header comment.

## License grant

The author of Saga Foundry RPG Creator (envy-ai, the owner of this fork)
granted the MIT license for the files listed below, for use in this
repository, in the Claude Code session of 2026-10-20 ("MIT is granted"). Those
code files are therefore under this repository's MIT license (`LICENSE`).
The fonts and their notices retain OFL 1.1, as stated below.

## Copied files

Source revision: rpg_creator `1f690d0263af8881a2d33ee374a2e5006355748b`.

| Here | From | Changes |
| --- | --- | --- |
| `docs/widgets.md` | `docs/editor-widget-library.md` | Adapted the copied widget/class contracts to studio scope and documented centerless/layout-only/generic-owner changes. |
| `public/fonts/inconsolata/Inconsolata-Variable.ttf` | `public/fonts/inconsolata/Inconsolata-Variable.ttf` | Unchanged OFL 1.1 binary; source/revision in public/fonts/README.md (binary header exception). |
| `public/fonts/inconsolata/OFL.txt` | `public/fonts/inconsolata/OFL.txt` | OFL 1.1 text retained with source/revision header; removed upstream trailing whitespace. |
| `public/fonts/inter/Inter-Variable.ttf` | `public/fonts/inter/Inter-Variable.ttf` | Unchanged OFL 1.1 binary; source/revision in public/fonts/README.md (binary header exception). |
| `public/fonts/inter/OFL.txt` | `public/fonts/inter/OFL.txt` | OFL 1.1 text retained with source/revision header; removed upstream trailing whitespace. |
| `scripts/audit-design-system.mjs` | `scripts/audit_editor_design_system.mjs` | Adapted shared tag scanner and ownership rules to studio; feature Sass appearance audit compiles in memory; removed RPG/domain requirements. |
| `src/domain/ids.ts` | `src/domain/ids.ts` | Unchanged apart from source/revision header. |
| `src/editor/ContextMenu.tsx` | `src/editor/ContextMenu.tsx` | Guard type redirected to generic EditorOperationOwners; other behavior unchanged. |
| `src/editor/DockableWorkspace.tsx` | `src/editor/DockableWorkspace.tsx` | Studio panel labels/types; optional centerless layout with redundant separator omitted; behavior otherwise preserved. |
| `src/editor/DockerList.tsx` | `src/editor/DockerList.tsx` | Unchanged apart from source/revision header. |
| `src/editor/EditorDialogShell.tsx` | `src/editor/EditorDialogShell.tsx` | Unchanged apart from source/revision header. |
| `src/editor/EditorOperationOwners.tsx` | `src/editor/EditorOperationOwners.tsx` | Generic schema/operation/guard types; removed assistant, runtime image and domain dependencies; owner leases retained. |
| `src/editor/EditorPreferenceStore.tsx` | `src/editor/EditorPreferenceStore.tsx` | Replaced project/domain preference aggregation with synchronous studio layout-only Storage scope; failures propagate. |
| `src/editor/EditorRenderErrorBoundary.tsx` | `src/editor/EditorRenderErrorBoundary.tsx` | Unchanged apart from source/revision header. |
| `src/editor/NativeWindowControls.tsx` | `src/editor/NativeWindowControls.tsx` | Geometry import extracted; native bridge exceptions now propagate. |
| `src/editor/NumberField.tsx` | `src/editor/NumberField.tsx` | Unchanged apart from source/revision header. |
| `src/editor/RecordSelect.tsx` | `src/editor/RecordSelect.tsx` | Unchanged apart from source/revision header. |
| `src/editor/dockLayout.ts` | `src/editor/dockLayout.ts` | Unchanged apart from source/revision header. |
| `src/editor/editorCommands.ts` | `src/editor/editorCommands.ts` | Unchanged apart from source/revision header. |
| `src/editor/editorTheme.ts` | `src/editor/editorTheme.ts` | Unchanged apart from source/revision header. |
| `src/editor/itemReorder.tsx` | `src/editor/itemReorder.tsx` | Unchanged apart from source/revision header. |
| `src/editor/storedJson.ts` | `src/editor/storedJson.ts` | Unchanged apart from source/revision header. |
| `src/editor/usePersistedPreferences.ts` | `src/editor/usePersistedPreferences.ts` | Unchanged apart from source/revision header. |
| `src/editor/workspaceLayout.ts` | `src/editor/workspaceLayout.ts` | Studio project/backend/log IDs, v1 schema/key and approved default; removed RPG v3 migration and game-specific float width. |
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


The font binaries and their OFL 1.1 notices retain their own license;
the MIT code grant above does not relicense the fonts. Their source/revision
for binaries is recorded in `public/fonts/README.md`; OFL.txt files carry
attribution headers above the license text, with upstream trailing whitespace removed. `src/styles/editor.scss` is newly authored composition/layout glue.
