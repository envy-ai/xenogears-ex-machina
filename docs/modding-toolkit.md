# Modding toolkit (fork plan)

The plan for the `modding-toolkit` branch of envy-ai/xenogears-ex-machina: a
modding tool that runs as an NW.js desktop app beside the native game and as a
page beside the browser build, from one code base. [plan.md](../plan.md) stays
authoritative for the project; this file owns only the fork's toolkit work and
says where it meets plan.md's phases.

## Relation to plan.md

- **Phase 11 (mods, authoring bridge).** The runtime parts here, namely mod
  packages with load order, validation and save compatibility, and the
  TypeScript authoring stack, are Phase 11 items. They are built to its
  contract ([authoring/README.md](authoring/README.md): source is authoritative,
  packages are immutable and validated in full, prebuilt play needs no Node or
  browser) and are candidates for upstream pull requests.
- **Phase 12 (editors).** plan.md specifies Slint desktop editors. The NW.js
  tool is this fork's choice of editor client instead. It uses the same services
  (the control layer, the decoders, the package loader), so a Slint client can be
  built on them later without anything existing only in the NW.js UI.
- **Order.** plan.md puts the toolkit after the Phase 10 release, which still
  has open items. Ask the maintainer before sending Phase 11 pull requests.
- **The decomp mandate holds.** Nothing in `src/decomp/` changes. Runtime
  additions go in `src/runtime/` and `src/port/`.

## What exists to build on

