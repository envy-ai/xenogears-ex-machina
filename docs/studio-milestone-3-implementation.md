# Milestone 3 implementation checkpoints

This execution plan implements the Result, acceptance criteria and steps in
[studio-milestone-3.md](studio-milestone-3.md). It starts at clean `ba61e15e` in
`/home/bart/xenogears_toolkit-modding`, branch `modding-toolkit`. No push, PR,
merge, security/trust change, main-checkout modification or later milestone is
authorized. The primary session remains Sol/high.

The work is large: disc transport, structured control-flow views and persistent
names cross several boundaries. The material risks are lossless structure,
unknown variable access roles, graph extraction licence scope, large-map render
cost and private-input evidence. They are resolved at separate checkpoints so
that UI approval does not block independent backend verification.

## 1. Mockups and copying boundary

- Astra/medium prepares image-generated previews in ignored
  `tmp/studio/milestone-3/mockups/`: disc/map browser and actor/event tree with
  command list, the same event's graph, and actor lanes. Match the registered
  Graphite Teal/compact widget style; show diagnostic and naming interactions,
  focus and the specified shortcuts using invented data.
- Relay actual PNG artifacts for Bart's explicit approval. UI implementation
  remains stopped until that approval arrives. Existing start authorization is
  not mockup approval.
- Inspect the exact pinned RPG Creator source and grant. Record proposed source
  ranges for canvas and history-aware naming fields. Existing MIT permission
  says only the files listed in PROVENANCE; hold new copying if no explicit
  permission covers those additions. Continue backend and mockup work.

The delivered mockups are `tmp/studio/milestone-3/mockups/v1/01-command-list.png`,
`02-flow-graph.png` and `03-actor-lanes.png`; approval is pending. The current
MIT grant is explicitly limited to the files already listed in PROVENANCE.
Proposed new copying stays held: generic portions of pinned
`src/editor/EventGraphEditor.tsx` (zoom/world constants 111–122, viewport/culling
around 607–741, coordinate/gesture/fit/zoom/pan helpers 2526–2836, and canvas/
wire/focus rendering around 4103–4280), plus generic `deltaHistory.ts` 1–813 and
focused form-history logic in `editorState.ts` 3552–3582/3606–3648. These are
candidate source regions, not permission to copy entire mixed-domain ranges;
actual extraction rows must identify precisely the portions retained. RPG node
schemas, palette/inspector, graph history, recovery and project-specific reducer
remain excluded. The permission question requests a bounded MIT extension for
those general layers, not a rewrite of the existing grant.

## 2. Disc/component backend

- Port `tools.analysis.packed.decode_block` faithfully: complete groups,
  overlapping copies, terminal speculative read and original error distinctions.
- Add stateful disc sessions over `xem-disc`, retaining `Studio::command(&self)`
  through interior state. Native paths open directly; wasm uses bounded
  `PrefetchedFile` pulls and one binary `disc_fill(session, offset, Uint8Array)`
  method. Validate JS integer offsets without truncation. Other operations keep
  typed JSON requests and the existing success/error envelope. A pure
  `disc.cue {text}` operation reuses the existing CUE parser for virtual BIN
  track readers; no duplicate frontend CUE parser is maintained. Checked sector
  arithmetic rejects overflowing time offsets in that shared parser.
- `disc.open` selects `{kind:"native",path}` or `{kind:"pull",name,bytes}`.
  `disc.poll`, `disc.missing`, `disc.maps` and `disc.close` use `{session}`;
  `field.map_events` uses `{session,map}`. Pending reads return explicit state
  and missing ranges; errors retain their original cause. Document exact replies
  after integration. Opening identifies the boot executable through `identify`;
  unknown images are refused, with no claim of whole-track Redump verification.
- Use `Index`, directory 4, file `0xb8+2n` and the preceding directory record.
  Preserve placeholders. Read bundles in bounded steps; extract the full event
  stream and declared boundary exactly as Python does, never raw-bundle decode.
