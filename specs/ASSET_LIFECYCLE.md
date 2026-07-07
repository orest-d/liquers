# Asset Lifecycle — Comprehensive Map

## Overview

This document maps all entry points, execution paths, status transitions, expiration handling, and
dependency tracking for assets in Liquers. It is intended to:

1. Provide a definitive reference for how assets are created and evaluated
2. Identify inconsistencies in expiration and dependency handling
3. Serve as a basis for potential refactoring — identifying code duplication and responsibility
   boundaries between `Context` and `AssetRef`/`assets.rs`

---

## 1. Asset Identity and Classification

An asset can be classified along three independent axes:

| Axis | Option A | Option B |
|------|----------|----------|
| **Initial state** | **Evaluate** — starts from empty state (`State::new()`) | **Apply** — caller supplies an initial input state |
| **Recipe source** | **Key asset** — recipe loaded from `RecipeProvider` by key; stored in `assets` map; fast-track possible | **Query/Ad-hoc** — recipe derived from query or passed directly; not in `assets` map; no fast-track |
| **Scheduling** | **Queued** — submitted to `JobQueue`; runs when capacity available | **Immediate** — `run_immediately()` called directly, bypasses queue |

Key observations:
- The **evaluate** convention (empty initial state) corresponds to `State::new()` passed as input.
- The **apply** convention (provided initial state) corresponds to a caller-supplied `State<V>`.
- `EnvRef::evaluate_immediately()` is named "evaluate" from the user's perspective, but internally
  calls `apply_immediately(query.into(), State::new(), …)` — i.e., it uses the apply path with an
  empty state. This is a naming inconsistency.

---

## 2. High-Level Entry Points

### Table: Public Entry Points

| Function | File | Description | Initial State | Recipe Source | Scheduling | Primary Caller |
|----------|------|-------------|---------------|---------------|------------|----------------|
| `EnvRef::evaluate(query)` | `context.rs:115` | Standard evaluation of a query | Empty (`State::new()`, internal) | Key or Query | **Queued** | HTTP handler, tests, runner |
| `EnvRef::evaluate_immediately(query, payload)` | `context.rs:131` | Ad-hoc evaluation with payload (e.g. for UI commands) | Empty (`State::new()`) | Key or Query (ad-hoc) | **Immediate** | UI runner, `AppRunner::evaluate_immediately` |
| `Context::evaluate(query)` | `context.rs:198` | Nested evaluation inside a command; records dependency | Empty (internal) | Key or Query | **Queued** | Commands during `apply_plan` |
| `Context::apply(query, state)` | `context.rs:250` | Apply query to a provided state inside a command | **Provided** | Key or Query (ad-hoc) | **Queued** | Commands |

### Table: AssetManager-Level Entry Points

| Function | File | Description | Initial State | Recipe Source | Scheduling |
|----------|------|-------------|---------------|---------------|------------|
| `AssetManager::get_asset(query)` | `assets.rs:2765` | Resolves query to key or query asset | Empty (internal) | Key (→ `get`) or Query | Queued |
| `AssetManager::get(key)` | `assets.rs:2862` | Get or create a key-based resource asset | Empty (internal) | Key → RecipeProvider | Queued |
| `AssetManager::apply(recipe, state)` | `assets.rs:2831` | Create ad-hoc asset from recipe + state | **Provided** | Recipe arg (any) | **Queued** |
| `AssetManager::apply_immediately(recipe, state, payload)` | `assets.rs:2846` | Create ad-hoc asset, run immediately | **Provided** | Recipe arg (any) | **Immediate** |

### Table: AssetRef-Level Execution

