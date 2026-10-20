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
No key contents are copied into the bundle. Accept the self-signed certificate
in the browser before opening folders. A Chromium browser with the File System
Access API and a secure context is required; unsupported browsers show an
explicit persistent error when a folder is requested.

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

The `--dev` desktop launch currently requires a decision on the narrowly scoped
`https://localhost:5176/*` Node/NW API grant. Until that is approved, the HTTPS
page is the browser preview; do not treat an ungranted remote page as a native
shell. This requires no certificate-trust changes or sandbox-bypass flags.

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
systemd-run --user --scope -p MemoryMax=16G -p MemorySwapMax=0 \
  python3 tools/repository/source_archive.py --check
```

Unit tests use real temporary Node folders and synthetic browser handles. Every
frontend test exercises the generated wasm `echo`; no fake backend substitutes
for it. Browser tests use the pinned shell's `XEM_CHROMIUM`. The NW smoke uses
a fixture folder and a bounded Xvfb process on Linux, with GPU disabled for that
headless test only; it writes its result, verifies it, and quits. No sandbox or
certificate bypass is added to NW.js.

`npm audit --json` currently reports three advisories in the mandated pins:
Vite 8.1.5 (high), Vitest 4.1.10 and its mocker (moderate). Their versions are
kept as planned; an automatic audit fix would change the approved toolchain.

The existing runtime compatibility check is `cargo build --locked
--manifest-path src/runtime/Cargo.toml --workspace`. The final runtime suite is
`src/runtime/scripts/suite.sh full`, in the same user memory scope. It also
requires local verified decomp ELFs; the first milestone-one attempt stopped
before downstream tests because `.local/decomp/build/battle.bin.elf` was absent.
That attempted run is not a full-suite pass. No `src/decomp/` source changed.

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
