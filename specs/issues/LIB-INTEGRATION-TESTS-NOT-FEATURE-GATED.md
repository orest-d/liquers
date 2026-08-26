---
id: LIB-INTEGRATION-TESTS-NOT-FEATURE-GATED
kind: issue
title: liquers-lib integration tests do not compile without default features
status: closed
priority: P2
complexity: S
area: [lib/value, build]
design:
created: 2026-08-18
github:
---
## Problem

`liquers-lib`'s integration tests use optional dependencies unconditionally, so the crate's test
targets only compile with default features on:

- `tests/polars_commands.rs` and `tests/polars_value_serde.rs` — `use polars::prelude::*` with no
  `#[cfg(feature = "polars")]`;
- `tests/ui_shortcuts_integration.rs` and the other egui-facing suites — `egui::Key::S` and
  `KeyboardShortcut::to_egui` behind no gate.

```
$ cargo test -p liquers-lib --no-default-features --lib --tests
error[E0433]: failed to resolve: use of unresolved module or unlinked crate `polars`
$ cargo test -p liquers-lib --no-default-features --features polars --lib --tests
error[E0433]: failed to resolve: use of unresolved module or unlinked crate `egui`
```

The **library** is correctly gated and passes in every configuration — `--no-default-features
--lib` runs 215 tests green. Only the test targets are affected.

## Impact

The feature matrix cannot be exercised, which is exactly where `#[cfg]` mistakes hide: a cfg'd-out
enum variant leaving a `match` non-exhaustive compiles fine with default features and fails only in
the configuration nobody can run. A wasm or minimal build therefore has no test coverage at all,
and a regression in it is invisible until someone builds it by hand.

## Expected behaviour

Each integration test file that touches an optional dependency is gated — `#![cfg(feature = "…")]`
at the top of the file is the least intrusive form — so that `--no-default-features` and each
single-feature configuration compile and run whatever subset applies.

Worth doing alongside: a CI job, or a documented command list in `CLAUDE.md` beside the existing
build guidance, that runs the matrix rather than leaving it to be remembered.

## Discovery

Found on 2026-08-18 during `value-type-system` step 10, which called for running the feature
matrix. Verified pre-existing: at commit `dc762b3`, before any implementation in that project,
`tests/polars_commands.rs` contained zero `cfg(feature = "polars")` guards.

## Resolution

Fixed 2026-08-25 on `claude/lib-integration-tests-feature-gated-3m20ni`.

Three test targets failed to compile without the default features, and three tests inside
`tests/registry_export.rs` compiled but could not pass in a reduced build. Both are now gated:

| File | Gate | Why |
|---|---|---|
| `tests/polars_commands.rs` | `#![cfg(feature = "polars")]` | every test builds a DataFrame |
| `tests/polars_value_serde.rs` | `#![cfg(feature = "polars")]` | same |
| `tests/ui_shortcuts_integration.rs` | `#[cfg(feature = "egui")]` on `integration_parse_and_convert_to_egui` | only that one test touches `egui`; the other six run everywhere |
| `tests/registry_export.rs` — `committed_registry_is_fresh` | `#[cfg(all(egui, image-support, polars))]` | compares against `specs/command_registry.yaml`, which is exported with the default features; a reduced build would report the absent groups as staleness |
| `tests/registry_export.rs` — `variadic_argument_round_trips_through_the_registry` | `#[cfg(feature = "polars")]` | `pl/select_columns` is the only registered variadic command |
| `tests/registry_export.rs` — `exported_registry_is_nonempty` | count floor lowered to the `core` + `lui` set, each optional group guarded by an anchor command instead | the `>= 20` floor assumed the default build; a per-group anchor keeps the guard without a count that has to be revised whenever a command is added |

`scripts/check-build-matrix.sh` already existed but checked libraries only. Its native
`liquers-lib` rows now pass `--tests`, and an `image-support` row was added, so a future ungated
`use` fails the matrix rather than lying dormant. The wasm32 row stays library-only: liquers-lib's
dev-dependencies (liquers-store with OpenDAL `services-fs`) are native and there is no wasm test
runner in that loop. `CLAUDE.md` gained a "Feature matrix" section under *Building and testing*
recording the script and the per-configuration test commands.

Verified: `bash scripts/check-build-matrix.sh` — 11/11 OK. `cargo test -p liquers-lib --lib
--tests` in six configurations — default 378, `--no-default-features` 273, `+polars` 311,
`+egui` 275, `+webui` 281, `+image-support` 337; 0 failures in every one.

No CI job runs the matrix — see `BUILD-MATRIX-NOT-RUN-IN-CI`.
