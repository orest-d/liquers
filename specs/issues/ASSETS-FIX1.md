---
id: ASSETS-FIX1
kind: feature
title: Unresolved TODO and FIXME markers in the asset lifecycle
status: closed
priority: P2
complexity: L
area: [core/assets]
design:
created: 2026-08-08
github:
---
# ASSETS-FIX1


## Summary
`ASSETS-FIX1` consolidates all `TODO`, `FIXME`, and `todo!()` markers in `liquers-core/src/assets.rs` into a concrete implementation backlog.  
Focus: remove known runtime gaps (dependency handling, delegation deadlock risk, metadata consistency), reduce duplication, and finalize incomplete API paths.

Related feature split:
- Expiration timing/race safety concerns are now tracked under [EXPIRATION-SAFETY.md](/home/orest/zlos/rust/liquers/specs/archive/EXPIRATION-SAFETY.md).
- Scheduler/runtime robustness for dependency delegation is now tracked under [SCHEDULER-IMPROVEMENTS.md](/home/orest/zlos/rust/liquers/specs/archive/SCHEDULER-IMPROVEMENTS.md).


## Inventory (assets.rs)

| # | Fix? | Location | Marker | Proposed solution |
|---|---|---|---|---|
| 16 | Phase4 | `assets.rs:1233` | log string contains `FIXME` | Replace with structured debug log without FIXME marker. |
| 17 | Phase4 | `assets.rs:1237` | `FIXME` delegation can deadlock if not queued | Replace blocking delegation (`asset.get().await`) with dependency scheduling + non-blocking parent wait state; ensure delegated asset submitted before parent waits. |

## Detailed analysis: remaining Phase4 problem

The remaining functional problem is issue **#17** (with #16 as a logging symptom in the same code path), in `AssetRef::evaluate_recipe()` around `assets.rs:1213-1237`.

Current behavior:
1. When the recipe can be mapped to a key (`recipe.key()`), evaluation fetches canonical keyed asset via `manager.get(&key)`.
2. If the keyed asset is not `self`, evaluation delegates by calling `asset.get().await`.
3. `asset.get().await` waits until the delegated asset produces `ValueProduced`/`JobFinished`.

### Clarification: when can `asset.id() != self.id()` actually happen?

For non-volatile keyed assets created through normal `get_asset(query-with-key)`/`get(key)` flow, `self` is typically the canonical map entry, so equality is expected.

However, there are concrete paths where inequality is expected and currently reachable:

1. Volatile key assets (most important, reachable via normal API):
   - `DefaultAssetManager::get_resource_asset()` routes volatile keys to `get_volatile_resource_asset()`.
   - `get_volatile_resource_asset()` creates a fresh `AssetRef` each call and does not store it in `assets` map.
   - During `evaluate_recipe()`, calling `manager.get(&key)` for the same key creates/returns another fresh volatile asset.
   - Result: `asset.id() != self.id()` is expected for volatile keyed recipes.

2. Ad-hoc assets with key-shaped recipe (reachable via `apply()` and concrete-manager APIs):
   - `apply()` constructs a new untracked asset via `AssetData::new_ext(...)`.
   - If that ad-hoc recipe is a pure key query (`recipe.key() == Some(key)`), `evaluate_recipe()` calls `manager.get(&key)`.
   - Manager returns keyed asset for that key, which is a different object than the ad-hoc `self`.
   - Result: inequality by construction.

3. Explicit untracked asset creation (`create_asset(...)` / direct constructors):
   - `create_asset(recipe)` and direct `new_from_recipe/new_ext` create assets outside manager key cache.
   - If such recipe resolves to a key, `manager.get(&key)` returns manager-owned asset, not caller-owned temporary.
   - Result: inequality by design.

4. CWD/normalization-derived key mismatch (secondary but possible):
   - `Recipe::key()` resolves relative keys to absolute if `recipe.cwd` is set.
   - A self asset built from non-canonical/relative recipe identity can resolve to a canonical key at evaluation time.
   - `manager.get(&resolved_key)` may return a different manager-owned asset.
   - Result: identity split from key normalization.

Conclusion:
- The inconsistency is real and not only theoretical.
- It is rare for non-volatile canonical keyed assets, but common for volatile keyed assets and for ad-hoc/untracked assets.
- Therefore, relying on `self == manager.get(&key)` as a general invariant is not safe.

### Verification: does "untracked" itself imply deadlock risk?

No. "Untracked in manager maps" and "scheduled in queue" are separate concerns.

Verified execution paths:
1. `apply(...)`:
   - Creates an ad-hoc/untracked asset (`new_ext(...).to_ref()`), then immediately calls `job_queue.submit(...)`.
   - So this path is untracked **but queued**.
   - Deadlock risk from queue-slot capture still applies.

2. `apply_immediately(...)`:
   - Creates an ad-hoc/untracked asset, then calls `run_immediately(...)` directly.
   - No queue slot is consumed by the parent asset itself.
   - By itself this path does not create the queue-capacity deadlock pattern.

3. `create_asset(...)` / direct ad-hoc construction:
   - Creates untracked asset only; scheduling depends on caller.
   - If caller runs directly (`run`/`run_immediately`) without queue submission, parent does not consume queue capacity.
   - Again, no direct queue-capacity deadlock from parent alone.

