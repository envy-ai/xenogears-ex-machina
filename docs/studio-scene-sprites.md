# Scene sprite preview

## Authorized result and boundary

Bart requested sprites in the existing Scene after the general texture repair
0ac5c701. Implement general original-resource decoding and source-qualified actor
previews, not map/actor artwork exceptions. Work in the existing isolated
studio-context-prerequisite-ux checkout. Keep the approved Scene layout, inline
properties, stable picking, source placement and history. No new control layout
or image asset generation is needed. Preview refresh is authorized after checks;
TLS/network/disc/game settings and protected main PR46 files remain unchanged.

This is a source preview, not execution of field gameplay or arbitrary animation
VM state. Display the first provable image of the source's initial animation at
qualified authored placement, using actual parts/palettes/texture uploads. Do not
substitute a party character, frame, pixel, heap byte or runtime position. Valid
state-dependent sources remain explicit unresolved markers with diagnostics;
malformed data is a structured error. New animation playback/editing controls,
battle effects and actor/game data edits are outside this request.

## Source facts and implementation order

1. Read actual resident SpriteResource/SpritePart/SpriteCell definitions and
   sprite_resolve_resource, sprite_apply_animation_header, sprite_set_facing,
   sprite_show_indexed_frame, sprite_build_frame, sprite_build_cell_frame and
   sprite_draw_parts. Use docs/scripts/sprite-vm.md and tools/analysis/sprite_vm.py
   as the existing format oracle; retired reconstruction prose is context only.
2. Bounded Rust component3 resource reader, relative section/header/frame/cell
   spans, palette banks, first-frame command qualification, signed offsets,
   per-part/group controls, flips and ordering. Preserve unknown/reserved data.
   Normal frames supply upload cells; cell-directory frames reference existing
   source VRAM. Missing resident/sequencer data is not invented.
3. Resolve first sprite construction in actor initialization from canonical
   field graph/operand metadata. Only proved immediate local resource/bank
   bindings are drawn. Unresolved branches, party/global/variable selectors,
   unsupported pre-frame VM commands and missing pixels keep diagnostics.
   Bindings follow stable authored actor IDs through edits/history; no second
   editable actor/opcode registry. Source-facing0/default field scale are explicit
   preview state, not a claim of the eventual runtime animation/visibility.
4. Compose original parts as camera-facing pixel sprites, respecting source
   part offsets, palettes, transparent words, per-part/whole-frame flips and
   authored origin. Existing picking/Frame/transform flows keep the same actor
   identity and do not edit bytes when viewing. Keep unresolved identities
   distinguishable. Read-only observed runtime pose remains separate.
5. Luna owns tests/runs, Astra critic; root integrates. Synthetic bounds,
   arithmetic, frame/table/command variants, controls, palette/pixel addressing,
   binding qualification and stable history/no-edit regression, native/WASM
   parity, plus real source metadata/coverage across both discs. Browser evidence
   must show readable original sprites, normal selection/Frame/orbit/layers,
   and preservation through view/history/project ownership. Tests in pinned Nix
   and one outer16GiB/no-swap scope, finite timeouts, fresh ignored artifacts.
6. Current production build/hooks/source/provenance checks and relevant normal
   desktop smoke; retain failures/skips and unavailable formats honestly. No full
   runtime suite for this standalone reader/preview change. Review, scoped commit
   and verified owned-preview refresh; normal GitHub/no-PR standing direction
   applies at a clean reviewed checkpoint. Never modify src/decomp or originals.

## Initial source inventory

Ignored metadata: tmp/studio/scene-sprites-20261024/source-inventory.json. Samples
Disc 1 maps 1/2/23/145 and Disc 2 maps23/400 contain10/5/5/1/5/8 local resources;
all are cell-directory frames, so the existing map/sprite VRAM uploads must be
reused rather than assuming every frame embeds pixels. Most initial scripts
start with indexed-frame13; one uses pushB4 before15. Forest effect resources
use sequencer/image upload/spawn state and must not be presented as simple NPC
frames. All sampled resource signatures validate with the Python oracle.
First metadata diagnostic failed on a wrong Python Instruction.size attribute;
corrected to the actual length property without changing source/data, retained
in session evidence. No original pixels are included in public sources.

