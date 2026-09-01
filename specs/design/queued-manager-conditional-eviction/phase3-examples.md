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
4. Add a deterministic Tokio concurrency regression using barriers/channels around the public
   cleanup operation and replacement; do not use timing sleeps. Assert the replacement id remains.

Use `#[tokio::test]`, existing `ownership_env`, and typed results where fallible setup uses `?`.
Run the focused asset tests and `cargo test -p liquers-core --lib`.
