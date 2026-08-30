# Phase 3: Examples and Tests - liquers-core No-Default-Features Build Decision

## Reproduction

Before the fix, `cargo check -p liquers-core --no-default-features` failed with missing
`futures`, `async_trait`, and async store symbols. The same command is the primary regression test:
it must compile the library with no default features selected.

## Corrected Behaviour

`liquers-core` exposes the async store API in every feature set. The `async_store` feature remains
accepted by Cargo for compatibility, but selecting or omitting it no longer changes core public
symbols or the `futures` / `async-trait` dependency edge.

## Validation Plan

| Check | Purpose |
|---|---|
| `cargo check -p liquers-core --no-default-features` | Reproduces and proves the fixed build configuration. |
| `cargo test -p liquers-core --no-default-features` | Runs the same crate tests under the repaired feature set. |
| `cargo check -p liquers-core` | Confirms the default consumer configuration still compiles. |
| `cargo test -p liquers-core` | Confirms existing behaviour is unchanged. |
| `bash scripts/check-build-matrix.sh` | Confirms the new core no-default row and existing workspace matrix remain valid. |

No new unit test is useful for this defect: the externally meaningful behaviour is Cargo feature
resolution and compilation of the public core API.
