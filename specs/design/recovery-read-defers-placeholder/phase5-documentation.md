# Phase 5: Documentation

**Status: executed and approved 2026-10-07.**

## Summary

Implemented 2026-10-07 as Wave 1 step 11 of
`archive/2026-10-06-p2-p3-s-implementation-order-revised.md`.

- `get_any_status` and `get_binary_any_status` answer from the live asset only when
  `!live_status_defers_to_store(status)`; a `None`/`Recipe` placeholder falls through to the store
  path, as `remove` does. Both are `DefaultAssetManager` trait-impl methods in
  `liquers-core/src/assets.rs` (Phase 2 called them trait defaults); the immediate manager reaches
  the same code.
- Tests (deterministic, a placeholder mapped but never run, on both managers):
  `recovery_reads_defer_placeholder_to_store_default`,
  `recovery_reads_defer_placeholder_to_store_immediate` — the stored value with a store entry,
  `Ok(None)` without one.

## Conformance and deviations

As designed. The store path's handling of a metadata-only entry (`Ok(None)` rather than an error)
was added by `memory-store-metadata-only-entry` the same day.

## Documentation

`reference/ASSETS.md`: a paragraph after the read-exposure table on the manager-level recovery
reads.

## New issues

None.
