# Phase 2: Architecture — Expiration Monitor AssetRef Tracking

**Status:** Draft
**Feature name:** `expiration-monitor-assetref`
**Date:** 2026-02-28

---

## Data Structures

See `TimedAsset<E>`, `ExpirationMonitorMessage<E>` sections below.

## Trait Implementations

See `TimedAsset<E>` `Ord/PartialOrd/Eq/PartialEq` section below.

## Sync vs Async

See Sync/Async Decision Summary table at end of document.

## Function Signatures

See individual "Changed Method" / "New Method" sections below.

## Error Handling

`expire()` errors (e.g., asset in wrong state) are silently ignored with `let _ = ...` — concurrent re-evaluation or removal may have changed state legitimately. `track_expiration` / `untrack_expiration` channel send errors are similarly ignored (monitor shut down).

---

## Overview

All changes are confined to `liquers-core/src/assets.rs`. The design:

1. Makes `ExpirationMonitorMessage` generic over `E: Environment`, replacing `Key` with `AssetRef<E>`.
2. Introduces a `TimedAsset<E>` newtype for use in the `BinaryHeap` (since `AssetRef<E>` is not `Ord`).
3. Makes `run_expiration_monitor` generic (via its `impl<E>` block), adding actual `expire()` calls and map cleanup via `envref`.
4. Changes `track_expiration` / `untrack_expiration` signatures to operate on `AssetRef` / `asset_id`.
5. Makes `AssetRef::schedule_expiration` async, routing through `envref` to the centralized monitor.
6. Adds `DefaultAssetManager::remove_expired_from_maps` for map cleanup.
7. Threads `untrack_expiration` into `remove()` and `set_binary()` to cancel stale expiration entries.

---

## New Type: `TimedAsset<E>`

```rust
/// Heap element for the expiration monitor priority queue.
/// Ordered by expiration time (ascending, earliest first), ties broken by asset_id.
struct TimedAsset<E: Environment> {
    expiration: chrono::DateTime<chrono::Utc>,
    asset_id: u64,
    asset_ref: AssetRef<E>,
}

impl<E: Environment> PartialEq for TimedAsset<E> {
    fn eq(&self, other: &Self) -> bool {
        self.expiration == other.expiration && self.asset_id == other.asset_id
    }
}

impl<E: Environment> Eq for TimedAsset<E> {}

impl<E: Environment> PartialOrd for TimedAsset<E> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<E: Environment> Ord for TimedAsset<E> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.expiration
            .cmp(&other.expiration)
            .then(self.asset_id.cmp(&other.asset_id))
    }
}
```

Used in: `BinaryHeap<std::cmp::Reverse<TimedAsset<E>>>` — `Reverse` makes it a min-heap (soonest expiration at top), preserving the existing `Reverse` pattern.

**Send + Sync analysis:** `AssetRef<E>` is `Arc<RwLock<AssetData<E>>>`. Since `E: Environment` (which requires `Send + Sync + 'static`) and `E::Value: ValueInterface` (which requires `Send + Sync + 'static`), `AssetData<E>` and `AssetRef<E>` are both `Send + Sync`. Therefore `TimedAsset<E>` and the heap are `Send`, satisfying `tokio::spawn` requirements.

---

## Changed Type: `ExpirationMonitorMessage<E>`

```rust
// Before (non-generic):
enum ExpirationMonitorMessage {
    Track { key: Key, expiration_time: ExpirationTime },
    Untrack { key: Key },
    Shutdown,
}

// After (generic):
enum ExpirationMonitorMessage<E: Environment> {
    Track { asset_ref: AssetRef<E>, expiration_time: ExpirationTime },
    Untrack { asset_id: u64 },
    Shutdown,
}
```

`DefaultAssetManager` field updated accordingly:

```rust
// Before:
monitor_tx: mpsc::UnboundedSender<ExpirationMonitorMessage>,

// After:
monitor_tx: mpsc::UnboundedSender<ExpirationMonitorMessage<E>>,
```

---

## Changed Function: `run_expiration_monitor`

The function remains an associated function in `impl<E: Environment> DefaultAssetManager<E>`, so `E` is in scope from the impl block. No additional generic parameters needed on the `fn` itself.

### Signature

```rust
// Before:
async fn run_expiration_monitor(
    mut rx: mpsc::UnboundedReceiver<ExpirationMonitorMessage>,
)

// After:
async fn run_expiration_monitor(
    mut rx: mpsc::UnboundedReceiver<ExpirationMonitorMessage<E>>,
)
```

