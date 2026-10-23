# Studio milestone 6 — Editing acceptance brief

Goal: Make field-event node edits, new nodes and new events usable through the shared Rust service and existing Studio views, compile by safe append relocation, build an M5 package, and play the edit in both hosts. M7 is authorized but must not start until M6 acceptance passes.

Baseline: clean 98968a2237f78e57a613d46b53dc231f2a40e4a0, modding-toolkit at /home/bart/xenogears_toolkit-modding. M5 verification-index.md records historical full suite 12/14 exit1 plus successful checkpoint67 corrective retry; do not rebrand this as a fully green rerun.

Effort: substantial backend/compiler and UI integration. Main uncertainty is safe relocation of instructions that consume neighboring bytes, entry references, absolute u16 PCs and container boundaries. Prove those through synthetic fixtures before broad UI work.

Architecture: Extend xem-studio field graph/encoder operations, not a second bytecode model. Existing CommandList/GraphCanvas/ActorLanes consume the same canonical draft. Shared service owns edits, validation, compilation and package preparation. Reuse packages.rs, packed.rs, disc.rs and the M5 delta builder; runtime game logic and src/decomp stay unchanged.

Boundaries:
- No main-checkout writes, push, PR, merge, destructive cleanup, Claude contact, trust changes or unrelated milestone.
- Preserve main PR46 files and hashes recorded in tmp/studio/milestone-5/final-source-state-98968a22.json.
- No game originals/derived payloads in tracked artifacts. Existing MIT/provenance rules apply.
- New UI implementation waits for HUMAN approval of concrete mockups. Coordinator generates with built-in image generation only. Do not use ComfyUI or treat agent-generated form replies as approval. Backend planning/proofs may proceed independently.
- Keep value editing inline in the persistent inspector; do not regress previous click-blocking fixes or convert it to modal editing. Reuse registered shared controls and provide keyboard/mouse paths.
- Read home/repository AGENTS, docs/agent/README.md, plan.md relevant phase, docs/matching.md, milestone2/3/4/5 docs, widget registry and field-events primary docs.

Implementation sequence owned by existing Sol/high:
1. Write concrete implementation plan with exact affected files, supported edit contracts and format limitations. Inspect field/{mod,decode,graph,structure}.rs and service lib.rs first. Reuse M2 lossless encoder for unedited content.
2. Have Luna/high write and run focused failing tests under one bounded systemd scope. Demonstrate edits/new nodes/new events without UI. Preserve immutable base/source identity and draft on refused transaction.
3. Implement graph transactions, typed operand/target validation, stable node/event identity and undo/redo as shared commands. Distinguish initial byte-identical graph from authored edits. Reject stale revisions if concurrent/delayed edits can overwrite newer work.
4. Implement append compilation with valid trampoline/out-and-back behavior and updated entry/reference mappings. Preserve pinned map-entry/gap regions, paired setup/consumer instructions (10/11,57), multi-exit handlers (9a,d4/fc,ext18/c6), a6 jump tables, overlaps and u16/size limits. Unsupported relocation must fail with precise diagnostics before mutation; never guess executable successors.
5. Repack only affected field components through supported packing contracts and emit deterministic M5 deltas. Project/draft save+reload retains authored edits and names; failed validation/build/write preserves prior draft and last valid output.
6. Submit UI requirements/layout to coordinator for generated mockup and human approval. After approval, integrate edit/insert/new-event, validation, compile/package/play into existing list/graph/inspector. Both views show the same edits. Persistent diagnostics identify the failed operation and cause.
7. Independent correctness review, focused integration checks, real-disc edited-event playback on native and browser, local scoped commits and final evidence. Stop at M6 acceptance for coordinator review before M7 instruction.

Acceptance matrix:
- No-edit decode/encode remains byte-identical; existing M2 both-disc round-trip evidence remains valid or rerun affected checks.
- Synthetic edits cover ordinary operand edit, inserted/new node, new event entry, branches/loops/cross-event references, length growth, valid relocatable compound units, boundary PC/overflow, unsupported/pinned/overlapping regions, malformed inputs, invalid target/index and failed transaction preserving draft.
- Undo/redo and save/reload preserve edits, stable bindings and unrelated content. Compiler deterministic for same draft/base.
- Native and wasm Studio services compile identical bytes and return matching diagnostics. M5 packaging/load identity remains unchanged in semantics.
- Real fixture: user edits an existing event, adds a node and creates a reachable new event through supported service/UI; build package then trigger behavior legally in both native and browser. Verify intended decoded behavior, runtime observable result and readable screenshots where visible. Pixel inequality alone is insufficient.
- Include an untouched map in the same package fixture and prove byte-identical decoded file bytes against original; unchanged components and absent/disabled replay remain original.
- Browser end-to-end exercises list/graph synchronization, selection, inline typing/commit, insertion/new event, undo/redo, validation rejection, save/reload and package/play. Check meaningful responsive/focus/accessibility paths; real native link and browser playback use identical package bytes.
- Run pinned Rust tests/wasm parity, TS/Vitest, source/archive/provenance checks, relevant browser/NW smoke. One outer scope MemoryMax=16G/MemorySwapMax=0, finite workload-appropriate timeout; preserve all failures/skips/exact inputs/commands/exits under tmp/studio/milestone-6/.
- Runtime input changes require full suite once final inputs stabilize per AGENTS. Preserve prior compatible results and distinguish copied harness modifications and focused retries from canonical full execution.
- Final clean worktree, exact local commits, protected hashes, actual remote unchanged, usable TLS LAN preview restored. Document how to edit/package/play and precise unsupported operations.

M7 next (not implementation authorization until M6 passes): three.js field scene with supported geometry/actors/collision/triggers/entrances; direct selection synchronized with properties/scripts; supported add/duplicate/delete and transforms; actual authored placement versus runtime pose; transactional undo/redo, safe draft save/reload and cross-host packaged playback. Decode required palettes/images/triggers/view block first; no guessed editable geometry or unsupported actor-count maximum (32 event slots per actor is not 32 actors).
