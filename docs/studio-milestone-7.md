# Studio milestone 7 — Field scene acceptance and implementation plan

Baseline: accepted M6 `d9598adea55d40c8e3005dad11b030d2e54f0100`, branch
`modding-toolkit`, worktree `/home/bart/xenogears_toolkit-modding`.
The parent independently accepted M6. Its original full-suite exit 1, focused
passing corrections and independent editor-layout screenshot limitation remain
historical evidence, not a newly green full-suite claim. No denied M6 capture
is retried by this milestone.

## Result and authority

Deliver the roadmap's three.js field scene: supported original geometry,
actors, collision, trigger zones and entrances, with direct selection linked to
inline properties and actor/event bindings. Supported entities can be added,
duplicated, removed and transformed transactionally; saved projects compile to
M5 packages through M6's measured-source preservation contract. The same package
must visibly/functionally execute in native and browser hosts. A live pose is
observation, never an implicit authored placement.

This is a substantial format, domain and UI milestone. Highest uncertainty is
original mesh/image interpretation, script-derived placements/entrances and
reference-safe structural changes. Decode and test those facts before exposing
editable controls. An unsupported operation must return a persistent explicit
reason; listing it as unsupported is not completion of an accepted requirement.
Material format blockers are reported to the coordinator with source evidence.
No invented geometry, asset, bounds, fixed actor maximum or zero padding.

Human M7 mockup approval is required before new UI implementation. The parent
alone generates mockups using built-in imagegen. M6 approval does not satisfy
this gate. Backend decoders, transactions and protocol proofs proceed separately.
No new copied RPG Creator source is planned: reuse already licensed registered
controls and newly authored scene/domain code. Any new extraction requires an
actual grant check and exact provenance, not an expanded inference.

## Primary facts and sources

- `src/decomp/src/field/field_load.h`: nine component identifiers, bundle sizes
  at 0x10c, offsets at 0x130, 28 signed view words at 0x154, u16 descriptor
  count at 0x18c and 16-byte records at 0x190. Records contain flags, three
  rotation words, three position words and a model selector.
- `src/decomp/src/field/field.c::field_load_from_bundle`: actual count/offset
  tables for palette/model/image blocks; descriptor words are consumed as u16
  (do not assume signed positions from the field names). Sprite descriptors
  have flag 0x40, reset descriptor rotation and are initialized through actors.
- `tools/analysis/field.py`: existing nine-component/event/collision analysis.
  Collision has 1–4 layers, 14-byte triangle records and 8-byte signed vertices;
  adjacency and attributes require reader-qualified interpretation.
- Component 8 contains four signed xyz corners per 24-byte zone. Actual event
  zone readers establish selectors, containment and references. Entrances and
  authored actor placement must be recovered from actual event readers and
  control flow; a destination or initial live pose alone is not a placement.
- M6 `xem-disc::field_context` and `xem-studio::disc` own source identity,
  complete decoded outputs and measured allocation-tail provenance. Component
  logical lengths are not full original runtime outputs. Preserve all nine.
- `docs/scripts/field-events.md`, generated opcode/access metadata and the
  original handlers remain the source for event operands and selectors.
- `tools/studio/docs/widgets.md` and `PROVENANCE.md` own shared controls/grants.
  `docs/later-phases.md` identifies palettes/images/zones/view as prior gaps;
  its gap list is not proof of new decode semantics.

## Ordered checkpoints

1. **Format preflight and public contract.** Inspect exact original readers,
   document offsets, units, signedness, flags and references. Implement bounded
   pure decoders for header/view/descriptors, collision, zones, mesh groups,
   palettes and images. Preserve opaque/reserved regions and source spans.
   Add a capability table naming exact editable forms and refusal reasons.
   Inspect existing original inputs in place; outputs stay ignored/local.
2. **Synthetic decode/encode proof.** Luna owns test coding/running. No-edit
   whole-bundle round trip is byte-identical; edited fields change only claimed
   spans or rebuilt tables. Truncation, overlap, unsupported primitive, invalid
   selectors and arithmetic overflow are errors before allocation or mutation.
3. **Authored scene domain.** Rust owns immutable source identity, stable
   source-derived entity IDs, monotonically unique authored IDs, revisioned
   atomic actions and undo/redo. Export/import persists allocated IDs, history
   and allocator watermark. Reject stale revision, reused IDs, malformed history,
   out-of-range transforms and dangling references atomically. Preserve unknown
   bits/bytes. Reuse the M6 authored-event document for script placements rather
   than maintaining an independently editable opcode model.
