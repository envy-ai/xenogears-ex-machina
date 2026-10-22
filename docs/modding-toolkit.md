# Modding toolkit (fork plan)

The plan for the `modding-toolkit` branch of envy-ai/xenogears-ex-machina: a
modding studio with a Rust backend and a web frontend. The frontend runs in
NW.js beside the native game and as a page beside the browser build.
[plan.md](../plan.md) stays authoritative for the project; this file owns only
the fork's toolkit work and says where it meets plan.md's phases. No upstream
pull requests are planned from this branch.

## Backend and frontend

- **Backend: `xem-studio`.** A Rust crate in the runtime workspace
  (`src/runtime/crates/xem-studio`). It holds everything that decides anything:
  - decoders and encoders;
  - the conversion between bytecode and node graphs;
  - validators;
  - the mod package model and the delta builder.

  Its interface is one service: `Studio::command(json) -> json`, returning
  `{ok: result}` or `{error: {kind, message}}`. It has typed
  operations, explicit errors, and nothing that exists only in a UI. This is
  the pattern of `xem_core::control::Session`. The web frontend calls it through
  wasm-bindgen, in NW.js and the browser alike. A Slint frontend could link the
  crate natively and call the same operations, leaving the backend as it is.
- **Frontend: `tools/studio/`.** React, TypeScript and Vite, the stack of
  Saga Foundry RPG Creator (`~/rpg_creator`), whose RPG Maker-patterned editor
  UI it takes widgets from. In development the Vite dev server reloads on save,
  in the browser and in NW.js pointed at it, so frontend work needs no manual
  compile. Only backend changes need a compile, a wasm-bindgen build in the
  runtime shell.
- **Taken from rpg_creator.** rpg_creator is AGPL-3.0 and this repository is
  MIT. Nothing is copied until the user, as rpg_creator's author, has added the
  MIT grant for the copied files to `tools/studio/PROVENANCE.md`. That file also
  lists each copied file's source path and rpg_creator revision, and each copied
  code file names them in a header comment. Font binaries and verbatim OFL
  notices keep attribution in the adjacent font README and PROVENANCE instead.
  What comes over is the general layers
  only:
  - the visual primitives (the editor SCSS themes);
  - the behavioural widgets: dockable workspace, docker lists, context menus,
    modal and prompt/choice/confirm dialogs, tabs, toolbars, native window
    controls, and form fields with history (`NumberField`, `RecordSelect`);
  - the event graph editor's canvas: pan, zoom, wiring, selection, entry nodes
    and condition nodes with True/False outputs.

  rpg_creator's editor state, project schema and node types stay behind; the
  studio's node types come from the backend. Its widget registry
  (`docs/editor-widget-library.md`) is the starting point for the studio's own
  registry. If both projects keep improving the same widgets, extract a shared
  package later instead of copying again.
- **The decomp mandate holds.** Nothing in `src/decomp/` changes. Runtime
  additions (mod packages, any new control commands) go in `src/runtime/` and
  `src/port/`.

```
frontend  tools/studio/        React + TS (Vite): command list, node graph, panels, viewport
            platform/files     Node fs (NW.js) | File System Access API (browser)
            platform/game      xem-desktop --control over pipes (NW.js)
                               | the game page's window.xem (browser)
            nw/  web/          the two entries (NW.js manifest, static page)
backend   xem-studio (wasm)    Studio::command: scripts, graphs, packages, validation
runtime   xem-* crates         the game; mod package loading in shared host code
```

## What exists to build on

