---
title: Status::Dependencies Specification
kind: reference
audience: internal
area: [core/assets]
reviewed: 2026-08-12
---
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

The current implementation solves F-1 by routing delegation through the ordinary dependency-wait
machinery:

- `AssetRef::record_dependency_on_asset(&child)` is called before waiting, but **records nothing in
  the delegation case**. See "Delegation is a hand-off, not a dependency" below.
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
`Dependencies`, and a queued child can progress even under queue-capacity pressure.

## Delegation is a hand-off, not a dependency

**Two assets that resolve to the same key are one node of the dependency graph.** `DependencyKey`
is the node identity, so a wait between two such assets has no edge to record.

This is exactly the delegation case. `AssetRef::evaluate_recipe` asks
`AssetManager::owned_key_asset(&key)` — with the key taken from *its own* recipe — whether some
other asset is the registered owner. When one is, the delegate is by construction registered under
the caller's own key, so both ends of any edge would be that same key.

`AssetRef::record_dependency_on_asset` therefore tests node identity before it writes anything and
returns `Ok(())` on a match: no `DependencyRecord` in parent metadata, and no edge offered to
`DependencyManager`. Both omissions matter.

**Identity comes from `AssetRef::bound_key_candidate()`** — the key each asset was *constructed*
with — and only falls back to the recipe-derived `DependencyKey`. `AssetData::recipe` is mutable:
provider resolution replaces it mid-evaluation, which is the same reason
`Context::schedule_dependency_asset` classifies a keyed dependent by `owner_key()` rather than by
its recipe. An owner whose recipe resolved to a pure-key alias `L` would otherwise look like a
different node than the delegate still holding `K`, and the edge `K -> L` would be recorded
carrying the *owner's* version — a version for `K` — which `DependencyManager::add_dependency`
compares against `L`'s and can expire `K` for.

- A self-record in metadata is persisted, and `DependencyManager::track_asset` feeds persisted
  records back through `load_from_records`, so it would reinstall a self-edge on every reload.
- `DependencyManager::would_create_cycle` returns `true` whenever `dependent == dependency`. That
  is the correct answer to the question it is asked; the fix is to stop asking it. Until 2026-08-12
  the delegation branch did ask, and so returned `Error::dependency_cycle` unconditionally — it
  could never succeed (`ASSET-KEYED-DELEGATION-ALWAYS-CYCLES`,
  `specs/design/keyed-delegation-hand-off/`).

The wait itself is unchanged: `AssetManager::wait_for_dependency` still provides the F-1 progress
guarantee. `DependencyManager::track_asset` needs no special case, because it resolves a key
through `AssetRef::bound_owner_key()`, which returns `None` for a non-owner — a delegating asset
does not re-register a version for the key or expire the owner's dependents.

Genuine dependencies between *different* keys are recorded exactly as before, and **genuine
self-dependency is still rejected**. The exemption is narrow in two ways: it applies only when the
two assets are the same node, and it lives only in `record_dependency_on_asset`, whose sole
production caller is the delegation branch. A runtime self-dependency — a command calling
`Context::evaluate` on its own asset's key — travels a different path entirely
(`schedule_dependency_asset` → `register_scheduled_dependency` → `would_create_cycle`) and still
fails fast with `Error::dependency_cycle`. That is pinned by
`liquers-core/tests/dependency_scheduling.rs::test_keyed_asset_evaluating_its_own_key_is_a_cycle`.

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
   - `evaluate_recipe()` finds child asset `B` registered as the owner of the key in `A`'s recipe.
   - `record_dependency_on_asset(B)` computes the child `DependencyKey` and compares it with `A`'s
     own. In this flow they are equal — `B` was looked up with `A`'s key — so the two assets are
     one graph node — compared by construction-time key, not by the owner's mutable resolved
     recipe — so nothing is recorded and `Ok(())` is returned. See "Delegation is a hand-off, not
     a dependency".
   - For any *other* caller, where the keys differ, the recorder behaves as documented in the
     glossary: it finds the best available version (child metadata version, `DependencyManager`
     version, or `Version::unknown()`), upserts the parent metadata dependency, and — if parent `A`
     is keyed — checks `would_create_cycle(A, B)` before `DependencyManager::add_dependency(A, B,
     version)`. With `Version::unknown()` the edge is still recorded, but stale-version comparison
     is skipped: graph shape is preserved without pretending to know a concrete version.

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
  delegation. It updates parent metadata and keyed `DependencyManager` edges — **except** when
  parent and child resolve to the same `DependencyKey`, which is one graph node and therefore a
  hand-off with nothing to record. Identity is the construction-time key
  (`bound_key_candidate()`), not the mutable resolved recipe.
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
`specs/design/dependency-scheduling/`). Key points for status semantics:

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

## History

| Date | Change | Source |
|---|---|---|
| 2026-08-12 | Delegation no longer records a dependency: two assets sharing a key are one graph node, compared by construction-time key rather than by the mutable resolved recipe (PR 32 review). New section "Delegation is a hand-off, not a dependency"; F-1 bullet, Flow A step 3 and the `record_dependency_on_asset` glossary entry corrected. Reviewed only for the delegation-recording claim — Flow A steps 5, 7 and 8 still describe the pre-2026-07-15 wait mechanics and are superseded by "Non-blocking dependency scheduling"; not re-verified here. | `specs/design/keyed-delegation-hand-off/` |
| 2026-07-15 | Last substantive edit, carried into `reference/` unchanged. Not reviewed against the implementation since. | migration |
