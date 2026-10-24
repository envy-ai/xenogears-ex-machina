# Studio milestone 7 — Field scene acceptance record

Baseline: accepted M6 `d9598adea55d40c8e3005dad11b030d2e54f0100`, branch
`modding-toolkit`. Private verification artifacts stay outside published sources.
The parent independently accepted M6. Its original full-suite exit 1, focused
passing corrections and independent editor-layout screenshot limitation remain
historical evidence, not a newly green full-suite claim. No denied M6 capture
is retried by this milestone.

## Current checkpoint

M7 is accepted for the roadmap's supported-scene scope. Human-approved UI,
shared native/WASM semantics, guarded browser and Windows desktop persistence,
exact-package native/browser playback, untouched-content preservation and reviewed
real-scene framing are verified. Windows NW.js 0.113.0 with supported WebGL2 closed
the final desktop criterion on 2026-10-24. The final checklist, format boundaries,
reproduction details and artifact identities are recorded at the end of this file.
The dated checkpoints below retain historical gates and failed/partial results;
Linux graphics failures are not rewritten as passes.

Current publication direction: Bart has authorized one normal, non-force push to
`origin` (`https://github.com/envy-ai/xenogears-ex-machina.git`), branch
`modding-toolkit`, at a clean validated checkpoint after outgoing-history review.
This supersedes earlier no-push wording in this plan only; no PR, merge or force
push is authorized. A published checkpoint does not establish M7 completion.
The owned LAN preview may select the best verified compatible Xenogears runtime;
TLS/network/security configuration stays unchanged. Its current game is the
accepted M6checkpoint36 module, with served identities checked over strict TLS.

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
requirements. The explicit human approval received below satisfies the M7 UI gate.

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
subdocument of scene export schema2 (superseded by schema4 below). Scene open can receive an existing event
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
This direction is implemented through the existing compiler. Eleven synthetic
actor cases and116-request native/default-WASM parity passed; the original
Disc1/map23 no-edit/static-transform preservation check also passed again at
checkpoint54. This is backend proof, not game execution or UI acceptance; new
UI remains human-gated.

Active actor snapshots expose stable ID, typed original/copy origin and explicitly
private `event_source_row` provenance. Add/Duplicate insert matching descriptors
and independently copied qualified event rows; Delete emits only remaining active
rows. Empty copied slots use the proven original end-slot seed, so `actor_event`
can author a new event by stable actor ID. Public numeric M6 NewEvent rows retain
original-source meaning and cannot silently select a new virtual row.

The private derivation allows an entry-induced block split only after strict
canonical checks on both graphs: original bytes/rows/instructions/diagnostics/pins
and ordered execution edges remain exact. Block IDs are recomputed from the new
metadata. Failed setup/guard/lint attempts and the independent diagnostic are
retained in ignored verification checkpoints46–55. No original source code or
game input was replaced to make these checks pass.

## Unified imported history backend checkpoint

Scene schema4 promotes validated M6 actions onto the same scene timeline, rather
than retaining an editable initial baseline. `event_document` is the required
actually empty original root: schema1, actions/allocation starts empty, cursor0,
next_id1. Required `imported_event_actions` identifies an Event-only prefix whose
entity allocation starts are1 and whose original namespaces remain unchanged.
The per-action event allocation starts, applied cursor, redo and global watermark
are retained. Restore the watermark after validating all actions, then restore
the cursor. Undo/redo can now reach all imported M6 actions; branching truncates
the prefix without reusing either kind of ID. M6's own schema1 is unchanged.

The validated original-namespace prefix retains unresolved targets with original
snapshot `reference_diagnostics`, including unknown wrapper roles and source
zone selectors outside the decoded table. This preserves existing M6 import
semantics without inventing bindings. Later affected namespace changes still
refuse; ordinary new scene edits cannot introduce/touch unresolved references.
Schema4 rejects hidden nonempty roots, malformed prefixes and older scene
exports explicitly. No duplicate history or compiled-source baseline is added.
Checkpoints58/60/61 verify26 regular native tests plus the separately invoked
141-request trace, exact default-WASM reply equivalence,3 transport unit cases,
typecheck/build, strict Studio lint, formatting and source boundary audit. Earlier
failed compile/fixture/hash/reference-identity test attempts remain retained in
ignored checkpoints56/57/59/60. Generated and served WASM identities match.
No new UI or M7 package/game execution acceptance is claimed. The original-source
format/preservation evidence from54 remains historical compatible byte-contract
coverage; no new private schema4 project/gameplay run is inferred from it.

## Frontend ownership integration plan (not implemented; UI gate pending)

Keep one active authored owner in `useFieldDraft`: event-only M6 until complete
scene source is available, then one Scene session. The existing List/Graph/Lanes
consume the active owner's compiled event view; do not keep an independently
editable M6 handle beside a Scene handle. Scene promotion validates the original
source plus the exact exported M6 history before adopting, then closes the old
handle. Failed acquisition/promotion retains the current draft and last-good
build. Complete-context failure must not disable the already working M6 editor.

Acquisition/promotion captures the current project/backend generation, disc/map
session, authored revision, inline generation and context generation before any
await. Adoption rechecks every owner and preserves edits made while ranges/context
load. A new measured context remains unsaved provenance even if script content
and revisions are unchanged. A revision reset during promotion must not turn an
unsaved M6 draft into Saved; carry saved equivalence only when the original owner
was saved and no content/context changed. Preserve saved-edit -> undo-to-base
unsaved behavior and explicit discard. Files use the existing atomic adapter;
load/save/build completion cannot mark a replacement project saved.

A scene project must explicitly identify its scene document/version, while the
existing M6 project remains a supported event-only document. Validate the actual
original source/context before adopting either; never use compiled bytes as a
new base or silently rewrite an old file. Scene compilation feeds its validated
bundle into the existing M5 package builder; no parallel delta/overlay pipeline.
A last-good build records source, package identity, owner generation/revision and
stable active actor/descriptor order, not just a transient numeric row.

Names attached to original actor indices retain their original source identity
through active actor-row changes; a render-time index is not a metadata key.
New actor IDs and event source rows are distinct, and deleted originals must not
silently transfer names to the next row. Live PC/actor navigation checks the
attached package and build/source identity before using the matching stable
binding/order. A stale or incompatible build gets an explicit diagnostic rather
than selecting a current instruction that happens to share its numeric PC.

## Human UI approval received 2026-10-24

Bart approved both parent-generated M7 mockups in message
`Sentinel_029fd2fc881c8191ba56d00be9a745fa` at01:11UTC (“Those mockups look really
good. Go for it.”). Root inspected both actual PNGs before UI implementation.
Ignored project-bound references are
`tmp/studio/milestone-7/approved-mockups-20261024/m7-scene-editing.png` and
`m7-scene-validation.png`. Main Library IDs are
`libfile_7d9ebde8882c8191b1811dffb7304545` /
`file_00000000775c81f6ab05572a66068445`; validation IDs are
`libfile_e54e27501c648191843faca084d4ee05` /
`file_00000000e13481f6a07c2fbf1466cd29`. These are layout direction, not original
game assets, format definitions or approval of unsupported operations.
The independent OGA blocker and denied extra M6 screenshots are unchanged.

Implementation now proceeds in checked increments: one Scene/script owner and
project lifecycle; Three.js viewport with existing controls and integer source
transforms; inline hierarchy/properties and read-only live observations; then
browser/NW/source audits and actual same-package edited-gameplay acceptance.
Three.js is a rendering dependency only; original formats and mutations stay in
shared Rust. Reuse the pinned existing M5/M6 package/runtime boundaries.

