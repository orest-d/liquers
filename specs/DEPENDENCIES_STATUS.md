# Dependencies Status Specification

## Overview

`Status::Dependencies` is the lifecycle state used when an asset cannot expose a value because it
is waiting for one or more dependency assets. It is not a terminal state and it does not contain
asset data: `poll_state()` returns `None` while the asset is in `Dependencies`.

The dependency graph remains the source of truth. Static plan dependencies, runtime dependencies
recorded by `Context`, persisted `MetadataRecord.dependencies`, and `DependencyManager` edges are
the dependency facts. `Status::Dependencies` only describes the current lifecycle wait.

## Issue F-1 and the implemented fix

Review issue **F-1** identified a hard deadlock in pure-key recipe delegation:

1. Parent asset `A` starts in the job queue and occupies one queue slot.
2. During `AssetRef::evaluate_recipe()`, `A` discovers that its recipe delegates to keyed asset
   `B`.
3. The old code called `B.get().await` directly while `A` still occupied its slot.
4. If the queue was already at capacity, `B` stayed `Submitted` and could not start. A delegation
   chain deeper than queue capacity therefore hung forever.

The current implementation solves F-1 by making delegation an explicit dependency wait:

- `AssetRef::record_dependency_on_asset(&child)` records the delegated child in parent metadata and
  in `DependencyManager` before waiting.
- `AssetRef::enter_dependencies(&child)` moves the parent to `Status::Dependencies` and notifies
  observers that the parent is blocked on the child.
- If the delegated child is still only queued, the parent path runs that child job inline. This is
  the current deadlock guard: the child no longer needs to wait for another queue slot before it can
  make progress.
- `AssetRef::fail_due_to_dependency(error)` turns parent evaluation into `Error` when the delegated
  child fails.
- `AssetRef::leave_dependencies_for_resubmit()` clears the dependency wait once the child is ready,
  and the parent can finish normally.
- `JobQueue` is notify-driven (`Notify`) rather than a periodic sleeper, so submitted work and job
  completion wake dispatch promptly. `DefaultAssetManager::with_capacity()` allows capacity=1
  regression coverage, and `shutdown()` stops queue/expiration background tasks.

The result is that the parent no longer waits invisibly in `Processing`; consumers see
`Dependencies`, dependency metadata/graph edges exist, and a queued child can progress even under
queue-capacity pressure.

## Current contract

- `Status::Dependencies` is the only status used for dependency waiting; there is no
  `WaitingForDependency` status.
- `Status::Dependencies` has no data, is not finished, is not considered processing, and remains
  cancellable like `Processing`.
- Dependency edges are graph/metadata facts, not status facts. Scheduler-local wait bookkeeping is
  diagnostic only.
- `Version::unknown()` (`Version(0)`) means the dependency version is not known yet. Unknown
  versions may record edges, but they must not replace an already-known dependency version in
  metadata.
- Dependency-cycle checks use `DependencyManager::would_create_cycle()` / `add_dependency()` and
  static dependency discovery. There is no separate canonical wait-cycle graph.

## Detailed evaluation flows

The flows below describe the most complex paths first. Simpler paths skip the marked steps.

### Flow A: queued keyed asset with pure-key delegation and a queued child

This is the F-1 path.

1. **Submit parent**
   - `DefaultAssetManager::get()` or `get_asset()` obtains/creates parent asset `A`.
   - `JobQueue::submit(A)` either starts `A` immediately or marks it `Submitted`.
   - `JobQueue::run()` wakes via `Notify`, collects candidate jobs without awaiting while holding
     the queue mutex, marks selected jobs `Processing`, and spawns `A.run()`.

2. **Start evaluation**
   - `A.run()` calls `evaluate_and_store()` / `evaluate_recipe()`.
   - `evaluate_recipe()` checks whether the current recipe's key maps to another asset. If it maps
     to `A` itself, this is the normal self-recipe path and steps 3-8 are skipped.

