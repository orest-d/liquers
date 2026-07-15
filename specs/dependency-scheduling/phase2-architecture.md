# Phase 2: Solution & Architecture - dependency-scheduling

## Overview

Non-blocking dependency scheduling is implemented entirely in `liquers-core` as three
cooperating pieces: (1) an **atomic run claim** on `AssetRef` that makes "who executes
this asset" a single CAS decision shared by every execution path; (2) a `JobQueue`
refactor extracting `try_to_start_immediately` plus **queue-resident per-dependent
local queues** used as a capacity fallback; (3) an **internal schedule helper**
(`Context::schedule_dependency_asset`) that captures the dependency `AssetRef`
exactly once (volatile-safe), records dependency facts, and cycle-checks via the
existing `DependencyManager`, letting the interpreter pre-pass schedule all known
dependencies (storing their AssetRefs) before executing plan steps, then wait on
each via `Context::wait_for_dependency`. No new commands, value types, or endpoints.

## Data Structures

### `try_to_start_immediately` return convention (boolean, not an enum)

`JobQueue::try_to_start_immediately` returns `Result<bool, Error>`:

- `true` — **the asset is being taken care of**: either a capacity slot was
  reserved and `tokio::spawn(asset.run())` was issued, or the asset was already
  claimed/finished elsewhere (the CAS-reserved slot, if any, was released before
  returning). Callers do nothing.
