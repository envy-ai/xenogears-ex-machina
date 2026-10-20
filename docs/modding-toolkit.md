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

  Its interface is one service: `Studio::command(json) -> json`. It has typed
  operations, explicit errors, and nothing that exists only in a UI. This is
  the pattern of `xem_core::control::Session`. The web frontend calls it through
  wasm-bindgen, in NW.js and the browser alike. A Slint frontend could link the
  crate natively and call the same operations, leaving the backend as it is.
- **Frontend: `tools/studio/`.** Plain JavaScript ES modules, HTML and CSS,
  with no build step, as `src/runtime/web/` is written. Editing a frontend file
  needs only a reload. `tsc --checkJs` over JSDoc types is a lint, not a build.
  Only backend changes need a compile, a wasm-bindgen build in the runtime shell.
- **The decomp mandate holds.** Nothing in `src/decomp/` changes. Runtime
  additions (mod packages, any new control commands) go in `src/runtime/` and
  `src/port/`.

```
frontend  tools/studio/        JS modules: node editor, panels, viewport
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
  - Up to 32 actors per map, each with 32 event entry PCs, run each frame by
    `field_event.c`.
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
   `tools/studio` opens in NW.js and as a page served by `serve.mjs`, loads the
   backend and reports its version and adapters. Add new authored files to
   `packaging/source-files.txt` and check them with
   `tools/repository/source_archive.py --check`.
2. **Field script round trip.** Export `events.py`'s tables once to data the
   crate reads, so there is still one source of opcode facts. Build the Rust
   decoder, the flow graph and the encoder.
   *Accept:* for every map of both discs, the Rust decoder agrees with the
   Python `--sweep`, and decoding to a graph and encoding back reproduces the
   event component byte for byte. Synthetic cases run in `cargo test`; the
   sweep over the real discs is a local test.
3. **Read-only node view.** Mockup first. Then flow graphs and actor lanes for
   any map, with the decoder's mnemonics and operands, and the name table.
   Choose the node-graph library then; one usable without a build step is
   preferred.
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
7. **Map editor.** A three.js viewport for the field bundle: collision, trigger
   zones (palettes, images, triggers and the view block are still undecoded, per
   later-phases.md's data formats table, so decode those first), actor
   placement and entrances.
8. **Presentation replacements.** Texture packs and 3D models in place of
   sprites (first: Fei's field sprite as a glTF model in Lahan) in the render
   layer, presentation only, so RAM and timing stay unchanged.
9. **Other machines and Lua.** Battle AI, battle events and world map scripts
   get the same graph treatment, using their decoders. Lua extensions follow
   plan.md Phase 11.

## Verification

- **Backend:** `cargo test` for `xem-studio`, including round trips of synthetic
  scripts. The sweeps over the real discs run locally only.
- **Frontend:** `node --test` for logic that runs without a page, and Playwright
  with the runtime shell's Chromium against the served page, inside the
  user-scope memory limit from the user's AGENTS.md.
- **NW.js:** its bundled ChromeDriver drives the same UI tests.
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
- **NW.js.** It is not in the pinned Nix shells. Pin the release whose Chromium
  is nearest the runtime shell's Chromium 152, and add it to `nix/authoring` or
  a new `nix/studio`. The browser preview inside NW.js depends on its Chromium
  enabling WebGPU on Linux; the native game does not.
- **Upstream fit.** plan.md's Phase 12 specifies Slint editors. The backend's
  service interface is what keeps that option open: a Slint frontend replaces
  `tools/studio` and calls the same `Studio::command`.
