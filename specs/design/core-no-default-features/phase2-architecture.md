# Phase 2: Solution and Architecture - liquers-core No-Default-Features Build Decision

## Overview

Choose the honest-manifest repair: remove the unsupported `async_store` gating from
`liquers-core`'s `futures` and `async-trait` dependencies, and simplify feature definitions so the
async store surface is always part of core. This is smaller and matches the documented Liquers rule
that async is the default and sync wrappers are compatibility layers.

## Known-Issue Preflight

| Issue | Status | Priority | Relevance and action | Blocking? |
|---|---|---|---|---|
| `BUILD-MATRIX-NOT-RUN-IN-CI` | draft | P2 | Motivates adding a no-default-features row later. This design should produce a command that can be added to that matrix, but CI expansion is separate. | no |
| `CORE-TOKIO-REMOVAL` | accepted | P3 | Broader wasm/tokio cleanup is not required; this issue concerns `futures` and `async-trait` build honesty only. | no |

## Files and Symbols

Primary file: `liquers-core/Cargo.toml`, `[features]` and dependency declarations for `futures` and
`async-trait`. Source files inspected: `liquers-core/src/context.rs`, `interpreter.rs`, `recipes.rs`,
`assets.rs`, `store_factory.rs`, and `store.rs`.

Implementation review found that a manifest-only repair is insufficient: `store_factory.rs`,
`assets.rs`, `recipes.rs`, and `interpreter.rs` already require async store symbols outside any
coherent feature boundary, while `context.rs` and `store.rs` still gate the definitions. The chosen
repair therefore removes `async_store` as a source-code cfg boundary in `liquers-core`, keeps
native-only async file store pieces gated only by `not(target_arch = "wasm32")`, and leaves
`async_store` as a no-op compatibility feature for existing Cargo selectors.

## Data, Ownership, Serialization and Errors

No runtime data structures, serialization format or `Error` path changes. Cargo dependency
metadata is the only configured state.

## Sync, Async and API Effects

`AsyncStore` remains available in all `liquers-core` builds. This removes the implied public API
variant where async store types disappear under `--no-default-features`; code inspection suggests
that variant already did not compile, so compatibility loss is theoretical rather than practical.

## Alternatives

Rejected: `#[cfg(feature = "async_store")]` around every async import, trait and caller. That is
larger than `S`, crosses central modules, and risks leaving a hollow core build without coherent
evaluation/store APIs. Rejected: removing the no-default-features use case from validation only;
that would leave the manifest false.

## Risk Assessment

| Assessment | Record |
|---|---|
| Files | 5 source/config files changed (`liquers-core/Cargo.toml`, `src/context.rs`, `src/store.rs`, `src/interpreter.rs`, `src/recipes.rs`) plus one narrow exposed compiler fix in `src/assets.rs`, the build matrix script, design/issue records, and specs/index. |
| Impact area | Build feature contract for `liquers-core`; downstream crates see fewer feature combinations. |
| Module/crate reach | One crate plus build script/docs records; source changes are the existing core async-store API boundary. |
| Existing-test breakage | None expected; tests run with defaults already have these dependencies. |
| New validation | `cargo check -p liquers-core --no-default-features`; `cargo check -p liquers-core`; `cargo test -p liquers-core --no-default-features`; `cargo test -p liquers-core`; `bash scripts/check-build-matrix.sh`. |
| Behavioural risk | No runtime behaviour; compatibility risk is removal of an advertised but broken feature shape. |
| Recovery | Revert Cargo feature/dependency edits; no data migration. |
| Certainty | High on failing imports and smaller solution; medium on downstream expectations outside workspace. |

## Rust Review

The design avoids broad `cfg` churn and keeps async APIs compiled consistently. No ownership,
error-handling, trait-bound or serialization changes are introduced.
