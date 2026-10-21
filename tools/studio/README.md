# XEM Studio

The studio has a dockable shell and a host-independent Rust/wasm command
service. Its field-event backend inspects original components without editing
game data or connecting to the running game. Milestone-three event views are
implemented only after explicit approval of their mockups.

## Try it

From the repository root, enter the pinned toolchain and build the backend:

```sh
. ~/.nix-profile/etc/profile.d/nix.sh
nix --extra-experimental-features 'nix-command flakes' develop path:./nix/runtime
npm --prefix tools/studio ci
python3 tools/studio/build-backend.py
export XEM_STUDIO_TLS_DIR=/absolute/path/to/local/tls
npm --prefix tools/studio run dev
```

Open `https://localhost:5176/` locally or
`https://192.168.50.8:5176/` on the LAN. Vite binds `0.0.0.0` and sends no HSTS.
Set `XEM_STUDIO_TLS_DIR` explicitly to an existing local directory containing
`cert.pem` and `key.pem`; there is no machine-specific fallback. Absolute paths
are used directly; relative paths resolve from the repository root for both
Vite and the NW.js launcher, regardless of their working directory. Missing
configuration or files cause startup to fail with the original diagnostic.
The built desktop app and production build need no TLS configuration.
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
using the same explicitly selected `XEM_STUDIO_TLS_DIR` as Vite, and serializes
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

Browser UI regression runs against the built bundle on loopback HTTP port 5176
(a localhost secure context), without ignoring certificate errors. The test-only
server lives in `tests/e2e/serve-built.mjs`; run `npm run build` first and stop
the dev server while that port is used for the browser run. The separate NW.js
`--dev` smoke checks the actual HTTPS server, selected public certificate and
native adapter. The production dev server remains HTTPS on `0.0.0.0:5176`, with
no HSTS. Browser HTTPS use still requires a normally trusted certificate.

Unit tests use real temporary Node folders and synthetic browser handles.
File-adapter and browser protocol tests exercise generated wasm `echo`;
configuration/decoder tests also cover malformed input directly. No fake backend
substitutes for wasm. Browser tests use the pinned shell's `XEM_CHROMIUM`.
The NW smoke uses a fixture folder and a bounded Xvfb process on Linux, with GPU
disabled for that headless test only; it verifies the result and quits. No
sandbox or certificate bypass is added to NW.js.

Cleanup verification passed: five Rust tests, 17 Vitest tests, nine browser
regressions, source/design audits, and built/development NW smokes. The dev
smoke also checks normal TLS/hostname validation, the served public certificate,
no HSTS, exact manifest origin/anchor and packaged absence. Fresh commands,
logs and screenshots are under `tmp/studio/cleanup-20261020/checks/`; the earlier
milestone-one failures and results remain in their original artifact paths.

The cleanup pins Vite 8.1.6 and Vitest 4.1.12, verified against official
advisories and registry engine/peer metadata before installation. Vite 8.1.6
fixes the three reported editor advisories without a minor-version upgrade;
Vitest 4.1.12 includes the patched mocker. Node 24.19.0, the React plugin 6.0.3,
Sass and the remaining direct pins are unchanged. The assessment and audit
limits are in [docs/dependency-audit.md](docs/dependency-audit.md).

The runtime evidence below predates this standalone cleanup and is carried
forward without rerunning the runtime suite. The changed studio protocol is
covered by its focused native Rust and wasm/frontend checks above.

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

The JSON request is `{ "op": "name", "args": value }`. Every response has
exactly one outer key: `{"ok":result}` or `{"error":{"kind":...,"message":...}}`.
Callers inspect that outer key, never the request operation or result truthiness.
The UI unwraps successful results in the response pane and records raw envelopes
in Log; errors remain persistent and do not prevent later commands.

Operations:

- `{"op":"version"}` (args absent or null) returns
  `{"ok":{"version":"0.5.0","operations":[...]}}`. The version follows
  the runtime workspace package version. Current operations are `version`, `echo`,
  `field.decode`, `field.encode`, `disc.open`, `disc.poll`, `disc.missing`,
  `disc.close`, `disc.maps`, `field.map_events`, `field.structure`,
  `names.validate`, `field.lanes` and `disc.cue`.
- `{"op":"echo","args":value}` returns `{"ok":value}`. Any JSON value,
  including null, false, zero, an empty string, and objects with `ok` or `error`
  keys is accepted unchanged inside `ok`; the args field is required.
