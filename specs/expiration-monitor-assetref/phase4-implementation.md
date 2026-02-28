# Phase 4: Implementation Plan — Expiration Monitor AssetRef Tracking

**Status:** Draft
**Feature name:** `expiration-monitor-assetref`
**Date:** 2026-02-28
**Crate:** `liquers-core`
**File:** `liquers-core/src/assets.rs` (all changes confined here)

---

## Overview

10 steps in 6 checkpoints. All changes are in a single file. Steps within a checkpoint must be done together to keep the codebase compilable between checkpoints.

```
Checkpoint 1: Add TimedAsset<E>                        (Step 1)
Checkpoint 2: Rework message type + monitor (atomic)   (Steps 2–4)
Checkpoint 3: Add remove_expired_from_maps             (Step 5)
Checkpoint 4: Async schedule_expiration + call sites   (Steps 6–7)
Checkpoint 5: Wire untrack into remove/set_binary      (Steps 8–9)
Checkpoint 6: Compile, test, remove old TODOs          (Step 10)
```

---

## Implementation Steps

Steps 1–10, grouped into 6 checkpoints. Each step specifies the Agent Assignment (model + skills + knowledge).

---

## Agent Assignment Summary

| Step | Model | Skills | Key Knowledge |
|---|---|---|---|
| 1 — TimedAsset<E> | haiku | rust-best-practices | Phase 2 doc |
| 2 — ExpirationMonitorMessage<E> | haiku | rust-best-practices | Phase 2 doc |
| 3 — track/untrack signatures | haiku | rust-best-practices | Phase 2 doc |
| 4 — run_expiration_monitor rewrite | sonnet | rust-best-practices | Phase 2 doc, assets.rs lines 759/254, recipes.rs line 137 |
| 5 — remove_expired_from_maps | sonnet | rust-best-practices | Phase 2 doc, assets.rs scc patterns lines 2544–2556 |
| 6 — schedule_expiration async | haiku | rust-best-practices | Phase 2 doc |
| 7 — call site .await | haiku | — | assets.rs lines 1160–1170 and 2475–2485 |
| 8 — untrack in remove() | haiku | — | assets.rs lines 2543–2556 |
| 9 — untrack in set_binary() | haiku | — | assets.rs lines 2625–2637 |
| 10 — compile/test/cleanup | sonnet | rust-best-practices, liquers-unittest | Phase 3 test plan, full assets.rs |

---

## Step 1 — Add `TimedAsset<E>` newtype

**Checkpoint:** 1 (standalone — pure addition, file compiles after this step alone)

**File:** `liquers-core/src/assets.rs`

**Location:** Insert immediately before `enum ExpirationMonitorMessage` (currently line 2074).

**What to add:**

```rust
/// Priority-queue element for the expiration monitor.
/// Ordered by expiration time (ascending); asset_id breaks ties for determinism.
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

**Notes:**
- `#[derive(Debug)]` is NOT added — `AssetRef<E>` does not implement `Debug` trivially, and `TimedAsset` is an internal type with no need for it.
- `AssetRef<E>: Send + Sync` holds because `E: Environment` (which is `Sized + Send + Sync + 'static`) and `E::Value: ValueInterface` (which is `Send + Sync + 'static`). Therefore `TimedAsset<E>: Send + Sync`, satisfying `tokio::spawn`.
- `Ord` only on `(expiration, asset_id)` — `asset_ref` is not compared. Clones sharing the same `id` compare equal.

**Validation:**
```bash
cargo check -p liquers-core
```

**Agent:** haiku | **Skills:** rust-best-practices | **Knowledge:** Phase 2 architecture doc

---

## Step 2 — Make `ExpirationMonitorMessage` generic and update `DefaultAssetManager::monitor_tx`

**Checkpoint:** 2 (must be done together with Steps 3–4 before compiling)

**File:** `liquers-core/src/assets.rs`

**Location:** Lines 2074–2095 (`ExpirationMonitorMessage` enum + `DefaultAssetManager` struct field).

**Change `ExpirationMonitorMessage` (lines 2074–2085):**

