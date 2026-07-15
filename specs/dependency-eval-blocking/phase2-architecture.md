# Phase 2: Solution & Architecture - Non-Blocking Dependency Evaluation

## Overview

Recipe delegation is reworked from **inline blocking** to **suspend/resume**. The internal
evaluation path returns an `EvaluationOutcome<E>` (`Completed(State)` or `Delegated(child)`)
instead of calling `child.get().await` while holding the parent's job slot. On `Delegated`,
the parent records the dependency, enters `Status::Dependencies`, ensures the child is
submitted, stamps a non-persisted `waiting_on` marker with a monotonic wait generation, and
**returns from `run()`** so its queue slot is freed. `DefaultAssetManager` registers a
one-shot completion hook (a spawned task subscribed to the child's existing
`watch::Receiver<AssetNotificationMessage>`); when the child reaches a terminal status the
hook resumes the parent via `leave_dependencies_for_resubmit()` + `JobQueue::submit()`. All
changes are confined to `liquers-core/src/assets.rs`.

## Data Structures

### New Enums

#### `EvaluationOutcome<E: Environment>`
```rust
/// Internal result of a single evaluation attempt of a recipe.
/// Not public API; used only between `evaluate_recipe` and `evaluate_and_store`/`run`.
enum EvaluationOutcome<E: Environment> {
    /// Recipe produced a final state in this attempt.
    Completed(State<E::Value>),
    /// Recipe delegates (pure-key) to another asset that is not yet ready.
    /// The parent must suspend and be resumed when `child` becomes terminal.
    Delegated { child: AssetRef<E>, generation: u64 },
}
```

**Variant semantics:**
- `Completed(state)`: the recipe (self-recipe or apply-recipe path) yielded a state; the
  existing store/finalize path runs unchanged.
- `Delegated { child, generation }`: pure-key delegation to a not-yet-ready `child`. The
  parent has already recorded the dependency and entered `Status::Dependencies`;
  `generation` is the wait generation stamped on the parent so the completion hook can be
  rejected if stale. Carrying `child` avoids a second manager lookup at hook-registration
  time.

**No default match arm:** every `match` on `EvaluationOutcome` handles both variants
explicitly (CLAUDE.md rule).

**Ownership:** `AssetRef<E>` is already an `Arc`-backed handle (cheap clone); `generation`
is a `Copy` `u64`. The enum is neither stored nor serialized — it lives on the stack for the
duration of one evaluation attempt.

### `AssetData<E>` field additions

```rust
pub struct AssetData<E: Environment> {
    // ... existing fields ...

    /// Scheduler-local bookkeeping: the child this asset is currently waiting on.
    /// NON-AUTHORITATIVE and NON-PERSISTED. The dependency graph / metadata remain the
    /// source of truth. Cleared on resume. Used for diagnostics and to prevent duplicate
    /// completion hooks. `WeakAssetRef` avoids a strong parent→child cycle keeping assets
    /// alive past their natural lifetime.
    waiting_on: Option<WeakAssetRef<E>>,

    /// Monotonic wait-generation counter. Incremented every time the asset enters a
    /// dependency wait. A completion hook captured at generation `g` is ignored on resume
    /// if `wait_generation != g` (asset was cancelled, overridden, expired, or re-waited
    /// meanwhile). NON-PERSISTED.
    wait_generation: u64,
}
```

**Ownership rationale:**
- `waiting_on: Option<WeakAssetRef<E>>` — weak to avoid a parent holding its child (and
  transitively a whole chain) alive; the child is independently kept alive by the queue /
  manager registry while it runs. `None` when not waiting.
- `wait_generation: u64` — plain `Copy` counter, no allocation.

**Serialization:** both fields are runtime-only. `AssetData` is not `Serialize` (it holds
channels/`EnvRef`); only `MetadataRecord` is persisted, and it is **not** extended — so no
`#[serde(skip)]` annotation is required. Explicitly documented as non-persisted.

