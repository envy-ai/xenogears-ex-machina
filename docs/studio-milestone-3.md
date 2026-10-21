# Studio milestone 3: read-only event views

The third milestone of [modding-toolkit.md](modding-toolkit.md), authorized by
the user on 2026-10-20. The studio opens the user's own disc images, lists the
field maps, and shows any map's event scripts three ways: an RPG Maker-style
command list, a flow graph, and actor lanes. Readable names come from a name
table. Nothing edits bytecode; editing is milestone 6. Milestone 2's
`field.decode`/`field.encode` and its graph contract are the foundation and are
not changed incompatibly. The working rules of
[2026-10-20-sol-briefing.md](2026-10-20-sol-briefing.md) still apply: the
worktree and branch, push only to the fork after review, licensing, never
committing game data, the network rules and evidence reporting.

## Result

### Backend (`xem-studio`)

- **Disc sessions.** The studio opens a disc image (CHD or bin/cue) through
  `xem-disc`, as the game's hosts do:
  - **Natively:** a path is opened directly.
  - **In wasm:** the session pulls byte ranges, following `xem-web`'s
    `open_disc`/`disc_poll`/`disc_fill`/`disc_missing` pattern. The frontend
    fills the ranges from the user's `File`, or through Node fs in NW.js.
  - **Transport.** Range fills are binary (`Uint8Array`) through one extra
    wasm-bindgen method beside `command`, never JSON number arrays. Everything
    else stays `Studio::command`.
  - **Identity.** A session identifies the disc (`xem_disc::identify`) and
    refuses an unknown image with an explicit error.
- **Map list.** `disc.maps` lists the map bundles: directory (4, 0) file
  `0xb8 + 2n`, through `xem_disc::index::Index`, the rule of `events.py`
  `MAP_FILE`. Placeholder slots are reported as placeholders, not skipped.
- **LZSS.** `packed::decode_block` ported to Rust with the same contract as
  `tools/analysis/packed.py`:
  - complete flag groups and overlapping copies;
  - the speculative final flag read;
  - explicit errors for invalid distances, copies crossing the output length,
    and truncation;
  - no invented padding.
- **Event component.** `field.map_events` returns a map's event component
  exactly as `events.py` `map_events` derives it (the whole packed stream, with
  `component_end`), ready for milestone 2's `field.decode`. The bundle bytes are
  read as the existing extraction workflow reads them, so the local sweep can
  compare them against the Python oracle byte for byte.
- **Structure.** `field.structure` folds one event's flow graph into a tree for
  the command list:
  - sequences;
  - `if`/`else` from branches with a single join;
  - loops from back edges (a wait-until-flag loop is the common case);
  - inline jump tables as a switch.

  Anything else stays as explicit labels and `goto` nodes. Every reachable
  instruction appears exactly once, and the tree's flattening reproduces the
  graph's edges, so the tree is a lossless view, not a decompiler's guess.
- **Lanes.** `field.lanes` derives cross-actor links: `request_event_started`
  and `request_event_finished` (`08`/`09`, with their actor and event operands
  resolved, including the party and self selectors as the decoder reports them),
  plus each event's variable reads and writes from the opcode metadata's operand
  descriptors. Unresolvable operands (variables in place of immediates) are
  reported as such.
- **Names.** `names.validate` checks a name table: variables, actors per map,
  events per actor, messages. It rejects unknown keys, duplicate names within a
  scope and out-of-range ids. The frontend stores the table as
  `names.json` in the opened project folder through the files adapter, and the
  backend only validates it.

### Frontend (`tools/studio`)

- **Mockups first.** Mockups of the three views and the map browser, for the
  user's approval before any of this UI is built (step 1).
- **Disc panel.** Open disc 1 and/or disc 2 (browser: file picker; NW.js: a
  path). It shows identity and progress, and errors persist with their message.
- **Map browser.** A list of maps per disc (number, actor count, script size;
  placeholders marked), filterable, keyboard navigable. Selecting a map loads
  its events.
- **Actor and event tree.** The map's actors with their 32 entry points,
  labelled by trigger convention (0 load, 1 idle, 2 talk, 3 touch, others by
  number) and by name-table names.
- **Command list (main view).** RPG Maker-style indented rows from
  `field.structure`:
  - each row is the mnemonic rendered as a phrase, with operands formatted by
    their descriptors and names substituted from the name table;
  - blocks fold, and raw bytes and the PC show on demand;
  - diagnostics (overlaps, unpaired steps, outside successors, pinned blocks)
    are marked inline, never hidden;
  - it is read-only, and selection and copy work.
- **Flow graph (alternate view).** The event's blocks and edges on a graph
  canvas extracted from rpg_creator's `EventGraphEditor` (pan, zoom, wires,
  selection, keyboard focus). It uses read-only mode, an automatic layered
  layout, and an edge style per kind and status. Selecting a node selects the
  same instruction in the command list and the reverse.