```rust
// REMOVE #[derive(Debug)] — AssetRef<E> is not Debug
enum ExpirationMonitorMessage<E: Environment> {
    /// Track an asset for expiration
    Track {
        asset_ref: AssetRef<E>,
        expiration_time: ExpirationTime,
    },
    /// Untrack a pending expiration (e.g., asset removed or re-evaluated)
    Untrack { asset_id: u64 },
    /// Shut down the monitor task
    Shutdown,
}
```

**Change `DefaultAssetManager` field (line 2094):**

```rust
// Before:
monitor_tx: mpsc::UnboundedSender<ExpirationMonitorMessage>,

// After:
monitor_tx: mpsc::UnboundedSender<ExpirationMonitorMessage<E>>,
```

**The `DefaultAssetManager::new()` spawn line (line 2120) stays unchanged** — the generic `E` is inferred from the impl block context:
```rust
tokio::spawn(Self::run_expiration_monitor(monitor_rx));
```

**Validation:** Do not compile yet — compile after Step 4.

**Agent:** haiku | **Skills:** rust-best-practices | **Knowledge:** Phase 2 architecture doc

---

## Step 3 — Update `track_expiration` and `untrack_expiration` signatures

**Checkpoint:** 2 (continued)

**File:** `liquers-core/src/assets.rs`

**Location:** Lines 2229–2243.

**Replace `track_expiration` (lines 2229–2236):**

```rust
/// Track an asset for expiration via the monitor task.
/// Only `ExpirationTime::At(_)` variants are tracked; Never and Immediately are no-ops.
pub fn track_expiration(&self, asset_ref: &AssetRef<E>, expiration_time: &ExpirationTime) {
    if let ExpirationTime::At(_) = expiration_time {
        let _ = self.monitor_tx.send(ExpirationMonitorMessage::Track {
            asset_ref: asset_ref.clone(),
            expiration_time: expiration_time.clone(),
        });
    }
}
```

**Replace `untrack_expiration` (lines 2239–2242):**

```rust
/// Untrack a pending expiration by asset id.
/// Idempotent — safe to call even if the asset was never tracked.
pub fn untrack_expiration(&self, asset_id: u64) {
    let _ = self.monitor_tx.send(ExpirationMonitorMessage::Untrack { asset_id });
}
```

**Note:** The `Shutdown` send at line 2380 uses `ExpirationMonitorMessage::Shutdown` which has no data fields — it requires no change.

**Validation:** Do not compile yet — compile after Step 4.

**Agent:** haiku | **Skills:** rust-best-practices | **Knowledge:** Phase 2 architecture doc

---

## Step 4 — Rewrite `run_expiration_monitor`

**Checkpoint:** 2 (final — after this step `cargo check` must pass for Steps 1–4)

**File:** `liquers-core/src/assets.rs`

**Location:** Lines 2129–2207 (the entire `run_expiration_monitor` function body).

**Replace the function body entirely:**