### Internal state changes

```rust
// Before:
let mut heap: BinaryHeap<Reverse<(DateTime<Utc>, Key)>> = BinaryHeap::new();
let mut cancelled: HashSet<Key> = HashSet::new();

// After:
let mut heap: BinaryHeap<Reverse<TimedAsset<E>>> = BinaryHeap::new();
let mut cancelled: HashSet<u64> = HashSet::new();  // asset_id
```

### Message handling changes

```rust
// Track message:
// Before:
Some(ExpirationMonitorMessage::Track { key, expiration_time }) => {
    if let ExpirationTime::At(dt) = expiration_time {
        cancelled.remove(&key);
        heap.push(Reverse((dt, key)));
    }
}

// After:
Some(ExpirationMonitorMessage::Track { asset_ref, expiration_time }) => {
    if let ExpirationTime::At(dt) = expiration_time {
        let asset_id = asset_ref.id();
        cancelled.remove(&asset_id);
        heap.push(Reverse(TimedAsset { expiration: dt, asset_id, asset_ref }));
    }
}

// Untrack message:
// Before:
Some(ExpirationMonitorMessage::Untrack { key }) => { cancelled.insert(key); }

// After:
Some(ExpirationMonitorMessage::Untrack { asset_id }) => { cancelled.insert(asset_id); }
```

### Expiration firing logic (the key fix)

```rust
// Before (only logs):
let Reverse((_, key)) = heap.pop().unwrap_or_else(|| unreachable!());
if cancelled.remove(&key) { continue; }
println!("Expiration monitor: asset {:?} expired", key);  // TODO

// After (actually expires + cleans up maps):
let Reverse(timed) = heap.pop().unwrap_or_else(|| unreachable!());
if cancelled.remove(&timed.asset_id) { continue; }

let asset_ref = timed.asset_ref;
let asset_id = timed.asset_id;

// 1. Expire the asset (transitions to Expired status, notifies waiters).
//    Errors (e.g. asset already Expired, or not in Ready state) are silently ignored —
//    the asset may have been re-evaluated or removed concurrently.
let _ = asset_ref.expire().await;

// 2. Map cleanup: remove from DefaultAssetManager::assets or ::query_assets.
//    Must release any data lock before calling .await on manager methods.
let (query, key) = {
    let data = asset_ref.data.read().await;
    let query = data.query.as_ref().as_ref().cloned();    // Option<Query>
    let key = data.recipe.key().ok().flatten();            // Option<Key>
    (query, key)
};
let envref = asset_ref.get_envref().await;
let manager = envref.get_asset_manager();
manager.remove_expired_from_maps(asset_id, query.as_ref(), key.as_ref()).await;
```

**Lock discipline:** The `data` read lock is acquired, query/key are cloned out, then the lock is dropped before any `.await` on external async operations. No lock is held across an await point.

---

## New Method: `DefaultAssetManager::remove_expired_from_maps`

```rust
/// Remove an expired AssetRef from the manager's in-memory maps.
/// Called by the expiration monitor after expire() on the asset.
/// Only removes if the stored entry has the same asset_id, guarding against
/// a newer replacement already occupying the same slot.
///
/// Arguments:
/// - `asset_id`: Unique id of the asset that expired.
/// - `query`: If the asset is a query-asset, the query key into query_assets map.
/// - `key`: If the asset is a key-asset, the key into assets map.
async fn remove_expired_from_maps(
    &self,
    asset_id: u64,
    query: Option<&Query>,
    key: Option<&Key>,
) {
    if let Some(query) = query {
        if let Some(entry) = self.query_assets.get_async(query).await {
            if entry.get().id() == asset_id {
                drop(entry);
                let _ = self.query_assets.remove_async(query).await;
            }
        }
    } else if let Some(key) = key {
        if let Some(entry) = self.assets.get_async(key).await {
            if entry.get().id() == asset_id {
                drop(entry);
                let _ = self.assets.remove_async(key).await;
            }
        }
    }
    // Ad-hoc assets (neither query nor key): no map entry to remove.
}
```

---

## Changed Method: `DefaultAssetManager::track_expiration`

