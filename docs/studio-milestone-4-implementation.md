# Milestone-four implementation checkpoints

Scope and acceptance are owned by [studio-milestone-4.md](studio-milestone-4.md).
Baseline: `3f4e5d175e25722d8a9336de9b1c0ef5c0581468`. Sol/high implements;
Luna/high codes/runs tests; Astra/medium supplies mockups and independent review.
Both mockups received explicit approval before UI implementation. No publication
or later milestone is included.

## Ordered checkpoints

1. **Runtime preflight and mockups.** Inspect actual native/browser
   control contracts, local built inputs and schema/actor provenance. Probe the
   actual hosts with isolated settings/cards. Both mockups were explicitly
   approved by Bart on 2026-10-22; artifacts are under ignored
   `tmp/studio/milestone-4/mockups/`. Existing-browser capability smoke passed;
   module-bound real native/browser parity passed on the same release module/disc
   with four matching status snapshots and memory hashes.
2. **Command ownership and adapters.** Add the narrow missing runtime contracts
   below, then implement serialized native/browser links and a connection-scoped
   controller under `tools/studio/src/game/`. Preserve original errors and exact
   integer tokens. Test framing, loss, ownership and stale completions.
3. **Schema and PC model.** Use only the attached session's schema/identity;
   lazily enumerate typed records, bounded arrays and pointer paths. Delegate
   inspect/write and readback to the runtime. Resolve exact actor/event ownership
   in the existing graph and refuse mismatched components or unavailable PCs.
4. **Approved UI.** Compose docked live controls and inspector from registered
   widgets, explicit selection API and existing persistent errors. Preserve M3
   navigation, names drafts, graph ports and 220px pitch. Keep F6 panel cycling;
   use F7 step and Shift+F6 run/pause as approved in the mockups.
5. **Acceptance and review.** Focused unit/typecheck/design/build/browser/NW;
   real desktop/browser paused replay and command/result/hash parity, snapshot
   restoration, valid/invalid scalar writes and live-PC selection. Runtime
   changes require affected platform checks and one final full suite. Retain
   every failure/skip and exact original commands. Independent review, scoped
   local commits, clean state and protected main-checkout hash comparison.

## Runtime gaps found before implementation

`xem-host::Game::pause()` drops queued audio, but `update()` continues pacing;
there is no persistent native control pause. `xem-desktop --control` also begins
running before the first pipe request. The browser already supports paused boot
and `setRunning`, but its JSON-object command API cannot transport an integer
outside JavaScript's exact numeric range without rounding. `Session` owns its
parsed schema privately; clients can list globals but cannot retrieve types.

Implemented narrow additions, preserving existing defaults and command semantics:

- Native `host_running {running?: bool}` acknowledges/query controls only the
  automatic host loop. Exact shared commands remain usable while paused.
  `--control-paused` starts that owned child paused before its first frame,
  requires `--control`, and does not change ordinary launches.
- Shared `status` additionally reports authoritative runtime mode, inserted drive
  disc and content identity for safe live-PC context checks.
- The normal game-module builder stamps its final wasm SHA-256 into the
  generated schema. Native/browser boot reject a declared mismatch; the new
  read-only `schema` command refuses an absent build binding and returns the
  actual session schema and its runtime module/disc/package identity. Studio never substitutes a separately selected
  schema file; reconnect invalidates the old model and checks identities.
- Browser page status exposes a generation incremented by each successful boot;
  a live link invalidates on generation changes even when API identity is stable.
- Browser `commandJson(text)` exposes the existing raw JSON command boundary
  for exact integer scalar reads/writes. Frame commands still use `run` to
  completion across disc fetches, with the automatic loop paused first.

These are runtime adapter/protocol changes only, not changes to decomp sources,
original game semantics or a new network service. Browser attach remains
same-origin direct `window.xem`; native transport owns only its spawned child.
If existing hosting cannot serve the actual game under the Studio origin,
add an explicit static build-directory mount, without a control endpoint,
CORS policy changes, trust changes or exposure of arbitrary filesystem paths.

## Evidence and material risks

Fresh evidence belongs under ignored `tmp/studio/milestone-4/`. Record exact
commands before execution, source/build/module/disc identities, scope and
stage exit codes. Real-host acceptance cannot be replaced by synthetic tests.
Private inputs are read in place; no game bytes, keys or schema artifacts are
committed. Main PR46 files are hash-protected independently of this worktree.

