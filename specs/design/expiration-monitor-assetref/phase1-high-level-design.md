# Phase 1: High-Level Design — Expiration Monitor AssetRef Tracking

**Status:** Draft
**Feature name:** `expiration-monitor-assetref`
**Date:** 2026-02-28

---

## Feature Name

**expiration-monitor-assetref** — Centralized expiration monitor tracking by `AssetRef` instead of `Key`.

## Crate Placement

All changes are in `liquers-core/src/assets.rs`. No new crates or modules required.

---

## Purpose

Replace key-based tracking in `run_expiration_monitor` with `AssetRef`-based tracking so the monitor can:
1. Call `asset_ref.expire().await` directly at expiration time (resolving the TODO at line 2178).
2. Remove the expired `AssetRef` from `DefaultAssetManager::assets` or `::query_assets` (by matching `asset_id`) — **stopping tracking, not deleting from store**.
3. Handle ad-hoc assets (from `create_asset()`) that are never in the key/query maps.

**Expiration ≠ removal.** Expiration transitions an asset's in-memory status to `Expired` and removes it from the asset manager's maps so future requests get a fresh evaluation. It does not touch the backing store.

---

## Current State (Two Competing Paths)

| Path | Mechanism | Status |
|---|---|---|
| `AssetRef::schedule_expiration()` | Spawns one `tokio::spawn` per asset using `Arc::downgrade` | Working but unmanaged |
| `DefaultAssetManager::run_expiration_monitor()` | Central priority queue by Key | Stub — only logs, never fires `expire()` |

This feature unifies both into the centralized monitor, removing the per-asset spawned task.

---

## What Changes

| Component | Current | Target |
|---|---|---|
| `ExpirationMonitorMessage` | `Track { key, expiration_time }` | `Track { asset_ref, expiration_time }` |
| `ExpirationMonitorMessage` | `Untrack { key }` | `Untrack { asset_id: u64 }` |
| `ExpirationMonitorMessage` | non-generic enum | generic `ExpirationMonitorMessage<E>` |
| `run_expiration_monitor` | non-generic, fires but only logs | generic over `E`, calls `expire()` + map cleanup via envref |
| Heap element type | `(DateTime, Key)` | newtype `TimedAsset<E>` (ordered by DateTime, ties by asset_id) |
| Cancelled set | `HashSet<Key>` | `HashSet<u64>` (by asset_id) |
| `DefaultAssetManager::track_expiration` | takes `&Key` | takes `&AssetRef<E>` |
| `DefaultAssetManager::untrack_expiration` | takes `&Key` | takes `asset_id: u64` |
| `AssetRef::schedule_expiration` | spawns per-asset tokio task | calls `envref.get_asset_manager().track_expiration(self, ...)` |

---

## Core Interactions

- **`AssetRef` / `AssetData`**: `AssetRef<E>` clones hold `Arc<RwLock<AssetData<E>>>`. The monitor holds strong clones — appropriate because expired assets may still be cached by callers with live refs; monitors calling `expire()` on them correctly signals all waiters.
- **Map cleanup via envref**: When an expiration fires, the monitor reads `asset_ref.data.read().await.envref` to reach `get_asset_manager()`, then removes the expired entry from `self.assets` or `self.query_assets` **only if the stored entry's `asset_id` matches** (guarding against a newer replacement having already taken that slot).
- **Key/Query discovery**: `AssetData` exposes `recipe.key()` and `query: Arc<Option<Query>>` — sufficient to identify which map an asset lives in, or whether it's ad-hoc (no map entry to clean).
- **`Environment::get_asset_manager()`**: Already exists on `EnvRef` (context.rs:95), returns `Arc<Box<DefaultAssetManager<E>>>`. No trait changes required.
- **Ad-hoc assets** (`create_asset()` / `create_dummy_asset()`): Never in the maps. Monitor calls `expire()` only; no map cleanup needed. Handled transparently.
- **Untrack on replacement/re-evaluation**: When an asset is re-submitted or overridden, `untrack_expiration(old_asset_id)` cancels the pending expiration for the old version.

---

## Open Questions

1. **`TimedAsset<E>` ordering**: Must implement `Ord + Eq` for `BinaryHeap`. Will compare by `(expiration_datetime, asset_id)`. No ambiguity.
2. **`schedule_expiration` edge case**: Can `envref` be unset at the time `schedule_expiration` is called? Investigation suggests no for assets created via `DefaultAssetManager`; but `new_temporary` (used in tests) may need the per-task fallback preserved. To confirm in Phase 2.
