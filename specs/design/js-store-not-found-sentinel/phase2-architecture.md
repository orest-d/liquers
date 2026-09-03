# Phase 2: Solution and Architecture

## Chosen Solution

Specify null or undefined from page get/getMetadata as KeyNotFound; thrown values remain KeyReadError.

## Integration Boundary

**Files and symbols:** liquers-web/src/store/js_store.rs, liquers-web/tests/store_js_STORE.rs, liquers-web/tests/store_conformance_CONF.rs, specs/guides/LANGUAGE_INTEGRATION_GUIDE.md. Reuse existing typed Error constructors and existing async traits; avoid new ownership or dispatch abstractions unless the named boundary requires them. Serialized additions are optional and additive; public Rust renames retain explicit compatibility handling where stated.

## Alternatives and Errors

Reject pre-checks that race or duplicate I/O, broad catch-all error mapping, and unrelated refactors. Fallible paths return existing `Result<_, Error>` types and retain typed error kinds.

## Risk Review

| Risk | Validation and recovery |
|---|---|
| Contract or compatibility drift | Pin the source acceptance cases and preserve documented wire/error behaviour. Revert the isolated change if the contract cannot be met. |
| Async or ownership regression | Keep existing AsyncStore/wasm Send bounds and borrow inputs; run focused crate tests. |
| Documentation or generated-data drift | Update named current documents and regenerate/check required indexes. |

