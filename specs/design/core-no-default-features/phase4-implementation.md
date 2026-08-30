# Phase 4: Implementation - liquers-core No-Default-Features Build Decision

## Plan

1. Update `liquers-core/Cargo.toml` so `futures` and `async-trait` are normal dependencies and
   `async_store` / `tokio_exec` are no-op compatibility features.
2. Remove `#[cfg(feature = "async_store")]` from core source and replace
   `#[cfg(all(feature = "async_store", not(target_arch = "wasm32")))]` with
   `#[cfg(not(target_arch = "wasm32"))]` for native file-store code.
3. Fix the exposed trait declaration in `assets.rs` by removing `mut` from a body-less parameter.
4. Add `liquers-core --no-default-features` to `scripts/check-build-matrix.sh`.
5. Run formatting, focused checks, core tests, the no-default core tests, and the build matrix.

Rollback is a direct revert of these files; no runtime data or persisted format is touched.

## Implementation Summary

The async store API is now unconditional in `liquers-core`. `AsyncFileStore` remains native-only
because it uses `tokio::fs`; in-memory async store, routers, environment accessors, recipes and
interpreter access all compile with or without default features. `async_store` remains in the
feature table so downstream crates that already select it do not fail Cargo feature resolution.

## Validation Record

| Check | Result |
|---|---|
| `cargo fmt -p liquers-core` | passed |
| `cargo check -p liquers-core --no-default-features` | passed |
| `cargo check -p liquers-core` | passed |
| `cargo test -p liquers-core` | passed: 724 unit tests, integration tests, and doctests |
| `cargo test -p liquers-core --no-default-features` | passed: 724 unit tests, integration tests, and doctests |

The build matrix check is recorded in Phase 5 after documentation/index maintenance.