```rust
/// Expiration monitor task: manages a priority queue of pending expirations.
///
/// Receives `Track` / `Untrack` / `Shutdown` messages on `rx`.
/// On expiration, calls `asset_ref.expire().await` and removes the expired
/// entry from `DefaultAssetManager::assets` or `::query_assets` via `envref`.
///
/// Lock discipline: read lock on `AssetData` is acquired, query/key cloned
/// out, then the lock is dropped before any async manager operations.
async fn run_expiration_monitor(
    mut rx: mpsc::UnboundedReceiver<ExpirationMonitorMessage<E>>,
) {
    use std::collections::{BinaryHeap, HashSet};
    use std::cmp::Reverse;

    // Min-heap: soonest expiration at top (Reverse makes BinaryHeap a min-heap)
    let mut heap: BinaryHeap<Reverse<TimedAsset<E>>> = BinaryHeap::new();
    // Cancelled asset ids (Untrack received before the timer fires)
    let mut cancelled: HashSet<u64> = HashSet::new();

    loop {
        let next_expiry = heap.peek().map(|Reverse(t)| t.expiration);

        if let Some(next_dt) = next_expiry {
            let now = chrono::Utc::now();
            let sleep_duration = if next_dt > now {
                (next_dt - now).to_std().unwrap_or(std::time::Duration::from_millis(100))
            } else {
                std::time::Duration::from_millis(0)
            };

            tokio::select! {
                msg = rx.recv() => {
                    match msg {
                        Some(ExpirationMonitorMessage::Track { asset_ref, expiration_time }) => {
                            if let ExpirationTime::At(dt) = expiration_time {
                                let asset_id = asset_ref.id();
                                cancelled.remove(&asset_id);
                                heap.push(Reverse(TimedAsset { expiration: dt, asset_id, asset_ref }));
                            }
                        }
                        Some(ExpirationMonitorMessage::Untrack { asset_id }) => {
                            cancelled.insert(asset_id);
                        }
                        Some(ExpirationMonitorMessage::Shutdown) | None => {
                            return;
                        }
                    }
                }
                _ = tokio::time::sleep(sleep_duration) => {
                    while let Some(Reverse(timed)) = heap.peek() {
                        if timed.expiration <= chrono::Utc::now() {
                            // heap.peek() guarantees non-empty; use match to avoid panic in library code
                            let Reverse(timed) = match heap.pop() {
                                Some(r) => r,
                                None => break, // should not happen, but safe fallback
                            };
                            if cancelled.remove(&timed.asset_id) {
                                continue; // skip cancelled
                            }
                            let asset_ref = timed.asset_ref;
                            let asset_id = timed.asset_id;

                            // 1. Expire the asset. Errors (wrong state, already expired) are
                            //    silently ignored — concurrent re-evaluation is legitimate.
                            let _ = asset_ref.expire().await;

                            // 2. Read query and key while holding the data lock briefly.
                            //    Release lock before any async manager operations.
                            let (query, key) = {
                                let data = asset_ref.data.read().await;
                                let query = data.query.as_ref().as_ref().cloned();
                                let key = data.recipe.key().ok().flatten();
                                (query, key)
                            };

                            // 3. Remove the expired entry from in-memory maps (not from store).
                            let envref = asset_ref.get_envref().await;
                            let manager = envref.get_asset_manager();
                            manager.remove_expired_from_maps(asset_id, query.as_ref(), key.as_ref()).await;
                        } else {
                            break;
                        }
                    }
                }
            }
        } else {
            // No pending expirations — wait for next message
            match rx.recv().await {
                Some(ExpirationMonitorMessage::Track { asset_ref, expiration_time }) => {
                    if let ExpirationTime::At(dt) = expiration_time {
                        let asset_id = asset_ref.id();
                        cancelled.remove(&asset_id);
                        heap.push(Reverse(TimedAsset { expiration: dt, asset_id, asset_ref }));
                    }
                }
                Some(ExpirationMonitorMessage::Untrack { asset_id }) => {
                    cancelled.insert(asset_id);
                }
                Some(ExpirationMonitorMessage::Shutdown) | None => {
                    return;
                }
            }
        }
    }
}
```

**Validation:**
```bash
cargo check -p liquers-core
# Must pass cleanly (Steps 1–4 are now consistent)
```

**Agent:** sonnet | **Skills:** rust-best-practices | **Knowledge:** Phase 2 architecture doc, lines 2129–2207 of assets.rs, AssetRef::get_envref() (line 759), AssetData.query field (line 254), Recipe::key() (recipes.rs line 137)

---

## Step 5 — Add `DefaultAssetManager::remove_expired_from_maps`

**Checkpoint:** 3 (standalone addition — file compiles after this step alone)

**File:** `liquers-core/src/assets.rs`

**Location:** Insert after `untrack_expiration` method (after line ~2243, before `create_asset`).

**What to add:**

```rust
/// Remove an expired asset's entry from the in-memory maps.
///
/// Called by `run_expiration_monitor` after `expire()`.
/// Only removes if the stored entry has the same `asset_id` — guards against
/// a newer replacement having already taken that map slot.
///
/// Does NOT touch the backing store (expiration ≠ removal).
///
/// Arguments:
/// - `asset_id`: Unique id of the expired asset.
/// - `query`: `Some(q)` if asset lives in `self.query_assets`; `None` otherwise.
/// - `key`: `Some(k)` if asset lives in `self.assets`; `None` otherwise.
async fn remove_expired_from_maps(
    &self,
    asset_id: u64,
    query: Option<&Query>,
    key: Option<&Key>,
) {
    if let Some(query) = query {
        if let Some(entry) = self.query_assets.get_async(query).await {
            if entry.get().id() == asset_id {
                drop(entry); // release before remove_async
                let _ = self.query_assets.remove_async(query).await;
            }
        }
    } else if let Some(key) = key {
        if let Some(entry) = self.assets.get_async(key).await {
            if entry.get().id() == asset_id {
                drop(entry); // release before remove_async
                let _ = self.assets.remove_async(key).await;
            }
        }
    }
    // Ad-hoc assets (query=None, key=None): no map entry — no-op.
}
```

