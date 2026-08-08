# Phase 3: Examples & Use-cases — Expiration Monitor AssetRef Tracking

**Status:** Draft
**Feature name:** `expiration-monitor-assetref`
**Date:** 2026-02-28

---

## Example Type

**User choice:** Conceptual code (illustrative pseudocode — not necessarily compilable as-is)

---

## Overview Table

| # | Type | Name | Key Concept Demonstrated |
|---|------|------|--------------------------|
| 1 | Scenario | Key Asset Expiration | Named-key asset tracked via `assets` map; full expiration + map removal with id guard |
| 2 | Scenario | Query Asset Expiration | Query-keyed asset tracked via `query_assets` map; replacement guard in action |
| 3 | Scenario | Ad-hoc Asset Expiration | Asset from `create_asset()` never in any map; `expire()` fires, `remove_expired_from_maps` is no-op; old vs. new contrast |
| 4 | Unit Tests | TimedAsset ordering | Heap ordering by `(expiration, asset_id)`; min-heap via `Reverse` |
| 5 | Unit Tests | Track message handling | `Track` inserts into heap; clears prior entry from cancelled set |
| 6 | Unit Tests | Untrack / cancellation | `Untrack` adds to cancelled; heap entry skipped at fire time |
| 7 | Unit Tests | expire() called on fire | Monitor calls `asset_ref.expire().await` when deadline elapses |
| 8 | Unit Tests | remove_expired_from_maps (key) | Removes from `assets` map only when `asset_id` matches |
| 9 | Unit Tests | remove_expired_from_maps (query) | Removes from `query_assets` map only when `asset_id` matches |
| 10 | Unit Tests | remove_expired_from_maps (ad-hoc) | No-op when both `query` and `key` are `None` |
| 11 | Unit Tests | schedule_expiration routing | Async; reaches manager via `envref`; sends `Track` message |
| 12 | Unit Tests | untrack in remove() | `DefaultAssetManager::remove()` sends `Untrack` for the removed asset |
| 13 | Unit Tests | untrack in set_binary() | `DefaultAssetManager::set_binary()` sends `Untrack` for the replaced asset |
| 14 | Integration | replacement_guard_race | Replacement asset in slot before monitor fires; guard preserves new asset |
| 15 | Integration | multiple_assets_priority_order | Many assets with different expirations; min-heap fires in correct order |
| 16 | Integration | rapid_retrack | Asset re-submitted quickly; old id cancelled, new id tracked; no double-expiration |
| 17 | Integration | monitor_shutdown | Shutdown message; monitor exits cleanly without firing pending expirations |
| 18 | Integration | expired_asset_lifecycle_clones_vs_fresh | Live `AssetRef` clones observe `Expired`; new request produces fresh asset |
| 19 | Integration | channel_dropped | Sender dropped; monitor exits gracefully when `rx.recv()` returns `None` |
| 20 | Integration | expiration_never_ignored | `ExpirationTime::Never` assets are never sent to monitor channel |
| 21 | Integration | same_time_tiebreak | Two assets with identical expiration time; deterministic order by `asset_id` |
| 22 | Integration | adhoc_asset_not_in_maps | Ad-hoc asset expires; `remove_expired_from_maps` is a no-op; no panic |
| 23 | Integration | concurrent_untrack_race | `Untrack` arrives while expiration fires; both outcomes are safe |
| 24 | Integration | no_lock_deadlock | Data lock released before `.await` on manager; no deadlock under concurrent load |
| 25 | Integration | expired_asset_remains_expired_indefinitely | Monitor does not re-fire; expired asset stays expired |
| 26 | Integration | concurrent_removal_race | `remove()` and expiration fire concurrently; id guard prevents double-removal |

---

## Monitor Loop (Reference — Shown Once)

The following pseudocode shows the core `run_expiration_monitor` loop. All three scenarios feed into this same loop — only the assets tracked differ. The loop is shown here once and referenced (not repeated) in the scenario descriptions.

