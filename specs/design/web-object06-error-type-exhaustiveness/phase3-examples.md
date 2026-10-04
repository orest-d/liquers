# Phase 3: Examples and Tests - Compiler-Checked `ErrorType` List for OBJECT06

## Tests Changed

| Test | Before | After |
|---|---|---|
| `object06_every_enum_variant_roundtrips` | fails: `left: 23, right: 22`; never checks `KeyNotAbsolute` | passes; round-trips 24 names incl. `key_not_absolute`; names distinct |
| `error01_every_error_type_maps` | 23 cases | 24 cases |

## Compile-Time Guarantee (manual check, not committed)

1. Temporarily delete `KeyNotAbsolute` from the `error_types!` invocation.
2. `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles --test objects_OBJECT --no-run`
   must fail with `non-exhaustive patterns: ErrorType::KeyNotAbsolute not covered`.
3. Restore it.

## Run

```bash
cargo clean   # per CLAUDE.md, the wasm loop runs separately from the native one
cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles --test objects_OBJECT
```

Requires `rustup target add wasm32-unknown-unknown` and the Node runner configured for
`wasm-bindgen-test` (see `liquers-web/README.md`).

## Coverage Review

Criteria 1, 3, 4 by the run; 2 by the manual compile check.
