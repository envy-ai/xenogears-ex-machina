# XEM Studio

The studio has a dockable shell and a shared Rust/wasm command service. Its
command list, flow graph and actor lanes inspect original field scripts and
edit a separate authored draft. The Scene view adds source-qualified geometry,
collision, zone and actor/descriptor transforms on the same authored history. Packages apply edits through the virtual disc;
original images stay read-only. The live inspector attaches through the existing
native/browser runtime adapters.

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

## Server disc folder

The browser can open images stored on the Studio server without selecting or
copying them onto the browser's computer. Enable this read-only development/preview
endpoint explicitly when starting Vite:

```sh
export XEM_STUDIO_DISC_DIR=/absolute/path/to/discs
npm --prefix tools/studio run dev
```

Relative directory configuration resolves from the repository root. No directory
is exposed by default; an invalid configured directory fails startup with its
original diagnostic. In Project → Discs, choose **Browse server discs**, select
**Server disc image**, then **Open server disc**. Native keyboard selection and
Enter work through the existing controls. The local browser file picker remains
available, and the desktop path picker is unchanged.

Only regular, nonhidden `.bin`, `.chd` and `.cue` files directly in the configured
folder are listed/read. Subdirectories and symlinks are excluded. A CUE's BIN
must be a listed sibling in that same directory; the existing Rust CUE parser
supplies track boundaries. The endpoint has no write operation and never serves
project sidecars, arbitrary paths or unsupported files. Clients able to reach
this Studio origin can read the configured images; this adds no authentication.
It adds no CORS allowance, Vite filesystem allowance, TLS trust or Node grant.

`GET /__studio/discs` returns the normal `{ok:...}` envelope containing
`enabled` and `files[{id,name,bytes,etag}]`. The IDs are opaque to callers.
`GET /__studio/discs/{id}` requires a single closed `Range: bytes=start-end`
(inclusive, at most 8 MiB) and the listed `If-Match` ETag. It returns HTTP 206
with exact Content-Range, Content-Length and ETag, or a non-success status and
`{error:{kind,message}}`. There is no unbounded whole-file GET. The server checks the open
file identity before and after reading; changing an image invalidates its old
listing. Refresh the catalogue and reopen rather than mixing old/new chunks.
Catalogue requests and each range read have a 30-second abort deadline, including
the CUE acquisition before a disc session exists. Both ends check ranges and exact response lengths, and the existing wasm loader
fills only requested binary ranges. Disc contents are not sent as JSON arrays.

Names still belong to the project folder selected on the browser's computer.
The server image endpoint does not grant remote project-folder writes.

## Workspace

Project lists files and opens subfolders through the selected platform adapter.
Backend shows the wasm service's version and operations; its JSON command box
sends the original text, including malformed input. Log retains every command,
reply, folder operation and error for this session, with a Copy log action.
Expected authored-field guard messages persist locally with expandable details;
the Validation log retains their original stacks. Unexpected errors use a compact
global summary with expandable original stacks and cleanup failures in the Error
log, until explicitly dismissed. The dock layout alone is persisted locally.
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
  the runtime workspace package version. The returned `operations` array is the
  complete current method list; inspection, package and authored-draft methods
  are documented below.
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
The corresponding read-only views use the approved milestone-three mockups.

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


### Event views

Open a project folder for `names.json`, then open your own disc image in Project.
Browsers select a CHD or BIN; select a CUE and its companion BIN together. NW.js
opens a local CHD/BIN/CUE path through the same bounded binary range transport.
The disc identity and indexing progress remain visible. Switch between the two
disc sessions, filter maps by number, and select a real map; placeholders remain
labelled. Selecting another disc clears the previous loaded map view.

Expand an actor to see its 32 event slots (Load, Idle, Talk, Touch, then numbered
entries). The command list folds proven structures and exposes original raw PC,
bytes, edges and diagnostics. The graph is a read-only layered view, with pan,
zoom, selection and offscreen culling. Wheel up zooms in; wheel down zooms out.
Main flow connects bottom-to-top, with branch/switch-case outputs on the right.
Unfollowed/outside paths have explicit
non-executable destination markers. Lanes show actor/event spans, start/finish
request links and possible shared-variable dependencies, without a guessed
execution timeline. All views retain map diagnostic references.

Ctrl+1/2/3 switches views; Ctrl+F finds within the map; Enter follows a jump/call;
Alt+left/right navigates followed targets. F2 opens names for the selected event
or actor, or a variable slot/message ID. Only validated tables are saved through
the opened folder's file adapter. A bad existing sidecar remains untouched with
its original error visible. Undo/redo persists the same validated metadata path.
No disc bytes, scripts or message text are edited or decoded as text.

The extended source grant and exact canvas/history ranges are in PROVENANCE.
Only an explicit `VITE_XEM_STUDIO_TEST=1` build installs the synthetic component/
automation entry. `XEM_STUDIO_WASM_PACKAGE` selects a separate ignored fixture
WASM directory only when that test build flag is present; normal builds refuse
that override. Test builds, fixtures, bundles, private inputs and screenshots
remain ignored. No production identity override is added.


### Event-view verification modes

The normal `npm --prefix tools/studio run test:e2e` checks the production bundle,
including absence of the test hook. Synthetic event-view journeys are explicitly
skipped in this mode. Build a separate ignored bundle with
`VITE_XEM_STUDIO_TEST=1`, then set that same flag when running
`tests/e2e/events.spec.ts`. Set `XEM_STUDIO_TEST_DIST` to the built directory
(relative to `tools/studio` for Playwright). The production-hook assertion is
skipped only in this explicit test mode. Keep test artifacts in fresh ignored
paths and run these commands in the pinned shell and outer memory scope above.

