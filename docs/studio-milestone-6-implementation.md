# Milestone-six implementation plan

Baseline: `98968a2237f78e57a613d46b53dc231f2a40e4a0`, branch
`modding-toolkit`. Scope and acceptance: [studio-milestone-6.md](studio-milestone-6.md).
Sol/high owns production changes and integration; Luna/high owns test coding and
execution. The coordinator owns independent acceptance and built-in-imagegen
mockups. No M6 UI code precedes relayed human approval. M7 waits for acceptance.

## Contracts and format decisions

- Keep `field.encode`'s canonical unedited contract intact. Add `field::draft`
  over the same `FieldGraph`, instruction descriptors and storage units. The
  immutable validated base/source hash is never replaced by compiled output.
- Drafts are service-owned handles. Mutations require the current revision;
  stale requests fail before mutation. Stable IDs are `pc:<decimal>` for source
  instructions and monotonically allocated `new:<decimal>` for authored nodes.
  Event identity remains the existing actor/event pair, independent of PCs.
- Public operations: `field.draft.open {graph}`, `.edit {draft,revision,action}`,
  `.undo/.redo {draft,revision}`, `.compile/.export/.close {draft}`, and
  `.import {document}`. Replies retain the existing success/error envelope.
  Snapshot replies include revision, undo/redo availability, canonical compiled
  graph and stable-ID/PC bindings. Compilation is deterministic.
- Initial actions are `set_operand {node,operand,value}` (value is tagged
  `literal {value,immediate?}` or `target {node}`),
  `insert_after {after,bytes,targets?}` and
  `new_event {actor,event,nodes:[{bytes,targets?}]}`. Raw instruction templates
  are decoded and validated by the existing metadata, not an independent opcode
  registry. Address bindings name stable nodes; untouched numeric address
  operands retain their source instruction binding. The UI exposes typed fields
  over these descriptors, with raw bytes on demand.
- New events explicitly bind one existing actor's 32 slots. Zero means absent
  only for the documented leading-ff format. Otherwise a terminal-only slot may
  be deliberately replaced; duplicate entry PCs alone never imply absence.
  Adding actors, deleting nodes/events and relocating opaque data are excluded.
- Preserve atomic setup/consumer units and fixed-relative multi-exit dependency
  closure, including dependencies from unchanged predecessors. No insertion
  inside such a unit. Preserve header/map-entry records, gaps and overlap bytes.
  Refuse wrapped/overlapping/unresolved-inner relocation, unproven variable a6
  table bounds, data references into moved storage, invalid operands/targets,
  PC overflow and unsupported component layouts with operation-specific errors.
- Same-size operand edits use in-place encoding when safe. Growth appends only
  affected dependency-closed islands, places a three-byte jump at each original
  island entrance, remaps explicit references/entries and emits explicit returns
  to original continuations. Short islands may expand through adjacent ordinary
  units; never consume pinned bytes to make room. Inspect all unresolved paths
  before append: growth must not activate a previously outside destination.
- Extra trampoline dispatches count against the original eight-opcode frame
  budget. Relocation preserves proven control flow, not an assertion of original
  frame scheduling. Real acceptance must observe the intended edited behavior.
- Each transaction validates and compiles its candidate before publishing state.
  Refused edits, undo/redo or import preserve prior state and last valid output.
  Export includes the base and authored history with a versioned schema; import
  validates/replays all states. Bound history/input sizes explicitly, never clamp.

## Ordered checkpoints and files

1. Preflight and failing synthetic contract tests: this plan, brief, ignored
   `tmp/studio/milestone-6/` evidence; `tests/field_draft.rs`.
2. Backend graph transactions/compiler: `src/runtime/crates/xem-studio/src/field/`
   (`draft.rs`, focused `relocate.rs` as needed), `lib.rs` typed dispatch, public
   protocol README. Retain M2 graph/structure/lanes semantics and public metadata.
3. Packing/package preparation: extend `packed.rs` and `disc.rs` with deterministic
   supported event-component replacement, retaining all other component streams
   and headers. Reuse `packages.rs`/M5 builder and identity/load contracts. Prove
   decoded replacement and unrelated component equality before emitting a delta.
4. After human approval: compose existing event workspace/CommandList/GraphCanvas
   and registered inline controls; one backend draft for all views. Add service
   adapter, project draft save/reload and package/play integration. Preserve
   unsaved inline fields on failed operations; explicit apply/discard on leaving.
   Update widget/protocol/provenance docs and authoritative Sass, compile CSS.
5. Luna synthetic/native-wasm/browser/NW checks, then gated real-disc fixture:
   edit existing event, add node, bind reachable new event, package and legally
   trigger identical bytes on native/browser. Include an unchanged map and prove
   unchanged components and absent/disabled behavior. Preserve readable evidence.
6. Final required runtime suite once inputs stabilize; independent review/fixes,
   scoped local commits, protected main hashes/remote unchanged, restore TLS
   preview on 0.0.0.0. Stop for M6 acceptance before M7.