- **One command layer.** `xem_core::control::Session` takes a JSON command and
  returns a JSON result, the same on every host ([runtime.md, Commands,
  introspection and snapshots](runtime.md#commands-introspection-and-snapshots)):
  `status`, `frames`, `run_until`, `pad`/`press`/`act`, `inspect`/`write` by
  typed path through `build/game/schema.json`, `globals`,
  `read_memory`/`write_memory`, `snapshot`/`restore`, `memory_hash`,
  `record`/`replay`, `scenario`, `field`, `context`.
- **Native.** `xem-desktop --control` answers those commands on stdin, one line
  each, while the game plays in its window (`crates/xem-desktop/tests/control.rs`
  drives it this way). No new socket is needed when the tool launches the game.
- **Browser.** The page exposes the same commands as `window.xem`
  (`src/runtime/web/xem.js`: `command`, `run`, `runFrames`), fetching disc
  ranges a frame waits for.
- **Decoders.** `tools/analysis/` has nine bytecode decoders whose `--sweep`
  decodes every shipped script on both discs with nothing unknown
  ([scripts/interpreters.md](scripts/interpreters.md)), plus the field bundle
  (`field.py`), LZSS (`packed.py`, and `tools/packed_container.py`, which
  re-packs every container exactly) and the disc index. There is no assembler or
  encoder yet ([later-phases.md, Script machines](later-phases.md#script-machines)).
- **Authoring pins.** `tools/authoring/package.json` already qualifies
  TypeScript 7.0.2, esbuild 0.28.2, three 0.186.0, zod 4.6.2,
  @gltf-transform/core 4.5.0 and manifold-3d 3.5.3 on Node 24.19.0, with a Nix
  shell in `nix/authoring/`.

## Architecture

```
tools/studio/                       one TypeScript code base, one esbuild bundle
  core/        decoders, encoders, validators, mod package model (no DOM, no Node)
  ui/          editors and panels (DOM, three.js viewport)
  platform/
    files      FileStore  : Node fs (NW.js) | File System Access API (browser)
    game       GameLink   : xem-desktop --control over pipes (NW.js)
                          | window.xem of the game page (browser)
  nw/          NW.js manifest and entry (Node available)
  web/         static entry, served beside build/web
```

- **`GameLink`**: `command(json) -> Promise<json>`, `run(json)` (frames across
  disc fetches), `events` (exit, errors). Both implementations send the same
  JSON. In NW.js it spawns `xem-desktop --disc <image> --control` and matches
  each reply line to its request. In the browser the tool and the game page share
  an origin, and the tool calls the game's `window.xem`, either in an iframe it
  embeds or in a window it opened. A failed or closed link is an error the UI
  shows. There is no reconnect fallback.
- **`FileStore`**: a project folder (the mod's sources, its packages and the
  user's disc images): `list`, `read`, `write` (atomic), `watch` where
  available. The browser version needs a Chromium browser, which is already the
  only one the project qualifies.
- **Shared core.** Everything that decides anything lives in `core/` and runs
  unchanged in Node, NW.js and the page, and in `node --test` without a browser.
- **What is desktop-only.** Running user generators (the supervised Node build of
  [authoring/README.md](authoring/README.md)) needs process isolation, so it is
  NW.js only. The browser tool edits data and packages and attaches to the game,
  but runs no user code.
- **NW.js.** Node and the DOM share one context, so the platform adapters are
  thin. Pin the NW.js release whose Chromium is nearest the runtime shell's
  Chromium 152, and add it to a pinned Nix shell (`nix/authoring` or a new
  `nix/studio`).

## Mod packages (runtime)

- **What a package changes.** It replaces disc files by their index in the
  game's own file table (sector 24 on both discs) and states which disc
  identities it applies to (`xem_disc::identify`).
- **No original bytes in packages.** A package must be shareable, so it carries
  binary deltas against the user's own disc files, plus wholly authored files.
  It never carries original data verbatim. The tool builds deltas; the loader
  applies them to the user's image and checks the hash of each source file
  before and of each result after.
- **Where it applies.** A virtual disc layer in shared host code (`xem-disc` and
  `xem-host`) used by every host. A replacement whose size differs is placed past
  the image's last sector, and the file table entries the game reads are patched
  to match. The exact sector format the port serves (raw 2352-byte sectors, EDC)
  must be read from `xem-disc` before this is designed.
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

1. **Scaffold.** `tools/studio` builds one bundle that opens in NW.js and as a
   page served by `serve.mjs`. `core/` has `node --test` coverage. Add the new
   files to `packaging/source-files.txt` and check them with
   `tools/repository/source_archive.py --check`.
   *Accept:* both shells start, and the platform self-check reports the right
   adapters.
2. **Live attach and inspector.** `GameLink` for both hosts. Panels:
   - status, frame step and run;
   - snapshot and restore;
   - a `schema.json` tree with `inspect` and `write`;
   - globals search;
   - a memory view.
   *Accept:* one test script attaches to `xem-desktop --control` (as
   `control.rs` launches it) and to the browser build (Playwright), runs the same
   commands, and gets equal results after the same replay.
3. **Read-only data browsers.** Disc file index, field bundle components and
   event script disassembly. The opcode tables are exported once from the Python
   decoders to JSON, so there is still one source of opcode facts.
   *Accept:* the TypeScript decoders agree with the Python `--sweep` on every
   script of both discs (a local test, since the discs stay local).
4. **Mod packages in the runtime.** Format, loader, delta builder, virtual disc
   layer, conflicts and save compatibility, as above. The tool packages and
   enables a mod.
   *Accept:* a changed message shows in both hosts, and with packages absent or
   disabled the `memory_hash` matches vanilla.
5. **Field event editor.** An assembler that preserves layout: absolute u16 PCs,
   handlers that read neighbouring bytes, and appending rather than inserting
   ([scripts/field-events.md](scripts/field-events.md)). Validators reject what
   the original would read out of range.
   *Accept:* decoding then encoding reproduces every shipped field script on both
   discs byte for byte, and an edited event plays in both hosts. A "reload this
   field" control command is added if restoring a snapshot is not enough.
6. **Map editor.** A three.js viewport for the field bundle: collision, trigger
   zones (palettes, images, triggers and the view block are still undecoded, per
   later-phases.md's data formats table, so decode those first), actor
   placement and entrances.
7. **Presentation replacements.** Texture packs and 3D models in place of
   sprites (first: Fei's field sprite as a glTF model in Lahan) in the render
   layer, presentation only, so RAM and timing stay unchanged. This ties in with
   plan.md's HD-2D mode.
8. **Authoring bridge and Lua.** Follow Phase 11 and the authoring contract
   (wall, door and collectible first). The NW.js tool is the desktop client.

## Verification

- **Core:** `node --test` over `tools/studio/core`, including round trips of
  synthetic inputs. Sweeps over the real discs run locally only.
- **Browser:** Playwright with the runtime shell's Chromium against the built
  page, inside the user-scope memory limit from the user's AGENTS.md.
- **NW.js:** its bundled ChromeDriver drives the same UI tests.
- **Native link:** under lavapipe and a headless Wayland, as `control.rs` does.
- **Runtime crates:** `src/runtime/scripts/suite.sh desktop` or `web` for
  whichever side changed, and `suite.sh full` once before any upstream pull
  request.

## Risks and open questions

- **Re-encoding.** Field scripts have overlapping handlers and absolute PCs, so
  edits must append. Some edits may still need relocation tooling.
- **Different file sizes.** Replacements of a different size depend on how the
  game reads its file table and on sector layout; milestone 4 settles this
  before committing to the format.
- **NW.js and WebGPU.** The browser preview inside NW.js depends on NW.js's
  Chromium build enabling WebGPU on Linux. The native game does not depend on
  it.
- **Upstream fit.** The NW.js shell differs from plan.md's Slint editors. Keep
  the runtime and core pieces free of NW.js so they can go upstream on their own.
