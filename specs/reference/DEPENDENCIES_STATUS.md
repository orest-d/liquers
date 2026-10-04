---
title: Status::Dependencies Specification
kind: reference
audience: internal
area: [core/assets]
reviewed: 2026-10-04
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
- **Every non-volatile keyed asset carries a concrete version.** It is assigned on the evaluation
  path — `Version::from_content` of the serialized value (a content hash, flagged by bit 127; see
  §Content changed outside Liquers in [`ASSETS.md`](ASSETS.md)), or `Version::new_unique()` when
  the value does not serialize — in the same write transaction as the status change, so no observer can read
  an asset that is ready but unversioned. A **non-keyed (query) asset gets none**: it is not a
  graph node, and serializing one would cost the commonest path in the system for a version
  nothing reads. A **volatile** asset gets none either.
- `Version::unknown()` (`Version(0)`) means the version is not known — and **only** that. It is not
  a marker for "this asset does not participate in invalidation". Unknown versions may record
  edges, but they must not replace an already-known dependency version in metadata.
- **A version is a fact about metadata, never about the value.** `AssetManager::version(key)` reads
  a live asset's metadata, else the store's sidecar, and never the value itself — so a key whose
  data has been deleted but whose sidecar remains still answers. That is deliberate: it is what
  lets a user delete large intermediates and keep the results derived from them.
- **`add_dependency` records; it does not verify.** This changed: it used to compare the recorded
  version against the manager's and expire the dependent on a mismatch. Recording is in-memory and
  happens on every edge; verifying may need a store read and is meaningful only on load or on
  demand, so fusing them put verification on the hot path and left nowhere to express a policy.
  The dependency graph now performs **no I/O**.
- **Verification is opt-in.** `AssetManager::trigger_dependency_audit(query)` and
  `trigger_dependency_audit_all_registered()` ask the graph which versions it is missing, resolve
  each store-resolvable one (`-R/`, `-R-dir/`) through `AssetManager::dependency_version` — which
  never evaluates — and hand the answer to `DependencyManager::audit_version`. Nothing in
  `liquers-core` calls them; the `_with(…, AuditMode)` variants take a mode.
- **An audit compares current with recorded versions, also on first observation.**
  `register_version` treats a first registration as "no change", which is right on the
  evaluation path and wrong for an audit: after a restart the version map is empty, so the first
  thing an audit learns is the current version. `audit_version` records the version and expires
  every dependent whose edge does not record exactly that version — an edge recording
  `Version::unknown()` included, as for any change. A current version of 0 ("none") follows
  `report_no_version`'s rules instead: only edges that expected a concrete version are expired.
  The root is not expired; its dependents get `Cascaded { Audit { found }, … }`.
- **`AuditMode::ReportOnly` changes nothing.** It goes through `stale_edges`, registers no
  version, removes no edge and expires nothing; the `AuditReport` lists what was `checked` and the
  direct `findings` (`AuditFinding { dependency, dependent, expected, found }`) that `Expire` mode
  would have expired. In `Expire` mode `expired` also lists the transitive keys.
  `AuditReport` and `AuditFinding` are `#[non_exhaustive]`, with `AuditFinding::new` for
  managers outside core.
- **`DependencyAuditPolicy` decides when stored dependents are checked.** It is set per
  environment (`AssetManagerOptions::with_dependency_audit`, or `assets.dependency_audit` in
  [`ENVIRONMENT_CONFIG.md`](ENVIRONMENT_CONFIG.md)) and read through
  `AssetManager::dependency_audit_policy()`:
  - `Explicit` (default): only the audits above.
  - `OnLoad`: also when `try_fast_track` loads a stored keyed asset. For each recorded dependency
    the version map does not know, and only for a store-resolvable key with a concrete recorded
    version, the current version is resolved through `dependency_version`. A different version, a
    current 0, or a store error refuses the stored copy, so it is recomputed. A recorded unknown is
    compatible and not asked. The comparison is equality, not `Version::matches`, which would
    accept a current 0. A version that passes is recorded in the map (`observe_version`, which
    fills only an empty entry and expires nothing), so a later recomputation of the dependency is
    compared with it rather than taken as a first observation.
