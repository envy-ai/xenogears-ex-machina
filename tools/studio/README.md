# XEM Studio

Milestone one is in progress. The shell mockup is in the local, ignored
`tmp/studio/mockup/shell-v1.png`; UI implementation awaits Bart's approval.

## Backend

Run commands from the repository root with the pinned tools:

```sh
nix --extra-experimental-features 'nix-command flakes' develop path:./nix/runtime
python3 tools/studio/build-backend.py
# Optional optimized build:
python3 tools/studio/build-backend.py --release
systemd-run --user --scope -p MemoryMax=16G -p MemorySwapMax=0 \
  cargo test --locked --manifest-path src/runtime/Cargo.toml -p xem-studio
```

The build emits web-target wasm-bindgen JavaScript, TypeScript declarations and
WebAssembly into ignored `tools/studio/src/backend/pkg/`. It honors
`CARGO_TARGET_DIR` relative to the caller's working directory. Native Rust
callers use the same `xem_studio::Studio::new().command(json)` service.

The JSON command envelope is `{ "op": "name", "args": value }`:

- `{"op":"version"}` (args absent or null) returns
  `{"version":"0.5.0","operations":["version","echo"]}`. The version follows
  the runtime workspace package version.
- `{"op":"echo","args":value}` returns `value` unchanged. Any JSON value,
  including null, is accepted; the args field is required.
- Failures return `{"error":{"kind":"...","message":"..."}}`. Kinds are
  `malformed_json` for parse errors, `invalid_command` for a missing/non-string
  op or a non-object envelope, `unknown_operation` for an unrecognized op, and
  `invalid_args` for a known operation with invalid arguments or extra fields.
  Messages preserve parser/deserializer diagnostics. Invalid calls do not
  prevent subsequent commands.
