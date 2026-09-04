# Phase 5: Documentation

## Implementation Summary

This shared design completed all three requested current-documentation repairs. The store
configuration reference now identifies `AsyncFileStore` as the existing filesystem implementation.
Three public rustdoc comments retain their explanatory identifiers as code prose instead of linking
to targets unavailable to public rustdoc. The build-matrix command in `CLAUDE.md` now relies on the
script's computed total rather than a stale copied count.

## Documentation Delivered

`specs/reference/STORE_CONFIG_FSD.md` was reviewed against `AsyncFileStore`, updated, and given a
same-day History row. `CLAUDE.md` and the three Rust source comments were updated as current
instruction and public API documentation. No new reference or guide was needed.

## Validation and Conformance

The strict `cargo doc -p liquers-core --no-deps` build passed with
`rustdoc::broken_intra_doc_links` and `rustdoc::private_intra_doc_links` denied. Focused searches
found none of the corrected stale text. The documentation index was regenerated and checked.

The implementation conforms to all three issues and adds no behavioural changes. `cargo fmt
--check` remains blocked by unrelated pre-existing formatting drift across the workspace; this
documentation-only change does not introduce formatting drift.

## Issues Filed

None. The Polars namespace and command-payload documentation issues were reviewed during intake
and remain independent work, not omitted portions of this design.
