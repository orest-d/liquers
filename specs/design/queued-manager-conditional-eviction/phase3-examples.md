# Phase 3: Examples and Tests

## Examples

If stale asset A expires after query/key slot replacement by B, cleanup for A returns false and B
remains reachable. Cleanup for B returns true and removes B. Query and key slots behave identically;
an ad-hoc asset with neither mapping returns false.

## Tests

1. Extend the existing `remove_key_asset_if_respects_id` fixture for queued-manager key identity.
2. Add the equivalent query-map test using two `AssetRef` ids and the private conditional helper.
3. Test `remove_expired_from_maps` for matching, stale replacement, query-first precedence, key
   fallback, and neither-map cases.
4. **No concurrent test is required.** Atomicity is proven structurally: after the change, every
   stale eviction is a single `remove_if_async` call, whose predicate and removal run under one
   bucket lock, so no interleaving exists to test. The sequential tests 1-3 prove each call site
   decides by id. Do **not** add a production hook to reach the former window. A reviewer checks
   the structural claim by searching `assets.rs` for `query_assets.remove_async` and
   `assets.remove_async`: only unconditional removals (`remove_key_asset`, explicit removal paths)
   may remain.

Use `#[tokio::test]`, existing `ownership_env`, and typed results where fallible setup uses `?`.
Run the focused asset tests and `cargo test -p liquers-core --lib`.