Local real-disc browser checks use `tests/e2e/events-real-disc.spec.ts`, the
ordinary identity-validating wasm package in a test-hook bundle, and all of
`VITE_XEM_STUDIO_TEST=1`, `XEM_STUDIO_EXTRACT_ROOT`,
`XEM_STUDIO_DISC1_PATH` and `XEM_STUDIO_DISC2_PATH`. Without the private-input
gates the case is explicitly skipped. The NW equivalents are
`tests/e2e/nw-real-discs-smoke.mjs` and `tests/e2e/nw-events-smoke.mjs` (run from
`tools/studio`); the synthetic smoke requires an explicit
`XEM_STUDIO_SYNTHETIC_DISC_IMAGE` and its isolated fixture wasm bundle. No harness
requires an old timestamped run's metadata. These desktop tests use fresh local
manifests with neither development permission, and do not change TLS trust.

First-render timings start after component bytes are available: the initial
list includes decode, structure and paint; graph and lanes include their first
mount/data derivation and paint. Disc opening/indexing and component I/O are
excluded. The tests require actual rendered content from disc 1 map 2: its 2,358 decoded
instructions match the Python/Rust corpus sweep. The plan's approximate 3,400
count was an estimate, not the observed component size. Cached tab changes are
not first-render evidence.


The local milestone-three real-disc checks opened both existing BIN images in
Chromium and NW.js, with normal production disc identity validation. The initial
actor 0/event 0 of disc 1 map 2 measured:

| Host | List | Graph | Lanes |
| --- | ---: | ---: | ---: |
| Chromium 152 | 168.7 ms | 25.3 ms | 109.7 ms |
| NW.js 0.113.0 (headless, GPU disabled) | 204.6 ms | 42.8 ms | 98.9 ms |

These observations meet the respective 1 s / 2 s / 1 s targets, with the timing
boundary above. They do not measure every event or every map's rendering cost.
The both-disc backend sweep remains the exhaustive component/structure check.
Exact commands, input/bundle hashes, failed attempts and screenshots are retained
under `tmp/studio/milestone-3-ui-20261020/`; the final acceptance batch is
`acceptance-20261021/`. UI acceptance uses synthetic data in committed fixtures;
real disc inputs remain local and gated. Development TLS, file adapters and
runtime evidence are reused where unchanged, with their recorded limitations.


## Live game attachment (milestone four)

The Live game and Schema inspector docks use the runtime command channel,
separately from the Rust Studio backend. Build the game module/schema together
with the normal pinned `tools/game_module.py --stubs --release` workflow, then
build the native desktop and browser host against that same module directory.
The generated schema must carry its matching `module_sha256`.

For browser development, explicitly set `XEM_STUDIO_GAME_DIR` to the built
runtime page directory (containing `index.html`, `pkg/` and `game/`) alongside
`XEM_STUDIO_TLS_DIR` and the optional existing `XEM_STUDIO_DISC_DIR`. Vite serves
only its fixed runtime build file set under `/runtime/` on the same Studio
origin. The listener remains HTTPS `0.0.0.0:5176`, without HSTS. This mount adds
no control endpoint or cross-origin policy. Production static hosting can place
that same runtime build under `/runtime/` beside Studio's static files.

Open game & attach opens the same-origin game window. Import your original disc
and boot there, then retry attach if no session was ready. Reattachment reuses
the window without navigating it. Studio pauses and verifies the host before
stepping, inspecting, writing or snapshots. F7 steps the entered 1–10000 frames;
Shift+F6 runs/pauses; F6 retains panel focus cycling. Run state is acknowledged.

NW.js launches only the executable named in the Live game form, with an explicit
disc path and separate settings/cards directories. It passes an argument array,
not shell text; disconnect closes only that owned child. Use new directories
under ignored `tmp/studio/` for acceptance, preserving normal gameplay data.
The native executable embeds its module; no independent module picker exists.
Development retains exactly the approved `https://localhost:5176/*` Node scope
and selected public TLS certificate trust anchor. Packaged manifests have neither.

Select/expand a schema node to inspect, enter an exact decimal integer or enum
name and choose Write & read back. Unsupported/opaque/untyped/bit-field nodes
are explicit. Values outside the declared integer range are refused without a
write. Details stay inline. Clicking another node with unapplied scalar text waits
for Write & read back or Cancel draft; Keep editing retains the current draft.
Timeline/connection changes retain dirty text for reference with writing disabled
until explicit cancellation. Snapshots are named within the current connection; Restore requires
confirmation. Reboot/reconnect invalidates the previous inspector and snapshots.

Jump to node requires the same disc and map already loaded in the event studio,
a byte-identical declared component and an exact decoded PC owner. Empty actor
selection means the runtime's current actor. Ambiguous aliases, gaps, other modes
or mismatches show an unavailable reason. Name drafts and read-only game bytes
remain intact. Implementation/provenance and numeric limits are documented in
[src/game/README.md](src/game/README.md).


## Runtime package builder (milestone five)

The shared `xem-disc::mods` implementation builds and validates data-only packages.
Studio adds three commands through its existing response envelope:

- `package.build`: `{spec:{id,discs,requires,overrides},files:[{disc,slot,source,result}]}`.
  `discs` contains `{serial,sha256}` original boot identities; `disc` names one
  declared serial, and `slot` is a zero-based game-index record. `source` and
  `result` are byte arrays; `source:null` explicitly declares wholly authored
  content. The result contains `bytes`, `identity` and `manifest`.
- `package.inspect`: `{bytes:[...]}` returns the validated manifest and identity.
- `package.bundle`: `{packages:[[...],...]}` returns a length-prefixed package-set
  `bytes` container and its ordered `identities`, rejecting dependency/conflict
  errors. It preserves the given order rather than sorting packages implicitly.

The same builder has a native file adapter, with no GUI or executable package
scripts. In the pinned runtime shell:

```sh
cargo run --manifest-path src/runtime/Cargo.toml --locked -p xem-studio \
  --features cli --bin xem-package -- build recipe.json tmp/new.xemmod
cargo run --manifest-path src/runtime/Cargo.toml --locked -p xem-studio \
  --features cli --bin xem-package -- inspect tmp/new.xemmod
cargo run --manifest-path src/runtime/Cargo.toml --locked -p xem-studio \
  --features cli --bin xem-package -- bundle tmp/active.xemmods tmp/new.xemmod
```