```rust
// DefaultAssetManager<E>::run_expiration_monitor
// (associated function in impl<E: Environment> block; E in scope from impl)
async fn run_expiration_monitor(
    mut rx: mpsc::UnboundedReceiver<ExpirationMonitorMessage<E>>,
) {
    let mut heap: BinaryHeap<Reverse<TimedAsset<E>>> = BinaryHeap::new();
    let mut cancelled: HashSet<u64> = HashSet::new(); // by asset_id

    loop {
        let deadline = heap.peek().map(|Reverse(t)| t.expiration);

        tokio::select! {
            // — Message branch —
            msg = rx.recv() => match msg {
                Some(ExpirationMonitorMessage::Track { asset_ref, expiration_time }) => {
                    if let ExpirationTime::At(dt) = expiration_time {
                        let asset_id = asset_ref.id();
                        cancelled.remove(&asset_id); // re-track cancels prior untrack
                        heap.push(Reverse(TimedAsset { expiration: dt, asset_id, asset_ref }));
                    }
                    // Never / Immediately: guard in track_expiration prevents reaching here.
                }
                Some(ExpirationMonitorMessage::Untrack { asset_id }) => {
                    cancelled.insert(asset_id);
                }
                Some(ExpirationMonitorMessage::Shutdown) | None => break,
            },

            // — Timer branch: fires when the soonest-expiring asset is due —
            _ = sleep_until(deadline), if deadline.is_some() => {
                let Reverse(timed) = heap.pop().unwrap();
                if cancelled.remove(&timed.asset_id) {
                    continue; // Cancelled — do not expire.
                }

                let asset_ref = timed.asset_ref;
                let asset_id  = timed.asset_id;

                // 1. Transition asset to Expired; notify waiters.
                //    Errors silently ignored (concurrent re-evaluation may have changed state).
                let _ = asset_ref.expire().await;

                // 2. Identify which map the asset lives in.
                //    IMPORTANT: release the read lock before calling async manager methods.
                let (query, key) = {
                    let data = asset_ref.data.read().await;
                    let query = data.query.as_ref().as_ref().cloned(); // Option<Query>
                    let key   = data.recipe.key().ok().flatten();       // Option<Key>
                    (query, key)
                }; // lock released here

                // 3. Remove from asset manager maps (id-guarded).
                let envref  = asset_ref.get_envref().await;
                let manager = envref.get_asset_manager();
                manager.remove_expired_from_maps(asset_id, query.as_ref(), key.as_ref()).await;
            }
        }
    }
}
```

---

## Scenario 1: Key Asset Expiration

**Scenario:** A named-key asset (produced by evaluating a query whose result is cached under a `Key`) reaches its expiration deadline. The monitor fires `expire()`, then removes it from `DefaultAssetManager::assets` so the next request gets a fresh evaluation.

**What is unique here:** The asset lives in the `assets` map keyed by `Key`. `data.recipe.key()` returns `Some(key)` and `data.query` is `None`, so `remove_expired_from_maps` takes the `else if let Some(key)` branch.

**Context:** Any asset produced by `/-/data.csv~filter` or similar key-rooted queries.

```rust
// ---- Setup ----
let key = parse_key("data/result.json").unwrap();

// Asset created and placed in manager.assets[key].
// schedule_expiration routes to the centralized monitor:
asset_ref.schedule_expiration(&ExpirationTime::At(Utc::now() + Duration::seconds(1))).await;
// Internally:
//   envref.get_asset_manager().track_expiration(&asset_ref, &expiration_time)
//   -> monitor_tx.send(Track { asset_ref: asset_ref.clone(), expiration_time })

// ---- 1 second later: monitor timer branch fires (see Monitor Loop above) ----
//   asset_ref.expire().await           // status -> Expired
//   data.query  -> None
//   data.recipe.key() -> Some(key)
//   remove_expired_from_maps(asset_id, None, Some(&key))

// ---- remove_expired_from_maps: key branch ----
async fn remove_expired_from_maps(
    &self,
    asset_id: u64,
    query: Option<&Query>,
    key: Option<&Key>,
) {
    // query is None — skip query_assets branch.
    if let Some(key) = key {
        if let Some(entry) = self.assets.get_async(key).await {
            if entry.get().id() == asset_id {   // <-- id guard
                drop(entry);
                let _ = self.assets.remove_async(key).await;
                // Slot is now empty; next request re-evaluates.
            }
            // id mismatch: a newer asset already occupies this slot — leave it.
        }
    }
}

// ---- Caller perspective after expiration ----
// Any AssetRef clone held by a caller:
//   asset_ref.status() -> Status::Expired
//   Waiters on asset_ref.wait_for_ready() receive Expired notification.
//
// Next request for the same key:
//   manager.assets.get(&key) -> None   (map entry removed)
//   -> fresh evaluation begins; new AssetRef with new asset_id created and tracked
```

**Expected behavior:**
- All callers holding the old `AssetRef` observe `Expired`.
- `assets[key]` is empty after cleanup.
- The next evaluation produces a new `AssetRef` with a distinct `asset_id`.
- The id guard ensures that if a fresh asset was already inserted (race with re-evaluation) it is not disturbed.

---

## Scenario 2: Query Asset Expiration

**Scenario:** A query-asset (tracked in `DefaultAssetManager::query_assets`) expires. The id guard protects a concurrently inserted replacement asset.