**Note on `drop(entry)` pattern:** `scc::HashMap::get_async` returns a guard that holds a read-lock on the bucket. The guard must be dropped before `remove_async` on the same key to avoid deadlock. This is the existing pattern in `remove()` and `set_binary()` (lines 2547–2548, 2628–2629).

**Validation:**
```bash
cargo check -p liquers-core
```

**Agent:** sonnet | **Skills:** rust-best-practices | **Knowledge:** Phase 2 architecture doc, scc::HashMap usage patterns (lines 2544–2556 in assets.rs), AssetRef::id() (line 754)

---

## Step 6 — Convert `AssetRef::schedule_expiration` from sync to async

**Checkpoint:** 4 (must be done together with Step 7 before compiling)

**File:** `liquers-core/src/assets.rs`

**Location:** Lines 1709–1730 (`schedule_expiration` method on `AssetRef<E>`).

**Replace the entire method:**

```rust
/// Schedule automatic expiration via the centralized expiration monitor.
/// Routes through `envref` so the asset manager owns the timer lifecycle.
///
/// Only `ExpirationTime::At(_)` is tracked; Never/Immediately are no-ops.
pub async fn schedule_expiration(&self, expiration_time: &ExpirationTime) {
    if let ExpirationTime::At(_) = expiration_time {
        let envref = self.get_envref().await;
        envref.get_asset_manager().track_expiration(self, expiration_time);
    }
}
```

**Note:** The old comment "Uses a Weak reference so the task exits cleanly if the asset is dropped" is removed — that was the per-task spawn pattern. The new approach holds a strong clone inside `TimedAsset<E>`.

**Validation:** Do not compile yet — compile after Step 7.

**Agent:** haiku | **Skills:** rust-best-practices | **Knowledge:** Phase 2 architecture doc, AssetRef::get_envref() (line 759), DefaultAssetManager::track_expiration (after Step 3)

---

## Step 7 — Update call sites of `schedule_expiration` (add `.await`)

**Checkpoint:** 4 (final — after this step `cargo check` must pass for Steps 6–7)

**File:** `liquers-core/src/assets.rs`

**Two locations:**

**Location A — Line 1165** (inside `AssetRef::finish_run_with_result`, an async method):
```rust
// Before:
self.schedule_expiration(&exp_time);

// After:
self.schedule_expiration(&exp_time).await;
```

**Location B — Line 2480** (inside `DefaultAssetManager::apply_immediately`, an async method):
```rust
// Before:
asset_ref.schedule_expiration(&exp_time);

// After:
asset_ref.schedule_expiration(&exp_time).await;
```

**Validation:**
```bash
cargo check -p liquers-core
# Must pass cleanly (Steps 6–7 are consistent)
```

**Agent:** haiku | **Skills:** (none needed for mechanical change) | **Knowledge:** lines 1160–1170 and 2475–2485 of assets.rs

---

## Step 8 — Wire `untrack_expiration` into `DefaultAssetManager::remove`

**Checkpoint:** 5 (must be done together with Step 9 — though each compiles independently, they should be done together for semantic completeness)

**File:** `liquers-core/src/assets.rs`

**Location:** Lines 2543–2556 (`async fn remove`).

**After `asset_ref.cancel().await?` (line 2551), add one line:**

```rust
// Before:
asset_ref.cancel().await?;
// [nothing]

// After:
asset_ref.cancel().await?;
self.untrack_expiration(asset_ref.id()); // cancel pending expiration if any
```

The full modified block (lines 2544–2556):
```rust
// 1. Check if asset exists in memory and cancel if processing
if self.assets.contains_async(key).await {
    if let Some(asset_entry) = self.assets.get_async(key).await {
        let asset_ref = asset_entry.get().clone();
        drop(asset_entry);

        // Cancel if processing
        asset_ref.cancel().await?;
        self.untrack_expiration(asset_ref.id()); // cancel pending expiration if any
    }

    // Remove from assets map
    let _ = self.assets.remove_async(key).await;
}
```