3. **Discover delegated child**
   - `evaluate_recipe()` finds child asset `B` for the same key/delegation target.
   - `record_dependency_on_asset(B)` computes the child `DependencyKey`, finds the best available
     version (child metadata version, `DependencyManager` version, or `Version::unknown()`), and
     upserts the parent metadata dependency.
   - If parent `A` is keyed, `record_dependency_on_asset(B)` checks `would_create_cycle(A, B)` and
     then calls `DependencyManager::add_dependency(A, B, version)`.
   - If `version == Version::unknown()`, `DependencyManager` still records the edge but skips stale
     version comparison. This preserves graph shape without pretending to know a concrete version.

4. **Enter dependency wait**
   - If `B.poll_state()` is `None`, `A.enter_dependencies(B)` sets `A` to
     `Status::Dependencies`, writes the metadata status, logs the wait, and sends
     `StatusChanged(Dependencies)`.
   - While this status is active, `A.poll_state()` returns `None` even if stale data happens to be
     present.

5. **Deadlock guard for queued child**
   - If `B.status()` is `Submitted` or `Dependencies`, the parent path invokes `B.run()` inline.
   - This step is skipped when `B` is already ready, already processing elsewhere, or already
     terminal.
   - This is the concrete F-1 fix for queue-capacity deadlocks: a child that could not acquire a
     queue slot can still run to completion.

6. **Child completion**
   - `B.run()` follows the same evaluation machinery recursively. If `B` delegates again, steps
     3-6 repeat for the next child.
   - On success, `B` reaches `Ready`/`Volatile`/another data-bearing state and notifies waiters.
   - On failure, `B.run()` returns an error.

7. **Propagate child result**
   - If the inline child run failed, `A.fail_due_to_dependency(error)` clears parent data/binary,
     sets `Status::Error`, records error metadata, and sends `ErrorOccurred`.
   - Otherwise `A` calls `B.get()` and obtains the child state. If `get()` returns an error,
     parent evaluation returns a dependency-context error.

8. **Leave dependency wait and finish parent**
   - `A.leave_dependencies_for_resubmit()` changes `Dependencies` back to `Submitted` before final
     completion.
   - `evaluate_recipe()` returns the delegated state. `evaluate_and_store()` stores it on `A`,
     finalizes status/expiration, persists if needed, and registers finished non-volatile metadata
     dependencies with `DependencyManager::track_asset()`.

### Flow B: queued or immediate command uses `Context::evaluate()` at runtime

This is the runtime dependency path for commands that discover dependencies while running.

1. **Command receives `Context`**
   - Both queued recipe evaluation and immediate evaluation create a `Context` for the current
     asset.
   - The context owns a shared `pending_dependencies` vector, also shared with cloned contexts.

2. **Command requests dependency**
   - The command calls `context.evaluate(query)`.
   - `Context::evaluate()` gets the current asset key when available.
   - If current and dependency keys are known, it calls
     `DependencyManager::would_create_cycle(current, dependency)` before recording the edge.

3. **Obtain child asset**
   - `Context::evaluate()` calls `manager.get_asset(query)`, which creates/submits or returns the
     dependency asset.
   - If the dependency is already data-bearing, steps 5-6 are skipped.

4. **Record pending dependency**
   - `Context::evaluate()` computes the dependency key and version.
   - Missing versions are represented as `Version::unknown()`.
   - `Context::add_dependency(record)` upserts into `pending_dependencies`; if a known version is
     already present, a later unknown observation is ignored instead of downgrading it.
   - If the current asset is keyed, `add_dependent_asset()` also records the current asset as an
     untracked dependent of the dependency key.

5. **Enter dependency wait**
   - If the dependency asset is not ready, `Context::evaluate()` calls
     `current_asset.enter_dependencies(child)`.
   - The command may then call `child.get().await` to obtain the child state; while it waits, the
     current asset is observable as `Status::Dependencies`.

6. **Drain runtime dependencies**
   - Queued `evaluate_recipe()` drains `context.take_pending_dependencies()` after recipe execution
     and merges the records into the produced metadata.
   - Immediate `evaluate_immediately()` does the same before publishing `ValueProduced`.
   - The legacy interpreter-level `evaluate()` helper also drains pending dependencies into the
     returned `State` metadata.
   - If no runtime dependencies were recorded, this drain is a no-op.