4. **References and compilation.** Establish typed descriptor/model/actor/event,
   collision adjacency, zone-use and entrance bindings. CRUD either repairs all
   proven references in one transaction or rejects precisely; unresolved dynamic
   selectors cannot be guessed. Script-controlled placements expose the owning
   stable instruction/event; ambiguous conditional or variable-derived placement
   remains explicit. Supported transforms use original integer/range/angle units;
   scale appears only where an original field/handler supports it.
5. **Package/project backend.** Integrate scene and event edits in one source-
   qualified project revision. Reuse strict packing, measured source context and
   M5 delta preparation. Verify every unrelated component's complete output,
   unrelated header bytes and untouched-map index/file bytes. Deterministic
   builds retain the last good output when a new draft fails. No original disc
   writes. Source-context acquisition is project dirty state as in M6.
6. **Approved scene UI.** Only after actual human approval: three.js viewport,
   layer/entity tree, picking, move/rotate manipulation, numeric fields, inline
   Apply/Cancel, source/runtime indication and script navigation. Selection is
   synchronized with properties and existing List/Graph/Lanes. Drafts protect
   selection, project/disc/map changes and Alt-history. Delayed load/save/build
   completions check current project/session/source/revision generations before
   adoption; an unapplied inline edit cannot be silently lost.
7. **Real acceptance and review.** Save/reload an original-map authored edit,
   select and transform it, exercise supported CRUD, build/play exact package in
   native/browser and prove causal visible/functional behavior, absent/disabled
   comparison, full unrelated-component and untouched-map preservation. Astra
   reviews correctness/scope; root integrates and records exact evidence. Local
   coherent commits only, no automatic later milestone or publication.

## Visual brief for parent mockups

Keep the approved dock shell and left disc/map browser. Add a central Scene tab
beside List/Graph/Lanes. Scene toolbar: Select, Move, Rotate; coordinate space;
frame selection/home view; layer visibility; Undo/Redo; validation/build state.
Inside the scene show original supported textured geometry, collision wireframe,
zone outlines and bound entrance/actor markers with a small orientation axis.
Never depict an undecoded object as guessed editable mesh. Unsupported content
has an explanatory diagnostic/list entry.

Use a compact left scene hierarchy grouped Geometry / Actors / Collision /
Zones / Entrances, searchable and keyboard navigable. The right inline properties
panel shows stable ID, original source span/index, supported capabilities,
Authored versus Live (read-only overlay), numeric xyz/rotation, binding selector
and Open script. Labels sit above inputs. Dirty field edits expose Apply/Cancel;
selection changes preserve fields until explicitly applied/cancelled. Add,
Duplicate and Delete act only on supported kinds and show reference diagnostics.
Bottom validation/build identity and existing Backend/Log remain available.

Proposed shortcuts: Q select, W move, E rotate, F frame selection, Delete remove,
Ctrl+D duplicate, Ctrl+Z / Ctrl+Shift+Z history, Ctrl+S save, Ctrl+Enter apply,
Escape cancel an active manipulation/field edit. They yield to text entry and
existing dialogs. Orbit/pan/zoom have labelled mouse help; manipulations commit
one atomic action per gesture, not one history item per pointer event.

Reference artifacts (existing, not new capture requests):
- `tmp/studio/milestone-5/checkpoint-64/screenshots/shell-1440x900.png`
- `tmp/studio/milestone-6/mockups/parent-approved-20261023/command-list.png`
- `tmp/studio/milestone-6/mockups/parent-approved-20261023/flow-graph-invalid-draft.png`
- M6 approved image provenance is adjacent in `provenance.md`.

Suggested mockups: populated authored scene with selection/inline transform and
script binding; invalid/unsupported/reference-conflict state retaining last-good
build and a read-only live overlay. Illustration names/counts are not format
requirements. M7 UI approval is currently pending.

## Acceptance and verification

- Synthetic byte-identical no-edit round trips; source spans/opaque bytes retained.
- Supported CRUD/transform/reference repair, atomic rejection of invalid, dangling,
  unsupported and overflowing edits; undo/redo and branch-history export/import
  preserve IDs, bindings and compiled bytes. Explicit actor/event bounds come from
  the decoded source and runtime consumers, never the 32 event-slot count.
- Native/WASM exact command-envelope, bytes and diagnostics parity. Reuse actual
  binary range-fill transport for disc-backed acquisition; no identity bypass.