**What is unique here:** The asset lives in the `query_assets` map keyed by `Query`. `data.query` contains `Some(Query)` and `key` is `None`, so `remove_expired_from_maps` takes the first (`query`) branch. The id guard is the critical safety property when re-evaluation races the expiration.

**Context:** Assets evaluated from full query expressions, e.g. `/-/source.csv~normalize~aggregate`.

```rust
// ---- Setup ----
let query = parse_query("/-/source.csv~normalize~aggregate").unwrap();

// asset_ref_v1.id() == 42, placed in manager.query_assets[query].
asset_ref_v1.schedule_expiration(
    &ExpirationTime::At(Utc::now() + Duration::seconds(5))
).await;
// monitor receives: Track { asset_ref: asset_ref_v1.clone(), ... }

// ---- Race: re-evaluation before expiration fires ----
// Concurrent path evaluates the same query and stores a replacement:
//   manager.query_assets.insert(query.clone(), asset_ref_v2)  // id == 99
//   manager.untrack_expiration(42)   // Untrack { asset_id: 42 }
//   asset_ref_v1.cancel()

// ---- 5 seconds later: timer fires for id=42 ----
// Case A (Untrack arrived first):
//   cancelled.remove(&42) -> true => continue; no expire() called.
//   asset_ref_v2 (id=99) in query_assets is undisturbed.

// Case B (timer fires before Untrack is processed):
//   asset_ref_v1.expire().await       // v1 -> Expired
//   data.query -> Some(query)
//   remove_expired_from_maps(42, Some(&query), None)
//     -> query_assets.get(query) -> entry with asset_ref_v2 (id=99)
//     -> entry.get().id() == 99 != 42   // id guard fires
//     -> entry dropped, map NOT modified
//   asset_ref_v2 remains in query_assets, ready for the next request.

// ---- remove_expired_from_maps: query branch ----
async fn remove_expired_from_maps(
    &self,
    asset_id: u64,
    query: Option<&Query>,
    key: Option<&Key>,
) {
    if let Some(query) = query {
        if let Some(entry) = self.query_assets.get_async(query).await {
            if entry.get().id() == asset_id {   // <-- id guard
                drop(entry);
                let _ = self.query_assets.remove_async(query).await;
            }
            // id mismatch: replacement present; do not remove.
        }
    }
    // key is None for query assets — else-if branch not reached.
}

// ---- Live refs ----
// asset_ref_v1 (and any clones): status -> Expired (all waiters notified).
// asset_ref_v2 (replacement): status unchanged (Ready).
// Next request finds asset_ref_v2 in query_assets; no re-evaluation needed.
```

**Expected behavior:**
- The id guard is the key invariant: `remove_expired_from_maps` removes a map entry only when the stored id matches the expiring asset's id.
- Expired `v1` refs correctly propagate `Expired` to their waiters; replacement `v2` refs are unaffected.
- Both Case A and Case B leave the system in a correct, consistent state.

---

## Scenario 3: Ad-hoc Asset Expiration + Old vs. New Contrast

**Scenario:** An asset created via `create_asset()` is never inserted into `assets` or `query_assets`. When its expiration fires, `expire()` is called correctly, but `remove_expired_from_maps` is a transparent no-op.

**What is unique here:** Both `data.query` and `data.recipe.key()` are `None`. Neither map branch in `remove_expired_from_maps` executes. The monitor handles this without any special casing in the call site.

**Context:** Temporary assets for intermediate computation, dummy assets in tests, or assets handed to callers outside the normal evaluation pipeline.

```rust
// ---- Setup ----
let asset_ref = manager.create_asset(
    value,
    ExpirationTime::At(Utc::now() + Duration::seconds(10)),
).await;
// asset_ref.id() == 77
// Not inserted into any map.
// After evaluation completes, apply_immediately or finish_run_with_result calls
// schedule_expiration, which routes to track_expiration on the manager.
// create_asset itself does NOT call track_expiration directly.

// ---- 10 seconds later: monitor timer branch fires ----
//   asset_ref.expire().await          // status -> Expired (correct)
//   data.query -> None
//   data.recipe.key() -> None (or Err — both flatten to None)
//   remove_expired_from_maps(77, None, None)
//     -> query branch: query is None, skip
//     -> key branch: key is None, skip
//     -> returns immediately (no-op, no panic)

// ---- remove_expired_from_maps: both-None path ----
async fn remove_expired_from_maps(
    &self,
    asset_id: u64,
    query: Option<&Query>,
    key: Option<&Key>,
) {
    if let Some(query) = query {
        // ... query branch (not taken)
    } else if let Some(key) = key {
        // ... key branch (not taken)
    }
    // Neither branch taken: ad-hoc asset; nothing to clean up.
}
```