| Function | File | Description | Used By |
|----------|------|-------------|---------|
| `AssetRef::run()` | `assets.rs:1271` | Full queued execution path | `JobQueue` |
| `AssetRef::run_immediately(payload)` | `assets.rs:1276` | Immediate execution with payload | `apply_immediately` |
| `AssetRef::run_with_future(future)` | `assets.rs:1246` | Common harness: spawns `process_service_messages`, runs evaluation, finalizes | `run`, `run_immediately` |
| `AssetRef::evaluate_and_store()` | `assets.rs:1360` | Evaluate recipe; set data; persist; register in DM | `run` (via `run_with_future`) |
| `AssetRef::evaluate_immediately(payload)` | `assets.rs:1468` | Evaluate recipe with payload; set data only | `run_immediately` (via `run_with_future`) |
| `AssetRef::evaluate_recipe()` | `assets.rs:1290` | Core recipe execution: get recipe, create context, call `apply_recipe`, collect deps | `evaluate_and_store` |
| `AssetRef::try_fast_track()` | `assets.rs:449` | Load from store without queuing (key assets only) | `get`, `get_asset` |
| `AssetRef::try_to_set_ready()` | `assets.rs:830` | Finalize status to Ready or Volatile after run | `finish_run_with_result` |
| `AssetRef::submitted()` | `assets.rs:987` | Set status=Submitted; send JobSubmitted service msg | `JobQueue::submit` |
| `AssetRef::process_service_messages()` | `assets.rs:1004` | Service loop: handles Cancel, progress, logs | `run_with_future` (spawned) |

---

## 3. Detailed Execution Paths

### Path A: Key Asset, Queued (Standard Evaluation)

Entry: `EnvRef::evaluate(query)` → `asset_manager.get_asset(query)` → `get(key)` (when query has key)