Recipe paths are resolved relative to the recipe file and are not stored in the
package. The strict recipe object has the same `spec` as `package.build`, but
`files` uses `source` (path or null) and `result` (path). Output files must be new;
existing outputs are not overwritten. Keep originals, edited private results
and local packages under ignored `tmp/` or `.local/`. Deltas copy unchanged
regions from the user's original instead of including an original file member.
The format, compatibility policy and checked limits are recorded in
[the M5 plan](../../docs/studio-milestone-5-implementation.md).

### Field authoring drafts (milestone 6)

The event workspace edits a separate authored draft. Original images and the live
schema inspector remain separate. The list, flow graph and actor lanes render the
same compiled draft graph; operand encoding, validation, history and relocation
run in shared Rust on native and wasm. The existing `field.encode` contract still
requires a canonical unedited graph.

`field.draft.open {graph}` returns `{draft,revision,graph,bindings,can_undo,
can_redo,modified}`. `bindings` maps stable source IDs (`pc:0`, etc.) and authored
IDs (`new:1`, etc.) to compiled PCs. These IDs survive relocation and project
save/reload. Every mutation supplies `{draft,revision}`; stale revisions fail
without changing the draft. New methods retain `{ok:result}` / `{error:{kind,
message}}`, including arbitrary error-shaped echo data.

| Operation | Arguments/result |
| --- | --- |
| `field.draft.edit` | `{draft,revision,action}` → updated snapshot |
| `field.draft.undo`, `field.draft.redo` | `{draft,revision}` → updated snapshot |
| `field.draft.snapshot` | `{draft}` → current snapshot |
| `field.draft.compile` | `{draft}` → `{bytes,declared_size,graph,bindings}` |
| `field.draft.export` | `{draft}` → versioned base/actions/cursor/allocation document |
| `field.draft.import` | `{document}` → new handle and snapshot; validates/replays history |
| `field.draft.close` | `{draft}` → `{closed:handle}` |
| `field.templates` | no args → safe standalone templates derived from authoritative opcode metadata |
| `field.instruction` | `{bytes,operands}` → prepared `{bytes,targets}`; operand encoding remains in Rust |
| `field.map_bundle` | `{session,map}` → bounded pending/ready source bundle, actual index slot and `source` original-disc identity (null for synthetic inputs), through the existing binary-fill disc transport |
| `field.draft.repack` | `{draft,bundle,context?,source?,module_sha256?}` → replacement bundle bytes; source stream and declared boundary must match the immutable draft base. The three optional measured-context arguments are supplied together. |

Actions use a `kind` tag:

- `set_operand`: `{node,operand,value}`. Value is `{kind:"literal",value,
  immediate?:boolean}` for raw numeric operands, or `{kind:"target",node}` for
  an address bound to a stable instruction. Numeric address edits must resolve
  to an existing instruction. Operand widths, variable-bank references, actor
  selectors and control-flow diagnostics are checked.
- `insert_after`: `{after,bytes,targets?:{"operand-index":"node-id"}}`. The
  template must decode as one supported ordinary instruction. Every authored
  address operand requires a stable target. No insertion splits paired setup/
  consumer instructions or fixed-relative skip windows.
- `new_event`: `{actor,event,nodes:[{bytes,targets?},...]}`. Explicitly choose one
  existing actor and an absent or terminal-only event slot (0–31). Zero is absent
  only in the leading-ff map-record format. New actors and slot deletion are not
  implemented. Authored event nodes can be edited or extended by insertion.
- `batch`: `{actions:[...]}` groups 1–32 non-batch actions into one undo step.
  Failure preserves the original draft, allocator and revision.

A draft document includes the allocator start for every retained action and its
final watermark. Undo followed by an alternate edit does not reuse abandoned
IDs. Missing/invalid history fields and reused allocator positions are errors;
there is no implicit migration. There may be at most 16 open drafts and 128
applied transactions per draft. Unsupported forms/limits fail explicitly.

Same-size operand edits stay in place. Growth appends dependency-closed code
islands, redirects entrances and returns to original continuations. Pinned
map-entry records, gaps and overlapping storage are preserved. Relocation of
unresolved inner paths, wrapped instructions, unbounded jump tables and opaque
data is refused. Compound setup/consumer units remain contiguous. The final
packed stream is revalidated, including its explicit zero-literal tail, so
padding cannot activate a formerly unresolved destination. Extra trampoline
jumps consume original interpreter dispatch budget; equivalent flow does not
promise unchanged frame timing.

The inline editor has Apply/Cancel (Ctrl+Enter/Escape). Unapplied fields block
selection changes and new builds; cancel or apply before navigating. Insert
opens insertion controls. Ctrl+Z / Ctrl+Shift+Z operate authored history when
focus is outside text entry; Ctrl+S saves the project. Existing Ctrl+1/2/3 view
shortcuts, graph controls and live-inspector shortcuts remain available.

Save project writes `field-map-N.json` through the selected file adapter. It
contains private source-derived graph data, authored history and the names
metadata needed to restore the draft. Keep it in your local project, outside
published source. Load validates map/disc/base identity and names before adoption.
Delayed I/O cannot replace a different project/map/revision. A saved edit undone
back to original bytes is still unsaved relative to that saved project; save or
explicitly discard before leaving. Failed operations retain their original error
in the persistent UI and preserve the draft and last successful build.

Build package reads the selected map's actual source bundle, replaces only its
event component, and invokes the unchanged M5 package/delta builder. The new
stream uses deterministic literal-only packing with explicit final-group zeros
and terminal flag; unchanged component streams retain their original bytes.
Output is `field-map-N-<package-sha256>.xemmods` in the project. Its content-based
path keeps a retained native build from being overwritten by a failed later
build. Playback requires a successful build of the current applied revision.

For browser playback, first open the same-origin runtime from Live game, import
and boot your original disc. Play stages the exact package bytes and performs a
fresh boot through the existing API, then starts it. Native playback uses the
configured executable/disc/settings/cards paths and the same saved package via
its existing `--packages` argument. No new game logic, module loader, remote
origin, TLS permission or live-memory patch mechanism is introduced.