- `false` — **no capacity** (`running_count` CAS failed against `capacity`): the
  caller must guarantee progress (park globally in `submit`, or enqueue on the
  parent's local dependency queue).

An earlier draft modelled this as a three-variant `StartOutcome`
(`Started`/`AlreadyActive`/`NoCapacity`), but every call site treats
`Started` and `AlreadyActive` identically and branches only on the no-capacity
case, so a boolean is the simpler, equally expressive surface. The
started-vs-already-active distinction (slot reserve/release, claim arbitration)
is fully internal to `try_to_start_immediately` and never observed by callers.

### New Structs

#### RunClaim

**Why needed.** Several independent paths may each try to *execute the same asset's
body* — the JobQueue worker, inline drains of a parent's local queue
(`drain_dependencies`), claim-recovery in `wait_for_dependency`, and the migrated
`evaluate_recipe` delegation. Today the only guard against a double run is
`run_with_future`'s `is_finished()` check (assets.rs:1373), which rejects an
already-*finished* asset but not one *currently* running — so two live paths can both
start a not-yet-finished asset (wasteful, and wrong for volatile/side-effecting
recipes). `RunClaim` closes this and one further gap:

1. **Execute-once arbitration** — `try_claim_for_run` makes one atomic status
   transition (not-yet-running → `Processing`) under a single write lock and hands the
   token only to the winner; `run()` is called *only* by a claim holder, so the body
   runs at most once.
2. **Cancellation liveness** — an inline drain may hold the claim inside a parent
   future; if that future is dropped, the dependency would be stranded in `Processing`
   with nobody driving it. `Drop` (while armed) re-parks + notifies so another runner
   recovers it. Both guarantees are lifetime-scoped ⇒ an RAII `Drop` guard, not a
   `bool`.

```rust
/// Proof that the holder is the unique runner of an asset, from an atomic status
/// transition (not-yet-running -> Processing) under one AssetData write lock.
/// Exists to give two guarantees a plain flag cannot:
/// - execute-once: run() is called only by a claim holder, closing the double-run
///   window left by run_with_future's is_finished()-only guard (assets.rs:1373);
/// - cancellation liveness: if the holder's future is dropped mid-run, Drop (while
///   armed) re-parks + notifies so another runner recovers the stranded asset.
pub(crate) struct RunClaim<E: Environment> {
    asset: AssetRef<E>,
    queue: Arc<JobQueue<E>>, // for Drop repair (re-park + notify)
    armed: bool,
}

impl<E: Environment + 'static> RunClaim<E> {
    /// Disarm after run() returned (Ok or Err); the asset's own terminal or
    /// error status is authoritative from then on.
    pub(crate) fn complete(self);
}

impl<E: Environment> Drop for RunClaim<E> { /* if armed: spawn repair task */ }
```
**Ownership rationale:** `asset` is a cheap `AssetRef` clone (Arc inside). `queue` is
`Arc` because the claim may outlive the borrow it was created from and the Drop repair
must reach the queue. Not `Serialize` — runtime-only.
**Drop repair semantics:** if the claim is dropped armed (the owning future was
cancelled mid-run, e.g. parent cancellation dropping an inline drain), a spawned repair
task re-checks the asset: if still `Processing` and not finished, it resets the status
to `Submitted` and calls `queue.submit(asset)` (global parking) + `notify_one()`, so
other waiters recover. Explicit status match, no default arm.

#### JobQueue (modified, assets.rs:3483)
```rust
pub struct JobQueue<E: Environment> {
    jobs: Arc<Mutex<Vec<AssetRef<E>>>>,          // unchanged: global parked/running list
    running_count: Arc<AtomicUsize>,             // unchanged
    notify: Arc<Notify>,                         // unchanged
    shutdown: Arc<AtomicBool>,                   // unchanged
    capacity: usize,                             // unchanged
    /// NEW: per-dependent local dependency queues — an implementation detail of
    /// the queue mechanism (Phase 1 decision). Keyed by the DEPENDENT asset id.
    /// Entries are created lazily on the no-capacity fallback and removed when the
    /// dependent drains them or finishes: zero per-asset cost otherwise.
    local_deps: Arc<Mutex<HashMap<u64, VecDeque<AssetRef<E>>>>>,
}
```
**Ownership rationale:** `tokio::sync::Mutex<HashMap<..>>` (matching the existing
`jobs` mutex style) rather than `scc`: the map is touched only on the fallback path
(rare by design) and drains pop one entry at a time; the lock is never held across
`.await` of asset execution. `VecDeque` preserves scheduling order (FIFO drain).

#### PlanDependencySchedule

**Why needed.** This is the hand-off table that makes evaluation non-blocking. Today
`do_step` resolves and immediately blocks on each dependency in turn
(`get_asset(q).await?.get().await?`, interpreter.rs:171-172/194-197/245-305),
serializing independent dependencies. The new design runs a pre-pass that schedules
*all* known dependencies up front (starting each if the queue has capacity, else
enqueuing it) and lets the step loop *wait* only when a step needs the result. That
split means a step must later wait on the *exact* AssetRef the pre-pass scheduled —
decisive for volatile queries, which return a fresh AssetRef on every resolution:
re-resolving would orphan the started work and run it twice. The map captures
`Query → scheduled AssetRef` for one `apply_plan`, so waits reuse the started asset and
each dependency is scheduled once per parent evaluation.

```rust
/// Schedule->wait hand-off for one apply_plan: Query -> the AssetRef the pre-pass
/// scheduled (liquers-core/src/interpreter.rs). Makes evaluation non-blocking
/// (schedule all known deps up front, wait later) while preserving execute-once:
/// the step loop waits on the SAME AssetRef, never re-resolving. The map is the
/// volatility anchor — a volatile query yields a FRESH AssetRef on each resolution,
/// so the stored one MUST be reused; dedup by Query means each dependency is
/// scheduled once per parent evaluation. Subsumes the earlier DependencyHandle:
/// parent/manager come from the waiter's Context, so only the AssetRef is stored.
pub(crate) struct PlanDependencySchedule<E: Environment> {
    assets: HashMap<Query, AssetRef<E>>,
}
```
`Query` already implements `Hash + Eq` (it is used as the key of the manager's
`query_assets` map).

### Modified Structs

- `AssetData<E>`: **unchanged** (Phase 1 decision: no per-asset queue storage).
- `Status` (metadata.rs:282): **unchanged** — `Status::Dependencies` remains the sole
  waiting status (WP-1 principle; no new variant).

## Trait Implementations

### Trait: AssetManager<E> (extended, assets.rs:2189)

Three new methods **with default implementations** (CLAUDE.md: extend traits with
defaults so existing implementors — including exotic/test managers — keep compiling
and stay deadlock-free-by-fallback). `#[async_trait]` as the rest of the trait.

```rust
#[async_trait]
pub trait AssetManager<E: Environment>: Send + Sync {
    // ... existing methods unchanged ...

    /// Resolve the asset for `query` and schedule it as a dependency of `parent`:
    /// start it immediately if queue capacity allows, otherwise enqueue it on
    /// `parent`'s local dependency queue (local-only parking — the dependency is
    /// NOT parked in the global jobs list on this path).
    /// Does NOT record dependency facts or cycle-check — Context does that.
    ///
    /// Default: plain `get_asset` (global submit) — safe fallback for managers
    /// without a JobQueue; DefaultAssetManager overrides.
    async fn get_dependency_asset(
        &self,
        parent: &AssetRef<E>,
        query: &Query,
    ) -> Result<AssetRef<E>, Error> {
        let _ = parent;
        self.get_asset(query).await
    }

    /// Drain `parent`'s local dependency queue: claim and inline-run each
    /// still-runnable entry sequentially inside the caller's future.
    /// Default: no-op (managers without local queues have nothing to drain).
    async fn drain_dependencies(&self, parent: &AssetRef<E>) -> Result<(), Error> {
        let _ = parent;
        Ok(())
    }

    /// Claim-aware wait for `dependency` on behalf of `parent`: guarantees
    /// progress (drain + direct claim before blocking), maintains
    /// Status::Dependencies on the parent, propagates dependency failure.
    /// Default: enter_dependencies + dependency.get().await +
    /// leave_dependencies_and_resume (correct, but without the local-queue
    /// progress guarantee — fine for managers whose get_asset submits globally).
    async fn wait_for_dependency(
        &self,
        parent: &AssetRef<E>,
        dependency: &AssetRef<E>,
    ) -> Result<State<E::Value>, Error>;
}
```

**Implementor:** `DefaultAssetManager<E>` overrides all three against its `JobQueue`
(signatures in Function Signatures below). **Bounds:** unchanged trait bounds; no new
generic parameters.

No other trait implementations change.

## Generic Parameters & Bounds

Everything is generic over the existing `E: Environment` exactly as the surrounding
code (`AssetRef<E>`, `JobQueue<E>`, `Context<E>`). `RunClaim<E>` requires
`E: Environment + 'static` only where it spawns the repair task (the same bound the
existing spawn sites use). No new bounds introduced.

## Sync vs Async Decisions

| Function | Async? | Rationale |
|----------|--------|-----------|
| `JobQueue::try_to_start_immediately` | Yes | takes async `jobs` mutex, async status claim |
| `JobQueue::submit` | Yes | unchanged public signature |
| `AssetRef::try_claim_for_run` | Yes | takes the AssetData RwLock write lock |
| `AssetRef::leave_dependencies_and_resume` | Yes | status + notification updates |
| `AssetManager::get_dependency_asset` | Yes | resolves assets (store/recipe I/O) |
| `AssetManager::drain_dependencies` | Yes | runs dependency evaluations inline |
| `AssetManager::wait_for_dependency` | Yes | awaits notifications |
| `Context::schedule_dependency_asset` | Yes | asset resolution + DependencyManager calls |
| `Context::evaluate_local_queue` | Yes | delegates to drain_dependencies |
| `Context::wait_for_dependency` | Yes | delegates to `AssetManager::wait_for_dependency` |
| `RunClaim::complete` | No | flips a bool; Drop must be sync anyway |

Async is the default throughout (CLAUDE.md); no sync wrappers are needed — Python
bindings consume the unchanged `EnvRef::evaluate` surface.

## Function Signatures

### Module: liquers_core::assets — JobQueue

```rust
impl<E: Environment + 'static> JobQueue<E> {
    /// Try to start `asset` on a reserved capacity slot right now.
    /// (1) dedup-register in `jobs` (as submit does today, assets.rs:3514-3523);
    /// (2) CAS-reserve a slot on running_count;
    /// (3) atomically claim via asset.try_claim_for_run();
    /// (4) tokio::spawn: run, complete claim, decrement, notify,
    ///     then manager-side local-queue cleanup for this asset id (see Cleanup).
    /// Returns `true` when the asset is being taken care of (slot reserved and
    /// spawned, or already claimed/finished elsewhere), `false` on no capacity
    /// (the caller must then guarantee progress).
    pub(crate) async fn try_to_start_immediately(
        &self,
        asset: &AssetRef<E>,
    ) -> Result<bool, Error>;

    /// Public semantics unchanged (assets.rs:3510). Reimplemented:
    /// if !try_to_start_immediately(&asset).await? {   // no capacity
    ///     asset.submitted().await?; notify_one();
    /// }
    /// Ok(())
    pub async fn submit(&self, asset: AssetRef<E>) -> Result<(), Error>;

    /// Push `dep` onto the local queue of dependent `parent_id`
    /// (lazily creates the entry; dedup by asset id within that queue).
    pub(crate) async fn push_local_dependency(&self, parent_id: u64, dep: &AssetRef<E>);

    /// Pop the next local dependency of `parent_id`, if any
    /// (removes the map entry when the queue becomes empty).
    pub(crate) async fn pop_local_dependency(&self, parent_id: u64) -> Option<AssetRef<E>>;

    /// Remove and return all remaining local dependencies of `parent_id`
    /// (used by terminal cleanup).
    pub(crate) async fn take_local_dependencies(&self, parent_id: u64) -> Vec<AssetRef<E>>;
}
```

The worker loop `run()` (assets.rs:3607) is reimplemented on the same primitives:
each `Submitted` candidate goes through `try_to_start_immediately`; a candidate
already claimed elsewhere (e.g. by an inline drain) makes `try_to_start_immediately`
return `true` without this worker spawning it, and its now-stale `jobs` entry is
removed on the next cleanup pass. This removes the existing TOCTOU between the status read (:3630) and
`set_status(Processing)` (:3653).

### Module: liquers_core::assets — AssetRef

```rust
impl<E: Environment> AssetRef<E> {
    /// Atomically claim the exclusive right to execute this asset's body.
    /// Under ONE data.write() lock: match status {
    ///   None | Recipe | Submitted | Dependencies => set Processing, Some(claim),
    ///   Processing | Partial | Storing | Directory | Ready | Error | Cancelled
    ///   | Expired | Source | Override | Volatile => None }
    /// Invariant: run()/run_immediately() are invoked only by claim holders
    /// (queue spawn paths, inline drains, delegation waits).
    pub(crate) async fn try_claim_for_run(
        &self,
        queue: &Arc<JobQueue<E>>,
    ) -> Result<Option<RunClaim<E>>, Error>;

    /// Leave Status::Dependencies and resume as Processing (Phase 1 decision:
    /// truthful status flow Processing -> Dependencies -> Processing).
    /// Counterpart of enter_dependencies (assets.rs:748);
    /// leave_dependencies_for_resubmit (assets.rs:767) remains for the genuine
    /// resubmission path.
    pub(crate) async fn leave_dependencies_and_resume(&self) -> Result<(), Error>;
}
```

### Module: liquers_core::assets — DefaultAssetManager overrides

```rust
#[async_trait]
impl<E: Environment + 'static> AssetManager<E> for DefaultAssetManager<E> {
    /// Resolve WITHOUT global submission, then schedule:
    /// 1. resolve the AssetRef: keyed -> the assets-map entry (as get(),
    ///    assets.rs:2983, minus the job_queue.submit call); query -> the
    ///    query_assets entry or a fresh volatile asset (as get_asset(),
    ///    assets.rs:2886, minus the submit); try_fast_track as today;
    /// 2. if poll_state() is Some or status is finished: return;
    /// 3. if !job_queue.try_to_start_immediately(&asset).await? {  // no capacity
    ///        asset.submitted().await?;  (Status::Submitted = "queued", here on a
    ///                  local queue, NOT in the global jobs list);
    ///        job_queue.push_local_dependency(parent.id(), &asset);
    ///    }  // else the asset was started or already active — return;
    async fn get_dependency_asset(&self, parent: &AssetRef<E>, query: &Query)
        -> Result<AssetRef<E>, Error>;

    /// Loop: pop_local_dependency(parent.id());
    ///   skip entries whose poll_state() is Some or whose status is finished;
    ///   match dep.try_claim_for_run(&self.job_queue) {
    ///     Some(claim) => { parent.enter_dependencies(&dep).await?;
    ///                      let r = Box::pin(dep.run()).await;   // inline, recursive
    ///                      claim.complete();
    ///                      /* Err(e): log on parent, continue — failure surfaces
    ///                         at wait time of whoever needs dep */ }
    ///     None => continue /* running elsewhere or finished meanwhile */ }
    /// After the loop: parent.leave_dependencies_and_resume() if it entered
    /// Dependencies. Recursion: dep.run() -> evaluate_recipe -> dep's own
    /// Context schedules dep's dependencies -> at capacity they land on DEP's
    /// local queue and dep's evaluation drains them inside this same task;
    /// Box::pin bounds the future type, depth is bounded by the dependency DAG
    /// (cycles rejected at schedule time).
    async fn drain_dependencies(&self, parent: &AssetRef<E>) -> Result<(), Error>;

    /// Loop:
    ///   1. if dependency.poll_state() is Some and status is data-bearing
    ///      (Ready | Volatile | Source | Override | Directory):
    ///      parent.leave_dependencies_and_resume(); return Ok(state);
    ///   2. if status is Error | Cancelled: extract the stored error from
    ///      metadata (or construct a dependency-failure error with the asset id
    ///      and query); parent.fail_due_to_dependency(e.clone()); return Err(e);
    ///      (deliberately NOT AssetRef::get(): its poll_state fabricates a
    ///      none-valued Some(State) for Error|Cancelled, assets.rs:604-612;
    ///      when WP-2's poll_outcome lands, steps 1-2 collapse into one match);
    ///   3. self.drain_dependencies(parent) — runs dep if it's on OUR queue;
    ///   4. dependency.try_claim_for_run(): Some(claim) => enter_dependencies,
    ///      Box::pin(dependency.run()).await, complete, loop  (recovers deps
    ///      re-parked by a cancelled claim holder);
    ///   5. None => parent.enter_dependencies(&dependency);
    ///      rx = dependency.subscribe_to_notifications();
    ///      rx.changed().await; loop  (authoritative-state-first re-check,
    ///      the same pattern as AssetRef::get, assets.rs:1941-1999).
    async fn wait_for_dependency(&self, parent: &AssetRef<E>, dependency: &AssetRef<E>)
        -> Result<State<E::Value>, Error>;
}
```

### Module: liquers_core::context — Context

```rust
impl<E: Environment> Context<E> {
    /// Internal helper: schedule a dependency of the current asset without
    /// waiting for it, returning the captured child AssetRef. NOT public — the
    /// only callers are `evaluate`, `get_dependency_state`, and the interpreter
    /// pre-pass; there is no command-facing schedule-then-wait API.
    /// 1. dependent = ScheduleNode: Keyed(key) if the current asset is keyed,
    ///    else Expression(query dep key) (the key-or-query pattern of
    ///    record_dependency_on_asset, assets.rs:796-805);
    /// 2. dependency = ScheduleNode: Keyed(key dep key) if `query` is a pure
    ///    key, else Expression(query dep key);
    /// 3. dm.register_scheduled_dependency(&dependent, &dependency, version)
    ///    AT SCHEDULE TIME — performs ALL cycle checks under the expansion
    ///    principle (see Cycle Handling) and registers keyed edges / expression
    ///    attribution; Err(Error::dependency_cycle) aborts the schedule;
    /// 4. asset = manager.get_dependency_asset(self.get_asset_ref(), query)
    ///    — captures the AssetRef exactly ONCE (volatile-safe); the caller stores
    ///    this AssetRef (in PlanDependencySchedule or a local) and reuses it for
    ///    the later wait;
    /// 5. record DependencyRecord in pending_dependencies (upsert, version-
    ///    preference rules as today, context.rs:376-388) and
    ///    add_dependent_asset(dep_key, weak self) as today (context.rs:237-242);
    /// 6. return the captured AssetRef.
    /// Does NOT enter Status::Dependencies (that happens at drain/wait time,
    /// removing today's status flicker for already-ready dependencies).
    pub(crate) async fn schedule_dependency_asset(&self, query: &Query)
        -> Result<AssetRef<E>, Error>;

    /// Wait on a previously-scheduled dependency AssetRef on behalf of the
    /// current asset. Thin wrapper:
    /// = manager.wait_for_dependency(&self.get_asset_ref(), asset).
    /// Idempotent: a second call re-reads the asset-held result.
    pub(crate) async fn wait_for_dependency(&self, asset: &AssetRef<E>)
        -> Result<State<E::Value>, Error>;

    /// Drain the current asset's local dependency queue
    /// (= manager.drain_dependencies(current asset)).
    pub async fn evaluate_local_queue(&self) -> Result<(), Error>;

    /// Convenience: schedule + wait; returns the dependency state
    /// (WP-1 Phase 1C's Context::get_dependency_state). Reimplemented:
    /// let asset = self.schedule_dependency_asset(query).await?;
    /// self.wait_for_dependency(&asset).await
    pub async fn get_dependency_state(&self, query: &Query)
        -> Result<State<E::Value>, Error>;

    /// Backwards-compatible (public signature unchanged, context.rs:198).
    /// Reimplemented: let asset = self.schedule_dependency_asset(query).await?;
    /// self.evaluate_local_queue().await?;  // eager drain: the returned
    /// AssetRef is data-bearing, terminal, or claimed by a live runner, so
    /// callers may still `.get().await` it safely.
    /// Ok(asset)
    pub async fn evaluate(&self, query: &Query) -> Result<AssetRef<E>, Error>;
}
```

### Module: liquers_core::interpreter

```rust
/// Pre-pass: walk plan steps and schedule every known dependency, dedup by Query.
/// Scheduled: Step::GetAsset/GetAssetBinary/GetAssetMetadata (key -> query),
/// Step::Evaluate(q), and each param.link() query of Step::Action (mirrors
/// find_dependencies, plan.rs:1667; schedules the literal keys/queries the steps
/// execute; the WP-1 relative-key TODO stays visible). Step::Plan sub-plans are
/// NOT pre-scheduled — the recursive apply_plan performs its own pass.
pub(crate) async fn schedule_plan_dependencies<E: Environment>(
    plan: &Plan,
    context: &Context<E>,
) -> Result<PlanDependencySchedule<E>, Error>;

impl<E: Environment> PlanDependencySchedule<E> {
    /// Captured-AssetRef lookup used by do_step (None for dynamically-formed
    /// queries, which fall back to context.get_dependency_state).
    pub(crate) fn asset(&self, query: &Query) -> Option<&AssetRef<E>>;
}
```

`apply_plan` (interpreter.rs:82): after creating the context, call
`schedule_plan_dependencies` (which stores each scheduled dependency's AssetRef via
`context.schedule_dependency_asset`), then `context.evaluate_local_queue()` (one
inline drain so at-capacity dependencies execute before the step loop), then run the
step loop passing `&PlanDependencySchedule` into `do_step`. `do_step`
(interpreter.rs:109) replaces each `get_asset(...).await?.get().await?` (Action links
:191-201, Evaluate :168-175, GetAsset* :243-305) with `schedule.asset(q)` →
`context.wait_for_dependency(asset).await` (fallback: `context.get_dependency_state(q)`
when absent). Side benefit: GetAsset*/
Evaluate step dependencies now flow through `pending_dependencies` recording (today
they bypass `Context::evaluate` entirely). `finalize_plan` (interpreter.rs:33) is
unchanged; `Context::add_dependency`'s upsert-by-key dedupes against the pre-pass.

### Migrated: evaluate_recipe pure-key delegation (assets.rs:1412-1465)

Keep `record_dependency_on_asset(&asset)`; replace the ad-hoc
`matches!(status, Submitted | Dependencies)` + `Box::pin(asset.run())` +
`asset.get()` block with a single
`manager.wait_for_dependency(self, &asset).await` — the F-1 inline guard retires
onto the shared, claim-based primitive.

## Integration Points

### Crate: liquers-core (only crate touched)

| File | Changes |
|---|---|
| `liquers-core/src/assets.rs` | `RunClaim`, `JobQueue.local_deps` + `try_to_start_immediately`/`push_local_dependency`/`pop_local_dependency`/`take_local_dependencies`, `submit`/worker-`run` refactor, `AssetRef::try_claim_for_run`/`leave_dependencies_and_resume`, `AssetManager` trait extension + `DefaultAssetManager` overrides, `cleanup_local_dependencies`, `evaluate_recipe` delegation migration |
| `liquers-core/src/context.rs` | `Context::schedule_dependency_asset`/`wait_for_dependency`/`evaluate_local_queue`/`get_dependency_state`, `Context::evaluate` reimplementation |
| `liquers-core/src/interpreter.rs` | `PlanDependencySchedule`, `schedule_plan_dependencies` pre-pass, `apply_plan`/`do_step` migration to captured AssetRefs |
| `liquers-core/src/dependencies.rs` | `ScheduleNode`, expression attribution maps, `register_scheduled_dependency`, `remove_expression` |
| `liquers-core/tests/dependency_scheduling.rs` | new integration test suite (Phase 3) |

### Dependencies

No new crate dependencies: `tokio` (sync primitives), `scc`, and `async_trait` are
already workspace dependencies of `liquers-core`.

### Downstream crates

`liquers-store`, `liquers-lib`, `liquers-axum`: no code changes; behavior-transparent.
`liquers-py`: no API change expected (trait methods have defaults); verified by
`cargo check -p liquers-py`.

## Cycle Handling (keyed-only graph; expressions expand to their keyed dependencies)

Design principle (user decision, Phase 2 gate): **only keyed assets can be nodes of
the dependency graph. A non-keyed asset (expression) is treated as the set of keyed
assets it depends on.** The verified gaps (DESIGN.md notes) are closed at **schedule
time** by implementing this expansion in the `DependencyManager`:

### Schedule node classification

```rust
/// How an asset participates in dependency-graph bookkeeping at schedule time
/// (liquers-core/src/dependencies.rs).
pub(crate) enum ScheduleNode {
    /// Keyed asset: a real graph node (its key as DependencyKey).
    Keyed(DependencyKey),
    /// Non-keyed asset (expression), identified by its query DependencyKey;
    /// NOT a graph node — stands for the set of keyed assets it depends on.
    Expression(DependencyKey),
}
```

### DependencyManager extensions (transient attribution bookkeeping)

```rust
pub(crate) struct DependencyManager<E: Environment> {
    // ... existing fields unchanged (versions, keyed_dependents,
    //     dependent_assets, expiration_lock) ...

    /// Transient: keyed assets that (directly or through expression chains)
    /// depend on this expression — the expression's ATTRIBUTION SET.
    expression_dependents: scc::HashMap<DependencyKey, scc::HashSet<DependencyKey>>,
    /// Transient: keyed dependencies discovered so far by this expression.
    expression_keyed_deps: scc::HashMap<DependencyKey, scc::HashSet<DependencyKey>>,
    /// Transient: expression dependencies of this expression
    /// (needed to propagate late-joining keyed dependents down chains).
    expression_expr_deps: scc::HashMap<DependencyKey, scc::HashSet<DependencyKey>>,
}

impl<E: Environment> DependencyManager<E> {
    /// Register a scheduled dependency edge under the expansion principle;
    /// performs all cycle checks. Called by Context::schedule_dependency_asset.
    ///
    /// A = attribution set of `dependent`:
    ///     Keyed(k) => {k};  Expression(q) => expression_dependents[q]
    ///     (empty for a top-level query — nothing depends on it).
    /// match dependency:
    ///   Keyed(d) => for R in A { would_create_cycle(R, d)? -> Err(dependency_cycle);
    ///               add_dependency(R, d, version) };
    ///               if dependent is Expression(q) { expression_keyed_deps[q] += d }
    ///   Expression(dq) => if dependent is Expression(q) && q == dq
    ///                       -> Err(dependency_cycle)  (self-schedule);
    ///               propagate_attribution(dq, A, visited)  (below);
    ///               if dependent is Expression(q) { expression_expr_deps[q] += dq }
    pub(crate) async fn register_scheduled_dependency(
        &self,
        dependent: &ScheduleNode,
        dependency: &ScheduleNode,
        version: Version,
    ) -> Result<(), Error>;

    /// Attribution propagation: add every R in `attribution` to
    /// expression_dependents[expr]; for each already-known keyed dep X in
    /// expression_keyed_deps[expr]: would_create_cycle(R, X)? -> Err : add R->X;
    /// then recurse into expression_expr_deps[expr] with a visited set.
    /// If the traversal re-encounters the ORIGINATING dependent expression,
    /// a pure-expression cycle exists -> Err(dependency_cycle). This is graph
    /// traversal of the attribution bookkeeping, not a second cycle detector:
    /// keyed cycle detection remains would_create_cycle's BFS.
    async fn propagate_attribution(...) -> Result<(), Error>;

    /// Transient cleanup: drop the three expression_* entries for `expr`.
    /// Called when the expression asset reaches a terminal status (same
    /// cleanup point as the local-queue leftover handling).
    pub(crate) async fn remove_expression(&self, expr: &DependencyKey);
}
```

### Why this satisfies the model and closes the gaps

- **Keyed↔keyed** (including purely dynamic mutual evaluation — verified gap 3):
  edges are now registered at schedule time, so `would_create_cycle` sees them.
- **K→Q→K through an expression:** when K schedules Q, K joins Q's attribution set;
  when Q schedules K, the expansion checks `would_create_cycle(K, K)` — the
  dependent == dependency fast path fails immediately.
- **Shared expression, second parent (K2→Q→K2, Q first scheduled by K1):** Q's
  earlier `depends on K2` registered edges R→K2 for R in Q's attribution set at
  that time; when K2 later schedules Q, K2 joins the attribution set and edges
  K2→X for every already-known keyed dep X of Q are created with cycle checks —
  X == K2 hits the fast path. Late joiners are propagated down expression chains
  via `expression_expr_deps`.
- **Pure-expression cycles (Q1→Q2→Q1):** detected by the attribution traversal
  re-encountering the originating expression (see `propagate_attribution`);
  additionally the direct self-schedule (q == dq) fails fast.
- **Semantic graph unchanged:** versions, expiration cascades, metadata
  dependency records, `track_asset` are untouched; expression entries are
  transient scaffolding removed at expression terminal cleanup. Keyed→keyed edges
  registered through the expansion ARE the semantic edges of the user model ("the
  keyed dependent really depends on the expression's keyed dependencies"), so no
  pollution occurs. (Today's raw-query `DependencyRecord`s in metadata remain as
  they are — a separate, pre-existing recording convention outside this feature's
  scope.)

## Cleanup & Lifecycle (leftover policy — Phase 1 resolution)

One cleanup point: when a dependent asset reaches a terminal status, its local-queue
entry is removed. Implemented as
`DefaultAssetManager::cleanup_local_dependencies(parent_id: u64)` called from (a) the
spawn-completion closures in `try_to_start_immediately` / worker `run`, and (b) the
end of inline runs in `drain_dependencies`. For each leftover obtained via
`take_local_dependencies(parent_id)`:

| Leftover kind | Test | Action |
|---|---|---|
| Shared (managed) | present in `assets` / `query_assets` maps | keep `Submitted`: insert into the global jobs list via `job_queue.submit` so the worker runs it and plain `get().await` waiters are never stranded |
| Non-shared | not in the maps (volatile / ad-hoc) | discard with a debug log — the `PlanDependencySchedule` entry and the local queue were the only references |

Also: the `DependencyManager::remove_expression(query_dep_key)` transient-attribution
cleanup (see Cycle Handling) runs at the same terminal point for non-keyed assets.
Cancellation (WP-1
Flow D): cancelling the parent never cancels dependencies; an inline-run dependency
whose parent future is dropped is re-parked by the `RunClaim` Drop repair.

## Progress / No-Deadlock Argument

1. **Claim uniqueness:** `try_claim_for_run` is one CAS under one write lock; at most
   one live `RunClaim` per asset; `run()` is only called by claim holders — shared
   dependencies execute at most once (this also closes the existing double-run window:
   `run_with_future` only guards finished assets today, assets.rs:1373).
2. **Claim liveness:** every claim is held by a queue-spawned task or by an inline
   drain inside a live parent future; if that future is dropped, the Drop repair
   re-parks the asset as globally `Submitted` and notifies the queue.
3. **Parked reachability:** an unfinished, unclaimed dependency some waiter needs is
   on that waiter's own local queue (pushed at its schedule) or globally `Submitted`
   (initial submit, repair, or leftover-cleanup path). `wait_for_dependency` drains
   the waiter's own queue and attempts a direct claim before ever blocking — so a
   waiter only blocks on an asset that is actively running in some live future.
4. **Termination:** schedule-time cycle checks make the waits-on relation a finite
   DAG; induction over its depth gives termination of every active run (assuming
   command bodies terminate). Inline runs consume zero extra queue slots, so no
   capacity wait-cycle exists at any queue capacity ≥ 1.

## Volatility Semantics (execute-once guarantee)

- `schedule_dependency_asset` performs the single resolution; volatile queries yield a
  fresh AssetRef exactly there (`get_volatile_query_asset`, assets.rs:2838 /
  `get_volatile_resource_asset`, :2786). The `PlanDependencySchedule` map entry (or
  the local variable in `get_dependency_state`/`evaluate`) and the local-queue entry
  alias that one AssetRef.
- Waiting twice on the same captured AssetRef returns the same result
  (`Status::Volatile` is data-bearing). Re-scheduling the same volatile query = a new
  evaluation (documented, intended).
- The interpreter pre-pass dedupes by `Query` within one `apply_plan`, so a volatile
  dependency referenced by several steps/links of one plan evaluates exactly once
  per parent evaluation; re-evaluating the parent re-schedules freshly.
- Volatile children never enter the manager maps; leftover cleanup therefore
  classifies them non-shared and discards them — no cache pollution, no leaks.

## Relevant Commands

### New Commands

**None.** The mechanism is command-transparent: commands interact with it only
through the `Context` API (`get_dependency_state`, `evaluate_local_queue`, and the
unchanged `evaluate`). Scheduling is not exposed to commands — the schedule/wait
split (`schedule_dependency_asset` + `wait_for_dependency`) is `pub(crate)` and used
only by the interpreter pre-pass and the two convenience methods above.

### Relevant Existing Namespaces

No liquers-lib command namespace is modified or specially affected; every existing
command that evaluates sub-queries via `context.evaluate` transparently gains the
non-blocking behavior. (User confirmation requested at the Phase 2 gate.)

## Web Endpoints

None added or changed. Observable difference for API/UI consumers: the status flow
`Processing → Dependencies → Processing` while an asset waits for a dependency
(documented in the `specs/DEPENDENCIES_STATUS.md` update).

## Error Handling

No new error types; typed constructors only (CLAUDE.md).

| Scenario | Constructor | Notes |
|---|---|---|
| Cycle at schedule time | `Error::dependency_cycle(&dependent_key)` | same constructor as context.rs:218 |
| Dependency failed (Error status) | stored error from dependency metadata, propagated via `parent.fail_due_to_dependency(e)` (assets.rs:779) | parent reaches `Error` with dependency context |
| Dependency cancelled | cancellation error extracted, or `Error::general_error(format!("Dependency asset {} was cancelled", id))` | aligns with WP-2's future poll_outcome |
| Inline run failure during drain | logged on the parent (metadata log); draining continues | failure surfaces at wait time of whichever waiter needs the asset |
| Claim repair anomalies | logged, never panics | Drop must not panic; repair is best-effort |

## Serialization Strategy

None of the new types (`RunClaim`, `PlanDependencySchedule`, the `local_deps` map) is
serialized or persisted — all are
runtime scheduling state. No serde derives. Metadata/DependencyRecord serialization
is unchanged.

## Concurrency Considerations

- `try_claim_for_run`: one `RwLock::write` on `AssetData`; no other lock held.
- `local_deps` mutex: held only for push/pop/take; NEVER held across an `.await` of
  asset execution (pop returns the AssetRef, the lock is released, then run).
- `jobs` mutex: unchanged discipline; `try_to_start_immediately` does not await
  asset status while holding it (statuses are claimed via the per-asset lock).
- Watch-channel waits re-check authoritative state before and after every
  `rx.changed()` (lossy-channel discipline, as `AssetRef::get`).
- Shared dependency scheduled by two parents at capacity: both local queues hold the
  same AssetRef; the claim arbitrates; the loser's drain skips it and its wait loop
  subscribes — no double execution, no lost wakeup (state is re-checked after every
  claim failure).

## Compilation Validation

- [x] All new/changed signatures specified above; generics limited to the existing
      `E: Environment` (+`'static` at spawn sites).
- [x] No `unwrap()`/`expect()` in any signature; `Result<_, Error>` throughout.
- [x] Explicit status matches everywhere `Status` is inspected (claim, wait,
      cleanup, leftover classification) — no default arms.
- [x] New trait methods have default implementations → existing `AssetManager`
      implementors (including liquers-py wrappers) keep compiling;
      `cargo check -p liquers-py` is part of validation.

## References to liquers-patterns.md

- [x] All changes confined to `liquers-core` (dependency-flow safe).
- [x] No ExtValue/commands/store changes; `register_command!` untouched.
- [x] Async default; `#[async_trait]` for the trait extension.
- [x] Typed error constructors only; no `Error::new`.
- [x] Performance-sensitive areas: no query-parsing or key-encoding changes; the
      fallback map is off the hot path (touched only when capacity is exhausted).