- Browser after approval: picking, manipulation plus numeric entry, focus/keyboard,
  property/script synchronization, authored/live indication, dirty lifecycle,
  delayed I/O ownership, history/save/reload and deterministic build.
- Actual both-host package playback with readable visuals and causal functional
  observations, not merely pixel inequality. Disabled/absent comparisons and
  complete unrelated component/untouched-map preservation are independently
  evidenced. Private local inputs are env-gated; missing inputs are skips, not
  passes. No game bytes or keys in logs/public fixtures/commits.
- Focused Rust, WASM, TS/Vitest, design, source/provenance and applicable NW checks
  in pinned Nix; Luna runs tests inside one outer user systemd scope with
  MemoryMax=16G, MemorySwapMax=0 and measured finite timeouts. Record exact source,
  command, scope, input hashes, exits, skipped/downstream-not-run stages in fresh
  ignored `tmp/studio/milestone-7/` artifacts. Preserve failed attempts. Inspect
  changed harness syntax before expensive runs. Reuse unaffected M6 evidence.
- Full runtime suite once only if M7 changes runtime inputs; no blanket replay of
  M6's accepted path-adapted full run or its passing focused corrections.
- No new UI before human approval; no source/decomp or prompt edits, main-checkout
  work, push/PR/merge, cleanup/GC, trust/security changes, credential disclosure,
  Claude contact or unrelated milestone. Do not stop existing scopes/processes.
- Recheck protected main PR46 hashes against M6 final-state JSON; retain five dirty
  files unstaged/uncommitted. Monitor disk headroom, stop heavy launches if low.
  Add all new authored/public generated files to sorted source-files allowlist.

## Initial preflight checkpoint (2026-10-23)

All five protected main-checkout hashes match M6's final-state record; evidence
is in ignored `tmp/studio/milestone-7/preflight.json`. The terminal status helper
refused an unavailable verified binding and changed no tab. M7 UI remains gated.

The first implemented primitives are source-identified header decode/transform
patching and zone decode/encode retaining complete tails. Nine synthetic format
cases passed at checkpoint 02 after checkpoint 01's test-only compile errors;
that earlier attempt is retained. A broad rustfmt check in checkpoint 02 also
reported the concurrently authored texture module before its format checkpoint;
this was not a functional format-test failure. Header descriptor positions remain
raw unsigned loader words; actor-floor lookup uses their signed low halfwords.
Sprite rotation patching is refused because the original resets it. Structural
CRUD and script placement resolution are not yet implemented or accepted.

Independent source review established entrances as seven-byte records beginning
at bytecode+1 after an ff marker, with dynamic camera/facing selectors; there is
no source count field. It also established that field-model pointers are group-
relative with the SpriteModel view at group+0x10. These facts constrain subsequent
transactions; they do not justify treating every pinned prefix byte as an entry.
Texture reads use complete verified output (including a supplied tail of at most
16 bytes), never invented allocation bytes. Shared sprite slots are explicit
inherited-VRAM dependencies. The service's expanded texture output budget is
4096 uploads / 8 Mi 16-bit words, a work limit rather than a source entity maximum.

The view record supplies lights/background, not a static camera eye/target.
Viewport orbit/home controls affect editor navigation only; a game camera edit
requires a separately proven script binding. Original field coordinates are
Y-down; the scene adapter must display its conversion and preserve source units.

Collision attributes are independently addressed and can precede triangles;
there is no stored complete vertex count. Decode the minimal prefix required by
actual nonnegative signed vertex indices, preserve unused bytes, and validate
neighbors against each layer. Preserve byte +13 separately from attribute byte
+12, including its source-observed signed height-bump interpretation. Some original
walkers read triangle[-1].attribute before checking the -1 sentinel; relocating
a table must preserve that measured pre-table byte instead of inserting zero.

Geometry expanded work includes primitive-group counts, not just face/model
counts. Gouraud preparation reads raw signed normal indices independently of
draw-index masking and optional header flags. Conditional lighting dependencies
that cannot be qualified remain diagnostics; draw geometry is not proof of an
original lighting read's safety. These checks are required before rendering.

## Verified decoder foundation checkpoint