Repacking requires complete bounded decoding of all nine original descriptors,
including unaffected component 8. Some original files (the current map-23
acceptance candidate among them) end before that stream's full packed output:
its final bytes depend on adjacent original RAM. Such builds report
`source_context_required` and preserve the draft and last-good build until the
matching measured context is available. Sector padding is a different input and
is not substituted. The source-context contract is described below. The local M6 acceptance fixture
now verifies measured map-23 context, project Save/Load, deterministic Studio
Build/Play, and execution of the same package in native and browser hosts.

### Measured original field source context

Some original packed components consume bytes beyond the indexed map file. A
logical component prefix or sector padding cannot establish the complete original
output. Build refuses these files with `source_context_required` until a matching
vanilla runtime has observed the selected map's complete original load. It reads
that observation through the existing attached runtime, without booting another
map automatically. Unsupported, incomplete or stale observations remain errors.

The read-only runtime command is
`{"cmd":"field_source_context","map":23,"source_sha256":"<indexed bundle SHA256>"}`.
The version-1 response binds `provenance: "runtime_field_decompress"` to the running
module SHA256, known original disc/boot hash, map, index slot/LBA, indexed file
size/hash, word-rounded allocation, source address and canonical decimal load
vblank. Its nine component records identify descriptor index/offset/logical size,
packed length, exact source consumption, complete output length/SHA256,
destination address and measured bytes consumed beyond indexed EOF. These bytes
are private source data and stay in local project/ignored verification files.
Reset, restore, disc changes and a new load invalidate prior observations; an
incomplete or failed load cannot publish context. Active packages are refused.

The shared observer captures input immediately before each original decompressor
and compares its complete output with destination RAM immediately afterward.
Studio validates the context against the selected original bundle and module,
materializes only consistent consumed bytes beyond EOF before appending events,
and rechecks every unchanged component's complete output. It never assumes
sector-rounded input is original RAM or invents a decompression tail.

The project JSON optionally carries `source_context`. Newly acquired valid
context marks the project **Unsaved** independently of instruction undo/redo.
Save persists it through the selected file adapter; Load validates it against the
selected original disc before adoption. Save before changing maps, or explicitly
discard the draft. A changed project/session/revision/context generation rejects
stale asynchronous completions and preserves the current draft and last-good
build. A successfully saved matching context permits reproducible Build without
reacquiring it from an attached vanilla runtime.

For isolated browser acceptance while the owned HTTPS preview remains active,
`XEM_STUDIO_TEST_PORT` selects the loopback test server's port (default 5176).
It does not alter the development NW.js origin grant or certificate configuration.


Find within map accepts exact `@0123` or `0x0123` hexadecimal instruction PCs.
These select the instruction at that PC, including a relocated authored binding,
rather than an earlier branch/call whose operand mentions it. Other queries
continue to search mnemonics and displayed names/operands.


For an identified original image, disc identity includes `package_discs`, the
known original game's two boot identities supplied by Rust's existing
`KnownDisc` facts. Studio Build declares that family and modifies only the
selected disc/map file; the other disc passes through unchanged. Declaration
does not certify another image: native/browser preparation still verifies every
actual image and the unchanged M5 target/hash checks remain mandatory. This
allows the same field package to boot natively when the original sibling disc
is present in the changer, as well as in the browser.


### M6 verification and trying the runtime

The local map-23 acceptance changes an existing message selector, inserts a call,
and creates its reachable event. Both production hosts display the selected
existing message. The causal probe observes the authored marker rise from 0 to
4660 during ordinary field entry, before Cross input; vanilla stays 0, and Cross
leaves those values unchanged. Both hosts use the exact same Studio-exported package set. The
shared virtual-disc check preserves every map-22 byte and the other disc's index
and files. Package-absent and disabled RAM hashes match; enabled RAM hashes match
across hosts. Enabled versus absent whole RAM differs because the M5 virtual
file index is intentionally patched, even while an unchanged map is loaded.
Gameplay status comparisons explicitly retain the expected package identity.

The new raw-index fixture models MODE2 sectors with header/ECC gaps and a
seven-byte record crossing the 2048-byte user-data boundary. Private full source,
context, project, packages and screenshots stay under ignored
`tmp/studio/milestone-6/`; they are not distributable assets or CI inputs.

For an end-to-end browser session, use a runtime built with the field-source
observer, select the original disc and load the source map in its vanilla state,
then attach Studio and Build. Save the project again when context acquisition
marks it Unsaved. The saved matching context makes later builds reproducible
without reacquiring it. Native Play likewise needs an executable containing the
same observer-enabled module, the original disc and configured settings/cards.

The retained local acceptance builds are
`tmp/studio/milestone-6/checkpoint-36/web` and
`tmp/studio/milestone-6/checkpoint-36/cargo-target/release/xem-desktop`.
Select the web directory with `XEM_STUDIO_GAME_DIR` when starting Studio, and
select the native executable in Live game. Use the existing explicit
`XEM_STUDIO_TLS_DIR`; no certificate-validation bypass is needed. An already
running preview retains its launch configuration until its owner restarts it.
The preserved LAN preview currently uses its older checkpoint67 runtime; its
process/configuration was kept intact during isolated M6 checks.


For isolated runtime-browser retries, `XEM_WEB_TEST_OUTPUT_DIR` selects a fresh
artifact directory; its default remains `build/web-test`. The headset scaling
check pauses the game and observes camera-follow convergence over at least
300 ms before starting the gesture. This synchronizes the existing presentation
easing; it does not change the renderer or the original `<1 mm` scaling-anchor
assertion. Each invocation retains diagnostics in a unique subdirectory.


The local runtime verification retains its first full-run exit1 (copied Android
setup syntax and a headset test precondition), alongside focused passing retries.
Android boot/x86/ARM64 and the unchanged strict headset assertion passed. Both
NW modes passed: dev retains only the approved localhost origin/public anchor;
the built app uses the unchanged packaged manifest with neither development
grant. The earlier built-fixture timeout remains historical evidence. Actual
commands, inputs, exits and caveats are in the ignored M6 verification index.

### Field scene inspection (milestone seven backend checkpoint)

