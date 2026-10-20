# Briefing for Sol: studio milestone 1 (2026-10-20)

You are taking over implementation of the modding studio on the user's fork of
Xenogears Ex Machina (XEM). The planning was done in a Claude Code session; this
is the handoff.

## The task

Implement [studio-milestone-1.md](studio-milestone-1.md): the `xem-studio`
backend crate and its wasm build, and the `tools/studio` frontend shell (Vite +
React + TypeScript) with widget layers copied from the user's Saga Foundry RPG
Creator, opening in NW.js and as a browser page. Its Result, Acceptance criteria
and Steps sections define done. Start with step 1, the shell mockup, and get the
user's approval before building UI.

Background, read as needed:

- [modding-toolkit.md](modding-toolkit.md): the overall design (Rust backend
  with one JSON command interface, web frontend, milestones 1-9).
- [runtime.md, Commands, introspection and snapshots](runtime.md#commands-introspection-and-snapshots):
  the game's control layer, the pattern `Studio::command` follows.
- `src/runtime/web/build.py`: the model for `tools/studio/build-backend.py`.
- `~/rpg_creator/docs/editor-widget-library.md`: the registry of the widgets
  being copied.

## Where and how

- **Checkout.** Work in the worktree `~/xenogears_toolkit-modding`, branch
  `modding-toolkit`. The main checkout `~/xenogears_toolkit` is on another
  branch (`serve-lan-https`) with uncommitted changes that are not yours; leave
  it alone.
- **Remotes.** `origin` is the user's fork (envy-ai/xenogears-ex-machina);
  `upstream` is brianblakely's. Push to `origin` only. Do not open pull requests
  to upstream; the user decides what goes upstream.
- **Commits.** Commit at each step that passes its checks, and push the branch.
- **Rules.** The repository's `AGENTS.md` (upstream's) and the user's
  `~/AGENTS.md` both apply. From the repository's:
  - never edit `src/decomp/`;
  - use the pinned Nix shells;
  - while developing, run only `src/runtime/scripts/suite.sh desktop` or `web`
    for the side you changed, never `full`.
- **Toolchain.**
  - Rust, wasm and wasm-bindgen: `nix develop path:./nix/runtime` (Rust
    1.97.1, wasm-bindgen 0.2.127, Node 24, Chromium 152).
  - Nix is a single-user install; a fresh shell may need
    `. ~/.nix-profile/etc/profile.d/nix.sh` first.
  - Frontend packages come from npm with a committed lock file, pinned to
    rpg_creator's versions, which the plan lists.
- **Tests.** Wrap test runs in the systemd memory scope from `~/AGENTS.md`. The
  system-level `systemd-run --wait` form needs root here, so this session used
  `systemd-run --user --scope -p MemoryMax=16G -p MemorySwapMax=0 <command>`.
  Playwright scripts go in `tools/studio/tests/e2e`.

## Licensing and the source allowlist

- **rpg_creator is AGPL-3.0; XEM is MIT.** The user, as rpg_creator's author,
  granted MIT for the copied files. The grant is recorded in
  `tools/studio/PROVENANCE.md`.
- **For every file copied** from rpg_creator (revision
  `1f690d0263af8881a2d33ee374a2e5006355748b`):
  - a header comment naming its source path and that revision;
  - a row in PROVENANCE.md's table, with what changed.
- **Copy only the general layers** listed in the plan's step 4. Cut the four
  links it names (`EditorPreferenceStore`, `EditorOperationOwners`, the native
  game window code, `domain/ids`) rather than copying the about 27,000 lines of
  rpg_creator domain code behind them.
- **The fonts** (Inter, Inconsolata) are OFL 1.1; copy their `OFL.txt` files
  with them.
- **Every new authored file** goes into `packaging/source-files.txt` in sorted
  position. Check with `python3 tools/repository/source_archive.py --check`.
  Generated output (`tools/studio/src/backend/pkg/`, `dist/`, `node_modules/`)
  is ignored by Git, not listed.

## Never commit

- `roms/` (the user's game archives), `discs/` and `.local/` (disc images,
  CHDs, the TLS key) and `tmp/`.
- No original game data or extracted assets of any kind.
- `roms/` and `tmp/` are in this branch's `.gitignore`; `discs/` and `.local/`
  are in upstream's.

## Network

- Bind dev servers to `0.0.0.0` so the user can reach them from their Windows
  machine. This machine is `shodan`, 192.168.50.8.
- The plan's HTTPS dev server uses the existing self-signed pair
  `.local/serve/cert.pem` and `key.pem` in `~/xenogears_toolkit`. That folder
  is not in the worktree, so reference it by absolute path or copy it into the
  worktree's ignored `.local/`.
- **Never send HSTS** (`Strict-Transport-Security`) or anything else that
  forces HTTPS on the host. The user has been explicit about this.

## State at handoff

- **Branch `modding-toolkit`** (head `672893d6`): upstream master as of
  2026-10-20 plus the plan documents and PROVENANCE.md. No studio code exists
  yet.
- **Disc images:** in `~/xenogears_toolkit/discs/` (`Xenogears disc 1.chd`,
  `Xenogears disc 2.chd`) and as bin/cue in `.local/discs/`. Milestone 1 does
  not need them.
- **Built game:** the matching decomp builds and the game module
  (`build/game/game.wasm`) and web build (`build/web`) exist in
  `~/xenogears_toolkit`, not in the worktree.
- **ASRA bridge:** `~/asra-bridge` lets ASRA message Claude sessions. It is not
  part of this task; don't change it.

## When you're done

Report:

- the exact commands you ran for each acceptance criterion and their results,
  including anything skipped or failing, with output;
- the commits on `modding-toolkit`.

If anything in the plan turns out wrong, such as a widget cut that changes
behaviour or an NW.js problem, say so and update studio-milestone-1.md in the
same change. Don't work around it silently.