The composed `field.scene.decode` operation is implemented through the existing
JSON envelope. Checkpoint 11 passed 49 regular command/scene cases, then explicitly
ran the synthetic trace writer and private per-mesh oracle comparison (the two
otherwise ignored tests). Its outer exit 1 is retained because new test-fixture
Clippy style findings remained. Style-only corrections passed strict Studio
all-target Clippy, formatting, source archive and diff checks at checkpoint 12;
no functional test was repeated for those corrections. Earlier setup/compile
failures remain separately logged.

Checkpoint 13 passed production-default WASM build/bindgen, frontend typecheck,
isolated test build and both focused browser cases: exact native/WASM replies
(including malformed maximum offsets/counts) and the existing backend discovery
view. The owned HTTPS/LAN preview and its generated backend package were not
changed. The synthetic test server used a separate loopback port, sandbox enabled,
with no disc/game directory inheritance or trust/Node permission changes.

The private oracle comparison covered every mesh/face/vertex/auxiliary/control/UV
field of 35 geometry groups / 35 meshes from the preserved M6-derived map23 result
bundle SHA-256 `81fccce57a6b83c9057caa978dba90bfc18cd54833b99322e379069c57a2f2a5`.
Component 2 logical 111644 / full 111647 bytes matched the Python reader, full-output
SHA-256 `342ea92438e49fed0b7d5e5ab617b5fcf64205f5bbc056bd17d38188f7d1f8f4`.
This is a reader/format preflight on an M6 artifact, not M7 original-map authored
package or playback acceptance. Source identities and exact commands/scopes/exits
are retained in ignored `tmp/studio/milestone-7/verification-index.md`.

Domain transactions/history, stable structural reference repair, project ownership,
new UI and M7 native/browser edited playback remain unfinished. Human M7 UI
approval remains pending. No M7 full runtime suite has been run: these changes
are standalone Studio code/tests, with game module/host inputs unchanged.

## Domain implementation contract

The decoder checkpoint is local commit `f1c57f14`; subsequent work adds a separate
revisioned scene document over that immutable input. Scene entity IDs are source-
qualified; new IDs and allocation watermarks survive undo/alternate history and
export/import. Batch validation and compilation happen on a candidate state
before adoption. No failed edit/import may replace current state or last-good
output. Project saved-state ownership includes measured context as in M6.

References must use proven operand roles, preserving party/self selectors and
exposing unresolved cases. Source-proven actor/zone binding metadata is generated
from canonical opcode descriptors and exact C-reader/macro proofs, rather than
a second registry. Actor selector bytes that overlap another operand (such as a
variable address) cannot be automatically renumbered if that changes the other
meaning. Raw-plus-normalized selector reads require both interpretations to
agree. Unknown code/data aliases block affected structural edits precisely.

An internal virtual actor-row namespace can retain deleted actors' source code
and authored-node identities while final compilation emits only active rows and
a corresponding descriptor order. Existing PCs stay bytecode-relative; typed
literal references map through stable entity IDs to final indices. Unreachable
source storage remains intact. Source-aware script edits reuse the M6 Draft
compiler and its tail-activation checks; stripping/reordering actor rows must
not activate new code or silently redirect script bindings.

Collision topology changes append validated tables, retaining original opaque
storage and exact pre-table sentinel bytes. There is no invented full vertex
count. Header/table pointers and known references change together. Static model
instances reuse decoded source groups; editing an unknown mesh/morph encoding
is unsupported. Actor placement is either an explicit header source or a proven
script binding; floor-derived Y and transient runtime pose remain distinct.
Entrance record counts/reference closure remain unresolved where source facts do
not establish them; same-selector edits do not authorize destructive renumbering.

The first domain checkpoint deliberately implements a bounded subset while the
remaining accepted operations are developed. It is not an M7 completion claim.
Current actions use source descriptor transforms, copies of proven static model
instances, zone CRUD, same-layout collision vertices and explicit same-selector
entrances. Candidate compilation reuses M6's instruction compiler and shared
packing-tail activation refusal; its public event protocol is unchanged. A new
static instance reuses a source model/template rather than inventing compatible
morph/material flags. Actor event-row CRUD and reference-safe collision topology
are separate subsequent checkpoints. The public scene command remains decode
only until the transaction contract is coherent and parity-tested.

The stable collision entity checkpoint adds per-layer vertex/triangle IDs,
source-derived references, opposite-edge joins, explicit dangling-reference
refusal and history/import proofs. Structural writes append tables and preserve
the exact original pre-table record. Independent review found a further conditional
read: `field_motion.c:920–935` can compute a plane from triangle[-1] before checking
its sentinel when attribute bit 0x400000 applies. Such layer relocation is refused
until those vertex references can be closed; copying only its attribute or fourteen
bytes does not establish that preservation. Same-layout transforms do not reindex
those references. No generic bridge or guessed boundary triangle is inserted.