```
1. get(key) called
   └─ get_resource_asset(key)
      ├─ is_volatile(key)? → get_volatile_resource_asset (new AssetRef each call; is_volatile=true)
      └─ else → get_nonvolatile_resource_asset (entry_async: reuse or create; is_volatile=false)

2. AssetRef created with status=None
   ├─ recipe = key.into()  (Recipe::from_key)
   ├─ initial_state = State::new()  (empty)
   ├─ metadata created from AssetInfo (empty, status=None)
   └─ is_volatile = false (until resolve_volatility_before_evaluation)

3. Check status: if is_finished() → return early (cached)
   Check status: if Expired → remove from map, retry loop

4. try_fast_track() attempted:
   ├─ Checks is_resource() → true for key assets with no initial value
   ├─ store.contains(key)?
   │   NO → fast-track fails → continue to queue
   │   YES:
   │     ├─ store.get(key) → loads (binary, metadata)
   │     ├─ Checks stored_status ∈ {Ready, Source, Override}
   │     │   else: clear_fast_track_payload, return false
   │     ├─ Deserializes value from binary
   │     ├─ Validates stored dependency versions against DM
   │     │   STALE: clear_fast_track_payload, return false
   │     ├─ Registers dependency versions in DM
   │     ├─ Sets: data=Some(value), binary=Some(..), status=stored_status, metadata=loaded
   │     ├─ Sends notification: JobFinished
   │     └─ Returns status=Ready/Source/Override ← FAST TRACK COMPLETE

5. job_queue.submit(assetref) [if fast-track failed]:
   ├─ If already queued: skip (duplicate guard)
   ├─ If capacity available:
   │   ├─ compare_exchange: increment running_count
   │   ├─ set_status(Processing) directly
   │   └─ tokio::spawn(asset_clone.run())
   └─ Else (at capacity):
       ├─ submitted() → status=Submitted, service msg JobSubmitted sent
       └─ JobQueue.run() loop picks up when slot opens:
           ├─ status → Processing (set directly)
           └─ tokio::spawn(asset.run())

6. run() called:
   └─ run_with_future(evaluate_and_store())
       ├─ resolve_volatility_before_evaluation()
       │   └─ Sets is_volatile = is_volatile || recipe.volatile || recipe.expires.is_volatile()
       │      If volatile: metadata.set_volatile()
       ├─ tokio::spawn(process_service_messages())  [service loop, separate task]
       └─ tokio::select!:
           ├─ wait_to_finish()  [waits for notifications]
           └─ evaluate_and_store()  [primary path]

7. evaluate_and_store():
   ├─ resolve_volatility_before_evaluation()  [called again, idempotent]
   ├─ evaluate_recipe():
   │   ├─ resolve_volatility_before_evaluation()  [called again, idempotent]
   │   ├─ initial_state_and_recipe() → (State::new(), recipe)
   │   ├─ If recipe has key: manager.get(key) to verify it's the same asset
   │   │   └─ recipe_provider.recipe(key) → loads actual Recipe from provider
   │   ├─ create_context() → Context { assetref, envref, is_volatile, pending_dependencies: [] }
   │   ├─ envref.apply_recipe(input_state, recipe, context):
   │   │   ├─ recipe.to_plan(cmr) → Plan (synchronous)
   │   │   ├─ finalize_plan(envref, plan, context):
   │   │   │   ├─ has_volatile_dependencies() → may set plan.is_volatile
   │   │   │   ├─ has_expirable_dependencies() → may update plan.expires
   │   │   │   ├─ Seeds context.pending_dependencies with plan deps at Version(0)
   │   │   │   └─ register_plan_dependencies() → registers edges in DM
   │   │   ├─ combined_expires = plan.expires | recipe.expires
   │   │   ├─ context.set_expires(combined_expires):  ★ EXPIRATION HINT (writes to metadata)
   │   │   │   ├─ metadata.set_expiration_time_from(&expires)
   │   │   │   └─ assetref.set_expiration_time(expiration_time)
   │   │   └─ apply_plan(plan, input_state, context, envref):
   │   │       └─ For each step: do_step() → commands execute
   │   │          └─ context.evaluate(dep_query) per dependency:
   │   │              ├─ cycle detection
   │   │              ├─ manager.get_asset(dep_query) [recursive!]
   │   │              ├─ dm.add_dependent_asset(dep_key, self.assetref.downgrade())
   │   │              └─ pending_dependencies.push(DependencyRecord{key, version})
   │   ├─ observed_deps = context.take_pending_dependencies()
   │   ├─ Updates metadata: type_identifier, type_name
   │   ├─ Adds all observed_deps to metadata.dependencies
   │   └─ Returns State { data: Arc<V>, metadata }
   │
   ├─ On Ok(State { data, metadata }):
   │   ├─ [write lock] lock.data = Some(data); lock.metadata = metadata (with type info)
   │   ├─ try_to_set_ready():  ★ SINGLE AUTHORITY for status + expiration (FIXED: Issue 1)
   │   │   ├─ should_be_volatile = is_volatile || metadata.expires().is_volatile()
   │   │   ├─ If volatile:
   │   │   │   ├─ is_volatile = true; status = Volatile
   │   │   │   ├─ metadata.set_volatile()
   │   │   │   └─ expiration_time = metadata.expiration_time()
   │   │   └─ If not volatile:
   │   │       ├─ status = Ready; metadata.set_status(Ready)
   │   │       ├─ metadata.set_expiration_time_from(&metadata_expires)
   │   │       └─ expiration_time = metadata.expiration_time()
   │   ├─ [read lock] Sends notification: ValueProduced
   │   ├─ persist_with_status_tracking():
   │   │   └─ save_to_store() → serializes (poll_state works: status is Ready/Volatile)
   │   │      → store.set(key, data, metadata)
   │   └─ dm.track_asset(self) → registers in DM (version tracking)
   │       └─ expire_dependencies_result() if any cascade
   │
   └─ On Err(e):
       ├─ data = None, binary = None
       ├─ status = Status::Error
       ├─ metadata.with_error(e)
       └─ Sends notification: ErrorOccurred(e)

8. finish_run_with_result() [after evaluate_and_store and psm complete]:
   ├─ If error from evaluate_and_store: set status=Error
   ├─ If Ok: check status ∈ {None, Recipe, Submitted, Dependencies, Processing, Partial}:
   │   └─ try_to_set_ready():  ★ EXPIRATION SET #3 (same logic as step 7, duplicate)
   │       ├─ If data present + not volatile: status=Ready, expiration_time = metadata.expiration_time()
   │       ├─ If data present + volatile: status=Volatile, expiration_time=Immediately then metadata
   │       └─ Else: status=Error
   ├─ Schedule expiration if expiration_time is ExpirationTime::At(_):
   │   └─ assetref.schedule_expiration(&exp_time)
   └─ Sends: service msg JobFinished, notification JobFinished
```

---

### Path B: Non-Key Query Asset, Queued

Entry: `EnvRef::evaluate(query)` → `asset_manager.get_asset(query)` [query has no key]