Approved m7-scene-editing.png SHA-256: `c59cf656b80c73c2c792a805d2439e0761c92f60cf2c94e1773b7e44b659bf23`.

Approved m7-scene-validation.png SHA-256: `d19299ad2250473d32d9f28a1ebcc016491694d9dfe9cc674d33d60aebb48b33`.


## Checked synthetic UI increment — checkpoints 62–74

The frontend uses one `FieldAuthoring` owner. It promotes the original M6 root
and exact history into a Scene document; List/Graph/Lanes/Scene share that Rust
revision and undo timeline. Scene project schema2 wraps strict Scene document4;
event-only project schema1 remains unchanged. Scene compilation feeds the existing
M5 package builder. New/changed source context remains unsaved project state.

Three.js renders decoded source meshes/known uploads, collision and zones, with
explicit unavailable-format diagnostics and sprite identity markers. Runtime
lighting/blending/morph/sprite VM poses are not simulated. Entrances require
explicit selectors; no count is guessed. Runtime pose currently has a locked
instruction surface, not completed live observation/overlay acceptance.

The shared canvas uses a geometry-centred display proxy and source-origin delta,
never directly mutating accepted objects. Synthetic browser71 verifies real
rotation Cancel/Apply with retained high bits/source position/unrelated state,
translation Cancel, ray selection, concurrent-history drag non-adoption and
local late-render/context-loss recovery. It does not prove real gameplay or
non-null last-good-package retention. Placement/browser68 is separate reused
evidence. Native/WASM60 semantics are unchanged by this UI increment.

Expected guard failures stay local with expandable original stacks and retained
Validation-log entries; unexpected errors use a compact global summary and
expandable Error-log details. After actual screenshot review caught footer
overlap, the inspector separates scrolling fields from fixed diagnostics/actions.
Per-axis native aria-invalid/aria-describedby link bad inputs to the concise
visible explanation without changing source bounds. Browser74 at1440x900 proves
all four corners accept normal clicks/focus after scrolling, with full input
rectangles above actions and hit-tested input centres. Accepted scene state is
unchanged while invalid; Apply is blocked and Cancel remains usable. Root and
Astra inspected the actual clean/invalid PNGs under ignored checkpoint74.

Exact commands, hashes, scope IDs, exits, intermediate failures and screenshots
remain in ignored `tmp/studio/milestone-7/verification-index.md` and checkpoint
leaves. Full runtime inputs are unchanged; no new full runtime-suite pass is
claimed. Real source/edited playback, save/reload and delayed ownership checks,
read-only live observations, entrance selection and desktop acceptance remain
required before M7 completion. The dependency audit retains a HIGH Zod advisory
caveat; this increment does not claim zero audit findings.


## Read-only runtime observation integration (verification pending)

The Scene runtime surface uses the existing paused game controller/schema
protocol, not a new game operation. Captured observations require the measured
module identity, original disc family/current map and matching event component.
Vanilla maps selected original actors by their source origin; an edited runtime
requires exactly the current last-good package identity, authored revision/owner
and stable actor/descriptor order. Added actors have no guessed vanilla binding.
Unknown or stale identities refuse observation.

Original field actor position is signed32 16.16 XYZ (`actors.h`); the heading
field is signed16. Runtime clock observations use the actual u64 `vblanks`
status field without rounding unsafe integers. Only status/inspect/read_memory
commands read a pose, while the existing controller explicitly pauses execution
and leaves it paused. Both controls state this effect; resume from Live game.
A labelled orange identity marker is read-only; it is not an evaluated sprite.
It never changes the authored descriptor or placement. Project/source/selection
generations and the controller epoch reject stale asynchronous completion.
These reads qualify the event namespace, not full unrelated-component package
preservation; that remains a separate real acceptance proof.

Explicit Bind original runtime source context acquires the existing measured
vanilla loader provenance for maps whose original stream needed no extra tail.
Rust validates a re-import with the exact existing history; this metadata change
marks the project unsaved, retains the last-good build historically, and requires
a fresh compatible build before edited-package observations. No file is silently
written and no TLS/trust/runtime game code is changed.

Reader and existing M4 PC unit checks pass7/7 in checkpoint76; they do not
exercise caller package/stable-order ownership or an actual runtime. Browser77
passes imported M6/Scene history, retained redo/IDs, project save/reload and
delayed folder read/write/source ownership, plus two affected script-navigation
regressions. The ordinary runtime UI source path still requires real acceptance.

Browser78 passes actual actor and zone Add/Duplicate/Delete buttons and undo/redo,
immediate auto-selected stable actor/event identity across Scene-to-List
switching, and atomic refusal of a referenced collision vertex deletion. The
normal/automatic selection path now uses the fresh namespace; history removes
dangling selected IDs/bindings. Structural actor changes clear temporary numeric
PC navigation, and automatic selection does not seed it with a deleted actor.
Placement/source-binding regression also passes. Expected structured Scene
refusals remain local with retained original technical details.

Actual browser/native runtime observations, package/gameplay and desktop checks
remain unfinished; no such pass is inferred from these synthetic tests.


## Explicit entrance inspection UI (verification pending)

The Scene hierarchy can inspect a caller-established unsigned16 entrance selector
through the original ff-prefix reader. It never enumerates a guessed count. A
valid inspection enriches the read-only view namespace and does not add an
authored action; it verifies the entire current compiled bundle and event
bindings remain byte-identical before adoption. Known-good package ownership
can transfer only across that byte-preserving inspection. The existing history,
allocator IDs, context dirty state and unsaved authored state are preserved.
Saving records the inspected selector list; inspection alone is optional view
qualification, unlike measured runtime context required to reproduce a build.

A supported X/Z change enters the shared authored history and makes the project
unsaved. Y/camera/facing/layer remain read-only here, with raw bytes shown.
Structural entrance CRUD is disabled with its reason beside the actions: no
proven count/incoming-reference closure. Invalid selectors preserve the prior
scene/document and expose the original source error. These controls use existing
NumberField/native details/form patterns; focused UI and persistence checks are
prepared separately from original-map gameplay acceptance.

### Original-disc observation increment (checkpoint83)

The original Disc1/map23 Scene opens through real pull/range-fill transport after
both discs are recognized. Its measured context validates all nine complete
component outputs, including component8's78 bytes rather than only its72 logical
bytes. Normal Bind imports a new validated owner, preserves authored history and
bytes, and marks provenance unsaved. Starting the real browser runtime Running
before Bind/Refresh proves both controls pause it and leave VBlank stable.
Original actor descriptor:3 has the explicit pc:1076 placement binding; the
read-only observed signed16.16 position converts to29/0/1270 and heading-32768.
This is caller-level original browser observation, not edited-package or native
runtime-pose acceptance. Exact command/source/scope evidence is local in83.

Checkpoint82 exposed a UI ownership bug: DiscSession retains its default progress
callback for later reads, so a completed Scene mapBundle request reused the old
opening callback and left global busy set to 'disc1.bin: ready'. The callback now
updates global busy only during its opening lifetime. Runtime observation
controls keep their guards and explain unavailable operations in their titles.
The corrected83 actual journey asserts normal busy completion before and after
Scene; all preflight/browser/source checks pass. Failed81/82 logs remain intact.

