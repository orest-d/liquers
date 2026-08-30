# Phase 5: Documentation - liquers-core No-Default-Features Build Decision

## Current State

`liquers-core` supports `--no-default-features`. The async store API is part of the core crate in
all feature sets; the `async_store` feature is retained only as a no-op compatibility selector for
existing Cargo configurations.

Native-only async file store code remains gated on `not(target_arch = "wasm32")`, because it uses
`tokio::fs`. The in-memory async store, store routers, environment async-store accessors, recipe
resolution, and interpreter paths are available with no default features selected.

## Documentation Maintenance

Updated `scripts/check-build-matrix.sh` to add `liquers-core --no-default-features` and remove the
stale comment saying that row could not be added. Updated the design tracker and issue status, and
regenerated `specs/index.csv`.

No new reference guide is needed. Existing store configuration reference text describes
`liquers-store`'s separate `async_store` feature and remains true.

Filed [`LIB-POLARS-ETHNUM-RUST-1-98-BROKEN`](../../issues/LIB-POLARS-ETHNUM-RUST-1-98-BROKEN.md)
as a separate draft issue for the unrelated Polars matrix failure found during broad validation.

## Final Validation

| Check | Result |
|---|---|
| `cargo check -p liquers-core --no-default-features` | passed |
| `cargo check -p liquers-core` | passed |
| `cargo test -p liquers-core --no-default-features` | passed |
| `cargo test -p liquers-core` | passed |
| `bash scripts/check-build-matrix.sh` | failed only on unrelated `liquers-lib` Polars rows; new `liquers-core --no-default-features` row passed |
| `cargo check -p liquers-core --features toml` | passed after rerun with Cargo registry write access |
| `python3 scripts/docs_index.py --check` | passed with pre-existing warnings |
