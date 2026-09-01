# Phase 4: Implementation Plan

1. In `liquers-core/src/assets.rs`, add a private conditional query removal mirroring
   `remove_key_asset_if`; implement both over `remove_if_async`, retaining `key_mutation_lock` on
   key mutation. Add sequential identity tests.
2. Rewrite `DefaultAssetManager::remove_expired_from_maps` to call the conditional helpers without
   a compare/drop/unconditional-remove gap. Preserve query-first and key-fallback return semantics.
3. Replace other open-coded stale-terminal removals in `get_asset`/`get` with the same helpers and
   add matching/replacement regressions for every changed path.
4. Add the deterministic coordinated race test from Phase 3. If no test-only coordination can
   reach the gap without production hooks, rely on helper atomicity plus sequential tests and
   document that structural proof explicitly.
5. Review `ASSETS.md` and `ASSET_LIFECYCLE.md` for eviction-concurrency claims; update lifecycle
   records and generated docs index only as needed.
6. Run formatting, focused asset tests, full core tests, core clippy, and docs-index checks. Review
   for lock-order changes, async guards held across awaits, unconditional map removal, debug output,
   and unrelated asset refactors. Rollback is confined to helper call sites and tests.
