# XEM Studio

Milestone one supplies a dockable shell and a host-independent Rust/wasm
command service. It browses a user-selected folder; it does not edit game data
or connect to the running game.

## Try it

From the repository root, enter the pinned toolchain and build the backend:

```sh
. ~/.nix-profile/etc/profile.d/nix.sh
nix --extra-experimental-features 'nix-command flakes' develop path:./nix/runtime
npm --prefix tools/studio ci
python3 tools/studio/build-backend.py
npm --prefix tools/studio run dev
```

Open `https://localhost:5176/` locally or
`https://192.168.50.8:5176/` on the LAN. Vite binds `0.0.0.0` and sends no HSTS.
It reads the existing local certificate/key from
`/home/bart/xenogears_toolkit/.local/serve/`; set `XEM_STUDIO_TLS_DIR` to another
local certificate directory containing `cert.pem` and `key.pem` if needed.
No key contents are copied into the bundle. Browser use requires a certificate
that the browser trusts; this workflow does not accept certificate warnings or
change browser/OS trust. NW.js development uses the app-scoped certificate
configuration described below. A Chromium browser with the File System Access
API and a secure context is required; unsupported browsers show an explicit
persistent error when a folder is requested.

For the desktop shell, in the same Nix shell:

```sh
npm --prefix tools/studio run build
npm --prefix tools/studio run desktop
```

The built desktop shell opens the same bundle, with Node file access and native
window controls. NW.js comes from the locked `nw@0.113.0` npm installer, normal
flavour. The Linux x64 archive and executable checksums are recorded in
[nw-binary.json](nw-binary.json); the archive hash matches the installer's
upstream SHA256 list. No SDK ChromeDriver is pinned or tested.

With the dev server running, `npm --prefix tools/studio run desktop -- --dev`
opens `https://localhost:5176/`. Its temporary manifest at
`tmp/studio/nw-launch/package.json` grants Node/NW APIs only to
`https://localhost:5176/*`, as explicitly approved for development. Code served
at that address can access native files. The temporary manifest also uses
NW.js's [additional trust anchors](https://docs.nwjs.io/References/Manifest%20Format/#additional_trust_anchors)
for the selected local public server certificate. It reads only `cert.pem`,
using the same `XEM_STUDIO_TLS_DIR` or default directory as Vite, and serializes
only its public X.509 certificate. TLS and hostname validation remain enabled.
The packaged manifest has neither `node-remote` nor `additional_trust_anchors`.
No OS/NSS trust store is changed, and no certificate or sandbox bypass is added.

## Workspace

Project lists files and opens subfolders through the selected platform adapter.
Backend shows the wasm service's version and operations; its JSON command box
sends the original text, including malformed input. Log retains every command,
reply, folder operation and error for this session, with a Copy log action.
Persistent errors include the original message/stack and cleanup failures, and
remain until explicitly dismissed. The dock layout alone is persisted locally.
No project data or folder handles are persisted or uploaded.

Drag panel tabs to dock or float them; their More actions menu offers keyboard
placement and Hide panel. View restores hidden panels or resets the layout.
Shortcuts: Ctrl/Cmd+O opens a folder; Ctrl/Cmd+Enter sends the command; F6 cycles
visible panels; F10 focuses File; arrows navigate menus; Escape closes menus
and About. About traps focus and restores it to Help when closed.

The copied widget contracts, centerless docking variant, and provenance are in
[docs/widgets.md](docs/widgets.md) and [PROVENANCE.md](PROVENANCE.md). New UI
composes those classes/widgets; feature styles own layout. Inter/Inconsolata
retain OFL 1.1, with their license notices under `public/fonts/`.

## Checks

Run checks from the repository root inside the pinned shell, in the required
16 GB/no-swap user scope. Browser scripts live in `tests/e2e`, and artifacts
are retained under the repository's ignored `tmp/studio` directory.

```sh
systemd-run --user --scope -p MemoryMax=16G -p MemorySwapMax=0 \
  cargo test --locked --manifest-path src/runtime/Cargo.toml -p xem-studio
systemd-run --user --scope -p MemoryMax=16G -p MemorySwapMax=0 \
  npm --prefix tools/studio run typecheck
systemd-run --user --scope -p MemoryMax=16G -p MemorySwapMax=0 \
  npm --prefix tools/studio test
systemd-run --user --scope -p MemoryMax=16G -p MemorySwapMax=0 \
  npm --prefix tools/studio run audit:design-system
systemd-run --user --scope -p MemoryMax=16G -p MemorySwapMax=0 \
  npm --prefix tools/studio run test:e2e
systemd-run --user --scope -p MemoryMax=16G -p MemorySwapMax=0 \
  npm --prefix tools/studio run test:nw
# With the HTTPS dev server running:
systemd-run --user --scope -p MemoryMax=16G -p MemorySwapMax=0 \
  npm --prefix tools/studio run test:nw -- --dev
systemd-run --user --scope -p MemoryMax=16G -p MemorySwapMax=0 \
  python3 tools/repository/source_archive.py --check
```

