# Studio milestone 2: field-script round trip

This milestone is backend-only. It adds `field.decode` and `field.encode` to
`xem-studio`, retaining the existing response envelope, version and arbitrary
JSON echo. It adds no editor UI, relocation, script editing or runtime hooks.

## Implementation

1. Export the parsed authoritative tables from `tools/analysis/events.py` into
   committed public opcode metadata. Include every form's mnemonic, handler,
   size, flow, operands, effects, selectors, alternative/inner advances and
   setup relation, plus operand widths/prefix/schema provenance. A deterministic
   generator and stale-output test prevent a second maintained opcode registry.
2. Implement the component parser and reachable walk in Rust over that metadata,
   following `docs/scripts/field-events.md`, `event_package`, `script_entries`
   and `walk`. Preserve u16 PC wrapping, extended fallback, selector/operand
   interpretation, alternative skip rules, setup pairs and inline jump tables.
3. Represent variable-type bytes and actor/event rows explicitly. Preserve the
   supplied stream and its optional declared component boundary separately.
   Instructions expose exact PCs/spans, selected form, raw operands and flow.
   Storage blocks partition bytecode without gaps/overlaps: ordinary basic
   blocks, compound paired/table units and pinned map-entry/unreachable/overlap
   regions. Edges retain target PCs and distinguish followed, interior and
   out-of-stream paths. No original component blob substitutes for these nodes.
4. Encode by assembling typed header fields and each block at its original
   offset. Require complete nonoverlapping coverage, consistent spans/entries/
   instructions/edges and source SHA-256 identity. Re-decode the reconstructed
   stream and require the canonical graph to agree. This deliberately supports
   unedited round trips only; edited/appended/relocated bytecode is milestone 6.

`field.decode` arguments are `{bytes: [0..255], declared_size?: number}`.
The default declared size is the supplied stream length. A supplied decoded tail
is retained, bounded by the original loader's 16-byte allocation slack.
`field.encode` arguments are `{graph: <the returned graph>}`; its result is the
byte array. All replies use `{ok: result}` or `{error: {kind, message}}`.

