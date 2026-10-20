# Studio milestone 1: skeleton

The first milestone of [modding-toolkit.md](modding-toolkit.md): the backend
crate and its wasm build, and the frontend shell with the rpg_creator widget
layers, opening in NW.js and as a browser page. No Xenogears data is edited yet;
milestone 2 (the field script round trip) builds on this.

## Result

- `src/runtime/crates/xem-studio`: a Rust crate with
  `Studio::command(&str) -> String` (JSON in, JSON out) and a wasm-bindgen
  export of it. Its first operations are `version` (crate version and the
  operation list) and `echo` (returns its argument). Every frontend test goes
  through `echo`. Unknown operations and malformed JSON return a structured
  error `{error: {kind, message}}`; nothing panics across the boundary.
- `tools/studio/`: a Vite + React + TypeScript app. Its shell is a dockable
  workspace with three panels:
  - **Project:** the opened folder's files, through the files adapter.
  - **Backend:** version and operations from the backend, plus a command box
    that sends JSON and shows the reply.
  - **Log:** every command and error, kept in the log; nothing is dropped
    silently.

  It has a menu bar, native window controls under NW.js, and an About dialog.
- **Two shells, one bundle:**
  - **Browser:** `npm run dev` serves on `0.0.0.0` with HTTPS (the
    `.local/serve` certificate, since the File System Access API needs a secure
    context away from localhost; never HSTS).
  - **NW.js:** `npm run desktop` opens NW.js on the dev server, or on `dist/`
    after `npm run build`.
- **Platform adapters:** `files`, with a Node fs implementation (NW.js) and a
  File System Access API implementation (browser), behind one interface. The
  game link adapter is milestone 4 and is not stubbed here.

## Acceptance criteria

1. `cargo test -p xem-studio` passes:
   - `version` lists the operations;
   - `echo` round-trips JSON;
   - unknown operations, malformed JSON and wrong argument types each return
     their error kind.
2. `npm run typecheck`, `npm test` (Vitest) and the design-system audit pass in
   `tools/studio`. The Node files adapter is tested against a temporary folder:
   list, read, atomic write and a missing file's error.
3. **Playwright against the served page:**
   - the shell renders with all three panels;
   - the Backend panel shows the wasm backend's version;
   - a JSON command sent from the command box comes back in the panel and the
     log;
   - a malformed command shows a persistent error with its message;
   - panels can be moved and closed, and the layout survives a reload;
   - the menus and dialogs work by keyboard (open, arrow keys, Escape).
4. **NW.js smoke test:** NW.js launched on the built app with a test flag loads
   the backend, lists a fixture folder through the Node adapter, writes the
   result to a file and quits; the test checks the file. Optionally also
   through the SDK flavour's ChromeDriver, if it is pinned.
5. `tools/repository/source_archive.py --check` passes with every new authored
   file listed in `packaging/source-files.txt`, and `PROVENANCE.md` lists every
   copied file.
6. Nothing under `src/decomp/` changes, and the existing runtime crates build
   unchanged.

## Steps

1. **Mockup.** Before the UI: a mockup of the shell (menu bar, the three
   docked panels, window controls) in the style of rpg_creator's editor, for the
   user's approval.
2. **Backend crate.**
   - Add `xem-studio` to the workspace (`members = ["crates/*"]` picks it up),
     with `crate-type = ["cdylib", "rlib"]` and dependencies `serde`,
     `serde_json`, and `wasm-bindgen` on wasm32 only (workspace pins, as
     `xem-web` does).
   - The command layer is an enum of operations deserialized from
     `{op, args}`, each returning a typed result or a `StudioError` with a kind
     and message.
   - `tools/studio/build-backend.py`, after `src/runtime/web/build.py`:
     `cargo build --locked -p xem-studio --target wasm32-unknown-unknown`
     (`--release` on request), then `wasm-bindgen --target web` into
     `tools/studio/src/backend/pkg/`, which is ignored by Git. It runs in
     `nix develop path:./nix/runtime`.
