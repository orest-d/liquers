# Phase 5: Documentation

**Status: executed and approved 2026-10-07.**

## Summary

Implemented 2026-10-07 as Wave 1 step 14 of
`archive/2026-10-06-p2-p3-s-implementation-order-revised.md`.

- `AsyncStoreRouter::get_metadata` (`liquers-core/src/store.rs`): when no member owns the key but
  `is_dir` answers `true`, it returns a directory record (`is_dir: true`, key set, `children` from
  `listdir_asset_info`). Otherwise `KeyNotFound`, as before.
- Deviation in detail: the record is built with `MetadataRecord::new()`, not
  `self.default_metadata(key, true)` as Phase 2 sketched, because the router's `default_metadata`
  delegates to a member and returns a blank record when none owns the key.
- Tests: `router_root_metadata_lists_members`, `router_intermediate_directory_metadata`,
  `router_unowned_key_metadata_not_found`; the router conformance suite is unchanged and passes.

## Documentation

`guides/STORE_IMPLEMENTATION_GUIDE.md` §9 router row (History row, `reviewed:`). STORE_SEMANTICS
already stated the rule.

## New issues

None.