Practical rule:
- The deadlock pattern requires a running queue worker to block on `asset.get().await` for another asset that still needs queue execution.
- Therefore, untracked assets are dangerous only when they are (or cause) queued execution in a way that consumes all available slots.
- This means your caveat is correct: non-queued untracked assets can still be involved **indirectly** if they trigger queued sub-evaluations that then deadlock among queued workers.

### Why this is dangerous

The parent evaluation is already running inside a queue worker (`asset.run()` from `JobQueue`).
In the delegation branch, that worker blocks waiting for another asset.

Deadlock case (deterministic):
1. Queue capacity is `1`.
2. Asset `A` starts and enters `evaluate_recipe()`.
3. `A` delegates to asset `B`.
4. `manager.get(B)` submits `B` as `Submitted` (cannot run, queue slot occupied by `A`).
5. `A` waits on `B.get().await`.
6. `B` cannot start until a slot frees; `A` cannot finish until `B` finishes.
7. System is stuck.

This also generalizes to larger capacities when dependency depth/fan-in exceeds currently free slots.

### Additional correctness gaps in this path

1. Waiting state is not explicit:
   - Parent asset does not transition to `Status::Dependencies` before waiting.
   - Runtime has tests for handling `Status::Dependencies`, but production path does not set it here.
2. Observability is weak:
   - Log line still contains `" - FIXME"`, giving no structured reason/context.
3. Liveness assumptions are implicit:
   - `AssetRef::get()` explicitly warns it may hang if evaluation is not running.
   - Delegation relies on queue/submission behavior but has no explicit liveness contract in this branch.
4. Cycle handling is incomplete:
   - `asset.id() == self.id()` avoids only trivial self-delegation.
   - Multi-asset cycles are not resolved in this runtime branch.

### Scope boundaries for Phase4 fix

A valid fix must address liveness, not only logging text.

Required properties:
1. Delegated dependency is guaranteed to be schedulable before parent waits.
2. Parent does not hold scarce execution capacity while waiting on dependency completion.
3. Parent exposes `Dependencies` state (status + metadata/progress) while blocked on dependencies.
4. Delegation logging is structured and traceable (parent id, dependency id/key, scheduling decision).

Non-goals for this Phase4 item:
1. Full scheduler redesign.
2. General command classification (`fast/slow`) or multi-queue strategy (moved to `EXTENDED-FAST-TRACK`).

### Suggested acceptance criteria for implementation

1. With queue capacity `1`, delegating evaluation does not deadlock.
2. Parent transitions through `Dependencies` while child is pending/running.
3. Delegated asset is submitted exactly once (no duplicate queue amplification).
4. Logs/metadata no longer contain `FIXME` markers and include dependency wait diagnostics.

## Resolution (2026-10-20, bulk-design triage)

Closed: both remaining items are fixed at HEAD (commit 18732dd). Verified by reading the code and
tests; nothing was changed for this record.

- **#16 (the `FIXME` log line)** — gone. `grep -n "FIXME" liquers-core/src/assets.rs` finds
  nothing. The delegation branch in `AssetRef::evaluate_recipe_outcome` logs
  "Delegating evaluation of asset {} to asset {}", and the wait records a structured
  `LogEntry::info("Waiting for dependency <key or query>")` in the parent's metadata
  (`AssetRef::enter_dependencies`).
- **#17 (delegation can deadlock)** — the bare `asset.get().await` is replaced by
  `AssetManager::wait_for_dependency` (`DefaultAssetManager::wait_for_dependency`, assets.rs
  ~8666), the claim-based primitive: it drains the parent's local dependency queue, directly claims
  a still-runnable dependency and runs it on the parent's slot (no new queue slot), or subscribes
  with the lost-wakeup re-poll. The parent enters `Status::Dependencies` while it waits and resumes
  afterwards (`enter_dependencies` / `leave_dependencies_and_resume`). The owner is found with the
  non-evaluating `owned_key_asset` map read, so the delegate is never submitted a second time.
  Designs: `dependency-management` (complete; "deadlock-safe delegation"), `keyed-recipe-ownership`
  and `keyed-delegation-hand-off` (complete). Evidence: `scenario_keyed_delegation`
  (`liquers-core/tests/common/manager_scenarios.rs`, run by `keyed_delegation_default` /
  `keyed_delegation_immediate` in `tests/manager_parametric.rs`) and the capacity-1 chain tests in
  `liquers-core/tests/deep_chain_evaluation.rs`.
- Suggested acceptance criteria 1-4 of this file are met by the above: no capacity-1 deadlock (the
  wait borrows the parent's slot), `Dependencies` while waiting, one submission, no `FIXME`.

Markers left in `assets.rs` at HEAD, and where they belong:

- `assets.rs:1394` `// TODO: support for quick plans` in `AssetData::try_fast_track` —
  `EXTENDED-FAST-TRACK` (its migration table, item 4).
- `assets.rs:5168` `// TODO: Remove?` on the `#[cfg(test)]`-only `AssetRef::set_value` — test
  scaffolding, no runtime effect; not tracked.
- The `apply()` / `apply_immediately()` fast-track TODOs (items 23, 24, moved to
  `EXTENDED-FAST-TRACK`) no longer exist: `apply` absorbed `apply_immediately` and runs inline
  without a job-queue slot.

The links to `specs/archive/EXPIRATION-SAFETY.md` and `SCHEDULER-IMPROVEMENTS.md` above are
absolute paths from another machine; the expiration work they name is the complete design
`expiration-safety`.