## Actor coordinate projection

The source header retains unsigned16 position words. `field.c` loads these into
matrix translations; `field_motion.c::field_actor_reset` assigns each through a
32-bit `<<16` to signed16.16 actor position (and casts X/Z to signed16 for floor
lookup). The viewport therefore projects event-actor header words as signed16,
without changing the saved source words. Header gizmo moves encode signed16
world coordinates back into unsigned16 words; out-of-range targets remain invalid
rather than wrapping. Explicit selected script placements already use their
signed operand semantics. Static instances do not undergo this actor reset and
retain their separate matrix projection. Collision-derived height and later
initialization overrides are not inferred by this initial-source preview.

## Verified result

The read-only component3 inventory covered 935 real bundles: 730 Disc 1 and 205
Disc 2, with 525 Disc 2 placeholders reported separately. It found 4395 resources,
4103 qualified initial image references and 292 state/effect-dependent references;
there were no header, packed-stream or sprite-reader errors. These are resource
parser counts, not a claim that 4395 actors are visible or every map has been
opened interactively. The Python sprite-VM sweep also passed.

Focused acceptance passed 10 Rust sprite tests; 24 existing bundle regressions;
14 frontend unit tests in 3 files; pinned format/Clippy/typecheck; and 146 exact
native/WASM request/reply trace steps including nonempty sprite resources,
malformed-resource diagnostics and snapshot bindings. Browser tests opened
Disc 1 maps 1/2 and Disc 2 map 23, checked real source bindings and unchanged bundle
hashes, then exercised ordinary sprite picking, Frame, camera orbit, layer
visibility and List/Scene retention. Actual 1440x900 images show original sprites
with the restored textured environment; no replacement art is used.

The default-package production build consumed the new backend with SHA256
`7188bedd1a962de5968cef921063c8e7a3c2e15efd76490dde85e4a774496f90`.
Both production browser checks and the normal built NW.js smoke passed. Served
WASM identity and absence of test hooks were checked, not inferred from a build.
No game/runtime execution inputs changed; the full runtime suite was not run.
Original disc hashes, locked dependencies and the protected PR46 files remained
unchanged. The reader/renderer received independent source review.

Ignored exact commands, per-stage exit codes, source identities, preserved failed
attempts and screenshots are under `tmp/studio/scene-sprites-20261024/`.
The final focused batch is output62; production/desktop is output64. The unchanged
resource inventory/bundle evidence is reused from 49 and native sprite tests
from 55. Failed setup/fixture/assertion attempts remain alongside those results;
they are not presented as passing runs. Unsupported VM/effect/party/inherited
states retain diagnostics rather than claiming full animation or runtime-pose
fidelity.

## Sprite size correction

The first preview used six source units per pixel, omitting the original view
scale. `field.c` initializes `field_view.scale` to `0x3000`; the composed
`scaled_world` matrix therefore multiplies geometry and actor positions by three.
`field_draw_sprites` instead uses the separate orbit matrix for billboard part
geometry, scaled by the default actor scale times 3/4 (`0xC00`).
`sprite_draw_parts` shifts part positions/sizes by 3. The conversion back to the
editor's unscaled source coordinates is therefore `8 * 0xC00 / 0x3000 = 2`
world units per pixel, one-third of the previous preview size. Parts, their
shared authored pivot and conservative Frame bounds use this one conversion.
Actor placement/source words, original pixels and authored documents are not
changed. Later script-driven scale changes remain outside the initial-state
preview. The derivation received independent source review.

The correction passed nine focused sprite unit tests (including pivot and Frame
bounds), typecheck, a fresh frontend build and the existing Disc1 maps1/2 plus
Disc2 map23 browser selection/Frame/orbit journey. New screenshots and exact
commands are retained under `tmp/studio/scene-sprite-scale-20261025/output-65/`.
The unchanged backend SDK and original disc hashes were verified; the native
reader/parity/runtime and desktop evidence above is reused rather than rerun.