- Failures return `{"error":{"kind":"...","message":"..."}}`. Kinds are
  `malformed_json` for parse errors, `invalid_command` for a missing/non-string
  op or a non-object envelope, `unknown_operation` for an unrecognized op, and
  `invalid_args` for a known operation with invalid arguments or extra fields.
  Messages preserve parser/deserializer diagnostics. Invalid calls do not
  prevent subsequent commands.


### Field-event inspection (milestone two)

`{"op":"field.decode","args":{"bytes":[...],"declared_size":1234}}` accepts
an **already decompressed event component**, including its 128 variable-type
bytes, actor count and actor/event entry rows. It does not accept a whole map
bundle or disc. `declared_size` is optional and defaults to the supplied length;
when present it includes the header and retains at most 16 supplied stream-tail
bytes. No padding is invented. The result is `{"ok": graph}`.

`{"op":"field.encode","args":{"graph":graph}}` returns `{"ok":[...]}` with the
same whole stream, including its tail. It assembles explicit header fields and
storage blocks and validates their source hash and canonical decode. This is
an unedited round trip: changing bytes, graph semantics, offsets or identity is
rejected. It provides no editing, insertion, relocation or package-writing API.

The graph schema, diagnostic limits, generator and local both-disc verification
workflow are documented in [studio-milestone-2.md](../../docs/studio-milestone-2.md).
Malformed/truncated/unsupported components return structured errors. Known
Python-sweep overlap and outside-successor cases remain explicit diagnostics;
a returned inspection graph does not assert executable control flow.

### Read-only event backend (milestone three)

The additional operations below retain the same typed request and outer
`ok`/`error` envelope. `version.operations` advertises the complete current list;
consumers must not assume the milestone-two four-operation list is final.
The UI for these operations requires separately approved mockups.

Disc sessions belong to one `Studio` instance. Native Rust callers open CHD,
BIN or CUE paths; wasm callers supply a random-access image reader:

```json
{"op":"disc.open","args":{"kind":"native","path":"/local/disc.chd"}}
{"op":"disc.open","args":{"kind":"pull","name":"disc.chd","bytes":12345678}}
{"op":"disc.poll","args":{"session":1}}
{"op":"disc.missing","args":{"session":1}}
{"op":"disc.maps","args":{"session":1}}
{"op":"field.map_events","args":{"session":1,"map":2}}
{"op":"disc.close","args":{"session":1}}
```

`{"op":"disc.cue","args":{"text":"..."}}` uses the existing `xem-disc`
CUE parser and returns `{file,number,start,end}` for the first MODE2/2352 track.
Start/end are sectors within the named BIN; end is null when it runs to EOF.
Pull adapters present that track as a virtual image, translating each byte range
by `start * 2352` and respecting the next track boundary. The browser requires
the matching user-selected companion file; Node resolves it beside the selected
CUE. No second CUE parser or whole-image copy is needed. Sheets over 64 KiB and
invalid/overflowing sector times are rejected explicitly.

Opening/polling returns `session`, `name`, `bytes`, `state` (`pending`/`ready`),
`phase`, `identity`, `missing` ranges and cache accounting. Identity includes the
recognized disc/serial, boot path/LBA and boot executable SHA-256. This is the
existing `xem_disc::identify` identity check, not whole-track Redump verification.
Unknown images fail explicitly. A native open failure returns no live handle;
a later failed session must be closed by its owner. Handles are never reused.

Pending replies expose half-open `[start,end]` byte ranges. Fill them through
`studio.disc_fill(session, offset, Uint8Array)` and unwrap its JSON envelope, then
retry the pending operation. This is the only extra wasm method: bytes never
travel through JSON fills. Offsets must be nonnegative safe JavaScript integers;
fills contain complete requested 256 KiB chunks (or the final short file chunk)
and fit the 8 MiB range-cache budget. Decoded CHD hunks have a separate 4 MiB
budget. `disc.missing` drains the currently requested ranges; callers must not
discard a pending reply's ranges or interleave competing reads on one session.
Read, cancellation and timeout failures must retain their original diagnostics.

`disc.maps` returns pending progress (`completed`, `total`, `missing`) or ready
`maps` rows: `map`, `file`, `slot`, `placeholder`, `actor_count`, `script_size`
(bytecode length) and `stream_size` (whole decompressed stream). Placeholder
counts/sizes are null; absent/invalid index records are errors, not placeholders.
`field.map_events` similarly returns pending ranges or ready `placeholder` and
`component`. A real component contains `bytes` (whole decompressed stream),
`declared_size` (including the header), `bytecode_offset`, `component_end`
(relative to bytecode), and `actor_count`. Pass `bytes` and `declared_size` to
`field.decode`; never pass the raw map bundle. Placeholders have null component.
The loader bounds bundles to 32 MiB and event allocations to 2 MiB and reports
malformed headers, truncated data and invalid LZSS distances/crossings explicitly.

