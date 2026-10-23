# Milestone-five implementation plan

Baseline: `cd2686e215f6d23a1c92a1305679a27639d0d3f6`, `modding-toolkit`.
Scope: [studio-milestone-5.md](studio-milestone-5.md). The accompanying M7
roadmap clarification is documentation only. Sol/high owns production code,
Luna/high tests and execution, Astra/medium independent correctness review.

## Preflight and integration decisions

- `xem-disc::SectorSource` supplies raw MODE2/2352 sectors. The game takes the
  2048-byte Form1 window for ordinary files. `index.rs` defines the 0x8000-byte
  index at LBA24, directory table at LBA40, and EXE offsets 0x804/0x8804.
  `xem-game::boot_source` and `XemApp::boot` load the identified executable
  directly into RAM; wrapping sectors alone misses that embedded index.
- The original index copies have identical complete seven-byte records but
  different opaque final bytes (offset 0x7fff) on both discs. Validate all
  complete records and directory entries; patch only target records in each
  copy, preserving its own trailing byte. Never copy that opaque byte between
  the on-disc and embedded indexes.
- Keep the verified original identification/digest. Patch a separate loaded EXE
  view plus its raw-sector copy and the LBA24 index. Both boot and subsequently
  inserted sibling discs must use the same immutable package-set overlay.
- Every delta targets the verified vanilla file. Explicit later overrides replace
  earlier results completely; they do not chain deltas over modified bytes.
  All earlier conflicting owners must be explicitly named in overrides.
- Add the shared format/delta/overlay implementation under `xem-disc::mods`.
  `xem-studio` provides typed build/inspect/bundle commands and a native CLI.
  Native and browser consume identical prebuilt bytes. No new graphical manager
  is required; no UI/mockup gate is triggered by these runtime/CLI entry points.
- Initial media support is ordinary MODE2 Form1 data files only. Reject directory
  records, missing/unused slots, XA/Form2/movie/mixed layouts, invalid extents,
  and metadata/boot overlaps. Validate signed file size, 24-bit LBA and the
  tighter two-digit BCD MSF bound. Replacements use deterministic appended
  virtual extents, zero-padded final data sectors and valid regenerated Form1
  EDC/ECC. Untouched original sectors are passed through byte for byte.
- Browser original prefetched/CHD caches remain original. Shared preparation
  retains bounded per-file progress across `NotReady` using small sector batches,
  preventing a replacement larger than the source cache from restarting forever.
  Source buffers are lazy: only the active file allocates, with allocation
  released when that file finishes. Many tiny-result deltas cannot eagerly
  reserve all vanilla sources; `resident_source_bytes` exposes this budget.
  Prepared overlays are keyed by the exact package-set generation, immutable
  once published, and discarded/reprepared on a staged set change.
- Package changes are staged for a new session. Validate contracts/order/content
  before replacing the active session; invalid input preserves that session.
  A package must declare every disc it is applied to, including mounted siblings.

## Public format and bounds

Version 1 has a binary magic/version, bounded strict JSON manifest, concatenated
payloads and a SHA-256 seal. Manifest fields: `id`, declared disc serial/boot
SHA-256 pairs, ordered dependency IDs, override IDs, and sorted `(disc, slot)`
file entries with payload kind/range, source hash for deltas, result size/hash.
No executable scripts or paths in package contents. Reject unknown fields,
unknown versions, duplicate entries/IDs, noncanonical or overlapping payload
ranges, invalid hashes, and undeclared conflicts. Authored entries hold entirely
authored bytes; delta entries hold copy-from-source/insert instructions.

The delta format is versioned copy/insert bytecode with explicit source/result
lengths and strict bounds/trailing-byte checks. The deterministic builder emits
source-copy operations for matching regions and literals for changed bytes.
No original source file is included as a package member. Source hash is checked
before applying a delta, result size/hash afterwards. Public synthetic fixtures
and code are committed; real originals/results/packages remain ignored.

Initial bounded limits: 32 packages; 1,024 files/package; 64 KiB manifest;
64 MiB/package; 128 MiB aggregate packages/results; 32 MiB source/result per file.
Integer and sector checks remain required independently of those limits.
A length-prefixed versioned package-set container carries packages in the exact
load order. Dependency IDs must precede their dependent; cycles/missing/order
errors are refused rather than sorted implicitly. Disabled sets activate `[]`.

## Persistence policy

Snapshots/rewind retain the existing version-9 identity record and ordered
`Vec<String>` package tokens, encoded unambiguously as `id@sha256` (IDs exclude
`@`). Validate before mutation using the existing restore gate. Do not add an
unrelated state migration. Replay logs gain an explicit identity record checked
before scheduling/resetting controls; legacy untagged logs are vanilla-only.