Unit tests use real temporary Node folders and synthetic browser handles. Every
frontend test exercises the generated wasm `echo`; no fake backend substitutes
for it. Browser tests use the pinned shell's `XEM_CHROMIUM`. The NW smoke uses
a fixture folder and a bounded Xvfb process on Linux, with GPU disabled for that
headless test only; it writes its result, verifies it, and quits. No sandbox or
certificate bypass is added to NW.js. The built-app smoke passed. The initial
`--dev` smoke timed out after 90 seconds with TLS handshake `net_error -202`
and produced no backend/Node result; `tmp/studio/nw-dev-smoke.log` preserves that
failure. After adding the approved development-only public certificate anchor,
the focused `--dev` smoke passed: native fixture file access, backend version,
wasm echo and rendered window controls. It also verified the actual manifest's
single public certificate against the selected certificate's fingerprint and
bytes, and both development fields' absence from the packaged manifest.
The successful log/result are `tmp/studio/nw-dev-smoke-anchor.log` and
`tmp/studio/nw-dev-anchor-smoke-result.json`; the actual manifest is
`tmp/studio/nw-dev-package-anchor.json`. The initial failure remains preserved.

`npm audit --json` currently reports three advisories in the mandated pins:
Vite 8.1.5 (high), Vitest 4.1.10 and its mocker (moderate). Their versions are
kept as planned; an automatic audit fix would change the approved toolchain.

The existing runtime compatibility check is `cargo build --locked
--manifest-path src/runtime/Cargo.toml --workspace`. The final runtime suite is
`src/runtime/scripts/suite.sh full`, in the same user memory scope. It also
requires local verified decomp ELFs; the first milestone-one attempt stopped
because `.local/decomp/build/battle.bin.elf` was absent. The subsequent offline
pinned matching build used only 26 existing, hash-verified original inputs
(2,171,720 bytes) copied into fresh ignored worktree paths. `make -C src/decomp
all-split` and `make -j4 -C src/decomp all-verify` passed: 26/26 binary agreements,
strict symbol checks and 2,386 cross-image names. This measures binary agreement;
source coverage was not measured. No `src/decomp/` source changed.

The runtime retry passed its game-module stage, then exhausted disk space.
Native, port, Android-build and arm64 stages failed; web, CI and Android-boot
result files were empty. Downstream stages did not run. Those failures and the
zero-byte summary artifact are preserved under `tmp/studio/runtime-verification-k2z9kv_0/`
and `build/suite/`; they are not passing evidence. After capacity was restored,
verification resumed with fresh logs, carrying forward the completed module and
matching checks. Native build, Rust workspace tests, port, web build, CI,
Android build/boot/smokes and arm64 passed. Browser and timing initially failed
because the worktree lacked its locked `playwright-core`; the documented
`npm ci` setup succeeded offline, with no lock changes. Only those two failed
stages were retried: browser passed 40 tests with four skipped; timing passed
five runnable cases, with four original-game/XR scenarios skipped. The slice
suite had no runnable tests without the local CHD. Validation and native XR
also require that private disc input and were not run.

The resumed full invocation retains exit 1 for its missing-dependency failures;
the supplemental stage passes are separate evidence, not a new all-green full
invocation. Exact commands, the unchanged suite's hash, the temporary harness
patch and per-stage logs are in `tmp/studio/runtime-verification-k2z9kv_0/`.

## Backend

`python3 tools/studio/build-backend.py` invokes locked Cargo for wasm32 and
`wasm-bindgen --target web`. Add `--release` for an optimized backend. The build
emits JavaScript, TypeScript declarations and WebAssembly into ignored
`src/backend/pkg/` and honors `CARGO_TARGET_DIR` relative to the caller's working
directory. Native callers use the same
`xem_studio::Studio::new().command(json)` service.

The JSON command envelope is `{ "op": "name", "args": value }`:

- `{"op":"version"}` (args absent or null) returns
  `{"version":"0.5.0","operations":["version","echo"]}`. The version follows
  the runtime workspace package version.
- `{"op":"echo","args":value}` returns `value` unchanged. Any JSON value,
  including null, is accepted; the args field is required.
- Failures return `{"error":{"kind":"...","message":"..."}}`. Kinds are
  `malformed_json` for parse errors, `invalid_command` for a missing/non-string
  op or a non-object envelope, `unknown_operation` for an unrecognized op, and
  `invalid_args` for a known operation with invalid arguments or extra fields.
  Messages preserve parser/deserializer diagnostics. Invalid calls do not
  prevent subsequent commands.