Root reviewed the new83 M7 captures. They show a tightly framed actor marker and
an opening 'Fei?' game prompt, so they are discovery evidence, not the required
usable real scene/edited-gameplay visual proof. Initial selection framing and
meaningful real package/save/reload/Play/native/browser evidence are still being
completed. No M7 completion or new full-runtime-suite pass is claimed.

### Current real browser proof (checkpoint86)

The authored actor descriptor:3 uses the explicitly qualified pc:1076 placement.
Normal inline Apply changes X29→61 while Z1270 and collision-derived Y remain
unchanged. Complete source outputs0–4 and6–8 match the measured original context;
component5 intentionally changes (5343→5344 bytes with authored packing tail).
No original-source padding or RAM is fabricated. The actual OPFS browser adapter
saves/reloads the scene document through UI controls; a new owner preserves full
export/compiled hashes and bindings. Its one Disc1/slot652 delta builds and plays
normally. Invalid X32768 blocks Apply/Build/Play while retaining the successful
package; actual WebGL context loss/retry preserves that package as well.

The exact set SHA-256 is
d242f77413949e6dfa32db17f3815fc4adf35c7f0ede5f1d2e5d96b08e229489,
identity studio-map-23@54458cc80057ab7af95dc59508f9f018d9c3d5531e1fa352cca154c2a1a2612d.
After UI Play, a separately recorded fresh canonical boot/reconnect retains the
same staged package. Runtime position61/0/1270 is already observed at ordinary
field entry, before Cross, and remains so after Cross+600 frames. Input is not
claimed as the cause of placement onset. The new game capture shows the forest,
character and readable Elly dialogue. This is real browser proof; the same set's
native comparison and untouched-map/Disc2 matrix remain separate work.

Human/root pixel review accepts simultaneous authored/observed coordinates,
source binding, explicit Map23 and the meaningful game image. The final viewport
still exposed a framing issue after owner/renderer replacement, so it is not
final scene-visual acceptance. The correction gives the generic canvas an
explicit model-owned frameBounds contract: overview uses visible static source
geometry rather than unresolved markers, while normal Frame selection uses the
current selected object's bounds and camera context. The96-unit marker padding
is editor camera policy, not an inferred sprite dimension. A focused saved-
project/framing regression and actual new pixels are pending; the successful86
playback proof is retained. App shortcuts now respect an already-handled event,
so inline Ctrl+Enter does not also submit an unrelated Backend command; its
focused regression is pending alongside framing.


### Exact-package native/browser and framing proof (checkpoints89–90)

Checkpoint89 verifies the exact86 UI set in native and browser for Disc1map23,
actual Disc1map22 and Disc2 title, each absent/disabled/enabled. Raw memory hashes
and status compare across hosts without normalization; absent equals disabled.
Enabled map23 actor3 enters at61/0/1270 versus vanilla29/0/1270 before Cross and
retains it afterward. All Disc1map22 file bytes and its index record are original;
all Disc2 index/directory records and full sampled files650/652 remain original.
Disc2slot652 is a24-byte placeholder. No full Disc2 progression is claimed.
The gated Rust test hashes original Disc2 files before moving its source into
the virtual wrapper, then compares wrapped reads; no original asset is copied
into the repository. Native captures show readable forest/Elly dialogue, while
functional displacement is established by the exact readback/disabled controls.

Checkpoint90 runs only the corrected focused synthetic picking/index tests and
the saved original-map project Frame journey. The same complete saved document
imports with a new owner and deterministically builds the exact86 set. Normal
Frame selection centers descriptor:3 after that replacement; the new fullsize
capture shows the selected marker and surrounding decoded collision geometry,
with authored61/0(floor)/1270 and vanilla observed29/0/1270 simultaneously visible.
Root reviewed actual pixels; no sprite fidelity or lighting equivalence is
claimed. SourceY remains an empty disabled floor-derived field, not authored0.
The synthetic picker uses a bounded non-collinear synthetic source triangle;
original failed89 used a collinear fixture and remains retained.89 aggregate1
reflects that failure;90 aggregate0 does not rewrite it. Scope/commands/hashes
and individual results remain in the ignored verification ledger.

NW integration and final publication are still pending; no full M7 completion
claim is made by this checked increment. The Three.js MIT notice is exact in
source and in the checked88 built distribution. No new runtime core/decomp
inputs changed, so accepted M6 full-suite/focused-retry evidence is reused with
its original exit1 and screenshot limitation preserved.


### Desktop acceptance limitation (checkpoints91–95)

Fresh test/production Vite builds, typecheck, design/source audits and the normal
built/development NW smokes passed at91. Development checks use the actual HTTPS
server, selected public certificate and strict TLS/hostname validation; the
canonical temporary manifest has exactly https://localhost:5176/* plus that one
public additional trust anchor. The copied production manifest has neither.
Three's notice and all distribution assets are copied/compared byte-for-byte.
No existing dist or prior result directory is cleared. --keep-artifacts retains
fresh smoke folders; ordinary default cleanup behavior stays available.

The new optional Scene NW journey uses actual original-disc path controls and
Map23 selection, seeds the exact86 project into a fresh ignored folder, then
uses normal Load through the real NodeFilesAdapter. Only OS directory-chooser
selection is intercepted. It is designed to qualify actor descriptor:3/pc:1076,
edit61→62 with inline Apply/Undo/Redo, and verify real Node Save/Load/document
identities. Those edit/write/reload assertions have NOT executed successfully.
91 failed an incorrect folder-observation selector before Scene;92 corrected
that harness observation and reached a real Three.js WebGL context failure.

