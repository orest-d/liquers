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

## Post-Phase-4 Review (2026-10-05)

- **Problem still valid:** yes for the **query** map. `remove_expired_from_maps` (`assets.rs`
  ≈6497) and the stale-terminal branch of `get_asset` (≈6855) both do `get_async` / compare /
  `drop` / `remove_async`. For the **key** map the race is probably already closed: that branch
  runs under `key_mutation_lock`, and key-slot insertion (`get_nonvolatile_resource_asset`
  ≈6601) takes the same lock. Phase 1 should check this and say so instead of treating both maps
  as racy. Using `remove_key_asset_if` there is still a worthwhile simplification.
- **Solution correct:** yes. `remove_if_async` is the right primitive and `ImmediateAssetManager`
  is already correct (one mutex).
- **Unnecessary:** none. One private `remove_query_asset_if` helper.
- **Detail:** adequate, though Phase 4 gives no line anchors.
- **Tests:** the sequential identity tests are enough to prove the decisions. The concurrent test
  has an escape hatch ("rely on helper atomicity"); state up front that atomicity is proven
  structurally, so nobody adds a production hook to reach the window.
- **Interactions:** none semantically. It edits `assets.rs`, as do five other designs.
- **Verdict:** ready.
