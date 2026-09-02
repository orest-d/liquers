# Phase 2: Solution & Architecture - Memory-Store Prefix Support

## Overview

Both memory stores change their existing synchronous predicate to:

```rust
!key.is_relative() && key.has_key_prefix(&self.prefix)
```

This establishes the minimum support boundary. A store may append narrower exclusions. Routers
retain their independent prefix prefilter because routing must remain safe even for custom stores.

## Known-Issue Preflight

| Issue | Status | Priority | Impact | Blocking? | Action |
|---|---|---|---|---|---|
| `CORE-ASYNC-MEMORY-STORE-IS-SUPPORTED-IGNORES-PREFIX` | draft | P1 | Target defect in async and sync memory stores. | No | Fix both predicates and test directly. |
| `STORE-ASYNC-STORE-NO-BEHAVIOURAL-CONFORMANCE-SUITE` | accepted | P1 | Explains why the divergence was not caught. | No | Add local evidence; update its row 8. |
| `STORE-ABSOLUTE-KEY-NOT-TYPE-ENFORCED` | draft | P3 | Runtime absolute-key checks remain necessary. | No | Preserve current guards. |

No prerequisite blocks this S-sized correction.

## Data Structures

None. Both stores reuse their owned `prefix: Key` and borrow the candidate key.

## Trait Implementations

Modify `AsyncStore for AsyncMemoryStore` and `Store for MemoryStore` without changing signatures.
Update both trait method docs to define the cumulative contract:

1. reject relative keys;
2. reject keys outside `key_prefix()`;
3. apply optional store-specific exclusions.

An empty prefix matches all absolute keys but does not force universal support. A single-file
overlay can use an empty prefix and return true only for one key, allowing subsequent stores in the
router to receive all other keys.

## Generic Parameters & Bounds

None.

## Sync vs Async Decisions

Both predicates stay synchronous and allocation-free. `AsyncStore::is_supported` intentionally
does not require an async runtime.

## Function Signatures

No signature changes: `fn is_supported(&self, key: &Key) -> bool`.
`Key::has_key_prefix` provides segment-aware comparison, including equal and empty prefixes, while
distinguishing `data` from `database`.

## Integration Points

- `liquers-core/src/store.rs`: update both predicates, trait rustdoc, stale comments, and tests.
- Store routers: unchanged; their prefix check and support check remain separate safeguards.
- No command, endpoint, dependency, feature, serialization, binding, or public API changes.

No current overlay/fallback implementation exists; the single-file overlay is the intended
composition use case that explains why direct support reporting must be truthful.

## Documentation Architecture

Update `specs/reference/STORE_SEMANTICS.md` section 6 with the cumulative support definition and
overlay example. Close the source issue with actual tests and current symbols. Update row 8 of
`STORE-ASYNC-STORE-NO-BEHAVIOURAL-CONFORMANCE-SUITE` to record the resolved parity. Remove the
temporary README capability entry after the reference is authoritative. No guide is needed.

Authoritative `affects_docs`: the reference, source issue, and conformance-suite issue.

## Relevant Commands

None. No Liquers command, query, or namespace participates.

## Web Endpoints (if applicable)

None.

## Error Handling

`is_supported` remains infallible. Fallible methods independently return `KeyNotAbsolute` for
relative keys; this predicate does not replace enforcement.

## Serialization Strategy

Not applicable.

## Concurrency Considerations

Only immutable keys are borrowed; no map or lock is touched.

## Compilation Validation

Focused inline tests, the store test module, and the full `liquers-core` suite are sufficient.

## References to liquers-patterns.md

The change reuses existing borrowed helpers and introduces no clone, allocation, generic, error,
or dependency.