```
1. get_asset(query) → get_query_asset(query):
   ├─ is_volatile(query)? → get_volatile_query_asset (new AssetRef each time, is_volatile=true)
   └─ else → get_nonvolatile_query_asset (entry_async: reuse or create, stored in query_assets)

2. AssetRef created: status=None, recipe=query.into(), initial_state=State::new()

3. Check if status is finished → return early if cached
   Check if Expired → remove from query_assets, retry

4. try_fast_track(): always returns false (is_resource() = false for non-key query)

5. job_queue.submit(assetref) → same flow as Path A from step 5 onward

6-8. Identical to Path A steps 6-8, except:
   - recipe_provider lookup uses the query, not a key
   - Result is NOT stored in `assets` map (only in `query_assets`)
   - After evaluation, stored to store only if recipe has a store_to_key
```

---

### Path C: Apply (Ad-hoc, Provided State, Queued)

Entry: `Context::apply(query, state)` → `asset_manager.apply(recipe, to)`

```
1. AssetData::new_ext(id, recipe, initial_state=to, envref):
   ├─ Status = None
   ├─ initial_state = provided State (NON-EMPTY)
   └─ Not stored in any manager map

2. job_queue.submit(asset_ref) → same queue flow as Path A step 5

3. run() → run_with_future(evaluate_and_store()):
   └─ evaluate_recipe():
       ├─ initial_state_and_recipe() → (provided_state, recipe)  ← key difference!
       └─ rest same as Path A step 7
```

---

### Path D: Apply Immediately (Ad-hoc, Provided State, Immediate, with Payload)

Entry: `EnvRef::evaluate_immediately(query, payload)` → `asset_manager.apply_immediately(recipe, State::new(), payload)`

OR: `AssetManager::apply_immediately(recipe, state, payload)` directly

```
1. AssetData::new_ext(id, recipe, initial_state, envref)
   ├─ initial_state = State::new() (from EnvRef::evaluate_immediately)
   │   OR provided state (from direct apply_immediately call)
   └─ Not stored in any manager map

2. asset_ref.run_immediately(payload):
   └─ run_with_future(evaluate_immediately(payload)):
       ├─ resolve_volatility_before_evaluation()
       ├─ initial_state_and_recipe() → (initial_state, recipe)
       ├─ create_context() then context.set_payload(payload)
       ├─ envref.apply_recipe(input_state, recipe, context):
       │   ├─ finalize_plan, combined_expires, context.set_expires(...)  ★ EXPIRATION SET #1
       │   └─ apply_plan → commands execute
       ├─ lock.data = Some(res)
       ├─ Sends notification: ValueProduced
       └─ [NO status update, NO persist, NO DM registration]  ← key difference from evaluate_and_store!

3. finish_run_with_result() [same as Path A step 8]:
   ├─ try_to_set_ready()  ★ EXPIRATION SET #3 (but #2 never ran for this path!)
   └─ schedule_expiration, JobFinished notifications
```

> **Note**: `evaluate_immediately` does NOT call `evaluate_and_store()` — it uses a lighter path
> that skips persistence and DM registration. Status finalization and expiration scheduling still
> happen via `finish_run_with_result()` → `try_to_set_ready()`.

---

## 4. Status Transition Timeline

### Table: Status Timeline (Queued Key-Asset Path)