**`WeakAssetRef<E>` — reuse existing (verified against code).** No new type is needed. The
weak handle already exists in `assets.rs`:
```rust
// assets.rs:714-717 / 720-723 (real shapes)
pub struct AssetRef<E: Environment>     { id: u64, data: Arc<RwLock<AssetData<E>>> }
pub struct WeakAssetRef<E: Environment> { id: u64, data: Weak<RwLock<AssetData<E>>> }

impl<E: Environment> AssetRef<E>     { fn downgrade(&self) -> WeakAssetRef<E>;         } // ~912
impl<E: Environment> WeakAssetRef<E> { fn upgrade(&self) -> Option<AssetRef<E>>;       } // ~2181
```
`waiting_on` is stored via `child.downgrade()` and read back via `.upgrade()`. Note the real
type carries an `id: u64` alongside the `Arc`/`Weak` — it is **not** a bare newtype.

## Trait Implementations

**None.** No new traits and no new trait impls. The feature adds inherent methods on the
existing `AssetRef<E>` / `AssetData<E>` / `DefaultAssetManager<E>` impl blocks and one
internal enum. The `AssetManager` trait is **not** modified (it is implemented by
`DefaultAssetManager` and consumed by `liquers-py`; changing it would ripple). Resumption is
an inherent method on `DefaultAssetManager`, invoked from within the completion-hook task
that the manager itself spawns.

## Generic Parameters & Bounds

All new items are parameterized by the existing `E: Environment` bound already used
throughout `assets.rs`. The completion-hook task requires `E: Environment + 'static` and
`E::Value: Send + Sync + 'static` — the **same** bounds `JobQueue<E>` and the existing
`tokio::spawn(async move { asset_clone.run().await; ... })` call sites already satisfy
(`impl<E: Environment + 'static> JobQueue<E>`), so no new bound is introduced. No new generic
type parameters are added.

## Sync vs Async Decisions

| Function | Async? | Rationale |
|----------|--------|-----------|
| `evaluate_recipe_outcome` (new internal) | Yes | Reads asset data via `RwLock`, calls async manager/recipe APIs |
| `enter_dependency_wait` (new, on `AssetRef`) | Yes | Writes `AssetData` under `RwLock`, sends notifications |
| `resume_after_dependency` (new, on `DefaultAssetManager`) | Yes | Re-checks child status, re-submits to `JobQueue` (async) |
| `register_completion_hook` (new, on `DefaultAssetManager`) | No (spawns) | Synchronously spawns a task; the task body is async |
| completion-hook task body | Yes | `child.subscribe_to_notifications()` + `changed().await` loop |