**Old vs. New: per-asset spawn approach vs. centralized monitor:**

```rust
// ===== OLD approach (per-asset tokio::spawn) =====
pub fn schedule_expiration_old(&self, expiration_time: &ExpirationTime) {
    if let ExpirationTime::At(dt) = expiration_time {
        let weak_data = Arc::downgrade(&self.data); // weak ref only
        let id = self.id;
        let dt = *dt;
        tokio::spawn(async move {
            tokio::time::sleep((dt - Utc::now()).to_std().unwrap_or_default()).await;
            if let Some(data) = weak_data.upgrade() {
                let asset_ref = AssetRef { data, id };
                let _ = asset_ref.expire().await;
                // No map cleanup — entry leaks in assets/query_assets.
            }
        });
        // Problems:
        // - One task spawned per asset: O(N) tasks for N tracked assets.
        // - No cancellation (no mechanism to stop the spawned task).
        // - No map cleanup for key or query assets (entries leak).
        // - On re-evaluation: both old and new tasks can fire expire() on different assets.
    }
}

// ===== NEW approach (centralized monitor) =====
pub async fn schedule_expiration_new(&self, expiration_time: &ExpirationTime) {
    if let ExpirationTime::At(_) = expiration_time {
        let envref = self.get_envref().await;
        envref.get_asset_manager().track_expiration(self, expiration_time);
        // Sends one lightweight channel message.
        // Single background task manages a priority queue: O(log N) per insert/pop.
        // Cancellation: untrack_expiration(asset_id) — one message, O(1) cancelled-set insert.
        // Map cleanup: guaranteed via remove_expired_from_maps.
        // Ad-hoc assets: handled transparently (no-op cleanup, no special casing here).
    }
}
```

**Benefits of the centralized monitor:**
- Single background task instead of one task per asset.
- Cancellation is a single channel message (`Untrack { asset_id }`).
- Map cleanup is guaranteed for key and query assets.
- Ad-hoc assets require no call-site special casing.

---

## Unit Test Plan

**File:** `liquers-core/src/assets.rs` — inline `#[cfg(test)] mod tests { ... }` at end of file.

All unit tests use helpers (`make_timed_asset`, `make_test_asset_ref`, `make_test_manager`, etc.) defined within the test module.

---

### Category 1: TimedAsset Ordering (Tests 1-3)

| # | Test Name | Description |
|---|-----------|-------------|
| 1 | `timed_asset_ord_earlier_first` | `TimedAsset` with earlier expiration is ordered before one with a later expiration; when wrapped in `Reverse` and pushed to a `BinaryHeap`, the earlier one is popped first (min-heap). |
| 2 | `timed_asset_ord_same_time_tiebreak_by_id` | Two `TimedAsset` values with the same expiration timestamp compare by `asset_id` ascending, giving a deterministic tiebreak. |
| 3 | `timed_asset_eq_same_time_and_id` | Two `TimedAsset` values with identical `expiration` and `asset_id` compare equal under `Eq`. |

```rust
#[test]
fn timed_asset_ord_earlier_first() {
    let t1 = make_timed_asset(Utc::now(), 1);
    let t2 = make_timed_asset(Utc::now() + Duration::seconds(10), 2);
    assert!(t1 < t2);
    let mut heap = BinaryHeap::new();
    heap.push(Reverse(t2));
    heap.push(Reverse(t1.clone()));
    // Min-heap: earlier expiration should pop first.
    assert_eq!(heap.pop().unwrap().0.asset_id, t1.asset_id);
}

#[test]
fn timed_asset_ord_same_time_tiebreak_by_id() {
    let t = Utc::now();
    let t1 = make_timed_asset(t, 1);
    let t2 = make_timed_asset(t, 2);
    assert!(t1 < t2); // lower asset_id orders first
}

#[test]
fn timed_asset_eq_same_time_and_id() {
    let t = Utc::now();
    assert_eq!(make_timed_asset(t, 5), make_timed_asset(t, 5));
}
```

---

### Category 2: Track Message Handling (Tests 4-6)

| # | Test Name | Description |
|---|-----------|-------------|
| 4 | `track_message_adds_to_heap` | Sending a `Track` message with `ExpirationTime::At(far_future)` causes the monitor to push an entry to the heap; the monitor shuts down cleanly thereafter. |
| 5 | `track_message_clears_cancelled_entry` | If `asset_id` was previously in `cancelled`, a subsequent `Track` for the same id removes it from `cancelled` so the asset is not skipped when the timer fires. |
| 6 | `track_never_expiration_not_forwarded` | Calling `track_expiration` with `ExpirationTime::Never` does not send a message to the monitor channel (guard in `track_expiration`). |

