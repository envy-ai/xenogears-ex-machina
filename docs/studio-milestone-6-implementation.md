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

## Current real-input acceptance blocker

The map-23 candidate reaches authored compilation but cannot yet be packaged.
The ninth descriptor (component 8, trigger zones) has logical size 72 and a packed
output length of 78. Its indexed file supplies only 74 decoded bytes before the
source ends. Four output bytes therefore depend on adjacent load-time RAM;
this is not merely a missing unused speculative flag. A separately measured
sector-rounded buffer decodes 78 bytes, but sector padding is not evidence of
that original RAM context (`tools/analysis/field.py::field_components`).

`replace_events` deliberately refuses this input with `unsupported_bundle`.
It does not invent padding or mark the ninth component preserved. The fixture
records every unaffected descriptor, including 8, only after its complete
bounded output can be established and compared. Real package creation,
Studio Build/Play and two-host playback remain unverified for this candidate.
The bounded diagnostic is retained under ignored
`tmp/studio/milestone-6/checkpoint-26/`; earlier failed fixture attempts remain.
Resolving the source-memory contract is required before claiming M6 acceptance.