- **One game command layer.** `xem_core::control::Session` takes a JSON command
  and returns a JSON result, the same on every host ([runtime.md, Commands,
  introspection and snapshots](runtime.md#commands-introspection-and-snapshots)):
  `status`, `frames`, `run_until`, `pad`/`press`/`act`, `inspect`/`write` by
  typed path through `build/game/schema.json`, `globals`,
  `read_memory`/`write_memory`, `snapshot`/`restore`, `memory_hash`,
  `record`/`replay`, `scenario`, `field`, `context`.
- **Native.** `xem-desktop --control` answers those commands on stdin, one line
  each, while the game plays in its window (`crates/xem-desktop/tests/control.rs`
  drives it this way).
- **Browser.** The page exposes the same commands as `window.xem`
  (`src/runtime/web/xem.js`: `command`, `run`, `runFrames`), fetching disc
  ranges a frame waits for.
- **Decoders.** `tools/analysis/` has a decoder for every bytecode machine.
  Each one's `--sweep` decodes every shipped script on both discs with nothing
  unknown ([scripts/interpreters.md](scripts/interpreters.md)). Alongside them
  are the field bundle (`field.py`), LZSS (`packed.py`, and
  `tools/packed_container.py`, which re-packs every container exactly) and the
  disc index (`tools/extraction/disc_files.py`: sectors 24-38, 7-byte records).
  There is no assembler or encoder yet
  ([later-phases.md, Script machines](later-phases.md#script-machines)).
- **Field events** ([scripts/field-events.md](scripts/field-events.md)):
  - Actors have 32 event entry PCs each and are scheduled each frame by
    `field_event.c`. Do not confuse event slots with an actor-count limit:
    shipped scripts include actor 56 (see scripts/field-events.md). Validate
    actor counts against the decoded table and demonstrated runtime limits;
    no unverified global maximum is assumed.
  - 256 primary and 227 extended opcodes, tabled in `tools/analysis/events.py`
    (`_PRIMARY`/`_EXTENDED`: mnemonic, handler, size, flow, operands, effect)
    and tested against the C.

## Node graphs of field scripts

- **Flow graph per event.** Basic blocks from each instruction's size and flow
  become nodes, and jumps and branches become edges. This is exact for every
  script. Recognised shapes fold into higher-level nodes: if/else, a loop that
  waits on a flag, a camera move sequence. Anything else stays as plain edges.
- **Actor lanes.** Each map's actors run concurrently, so a cutscene view shows
  one lane per actor. Links show `request_event_started` and
  `request_event_finished` (`08`, `09`) and the shared variables that actors
  wait on.
- **Back to bytecode.**
  - Every node keeps the byte span it came from.
  - Unedited nodes are written back in place, byte for byte.
  - Edited and new nodes are appended, with a jump out from the original spot
    and a jump back, because field PCs are absolute u16 offsets.
- **Units that stay together.** Paired instructions that re-read operands
  behind them (`10`/`11`, `57`) stay one node. Handlers that advance further
  (`9a`, `d4`/`fc`, ext `18`/`c6`) and `a6`'s inline jump table become nodes
  with several exits. Map entry records (bytecode opening with `ff`) and gaps
  become pinned blocks that cannot be edited.
- **Two views of one graph.** The main event view is an RPG Maker-style
  indented command list: folded blocks nest, and an insert-command window offers
  the commands by category, each with its own form. The node graph is the
  alternate view, for complex flow and for actor lanes. Both edit the same
  backend graph. rpg_creator went graph-only and has no list editor, so the list
  view is new work.
- **Pages.** Xenogears has no condition-selected pages. Where an event's head
  branches on flags, the frontend can present the branches as pages; the
  backend reports that shape and the bytecode stays as it is.
- **Names.** The game knows `v0428`, `actor25` and `msg0000`. A name table kept
  with the mod project gives them readable names; the backend stores and
  validates it, and the frontend shows it.
- **Validation.** Reject what the original would read out of range: extended
  `e3`-`ff`, unchecked table indexes ([scripts/interpreters.md](scripts/interpreters.md)).

## Mod packages (runtime)

- **What a package changes.** It replaces disc files by their index in the
  game's own file table and states which disc identities it applies to
  (`xem_disc::identify`).
- **No original bytes in packages.** A package must be shareable, so it carries
  binary deltas against the user's own disc files, plus wholly authored files.
  It never carries original data verbatim. The loader applies deltas to the
  user's image and checks the hash of each source file before and of each
  result after.
- **Where it applies.** A virtual disc layer in shared host code (`xem-disc` and
  `xem-host`) used by every host. A replacement whose size differs is placed past
  the image's last sector, and the file table entries the game reads are patched
  to match. Read the sector format the port serves from `xem-disc` before
  designing this.
- **Load order and conflicts.** Deterministic: a manifest list order with
  declared dependencies. Two packages that replace the same file is a conflict
  error unless one declares that it overrides the other.
- **Saves and states.** Snapshots, saves and replays record the active packages
  (ids and hashes). Restoring one under different packages is refused
  explicitly, per Phase 11.
- **Absent or disabled means vanilla.** With no packages, state matches today's
  byte for byte (`memory_hash` over a fixed replay).

## Milestones

Each one ends usable and tested. UI mockups come before each editor's UI work.

1. **Skeleton.** The `xem-studio` crate with `Studio::command` and a wasm build.
   `tools/studio` is a Vite app with the rpg_creator widget layers copied in
   (with provenance). It opens in NW.js and as a browser page, loads the backend
   and reports its version and adapters. Rust/wasm/Node/Chromium use the pinned
   runtime Nix shell; NW.js, Vite and React use the npm lockfile pins from
   [studio-milestone-1.md](studio-milestone-1.md). Add new authored files to
   `packaging/source-files.txt` and check them with
   `tools/repository/source_archive.py --check`.
2. **Field script round trip.** Export `events.py`'s tables once to data the
   crate reads, so there is still one source of opcode facts. Build the Rust
   decoder, the flow graph and the encoder.
   *Accept:* for every map of both discs, the Rust decoder agrees with the
   Python `--sweep`, and decoding to a graph and encoding back reproduces the
   event component byte for byte. Synthetic cases run in `cargo test`; the
   sweep over the real discs is a local test. The backend protocol, canonical
   graph, public metadata provenance and exact local checks are documented in
   [studio-milestone-2.md](studio-milestone-2.md).
3. **Read-only views.** Mockups first. Then, for any map, the command list, the
   flow graph (on the canvas from rpg_creator) and the actor lanes, with the
   decoder's mnemonics and operands and the name table.
4. **Live attach and inspector.** The game link for both hosts, and panels for:
   - status, frame step and run;
   - snapshot and restore;
   - a `schema.json` tree with `inspect` and `write`;
   - jumping from a running actor's PC to its node.

   *Accept:* one test attaches to `xem-desktop --control` (as `control.rs`
   launches it) and to the browser build (Playwright), runs the same commands,
   and gets equal results after the same replay.
5. **Mod packages in the runtime.** The format, loader, delta builder, virtual
   disc layer, conflicts and save compatibility described above.
   *Accept:* a changed message shows in both hosts, and with packages absent or
   disabled the `memory_hash` matches vanilla.
6. **Editing.** Node edits, new nodes and new events. Recompile by appending,
   validate, package, then play the edit in either host.
   *Accept:* an edited event plays as edited in both hosts, and an untouched map
   in the same package is byte-identical to the original.
7. **Map editor.** A three.js viewport for the field bundle: map geometry,
   actors, collision, trigger zones and entrances. Decode required palettes,
   images, triggers and the view block first; later-phases.md's data formats
   table identifies these current gaps. Unsupported/undecoded content must be
   labelled explicitly, never represented by guessed editable geometry.
   - Select entities directly in the scene and keep viewport selection,
     properties and the corresponding actor/event scripts synchronized.
   - Add, duplicate and delete supported entity kinds directly from the map
     view, validating actor/event references and dependent triggers/entrances
     rather than silently leaving dangling bindings.
   - Move and rotate supported entities using scene controls and precise
     property fields; expose scale or other transforms only where the original
     format/runtime supports them. Show relevant coordinate/range constraints.
   - Distinguish authored placement from a live runtime pose. Script-controlled
     placement must resolve to supported authored data/script bindings or explain
     why it cannot be edited; moving a live actor is not a saved map edit.
   - Apply entity changes transactionally with undo/redo, preserving unrelated
     content and stable identity/reference mappings. Persist through the validated
     package/compiler pipeline from milestones 5 and 6.
   *Accept:* select, add/duplicate, transform and remove supported entities;
   undo/redo restores their properties and script bindings; save/reload preserves
   the authored scene; the packaged changes play correctly in both hosts.
   Unedited content stays byte-identical, invalid references or unsupported edits
   are rejected before mutation, and failures preserve the user's draft/work.
8. **Presentation replacements.** Texture packs and 3D models in place of
   sprites (first: Fei's field sprite as a glTF model in Lahan) in the render
   layer, presentation only, so RAM and timing stay unchanged.
9. **Other machines and Lua.** Battle AI, battle events and world map scripts
   get the same graph treatment, using their decoders. Lua extensions follow
   plan.md Phase 11.

## Verification

- **Backend:** `cargo test` for `xem-studio`, including round trips of synthetic
  scripts. The sweeps over the real discs run locally only.
- **Frontend:** `tsc` typecheck, Vitest for logic that runs without a page, and Playwright
  with the runtime shell's Chromium against the served page, inside the
  user-scope memory limit from the user's AGENTS.md.
- **NW.js:** milestone one uses the pinned normal flavour's fixture-only smoke
  test; an SDK ChromeDriver is not pinned. Browser interactions are covered with
  the runtime shell's Chromium and Playwright.
- **Native link:** under lavapipe and a headless Wayland, as `control.rs` does.
- **Runtime crates:** `src/runtime/scripts/suite.sh desktop` or `web` for
  whichever side changed.

## Risks and open questions

- **Re-encoding.** Appending with jumps out and back keeps layout, but
  instructions that read neighbouring bytes limit where those jumps can go. The
  round trip in milestone 2 is the proof.
- **Different file sizes.** Replacements of a different size depend on how the
  game reads its file table and on sector layout; milestone 5 settles this
  before committing to the format.
- **NW.js.** Milestone one pins `nw@0.113.0` through npm, matching rpg_creator,
  while the pinned runtime Nix shell supplies Node, Rust, wasm-bindgen and
  Chromium. The downloaded Linux archive/checksum is recorded in
  `tools/studio/nw-binary.json`. Milestone one needs no WebGPU. A remote dev page
  uses the approved `https://localhost:5176/*` Node/NW API grant only in its
  temporary development manifest. That manifest trusts the selected local public
  server certificate selected by explicit `XEM_STUDIO_TLS_DIR` through
  `additional_trust_anchors`, retaining TLS/hostname
  checks without OS trust changes. Both fields are absent from the packaged
  manifest. See `tools/studio/README.md` for certificate selection and checks.
- **Upstream fit.** plan.md's Phase 12 specifies Slint editors. The backend's
  service interface is what keeps that option open: a Slint frontend replaces
  `tools/studio` and calls the same `Studio::command`.