`field.scene.decode` accepts `{input:{bundle,context,source,module_sha256},
entrances:[]}` through the existing command envelope. `bundle` is the indexed
source file's bytes, not an event component. Optional fields are null for a pure
synthetic/unbound asset decode. A selected original `source` identity can be
validated on its own; measured `context` additionally requires that exact source
and `module_sha256`. It verifies all nine complete original outputs and consumed
tail bytes. Missing reads return `source_context_required`; sector padding and
zero fills are never substituted for original RAM. This operation does not open
or identify an image: obtain real source bytes/identity through the existing
verified disc session and `field.map_bundle`.

The result includes source identity/hash, raw source header/view/descriptors,
all nine component summaries, original geometry groups/meshes/faces, collision
referenced vertex prefixes/triangles/attributes, zone corners/tail, palette/TIM
and sprite-image upload words, the canonical M2 event graph, actor source IDs,
explicitly selected entrance records and unresolved-dependency diagnostics.
Input/output and project data derived from original discs remain private/local.
No game bytes are part of public metadata or test fixtures.

`entrances` contains explicitly established u16 selectors, not a record count
inferred from the ff prefix. Raw ff angle bytes still select variables 8/6.
Same-index entrance patching in the backend preserves executable decoding;
deleting/renumbering needs stronger cross-map/variable/saved-state reference
closure. Collision has no stored complete vertex count; unused storage is opaque,
not inferred from the independently addressed attribute table. Source positions
are the original raw descriptor words (actor initialization also uses signed low
halfwords). Actor event scripts/floor initialization can override placement;
these source markers are not claimed to be evaluated or live runtime poses.
The view words contain lights/background, not a static game camera eye/target.

Shared image slots report inherited-VRAM requirements explicitly. Texture-page/
CLUT control words are source declarations, not guessed runtime override state;
environment-map UVs and morph poses remain runtime-dependent. Decoder work
budgets are service limits, not original actor/geometry maxima: 4096 texture
uploads / 8 Mi words; 4096 expanded models and 262144 expanded vertices, faces
or primitive groups / 1048576 control words; 32 MiB indexed bundle and original
logical+16 component allocation within 2 MiB. The 32 sprite slots and 32 event
slots per actor do not establish an actor-count maximum.

Scene UI and packaged playback are verified within the supported M7 scope.
The human UI approval and final acceptance evidence are recorded in
[studio-milestone-7.md](../../docs/studio-milestone-7.md).
See `docs/studio-milestone-7.md` for the implementation/acceptance sequence.

`scene::bindings` captures source-owned canonical operands and repairs stable
literal targets while preserving symbolic self/party meaning. Unknown, aliased
or dangling references receive explicit diagnostics/refusals. Captures bind to a
complete source graph hash; namespaces come from that source, not a separately
supplied mutable identity registry. The service namespace budget is65535 stable
entities, not an original actor/zone maximum. Literal actor indices colliding
with reserved self/party bytes are refused;0xfc is an ordinary literal index.

`scene::domain` owns immutable bundle/context, atomic actions, stable identities,
monotonic allocators and undo/redo. Candidate validation and compilation precede
adoption. Actions cover descriptor transforms, source-template static instance
CRUD, qualified independent actor CRUD and stable actor events, scene-owned M6
edits, explicit script placements, zone CRUD, collision vertices/topology and
bound same-selector entrance transforms. Actor0 and party-owner initializers,
unqualified clone forms, unresolved references and unsafe collision sentinels
have explicit restrictions detailed below. Entrance structural CRUD/reference
closure remains unresolved; no record count or unused prefix space is guessed.
A header position does not imply scripts/floor initialization leave it unchanged.

Event-selector repairs reuse the existing M6 compiler through a private adapter;
public M6 actor-table bounds are unchanged. Scene packing now shares M6's exact
packing-tail activation refusal. Complete unrelated component outputs and the
original measured source guard are verified before container adoption.

#### Scene document protocol (M7 shared backend)

`field.scene.draft` is the typed document boundary. Its `args.kind` is one of
`open`, `import`, `edit`, `undo`, `redo`, `snapshot`, `placement`, `compile`,
`export`, `close`.
It uses the existing `{ok:result}` / `{error:{kind,message}}` envelope and preserves
`echo` and the M6 event API. There are at most four open scene documents (a service
memory budget); scene and event-draft handles are separate, monotonic namespaces.
Failed open/import/edit never adopts a partial owner or consumes its identity.

Open with `{kind:"open", input, entrances:[]}`. `input` has the same validated
indexed bundle/context/source/module fields as `field.scene.decode`; `entrances`
contains explicit established selectors. The result is `{scene,state}`. Edit with
`{kind:"edit",scene,revision,action}`, then inspect/undo/redo using that current
revision. Import establishes revision zero under a new handle. Export returns
schema4 source and action history, including both allocation-start lists,
retained allocator watermarks and the imported-event prefix described below.
The combined timeline permits128 applied transactions, with batches grouping
1–32 non-batch scene actions. Missing/older fields are rejected explicitly.
The cursor and entrance spans use portable u32 wire values; component context
lengths exceeding u32 are rejected before host-sized integer deserialization.

For example, after opening a document and receiving `scene:1,revision:0`:

```json
{"op":"field.scene.draft","args":{"kind":"edit","scene":1,"revision":0,"action":{"kind":"set_zone","id":"zone:0","corners":[[0,0,0],[0,0,100],[100,0,100],[100,0,0]]}}}
{"op":"field.scene.draft","args":{"kind":"export","scene":1}}
{"op":"field.scene.draft","args":{"kind":"compile","scene":1}}
```

These are illustrative values, not claims that zone0 exists in every map. Compile
returns the deterministic authored bundle/hash, immutable source hash, stable
entity ID order and event graph/bindings. It does not write a disc or a project.
The frontend owns guarded local project persistence through the shared Scene
owner; exported source-derived documents and packages remain private local data.

