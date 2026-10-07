# Phase 5: Documentation

## Summary

Implemented 2026-10-07 as Wave 1 step 4 of
`archive/2026-10-06-p2-p3-s-implementation-order-revised.md`.

- `AsyncMemoryStore` keeps `Option<Arc<[u8]>>` data. `set_metadata` on a new key stores `None`, and
  `get` / `get_bytes` on it report `KeyNotFound`. `set(k, b"", m)` stores an empty data object.
- New conformance rule `sidecar05` ("a key holding only metadata has no data object"), with three
  refutation tests (`refute_sidecar05_*`). Every in-tree suite passes it: memory, file, router,
  trait defaults, OpenDAL memory and fs. The `liquers-web` `JsStore` stub now uses `data: null`
  for a metadata-only entry and implements `getMetadata`.
- `no_bytes_by_design` is removed from `assets.rs`. Part G skips only a `KeyNotFound` read; empty
  bytes are always checked.
- Tests: T1–T3 in `store.rs`, T5/T6 in `tests/external_change_integration.rs`, T7
  `metadata_only_entry_on_memory_store_is_recomputed` in `tests/metadata_only_entry_reload.rs`.

## Conformance and deviations

As designed, plus one addition found by the tests: the store branches of the recovery reads
`get_any_status` / `get_binary_any_status` now answer `Ok(None)` for a contained key whose `get`
reports `KeyNotFound`, instead of failing. Before, the memory store served empty bytes there and
the file store failed the read. The test-only `DefaultsStore` in `tests/store_conformance_CONF.rs` also gained
`Option` data, because it failed `sidecar05` the same way the memory store did.

## Documentation

- `reference/STORE_SEMANTICS.md` §8: the normative sentence and `sidecar05`.
- `guides/STORE_IMPLEMENTATION_GUIDE.md` §8 table and §9 counts (44 rules).

## New issues

None.