```json
{"op":"field.structure","args":{"graph":{},"actor":0,"event":2}}
{"op":"field.lanes","args":{"graph":{}}}
```

Replace `{}` with an actual canonical `field.decode` graph. Structure validates
that graph and returns schema version 1, actor/event, entry PC, skipped state,
`body.nodes`, and fold counts (`ifs`, `loops`, `switches`, `gotos`). Nodes are
`instruction`, `if`, `loop`, `switch`, `label` or `goto`. Each instruction owner
contains its PC and exact original outgoing edge records. Loop bodies own their
header once; switch cases retain inline-table slot PCs. The public Rust
`structure::flatten` reads those owners and reproduces reachable PCs/edges;
labels are anchors, not duplicate instruction owners. Conservative fallbacks
remain explicit, including excessive nesting and irreducible/shared regions.
Skipped `ff` entries have no reachable instructions. No fold changes bytecode.

Lanes returns all actors and their 32 event slots, request links, per-event
reachable PCs/skips and variable accesses, plus dependencies grouped by canonical
variable slot. `08`/`09` actor operands are literal selectors: direct actors,
self (`fb`) and symbolic party slots (`ff/fe/fd`). Self can resolve to the source
actor; party membership cannot be inferred statically. Packed event low five
bits select the entry and high three bits retain priority. Invalid direct actors
remain unresolved. Variable roles use the generated source-proven access facts
in [data/README.md](../../src/runtime/crates/xem-studio/data/README.md).
Odd variable references retain their raw value/alias diagnostic but share the
original `reference >> 1` slot. Invalid, indirect or unknown-role accesses never
create invented concrete dependencies. Grouped readers/writers express possible
shared-state relationships, not an event execution timeline.

#### Names sidecar contract

```json
{"op":"names.validate","args":{"json":"{\"schema_version\":1,\"variables\":{\"4\":\"Gate open\"},\"maps\":{\"2\":{\"actors\":{\"0\":{\"name\":\"Gatekeeper\",\"events\":{\"2\":\"Talk\"}}}}},\"messages\":{\"12\":\"Greeting\"}}","context":{"actor_counts":{"2":8}}}}
```

The original sidecar text is supplied as a string so duplicate JSON keys cannot
be lost before validation. Success returns the validated `table` and variable,
actor, event and message name counts. Unknown keys, duplicate names within a
scope (Unicode lowercasing), noncanonical decimal IDs and invalid names fail.
Names are nonempty with no surrounding whitespace/control characters. Variable
slots are 0–1023, field IDs 0–4095, events 0–31, messages 0–65535. Message ID
validation checks the operand domain only: message text/bank existence is not
claimed. Actor bounds require loaded map `actor_counts`; missing context is
`names_context_required`, never a guessed zero. Merge known non-placeholder
metadata across open discs. Variables/messages alone need no disc context.

The backend never reads or writes `names.json`. The frontend file adapter owns
that sole user-data write and must preserve an invalid existing sidecar and its
original error instead of replacing or repairing it. Game data remains read-only.

The `test-fixtures` Cargo feature is default-disabled and accepts exactly one
invented boot-executable identity for synthetic disc tests. No request or
environment variable bypass exists. Fixture wasm/dist must stay in separate
ignored test paths; never use that feature for the normal studio bundle.

The milestone-three local component/structure sweep passed for the existing
both-disc extracts: disc 1 has 730 real maps and no placeholders; disc 2 has
205 real maps and 525 placeholders. It compared bundle hashes, whole decoded
components/boundaries, instruction/header/diagnostic semantics and unedited
encoding with Python. All 746,496 actor/event slots were checked for unique
reachable instruction ownership and exact flattened edges (778,140 distinct
per-map decoded instructions across the corpus). Fold counts are event totals,
including shared-entry aliases; they are not unique source constructs. Gotos
count explicit fallback instruction owners, including ordinary/terminal rows
inside an unstructured region; they are not a count of jump opcodes:

| Disc | Event slots | Skipped slots | Ifs | Loops | Switches | Gotos |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 579,744 | 474,041 | 10,211 | 111,769 | 4 | 687,352 |
| 2 | 166,752 | 134,629 | 3,339 | 34,873 | 4 | 292,398 |

This is backend corpus evidence, not a real-disc UI performance result. The
scoped run completed in 166.55 seconds; its exact invocation, source identities
and output are retained in `tmp/studio/milestone-3/checks/both-disc-sweep-v1.*`.