Collision snapshots expose `collision_entities` (stable layer/vertex/triangle IDs)
and a resolved numeric collision view derived from those same entities. New
triangles join only oppositely wound edges sharing exact stable vertex IDs;
proximity never guesses a connection. Deletion opens neighbors to `-1`, and a
referenced vertex cannot be removed. Duplicating a triangle copies its three
vertices with new identities. Structural preparation preserves old opaque bytes
and exact pre-table storage; conditional `triangle[-1]` plane dependencies return
`unsupported_scene_collision` until their vertex-reference closure is proven.
Same-layout vertex edits retain the original storage and reference identities.
Qualified actor-row CRUD and script placement compose through this protocol;
UI and actual package playback have separate acceptance evidence below.
Destructive entrance renumbering remains unsupported.

The pure `scene::actor_rows` primitive can rebind source event rows without moving
or normalizing code storage. Synthetic56-row checks do not imply a32-actor maximum.
It keeps original actor 0 fixed for engine callbacks. Source row templates share
entry PCs; independent code cloning and safe party/global initialization require
additional domain proofs before actor CRUD is enabled. Structural descriptor edits
also refuse the unqualified raw/variable effect-owner path in
`field_event_begin_effect`; an ordinary actor-selector-byte registry alone does
not close this dependency.

A local gated original-source preflight is available as the ignored Rust target
`scene_real_preflight::original_disc_scene_formats_context_and_static_transform_preflight`.
Set `XEM_STUDIO_M7_DISC1`, `XEM_STUDIO_M7_CONTEXT` and a fresh ignored
`XEM_STUDIO_M7_PREFLIGHT_OUTPUT` explicitly, using the documented pinned shell
and 16 GiB/no-swap scope. It reads the existing original image in place, verifies
source/context identities and complete outputs, and emits only numeric/hash
summary data. Missing inputs or unavailable static templates are explicit; this
target never claims runtime playback or generates editor screenshots.


Scene open also accepts an optional `event_document` in the existing M6 schema1.
Its base must exactly match the complete original event component and declared
boundary from the selected bundle. The scene promotes this authored history onto its one undo/redo timeline, retaining
M6 stable bindings, action allocation starts, cursor, redo and global watermark.
Source bytes remain the immutable original bundle. Scene export schema4 requires
an actually empty original-root `event_document` (schema1, no actions/allocation
starts, cursor0, next_id1), plus `imported_event_actions`, the count of validated
M6 event actions at the beginning of the scene timeline. The prefix contains only
event transactions in the unchanged original entity namespace. Incomplete or
older scene documents and hidden baseline history are explicitly rejected. M6's own document/protocol is
unchanged. A scene transform must not reset prior authored event changes, and a
zone-selector repair must follow an M6 node through its existing relocation.

The M6 subdocument's unsigned numbers and numeric target keys are checked against
portable u32 wire ranges before host-sized deserialization. `modified` indicates
authored history relative to the original source, including an initial M6 edit;
it is not a saved-project dirty flag. Frontend saved-state ownership must still
compare the saved revision and active project/source/context generations.

Ongoing event edits use the same scene revision/history, for example:

```json
{"op":"field.scene.draft","args":{"kind":"edit","scene":1,"revision":0,"action":{"kind":"event","action":{"kind":"insert_after","after":"pc:15","bytes":[14],"targets":{}}}}}
```

The selected stable node must actually exist and be writable; the sample PC is
illustrative. Nested event actions are the existing M6 typed actions and are
compiled against their immutable source. Scene schema4 additionally requires
`event_allocation_starts` and `next_event_id`; undo and alternate history cannot
reuse authored event IDs. Snapshot's `events` is the current compiled graph,
bytes and stable bindings, including scene selector/entrance repairs. Its
`next_event_id` is the retained global watermark. Initial M6 history is directly undoable/redoable through the scene commands,
including redo reserved before promotion. A branch edit truncates its abandoned
prefix without reusing allocated IDs. Snapshot `reference_diagnostics` retains
original unresolved-reference reasons. Validated imported M6 history can retain
unknown targets, including out-of-table zone bytes, without guessing an entity;
subsequent namespace changes still refuse unsafe remapping. Ordinary new scene
actions that introduce or explicitly touch unresolved selectors are rejected.

Selector bindings are captured against the scene namespace when an instruction
is authored or its overlapping operand bytes are explicitly edited. Untouched
references keep stable entity targets across code relocation and scene
renumbering; setting the same numeric selector explicitly can select a different
current entity. Unreachable bindings are retained, and restoring a dangling one
is an atomic error. New or explicitly edited unresolved scene selectors are
refused with their original reason. All changes, both allocators and captures
are adopted together only after candidate bundle validation.

The nonvisual TypeScript adapter is `src/backend/scenes.ts`:
`SceneDraftSession.open/import`, `edit`, `history`, `placement`, `compile`,
`export`, `close` and `nodeAt`. It retains its scene handle separately from M6
draft handles and adopts a returned revision/state only after a successful Rust
response. It creates no second event compiler or editable script copy. The
frontend integrates it under the guarded shared UI/project owner.

To inspect and explicitly transform a qualified source placement instruction:

```json
{"op":"field.scene.draft","args":{"kind":"placement","scene":1,"node":"pc:15"}}
{"op":"field.scene.draft","args":{"kind":"edit","scene":1,"revision":0,"action":{"kind":"set_placement","change":{"node":"pc:15","actor":"descriptor:0","event":0,"position":[120,null,80],"layer":null}}}}
```

The example requires that this actual stable node be one of the qualified place
writers in actor0/event0. It is not a generic live actor teleport. Inspection
returns signed immediate/variable/floor coordinates, all graph owner contexts,
capability and persistent diagnostic strings. Unsupported, shared, pinned or
variable-derived bindings fail explicitly. X/Z-only floor placement remains
editable; an entered floor Y is rejected. Edits reuse M6 field transactions and
therefore participate in the scene's atomic undo/save/build history.

Actor actions are `add_actor{template,position,rotation}`, `duplicate_actor{id}`,
`delete_actor{id}` and `actor_event{actor,event,nodes}`. Template/id/actor fields
use snapshot's stable actor IDs. New actors independently copy qualified current
scripts; constants keep their stable old targets and self remains symbolic.
Paired/inline/relative/party-owned/unclosed forms fail atomically with their
original reason. The template's scripts may override descriptor placement; use
explicit source placement bindings for those writers.