- **A write through Liquers is always a change.** `set_binary`, `set_state` and
  `AssetManager::publish_version` register the written version with `register_written_version`,
  not `register_version`. When the map holds no version for the key, as after a restart while
  dependents served from the store carry edges recording the old one, the write still expires
  every dependent whose edge does not record exactly the written version. Rewriting the version
  the map already holds expires nothing.
- **A folder listing is a versioned dependency.** `-R-dir/<dir>` (`GetAssetDirectory`) has, as its
  version, the content hash of its sorted, length-prefixed names (`listing_version`) — membership
  only, so rewriting an existing member does not change it (the member's own key cascades). The
  step builds its value and its version from the same `listdir` read. After registering the
  version it reads the listing again (up to three reads in all): a write that landed before the
  registration skipped its refresh, so a moved listing is registered and used instead. It records
  the edge and registers the version, and also sets the listing version on its own
  query asset's metadata: `-R-dir/` is an evaluation boundary, so the dependent learns the version
  from there through `wait_for_dependency_recording` and `track_asset` / `load_from_records`.
  After every manager-mediated write or removal — an evaluation persisting a keyed value,
  `set_binary`, `set_state`, `remove`, `makedir`, `removedir`, and an outside change deleted as
  corrupted — the parent
  listing is recomputed by `refresh_listing_version`, **only if** the version map already holds a
  version for it, and its dependents are expired with `Updated { version }` when it moved. A
  `listdir` error there is logged, not fatal. A change made to the folder outside Liquers is
  found only by an audit. A write after the step's last read, but before the dependent records its
  edge, is not caught in-process (`DEPENDENCY-EDGE-RECORDED-AGAINST-SUPERSEDED-VERSION-IS-NOT-EXPIRED`).
- **A change expires only what it provably affects.** Each edge records the version its dependent
  observed, and `register_version` spares a dependent only when that expectation is concrete and
  equal to the new version. An edge recording `Version::unknown()` is expired: no evidence either
  way is not evidence of safety, and `propagate_attribution` records every attribution edge that
  way, so sparing them would drop every keyed dependent reached through a non-keyed expression out
  of the cascade. The invariant, which is stronger than the case list: **this expires a subset of
  what an unconditional cascade expires, and drops a dependent from that set only on positive
  evidence.**
- **The edge's expected version is caller-trusted.** Nothing validates that it was ever true; it is
  whatever the caller observed. That is only sound because a concrete version reaches
  `add_dependency` solely for a dependency that had one — a volatile, non-keyed or not-yet-versioned
  dependency yields `Version::unknown()`. A change that made any caller synthesize a concrete
  version would reintroduce spurious expiry.
- Dependency-cycle checks use `DependencyManager::would_create_cycle()` / `add_dependency()` and
  static dependency discovery. There is no separate canonical wait-cycle graph.
- **A `cached: false` keyed asset is still its key's graph node.** Such an asset is never
  registered in the manager's key map, so registration cannot identify it as the key's owner.
  `AssetRef::bound_owner_key` — which `track_asset` and the stale-dependency path both use to
  decide whether an asset is a keyed node — therefore also answers the key for an **unregistered**
  asset that was constructed for the key, is not volatile, and whose recipe targets the key and
  declares `cached: false`, provided **no other asset is registered** for it. Its dependencies are
  recorded and its version registered like an owner's, so a change upstream still reaches its
  dependents. A registered owner, when there is one, stays the only answer, which keeps a
  delegating asset answering `None`.
- **Expiring a key no registered asset holds expires its stored copy.** `expire_dependencies_result`
  expires each expired key through its registered asset when there is one; otherwise — the normal
  case for a `cached: false` key — it rewrites the store's metadata for the key to `Expired`, only
  when a copy exists and its stored status is `Ready` or `Override`. Without that, the stored copy
  would stay `Ready` and a fresh process would fast-track data the graph knows is stale. The race
  with an evaluation already in flight is
  `UNCACHED-STORED-COPY-EXPIRY-RACES-AN-INFLIGHT-EVALUATION`.

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
     version)`. The edge is recorded either way; `add_dependency` no longer compares versions at
     all, so an unknown version costs only the precision of a later `register_version`, which
     cannot spare an unknown-expecting dependent.

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
   - The command calls `context.submit(query)` (start, return the asset), `context.evaluate(query)`
     (`submit`, then drain the local queue) or `context.get_dependency_state(query)` (`submit`,
     then `wait_for_dependency`). `submit` is not lazy: the dependency has started, and on the
     inline manager finished, before it returns (`SUBMIT-IS-NOT-LAZY-ON-ANY-MANAGER`).
   - `Context::evaluate()` gets the current asset key when available.
   - If current and dependency keys are known, it calls
     `DependencyManager::would_create_cycle(current, dependency)` before recording the edge.

3. **Obtain child asset**
   - `Context::evaluate()` calls `manager.get_asset(query)`, which creates/submits or returns the
     dependency asset.
   - If the dependency is already data-bearing, steps 5-6 are skipped.

4. **Record pending dependency**
   - `Context::evaluate()` computes the dependency key and version.
   - Missing versions are represented as `Version::unknown()`. The version read at *schedule* time
     is taken before the dependency has evaluated, so for an ordinary recipe chain it is unknown;
     `Context::get_dependency_state` upgrades the record with the dependency's settled version once
     the wait completes, using the key the scheduler handed back rather than one derived at the
     wait — a differently-derived key would write a second record instead of upgrading the first.
     A command that calls `Context::evaluate` or `Context::submit` and awaits `AssetRef::get`
     directly bypasses that upgrade and keeps an unknown record, and also bypasses the
     stale-dependency policy: `get` fails on an expired dependency.
   - `Context::add_dependency(record)` upserts into `pending_dependencies`; if a known version is
     already present, a later unknown observation is ignored instead of downgrading it.
   - If the current asset is keyed, `add_dependent_asset()` also records the current asset as an
     untracked dependent of the dependency key.

5. **Enter dependency wait**
   - The command waits with `context.wait_for_dependency(&child)`, which delegates to
     `AssetManager::wait_for_dependency`; while it waits, the current asset is observable as
     `Status::Dependencies`. A dependency that expired meanwhile is used as it stands and the
     current asset finishes `Expired` with `Direct { StaleDependency { dependency } }`. Both
     built-in managers apply this policy.

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
3. If the plan's query is keyed, `AssetManager::register_plan_dependencies()` registers every
   direct plan edge in `DependencyManager`, with the dependency's registered version or, when it
   has none yet, `Version::unknown()`. The unknown edge is what lets a later registration (a
   listing, a value) expire the dependent, and what an audit resolves.
4. Later runtime dependency drains merge these static records with runtime records. Duplicate keys
   are represented once, and known versions are preserved over unknown versions in the context
   pending-dependency path.

### Flow D: cancellation and failures while waiting

1. Cancellation of an asset in `Dependencies` is handled like cancellation from `Processing`:
   the current asset transitions to `Cancelled`.
2. The dependency asset is not cancelled; it may be needed by other assets.
3. Dependency failures propagate through `fail_due_to_dependency()` in the delegation path or as
   errors returned from `context.wait_for_dependency(&child)` in runtime-command paths.
4. `Status::Dependencies` itself is never terminal and never exposes data.

## Function glossary

- `Context::evaluate(query)`: runtime dependency entry point for commands. It requests/submits the
  dependency asset, records a pending dependency, performs graph-cycle checks when possible, and
  enters `Status::Dependencies` if the child is not ready.
- `Context::submit(query)`: schedules and records a dependency and returns its asset without
  waiting; the dependency has already started.
- `Context::wait_for_dependency(&asset)`: waits on behalf of the current asset, applies the
  stale-dependency policy, and upgrades the record `submit` wrote to the settled version.
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
- `DependencyManager::audit_version(key, version)` (crate): records `version` and expires every
  dependent that does not positively match, also on a first observation; returns the expired set
  and the direct findings.
- `DependencyManager::stale_edges(key, version)` (crate): the direct edges `version` contradicts.
  Read-only.
- `DependencyManager::register_written_version(key, version)` (crate): the write path's
  registration. A first registration counts as a change, so dependents whose edges record another
  version expire; an unchanged entry expires nothing.
- `DependencyManager::observe_version(key, version)` (crate): fill an empty entry with a version
  just confirmed against the store, expiring nothing (the `on_load` check).
- `AssetManager::dependency_version(dep_key)`: the current version of a `-R/` key (`version`) or a
  `-R-dir/` key (listing version), without evaluating; any other key answers 0.
- `AssetManager::refresh_listing_version(dir)`: recompute and register a listing version iff one
  is registered; cascades with `Updated` when it moved. Never takes `key_mutation_lock`.
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
| 2026-10-04 | Review fixes on orest-d/liquers#75: writes register through `register_written_version` (a first registration is a change); `on_load` records a version it confirmed; `makedir` / `removedir` refresh the parent listing; the directory step versions the read that built its value and re-reads once registered. | phase-5 |
| 2026-10-02 | Reviewed against `design/dependency-audit-and-expiry-provenance/`. Current contract: versions are `from_content`; the "never / policy not expressible" bullet replaced by audits on first observation (`audit_version`), `AuditMode::ReportOnly` / `AuditFinding`, `DependencyAuditPolicy` (`explicit` / `on_load`) and folder-listing (`-R-dir/`) versions with their refresh. Flow B uses `submit` / `wait_for_dependency`; Flow C records an unknown edge for an unversioned plan dependency; glossary gains `submit`, `wait_for_dependency`, `audit_version`, `stale_edges`, `dependency_version`, `refresh_listing_version`. | phase-5 |
| 2026-09-27 | Reviewed against `design/record-streams/` Phase 5. Current contract gains two bullets from its review fix: a `cached: false` keyed asset stays its key's graph node through `bound_owner_key` when no other asset is registered, and `expire_dependencies_result` marks the stored copy of an expired key no registered asset holds `Expired` (from `Ready`/`Override` only). The rest of the contract was not re-verified beyond what these touch. | phase-5 |
| 2026-09-06 | Computed keyed assets now carry a concrete version, assigned atomically with their status; `add_dependency` records without verifying and the graph does no I/O; verification moves to opt-in `trigger_dependency_audit*` with a default of never; edges carry the dependent's expected version so a change expires only what it provably affects; `Version(0)` means "unknown" and nothing else. Current-contract bullets rewritten, Flow A step 3 and Flow B step 4 corrected. | `specs/design/keyed-expiry-cascade-fix/` |
| 2026-08-12 | Delegation no longer records a dependency: two assets sharing a key are one graph node, compared by construction-time key rather than by the mutable resolved recipe (PR 32 review). New section "Delegation is a hand-off, not a dependency"; F-1 bullet, Flow A step 3 and the `record_dependency_on_asset` glossary entry corrected. Reviewed only for the delegation-recording claim — Flow A steps 5, 7 and 8 still describe the pre-2026-07-15 wait mechanics and are superseded by "Non-blocking dependency scheduling"; not re-verified here. | `specs/design/keyed-delegation-hand-off/` |
| 2026-07-15 | Last substantive edit, carried into `reference/` unchanged. Not reviewed against the implementation since. | migration |