Malformed headers, truncated reachable instructions and unsupported opcodes are
hard errors. The Python sweep explicitly retains known control-flow hazards
(such as map 489's outside-stream successor) and overlapping views. These remain
visible diagnostics/unresolved edges in a lossless inspection graph; they do
not claim valid execution, invent allocation bytes or silently normalize data.
The graph's diagnostics must be inspected before treating it as executable.

## Acceptance and execution order

- Deterministic metadata regeneration/check passes, and Python tests compare all
  metadata fields against the current authoritative tables.
- Synthetic Rust tests cover each documented special span/flow, operand modes,
  header/entries, basic blocks, paired/table units, pinned gaps, u16 edge cases,
  malformed/unsupported inputs and encoder invariant rejection.
- Native command and wasm checks exercise both new operations, whole-component
  round trips and version/echo/error regressions with the existing envelope.
- A local test gated by `XEM_STUDIO_EXTRACT_ROOT` reads an existing root containing
  `extract/disc{1,2}` and `discs/disc{1,2}.bin`. First run the authoritative Python
  sweep; then compare every map's instructions, forms/operands, ordered
  successors, skip decisions and diagnostics, not just aggregate totals.
  Require exact full-stream and declared-component byte round trips. Record
  per-disc map/placeholder counts and input/output hashes; report mismatches,
  absent inputs and skips explicitly. Private data/oracle details stay ignored.
- Run package-scoped Rust tests/lints, wasm build/parity, relevant Python tests,
  metadata/source audits and focused protocol regressions. Reuse unaffected
  frontend and runtime evidence; do not launch a multi-platform runtime suite.
- Use pinned Nix and one outer user systemd scope (`MemoryMax=16G`,
  `MemorySwapMax=0`) for tests/build checks. Delegate test implementation and
  execution per home guidance. Preserve prior logs and monitor disk headroom.
- List new authored sources and generated public opcode facts in
  `packaging/source-files.txt`. Never commit game components, extracts, private
  sweep oracles or listings. No `src/decomp/` changes, push or later milestones.

The implementation starts from clean `a71733da` in the existing
`modding-toolkit` worktree. Existing main-checkout inputs are read in place;
main PR46 files and other worktrees remain untouched.


## Graph contract

All offsets in instructions, blocks, entries, edges and diagnostics are relative
to the bytecode beginning at `bytecode_offset`; component sizes include the
header. `stream_size` includes the retained tail, `declared_size` excludes it.
`schema_version` is 1. `source_sha256` identifies the entire supplied stream.
`variable_unsigned_bits` and `actors` expose the complete typed header, including
zero actor entries skipped by the `ff` map-record rule. `entries` contains active
actor/event/PC triples; `skipped_zero_entries` counts only that documented rule.

`instructions` is sorted by PC. Each selected `spec` contains the generated form
facts, including effects and operand descriptors; raw `operands` and nullable
`immediate` flags retain Python interpretation. `size` is the logical dispatch
advance. `ranges` lists physical half-open consumed ranges, including a wrapped
extended prefix at PC ffff. Operand reads/selector look-ahead can extend beyond
the advance; the complete storage partition retains those bytes as well.
`successors`, `skips` and `inner` preserve ordered interpreter paths.

`blocks` partition all supplied bytecode with explicit `[start,end)` spans,
`bytes`, `kind`, `reason` and member `instruction_pcs`. Basic blocks are split
at entries and control-flow boundaries; compound setup/step pairs and variable
jump tables stay together. Overlaps/wrapped views, map-entry records and
unreachable gaps are pinned. This preserves opaque bytes without representing
the whole component as an undifferentiated input blob. `edges` retain source/
target PCs and block indices (nullable target for paths outside the stream),
flow kind and `followed`, `not_followed` or `outside_stream` status. Interior
continuations remain visible even when they are deliberately not traversed.

`diagnostics` mirrors Python walk order for overlaps, unpaired steps, inner
continuations, variable tables, non-whole-instruction skips and outside paths;
`beyond_declared` identifies advances into supplied stream tail. These are
inspection limitations, not silent repairs. Hard input errors include
`truncated_input`, `unsupported_instruction`, `entry_outside`, `invalid_pc`,
`invalid_header` and `invalid_declared_size`. `invalid_graph` rejects invalid or
modified encoder input; `invalid_args` reports typed JSON schema violations.

## Metadata provenance and regeneration

`src/runtime/crates/xem-studio/data/field-opcodes.json` contains public opcode
facts generated only from the independently authored analysis tables in
`tools/analysis/events.py`. It contains no original game component bytes,
assembly, private extracts or sweep output. Its source SHA-256 identifies that
Python module; the JSON includes all parsed forms and supporting table facts.
See the adjacent data README for the source policy. Regenerate/check from the
repository root in the pinned shell:

```sh
python3 -m tools.studio.field_opcodes
python3 -m tools.studio.field_opcodes --check
```

The consistency test compares the complete deterministic bytes and parsed schema
against the authoritative Python objects, so a Python-table change cannot leave
stale Rust metadata unnoticed.


## Example commands

With an existing `Studio` instance, these JavaScript calls build an invented
component (one actor, all events at an `end_slot`) and exercise both JSON
operations. They contain no original game data and work through the same
`command` method used by native callers and wasm:

```js
const bytes = new Array(197).fill(0);
bytes[128] = 1; // u32 actor count; bytecode begins at 196
const decodeRequest = { op: "field.decode", args: { bytes } };
const decoded = JSON.parse(studio.command(JSON.stringify(decodeRequest)));
if (!Object.hasOwn(decoded, "ok")) throw new Error(JSON.stringify(decoded.error));
const encodeRequest = { op: "field.encode", args: { graph: decoded.ok } };
const encoded = JSON.parse(studio.command(JSON.stringify(encodeRequest)));
if (!Object.hasOwn(encoded, "ok")) throw new Error(JSON.stringify(encoded.error));
if (JSON.stringify(encoded.ok) !== JSON.stringify(bytes)) throw new Error("round trip changed bytes");
```

The requests are ordinary JSON: `decodeRequest` uses `field.decode` with a byte
array; `encodeRequest` supplies exactly the returned graph to `field.encode`.
No UI beyond the existing JSON command panel is added in this milestone.


## Running the local checks

From the repository root, use the existing pinned runtime shell and one outer
user scope per invocation. For example:

```sh
. ~/.nix-profile/etc/profile.d/nix.sh
systemd-run --user --scope -p MemoryMax=16G -p MemorySwapMax=0 \
  nix --extra-experimental-features 'nix-command flakes' --offline \
  develop path:./nix/runtime --command \
  cargo test --manifest-path src/runtime/Cargo.toml --locked -p xem-studio --test field
systemd-run --user --scope -p MemoryMax=16G -p MemorySwapMax=0 \
  nix --extra-experimental-features 'nix-command flakes' --offline \
  develop path:./nix/runtime --command \
  env XEM_STUDIO_EXTRACT_ROOT=/existing/local/root \
  cargo test --manifest-path src/runtime/Cargo.toml --locked -p xem-studio --test field_sweep -- --nocapture
```

The extract root must already contain both `extract/discN/manifest.json`, their
listed map files, and `discs/discN.bin`, using the existing extraction workflow.
No disc data is downloaded. With the environment variable unset, `field_sweep`
prints an explicit skip and returns without coverage; an ordinary green Cargo
result for that gated test is not evidence of a disc sweep.

The Rust test starts `tools/studio/tests/field_oracle.py` with pinned Python.
It creates the ignored artifact parent, reserves a fresh run directory without
overwriting prior evidence, and lets Python create a fresh `components/` child.
Python reads the existing extracts in place and writes only small decompressed
event components there; no archives or whole extraction trees are copied.
Private components remain ignored, and nothing is cleaned up automatically.
Python stdout contains only paths, counts and hashes. Its original stderr is
preserved in the fresh run directory, including failure diagnostics.

For each map, Rust compares the typed header/entry digest and every reachable
PC's canonical JSON semantic digest: opcode/extended byte, advance, complete
selected form (including effect/operand descriptors), raw operands, immediate
modes, ordered successors, skips and inner paths. Diagnostic lists retain walk
order. It then encodes and compares the entire stream directly with the local
input, and checks its declared-prefix SHA-256. Aggregate counts alone do not
satisfy this test. No private bytes or raw operands are printed on a mismatch.


## Verification result (2026-10-20)

Milestone two passed the available-input acceptance checks. All commands used
the pinned runtime Nix environment and one outer 16 GiB/no-swap user scope.
Tool versions were Rust/Cargo 1.97.1, Python 3.14.7, Node 24.19.0 and
wasm-bindgen 0.2.127. The generated metadata contains 508 forms over 255 primary
opcodes and 227 extended opcodes; the primary prefix is represented separately.

| Check (inside the scope and pinned shell) | Result |
| --- | --- |
| `cargo test --manifest-path src/runtime/Cargo.toml --locked -p xem-studio --test command --test field` | 5 command + 9 synthetic field tests passed. |
| Same Cargo invocation with `--test field_sweep`, `XEM_STUDIO_EXTRACT_ROOT=/home/bart/xenogears_toolkit/.local` | Both-disc per-PC semantic/header/entry/diagnostic parity and exact unedited byte round trips passed; 143.43 seconds. |
| `python3 -m unittest -v tests.test_events tests.test_studio_field_opcodes tests.test_workflow.SourceBoundaryTests` | 25 passed. |
| `python3 -m tools.studio.field_opcodes --check` | Exact generated metadata consistency passed. |
| `python3 tools/repository/source_archive.py --check` | 1,276 allowlisted public source/reviewed-data files passed. |
| Package `cargo fmt -- --check`, pinned `cargo-clippy clippy ... --all-targets -- -D warnings`, focused Ruff | Passed. |
| `python3 tools/studio/build-backend.py`, `npm --prefix tools/studio run build` | wasm32 bindings, frontend typecheck and production build passed. |
| `npm --prefix tools/studio run test:e2e -- tests/e2e/field.spec.ts` | 3 passed, including complete native/wasm graph equality and encoding. |
| Same Playwright command for `studio.spec.ts --grep 'shows all workspace panels'` | Updated four-operation version display passed. |
| `npm --prefix tools/studio run test:nw` | Built desktop smoke passed: four operations, native folder access, echo and window controls. |

Disc one covered 730 real maps and 572,016 reachable instructions. Disc two
covered 205 real maps and 206,124 reachable instructions, with all 525 placeholder
slots accounted for. All 205 shared declared packages were identical in the
Python baseline. The Rust test checked all 935 real maps individually, including
all 778,140 reachable instructions. No unknown or undecodable reachable paths
were reported by the baseline. Known diagnostics remain visible: map 222's
non-whole-instruction skip and map 489's overlapping view, supplied-tail read
and outside successor occur on both discs. They were compared, not suppressed.
This is a lossless inspection/round-trip result, not execution validation or
source-coverage measurement.

Exact commands, retained failed attempts, tool/source/artifact hashes, local
input identities and results are under ignored
`tmp/studio/milestone-2-20261020/checks/verification-evidence.md` and its linked
logs. The baseline's per-map hashes are in `python-map-hashes.json`; Rust's
per-instruction comparisons and output byte checks ran in memory. No private
map bytes were printed or committed. The sweep's local component files and
stderr remain in its fresh ignored `oracle-run-*` directory.

Final native tests and lint checks include the condition-collapse fixes requested
by Clippy. Earlier disc-sweep, wasm, browser and NW evidence carries through
those mechanical changes; no interpreter or graph behavior changed. Initial
fixture/setup failures are retained rather than relabeled as passes. The full
runtime suite, unchanged presentation journeys/Vitest and development TLS/NW
configuration checks reuse the previous milestone/cleanup evidence at
`a71733da`; they were not rerun for this standalone backend addition. No editor
UI, game editing, runtime integration or later milestone is included.
