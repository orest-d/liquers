# Phase 5: Documentation - Memory-Store Prefix Support

## Completion Preconditions

- [x] Implementation and tests are complete.
- [x] User and review feedback is incorporated.
- [x] Documentation matches verified behavior.

## Implementation Summary

`AsyncMemoryStore::is_supported` and `MemoryStore::is_supported` now require both an absolute key
and segment-aware membership in their configured prefix. Trait rustdoc defines support as a
cumulative decision: absolute-key validity, prefix membership, then optional store-specific
exclusions. The empty-prefix single-file overlay example explains why `is_supported` remains
meaningful even though routers also prefilter by prefix.

Six direct paired tests cover descendants, outside keys, relative keys, an empty prefix, a key
equal to the prefix, and a similar-but-distinct segment. Router and fallible-operation behavior are
unchanged.

## Documentation Delivered

### New Reference Documents

None; `specs/reference/STORE_SEMANTICS.md` remains authoritative.

### New Guide Documents

None; this is an implementor contract rather than a user workflow.

### Existing Documents Reviewed or Updated

- `specs/reference/STORE_SEMANTICS.md`: cumulative support contract, overlay rationale, tests, and
  History.
- Source issue: closed with current implementation and test evidence.
- `STORE-ASYNC-STORE-NO-BEHAVIOURAL-CONFORMANCE-SUITE`: row 8 records resolved prefix parity.

### Links and Capability Map

The temporary README entry was removed after the reference became authoritative. Generated indexes
were refreshed; the source issue and reference History retain design links.

## Issues Filed

None for this correction. The briefly proposed cross-backend issue was removed because those
stores already implement the required prefix term. The unrelated synchronous `MemoryStore::makedir`
issue remains outside this work.

## Important Learning

Router prefix filtering and a store's own support answer are intentionally redundant at the
minimum boundary. `is_supported` can then narrow that boundary for overlays or backend limitations.
This makes a router safe while allowing ordered stores to pass unsupported keys onward.

## Conformance and Remaining Work

The delivered implementation matches the clarified requirement. Both memory stores now align with
the other prefix-bearing stores, while future store-specific exclusions remain possible. Nothing
from this issue remains deferred.

## Validation

Passed after the corrected implementation:

```text
cargo test -p liquers-core --lib memsupport
cargo test -p liquers-core --lib store::tests
cargo test -p liquers-core
git diff --check
python3 scripts/docs_index.py --check
```

The workspace-wide formatting check has unrelated pre-existing failures; no broad formatting churn
was applied.
