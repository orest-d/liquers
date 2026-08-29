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
`async-trait`. Source files inspected: `liquers-core/src/context.rs`, `interpreter.rs`, and
`store.rs`; no code edits are expected if the dependencies become unconditional.

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
| Files | 1 config file likely changes (`liquers-core/Cargo.toml`), plus specs/index. |
| Impact area | Build feature contract for `liquers-core`; downstream crates see fewer feature combinations. |
| Module/crate reach | Manifest-only in one crate; workspace validation may touch dependency resolution. |
| Existing-test breakage | None expected; tests run with defaults already have these dependencies. |
| New validation | `cargo check -p liquers-core --no-default-features`; default `cargo check -p liquers-core`. |
| Behavioural risk | No runtime behaviour; compatibility risk is removal of an advertised but broken feature shape. |
| Recovery | Revert Cargo feature/dependency edits; no data migration. |
| Certainty | High on failing imports and smaller solution; medium on downstream expectations outside workspace. |

## Rust Review

The design avoids broad `cfg` churn and keeps async APIs compiled consistently. No ownership,
error-handling, trait-bound or serialization changes are introduced.
