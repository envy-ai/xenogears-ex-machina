# Studio milestone 5 — Runtime mod packages

**Goal:** Complete the next unfinished fork milestone: deterministic validated mod packages consumed by native and browser runtimes, with a reusable delta builder and explicit save/state compatibility.

**Owner:** Existing Sol/high session 01a15b56-4f8d-7d01-940a-ee0348d29bd9. Use the established Sol implementation, Luna/high tests, Astra/medium correctness-review workflow. This is the coordinator brief; Sol owns the concrete implementation plan after source preflight.

## Baseline and boundaries

Work only in /home/bart/xenogears_toolkit-modding, branch modding-toolkit. Verified clean baseline cd2686e215f6d23a1c92a1305679a27639d0d3f6 includes M4 d550f037 and 45000ff9. The authorized accompanying docs/modding-toolkit.md change expands M7 requirements; do not implement M7, M6 editing, Lua, presentation replacement or the wider Phase 11 authoring bridge.

Read /home/bart/AGENTS.md, repository AGENTS.md, plan.md Phase 11, docs/modding-toolkit.md sections Mod packages/Milestones/Risks, and relevant runtime documentation before coding. No changes to src/decomp/, prompt.md, original disc images, credentials, trust/security configuration or the separate main checkout. No push, PR, merge, destructive cleanup or later milestone. Preserve main checkout PR46 files: docs/runtime.md, src/runtime/web/main.js, src/runtime/web/serve.mjs, src/runtime/web/tests/disc.test.mjs, src/runtime/web/tests/serve.test.mjs. Changes to the corresponding paths in this modding worktree are permitted when required by M5.

Effort is substantial shared-runtime integration. Main uncertainties are raw-sector overlays and embedded index copies, browser demand reads/cache identity, and package identity across ordinary saves, snapshots and replay. Resolve those in source preflight and focused fixtures before broad integration.

## Required result

- [ ] Versioned package format replaces game file-table indexes and declares applicable disc identities. Reject malformed/unknown contracts with useful errors. No executable package scripts.
- [ ] Builder in the shared Studio backend emits binary deltas against user-owned files and permits wholly authored files. Never bundle original assets verbatim; package artifacts and real-disc derivatives stay ignored locally. Check source hash before delta application and result hash afterwards. Document delta format, deterministic bytes, limits and build/load entry points.
- [ ] Deterministic manifest load order, declared dependencies and explicit override relationships. Duplicate IDs, missing dependencies, dependency cycles, wrong disc, malformed delta, wrong hashes, overflow and undeclared same-file conflicts fail before publishing an active package set. Explicit override result must be deterministic and tested.
- [ ] Shared virtual-disc implementation serves replacements on every host through shared code. Preserve vanilla source exactly when absent/disabled. Size-changing replacements occupy virtual sectors beyond the original image; update the actual index copies consumed by the game, not merely a UI table. Keep source images read-only. Validate 24-bit LBA and signed-size limits, sector form/padding, partial/cross-sector reads, and overlapping intervals. Reject unsupported media/file layouts explicitly rather than fabricating sectors.
- [ ] Native and browser receive the same prebuilt package bytes and expose active ordered package IDs/hashes through their common command/status contract. Browser loading works without Node, native plugins or privileged paths, and demand reads/cache keys cannot reuse bytes from another package set. Use startup/session replacement for changes unless existing architecture demonstrates safe atomic switching; never silently change content beneath an active timeline.
- [ ] Ordinary saves, snapshots/rewind and replays retain ordered package IDs/hashes and refuse incompatible restoration/playback explicitly before state mutation. Preserve original-save import/export semantics: do not rewrite original game/card structures to invent metadata; document and validate any wrapper/sidecar boundary and vanilla import policy in the implementation plan.
- [ ] Package format/loader/delta builder are usable through supported CLI/shared service entry points. This runtime milestone does not itself require new graphical management UI. If new UI becomes necessary, generate built-in-imagegen mockups first and obtain Bart approval via parent before implementing that UI. Do not use ComfyUI or infer approval from agent text or queued messages.

## Source preflight and implementation sequence