The shared JSON operation is `field.scene.draft`, with a tagged kind for document
open/import/edit/history/snapshot/compile/export/close. This keeps one new typed
transport surface while the actor/script operations are developed. Source-owned
scene handles have a four-document residency budget, distinct from event drafts.
The original M6 source-memory context is retained in export/import. New UI, guarded
frontend persistence and actual M7 authored gameplay remain acceptance work after
the human mockup gate. Existing M6 visual limitations remain historical caveats.

## Original-source and actor-row checkpoint

The gated original Disc 1 / map 23 preflight identifies the actual image through the
production disc session, verifies its indexed bundle and historical measured
context, then exercises the public scene decode/document commands. It proves
byte-identical no-edit compilation; a static descriptor 25 transform preserves
all nine complete component outputs and every unrelated indexed byte. Its only
suffix is the exact five-byte measured source guard. Export/import reproduces
the candidate SHA-256 `3f1120edd015a016b2c7d414bbb176f4f61aa0197089020b710a49769da1f89d`.
This is offline asset/transaction proof, not a package/gameplay or UI pass.
The failed initial length assumption and failed synthetic fixture attempts are
retained separately in ignored checkpoints26–28.

The pure actor-row container primitive preserves full original bytecode/PCs and
logical-versus-stored tails while rebinding source entry rows, with synthetic
56-row coverage. Actor 0 retains its original identity/index because engine
callbacks invoke its events 2/3. A template row shares 32 source entry PCs with a
new current-actor context; this is not an independent script clone or proof of
unchanged global initialization effects. Domain actor CRUD is still unfinished.

Further source review identified references outside the canonical actor-byte
operands: `field_event_begin_effect` reads raw/variable iv@1, which reaches emitter
`unk52` and descriptor reads in `field_effect.c`. Until those bindings are qualified,
structural descriptor changes in such sources return an atomic explicit refusal;
same-namespace transforms remain supported. Party registration is another relevant
constraint: event0 runs for every actor in row order, and `become_party_slot` /
`become_party_character` publish current-actor ownership. Actor duplication must
qualify those initializers rather than infer safety from copied entry pointers.

The next integration checkpoint composes the existing M6 authored-event document
with scene actions under one source-qualified project. Existing authored script
edits must survive a scene transform/build/save/reload. Reuse M6 compilation and
stable bindings; do not make its compiled bytes the new original source or create
another event encoder. Then add qualified script-placement/actor transactions,
with explicit shared/dynamic/global-effect refusals where closure is not proven.
M7 UI approval remains pending and no new scene UI has been implemented.


The first M6 composition checkpoint kept its schema1 event document as a required
subdocument of scene export schema2 (superseded by schema3 below). Scene open can receive an existing event
document and validates its base against the complete original component before
replaying it. Reference repair maps the initial compiled PC back to the M6 stable
node ID, then reuses that existing compiler/state. This is distinct from adopting
compiled bytes as the original source. Omitted open input means an unedited M6
document; incomplete/old scene exports are rejected explicitly. Source-relative
`modified` includes initial authored M6 history, while project saved ownership
remains a separate revision/generation check. Tests for source mismatch, moved
zone references, export/import and portable malformed subdocuments accompany the
integration before new UI or further actor/script operations.

## Qualified independent event-copy primitive

`scene::clone_event::prepare` produces M6 `NodeInput` templates from a qualified
event, with address operands bound to monotonically allocated authored IDs.
Literal selectors retain their original meanings; no literal is guessed to mean
self. It refuses paired/inline/fixed-relative/pinned/wrapped storage, layouts whose
entry is not the first copied unit, party-owner initialization, and every implicit
continuation outside the copied set. The subsequent M6 transaction validates
the destination slot and final u16 layout. This is not domain actor CRUD or a
claim of arbitrary script purity. Five focused cases passed at checkpoint33; its
terminal-case assertion and manifest-order failures remain recorded. The corrected
terminal case and gated strict lint/archive checks passed at checkpoint34. No
new UI, original-disc playback or host acceptance is claimed for this primitive.

## Ongoing scene-owned event transactions