| Phase | Active Function | Status Before | Status After | What Happens |
|-------|-----------------|---------------|--------------|--------------|
| **Creation** | `get_nonvolatile_resource_asset` | — | `None` | AssetRef created; empty metadata; recipe=key; no data |
| **Fast Track: Store Hit** | `try_fast_track` | `None` | `Ready` / `Source` / `Override` | Data and metadata read from store; dependency versions validated; DM updated; `JobFinished` notification sent |
| **Fast Track: Store Miss** | `try_fast_track` | `None` | `None` | Store check fails; asset proceeds to queue |
| **Queue Submission (capacity available)** | `JobQueue::submit` | `None` | `Processing` | Slot reserved; `tokio::spawn(run())` immediately |
| **Queue Submission (at capacity)** | `JobQueue::submit` → `submitted()` | `None` | `Submitted` | `service_tx.send(JobSubmitted)`; waiting in queue |
| **Queue Pick-Up** | `JobQueue::run` | `Submitted` | `Processing` | Slot available; `tokio::spawn(run())` |
| **Resolve Volatility** | `resolve_volatility_before_evaluation` | `Processing` | `Processing` | Checks `recipe.volatile` and `recipe.expires.is_volatile()`; sets `is_volatile`; marks metadata volatile |
| **Plan Building** | `evaluate_recipe` → `recipe.to_plan` | `Processing` | `Processing` | Plan built from recipe; command metadata resolved |
| **Dependency Finalization** | `finalize_plan` | `Processing` | `Processing` | Volatile/expirable dependencies checked; `pending_dependencies` seeded; DM edges registered |
| **Expiration Set (Phase 1)** | `context.set_expires` (in `apply_recipe`) | `Processing` | `Processing` | `expires` resolved from plan + recipe; written to metadata and `AssetData.expiration_time` |
| **Command Execution** | `apply_plan` → `do_step` | `Processing` | `Processing` | Commands execute; `context.evaluate(dep)` adds runtime dependencies to `pending_dependencies` |
| **Dependencies** | _(intended, not reliably set)_ | `Processing` | `Dependencies` | Would be set when awaiting sub-evaluations; currently `Dependencies` status is rarely set |
| **Dependency Collection** | `evaluate_recipe` (after apply_plan) | `Processing` | `Processing` | `context.take_pending_dependencies()` → written to metadata |
| **Data Set** | `evaluate_and_store` (write lock) | `Processing` | `Processing` | `lock.data` and `lock.metadata` populated from `evaluate_recipe` result |
| **Status + Expiration Finalized** | `try_to_set_ready` (called from `evaluate_and_store`) | `Processing` | `Ready` / `Volatile` | **Single authority**: reads `metadata.expires()`, sets status and `expiration_time`; no duplicate logic *(FIXED: Issues 1 & 2)* |
| **Value Produced** | `evaluate_and_store` (read lock) | `Ready` / `Volatile` | `Ready` / `Volatile` | `ValueProduced` notification sent after status is finalized |
| **Persistence** | `save_to_store` | `Ready` / `Volatile` | `Ready` / `Volatile` | `poll_state()` returns Some (status is Ready/Volatile); binary serialized; `store.set(key, data, metadata)` |
| **DM Registration** | `dm.track_asset` | `Ready` | `Ready` | Version registered in DependencyManager; cascade expiration if needed |
| **Status Finalize (skip)** | `finish_run_with_result` | `Ready` / `Volatile` | `Ready` / `Volatile` | Status already finalized; `try_to_set_ready` is skipped for `Ready`/`Volatile` states |
| **Expiration Scheduling** | `schedule_expiration` | `Ready` | `Ready` | Sends `Track` message to expiration monitor if `ExpirationTime::At(_)` |
| **Completion** | `finish_run_with_result` | `Ready` | `Ready` | `JobFinished` service msg; `JobFinished` notification |

### Table: Status Timeline (Immediate Evaluation Path)

| Phase | Active Function | Status Before | Status After | What Happens |
|-------|-----------------|---------------|--------------|--------------|
| **Creation** | `new_ext` | — | `None` | AssetRef created; not in any manager map |
| **Volatility Resolve** | `resolve_volatility_before_evaluation` | `None` | `None` | Same as queued path |
| **Plan + Expiration** | `apply_recipe` → `context.set_expires` | `None` | `None` | **Only** expiration-setting step for this path |
| **Command Execution** | `apply_plan` | `None` | `None` | Commands execute with payload available |
| **Value Set** | `evaluate_immediately` | `None` | `None` | `lock.data = Some(res)`; `ValueProduced` notification; **no status change, no persist** |
| **Status Finalize** | `try_to_set_ready` | `None` | `Ready` / `Volatile` | Checks data, sets Ready or Volatile |
| **Expiration Set** | `try_to_set_ready` | `Ready` | `Ready` | Sets `expiration_time` from metadata — this is the ONLY expiration-setting step that writes to `expiration_time` in this path |
| **Completion** | `finish_run_with_result` | `Ready` | `Ready` | `JobFinished` notifications |

---

## 5. Expiration and Volatility

### Expiration-Setting Points (after fixes)

Expiration time is written to `AssetData.expiration_time` in exactly **two places**:

```
#1: context.set_expires()    — called from apply_recipe() in SimpleEnvironment::apply_recipe
    ↳ Writes expiration hint: metadata.set_expiration_time_from(&expires) + assetref.set_expiration_time(...)
    ↳ Source: plan.expires | recipe.expires
    ↳ Note: this is an intermediate write; try_to_set_ready() is the authoritative final step

#2: try_to_set_ready()       — called from evaluate_and_store() [queued path]
                                 or from finish_run_with_result() [immediate path, edge cases]
    ↳ Re-reads metadata.expires() and sets expiration_time = metadata.expiration_time()
    ↳ This is the SINGLE AUTHORITY for final expiration and status
```

For the **queued path** (`run` → `evaluate_and_store`):
- `#1` runs during recipe execution (sets expiration in metadata)
- `#2` runs from `evaluate_and_store` → `try_to_set_ready()` (reads back from metadata)
- `finish_run_with_result` sees status=Ready/Volatile and skips `try_to_set_ready`

For the **immediate path** (`run_immediately` → `evaluate_immediately`):
- `#1` runs during recipe execution (sets expiration in metadata)
- `#2` runs from `finish_run_with_result` → `try_to_set_ready()` (reads back from metadata)

> **Previously** (before fix): the queued path had three separate expiration-setting points —
> `context.set_expires()`, an inline match block in `evaluate_and_store()`, and `try_to_set_ready()`
> called in edge cases. The inline block was removed; `evaluate_and_store()` now calls
> `try_to_set_ready()` directly. *(Fixed: Issue 1)*

### Expiration Not Handled for Custom Environments

`context.set_expires()` is only called if the `Environment::apply_recipe` implementation calls it.
The `SimpleEnvironment` and `SimpleEnvironmentWithPayload` implementations do call it, but a custom
environment that forgets to call `context.set_expires()` will silently lose expiration information —
the expiration would still be reconstructed in `try_to_set_ready()` from `metadata.expires()`, but
only if the metadata was correctly set via `context.set_expires()` first.

### Dependency Expiration Path

```
dm.track_asset(self)             — after evaluate_and_store success
dm.register_version(&dep_key, version)  — during try_fast_track (store load)
dm.load_from_records(&dep_key, deps)    — during try_fast_track (dependency check)
dm.add_dependent_asset(dep_key, weak)   — during Context::evaluate()
register_plan_dependencies(key, deps)   — in finalize_plan()
```

The dependency tracking has two overlapping paths:
- **Plan deps** (known before execution): registered in `finalize_plan` via `register_plan_dependencies`
- **Runtime deps** (observed during execution): accumulated in `context.pending_dependencies` via `Context::evaluate()`, then written to metadata in `evaluate_recipe()`

Both are registered into the DM, but at different times and via different mechanisms.

### Cascade Expiration

When a dependency changes:
```
asset.expire()
  └─ mark_expired_status()  → status: Ready/Override → Expired; sends Expired notification
  └─ cascade_expire_dependents(dep_key)
      └─ dm.expire(dep_key) → returns list of expired keys + WeakAssetRef list
      └─ expire_dependencies_result(expired):
          ├─ For keyed dependents: lookup in assets map → expire_without_cascade()
          └─ For untracked (query/ad-hoc): upgrade WeakAssetRef → expire_without_cascade()
```

`expire_without_cascade()` is used for cascade recipients to avoid infinite loops.

---

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

## 9. Cross-Reference

| Concept | Implementation | Spec |
|---------|---------------|------|
| Asset status enum | `liquers-core/src/metadata.rs`: `Status` | `specs/ASSETS.md` |
| AssetData / AssetRef | `liquers-core/src/assets.rs` | `specs/ASSETS.md` |
| JobQueue | `liquers-core/src/assets.rs`: `JobQueue` | `specs/JOBQUEUE_FIX.md` |
| Dependencies status | `liquers-core/src/assets.rs`: `Status::Dependencies` | `specs/DEPENDENCIES_STATUS.md` |
| Context | `liquers-core/src/context.rs` | — |
| Interpreter | `liquers-core/src/interpreter.rs` | — |
| Expiration types | `liquers-core/src/expiration.rs` | — |
| DependencyManager | `liquers-core/src/dependencies.rs` | — |
| RecipeProvider | `liquers-core/src/recipes.rs` | — |