```rust
#[tokio::test]
async fn track_message_clears_cancelled_entry() {
    let (tx, rx) = mpsc::unbounded_channel();
    let asset_ref = make_test_asset_ref();
    let asset_id = asset_ref.id();

    // Add to cancelled first.
    tx.send(ExpirationMonitorMessage::Untrack { asset_id }).unwrap();
    // Re-track with a past deadline so the timer fires immediately.
    tx.send(ExpirationMonitorMessage::Track {
        asset_ref: asset_ref.clone(),
        expiration_time: ExpirationTime::At(past_time()),
    }).unwrap();
    tx.send(ExpirationMonitorMessage::Shutdown).unwrap();

    DefaultAssetManager::<TestEnv>::run_expiration_monitor(rx).await;

    // cancelled was cleared by Track, so expire() was called.
    assert_eq!(asset_ref.status().await, Status::Expired);
}
```

---

### Category 3: Untrack / Cancellation (Tests 7-9)

| # | Test Name | Description |
|---|-----------|-------------|
| 7 | `untrack_adds_to_cancelled_set` | `Untrack { asset_id }` causes the monitor to insert `asset_id` into the `cancelled` set; verified by observing that the asset is not expired when the timer fires. |
| 8 | `cancelled_asset_not_expired_on_fire` | When a tracked asset's timer fires and its `asset_id` is in `cancelled`, `expire()` is NOT called (asset remains in its prior status). |
| 9 | `untrack_clears_on_retrack` | After `Untrack` then `Track` for the same `asset_id`, the asset is treated as active and expires normally when the deadline is reached. |

---

### Category 4: expire() Called on Fire (Tests 10-12)

| # | Test Name | Description |
|---|-----------|-------------|
| 10 | `expire_called_when_timer_fires` | When a `Track` entry's `At(dt)` deadline elapses, `asset_ref.expire().await` is called exactly once; asset status transitions to `Expired`. |
| 11 | `expire_error_silently_ignored` | If `expire()` returns `Err` (e.g. asset already in wrong state), the monitor does not panic and continues processing subsequent messages. |
| 12 | `expire_not_called_for_cancelled` | If `asset_id` is in `cancelled` when the timer pops the entry, `expire()` is not called and the asset remains in its previous status. |

---

### Category 5: remove_expired_from_maps — Key Branch (Tests 13-14)

| # | Test Name | Description |
|---|-----------|-------------|
| 13 | `remove_expired_from_maps_key_matching_id` | When `key` is `Some` and the `assets` map entry's `asset_id` matches, the entry is removed. |
| 14 | `remove_expired_from_maps_key_mismatched_id` | When `key` is `Some` but the `assets` map entry's `asset_id` does not match (a replacement is present), the entry is NOT removed. |

```rust
#[tokio::test]
async fn remove_expired_from_maps_key_matching_id() {
    let manager = make_test_manager().await;
    let key = parse_key("test/data.json").unwrap();
    let asset_ref = make_test_asset_ref_with_id(42);
    manager.assets.insert(key.clone(), asset_ref.clone());

    manager.remove_expired_from_maps(42, None, Some(&key)).await;

    assert!(manager.assets.get(&key).is_none());
}

#[tokio::test]
async fn remove_expired_from_maps_key_mismatched_id() {
    let manager = make_test_manager().await;
    let key = parse_key("test/data.json").unwrap();
    let new_asset = make_test_asset_ref_with_id(99); // replacement
    manager.assets.insert(key.clone(), new_asset.clone());

    // Attempt to remove on behalf of old asset (id=42); map has id=99.
    manager.remove_expired_from_maps(42, None, Some(&key)).await;

    // Replacement must remain.
    assert_eq!(manager.assets.get(&key).unwrap().id(), 99);
}
```

---

### Category 6: remove_expired_from_maps — Query Branch (Tests 15-16)

| # | Test Name | Description |
|---|-----------|-------------|
| 15 | `remove_expired_from_maps_query_matching_id` | When `query` is `Some` and the `query_assets` entry's `asset_id` matches, the entry is removed. |
| 16 | `remove_expired_from_maps_query_mismatched_id` | When `query` is `Some` but the `query_assets` entry's `asset_id` does not match (replacement present), the entry is NOT removed. |

---

### Category 7: remove_expired_from_maps — Ad-hoc Branch (Tests 17-18)

| # | Test Name | Description |
|---|-----------|-------------|
| 17 | `remove_expired_from_maps_adhoc_no_panic` | When both `query` and `key` are `None`, `remove_expired_from_maps` returns without error. |
| 18 | `remove_expired_from_maps_adhoc_maps_unchanged` | After a no-op call for an ad-hoc asset, any unrelated entries already in `assets` and `query_assets` remain intact. |