```rust
// Before:
pub fn track_expiration(&self, key: &Key, expiration_time: &ExpirationTime) {
    if !expiration_time.is_never() {
        let _ = self.monitor_tx.send(ExpirationMonitorMessage::Track {
            key: key.clone(),
            expiration_time: expiration_time.clone(),
        });
    }
}

// After:
pub fn track_expiration(&self, asset_ref: &AssetRef<E>, expiration_time: &ExpirationTime) {
    if let ExpirationTime::At(_) = expiration_time {
        let _ = self.monitor_tx.send(ExpirationMonitorMessage::Track {
            asset_ref: asset_ref.clone(),
            expiration_time: expiration_time.clone(),
        });
    }
    // Never / Immediately: not tracked (Immediately assets never reach Ready status).
}
```

---

## Changed Method: `DefaultAssetManager::untrack_expiration`

```rust
// Before:
pub fn untrack_expiration(&self, key: &Key) {
    let _ = self.monitor_tx.send(ExpirationMonitorMessage::Untrack { key: key.clone() });
}

// After:
pub fn untrack_expiration(&self, asset_id: u64) {
    let _ = self.monitor_tx.send(ExpirationMonitorMessage::Untrack { asset_id });
}
```

### Wire `untrack_expiration` into removal paths

In `DefaultAssetManager::remove(&self, key: &Key)`:
```rust
// After cancel(), add:
self.untrack_expiration(asset_ref.id());
```

In `DefaultAssetManager::set_binary(...)` (which also cancels assets):
```rust
// After cancel(), add:
self.untrack_expiration(asset_ref.id());
```

---

## Changed Method: `AssetRef::schedule_expiration`

```rust
// Before (sync, spawns per-asset tokio task):
pub fn schedule_expiration(&self, expiration_time: &ExpirationTime) {
    if let ExpirationTime::At(dt) = expiration_time {
        let weak_data = Arc::downgrade(&self.data);
        let id = self.id;
        let dt = *dt;
        tokio::spawn(async move {
            // ...sleep, upgrade weak, expire
        });
    }
}

// After (async, routes through envref to centralized monitor):
pub async fn schedule_expiration(&self, expiration_time: &ExpirationTime) {
    if let ExpirationTime::At(_) = expiration_time {
        let envref = self.get_envref().await;
        envref.get_asset_manager().track_expiration(self, expiration_time);
    }
}
```

**Call-site changes** (both are inside `async fn` bodies):
- Line 1165: `self.schedule_expiration(&exp_time)` → `self.schedule_expiration(&exp_time).await`
- Line 2480: `asset_ref.schedule_expiration(&exp_time)` → `asset_ref.schedule_expiration(&exp_time).await`

---

## Relevant Commands

This feature is internal infrastructure — no new user-facing commands.
No changes to command namespaces (`lui`, `pl`, etc.).

---

## Integration Points Checklist

| Integration | Change Required |
|---|---|
| `DefaultAssetManager::new()` | `monitor_tx` type changes; `tokio::spawn` still compiles since `E` is in scope |
| `DefaultAssetManager::remove()` | Add `untrack_expiration(asset_ref.id())` after cancel |
| `DefaultAssetManager::set_binary()` | Add `untrack_expiration(asset_ref.id())` after cancel |
| `AssetRef::run_with_future()` | `schedule_expiration(...).await` (async call) |
| `DefaultAssetManager::apply_immediately()` | `schedule_expiration(...).await` (async call) |
| `DefaultAssetManager::drop` / `shutdown` | No change; Shutdown message still sent on monitor_tx |

---

## Sync/Async Decision Summary

| Operation | Sync/Async | Reason |
|---|---|---|
| `track_expiration` | Sync | Only sends to unbounded channel, never blocks |
| `untrack_expiration` | Sync | Only sends to unbounded channel, never blocks |
| `schedule_expiration` | **Async** (changed) | Needs read lock on `AssetData` to get `envref` |
| `remove_expired_from_maps` | Async | Needs `scc::HashMap::get_async` / `remove_async` |
| `run_expiration_monitor` | Async | Unchanged; uses `tokio::select!` loop |

---

## Open Questions (Resolved from Phase 1)

1. **`TimedAsset<E>` ordering**: Resolved — `Ord` by `(expiration, asset_id)`, heap wrapped in `Reverse`.
2. **`Environment::get_asset_manager()` access**: Confirmed — already on `EnvRef` (context.rs:95).
3. **Weak references**: Explicitly deferred to a future phase per user direction.
4. **`schedule_expiration` edge case (`new_temporary`)**: `new_temporary` receives an `envref` at construction, so `get_envref().await` will succeed. No fallback needed.