`actor_event` uses the existing M6 NodeInput form for an empty event slot, with
IDs beginning at snapshot.next_event_id. Numeric M6 NewEvent actor indices still
mean active original source rows, not the current runtime actor order or private
virtual rows. Snapshot actors carry typed origins and event_source_row as
provenance only. Generic creation supports runtime indices0..254 because actual
byte actor links and resolved0xff selectors have an absent sentinel. This does
not assert a universal original actor-count maximum; wider sources remain
losslessly readable. Original actor0 is fixed for engine events2/3.


## M7 Scene UI — supported-scene acceptance complete

Choose Scene (Ctrl+4) for the selected original map. Its source-qualified bundle
may require a one-time qualified context capture from an attached vanilla runtime,
as described for M6. If disconnected when this source needs context, Scene explains
how to use Live game → Open game & attach (/runtime/ by default), Choose disc
image…, Boot, attach again if needed, and load the same vanilla disc/map before
retrying Scene. Save project after capture to retain the matching context for
offline reuse; Load project can reuse an already saved qualified context. Sources
that decode completely offline do not require a runtime connection. The hierarchy filters original entity identities and layers. Q/W/E select,
move or rotate; F frames the selected source. Dragging previews inline values;
Apply commits one Rust transaction and Cancel preserves the accepted scene.
Raw integer units and unsupported operations are explicit. A display handle is
centred on selected geometry; translation retains the stored source origin, and
rotation changes the source rotation about that origin. No scale is invented.

Use the Scene inspector's scrolling fields for all four zone corners. Validation
and Apply/Cancel occupy a separate fixed region, so actions cannot intercept
coordinate clicks. Invalid inputs have native aria-invalid and aria-describedby
links to the visible reason. Invalid/unapplied fields keep the last accepted
scene while blocking new build/play. Technical stacks remain expandable locally
or in Log. Runtime observations are read-only; the locked pose overlay is qualified by
source, module, package and stable actor order.

Scene promotion shares the script history, including imported undo/redo state.
Save uses the existing field-map-N.json name with outer schema2 and strict Scene
document4; event-only schema1 remains supported. Names for original actors retain
source identity after active row changes. Three.js0.186.1 is an exact rendering
dependency; no additional RPG Creator source was copied. Source edits, original
asset interpretation and package preparation remain in shared Rust.

Focused synthetic interaction71 and visible/accessibility layout74 pass; actual
clean/invalid1440x900 PNGs are under ignored milestone-7/checkpoint-74/screenshots.
Source/SCSS/type/design audits passed with that source. Earlier failed attempts
are retained in the ignored verification ledger. These synthetic results alone
do not establish real editing/playback or persistence; the later original-map
and desktop evidence below supplies that coverage. Audit62/63 reports a HIGH direct Zod advisory, without a
demonstrated current widget collection-schema path. No zero-audit claim is made.


The runtime-pose integration uses the existing paused schema commands. Both
Refresh and Bind explicitly pause the runtime and leave it paused; resume from
Live game. Refresh reads signed16.16 actor XYZ and signed heading only after
module/family/map/component and stable actor/package-order qualification. Binding
vanilla source context reimports the same authored history and marks provenance
unsaved. The orange observation marker is read-only. Seven reader/PC unit tests
and source review pass; original-map browser observation and exact-package
native/browser playback now pass as recorded below.

Scene project browser checks now cover imported redo/allocator state, shared
history/save/reload and stale folder I/O. The actual actor/zone CRUD buttons and
undo/redo have synthetic browser coverage, including immediate actor/script
auto-selection and referenced-vertex refusal. Actor-order changes reset temporary
numeric navigation; persisted source bindings/history remain stable. These are
standalone editor checks, separate from real native/browser M7 gameplay and NW
acceptance.


M7 checkpoints86/89/90 now verify original Disc1map23 inline authored X29→61,
actual project save/reload, exact deterministic package Build/Play and same-set
native/browser gameplay. The authored placement exists at normal field entry
before Cross; absent/disabled controls stay29. Full map22 bytes/index and Disc2
pass-through are checked separately. Disc2 runtime coverage is title entry, not
full story. The saved project reloads under a new owner, rebuilds the exact set
and normal Frame selection centers the selected marker with surrounding decoded
collision geometry. Authored and read-only observed coordinates remain visible
together at1440×900. Sprite VM poses/runtime lighting are not reproduced in the
editor. SourceY marked floor is disabled and empty; it is not a literal zero.

Overview framing uses visible static source geometry, excluding unresolved actor
markers/vertex handles/live overlays. F or Frame selection uses the current
selected object after project/renderer replacement. View→Reset workspace layout
adopts the new default inspector proportions without replacing saved user layouts.
Three's exact MIT notice accompanies builds at licenses/three/LICENSE.

The ordinary development and production NW smokes pass. Earlier Linux Scene
attempts failed: Xvfb lacked GLX, normal Mesa Vulkan was WebGL2-blocklisted, and
the existing desktop could not provide a suitable EGL configuration. These
failures remain recorded. Windows desktop Scene acceptance subsequently passed
on NW.js 0.113.0 normal with ANGLE/NVIDIA RTX 3090 Direct3D11 WebGL2, without any
graphics, sandbox, blocklist or TLS bypass. Smoke runs can retain their
fresh filesystem fixtures and result artifacts with npm run test:nw --
--keep-artifacts (add --dev for the existing strict-TLS development launcher).
This option changes artifact retention only, not Node/TLS permissions.


The optional original-project desktop journey is npm run test:nw -- --scene
--keep-artifacts, with explicit XEM_NW_SMOKE_TEST_DIST (a test-only Vite build),
XEM_STUDIO_DISC1, XEM_STUDIO_DISC2 and XEM_STUDIO_M7_SAVED_PROJECT inputs. Private
inputs/output stay under ignored M7 storage. It uses a physical copied app with
canonical package/launcher/TLS helper and hash-checked assets. The optional
--scene-vulkan is valid only with --scene and selects the normal ANGLE Vulkan
renderer; set VK_ICD_FILENAMES to the existing pinned XEM_LAVAPIPE_ICD when
reproducing that measured test configuration. That attempted configuration
failed on this host and is retained as a diagnostic, not a successful recipe.
Use a normal supported WebGL2 desktop environment. The verified Windows run
used the unchanged app/test driver with a fresh local configuration and profile.
It corrected only the fixture's `sourceHeaderX` from29 to0: descriptor3's original
word at0x1c8 is0, distinct from its original script placement X29 and saved X61.
The first Windows failure remains retained. See the milestone record for the
exact configuration, input/artifact hashes and official runtime verification.