---

### Category 8: schedule_expiration Routing (Tests 19-21)

| # | Test Name | Description |
|---|-----------|-------------|
| 19 | `schedule_expiration_sends_track_message` | Calling `asset_ref.schedule_expiration(&At(dt)).await` results in exactly one `Track` message arriving on the monitor channel, containing the same `AssetRef`. |
| 20 | `schedule_expiration_never_sends_no_message` | Calling with `ExpirationTime::Never` sends no message to the monitor channel; channel remains empty. |
| 21 | `schedule_expiration_is_async` | `schedule_expiration` is an `async fn`; it must be `.await`-ed; calling it from a `tokio::test` context completes without blocking. |

---

### Category 9: untrack in remove() (Tests 22-23)

| # | Test Name | Description |
|---|-----------|-------------|
| 22 | `remove_calls_untrack_expiration` | `DefaultAssetManager::remove(&key)` sends an `Untrack { asset_id }` message to the monitor for the removed asset. |
| 23 | `remove_untrack_uses_correct_asset_id` | The `asset_id` in the `Untrack` message matches `asset_ref.id()` of the asset that was in the map at removal time. |

---

### Category 10: untrack in set_binary() (Tests 24-25)

| # | Test Name | Description |
|---|-----------|-------------|
| 24 | `set_binary_calls_untrack_for_replaced_asset` | When `set_binary()` cancels an existing asset, it sends `Untrack { asset_id }` for the old asset's id. |
| 25 | `set_binary_does_not_untrack_when_no_prior_asset` | When `set_binary()` is called for a key with no existing asset in the map, no `Untrack` message is sent. |

---

## Integration Test Plan

**File:** `liquers-core/tests/expiration_monitor.rs`

All tests are `#[tokio::test]`. Tests use a real `DefaultAssetManager` with a live monitor task (started via `DefaultAssetManager::new()`). Short durations (50-300ms) are used to keep test runtime acceptable.

---

### Test 14: replacement_guard_race

**Setup:**
1. Create manager with live monitor.
2. Insert `asset_ref_v1` (id=42) into `assets[key]`; track with expiration T+50ms.
3. At T+10ms: insert `asset_ref_v2` (id=99) into `assets[key]`; call `untrack_expiration(42)`.
4. Wait until T+100ms.

**Key assertions:**
- `assets[key]` contains `asset_ref_v2` (id=99).
- `asset_ref_v2.status()` is `Ready`.
- Whether `asset_ref_v1` is `Expired` or not depends on race outcome, but the map is always in a consistent state.

---

### Test 15: multiple_assets_priority_order

**Setup:**
1. Track 5 assets with expiration offsets: 300ms, 100ms, 500ms, 200ms, 400ms.
2. Each asset records the wall-clock time it observed `Expired` status.
3. Wait 600ms.

**Key assertions:**
- All 5 assets reach `Expired`.
- The expiration sequence is: 100ms asset, 200ms asset, 300ms asset, 400ms asset, 500ms asset.
- No asset expires more than once.

---

### Test 16: rapid_retrack

**Setup:**
1. Track `asset_ref_v1` (id=10) with expiration T+100ms.
2. Immediately (T+1ms): send `Untrack { asset_id: 10 }`.
3. Immediately: track `asset_ref_v2` (id=20) with expiration T+100ms.
4. Wait until T+200ms.

**Key assertions:**
- `asset_ref_v1.status()` is NOT `Expired` (untracked before firing, or timer skips due to cancelled).
- `asset_ref_v2.status()` IS `Expired`.
- No double-expiration observed on either asset.

---

### Test 17: monitor_shutdown

**Setup:**
1. Track 3 assets with expirations T+1s, T+2s, T+3s.
2. At T+200ms, send `Shutdown` message.
3. Wait for the monitor `JoinHandle` to resolve.

**Key assertions:**
- Monitor task exits cleanly (no panic, `JoinHandle` resolves `Ok`).
- None of the 3 assets have transitioned to `Expired` (shutdown before any timer fired).

---

### Test 18: expired_asset_lifecycle_clones_vs_fresh

**Setup:**
1. Create `asset_ref_a` in `assets[key]` with expiration T+50ms.
2. Retain `asset_ref_a_clone = asset_ref_a.clone()`.
3. Wait until T+100ms.
4. Submit a new evaluation for the same `key`.

