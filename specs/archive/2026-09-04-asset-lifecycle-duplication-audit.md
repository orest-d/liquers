---
title: Asset lifecycle duplication audit
kind: archive
audience: internal
area: [core/assets]
reviewed: 2026-09-04
---
# Asset lifecycle — duplication audit (as of 2026-09-03)

This is the analysis half of the former `reference/ASSET_LIFECYCLE.md`, preserved on the day the
duplication it catalogues was removed by `specs/design/evaluate-path-consolidation/`.

It is kept because it is the **evidence trail** for `CORE-EVALUATE-PATH-CONSOLIDATION`: it is the
record of what the two evaluation bodies actually differed on, written before the change, and the
reason the issue was accepted. A reader wanting the *current* behaviour should read
`reference/ASSET_LIFECYCLE.md` instead — this file describes a state that no longer exists.

Note that parts of it were already stale when archived: §6 claims the immediate path never
collected dependencies, which HEAD did do by then, and §7 Issues 3 and 5 describe work that was
completed. That staleness is preserved rather than corrected — an archive records what a document
said on a date.

## 6. Context vs Asset Responsibility Analysis

### What Context Currently Does

`Context` is created per evaluation via `AssetRef::create_context()` and holds:

| Field | Purpose | Should Stay in Context? |
|-------|---------|------------------------|
| `assetref: AssetRef<E>` | Target asset being evaluated | Delegate only; could be implicit |
| `envref: EnvRef<E>` | Access to environment services | Yes — needed for `Context::evaluate` |
| `cwd_key: Mutex<Option<Key>>` | Current working directory | Yes — per-evaluation state |
| `service_tx` | Send log/progress to service loop | Could move to AssetRef API |
| `payload: Option<E::Payload>` | UI/user context for commands | Yes — per-call state |
| `is_volatile: bool` | Propagates volatility to sub-evaluations | Yes — per-call state |
| `pending_dependencies: Arc<Mutex<Vec<DependencyRecord>>>` | Accumulated runtime deps | Borderline — could be in AssetRef |

Context methods that **directly delegate to AssetRef**:

| Context method | Delegates to |
|----------------|-------------|
| `set_value(value)` | `assetref.set_value(value)` |
| `set_state(state)` | `assetref.set_state(state)` |
| `set_error(error)` | `assetref.set_error(error)` |
| `set_expires(expires)` | `assetref.set_expiration_time(…)` + metadata |
| `set_filename(filename)` | `assetref.data.write().metadata.set_filename(…)` |
| `get_metadata()` | `assetref.data.read().metadata.metadata_record()` |

### What Should Move Out of Context

The main concern is that `Context` currently participates in the execution protocol by:
1. Holding `pending_dependencies` — these are really about the asset's evaluation result
2. `set_expires()` — an execution concern, not a per-command concern

**Recommendation**: Context should be a thin execution facade:
- Keep: `envref`, `cwd_key`, `payload`, `is_volatile`
- Remove delegation methods (they are noise; callers should use AssetRef directly, or AssetRef should grow a richer evaluation API)
- Move `pending_dependencies` into `AssetRef::evaluate_recipe()` scope (local variable passed through
  the call stack), or into `AssetData` as a per-run field that is cleared after each evaluation

### evaluate_and_store vs evaluate_immediately Asymmetry

The two paths handle post-evaluation differently:

| Concern | `evaluate_and_store` | `evaluate_immediately` |
|---------|----------------------|-----------------------|
| Status + expiration | `try_to_set_ready()` called directly → `Ready`/`Volatile` | Not set; `try_to_set_ready()` called later in `finish_run_with_result` |
| Persistence | `save_to_store()` | None |
| DM registration | `dm.track_asset()` | None |
| `ValueProduced` notification | Yes (after status is Ready/Volatile) | Yes (status still None/Processing at this point) |
| Dependency collection | Yes (from context) | Not explicitly (context still has them) |

This asymmetry means:
- `evaluate_immediately` assets are never persisted and never tracked in DM
- Their status must be set by `try_to_set_ready` (called in `finish_run_with_result`)
- Dependencies recorded in `context.pending_dependencies` during `evaluate_immediately` are
  **never written to metadata** (no `context.take_pending_dependencies()` call)

---

## 7. Identified Issues and Recommendations

### Issue 1: Triple Expiration Setting (Redundancy) — **FIXED**

**Problem**: Expiration was set in three places — `context.set_expires()`, an inline match block in
`evaluate_and_store()`, and `try_to_set_ready()` — for the queued path.

**Fix applied** (`assets.rs`): The inline `match lock.status` block in `evaluate_and_store()` was
removed. `evaluate_and_store()` now calls `try_to_set_ready()` directly, which is the single
authority for setting status (`Ready`/`Volatile`) and `expiration_time`. The write lock scope was
tightened to just data/metadata assignment; `try_to_set_ready()` acquires its own lock.
`ValueProduced` is now sent via a read lock **after** status is finalized, ensuring clients see
Ready/Volatile when they call `poll_state()` in response to the notification.

### Issue 2: Volatile ExpirationTime Overwrite (Minor Bug) — **FIXED**

**Problem**: In `try_to_set_ready()` (and previously also in the now-removed inline block):
```rust
lock.expiration_time = ExpirationTime::Immediately;  // set
lock.expiration_time = lock.metadata.expiration_time(); // immediately overwritten
```