1. Inspect actual file-index consumption and sector formats: src/runtime/crates/xem-disc/src/{index,sector,source,read_at,cache}.rs, plus port disc reads. index.rs records a 0x8000-byte 7-byte-record index at LBA24, directory table LBA40, and embedded boot EXE copies. Prove which views must change with a synthetic size-changing replacement before selecting representation.
2. Trace native and web disc adapters through xem-host/src/game.rs, xem-game/src/native.rs, xem-web/src/disc.rs and src/runtime/web/{disc,game,xem}.js. Choose one reusable decoder/validator/overlay core; avoid parallel native/JS package semantics.
3. Trace persistence through xem-core/src/{snapshot,rewind,control}.rs, xem-game/src/storage.rs, xem-storage/src/lib.rs, host/web storage and replay. Record affected archive versions and rejection-before-mutation behavior. Do not broaden this into unrelated archive migrations.
4. Write focused failing synthetic package/delta/index tests, then implement shared validation/overlay and expose builder operations through xem-studio/src/lib.rs plus focused modules. Test modified sizes, unchanged files and exact vanilla sector passthrough before host wiring.
5. Integrate native/browser package acquisition and common status/persistence boundaries. Test invalid loads leave prior session/content intact and errors retain original context. Update source allowlist, relevant docs and dependencies provenance when needed.
6. Demonstrate a legally entered field message changed in both native and browser, using a small local package built from user-disc data. No M6 editor is required for this fixture. Keep all source assets/local generated payloads ignored and show package structure contains deltas/authored content only.
7. Run focused and integration acceptance below, review independently, commit scoped verified increments locally and restore the owned existing LAN preview if stopped. Stop after M5.

## Acceptance and evidence

Use pinned nix/runtime and one outer systemd scope with MemoryMax=16G and MemorySwapMax=0, finite proportional timeouts and no nested scopes. Luna/high owns tests. Preserve failures and report skips accurately under tmp/studio/milestone-5/. Preflight harness environment/paths before expensive runs. Record source revision, build/module/package/disc identities, exact command, exits and artifact paths.

- [ ] Shared Rust synthetic tests: deterministic package build; round-trip application; wrong source/result hashes; malformed/truncated/oversized delta; all dependency/conflict cases; record/sector overflow; first/last/cross-sector reads; different-size appended file and actual game index view; unaffected files/vanilla sectors byte-identical.
- [ ] Native/browser fixture parity consumes identical package bytes, ordered package status and replacement file results. Browser tests cover missing ranges, reconnect/new package generation, package-load failure and cache invalidation.
- [ ] Persistence tests: matching package set resumes; altered hash/order/dependency or absent package refuses save/state/replay without mutation; disabled/absent vanilla compatibility is explicit. Exercise ordinary save wrapper and snapshot/replay boundaries separately.
- [ ] Real-disc acceptance: changed message is visibly rendered in both hosts via supported control/interaction, with screenshots/assertions and original-versus-modified text evidence. Same replay with packages absent and disabled produces vanilla memory_hash in both hosts. Untouched file reads remain original. Snapshot/package mismatch refuses before mutation.
- [ ] Targeted M4 regression for native/browser attach, deterministic frames, schema identity and snapshot/restore because shared command/session contracts changed. Reuse unrelated valid M3 evidence with its identity clearly stated.
- [ ] Required full runtime suite once final shared runtime inputs stabilize, per AGENTS.md; focused tests first. Do not delete old evidence or modify scripts merely to bypass a restriction. If a copied isolated harness is necessary under an existing constraint, document exact changes and distinguish it from unchanged canonical execution; report missing private CHD/XR/device inputs as skips, not passes.
- [ ] Source archive audit, diff hygiene, clean final worktree, exact local commits and protected main-checkout hashes. Final report links evidence index and identifies any supported-format limits, skipped checks or blockers.

## M7 clarification carried alongside this work

docs/modding-toolkit.md now explicitly requires direct scene selection, supported add/duplicate/delete and transforms, properties/script synchronization, authored-versus-runtime placement, transactional undo/redo and validated save/reload/playback. Its old 32-actor assertion is removed: 32 event slots per actor are evidenced, while docs/scripts/field-events.md describes actor56. Do not invent an actor maximum; establish concrete decoder/runtime limits before M7 implementation.