## Verification and evidence

Every test/build batch uses pinned `nix/runtime` and one outer user systemd scope
with MemoryMax=16G and MemorySwapMax=0, finite measured timeout. Inspect harness
syntax first; one delegated executor owns runs. Preserve exact commands, source
and input identities, exit codes, failures/skips, available resource telemetry
under fresh ignored checkpoint paths. No deletion or game-byte terminal output.

Tests cover ordinary operand edits, typed rejection, branch/loop/cross-event
bindings, insertion/growth, new-event slots, compound units, fixed-skip closure,
trampolines/out-and-back, unresolved paths, pinned/overlap/overflow boundaries,
transaction atomicity/stale revisions, deterministic compile, history/import,
packing/unaffected bytes and native/wasm parity. Browser tests exercise actual
normal-click selection, inline edit/apply/cancel, insertion/new event, history,
validation rejection, save/reload, package/play and meaningful keyboard/focus
paths. Reuse compatible M2-M5 evidence; rerun affected checks, not unrelated
journeys. Record the required full suite's actual result without hiding failures
or describing a modified copied harness as canonical execution.

## Historical checkpoint-26 real-input blocker

The map-23 candidate reaches authored compilation but cannot yet be packaged.
The ninth descriptor (component 8, trigger zones) has logical size 72 and a packed
output length of 78. Its indexed file supplies only 74 decoded bytes before the
source ends. Four output bytes therefore depend on adjacent load-time RAM;
this is not merely a missing unused speculative flag. A separately measured
sector-rounded buffer decodes 78 bytes, but sector padding is not evidence of
that original RAM context (`tools/analysis/field.py::field_components`).

At checkpoint aa514bab, `replace_events` deliberately refused this input with
`unsupported_bundle`.
It does not invent padding or mark the ninth component preserved. The fixture
records every unaffected descriptor, including 8, only after its complete
bounded output can be established and compared. Real package creation,
Studio Build/Play and two-host playback remain unverified for this candidate.
The bounded diagnostic is retained under ignored
`tmp/studio/milestone-6/checkpoint-26/`; earlier failed fixture attempts remain.
Resolving the source-memory contract is required before claiming M6 acceptance.

## Authorized source-context extension

Bart explicitly selected proper source/load-time-context support after checkpoint
`aa514bab`; the former unsupported-map stop is historical, not the chosen exit.
The original CD callback copies only the word-rounded final file bytes into the
map allocation and drains the remaining sector into a different buffer. Capture
actual RAM at each decompression seam; never substitute the rounded sector.

1. Add read-only port wrappers at field-load begin/end and immediately before/
   after each original field decompressor call. The original functions run
   unchanged. A bounded observer in the shared core collects one complete load,
   validates original file/index/allocation identity, measures consumed input
   including speculative reads, and compares complete decoded output to actual
   destination RAM before relocation/processing can alter it. Failed observation
   refuses context publication, not original game execution. Reset/restore and
   disc changes invalidate observations; capture is host metadata, not game state.
2. Share the existing strict packed decoder through xem-disc, without a second
   implementation. Publish a versioned runtime `field_source_context` query,
   bound to module, original known disc/boot hash, map/slot/LBA, indexed file hash
   and size, allocation/source addresses and load time. Include all nine
   descriptors, full output hashes/lengths, exact source consumption and bounded
   measured bytes beyond indexed EOF. Only a complete vanilla load qualifies.
3. Extend Studio repack with an explicit context plus expected selected-source
   identity. Validate all fields/bounds, merge only consistent measured extra
   ranges, and materialize those bytes before the appended event stream. Recheck
   every unchanged full output against the measured original runtime hashes.
   Missing, conflicting, stale or mismatched context fails before publication.
4. Build uses a matching context from the attached vanilla runtime when needed;
   project save/load retains its source-bound provenance. It never boots a
   different map or fabricates context behind the user's back. Existing bounded
   self-contained inputs remain supported without context. Package bytes remain
   deterministic and use the existing M5 source-validated delta/load path.
5. Luna regression, module/core/Studio/native-WASM parity, original map23 capture,
   real package/save-load/Studio Build-Play, both-host visible edit + inserted call
   + authored event, unchanged components/map, then final required runtime suite.
   Retain every previous failed checkpoint and distinguish new evidence exactly.


Context persistence is part of project ownership: validated context acquisition
increments a metadata generation, marks Unsaved even after an earlier project
Save, and participates in leaving and async completion guards. Explicit Save
records both authored revision and metadata generation only after the original
folder/session still owns the write. Load validates saved context against the
actual selected original disc before replacing the draft. Last-good build remains
independent and survives failed acquisition, validation or file operations.


The first actual original capture passed at ignored checkpoint37: native and
browser observed identical complete contexts for map23 using the new normal
module build. All nine immediate destination outputs matched the shared decoder;
component8 consumed five measured bytes beyond indexed EOF and produced all78
bytes, including its six-byte logical tail. This resolves the context-prerequisite
question with measured RAM, not sector padding. Repack and edited playback remain
separate acceptance stages; their results are recorded in the verification index.