3. **Frontend scaffold.**
   - `tools/studio/package.json` takes rpg_creator's pins: React 19.2.7,
     TypeScript 7.0.2, Vite 8.1.5, @vitejs/plugin-react 6.0.3, Sass, Vitest
     4.1.10, Playwright 1.61.1, nw 0.113.0, lucide-react 1.25.0 and zod 4.4.3.
     Its lock file is committed.
   - Node comes from the Nix shell (Node 24.19.0, as `tools/authoring`).
   - The `nw` npm package downloads the NW.js binary at install. Record its
     version and checksum.
4. **Copy the widget layers** (rpg_creator `1f690d0`, each with a header
   comment and a `PROVENANCE.md` row). Their import closure today pulls in about
   27,000 lines of rpg_creator's domain code through four links. Each link is
   cut, not copied:

   | Widget files copied | Link cut | Replacement |
   | --- | --- | --- |
   | `DockableWorkspace.tsx`, `dockLayout.ts`, `workspaceLayout.ts` | `EditorPreferenceStore`, which aggregates every rpg_creator feature's preferences | a studio preference store with only the workspace layout, behind the same hook (`usePersistedPreferences`) |
   | `ContextMenu.tsx`, `EditorDialogShell.tsx` | `EditorOperationOwners`, which is typed on rpg_creator's assistant workflow | the owner registry, kept generic (an owner's operations typed by the studio) |
   | `NativeWindowControls.tsx` | `runtime/nativeGameWindow.ts`, `nativeWindowState.ts` | copied (they depend only on each other), with game-window specifics removed |
   | `dockLayout.ts` | `domain/ids` | copied (39 lines) |

   Copied as they are: `DockerList.tsx`, `itemReorder.tsx`, `NumberField.tsx`,
   `RecordSelect.tsx`, `editorCommands.ts`, `storedJson.ts`, `editorTheme.ts`,
   `EditorRenderErrorBoundary.tsx`. Styles: the SCSS theme files
   (`_editor-*-theme.scss`, `editor-buttons`, `editor-controls`,
   `editor-surfaces`, `editor-typography`) and the copied widgets' rules from
   rpg_creator's `styles.css`. That file is 15,141 lines; take only the rules
   for the copied classes, found by class name. Fonts: Inter and Inconsolata,
   which are OFL-licensed, with their license files.
5. **Widget registry.** `tools/studio/docs/widgets.md`, started from
   rpg_creator's `editor-widget-library.md` entries for the copied widgets, and
   a design-system audit script adapted to the studio's files.
6. **Shell, adapters and backend loading**, as in Result.
7. **Tests**, as in the acceptance criteria. Browser runs use the user-scope
   systemd memory limit from the user's AGENTS.md, and Playwright scripts live
   in `tools/studio/tests/e2e`.
8. **Docs.** A `tools/studio/README.md` (dev, desktop, build and test
   commands; the backend build), and updates to modding-toolkit.md where
   decisions changed.

Commit at each step that passes its checks. Push to the fork only.

## Risks

- **NW.js and Chromium.** NW.js 0.113.0 is the version rpg_creator runs. The
  studio needs no WebGPU in milestone 1; Vulkan and WebGPU flags matter only for
  the browser preview later.
- **Nix purity.** The NW.js binary comes from npm's download, not a Nix
  derivation. That is acceptable for a fork-local tool. If it causes trouble,
  move it into a pinned Nix shell.
- **Widget cuts.** If one of the four cuts changes a widget's behaviour (for
  example, layout persistence relies on rpg_creator's store semantics), copy
  that store contract instead of reimplementing it, and note it in
  PROVENANCE.md.
- **HTTPS dev server and LAN.** Browsers outside this machine must accept the
  self-signed certificate once, as with the game page.