The next backend checkpoint advances unreleased scene exports to schema3 with
required per-action `event_allocation_starts` and global `next_event_id`. M6's
own schema1 stays unchanged. `Action::Event` reuses M6 transactions inside the
same scene history. Undo/redo replays original actions with recorded allocation
starts; alternate edits reserve the retained watermark before allocating.
There is no second event bytecode encoder or adopted compiled-source graph.

A cache binds stable M6 node/operand identities to source-qualified scene entity
IDs. New or explicitly edited selector bytes (including overlapping operands)
are captured against current scene namespaces. Untouched references retain
their identities across relocation/renumbering. Unreachable bindings are kept
for later reachability restoration; dangling or unresolved new bindings reject
the candidate. Captures share namespace storage per edit instead of allocating
a full namespace for each operand. Scene snapshot/build use the same compiled
event stream, including entrance repairs. Event state is shared immutably across
non-event actions; only a private candidate is cloned for a script edit.

These backend operations passed the focused history/reference checks and
71-step native/default-WASM exact raw-protocol replay in checkpoints35–40.
Earlier failed compile/launch/lint attempts are retained separately. Actor-row structural integration, qualified
script placements/entrance CRUD, project UI and original-map package/gameplay
acceptance remain unfinished. Human M7 mockup approval is still required.

## Explicit script placement contract

`field.scene.draft` kind `placement` inspects a stable node; `set_placement` is
a revisioned scene action. Qualified source writers are `field_event_place`,
`place_on_layer`, `place_at_height` and `set_position`. Canonical opcode metadata
still owns their forms/operands/flags. The adapter exposes source/world xyz even
though these handlers encode x,z,y. Signed immediate coordinates, variable
references and collision-derived floor Y are distinct. Floor placement supports
immediate X/Z changes while Y remains read-only; variable coordinates cannot
be silently converted by a viewport gesture. A requested explicit layer must
exist in the decoded 1..4 layer namespace.

The shared graph index reverses only its existing followed edges, with a visited
set, to enumerate every actor/event owner. A targeted single-actor transform is
refused when the instruction has multiple actor contexts. The selected stable
actor/event must actually own that instruction. Original M6 source writability
and current pinned storage also constrain the reported capability; a relocated
PC does not erase its source restriction. No first/last writer or runtime
execution/floor result is inferred. In particular, `place_at_height` changes
actor Y after floor placement; descriptor/model pose can differ.

Preparation produces an existing M6 batch with only the selected fields, keeping
unrelated instruction flags and code. Scene validation/undo/history/persistence
remains the sole transaction boundary. This backend extension passed8 placement,5 structure and7 event-history cases,
strict Studio lint/source checks, and94-step exact native/default-WASM protocol
replay at checkpoints41–45. Checkpoint44's skipped downstream stages and misleading
outer0 are explicitly retained as a harness failure. These are not UI or
live-pose/gameplay acceptance.

## Actor transaction implementation direction

Consumer-derived runtime qualifications constrain supported actor creation: the
sprite callback stores a signed16 index (`SpriteSequencer.actor`, `sprite.h:166`,
`field.c:2487,2570`). Actor self selectors resolve0xfb to the current actor, but
several consumers treat resolved0xff as absent (`field_event.c:8681–8695,9046,9434`).
`ridden_actor` and linked-actor+75 are bytes with sentinel/truncation concerns
(`actors.h:70`, `field_motion.c:3598,3654`, `field_event.c:3092`). Generic actor
creation therefore needs indices0..254; this is a qualification of that operation,
not a universal actor-count format limit. Existing wider original tables remain
losslessly inspectable; no32-actor assumption is introduced.

The next actor implementation must retain the original source bundle/M6 base and
explicitly derive a private virtual event-row namespace, growing only metadata
over identical original code. Original rows and actor0 stay fixed in that
namespace; final runtime rows/descriptors are rebuilt together from stable active
actor IDs. A proven original end-slot instruction seeds empty virtual rows.
Existing M6 actions are replayed with the same IDs/allocation starts against that
derivation; compiled bytes never become original input.

Actor duplication should independently copy qualified event code through the M6
NodeInput/target primitive, preserving explicit literal targets and symbolic self.
Copies capture literal bindings against the pre-insertion namespace before
static descriptors move. Paired/inline/relative/unclosed or party-owner scripts
remain explicit atomic refusals until separately qualified. New scene event
operations address stable actors rather than exposing transient virtual row
indices. Final compilation includes only active rows for reachability/reference
closure while retaining original storage, contexts and monotonic IDs/history.
This direction is not implemented or accepted yet; new UI remains human-gated.