**Key assertions:**
- `asset_ref_a.status()` == `Expired`.
- `asset_ref_a_clone.status()` == `Expired` (same underlying `Arc`; both see the same state).
- After re-evaluation: `assets[key]` contains a new asset with a different `id`.
- The new asset's status is `Ready`.

---

### Test 19: channel_dropped

**Setup:**
1. Start a monitor with a detached channel (hold only the receiver, drop the sender side).
2. Observe the monitor task.

**Key assertions:**
- `rx.recv()` returns `None` immediately (no sender).
- The `None` arm causes `break`.
- Monitor task exits cleanly (no panic from unwrapping `None`).

---

### Test 20: expiration_never_ignored

**Setup:**
1. Create an asset; call `asset_ref.schedule_expiration(&ExpirationTime::Never).await`.
2. Inspect the monitor channel.
3. Wait 200ms.

**Key assertions:**
- No message was sent to the monitor channel.
- `asset_ref.status()` remains `Ready` after 200ms.
- Monitor heap is empty.

---

### Test 21: same_time_tiebreak

**Setup:**
1. Track two assets with identical expiration time `t`: `asset_ref_a` (id=5), `asset_ref_b` (id=3).
2. Wait until both have expired.

**Key assertions:**
- Both assets eventually reach `Expired` (no starvation).
- `asset_ref_b` (id=3) expires in the same or earlier iteration than `asset_ref_a` (id=5), consistent with ascending `asset_id` ordering.
- Ordering is deterministic: running the test multiple times yields the same sequence.

---

### Test 22: adhoc_asset_not_in_maps

**Setup:**
1. Create an ad-hoc asset via `create_asset()` with expiration T+50ms.
2. Verify the asset is not in `assets` or `query_assets` before expiration.
3. Wait until T+100ms.

**Key assertions:**
- `asset_ref.status()` == `Expired` (`expire()` was called correctly).
- `assets` is empty.
- `query_assets` is empty.
- No panic from `remove_expired_from_maps`.

---

### Test 23: concurrent_untrack_race

**Setup:**
1. Track asset (id=7) with expiration T+50ms.
2. At T+45ms, spawn a task that sends `Untrack { asset_id: 7 }`.
3. Wait until T+100ms.

**Key assertions (either outcome is correct):**
- **Outcome A (Untrack first):** Asset NOT expired. `cancelled` set absorbed the id; timer branch skipped it.
- **Outcome B (timer first):** Asset IS expired. `cancelled.remove(7)` returned false; `expire()` called; map cleaned up.
- In neither case is there a panic or data structure corruption.
- The test may assert that only one outcome happens per run (no partial-state corruption).

---

### Test 24: no_lock_deadlock

**Setup:**
1. Create 20 assets all expiring at T+50ms.
2. Concurrently, a background task calls `manager.assets.get_async(key)` at 10ms intervals throughout.
3. Wait until T+200ms.

**Key assertions:**
- All 20 assets reach `Expired`.
- Test completes within a 5-second timeout (deadlock would cause timeout).
- Validates that no `AssetData` read lock is held across `.await` on manager map operations.

---

### Test 25: expired_asset_remains_expired_indefinitely

**Setup:**
1. Track asset with expiration T+50ms.
2. Wait until T+100ms (asset expires).
3. Check status at T+100ms and again at T+600ms.

**Key assertions:**
- `asset_ref.status()` is `Expired` at both checkpoints.
- Monitor does not re-fire or re-enqueue the asset.
- Monitor heap is empty after the single expiration.

---

### Test 26: concurrent_removal_race

**Setup:**
1. Insert `asset_ref` (id=15) into `assets[key]`; track with expiration T+50ms.
2. At T+45ms, spawn a task calling `manager.remove(&key)`.
3. Wait until T+100ms.

**Key assertions (either race outcome is correct):**
- **remove() wins:** `assets[key]` is empty; monitor's `remove_expired_from_maps` finds no entry; no panic. `untrack_expiration(15)` from `remove()` may prevent `expire()` from being called if `Untrack` arrives before the timer.
- **Monitor wins:** `expire()` called; `remove_expired_from_maps` finds the entry already gone (or matches id and removes it); no double-remove panic.
- In all outcomes: no panic, no data corruption, `assets` map is empty.

---

## Corner Cases

### 1. Replacement Guard Invariant

**Situation:** A new asset is inserted into the map (re-evaluation) between when the old asset's expiration was tracked and when the monitor fires.

**Expected behavior:** The id guard in `remove_expired_from_maps` prevents removal of the replacement. The check is `entry.get().id() == asset_id` where `asset_id` is the id of the asset being expired, not the id of whatever is currently in the map.

**Failure mode without guard:** The monitor would remove the freshly-evaluated replacement, forcing the next caller to re-evaluate unnecessarily and potentially triggering a thundering herd of redundant evaluations.