Final focused checks through97 pass: current typecheck/design/source/provenance
and opcode/access/model/scene-binding consistency checks, production assets and
Third-party notice identity, and the real production-browser test-hook absence
case. Matching-source Rust/native/WASM and unit proofs are reused; no unchanged
runtime rebuild/full-suite repeat is claimed. The final Windows run verifies
placement61 to62, Undo/Redo, real NodeFilesAdapter Save/Load, new-owner adoption
and equal saved-document/compiled-bundle identities, with original descriptor
X0 preserved. Application exit0 alone is insufficient: require a successful
Scene result without an error. Failed earlier attempts remain retained. The
supported-scene acceptance record, including format limitations, is
[studio-milestone-7.md](../../docs/studio-milestone-7.md).


## Qualified Map23 offline Scene policy (initial isolated fix)

The original isolated fix admitted one exact Disc1 Map23 source (slot652, LBA124528, indexed110052 bytes,
bundle SHA2560f0a131acdf052cae070a64912fc09eb00e0b5e6a81f9d8cb4719d6416213add)
to open and compile Scene offline without a runtime capture. The general
structural policy below now replaces that source allowlist. Component8 at
source offset109992 has logical72 and packed output78. Only its72-byte logical
payload is verified in this mode; six tail bytes are not preserved. This does
not establish that they are junk, unused, or developer-intended padding. Other
sources kept their existing validation and actionable context-required errors
at that checkpoint. The general audit below now supplies explicit coverage.

No edit, or edit then undo, returns exact indexed source bytes. Any actual edit
normalizes component8 into a new self-contained authored stream and updates its
descriptor, even if only the header/another component changed. Final streams
strict-decode safely; unchanged complete outputs of the other components are
protected. No missing RAM/sector bytes are synthesized. Offline assets expose
optional_tail_bytes6 and an explicit non-runtime-provenance diagnostic.

Save/Load retains the context-free Scene document. Existing captured-context
projects remain supported with strict full-output validation; event-only M6
repacking is unchanged. Actual disconnected Scene edit/save/reload/Build and a
new same-package native/browser field-entry comparison pass in the isolated
verification. The active preview is not changed by those checks.


## Structural offline Scene stream completion

Scene first attempts the unchanged strict packed decoder. Only a missing-source
read can enter offline completion; malformed references, output overflow and
missing logical data still fail. Measured-context inputs/imports always retain
strict complete-output validation. No RAM or disc-sector bytes are invented.

When all advertised output is already known and only terminal lookahead is
missing, Scene retains every known output byte. Nonempty output must end at
an actual eight-token group boundary; reaching the advertised size mid-group
cannot hide an inevitable output overflow behind a missing token. This includes
a complete zero-length header at indexed EOF. It does not require a runtime just to obtain
an otherwise unnecessary exit flag. Existing component parsers still enforce
their own minimum headers, counts, offsets and bounds.

An unavailable decoded suffix needs a component-specific proof. Component8 must
contain whole 24-byte zone records within its logical prefix. Component7 must
contain a complete message-table header/offset/dimension region and every valid
entry must terminate inside the known logical bytes. Scene has no qualified
loaded-font context: a bounded backward proof requires *both* one- and two-byte
interpretations of every non-control glyph to terminate safely, and checks the
original control lengths/continuations, including 0F04's selection fallback and
0F0B's operand reread. It never assumes that a zero second glyph byte terminates
text. Ambiguous messages still require qualified context. This is boundary
validation, without translating or editing messages.

Genuinely unavailable sprite, encounter, event or other opaque output remains
context-required. In particular, event code may traverse bytes past its logical
size. A logical prefix alone is not an execution-boundary proof. This policy
makes no claim that any omitted suffix is junk, unused, or developer-intended
padding; incomplete-source summaries expose `offline_stream_completion:true`
and, where output is omitted, `optional_tail_bytes` plus a persistent diagnostic.

No edit or edit followed by Undo returns the exact original indexed bytes.
Every actual edited bundle serializes **every** admitted incomplete component as
a complete self-contained authored stream and redirects its descriptor, including
header-only edits or unchanged component payloads. The original incomplete stream
cannot consume newly appended data. All final streams strictly decode; known
unaffected outputs are preserved, and normal authored literal packing explicitly
completes its final group within the original logical+16 allocation bound.
Event-only M6 repacking and the runtime decoder are unchanged.


Both-disc conformance inspects935 real bundles and525 placeholders:926 containers
preserve exact no-edit bytes and strictly repack edits;9 Disc1 message tables
remain context-required without a qualified font/source context (maps54,84,128,
140,162,168,183,453,473). Public Scene decode accepts923; Disc1 maps164/165 and
Disc2 map164 additionally report `invalid_scene_texture: texture offset table
exceeds component`. Draft opening accepts926. These counts describe separate
operations and do not establish whole-game gameplay support. Original Map0 and
Map145 offline edit/Undo/Redo/Save/Load/Build pass; final native/WASM project replay
reproduces their exact UI-built candidate hashes. See the current evidence in
[the milestone document](../../docs/studio-milestone-7.md#general-offline-scene-stream-completion-2026-10-24-active-plan).

The final original-map packages also execute on native and browser hosts with
exact state/pose parity and absent=disabled controls. Map0 changes its authored
X0→32. The Map145 fixture uses verified in-walkmesh X382→366; an earlier414
fixture was outside its usable floor and is retained as a failed attempt. The
corrected actor moves between total frames603 and733; its later stationary pose
is reported rather than presenting a permanent offset or uninterrupted patrol.
Normal production build verification consumes the current backend WASM and
confirms the explicit test driver is absent. Publication and live-preview
integration are separate steps; this fix's acceptance does not refresh a running
user preview.
