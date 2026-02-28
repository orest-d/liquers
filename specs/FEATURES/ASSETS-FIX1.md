# ASSETS-FIX1

Status: Draft

## Summary
`ASSETS-FIX1` consolidates all `TODO`, `FIXME`, and `todo!()` markers in `liquers-core/src/assets.rs` into a concrete implementation backlog.  
Focus: remove known runtime gaps (dependency handling, delegation deadlock risk, metadata consistency), reduce duplication, and finalize incomplete API paths.

## Don't fix
| # | Fix? | Location | Marker | Proposed solution |
|---|---|---|---|---|
| 1 | No | `assets.rs:98` | `TODO: remove argument?` for `StatusChanged(Status)` | Keep argument for now and document notification payload contract; remove only after all consumers fetch status via `get_metadata()`/`status()` without relying on message payload. |
| 4 | No | `assets.rs:401` | `TODO: support for quick plans` | Extend `try_fast_track()` to support pure-query quick plan evaluation for known no-dependency recipes. |
| 7 | No | `assets.rs:704` | `TODO: Make it to return non-async shared Arc string` | Cache `asset_reference` in `AssetData` as precomputed `Arc<str>` updated on recipe/status changes; expose sync getter on `AssetData`. |
| 18 | No | `assets.rs:1298` | `TODO: reference to asset` in `Context::new(...)` | Clarify and codify context contract: context carries weak/explicit asset reference required for progress/log routing. |
| 19 | No | `assets.rs:1299` | `TODO: Separate evaluation of dependencies` | Split dependency resolution from recipe execution into explicit pre-pass/state (`Dependencies`) and resume execution on ready dependencies. |
| 23 | No | `assets.rs:2380` | `TODO` fast-track for `apply()` | Add optional in-memory fast path for simple apply recipes where result can be produced synchronously without queue wait. |
| 24 | No | `assets.rs:2399` | `TODO` fast-track for `apply_immediately()` | Same as #23; likely naturally satisfied once shared execution helper exists. |
| 25 | No | `assets.rs:2449` | `TODO` create should construct new settable asset | Implement `create(key)` as explicit new asset creation with empty/default state and no implicit `get()` side effects. |

## Inventory (assets.rs)

| # | Fix? | Location | Marker | Proposed solution |
|---|---|---|---|---|
| 2 | Phase3 | `assets.rs:134` | `TODO: Make a proper save immediately task` | Introduce `MetadataSaver::save_immediately(...)` with coalescing and one in-flight task guard (`JoinHandle` or single worker loop). |
| 3 | Phase3 | `assets.rs:334` | `TODO: prevent too frequent saving` | Route through `MetadataSaver` throttling instead of direct write in `save_metadata_to_store()`. |
| 16 | Phase4 | `assets.rs:1278` | log string contains `FIXME` | Replace with structured debug log without FIXME marker. |
| 17 | Phase4 | `assets.rs:1282` | `FIXME` delegation can deadlock if not queued | Replace blocking delegation (`asset.get().await`) with dependency scheduling + non-blocking parent wait state; ensure delegated asset submitted before parent waits. |
| 20 | Yes | `assets.rs:1955` | `TODO` apply input should be `State` | Change `AssetManager::apply` signature to accept `State<E::Value>`; preserve legacy wrapper that builds state from value. |
| 21 | Yes | `assets.rs:1959` | `TODO` apply_immediately input should be `State` | Same as #20 for `apply_immediately`. |
| 22 | Yes | `assets.rs:2156` | `TODO` expiration monitor should call `expire()` | Wire expiration monitor to asset manager lookup and invoke `asset.expire().await` (with missing-asset tolerance). |

## Implementation Plan

1. Phase 1: Remove runtime blockers.
   1. Implement both `Status::Dependencies` branches.
   2. Fix delegation deadlock path.
   3. Wire expiration monitor to `expire()`.
2. Phase 2: Metadata and lifecycle consistency.
   1. Add metadata helpers: `set_volatile`, `set_expiration_time`, `set_expiration_time_from`.
   2. Resolve volatility once at start; reuse in finalize.
   3. Improve fast-track corruption handling.
3. Phase 3: Refactor and API cleanup.
   1. Extract shared `run`/`run_immediately` helper.
   2. Add save-throttling integration.
   3. Migrate `apply`/`apply_immediately` to `State<E::Value>` input.
4. Phase 4: Nice-to-have improvements.
   1. `asset_reference` sync/cache improvement.
   2. Quick-plan/fast-track for non-resource apply paths.
   3. `create(key)` dedicated constructor semantics.

## Priority

1. Critical: #13, #15, #17, #22.
2. High: #8, #9, #10, #11, #14, #19.
3. Medium: #2, #3, #5, #20, #21, #25.
4. Low: #1, #4, #7, #12, #16, #23, #24.

## Acceptance Criteria

1. No `todo!()` remains in `assets.rs` for `Status::Dependencies`.
2. Delegation path cannot deadlock under queue capacity constraints (covered by tests).
3. Expiration monitor actively expires tracked assets.
4. Volatility and expiration metadata updates are done through metadata API helpers.
5. `run` and `run_immediately` share finalization logic via one helper.
6. `apply` APIs support `State<E::Value>` directly (with compatibility shim for value-only callers).