7. **Finalize**
   - The asset eventually reaches a data-bearing status, `Error`, or `Cancelled`.
   - For non-volatile ready assets, `DependencyManager::track_asset()` loads persisted metadata
     dependencies back into the graph.

### Flow C: static plan dependencies

This path handles dependencies known before command execution.

1. `recipe.to_plan()` builds a plan.
2. `finalize_plan()` performs static dependency analysis for volatility/expiration and seeds
   `Context::pending_dependencies` with plan dependencies.
3. If the plan's query is keyed, `DefaultAssetManager::register_plan_dependencies()` registers
   direct plan edges in `DependencyManager` when concrete dependency versions are available.
4. Later runtime dependency drains merge these static records with runtime records. Duplicate keys
   are represented once, and known versions are preserved over unknown versions in the context
   pending-dependency path.

### Flow D: cancellation and failures while waiting

1. Cancellation of an asset in `Dependencies` is handled like cancellation from `Processing`:
   the current asset transitions to `Cancelled`.
2. The dependency asset is not cancelled; it may be needed by other assets.
3. Dependency failures propagate through `fail_due_to_dependency()` in the delegation path or as
   errors returned from `child.get().await` in runtime-command paths.
4. `Status::Dependencies` itself is never terminal and never exposes data.

## Function glossary

- `Context::evaluate(query)`: runtime dependency entry point for commands. It requests/submits the
  dependency asset, records a pending dependency, performs graph-cycle checks when possible, and
  enters `Status::Dependencies` if the child is not ready.
- `Context::add_dependency(record)`: pending dependency upsert helper. It preserves a known version
  over a later `Version::unknown()` observation.
- `Context::take_pending_dependencies()`: drains runtime/static dependency records for metadata
  assembly after evaluation.
- `AssetRef::record_dependency_on_asset(child)`: direct asset dependency recorder used by pure-key
  delegation. It updates parent metadata and keyed `DependencyManager` edges.
- `AssetRef::enter_dependencies(child)`: status/metadata/notification helper for entering the
  dependency wait state.
- `AssetRef::leave_dependencies_for_resubmit()`: helper for leaving `Dependencies` before parent
  evaluation finishes or is resubmitted.
- `AssetRef::fail_due_to_dependency(error)`: helper for converting dependency failure into parent
  `Error` state.
- `DefaultAssetManager::with_capacity(capacity)`: constructs a manager with configurable queue
  capacity, used to exercise F-1 capacity-sensitive paths.
- `DefaultAssetManager::shutdown()` and `JobQueue::shutdown()`: stop background queue/expiration
  tasks.

## Non-blocking dependency scheduling (2026-07-15)

Dependency evaluation is now non-blocking and deadlock-free (see
`specs/dependency-scheduling/`). Key points for status semantics:

- A parent waiting for a dependency follows the truthful flow
  `Processing → Dependencies → Processing`: it enters `Status::Dependencies` only at
  drain/wait time (via `AssetRef::leave_dependencies_and_resume`, the resume
  counterpart of `enter_dependencies`), not eagerly at schedule time. `Status::Dependencies`
  remains the sole waiting status and carries no data (`poll_state()` is `None`).
- "Who runs an asset" is a single atomic decision: `AssetRef::try_claim_for_run`
  transitions a not-yet-running asset to `Processing` under one lock and hands out a
  `RunClaim`; `run()` is only ever called by a claim holder (execute-once). A claim
  dropped mid-run (cancelled parent) re-parks the asset as `Submitted` and re-submits it.
- Dependencies are scheduled without occupying a parent's queue slot: they start
  immediately when capacity allows, else park on the parent's local queue and are
  drained inline from the parent's own future (`AssetManager::wait_for_dependency`
  drains + direct-claims before ever blocking). Cancelling a parent never cancels its
  dependencies.
- Schedule-time cycle detection (`DependencyManager::register_scheduled_dependency`,
  keyed-expansion model) rejects dependency cycles with `Error::dependency_cycle`
  instead of hanging.