**Validation:** Compiles independently after this change.

**Agent:** haiku | **Skills:** (none needed) | **Knowledge:** lines 2543–2556 of assets.rs

---

## Step 9 — Wire `untrack_expiration` into `DefaultAssetManager::set_binary`

**Checkpoint:** 5 (continued)

**File:** `liquers-core/src/assets.rs`

**Location:** Lines 2625–2637 (`set_binary`, cancel block).

**After `asset_ref.cancel().await?` (line 2632), add one line:**

```rust
// Before:
asset_ref.cancel().await?;
// [nothing]

// After:
asset_ref.cancel().await?;
self.untrack_expiration(asset_ref.id()); // cancel pending expiration for replaced asset
```

**Validation:**
```bash
cargo check -p liquers-core
# Must pass cleanly
cargo clippy -p liquers-core --all-targets --all-features -- -D warnings
# No new warnings introduced
```

**Agent:** haiku | **Skills:** (none needed) | **Knowledge:** lines 2625–2637 of assets.rs

---

## Step 10 — Final compile, test, and cleanup

**Checkpoint:** 6

**File:** `liquers-core/src/assets.rs`

**Actions:**

1. **Remove the TODO comment** at the old expiration firing site (now replaced by Step 4):
   The lines:
   ```
   // TODO: Look up the asset and call expire() on it
   // For now, just log. The actual expire call will be wired
   // when DefaultAssetManager has a way to look up by key and expire.
   #[cfg(debug_assertions)]
   println!("Expiration monitor: asset {:?} expired", key);
   ```
   These no longer exist after Step 4's full rewrite of `run_expiration_monitor`. Verify they are gone.

2. **Remove the FIXME comment** at the heap declaration (the one the user originally referenced):
   The lines:
   ```
   // FIXME: This should not be a heap of keys, but a heap of asset references
   // FIXME: Preferably it should be weak references...
   // Map from key to AssetRef is not needed...
   ```
   Also gone after Step 4's rewrite. Verify they are gone.

3. **Run full test suite:**
   ```bash
   cargo test -p liquers-core 2>&1 | tail -20
   ```
   Existing tests must still pass.

4. **Run clippy:**
   ```bash
   cargo clippy -p liquers-core --all-targets --all-features -- -D warnings
   ```

5. **Run new expiration-specific tests** (once written per the Phase 3 test plan):
   ```bash
   cargo test -p liquers-core expir 2>&1 | tail -20
   ```

**Agent:** sonnet | **Skills:** rust-best-practices, liquers-unittest | **Knowledge:** Phase 3 test plan, entire modified assets.rs

---

## Testing Plan

| Stage | Command | What it checks |
|---|---|---|
| After Checkpoint 1 | `cargo check -p liquers-core` | `TimedAsset<E>` compiles with correct trait bounds |
| After Checkpoint 2 | `cargo check -p liquers-core` | Message type + monitor consistent, Send bounds satisfied |
| After Checkpoint 3 | `cargo check -p liquers-core` | `remove_expired_from_maps` consistent with maps and existing patterns |
| After Checkpoint 4 | `cargo check -p liquers-core` | `schedule_expiration` async + both call sites updated |
| After Checkpoint 5 | `cargo clippy -p liquers-core -- -D warnings` | No new lints, `untrack` correctly wired |
| After Checkpoint 6 | `cargo test -p liquers-core` | All existing tests pass + new expiration tests |

---

## Rollback Plan

All changes are in a single file (`liquers-core/src/assets.rs`). Rollback at any checkpoint:
```bash
git diff liquers-core/src/assets.rs  # review changes
git checkout liquers-core/src/assets.rs  # revert if needed
```

Since the feature branches by checkpoint, each checkpoint is a safe commit point:
```bash
# After each checkpoint:
git add liquers-core/src/assets.rs
git commit -m "expiration-monitor-assetref: checkpoint N complete"
```

---

## Documentation Updates

- **`specs/ISSUES.md`**: Remove the FIXME items related to expiration monitor key-tracking (if listed).
- **No changes to `specs/PROJECT_OVERVIEW.md`** — this is an internal implementation improvement, not a change to the query language or core concepts.
- **No changes to `CLAUDE.md`** — conventions are unchanged.