Potential blockers: absent actual host builds, unsupported runtime scalar type,
missing authoritative actor/event state, same-origin hosting restrictions, or
an unresolved UI decision. Report exact limitations; do not invent a successful
attach, coerce numbers, guess event aliases or drop parity differences.

## Verification discoveries resolved

- An async browser `run` awaiting disc input could outlive its link or session.
  The runtime now checks the captured boot generation and optional AbortSignal
  before subsequent commands/fills; `runFrames` also retains the generation
  across chunks. Studio aborts that signal on loss, close and timeout and
  requires the additive `cancellableRun` capability. Focused deferred-fill
  reboot/cancellation regressions and the final affected browser checks pass.
- Vite removed a bare `process.env` spread from the native transport bundle,
  stripping the owned child's display environment. The launch form now obtains
  `nw.require('node:process').env` explicitly and passes it to the transport.
  NW acceptance uses the actual runtime executable, without a wrapper that
  could hide this production failure.
- Independent review's stale-inspector, connection ownership, identity ordering
  and static-mount containment findings were resolved. The final focused review
  found the async continuation fix complete by source inspection. Original
  failures and commands remain in ignored milestone-four evidence.

The final runtime suite uses reviewed ignored copies selecting fresh output/log
paths and a read-only Android emulator overlay, with cleanup limited to the
recorded owned PID. A verification-only preload enables Chromium sandboxing in
existing runtime tests. These adaptations preserve prior evidence and user data;
no test expectations or required stages are removed. Exact diffs/hashes and the
original invocation are retained under `tmp/studio/milestone-4/`.

## Acceptance results

The real native/browser replay passed with identical module/disc identities,
four matching canonical status snapshots and memory hashes, valid scalar
readback, rejected writes preserving state, and snapshot restoration. Actual
pinned NW.js UI acceptance passed against the directly launched desktop binary.
The real Studio browser journey passed against disc 1, including loading map 30,
selecting an existing instruction from actor 17's live PC, scalar readback and
snapshot restore. Synthetic UI checks cover keyboard/focus, confirmation,
connection loss, reattachment and persistent original errors.

The final runtime suite passed all 13 executed stages in 1,014 seconds. Private
CHD-dependent XR/native validation and Android real-disc import were skipped
because the expected worktree-local CHD was absent; separate M4 BIN-backed real
native/browser and NW checks passed. Both-disc M3 analysis evidence is reused;
this milestone does not change its backend. The runtime run retained its normal
host CPU concurrency: the attempted runtime-only affinity adjustment was
ineffective because the cgroup lacked the cpuset controller. The enforced
16 GiB/no-swap scope recorded no OOM event.

Late verification fixes make unsupported floating-storage semantics explicit,
assign rejected-operation error reporting to the UI once, and preserve a bounded
schema tree viewport when inspection details open. The original failed attempts
remain available alongside their corrected focused checks.

Exact commands, stage logs/exit codes, source/build/input identities, screenshots,
failed attempts and skips are indexed in ignored
`tmp/studio/milestone-4/verification-evidence.md`. This index is local evidence,
not a distributable game asset. No new push, PR, merge or later milestone is
performed. Main-checkout PR46 hashes and decomp/prompt status are checked before
completion.

## Corrective inline inspector handoff

Bart's latest instruction retains persistent inline details and supersedes the
intervening modal-conversion request. No modal conversion was implemented;
snapshot restore retains its existing independent modal. Preserve the real
normal-click/geometry/readability regressions from attempts 25/28.

Unapplied scalar text now holds a requested node change until explicit apply or
cancel. Successful Write & read back follows the pending node; failed writes keep
the original draft/selection/target. Cancel draft discards and follows the target;
Keep editing abandons only pending navigation. Refresh cannot erase dirty text.
Timeline/schema changes retain dirty text for reference without allowing it to be
written into another state. Focused synthetic browser coverage exercises these
normal-click flows, and affected real UI acceptance is rerun. Unchanged runtime,
parity and full-suite evidence remains valid; exact corrective commands/results
are appended to the ignored milestone-four verification index.