**Decision:** everything stays async and reuses the existing tokio runtime and the
`Notify`-driven `JobQueue`. No sync wrappers needed (this path is never called from
`liquers-py` sync bindings directly — Python goes through the manager's public async API).

## Function Signatures

All in `liquers-core/src/assets.rs`.

### On `AssetRef<E>`

```rust
/// Evaluate the recipe once, returning either a completed state or a delegation request.
/// Replaces the delegating branch of the current `evaluate_recipe`.
async fn evaluate_recipe_outcome(&self) -> Result<EvaluationOutcome<E>, Error>;

/// Enter a dependency wait: record dependency (already done by caller), set
/// `Status::Dependencies`, bump `wait_generation`, store `waiting_on = downgrade(child)`,
/// and return the new generation. Extends the existing `enter_dependencies` helper.
async fn enter_dependency_wait(&self, child: &AssetRef<E>) -> Result<u64, Error>;

/// Clear `waiting_on`, leave `Status::Dependencies` -> `Submitted`. Extends the existing
/// `leave_dependencies_for_resubmit`. Returns the generation that was active, or `None`
/// if the asset was not waiting.
async fn clear_dependency_wait(&self) -> Result<Option<u64>, Error>;

/// Read the current wait generation (for stale-hook checks).
async fn wait_generation(&self) -> u64;
```

**No compatibility shim needed (verified).** `evaluate_recipe()` is `pub` but has exactly
one caller — `evaluate_and_store()` at `assets.rs:1509` — and no external or test callers.
Therefore `evaluate_recipe()` is either (a) refactored in place to first compute the outcome
and, on `Delegated`, return early to its sole caller, or (b) demoted so `evaluate_and_store`
calls `evaluate_recipe_outcome()` directly. Option (b) is preferred: it removes any lingering
inline/blocking path entirely, honoring the Phase 1 "inline guard removed" decision. There is
no dual synchronous/asynchronous delegation path.

### On `DefaultAssetManager<E>`

```rust
/// Spawn a one-shot task that watches `child` for a terminal status and, when reached,
/// resumes `parent` exactly once for wait `generation`. Idempotent per (parent, generation).
fn register_completion_hook(
    &self,
    parent: AssetRef<E>,
    child: AssetRef<E>,
    generation: u64,
);

/// Resume a suspended parent: verify it is still waiting at `generation` and not
/// cancelled/finished; clear the wait; re-submit to the JobQueue. A stale generation is a
/// silent no-op.
async fn resume_after_dependency(&self, parent: AssetRef<E>, generation: u64) -> Result<(), Error>;
```

### Changed control flow (signatures unchanged, bodies reworked)

```rust
// run() / evaluate_and_store() gain a match on the outcome:
async fn evaluate_and_store(&self) -> Result<(), Error>;
// -> on Delegated { child, generation }: register hook, return Ok(()) (slot freed).
// -> on Completed(state): existing store/finalize path.
```

## Integration Points

### Crate: `liquers-core`  (only crate touched)

**File:** `liquers-core/src/assets.rs`

1. **`EvaluationOutcome<E>` enum** — new, near the top of the impl region (private).
2. **`AssetData<E>`** (struct at line 229) — add `waiting_on` + `wait_generation`; initialize
   to `None`/`0` in `new_ext` (line ~302) and any other constructor.
3. **`WeakAssetRef<E>`** — add if not present, alongside `AssetRef<E>` definition.
4. **`evaluate_recipe` (line 1412)** — split the delegating branch (lines 1439-1465) out into
   `evaluate_recipe_outcome`. The current inline guard (lines 1447-1455,
   `Box::pin(asset.run()).await`) is **removed**. Instead:
   - record dependency (`record_dependency_on_asset`, unchanged),
   - if `child.poll_state().is_none()`: `generation = self.enter_dependency_wait(&child)`,
     ensure child submitted (`manager.job_queue.submit(child.clone())` — idempotent; there is
     no `submit` on the manager itself, only `JobQueue::submit`, reached via the private
     module-scoped `DefaultAssetManager::job_queue: Arc<JobQueue<E>>` as existing code does at
     `assets.rs:2913/2961/3012`), return `Delegated { child, generation }`,
   - else: return `Completed(child.get().await?)` (fast path, child already ready).
5. **`evaluate_and_store` (line 1507)** — match the outcome:
   - `Completed(state)` → existing store/finalize block (lines 1511-…),
   - `Delegated { child, generation }` → `manager.register_completion_hook(self.clone(),
     child, generation)`, then `return Ok(())`. `run_with_future`/`run` return, the
     `tokio::spawn` wrapper in `JobQueue` decrements `running_count` and `notify_one()`s —
     **slot freed**.
6. **`DefaultAssetManager` (line 2341)** — add `register_completion_hook` +
   `resume_after_dependency`. The manager holds `job_queue: Arc<JobQueue<E>>` (private,
   module-scoped in `assets.rs`); `get_asset_manager()` returns the concrete
   `Arc<Box<DefaultAssetManager<E>>>` (context.rs:53/96/515/643), so these inherent methods
   and `self.job_queue.submit(...)` are reachable. The hook task (note:
   `AssetRef::subscribe_to_notifications` is the **async** method at `assets.rs:1692`, not the
   sync `AssetData` one at 568):
   ```text
   let mut rx = child.subscribe_to_notifications().await;
   // Pre-check FIRST: tokio's watch `changed()` does NOT re-deliver the current value, so a
   // child that is already terminal at subscription time must be caught before awaiting.
   // This mirrors the codebase pattern (`rx.borrow().clone()` then `changed().await`, e.g.
   // assets.rs:1247/1750/1959). Safety here comes from this pre-check, not from watch replay.
   loop {
       if child.status().await.is_finished() { break; }
       if rx.changed().await.is_err() { break; }   // child dropped
   }
   manager.resume_after_dependency(parent, generation).await
   ```
   `resume_after_dependency`:
   - `if parent.wait_generation().await != generation { return Ok(()); }` (stale hook),
   - `if parent.status().await` is `Cancelled`/`Error`/finished → no-op,
   - `if child` failed → `parent.fail_due_to_dependency(child_error)` (existing),
   - else `parent.clear_dependency_wait().await?; self.job_queue.submit(parent).await?;`.
7. **Re-entry:** when the resubmitted parent runs again, `evaluate_recipe_outcome` re-checks
   `child.poll_state()`. If ready → `Completed`. If (rare) still not ready → a fresh
   `Delegated` at a new generation (the previous hook is already consumed). This makes
   resumption self-correcting without trusting stale hook state.

**Terminal-status helper:** reuse existing `Status::is_finished()` (`metadata.rs:372`) for
the hook's break condition. **Verified:** it already returns `true` for both `Error` and
`Cancelled`, so no `is_terminal()` extension is needed.

**Removed:** the inline `Box::pin(asset.run()).await` deadlock guard at `assets.rs:1451`
(inside the `Submitted | Dependencies` check at 1447-1455) and its explanatory comment.
Update the corresponding narrative in `specs/DEPENDENCIES_STATUS.md` (Flow A step 5) as part
of implementation.

### Dependencies

No new crate dependencies. `tokio` (`sync`, `rt`, `macros`, `time`), `Weak`/`Arc` from std,
and the existing `watch`/`Notify` primitives are already in use.

## Relevant Commands

### New Commands
**None.** This feature adds no commands.

### Relevant Existing Namespaces
**None functionally.** Delegation is exercised by *recipes* (pure-key `-R/<key>` targets),
not by a command namespace. Test recipes use the standard `SimpleEnvironment<Value>` +
`register_command!` fixtures (e.g. a trivial `to_text`/gate command) only to *build* the
delegation chain, not because any namespace is intrinsic to the feature.

**Question for user:** confirm no command-namespace involvement is expected (the feature is
purely scheduler/asset-lifecycle mechanics).

## Web Endpoints

**None.** No routes added or modified. Existing metadata/status endpoints report the
`Status::Dependencies` state that already exists.

## Error Handling

Uses existing typed constructors only (no `Error::new`, no new `ErrorType`).

| Scenario | Constructor | Notes |
|----------|-------------|-------|
| Delegated child fails | `fail_due_to_dependency(child_err)` (existing) | Sets `Status::Error`, `ErrorOccurred` |
| Delegation cycle | `Error::dependency_cycle(&key)` (existing, in `record_dependency_on_asset`) | Detected before entering wait; fails fast, no hang |
| Child `get()` error on re-entry | `Error::general_error(format!("Delegated dependency asset {} failed: {}", child.id(), e))` (existing text at 1457-1463) | Preserved |
| Resubmit fails (queue shut down) | propagate `JobQueue::submit` error via `?` | Parent left in `Dependencies`; documented terminal-on-shutdown behavior |

**Error propagation:** `?` throughout. The completion-hook task is spawned detached; it must
not panic — any error from `resume_after_dependency` is logged via the asset's existing log
mechanism / `eprintln!` consistent with current `JobQueue` diagnostics, never `unwrap()`.

## Serialization Strategy

**No serialization changes.** `waiting_on` and `wait_generation` are runtime-only fields on
the non-serialized `AssetData`. `MetadataRecord` (the persisted structure) is **unchanged**,
preserving cross-restart compatibility. Dependency edges continue to serialize exactly as
today via the existing `MetadataRecord.dependencies` path.

## Concurrency Considerations

- **`AssetData` access** stays behind its existing `RwLock`; the two new fields are mutated
  only under the write lock in `enter_dependency_wait` / `clear_dependency_wait`.
- **Wait generation** guards against three races: (a) a stale hook firing after the parent
  was cancelled/expired, (b) duplicate hooks from re-entry, (c) the child completing between
  the `poll_state().is_none()` check and hook registration — case (c) is handled by the
  hook's **pre-check** (`if child.status().await.is_finished() { break; }`) *before* the first
  `rx.changed().await`. Tokio's `watch::changed()` does **not** re-deliver the already-current
  value, so the pre-check — not watch replay — is what prevents a lost completion.
- **`WeakAssetRef`** prevents the parent→child strong reference from extending asset
  lifetimes or forming an `Arc` cycle; the child is kept alive by the queue/registry while
  relevant and by the hook task's own `AssetRef` clone until the hook completes.
- **Idempotent resubmission:** `JobQueue::submit`'s existing duplicate guard (lines
  3516-3520) plus the generation check make "resume at most once per wait generation"
  (WP-1 Phase 2D) hold even if two hooks race.