- **Actor lanes.** One lane per actor, with each event as a span and links for
  the start/finish requests and shared-variable dependencies. Selecting a link
  opens the source instruction.
- **Name editing.** Renaming a variable, actor, event or message writes
  `names.json`, with undo through the copied history-aware fields. This is the
  only thing milestone 3 writes, and it never touches game data.
- **Shortcuts.**
  - `Ctrl+1/2/3` switch between the list, graph and lanes views;
  - `F2` renames;
  - `Ctrl+F` finds within the map;
  - `Alt+←/→` navigate back and forward through jumps followed;
  - `Enter` follows a jump or call target.

## Acceptance criteria

1. **Rust unit tests:**
   - LZSS on synthetic streams, covering each documented edge and error;
   - the map list and component extraction over a synthetic index and bundle;
   - structure folding on synthetic graphs: if/else, nested if, loop, switch,
     irreducible flow falling back to `goto`;
   - lanes;
   - name-table validation errors.
2. **Local sweep** (gated by `XEM_STUDIO_EXTRACT_ROOT`, as in milestone 2,
   extended):
   - every map's Rust-extracted event component equals the Python oracle's
     component byte for byte, for all 935 real maps;
   - placeholders match;
   - for every event of every map, the structure tree covers each reachable
     instruction exactly once and its flattening reproduces the graph's edges;
   - per-disc counts of folded if/else blocks, loops, switches and `goto`
     fallbacks are reported (no target ratio; reported for review).
3. **Wasm parity.** A synthetic disc fixture opened through the wasm session
   (range pulls and binary fills) gives the same map list, component and
   structure as native.
4. **Playwright with synthetic data only** (no game data in tests): a synthetic
   component fixture is loaded through a test-only entry:
   - all three views render;
   - selection syncs between the list and the graph;
   - folding works;
   - names apply after a rename and persist after a reload;
   - diagnostics are visible;
   - the keyboard shortcuts work.
5. **Local real-disc UI check** (gated, as the sweep): open both discs in the
   browser and in NW.js, open a large map (disc 1 map 2, about 3,400
   instructions) and record time to first render for each view.
   - Target under one second for the list and lanes, and under two seconds for
     the graph. Report the measured values.
   - The graph view virtualizes or culls off-screen nodes if needed to meet
     this.
6. **NW.js smoke test** extended: open a synthetic disc fixture by path and
   list its maps.
7. **Repository checks:**
   - typecheck, design-system audit, Vitest, `cargo fmt`/Clippy, the
     metadata check;
   - `source_archive.py --check` with new files listed;
   - provenance rows for code extracted from `EventGraphEditor.tsx`;
   - no `src/decomp/` changes.

## Steps

1. Mockups of the disc panel and map browser, the command list, the flow graph
   and the actor lanes. Get the user's approval. Register new widgets in
   `tools/studio/docs/widgets.md` as they are built.
2. Backend: LZSS port and tests; disc session (native and wasm pull and fill);
   `disc.maps`; `field.map_events`; extend the local sweep to compare
   components.
3. Backend: `field.structure`, `field.lanes`, `names.validate`, and the sweep's
   structure checks.
4. Frontend: disc panel and map browser, then the actor and event tree and
   command list, then the graph canvas extraction and flow graph, then lanes,
   then names.
5. Tests, the real-disc check and docs (`tools/studio/README.md`, the protocol
   section for new operations, and modding-toolkit.md if decisions change).

Commit at each step that passes its checks. Push to the fork only, after review.

## Extracting the graph canvas

`EventGraphEditor.tsx` (rpg_creator `1f690d0`, about 5,300 lines) is a single
component that combines the canvas with rpg_creator's node types, palette,
inspector, history and project schema. Extract only the general-purpose
canvas into a studio `GraphCanvas`:
- the world and viewport transform, pan and zoom limits;
- node frames and ports, and wire routing;
- selection, keyboard focus and the canvas's own shortcuts.

The studio supplies its node rendering and layout. Record the extraction in
`PROVENANCE.md`, with a source line range per extracted part. Leave behind the
palette, inspectors, draft recovery and history: milestone 3 is read-only, and
milestone 6 decides editing.

## Risks

- **Structure quality.** Original scripts may fold poorly in places. `goto`
  fallbacks are correct, just less readable; the sweep's counts show where.
  Don't force structure by duplicating or reordering instructions.
- **Graph performance** on the largest maps. Measure first, then cull or
  virtualize.
- **Message text.** Milestone 3 shows message ids (`msg0000`), not text. The
  text decoder (`text_control.py`) still has open glyph items. Showing dialogue
  text is a strong candidate for the next milestone, but not this one.
- **Operand phrasing.** Rendering mnemonics as readable phrases is presentation
  only. The underlying mnemonic and raw operands stay one click away and are
  what tests compare.