Ordinary game/card bytes stay unchanged. A separate sealed, versioned save
wrapper binds the base game/disc family and ordered package tokens to the raw
card payload. Module builds do not make original game cards incompatible.
Vanilla raw original-card/single-save import/export remains supported. Untagged
saves are vanilla-only: modded sessions require a matching wrapper, except a
new absent card may be created and subsequently stored with the active binding.
No automatic rebinding of an existing raw card to mods. Validate before both
insertion and persistent storage, including browser `Cards.replace` and boot.
Pending browser card writes retain their originating binding and settle before
session replacement; they are never encoded under a later active package set.
An unavailable store with no queued card writes retains its visible read error
and permits the existing volatile-session behavior. Failed pending writes still
prevent session replacement until an explicit retry succeeds.
Wrapper incompatibility is a fatal preflight, before `Card::attach_storage`
(which otherwise records load errors and leaves empty slots).
Modded exports use the wrapper; raw export must not silently discard metadata.
Native writes use the existing atomic whole-file replacement; browser IndexedDB
stores wrapper and payload together. Refusals preserve prior bytes/session/card.

## Ordered implementation checkpoints and acceptance

1. **Core proof.** Luna authors failing synthetic size-changing/delta/index
   fixtures against the shared API. Implement deterministic build/decode/order
   validation, resumable preparation and virtual sector reads. Prove both actual
   index views, bounded headers/EDC/ECC, padding/cross-sector reads, unchanged
   data and absent/disabled passthrough. Record the first actual failing check.
2. **Builder and CLI.** Expose package build/inspect/bundle in Studio's typed
   `{ok:...}/{error:{kind,message}}` boundary without changing existing methods.
   Add a thin native CLI over the same shared builder; document exact contracts.
3. **Native/browser integration.** Stage identical package-set bytes at native
   startup and through a browser binary method/API. Finish all validation before
   swapping sessions. Exercise missing ranges, generation/cache isolation,
   ordered status identities and rejected loads preserving the old session.
4. **Persistence.** Shared save wrapper/storage adapters, import/export gates,
   replay identity and unchanged snapshot gate. Test same set succeeds; altered
   hashes/order/dependencies/absence refuses before state/storage mutation.
5. **Real acceptance.** Build one small ignored delta package from existing
   user-owned disc inputs. Enter a real field message legally and visibly render
   the authored change in native and browser hosts, recording text/screenshots,
   matching package/module identities, untouched file reads, and absent versus
   disabled vanilla `memory_hash` over the same replay. Regress M4 real attach,
   exact frames/schema identity and snapshot restore/mismatch.
6. **Final integration.** Once runtime inputs stabilize, run focused checks then
   one required full runtime suite. Source/provenance/format checks, independent
   review, local scoped commits and clean state. Restore the owned LAN preview
   if stopped. Stop after M5, with no push/PR/merge or later implementation.

All tests use pinned `nix/runtime`, one outer user systemd scope with
`MemoryMax=16G`/`MemorySwapMax=0`, finite timeouts and fresh ignored paths under
`tmp/studio/milestone-5/`. Record exact commands, source/build/package/disc
identities, results and every skip/failure. Reuse compatible prior evidence.
If an isolated full-suite harness is necessary to preserve old artifacts, retain
its exact diff and distinguish that run from unmodified canonical execution.
The five protected main-checkout hashes are checked at preflight and completion;
no `src/decomp/`, `prompt.md`, security/trust or original-image changes.

## Host entry points and ordinary-card envelope

Native `xem-headless` and `xem-desktop` accept `--packages SET.xemmods`.
`--packages-disabled` still validates that container but activates the empty set.
Without either option, source sectors and original-card exports remain vanilla.
`status.identity.packages` exposes the ordered `id@sha256` tokens in both hosts.
The embedded native game module remains the game's logic provider.

In the browser, stage the same bytes before a fresh boot:

```js
window.xem.packages.stage(new Uint8Array(await file.arrayBuffer()), { enabled: true });
await window.xem.boot();
```

Use `{enabled: false}` to validate a set while booting vanilla. Staging never
changes an active session. The browser prepares through existing missing-range
pulls before publishing a fresh instance; original chunk/hunk caches contain
only original bytes. Session replacement waits for originating card operations and invalidates older
asynchronous frame continuations when boot begins, including a failed attempt.
A refused set, disc, card identity or module does not replace the active session.
During asynchronous boot preparation, Run, direct run-state changes and snapshot
restores refuse explicitly; scheduled window/XR game frames stay paused.
No package-file script, Node API, privileged origin or new graphical manager is
involved. This API does not persist the selected package set automatically.

Ordinary modded cards use `XEMSAVE1`, a little-endian u32 strict-JSON metadata
length, `{disc, packages}` metadata (at most 16 KiB), the exact 128 KiB raw card,
and SHA-256 of all preceding bytes. The disc value is the existing shared
supported-disc family identity, allowing compatible progress across disc 1/2.
Unknown fields, malformed lengths, checksum and identity mismatches are errors.
The original card structures, payload checksums and game save code are unchanged.
Storage and export use this whole envelope; import unwraps only under a matching
active identity before the existing card/save parser. Vanilla import still
supports the original container formats; modded import refuses untagged saves.
A missing card creates a fresh formatted card and acquires its immutable binding
when attached. Native atomic writes and browser IndexedDB keep metadata and
payload together. Existing malformed vanilla files retain the original empty-slot
load diagnostic rather than being overwritten. No archive migration is added.

`record` includes `identity`; supply it unchanged with `replay`. Authored
`schedule` logs for a modded session must also include its exact `identity`.
Untagged legacy replay/schedule input is accepted only in vanilla sessions.
Snapshots and rewind retain version 9 and the existing before-mutation identity
validation; no new snapshot format is introduced.