- **No lock across `.await` on the queue mutex:** the hook inspects child status and calls
  `submit` without holding the `JobQueue::jobs` mutex, matching the existing rule at
  `assets.rs:3618-3633`.

## Compilation Validation

Mental `cargo check` outcome: compiles, pending method bodies (Phase 4). The following were
flagged as verification items and have now been **confirmed against the real code**:

- [x] `AssetRef<E>` = `{ id: u64, data: Arc<RwLock<AssetData<E>>> }` and `WeakAssetRef<E>` =
      `{ id: u64, data: Weak<RwLock<AssetData<E>>> }` **already exist** (`assets.rs:714-723`),
      with `downgrade` (~912) and `upgrade` (~2181). Reuse them; add no new type.
- [x] `Status::is_finished()` (`metadata.rs:372`) already returns `true` for `Error` and
      `Cancelled`. Use it directly; no `is_terminal()` helper.
- [x] `DefaultAssetManager` holds `job_queue: Arc<JobQueue<E>>` (private, module-scoped);
      reachable as `self.job_queue.submit(...)` (existing usage at `assets.rs:2913/2961/3012`).
      There is **no** `submit` on the manager or the `AssetManager` trait.
- [x] `AssetRef::subscribe_to_notifications()` is **async** (`assets.rs:1692`) and returns
      `watch::Receiver<AssetNotificationMessage>`; must be `.await`ed. `changed()` does not
      replay the current value, so the hook pre-checks `child.status()` first.
- [x] Only `AssetData::new_ext` (~302) is a struct-literal constructor (`new`/`new_temporary`
      delegate to it), so it is the sole place to initialize `waiting_on`/`wait_generation`.

No `unwrap()`/`expect()` in any new signature; all fallible paths return `Result`.

## References to liquers-patterns.md

- [x] Crate dependency flow respected — change is confined to `liquers-core`.
- [x] No new `ExtValue` variants (feature is not a value type).
- [x] Commands unaffected — no `register_command!` changes.
- [x] AsyncStore pattern untouched — no store changes.
- [x] Error handling uses typed constructors (`fail_due_to_dependency`,
      `Error::dependency_cycle`, `Error::general_error`), never `Error::new`.
- [x] Async is default; reuses the existing tokio runtime and `Notify`-driven queue.
- [x] No default match arms on `EvaluationOutcome` (both variants explicit).
- [x] `AssetManager` trait left intact (Python bindings safe).
