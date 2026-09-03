# Phase 2: Solution and Architecture

## Chosen Solution

Make an absent member prefix enumerate as an empty namespace at the AsyncFileStore/listdir boundary, retaining other errors.

## Integration Boundary

**Files and symbols:** liquers-core/src/store.rs, liquers-core/tests/store_conformance_CONF.rs. Reuse existing typed Error constructors and existing async traits; avoid new ownership or dispatch abstractions unless the named boundary requires them. Serialized additions are optional and additive; public Rust renames retain explicit compatibility handling where stated.

## Alternatives and Errors

Reject pre-checks that race or duplicate I/O, broad catch-all error mapping, and unrelated refactors. Fallible paths return existing `Result<_, Error>` types and retain typed error kinds.

## Risk Review

| Risk | Validation and recovery |
|---|---|
| Contract or compatibility drift | Pin the source acceptance cases and preserve documented wire/error behaviour. Revert the isolated change if the contract cannot be met. |
| Async or ownership regression | Keep existing AsyncStore/wasm Send bounds and borrow inputs; run focused crate tests. |
| Documentation or generated-data drift | Update named current documents and regenerate/check required indexes. |