93 measured the test Xvfb server's actual extensions: GLX is absent. The suite's
normal Mesa EGL/llvmpipe environment cannot supply an RGB GLX visual there.94
confirmed the existing pinned Mesa Lavapipe Vulkan driver is available.95 used
only normal renderer selection --use-gl=angle --use-angle=vulkan and the pinned
VK_ICD_FILENAMES; NW then explicitly reported WebGL2 blocklisted and Three.js
refused context creation. This was a modified test renderer configuration, not
a passing default launch. [ANGLE's documented renderer selection](https://chromium.googlesource.com/angle/angle/+/HEAD/doc/DebuggingTips.md#usage) is distinct
from its unrelated sandbox/watchdog options, which were not applied.

No blocklist, unsafe SwiftShader, sandbox, TLS, system trust or Node-origin bypass
is permitted or present. The app keeps the local original renderer diagnostic.
A normal NW.js display/driver providing supported WebGL2 is required to finish
Scene desktop edit/save acceptance. Browser Scene and exact-package both-host
gameplay remain separately verified. Failed91/92/95 logs and93/94 diagnostic
results remain intact; a pushed checkpoint will not establish full M7 completion.
Astra's final source review additionally removed a premature test-only coordinate
wait before actor/binding selection, with focused typecheck96 passing. No
further NW graphics retries were run.


### Final available checks and review (checkpoints96–97)

All four public metadata generators pass --check; typecheck, source/archive
policy and diff checks pass at96. Its production inventory verifier initially
failed because Node localeCompare and Python lexical ordering differed, despite
all 11 complete path/size/SHA identities matching.97 fixes only that ignored
verifier, rejects duplicate paths, compares full identities independent of
enumeration order, and reuses96 only after all recorded source inputs match.
The actual production-hook browser test passes1/1 in4.8s with Chromium sandbox
enabled and the test flag unset, against exact91 production assets. Inventory
checks before/after both pass. No production rebuild or other journey repeats.

Matching-source unit evidence remains76's seven pose/PC tests,64's three
field-authoring cases plus unchanged scene/service cases from63. Native26 cases
and141 full command envelopes from58, WASM parity60 and strict Clippy/fmt89
remain valid for unchanged backend inputs. Runtime core/decomp did not change,
so no full runtime suite was launched. Accepted M6's full-suite exit1, focused
passing retries and residual independent screenshot limitation stay historical.

Astra independently reviewed the current package/ownership/framing proof and the
new NW harness permission, retention and private-input boundaries. Its one latent
coordinate-wait ordering finding was corrected and typechecked. No further code
blocker was found in that bounded review. Scene NW edit/save remains unexecuted
because of the measured normal-WebGL2 environment barrier. M7 is not declared
complete. Five protected main PR46 files retain their exact baseline hashes;
their separate changes are not part of the outgoing editor patch. Publication
is a separate authorized checkpoint action, with exact remote/CI receipts kept
locally and reported in-session; it does not waive this remaining acceptance.

## Windows desktop closure and final acceptance (2026-10-24)

The supported-scene M7 acceptance checklist is complete against application
commit `54fb46b887449fff7f6c5450bd0403533b2e2764`. The final missing desktop
edit/save/reload journey passed on Windows with official NW.js **0.113.0 normal**,
WebGL 2.0, and ANGLE's NVIDIA GeForce RTX 3090 / Direct3D11 renderer. This closes
the acceptance gap; it does not make the previously failing Linux display
configurations supported or change their recorded outcomes.

The unchanged application opened both recognized original discs, selected
Disc 1/map 23, loaded the hash-qualified saved Scene project through the actual
NodeFilesAdapter and selected actor `descriptor:3` / placement `pc:1076`.
Normal inline Apply changed authored X61 to62; Undo/Redo restored the expected
values. Normal Save and Load adopted a new Rust owner with equal saved document
and compiled-bundle identities. Source descriptor X stayed0. The test verified
a live WebGL2 context (1197×598 canvas), exact source/module identity and the
expected two project files, then exited normally. Only the OS directory-chooser
selection was intercepted; file reads/writes and editor actions used the real
application paths. No graphics, sandbox, blocklist, TLS or origin bypass was used.
The harness did not capture a Windows screenshot; reviewed real browser images
at86/90 remain the visual acceptance evidence.

The first Windows attempt failed before edits because its isolated configuration
used `sourceHeaderX:29`. The original descriptor word is actually0: source header
0x190 + descriptor3×16 + X offset8 = **0x1c8**, little-endian bytes0000.
`scene/formats.rs::decode_header` establishes that layout. Independent original
and saved-document decoding through the verified shared backend both returned
descriptor XYZ0/0/0, while the separate `field_event_place` instruction returned
original X29 and saved X61. The successful retry changed only that isolated
configuration expectation to0, with fresh output/project paths. All preservation,
edit, history and persistence assertions were retained. The failed run remains
evidence; process exit0 alone was not considered a pass because the driver can
write an error result before normal application exit.

### Checklist evidence and supported boundaries

| Acceptance item | Verified evidence |
|---|---|
| Bounded decode, no-edit bytes and opaque/source preservation | Synthetic02/11–13 and original-map26–28/54; full unrelated outputs and untouched-map preservation at86/89. |
| CRUD, reference closure/refusal, stable IDs and shared history | Backend35–61, including26 native cases and141 complete native/WASM command envelopes; actual actor/zone Add/Duplicate/Delete and script-selection checks at78. |
| Picking, transforms and properties/scripts | Browser71 covers real move/rotate gestures, Apply/Cancel, source origin/high bits and stale-drag rejection;78 covers actor/script synchronization;80 covers explicit entrance inspection and X/Z edit persistence. |
| Dirty state, validation, delayed ownership and recovery | Visible/accessible validation and all corner inputs74; imported history and delayed file I/O77; actual invalid draft and WebGL context-loss recovery retain the last-good package86. |
| Authored/live distinction | Reader/PC tests76 and real original/edited observations83/86 enforce source/module/package/order identity. Observation pauses and leaves the game paused; it does not write authored state or game memory. |
| Saved scene and cross-host package behavior | Browser86 normal Save/Load/Build/Play;89 exact-set native/browser absent/disabled/enabled matrix;90 reload/rebuild/framing; Windows closure above verifies native file persistence. |
| Current checks and provenance | Strict Rust lint/format89; fresh builds and ordinary NW development/production91; metadata/type/source96; exact production assets and test-hook absence97. Windows app/assets and private inputs were independently hash-checked before/after. |
| Scope and preservation | Human UI approval recorded above; no decomp or protected main-checkout changes, original disc writes, private artifact publication or security bypass. Five protected file hashes and clean application HEAD were rechecked after Windows validation. |

Completion is limited to the roadmap's **supported entity kinds**, not every
possible field encoding. Actor Add/Duplicate use qualified source templates and
independently qualified event copies; unrestricted construction and unresolved
party/dynamic/reference cases remain atomic refusals. Static model instances,
zones and qualified collision topology have their documented operations.
Entrances support explicit selector inspection and same-selector X/Z changes;
destructive entrance CRUD lacks reference closure and stays disabled with its
reason. Sprite VM evaluation, runtime lighting/morph fidelity, inherited VRAM
dependencies, invented scale and a guessed game-camera transform are not claimed.
These are the documented capability boundaries, not substitutes for the verified
supported CRUD/transform/history requirements.

### Portable reproduction and identities

Use your own recognized disc inputs and the qualified private map23 project;
neither is distributed with this repository. Keep all outputs in fresh ignored
storage. The recorded Windows sequence was:

1. Verify the official normal Windows archive against
   [NW.js0.113.0 SHA-256 manifest](https://dl.nwjs.io/v0.113.0/SHASUMS256.txt),
   then unpack it side by side with any existing runtime.
2. Build the application at the recorded commit with
   `VITE_XEM_STUDIO_TEST=1`, the verified Studio WASM, and a fresh test-dist
   output. Copy the canonical package manifest unchanged: `main=dist/index.html`,
   no `node-remote` and no additional trust anchors. Compare every copied asset.
3. Configure the existing `src/events/test-driver.ts` Scene journey with both
   disc paths, a fresh `output`, `disc:1`, `map:23`, and a `scene` object
   containing fresh `folder`, private `seedProject`, the seed hash below,
   `actorId:"descriptor:3"`, `placementNode:"pc:1076"`,
   `sourceHeaderX:0`, `initialX:61`, `editedX:62`, and the original bundle
   hash below as `expectedSourceSha256`. At the recorded commit the Linux
   `nw-smoke.mjs --scene` configuration still assumes header X29; do not confuse
   it with the opcode placement. This verification corrected the isolated config,
   not application code.
4. Launch the pinned normal executable with the copied app directory,
   `--studio-events-test=<config.json>` and a fresh
   `--user-data-dir=<private-profile>`. The existing driver calls
   `nw.App.quit()` in its finalizer. No renderer/security override is required
   on the verified Windows host.
5. Require a `kind:"scene"` result with no `error`, actual WebGL2, saved X62,
   unchanged header X0, a changed owner on reload and matching document/compiled
   hashes. Verify the real saved file independently; retain failures as well as
   the passing result.

| Artifact | SHA-256 |
|---|---|
| Official `nwjs-v0.113.0-win-x64.zip` | `2fff537d37e827ddc0149cd3c17f5a15f71bef782d3795f7870a73b1f6cea4b1` |
| Windows `nw.exe` | `933feff7344b57f3eacb6f961194e81d496362543c0bc4bb33bbe4faa766edc1` |
| Studio backend WASM | `b6e7a3a55ee482135e6dbf386e622bd0cde1681a72bec707b291fbfd26a662a6` |
| Measured game module | `613c3a747e1c07e0e0e2632710eae1f873e4b14dda880cd6ce1c556b31c152a8` |
| Original indexed map23 bundle | `0f0a131acdf052cae070a64912fc09eb00e0b5e6a81f9d8cb4719d6416213add` |
| Input saved project | `3cffb561e775730c7152a83d524729e02bdbcaeff13a57007763752a37581f9a` |
| Windows saved project | `fda0309fd16766fe18913565989bcc2355c381259750cae8f8ce86c9c819f5b9` |
| Windows saved document | `7a983173ce3bea1e9ada10cec4bf27e9d23db72e9a7b75c6f1c42a980a9a0471` |
| Windows compiled bundle | `d28ec7604d797e133aea9211f297343a3132d919442f648ac95cccba4ba09a1f` |

Local evidence includes `provisioning-receipt.json`, `transfer-receipt.json`,
`fixture-correction.json`, the failed and successful `smoke-result.json` files,
`final-validation-receipt.json` and `remote-final-preservation.json`. They are
private local verification artifacts, not repository assets. Original input
hashes, all14 copied public app files, the executable, five protected main files
and clean published application commit matched after the run; no test process
remained. The older NW installation was retained.

No full runtime suite was rerun: runtime/decomp inputs are unchanged. Earlier
M6 full-suite exit1 and its passing targeted retests, existing Zod advisory,
independent M6 screenshot limitation and separate asset-permission boundary
remain unchanged. GitHub showed zero workflow/check/status contexts for the
application branch push; that is not a CI pass. The Windows evidence closes M7's
desktop criterion without rewriting any earlier failed or skipped result.


## Isolated Map23 optional-tail scope (2026-10-24; implementation plan)

Bart authorized ignoring only the unverified decoded suffix of the exact
original Disc1 Map23 component8: logical72, packed output78. This does not
establish that the six bytes are junk, unused, or developer-intended padding.
Original CD-copy behavior and measured-context provenance remain unchanged.
Implementation and tests stay in the isolated studio-context-prerequisite-ux
worktree; the active preview/live checkout must not change. No push yet.

1. Add a separate bounded logical-prefix packed decoder; keep decode_block's
complete eight-token groups and speculative terminal flag behavior unchanged.
Validate declared output limits, every required token/distance and full token
output bounds, even when a copy straddles the requested logical end. Stop at
the exact required length without consuming an optional token or flag.
2. Qualify only exact original disc/boot/map/slot/LBA/indexed size/sourceSHA,
component8 offset109992, logical72 and advertised78. Other inputs remain strict.
With no context expose72 known bytes and an explicit optional-tail diagnostic,
not measured RAM provenance. Context-present/imported documents retain strict
full-output behavior and their existing byte/hash contract.
3. Preserve exact original indexed bytes for no edit and edit→undo. Any edited
no-context package must author a self-contained component8 stream from known
logical data and update its descriptor, even if that payload was unchanged or
the edit only changed a header/different component. Strict-decode the final
stream safely; preserve the other eight complete outputs. Retain original source
bytes in their original prefix except required header/descriptor changes.
4. Focused malformed/truncation/straddling-token/source-identity/import/history
regressions, actual original Map23 offline open/edit/build/save/reload and fresh
identical-package native/browser gameplay. No old context-based package may
substitute for the new offline package. Retain failures and exact evidence.
5. Source/type/Rust/WASM/metadata checks in pinned bounded scopes; independent
review, local reviewable commit/diff. Live application and publication are
separate coordination boundaries, not part of this isolated implementation.


### Qualified offline contract and verified isolated result

The initial isolated exception (superseded by the general structural policy
below) was selected only after ordinary source validation and the exact
Disc1/boot identity, Map23/slot652/LBA124528, indexed size110052 and bundle SHA256
`0f0a131acdf052cae070a64912fc09eb00e0b5e6a81f9d8cb4719d6416213add` match.
Component8 must start at109992, declare logical72 and advertise packed output78.
That initial policy admitted no other map/source. Without context, its decoded
asset summary reports output_size72 and optional_tail_bytes6, and diagnostics
explicitly distinguish this policy from measured runtime context. Those six
bytes are not preserved; their use and the developers' intent are not established.
The input can supply more than72 output bytes, but this contract stops at the
logical boundary rather than claiming that every omitted byte is unavailable.

No-edit and edit→undo return the exact original indexed bundle. On any edit,
including header-only or sparse other-component changes, preparation authors a
new complete component8 literal stream from the known logical payload and
redirects its descriptor. It does not leave the old stream reading appended
authored bytes. All final streams are strict-decoded; other complete outputs
remain unchanged except intentional authored replacements. Original indexed
source bytes remain in the prefix except required descriptor/header changes.
This authored stream is not reconstructed RAM. Context-present documents still
use strict complete outputs; no blanket truncation exception or global runtime
decoder change exists. Event-only M6 repacking keeps its existing strict contract;
this new policy belongs to source-qualified Scene compilation.

Focused native results: prefix4/4, existing strict packed7/7, scene container10/10,
original-map compatibility1/1 and source-identity/sparse-repack1/1 pass. These
include required-token straddling/bounds, one-byte-short real prefix rejection,
wrong SHA/map/slot/LBA rejection, original no-edit/undo, header-only and missing-c8
replacement normalization. Historical saved-context project SHA3cffb561…81f9a
imports/re-exports unchanged and compiles to prior2a50712b…91ca1f9; restoring its
X61 instruction to original29 returns0f0a131a…6213add. Prior context-based static
transform3f1120… is reproduced. Those hashes are compatibility proofs, not the new
offline package. Strict Clippy and formatting pass on the pinned1.97.1 toolchain.

The new WASM-backed isolated browser opens actual Map23 without an attached game,
with source-context handler calls0 and no popup. It edits descriptor:3/pc:1076
X29→61, saves/reloads the actual context-free project and builds through the
normal active.repack path. New candidate SHA256 is
`d9506735b94efcc4cb41f33a8be721f46319353fcf3a26db4b5cadd21007bbbc`.
New7002-byte set SHA256 is
`5ba108ec9d59316dcb3e55072d7826c16b527cdd7db55b269d78d089f6defa2a`,
member identity studio-map-23@4055429ca45de66c80ca96d09903e0bcd1f4057daf7638786649219e8191aac8.
This exact new package executes on fresh native/browser routes in all three
absent/disabled/enabled modes. Raw memory/status/index observations agree;
enabled actor3 enters at61 while absent/disabled remain29, before public input
and after Cross+600. Input is not claimed to cause the placement onset. Fresh
actual captures show forest, character and readable Elly dialogue. This is a
bounded field-entry route, not proof of all uses of the omitted tail or full story.
Map22/Disc2 gameplay and the full runtime suite were not repeated.

Production validation stages current source in a separate ignored app with the
NEW checkpoint05 backend, both test/package-override flags unset. The built and
actually served WASM SHA matches that new SDK, not preserved oldpkg; production
hooks are absent. Design/type/source audits pass, and the five disconnected/offline
UX unit regressions are reused by exact source hashes. The public new host fixture
uses env-resolved inputs, verified hashes and ignored fresh-output guards. New
codec/policy code is locally authored from the repository codec; only source
identity facts/hashes are public. Original game/context/project/package bytes
remain ignored private inputs/artifacts, with no new external extraction.

Failed02(output-root guard),04(test TypeScript syntax),05(empty-Y fixture) and
07(Clippy dispatch) remain retained with targeted successful03/06/08 retests.
The06 summary's member hash incorrectly included its containing buffer; a separate
correction and synthetic helper regression retain the original receipt and prove
member405542… distinct from set5ba108…. Current production checks pass in10.
Exact local commands, source/tool/input identities, stage results, full hashes
and explicit missing outer telemetry are recorded in ignored
`tmp/studio/context-ux/commands-and-results.md`. No live checkout/HMR/service
change or push forms part of this isolated result.


## General offline Scene stream completion (2026-10-24; active plan)

Bart expanded the isolated Map0 bugfix to a format-level policy after encountering
context-required errors on multiple maps. This supersedes the Map23-only
allowlist for newly admitted offline inputs; it does not change strict runtime
decoding or establish that output beyond logical sizes is unused. Work stays in
the isolated worktree. The user's live checkout and preview remain untouched.

1. Audit every available map/component on both private original discs. Separate
   strict successes, complete output missing only terminal lookahead, empty
   terminal streams, available logical prefixes with unavailable output suffixes,
   missing required logical bytes, and malformed references/bounds. Record exact
   per-disc coverage and representative diagnostics without committing game data.
2. Define admission from validated stream structure, not map numbers or source
   hashes. Preserve complete known outputs when only terminal lookahead is absent.
   A missing suffix requires validated required tokens and declared output bounds,
   including a token crossing the logical boundary. Event execution can traverse
   retained tails, so missing event data must not be declared optional merely
   from its logical length. Malformed streams remain errors.
3. Keep measured-context loading/import strict and original input immutable.
   No-edit and edit then undo return the exact indexed source. On every actual
   edit, materialize every admitted incomplete stream as a complete authored
   stream, even when its payload is unchanged. Strict-decode all final streams;
   protect unaffected complete outputs and disclose any omitted optional suffix.
4. Add structural and malformed boundary regressions plus both-disc conformance,
   disconnected original Scene open/edit/save/reload/Build, and fresh identical
   edited-package native/browser playback. Reuse compatible evidence, retain
   failures, and verify current production WASM/test-hook boundaries. Independent
   review and a scoped local commit precede publication; preview integration is
   a separate authorization boundary.


### Both-disc boundary audit and native conformance

The retained Python inventory inspected all730 Disc1 map bundles and205 Disc2
bundles; the other525 Disc2 slots are short-header placeholders confirmed against
the raw index. Across8415 real components:7308 strict successes,682 empty terminal
streams,160 complete outputs missing terminal lookahead, and265 available logical
prefixes missing advertised suffixes. No missing logical data or malformed packed
references/output bounds occurred in these original inputs. Synthetic regressions
cover those rejection paths. Missing output suffixes occurred only in components7
and8:115 message tables and150 whole-zone prefixes.

Component7 is the message table (`field_load.h:BUNDLE_MESSAGES`), not arbitrary
opaque data. The resident `text_get_resource_entry` reads starts at4+2*index;
`text_get_message_columns/rows` use count-based dimension pairs. The final table
offset is not a runtime execution fence. The bounded offline proof follows the
original `window_reveal_text` controls within the known logical payload, allowing
both glyph widths without assuming a loaded font. All115 candidates decode using
the analysis tool's separately verified original-font threshold0xFE, but that alone
does not qualify arbitrary Scene input. The font-independent proof admits106
(74 Disc1,32 Disc2). Nine Disc1 maps remain context-required:54,84,128,140,162,168,
183,453,473. Opaque sprite/encounter suffixes and genuinely incomplete event output
are also refused. All real event components decode strictly; this is not proof of
an arbitrary event execution boundary at logical size.

Native checkpoint09 passed20 container regressions and the raw-index public-API
sweep in one bounded16GiB/no-swap user scope. Of935 real bundles inspected,926
(721 Disc1,205 Disc2) load offline, preserve exact no-edit indexed bytes, and
strictly reload the authored header-edit candidate with known outputs protected.
The other9 are the explicit message-proof refusals above; placeholders are not
counted as successful containers or Scenes.

Public `field.scene.decode` and `field.scene.draft` were checked separately for
every real map. Decode succeeds for923 (719 Disc1,204 Disc2), with the9 context
refusals plus three `invalid_scene_texture` results: Disc1 maps164/165 and Disc2
map164, each "texture offset table exceeds component". Draft open succeeds for926
(721+205); every opened handle closes. These additional texture-parser limitations
remain explicit; container conformance is not a935-map UI/gameplay claim.

Disc1 Map0 is accepted by both operations. Its raw-index source is slot606,
LBA120698,105980 bytes, SHA256
`c7cdbefd834e1d0405482d95204c822c8f4ef35249eee62c54a3af09cfd52921`;
component8 is a complete logical0/declared0 header at offset105976, exactly EOF.
The historical Map23 context-free sparse-repack regression also passes: invalid
source SHA/size/boot still reject, but in-range map/slot/LBA labels alone no longer
select an allowlist and do not independently prove a raw-index relationship.
Measured-context exact identity checks remain unchanged.

Ignored artifacts live under `tmp/studio/offline-components/`. Earlier failed
setup attempts remain recorded; no game bytes enter source. Fresh UI/package and
native/browser execution acceptance is recorded separately below when completed.


### Final backend and offline UI replay

The final continuation check rejects a missing token when known flag bits prove
that completing the group cannot fit the advertised output, including partially
read references with an impossible low distance. At a fresh group boundary the
only possible remaining sizes are0,8,or>=10. This is a feasibility check on known
framing, without manufacturing a source continuation or claiming original RAM.
The strict `decode_block` and bounded `decode_prefix` implementations remain
unchanged. Complete known outputs are retained; admitted omitted suffixes remain
subject to their message/zone consumer proof.

Checkpoint15 passes24 container regressions, current Clippy/type/format/source
checks, the source-only two-disc audit and native/WASM saved-project replay.
The raw scan independently confirms all160 nonempty full-output truncations
finish complete flag groups; none is a hidden mid-group overflow. It separately
reports935 real sources,926 admitted and9 context-required, plus525 placeholders.
This follow-up validates the stricter condition without repeating checkpoint09's
public decode/draft sweep. The latter's923 decode/926 draft outcomes and explicit
texture-parser limitations remain the compatible evidence.

Checkpoint13's actual browser journeys open Map0 and145 with no game connection
and context-handler calls0. They select the verified source bindings, Apply an
X edit, Undo to the exact original compiled hash, Redo, Save/Load through the
normal file adapter, and Build via the active Scene repack path. They preserve
the source header placement when the authored change belongs to a script.
Map0 uses descriptor18/pc2327 (X0→32, immediate Y0/Z420); the initial Map145
fixture used descriptor11/pc124 (X382→414, floor-derived Y/Z322). Later playback
identified414 as outside the usable walkmesh; this retained initial fixture is
not the final patrol acceptance. Checkpoint20 regenerates it through the same
normal UI with verified in-mesh X366. Map145 exercises an
incomplete component7 message stream (logical1816, omitted6) as well as empty
component8. Each normalized candidate has complete strictly decodable streams.

Those UI journeys predate the last malformed-input guard. Current native and
browser SDK replay in checkpoint15 imports the exact saved projects and produces
the same candidate hashes, so the earlier UI/Build evidence is reused explicitly.
The final SDK and its test-served WASM asset both hash to
`ee135db37c4675bfaa0fefd9454b6b7e990fc794c40f8339a7f527bbbc268a30`.

| Map | UI-built candidate SHA256 | UI-built package-set SHA256 |
| --- | --- | --- |
| 0 | `8818c8454e507b9fd64cd9651b5d8edce8caa32c312fa0dd48bd3f382384259c` | `c7a65d209995cd84ed54857631b7e1afb70f50d67672e8616f04ce474c3108b5` |
| 145, initial414 (superseded fixture) | `4c7103fdf15bdd2b62f630eb8d3c18634720d5e60135b738b27814dadb547714` | `8aa52b41d308b2c29a42cfe8552132fd6f1efb706ca2ae99e8f6a6ba540800d7` |
| 145, verified in-mesh366 | `0c4742046506af9226525468d0171d235d7f1ef40de0efd9d61c02712dcdf4b2` | `e08cf109e85529c31e9d0e4a9bb94f4cf6d765a3fbf441e2a199c6a8e84fb0f6` |

Historical measured-context project import/edit/export still reproduces its
accepted hashes in checkpoint11; the final guard is restricted to context-free
loading, so that unaffected proof is retained. The saved project remainsSHA
3cffb561…81f9a, its original authored candidate2a50712b…1ca1f9, its reverted source
0f0a131a…6213add and measured edited candidate3f1120…. No matching/decomp or runtime
inputs changed, and no full runtime suite is claimed for this standalone fix.

Retained failed attempts: initial audit import/setup failures01/03/04/05;
checkpoint10's missing Disc1 test environment; checkpoint12's full pinned-format
gate (WASM/UI correctly not run). Checkpoints02/06/08/09/11/13/15 provide their
stated corrected evidence; intermediate proof policies are not final support
claims. Runner14 was prepared but never launched. Private commands, results,
projects and packages remain under ignored `tmp/studio/offline-components`,
`tmp/studio/context-ux` and the corresponding `tmp/studio/milestone-7` leaves.


### Playback fixture diagnosis and in-mesh regeneration

Checkpoint16 correctly stopped after an external test expected64 characters for
`memory_hash`; the actual runtime contract is u64 formatted as16 hexadecimal
characters (`xem-core/control.rs`). The failed assertion is retained; the correction
keeps exact digest equality. Checkpoint17 completes all six Map0 routes but then
exposes a Map145 fixture error: actor11 is moving, not stationary at its authored
initial X382. At frames733/1333, vanilla and disabled agree at X352.00732421875 and
273.025634765625 on both hosts. Checkpoint18's initial X414 package stays414 at
both checkpoints and fails the retained motion assertion. It is not accepted by
substituting the observed value or removing movement coverage.

Bounded source analysis uses the actual original and current compiled component.
Actor11 event0 entry121 places atPC124; event1 entry133 patrols between X262 and
X382, with waits after arrival. The compiled event output is1920 bytes before and
after; only component offset1281 differs (the intended X operand), all variable
bits/event entries and the entire event1 instruction sequence are unchanged.
Complete outputs0–4/6 are exact, the admitted1816-byte component7 prefix is exact,
and component8 is empty in both. No repack/script mutation explains the stalled
patrol.

The original `field_event_place_actor_on_floor` searches layers below
`layer_count-1`; this map's usable layer0 contains original(382,322), target(262,322)
and proposed(366,322) in triangle12, with zero floor height. Point(414,322) is
outside that layer; the last broad layer is not a placement-floor substitute.
The whole segment262→366 stays in the same convex triangle. Checkpoint20 therefore
regenerates the same single-operand edit with X366 through real UI Apply,
Undo/Redo, Save/Load and Build, with context-handler calls0. The resulting saved
project SHA is`e85ccc24cd9806597c6e578710fcd796e0ab3af856f5dbfd61a78cf3ff5b51f0`.
Independent current-backend comparison again finds only event byte1281 changed,
with the same unrelated component preservation. The package/candidate hashes are
listed above. All failed attempts and the invalid414 package remain ignored local
artifacts; no product/game behavior or live preview was changed for these tests.


### Completed bounded host and production acceptance

The twelve required mode/host routes are retained without repeating a passed
matrix: checkpoint17 supplies all six Map0 routes (its later Map145 assertion
failed), and checkpoint23 supplies all six corrected Map145 routes. Each uses
the exact normal-UI package listed above, unchanged on both hosts. Absent and
disabled have exact equal memory/status/pose results; enabled has the expected
package identity, virtual index record and causal authored-state effect. The
unrelated Map22/Map23 index records remain original.

| Map/condition | Total frame603 | Total frame733 | Total frame1333 |
| --- | --- | --- | --- |
| Map0 absent/disabled X | not sampled | 0 | 0 |
| Map0 enabled X | not sampled | 32 | 32 |
| Map145 absent/disabled X | 273.025634765625 | 352.00732421875 | 273.025634765625 |
| Map145 enabled X366 fixture | 366 | 272.02197265625 | 272.02197265625 |

Native and browser agree exactly at each sampled checkpoint, including Y/Z,
memory digests, status, frame and VBlank counters. Frame603 is field+300,
frame733 is field+430, and frame1333 is field+1030. The early Map145 sample is
followed by the remaining130 frames; it preserves the original733/1333 endpoints.
Enabled movement is proved between603 and733 and differs from controls at all
three points. The later stationary sample is retained. This does not claim a
fixed displacement, continuous patrol after733, or input-triggered initialization.
Only Map145 uses this observation split; the existing other-map430-frame and
Disc2 title303-frame helper behavior is restored. Checkpoint24's current-source
checks pass after that test-only restoration; the executed Map145 path is unchanged.

Native and current-WASM replay also reproduce the final in-mesh project's exact
candidate. Checkpoint23's normal production Vite build uses a fresh staged app
with checkpoint15 SDK installed as its default package and test/override flags
unset. The built and actually served WASM both hash to
`ee135db37c4675bfaa0fefd9454b6b7e990fc794c40f8339a7f527bbbc268a30`;
the production test driver is absent, the exact member-view digest regression
passes, and the Three.js licence notice is intact. No old backend package was
substituted or overwritten. Current type/design/source/format/privacy checks pass.

Root reviewed the retained real game images: Map0's native sprite/test-room view
and Map145's native/browser library are readable. An initial thumbnail assessment
of the native Map145 image was corrected by reopening the actual file at original
resolution; no new screenshot was taken. Native captures are320×216 and browser
captures1440×686. Same-host native absent/enabled images differ in35/69120 pixels
around the actor at the late checkpoint; this is not a cross-host pixel-parity
claim and does not replace the source, state, trajectory and preservation evidence.

Additional retained attempts: checkpoint21 passed native/WASM replay but failed
its overly late movement assertion; checkpoint22 stopped at a mistyped headless
input before running tests; checkpoint19 stopped at a nonexistent reuse receipt.
The corrected fixture's real UI evidence is20 and its six-host route/production
result is23. No previous failed aggregate is rewritten as success. Exact commands,
scope IDs, stage exits, artifact identities and honest reuse are indexed in the
ignored `tmp/studio/offline-components/commands-and-results.md`.

The active preview remains the separately managed d4f480c3 checkout/74c1 backend,
with PID139863. These isolated tests do not integrate source, overwrite its
ignored outputs, restart the service or interact with the user's browser. The
five protected PR46 files retain their baseline hashes and unstaged state.


## General Scene texture repair — implementation plan (2026-10-24)

Bart reported gray geometry on multiple maps and authorized a format-level fix,
not a Map1 exception. Work starts from published96635ecd in the isolated
`studio-context-prerequisite-ux` checkout. The current preview may be updated only
after verification; preserve its TLS/network/disc/runtime configuration.

The missing input is the companion image stream: `field_map_stream_start`
selects file `0xb9 + 2*map` in directory4, whereas the Scene loaded only the
`0xb8 + 2*map` bundle. Original readers are
`resident/cd_reads_and_streams.c:stream_load_image_strip` and
`field/field_screen.c` field-load ordering. Header sectors (2048 user bytes)
contain type0x1200/0x1201, placement halfwords at+4, width at+12, first-image
count at+20, strip count at+24 and strip heights at+28. Each following sector
supplies one VRAM strip. Field mode uses placement mode0 and zero base offsets.
Other header types end the original stream; malformed recognized headers,
missing strip bytes, invalid rectangles and resource excess remain errors.

Ordered work/checkpoints:
1. Inventory actual companion streams on BOTH discs, separating real bundles
   and placeholders; retain counts, file identities and decoder limitations.
2. Add a bounded shared Rust strip decoder and `field.map_textures` session
   command through the existing incremental native/pull+binary-fill path.
   Return file identity and original VRAM uploads, never inferred pixels.
3. Merge stream uploads after bundle palettes/images in the Scene renderer.
   Keep them read-only and separate from authored documents/packages; rehydrate
   from the selected disc and preserve them on same-source project/history
   operations. Check async source/project ownership before adoption. Keep
   unresolved VRAM/texture state diagnostic rather than making it up.
4. Luna owns synthetic decoder/addressing/ordering/transport regressions and
   focused Rust/WASM/TS/browser runs, pinned tools in one outer16GiB/no-swap
   scope. Verify real texture coverage on multiple maps on BOTH discs and
   readable rendered output, source preservation, load/history and stale-source
   guards. Astra reviews the final source/provenance/scope diff.
5. Record exact commands/artifacts/failures, source/private-input hashes,
   current production build and test-hook absence. Commit a coherent reviewed
   fix; the preview update preserves the game module and access settings.

No game-byte editing, decomp changes, guessed inherited state, sprite VM or
unrelated rendering redesign is included. No full runtime suite is required
for these standalone Studio asset-reader/preview changes.


Texture repair preflight: the corrected complete inventory
`tmp/studio/scene-textures-20261024/both-disc-real-stream-inventory.json` scans
all730 registry slots on each disc, skips525 Disc2 short-bundle placeholders,
and validates all935 real companion streams (730 Disc1,205 Disc2). They supply
75662 and21936 sector-strip uploads respectively, with maximum companion555008
bytes and maximum single-image240 strips. No malformed real stream was observed
by the independent Python format inventory. This is stream/container coverage,
not935 Scene-open or gameplay passes. The earlier
`original-stream-inventory.json` D2 scan was only maps0..204 and included
placeholders; it is retained and superseded for complete Disc2 coverage.

Astra's source review found no bounds/ownership/cache defect, and corrected one
load-order claim: ordinary field entry loads bundle then companion; transition
kinds3/5 start streaming before the bundle load and finish after it
(`field_screen.c:779-816`). The preview explicitly uses ordinary-entry order;
it does not claim arbitrary transition/event VRAM replay. Missing inherited
pixels and existing Scene parser/context limitations remain explicit.


### Texture repair verification checkpoint

The production path contains no map-number, slot or source-hash exception. Native
Rust decoder coverage independently reproduces the inventory: all935 real
companions finish their advertised image counts, with97598 total strip uploads;
525 Disc2 placeholder bundles are classified separately. This does not remove
the previously documented Scene parser/context limitations on other maps.

Successful focused evidence under ignored
`tmp/studio/scene-textures-20261024/`:
- Checkpoint27:11 Rust texture tests,12 focused TypeScript texture/context tests,
  command operation/version regression, source/type/format checks.
- Checkpoint30/32: real aligned pending-range/fill/ready synthetic companion
  transport, explicit empty companion, unchanged exact synthetic boot identity;
  strict Clippy and current source checks. No identity allowlist was expanded.
- Checkpoint31: actual native original-disc stream sweep (730+205), plus empty-
  companion protocol regression. Source data and Cargo.lock remain unchanged.
- Checkpoint33: fresh WASM/bindgen/Vite test build, SDK and dist WASM both
  `0d7bd3955f4a5cea8cd376bfb4f9a0ba13449d87be25f5312f2abbae5de04c19`.
- Checkpoint34: normal browser disc/map opening and companion range filling,
  Scene open, static hierarchy selection, layer controls, Frame selection and
  Scene/List/Scene navigation, with original compiled-bundle SHA equality.
  Disc1Map1:147 mounted textured meshes,6705 triangles,30 page/CLUT pairs;
  Disc1Map2:122/5676/29; Disc2Map23:52/6673/8. Exact companion hash/size/slot
  matches the native inventory. All three1440x900 PNGs were inspected at full
  size: textured village grass/buildings/rock and dark forest surfaces are
  visible. The forest capture is not a runtime-lighting or sprite-fidelity claim.

All test invocations used pinned Nix/Rust/browser inputs and one outer user
scope per batch with MemoryMax16G,MemorySwapMax0, finite measured timeouts. Exact
commands, source manifests, scope invocation IDs, stage exits and private-input
hashes are retained in the ignored runners/output directories and verification
ledger. No full runtime/gameplay suite was repeated for this standalone preview
reader change; existing game-module/package evidence is unaffected.

Retained failed attempts are not rewritten:25 lacked required invocation env;
26 exposed two missing disc-method mocks (93 other frontend tests passed,3
skipped), corrected by27;27's new transport fixture violated chunk alignment;
28 changed its test boot identity;29 asserted identity before pending open
completed and captured a pre-format source hash;30 corrected those harness
issues while preserving the original boot and production validation.31 Clippy
found unused fixture helpers, corrected with feature scoping at32.33's browser
case was interrupted with exit130 after getting stuck on Scene's collapsed map
controls, and its unhelpful far-overview Map1 screenshot is retained.34 uses
normal Command-list navigation and qualified static-instance framing; no forced
clicks, invented geometry or camera-code workaround. The earlier partial Disc2
Python inventory is also retained with its explicit coverage correction.


Final texture production checks: checkpoint36's fresh staged application builds
with the default package selection (no test flag/package override), design audit
and typecheck pass. Checkpoint37 serves the exact new WASM hash above, excludes
all test hook markers, carries the exact pinned Three MIT notice, and passes
both the production browser test and canonical packaged NW.js smoke (new
operation/version, Node file adapter and window controls). The NW smoke disables
GPU as its established filesystem/protocol test requires; it is not new native
GPU texture-rendering proof. Browser34 is the actual rendered-texture evidence.

Production-validation harness failures remain honest:35 initially gated on the
aborted33 aggregate instead of its successful build stages;36's asset-check shell
wrapper had a quoting error;37's final receipt comparison compared different
absolute/relative path spellings despite equal hashes.38 verifies the normalized
digests and all successful individual stage receipts, without repeating tests or
builds. The old isolated default SDK is byte-identical; all new builds use fresh
ignored artifacts. The denied M6 diagnostic and game/runtime full suites were
not retried. Source/provenance review found no new licence grant, private game
asset, source identity bypass or protected-checkout edit in the fix.
