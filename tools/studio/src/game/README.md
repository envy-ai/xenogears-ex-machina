# Live runtime link

The game remains authoritative. These modules call its existing JSON command
layer directly; they never route live mutations through `Studio::command`.

- `protocol.ts` preserves unsafe integer tokens as `ExactInteger`. It recognizes
  the runtime's canonical sole `{error: string}` refusal and preserves other
  objects. This runtime wire format is separate from Studio's envelope.
- `native.ts` spawns an explicit executable and argument array, with `--control
  --control-paused`, explicit disc/settings/cards paths and no shell. It owns
  only that child. Complete stdout lines are replies; stderr is diagnostic,
  with a bounded tail included on EOF/exit. A malformed/unsolicited reply or
  timeout invalidates the pipe. Closing explicitly terminates the owned child.
- `browser.ts` uses a same-origin window's `window.xem`. It checks API and
  successful-boot generation before/after operations and while idle. Frames use
  `run` to finish disc fetches, then raw JSON status to retain integer precision.
  Paused state and exact frame-counter delta are verified across that async work.
  Runtime frame continuations capture the boot generation and receive the link’s
  AbortSignal; loss, timeout or detach prevents further frame commands and guarded
  disc fills. Studio requires the runtime’s `cancellableRun` capability.
  Detach leaves the game window open. No CORS/message bridge/control server exists.
- `controller.ts` serializes commands, acknowledges host pause before deterministic
  operations and guards each awaited completion with a connection epoch. An old
  connect attempt cannot close a newer connection. Snapshots belong to the current
  connection. Callers report rejected operations once; unsolicited transport loss
  uses the controller’s asynchronous report callback. Original errors remain in
  state. Explicit run is the only action that leaves automatic pacing on.
- `schema.ts` consumes only the attached session's `schema` result, with matching
  `module_sha256` build binding and runtime identity. There is no user-selected
  schema fallback. Expansion is bounded/lazy, arrays/root lists paged. Untyped,
  opaque, bit-field and unsupported float semantics are explicit. The runtime's
  current inspection treats base values as integer storage; this UI does not
  present float storage bits as a decoded value or support float editing; those
  nodes show an explicit unsupported reason without issuing an inspection. Decimal integer bounds use BigInt before
  sending, without rounding/clamping; enums can use runtime enumerator names.
- `pc.ts` resolves schema paths from the recovered actor layout and scheduler,
  checks live mode/disc/map and exact declared component bytes, then selects an
  existing actor/event instruction. It never synthesizes graph nodes.

## PC provenance

`src/decomp/include/field/actors.h` defines `FieldActor.pc` (u16, bytecode-relative),
`slot` and eight `ScriptSlot`s with event `tag`. `field_event.c`'s
`field_event_run_all_actors` publishes the actor index/descriptor, chooses the
active slot, runs its working PC and stores the resume PC. Requested events set
`tag` to the event ID (`field_event_request_event_started/finished`); an untagged
slot does not by itself prove an event number. Studio accepts its explicit tag
only when that event reaches the PC; otherwise an untagged PC requires exactly
one actor/event owner. Aliased ownership stays unavailable rather than picking
an arbitrary event.

`field_event_package` is component 5's decompressed header;
`field_event_bytecode` follows its actor entry table (`field_event.h`, field
load). The loader indexes `mode_field_map_id & 0x0fff`; high bits include the
gear flags set by `field_event_switch_party_to_gears`. Live actor count must
match the loaded component's actual count, independently of 32 event slots.
Shared runtime status supplies the active mode and inserted drive disc; snapshot
identity intentionally groups both discs and cannot substitute for drive identity.
Only declared component bytes are compared: decompression's trailing speculative
bytes lie outside the declared boundary and are not authoritative allocation data.

## UI lifecycle

Both M4 mockup layouts were explicitly approved on 2026-10-22. `LiveGame.tsx`
composes the existing licensed widgets. F6 cycles panels, F7 steps, Shift+F6
runs/pauses; text entry and dialogs retain their own keys. Snapshot restore is
explicitly confirmed. Inspector reads are revision-scoped. Live navigation uses
M3's explicit actor/event selection API without loading a different map or
resetting name drafts. Unavailable navigation and original errors remain visible.

The native launch form passes `nw.require('node:process').env` explicitly into
the child transport. Reading bare `process.env` in bundled browser code is unsafe:
Vite replaces that expression. This preserves the owned child’s normal display,
loader and executable environment while overriding only its isolated XEM_DATA_DIR.