Cross-host package validation retains the M5 rule that a set declares every
mounted disc, including native changer siblings. Studio supplies the known
two-disc target list from Rust's existing KnownDisc facts, with only the selected
map file changed. It does not ignore unsupported siblings or weaken boot-hash
validation. The earlier one-disc UI package/native refusal remains checkpoint48.

Untouched-map acceptance compares the original file bytes through the actual
shared virtual-disc overlay, gameplay status at the same frame, expected index
records and same-host screenshots. Absent/disabled raw RAM hashes must remain
identical, and native/browser raw RAM hashes must match in every mode. Enabled
versus absent full RAM is deliberately not claimed identical: M5 patches the
seven-byte file-index record in both disc and embedded EXE views, including
cd_disc_files at 0x80010004, even when another map is loaded. Record every raw
hash and validate the explicit relocation records; never mask arbitrary RAM
differences or describe an enabled package as wholly vanilla. Every unchanged
map23 component still requires its complete measured original output hash.


## Actual edited playback checkpoint55

The exact Studio UI-exported set from checkpoint49 passed the gated production
browser/native playback test in90.2s (one test, no private-input skip). It changes
the existing map23 message selector, inserts a call and binds a reachable authored
event. Both hosts displayed the expected existing message bytes and observed
marker4660 after normal input and600 frames. All12 edited/untouched,
absent/disabled/enabled routes passed their final assertions. Native/browser raw
RAM hashes match in every mode; absent/disabled match vanilla. Complete unrelated
component outputs are established by original capture37 and repack fixture50.

Checkpoint54 separately passed the new synthetic raw-sector index reader and
exact UI-package preservation test: every map22 byte and Disc2 index/files are
unchanged. Its first playback attempt failed only because its expected status
omitted the active package identity. Corrected55 explicitly expects that identity,
retaining all other status/identity fields and exact cross-host equality.
The failed54 logs and all earlier failures remain.

The original/virtual index assertions now read logical Form1 bytes through MODE2
sector-aware slices, including records straddling a payload boundary; no
contiguous raw-image offset assumption remains. Final source/NW checks and the
required full runtime suite are subsequent evidence, not inferred from55.


## Causal event timing and final runtime retries

The separate gated marker probe uses the exact Studio set from checkpoint49 and
the checkpoint36 native/browser hosts. Both hosts read title0, field-entry4660
before Cross, then4660 after Cross plus600frames; vanilla reads0 at all three
checkpoints. The authored event therefore executes during ordinary field entry.
Cross does not cause a0→4660 transition. The retained result flag describing the
marker after input is an endpoint assertion, not evidence of its onset.

The path-adapted full suite finished exit1:12stages passed; copied Android boot
failed its syntax error and the first xr-diorama timing case reported1.1082mm
anchor drift. All original logs/result files remain. Focused Android boot/x86/
ARM64 retries passed using the original successful APK/native outputs and fresh
read-only AVD state. Production Studio build and development NW smoke passed.
The first copied built-NW fixture timed out. The replacement fixture passed
with byte-identical canonical manifest/launchers and physical production assets
inside its app root. Its30 advertised operations, Node file adapter, listing,
echo and window controls passed. No production fix or development permission was
added to packaged mode; the specific cause of the older timeout remains unproven.

Headset diagnosis identified a test precondition: game pause leaves host-time
camera-follow easing active, while the old setup waited only presentation frame
counts. The corrected test observes a fixed-scale, paused camera target with
anchor/transform stability in a trailing window of at least300ms, bounded by10s.
It retains the original strict `<0.001 m` scaling assertion. Only the failed case
was rerun: pass1/1,0skipped,41.4s, baseline404ms/four distinct XR frames, scaling
movement6.843186916831157e-7m. Its unique diagnostics preserve the measured
transforms and timestamps. No renderer change or tolerance relaxation was needed.
The other three headset cases and all compatible successful stages are reused.

The follow-up runner also encountered missing `compgen` in the pinned shell
when collecting outcomes. Its per-stage exits are preserved; the run already
fails from the built-NW timeout independently of this aggregation error. Future
recording reads explicit stage files and does not relabel the run green.


## Later qualified Scene-only optional-tail contract

A separately authorized isolated M7 change permits only the exact verified
Disc1 Map23 Scene source to use component8's logical72-byte prefix without a
runtime context. Any edited Scene package authors a self-contained component8
stream, rather than allowing the original output78 stream to read appended data.
Those six tail bytes are not preserved or claimed to be junk, unused, or measured
RAM. No-edit output stays byte-identical. The original runtime decoder and the
measured-context contract above remain unchanged, including strict captured
project imports. Event-only M6 repacking is not relaxed. Exact qualification,
limits and new offline-package verification are in studio-milestone-7.md.
