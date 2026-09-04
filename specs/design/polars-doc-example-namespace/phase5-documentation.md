# Phase 5: Documentation

## Implementation Summary

Every complete resource pipeline in `POLARS_COMMAND_LIBRARY.md` now selects the `pl` namespace
with `ns-pl`. The query-format grammar explains the rule and the reference preserves the exception
for callers that already selected that namespace. Command fragments and historical implementation
sketches were deliberately left unchanged.

## Documentation Delivered

`reference/POLARS_COMMAND_LIBRARY.md` is the authoritative affected document. It was reviewed
against the committed command registry, updated, and given matching `reviewed:` and History dates.
No new guide or reference was needed.

## Validation and Conformance

The ten complete resource pipelines reported by the issue all failed without `ns-pl` and all
validated successfully with it using `liquers-validate` and the committed registry. A scoped search
found no complete resource pipeline without the namespace instruction. The source issue is closed.

`cargo fmt --check` still reports unrelated workspace-wide formatting drift; this change contains
no Rust source or formatting changes.

## Issues Filed

None. The existing Polars command-test and registry-freshness issues were rechecked and remain
independent from this documentation correction.