---

### 2. Lock Discipline — No Lock Held Across Await

**Situation:** The monitor must read `AssetData` fields (`query`, `recipe.key()`) and then call async operations on the manager. Both operations require the same `RwLock`.

**Expected behavior:** The read lock guard is dropped before any `.await` on manager methods. The explicit block scope enforces this:

```rust
let (query, key) = {
    let data = asset_ref.data.read().await;
    let query = data.query.as_ref().as_ref().cloned();
    let key   = data.recipe.key().ok().flatten();
    (query, key)
}; // guard dropped here — no lock held below this line

manager.remove_expired_from_maps(asset_id, query.as_ref(), key.as_ref()).await;
```

**Failure mode:** Holding the `RwLock` read guard across an `.await` causes deadlock if any code path in `remove_expired_from_maps` (or its callees in `scc::HashMap`) attempts a write lock on the same `AssetData`.

---

### 3. Channel Send Errors Are Silent

**Situation:** `track_expiration` and `untrack_expiration` both use `let _ = self.monitor_tx.send(...)`. The monitor task may have exited (due to shutdown or panic) before the send.

**Expected behavior:** The error is silently discarded. If the monitor is down:
- `track_expiration` failure: the asset's expiration will simply not fire — acceptable during shutdown.
- `untrack_expiration` failure: the cancellation is not registered — acceptable since the monitor is no longer running.

**Failure mode:** Returning `Err` or panicking from `track_expiration` / `untrack_expiration` would break all callers, which currently treat these as fire-and-forget operations.

---

### 4. ExpirationTime::Immediately Assets

**Situation:** An asset created with `ExpirationTime::Immediately` is intended to be expired before it ever reaches `Ready` status and is therefore never a candidate for monitor tracking.

**Expected behavior:** The guard `if let ExpirationTime::At(_) = expiration_time` in both `track_expiration` and `schedule_expiration` ensures `Immediately` assets are never sent to the monitor. They are handled by the evaluation pipeline directly.

**Failure mode:** Sending `Immediately` assets to the monitor would add them to the heap with `dt ≈ now`, causing an immediate but redundant fire that races with the normal `Immediately` handling path, potentially calling `expire()` on an asset already in an incompatible state.

---

### 5. Asset Id Uniqueness

**Situation:** The cancelled set and id guard in `remove_expired_from_maps` both rely on `asset_id` values being unique across all assets in the lifetime of a single `DefaultAssetManager` instance.

**Expected behavior:** `asset_id` is a monotonically incrementing `u64` counter in `DefaultAssetManager::new()`. Given that `u64::MAX` is approximately 1.8 × 10^19, overflow is not a practical concern for any realistic workload.

**Failure mode:** If two distinct `AssetRef` values shared an `asset_id`, an `Untrack` message intended for one could cancel the other, or the id guard could leave a stale entry in the map while removing a live replacement.

---

### 6. Monitor Holds Strong AssetRef Clones

**Situation:** The monitor's heap holds strong `Arc` clones of `AssetRef<E>` (not weak references). This keeps the asset alive for the duration of the pending expiration.

**Expected behavior:** After `expire()` fires and the `TimedAsset` is popped from the heap, the monitor releases its clone. If no other callers hold references, the asset is dropped at that point. The asset's in-memory representation is always reachable for the duration of its tracking period.

**Rationale (from Phase 1):** Strong refs are intentional. The monitor must call `expire()` on a live object. A weak ref approach would silently fail to expire an asset if all caller refs were dropped before the deadline — violating the expiration contract. The cost is one extra `Arc` clone per tracked asset, which is acceptable.

**Note:** Weak reference support is explicitly deferred to a future phase per Phase 2 resolved questions.

---

## Summary of Key Invariants

| Invariant | Enforced By |
|-----------|-------------|
| Heap is min-ordered by `(expiration, asset_id)` | `TimedAsset<E>: Ord` + `Reverse` wrapper |
| Cancelled assets never have `expire()` called | `cancelled.remove(&asset_id)` check before firing |
| Map replacement is never removed by stale expiration | Id guard in `remove_expired_from_maps` |
| Ad-hoc assets handled without special casing at call sites | Both-`None` path in `remove_expired_from_maps` |
| No `AssetData` lock held across async map operations | Explicit block scoping of `let data = ... { }` |
| Channel errors are silent (monitor shutdown is ok) | `let _ = monitor_tx.send(...)` pattern |
| `ExpirationTime::Never` never reaches the monitor | Guard in `track_expiration` and `schedule_expiration` |
| `ExpirationTime::Immediately` never reaches the monitor | Same guard as above |