- Synthetic identity acceptance is confined to an explicitly enabled,
  default-disabled `test-fixtures` build and one fixed invented executable
  digest/serial. There is no request/env override or production bypass. Keep
  fixture wasm/dist separate under ignored paths and prove the default build
  rejects that image.
- Checkpoint: synthetic Rust cases, native/pull parity, all 935 real components
  and 525 placeholders against the existing Python/extracted inputs. Commit the
  verified backend increment without publishing.

## 3. Structure, lanes and names backend

- Preserve milestone 2 graph/API serialization and unedited encoding behavior.
  Structure one selected event's reachable flow into sequences, single-join
  if/else, definite loops and inline-table switches. Keep irreducible/unsafe
  regions as labels/gotos. Instruction ownership is unique; explicit original
  edges remain inspectable and flattening must reproduce them exactly. Never
  force folding by duplicating instructions or moving them between branches.
- Lanes retain actor/event requests, symbolic self/party/unknown selectors and
  variable dependencies. Read authoritative handler source where operand kinds
  alone do not establish access roles. Any added machine-readable access facts
  must have one authoritative source and provenance; unknown roles stay unknown.
  Do not infer writes from mnemonic wording or bind party slots to concrete actors.
- Define a versioned `names.json` schema with explicit variable/map/actor/event/
  message scopes and contextual bounds. Backend validates only; the file adapter
  is the sole writer. Reject unknown keys, duplicate scope names and invalid IDs.
  Invalid pre-existing sidecars must remain untouched with original errors.
- Checkpoint: synthetic structure/flattening, lanes and names tests; native/wasm
  command parity; every event of both-disc maps checked for coverage/edges, with
  measured per-disc if/loop/switch/goto counts and known diagnostics retained.
  Commit verified backend behavior and protocol documentation.

## 4. Approved UI and nonvisual integration

Only after approval and resolved copying permission: disc picker/Node path and
pull transport; map browser and actor/event tree; command list; extracted general
GraphCanvas; actor lanes; names editing/persistence/undo. Reuse registered widget
layers, register coherent additions and compile authoritative SCSS. Preserve
selection across views, folds, explicit raw/diagnostic access, copy/navigation,
Ctrl+1/2/3, F2, Ctrl+F, Alt+left/right and Enter behavior with accessible focus.
Copy only permitted pinned ranges, with source headers and PROVENANCE entries.
No graph editing, RPG domain/palette/inspector/draft recovery, message text,
package writing or game link. Only names sidecar metadata is writable.

## 5. Acceptance and evidence

Luna/high owns test coding/execution; Astra/medium reviews correctness and scope.
Use pinned `nix/runtime`, one outer user systemd scope with MemoryMax=16G and
MemorySwapMax=0, finite timeouts and fresh logs. Record the exact command before
each attempt, its source/tool/config identities, scope and stage exit codes.
Keep failures and skip reasons. Private bytes remain in ignored local artifacts;
terminal output and committed tests use synthetic data or hashes/counts only.
Read both existing discs/extracts in place; never duplicate full archives.

After synthetic browser coverage and extended NW fixture smoke, open both real
discs in browser and NW behind the local input gate. Measure disc1/map2 first
render for each view (list/lanes target <1s, graph <2s), then cull/optimize if
needed. Report observed times, not stub-based estimates. Run focused Rust/wasm,
Python metadata, typecheck/Vitest/design, fmt/Clippy and source/provenance audits.
No unnecessary full multi-platform runtime rebuild. List new authored/public
metadata files in sorted `packaging/source-files.txt`; never list private inputs
or generated wasm/dist. Monitor disk headroom (53 GiB at start); no deletion/GC.
Protect the five main-checkout PR46 files using the prior exact hash baseline.
Finish with scoped local commits, clean tracked state and reviewable evidence.