**Fix applied** (`assets.rs`): The dead `lock.expiration_time = ExpirationTime::Immediately` line
was removed from `try_to_set_ready()`. Only `lock.metadata.expiration_time()` is used, which
returns `ExpirationTime::Immediately` for volatile metadata anyway.

### Issue 3: Dependencies Not Written for Immediate Path

**Problem**: `evaluate_immediately()` does not call `context.take_pending_dependencies()`, so
runtime dependencies observed during command execution are lost for ad-hoc assets.

**Impact**: Ad-hoc (apply_immediately) assets don't track their runtime dependencies. This is
probably acceptable since they're not stored, but it should be documented or fixed.

**Fix**: Either call `take_pending_dependencies()` in `evaluate_immediately()` and record them in
metadata, or add a comment explaining why they're intentionally discarded.

### Issue 4: Dependencies Status Rarely Set

**Problem**: `Status::Dependencies` exists but is almost never set. It was intended to be used when
an asset is waiting for sub-evaluations, but the current interpreter evaluates dependencies inline
(via `Context::evaluate`), so there's no "waiting" phase.

**Impact**: The status is dead code for current implementations.

**Recommendation**: Either implement proper async dependency pre-resolution that could use this
status, or remove it and simplify the status enum.

### Issue 5: evaluate_and_store vs try_to_set_ready Duplication

**Problem**: Both `evaluate_and_store()` and `try_to_set_ready()` contain identical logic for
setting status to Ready/Volatile and computing expiration. `try_to_set_ready` was supposed to be
the final authority but `evaluate_and_store` does it first.

**Fix**: Remove the status/expiration logic from `evaluate_and_store()`. Let `try_to_set_ready()`
be the single place where final status is determined after evaluation.

### Issue 6: resolve_volatility_before_evaluation Called Multiple Times

**Problem**: `resolve_volatility_before_evaluation()` is called in:
- `run_with_future()` (entry of run/run_immediately)
- `evaluate_and_store()` (again)
- `evaluate_immediately()` (again)
- `evaluate_recipe()` (again)

All four calls are redundant after the first.

**Fix**: Call once at the top of `run_with_future()` and remove from inner functions.

### Issue 7: Context Responsibility Creep

**Problem**: `Context` has accumulated delegation methods (`set_value`, `set_state`, `set_error`,
`set_expires`) that simply forward to `AssetRef`. These make `Context` an unnecessary intermediary.

**Recommendation**:
- For internal asset management (set_value, set_state, set_error): make them `AssetRef` methods
  only; do not expose on Context.
- For evaluation-specific operations (set_expires, set_filename): consider moving to `AssetRef`
  with a session token to prevent misuse.
- Rename `Context` to `ExecutionContext` to make its role clear: it's the context for command
  execution, not for asset lifecycle management.

---

## 8. Refactoring Opportunities

### Simplify the Expiration Path — **DONE**

Issues 1 and 2 have been applied. `evaluate_and_store()` now calls `try_to_set_ready()` directly
instead of duplicating the status/expiration logic. `try_to_set_ready()` no longer contains the
dead `ExpirationTime::Immediately` assignment. The current `try_to_set_ready()`:

```rust
async fn try_to_set_ready(&self) {
    let mut lock = self.data.write().await;
    if lock.data.is_some() {
        let metadata_expires = lock.metadata.expires();
        let should_be_volatile = lock.is_volatile || metadata_expires.is_volatile();
        if should_be_volatile {
            lock.is_volatile = true;
            lock.status = Status::Volatile;
            lock.metadata.set_volatile().ok();
            lock.expiration_time = lock.metadata.expiration_time();
        } else {
            lock.status = Status::Ready;
            lock.metadata.set_status(Status::Ready).ok();
            lock.metadata.set_expiration_time_from(&metadata_expires).ok();
            lock.expiration_time = lock.metadata.expiration_time();
        }
    } else {
        lock.status = Status::Error;
        // ... log entry
    }
}
```

### How a failed asset is typed

`fail_asset` and `fail_due_to_dependency` both clear `data` and `binary`, so the asset holds no
value. They therefore re-type the metadata as the **none** type (`retype_as_none`), because the type
axis reports what is *available*, not what the asset was going to produce. There is no `error` type
identifier: the failure is recorded in `is_error`, `Status::Error` and `error_data`, and the intent
survives in the query, the key and the filename.

This is what lets a failed asset persist at all — both halves of the type are set, so
`validate_metadata_hard` accepts it and the asset is stored as metadata with no bytes. See
`specs/reference/VALUE_TYPE_SYSTEM.md`, "How a failure is typed".

### Unify evaluate_and_store and evaluate_immediately post-processing

Both should call the same post-evaluation hook:
```rust
async fn post_evaluate(&self, result: Result<State<E::Value>, Error>, persist: bool) {
    match result {
        Ok(state) => {
            // set data, collect deps, notify ValueProduced
            if persist { self.save_to_store().await; self.dm_track().await; }
        }
        Err(e) => { /* set Error */ }
    }
}
```

### Thin Context

```rust
// After refactor: Context only carries per-call state
pub struct Context<E: Environment> {
    pub envref: EnvRef<E>,
    pub asset_key: Option<Key>,    // for dependency cycle detection
    pub cwd_key: Option<Key>,
    pub payload: Option<E::Payload>,
    pub is_volatile: bool,
    pub pending_dependencies: Vec<DependencyRecord>,
    // service_tx moved to AssetRef; not needed in Context
}
```

---

