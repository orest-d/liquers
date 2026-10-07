# Phase 5: Documentation - Conditional Queued-Manager Cache Eviction

**Status: executed 2026-10-07**, after implementation (Wave 1 step 5 of
`archive/2026-10-06-p2-p3-s-implementation-order-revised.md`). Awaiting approval.

## Completion Preconditions

- [x] Implementation is finished and validated (Phase 4)
- [x] All user comments are answered or incorporated
- [x] All review comments are answered or incorporated (none yet)
- [x] Documentation is consistent with the implemented and tested behavior
- [x] Documentation is included in the implementation PR

## Documentation Plan

### New Reference Documents

None: the change extends behaviour that existing documents own.

### New Guide Documents

None.

### Existing Documents to Update (`affects_docs`)

| Document | Planned change |
|---|---|
| `guides/ASSET_MANAGER_IMPLEMENTATION_GUIDE.md` | the `remove_expired_from_maps` row (≈143, "Drop the entry only if it is still the asset with that id"): add *atomically* — compare and remove in one map operation (`remove_if_async`, or one mutex guard) — since a manager outside core would otherwise copy the racy compare-then-remove pattern |

Each updated `reference/` or `guides/` document gets a `## History` row (source `phase-5`) and a
`reviewed:` bump in the same commit.

### Candidates Considered and Discarded

By area (`core/assets`): `ASSETS.md` (≈59 `query_assets`, ≈191 `remove_expired_from_maps`) and `ASSET_LIFECYCLE.md` describe *when* entries are evicted, which is unchanged; `ASSET_SET_OPERATION.md`, `DEPENDENCIES_STATUS.md`, `DOC_03_ASSETS_EXECUTION_LIFECYCLE.md` do not describe eviction mechanics.

### Links and Capability Map

None needed in `specs/README.md`: the capability is reached through the documents above.

### Issues to Close

`QUEUED-MANAGER-EVICTION-RACE` → `closed`, resolution naming the tests and the structural proof (no unconditional stale removal remains).

## Implementation Summary

`DefaultAssetManager` gained a private `remove_query_asset_if` (one `remove_if_async` call). The
query and key branches of `remove_expired_from_maps` and the stale-terminal branches of
`get_asset` (query map) and `get` (key map) now call `remove_query_asset_if` /
`remove_key_asset_if` instead of `get_async` / compare / `drop` / `remove_async`. The key paths keep
`key_mutation_lock`. Tests: `remove_query_asset_if_respects_id` and
`remove_expired_from_maps_respects_replacements` (stale id, query first, key fallback, ad-hoc).

Structural proof: the only `assets.remove_async` calls left in `assets.rs` are the replacement in
`set` / `set_state` (under `key_mutation_lock`) and `remove_key_asset` (explicit removal). No
`query_assets.remove_async` remains.

## Documentation Delivered

`guides/ASSET_MANAGER_IMPLEMENTATION_GUIDE.md`: the `remove_expired_from_maps` row says the
compare and the removal are one atomic map operation; History row added and `reviewed:` bumped.

## Issues Filed

None.

## Important Learning

the key map was already serialized by `key_mutation_lock`; only the query map was racy.

## Conformance and Remaining Work

Conforms to Phases 1–4. No remaining work.

## Validation

`cargo test -p liquers-core --lib --tests`; `python3 scripts/docs_index.py --check`.
