---
id: INLINE-MANAGER-FLAT-DYNAMIC-DEPENDENCY-RUNS
kind: design
title: The inline manager runs every dependency from the top-level caller, so no chain nests
form: compact
status: in_review
phase: implementation
readiness: needs-decision
autofix: not-eligible
area: [core/assets]
issues: [INLINE-MANAGER-DYNAMIC-DEPENDENCY-CHAIN-NESTS]
created: 2026-10-20
---
# The inline manager runs every dependency from the top-level caller, so no chain nests

Bulk design (`specs/guides/autonomous_bulk_design.md`), 2026-10-20: Phases 1-4 written, not
approved, nothing implemented. Residual limit of `dependency-resolved-once-per-evaluation`
(complete), which flattened only the *static* upstream by priming.

## Phase 1: High-Level Design

### Purpose

On `ImmediateAssetManager`, stack depth must not grow with chain length for any kind of
dependency. Priming covers plan-level reads served by a recipe; a dependency a command requests at
run time, a volatile key, and the dependencies of an `apply_queued` asset still run nested, one
`run_inline` per link, inside their dependent's poll.

### Problem Example

Verified at HEAD (`liquers-core/src/assets.rs`): `ImmediateAssetManager::get_dependency_asset` →
`run_dependency` → `asset.run_inline(None).await` inside the parent's run;
`get_dependency_asset_with_payload`, `apply_for` and `apply_queued` likewise; `prime_upstream`
skips `node.volatile` and sees only `upstream_graph`.

Chain fixture (`liquers-core/tests/common/chain_fixture.rs` style): `data/l0.txt` = `make_text/l0.txt`,
and for `i` in `1..=100` `data/l{i}.txt` = `dyn_upper-{i-1}/l{i}.txt`, where `dyn_upper` is an async
command that requests `-R/data/l{n}.txt` through `Context::get_dependency_state` and upper-cases it
(`liquers-validate --command dyn_upper -- 'dyn_upper-99/l100.txt'`: `Ok`, one `Action` with the
string parameter `99`, filename `l100.txt`).

```rust
on_small_stack(|| futures::executor::block_on(async {       // 1 MiB thread, no runtime
    outcome(&envref, "-R/data/l100.txt").await              // today: stack overflow, abort
}));
```

Should be: returns `TEXT`, `dyn_upper` ran 100 times, as the static chain does since priming
(`deep_chain_evaluation.rs` `immediate_cold_100_link_chain_without_runtime_on_1mib_stack`). The same
for a chain whose links are all volatile.

### Scope and Acceptance Criteria

- **AC-1** Dynamic chain, small stack
  WHEN a cold 100-link chain whose links request their input at run time is evaluated on the inline manager under `futures::executor::block_on` on a 1 MiB thread
  THEN it returns the value and each link ran once
- **AC-2** Volatile chain, small stack
  WHEN a 100-link chain of volatile keys is evaluated the same way
  THEN it returns the value and each link ran once
- **AC-3** Finished on return
  WHEN a top-level `get`, `get_asset`, `apply` or `apply_queued` returns on the inline manager
  THEN every dependency run it caused has finished (none is left running or queued)
- **AC-4** Once per evaluation, failures fail fast
  WHEN a dynamic chain's root fails
  THEN the root ran once, the top-level returns its error, and the evaluation-session suites (`evaluation_session.rs` `*_inline`) stay green
- **AC-5** Static chains unchanged
  WHEN the existing `deep_chain_evaluation.rs` inline tests run
  THEN they pass unchanged
- **AC-6** Two top-level evaluations of one asset
  WHEN two tasks evaluate the same cold dynamic chain at once
  THEN each link runs once and both return the value

Non-goals: the queued manager (spawns each run already); removing priming (it stays; a follow-up
may retire it once this lands); wasm `spawn_local`; making a command's own recursion
(`get_dependency_state` of itself) legal.

### Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — readiness is not `ready` (rule 5), and it adds a private
  run-queue type to `EvaluationSession` (rule 3).
- **Leading issue:** **Open design - orphaned runs.** A dependency run queued for a parent whose
  future is later dropped (a command that times out its own `get_dependency_state`) keeps running
  in the driver. Recommended: the driver drains it before the top-level returns (as the queued
  manager lets a detached child finish). Alternative: drop it with the top-level's remaining work,
  leaving its asset re-claimable through `InlineRunClaim`'s repair. Either is consistent with AC-3.
- **Explanation:** a session-scoped driver polls every queued run at a constant depth from the
  top-level caller, with no executor; `dependency-resolved-once-per-evaluation` rejected a task per
  dependency because `spawn_local` breaks "finished on return", which a driver that drains keeps.
- **Open questions:**
  1. **Open design - orphaned runs:** as above; recommended: drain.
  2. **Proposed resolution - mechanism:** a driver in the evaluation session rather than the
     issue's "prime on demand", which flattens only the static upstream of a dynamic link and
     still nests a chain whose every link is dynamic (AC-1 fails under it).

### Design Dependencies

- `dependency-resolved-once-per-evaluation` — **requires** (complete, E4; not extended): the
  session, pinning and `prime_upstream`.
- `inline-run-claim-repair-under-lock-contention`, `inline-waiter-recovers-dropped-run` —
  **overlaps**: queued runs still claim through `run_inline` / `InlineRunClaim`; a dropped driver
  drops its runs and their claims repair as today. No order.
- `external-manager-evaluation-session-sharing` — **overlaps** (in_review): it changes how a
  dependency adopts its parent's session; the driver lives in the session, so whichever lands
  second rebases `EvaluationSession`. No order.
- `environment-manager-reference-cycle` — **overlaps**: the driver holds the session strongly only
  while it runs; no new cycle.

### Consolidated Findings

- Depth: a queued run is polled by `FuturesUnordered` inside the driver, so the stack holds the
  top-level caller, the driver and one run. A dependent waiting for it awaits a oneshot and
  returns `Pending`; the driver wakes it on completion.
- The driver must refuse a job once it has decided to stop (one lock covers "queue empty" and
  "inactive"), or a job enqueued from another task in that instant would never run.
- `apply_queued` becomes a driver entry like `get`, which also covers the case the issue says
  priming left out.
- A session already being driven (an asset re-entering through an adopted session) runs its new
  top-level future directly; its requests go to the active driver.

## Phase 2: Architecture

### Solution

**`liquers-core/src/evaluation_session.rs`.** `EvaluationSession` gains a private
`inline_runs: InlineRuns` holding an unbounded `futures::channel::mpsc` of
`crate::maybe_send::BoxFuture<'static, ()>` and, under one `std::sync::Mutex`, the receiver and an
`active` flag. New crate-private methods:

```rust
/// Runs `top` and every run queued while it runs, all polled from this future; returns once
/// `top` has finished and the queue is empty. Runs `top` directly if the session is already driven.
pub(crate) async fn drive<T>(&self, top: impl Future<Output = T>) -> T;
/// Queues `run` on the active driver and waits for its result; `Err(run)` when none is active.
pub(crate) async fn run_on_driver<T: MaybeSend + 'static>(
    &self, run: BoxFuture<'static, T>) -> Result<T, BoxFuture<'static, T>>;
```

`drive` takes the receiver, sets `active`, and loops with `futures::select_biased!` over `top`, the
receiver (push into a `FuturesUnordered`) and the `FuturesUnordered`; after `top` finishes it keeps
polling until the queue is empty and the set is drained, then, under the lock, confirms the
receiver is empty, clears `active` and puts the receiver back. `run_on_driver` pushes
`async move { let _ = tx.send(run.await); }` (a `futures::channel::oneshot`) under the lock only if
`active`, and awaits the oneshot; a cancelled oneshot (the driver was dropped) is
`Error::general_error("the evaluation was dropped")`.

**`liquers-core/src/assets.rs`, `ImmediateAssetManager`.**

- Driver entries: `get` (keyed), `get_query_asset_run` for `QueryAssetOrigin::TopLevel`, `apply`
  and `apply_queued` wrap their `prime_upstream` + `run_inline` in `session.drive(async { … })`.
- Queued runs: `run_dependency`, `apply_for` and `get_dependency_asset_with_payload` take the
  parent's session (`parent.evaluation_session()`); when present, the run is
  `session.run_on_driver(Box::pin(async move { asset.run_inline(payload).await }))`; on
  `Err(run)` (no driver, e.g. a direct `AssetRef::run_inline` by a caller) it is awaited in place,
  as today. Result handling (`Ok(asset)` for an `Error`/`Cancelled` dependency) is unchanged.
- `prime_upstream` runs inside the driver and is otherwise unchanged.

Rejected alternatives:

- **Prime on demand** (the issue's suggestion): a chain of dynamic links has no static upstream to
  prime; AC-1 still overflows.
- **`wasm_bindgen_futures::spawn_local` / a task per dependency:** rejected before (no executor
  natively; breaks "finished on return").
- **`stacker`, a depth limit, more boxing:** rejected before (wasm, failing deep chains, constant
  factor only).

Known-issue preflight: `CORE-TOKIO-REMOVAL` (P3) — the driver uses `futures` only.
`EXTERNAL-MANAGER-CANNOT-SHARE-AN-EVALUATION-SESSION` (design in_review) — touches session
adoption, not runs. Nothing blocks.

### Changes

- `liquers-core/src/evaluation_session.rs`: private `InlineRuns`, field, `drive`, `run_on_driver`.
- `liquers-core/src/assets.rs`: `ImmediateAssetManager::{get, get_query_asset_run, apply,
  apply_queued, run_dependency, apply_for, get_dependency_asset_with_payload}`.
- No `pub` item, trait, command, registry or format change. `futures` is already a dependency.
  Futures stay `MaybeSend` (native `Send`, wasm none).
- Documents: `specs/reference/DEPENDENCIES_STATUS.md` (evaluation session: the inline driver; the
  priming paragraph's residual-limit sentence), `specs/reference/api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md`
  (inline manager execution), each with a History row and `reviewed:` bump.

### Risks

| Risk | Mitigation | Certainty |
|---|---|---|
| A job enqueued as the driver stops is never run (hang) | one lock for "empty" and "inactive"; AC-6 from two tasks | medium |
| Deadlock: a parent awaits a run the driver never polls | every run of a driven session goes to that driver; the driver polls `top` and the set alike | medium |
| `Send` bounds on native (`async_trait` futures) | `BoxFuture` alias, `FuturesUnordered` of `Send` futures | high |
| Changed timing breaks the inline suites | `evaluation_session`, `deep_chain_evaluation`, `manager_parametric`, `dependency_scheduling` run on both managers | medium |
| Orphaned runs delay the return | decision 1 | high |
| wasm32 | `scripts/check-build-matrix.sh`; liquers-web conformance loop | high |

Recovery: the driver entry and the queued-run branch are separable; reverting both restores nested
runs.

## Phase 3: Examples and Tests

### Examples

The Problem Example is AC-1's test. Fixture additions in `liquers-core/tests/common/chain_fixture.rs`:
`dyn_chain(root, n)` (`l0 = <root>/l0.txt`, `l{i} = dyn_upper-{i-1}/l{i}.txt`),
`volatile_chain(root, n)` (`chain` with every entry `volatile`), and the async command `dyn_upper`
(argument `n: String`, counted in `Counters::upper`).

### Tests

In `liquers-core/tests/deep_chain_evaluation.rs` (its own binary; a stack overflow aborts it):

- `immediate_dynamic_100_link_chain_without_runtime_on_1mib_stack` — `TEXT`, `upper == 100`, and
  `lookup_key_asset(data/l1.txt)` is `Ready`. Covers AC-1, AC-3. Aborts at HEAD.
- `immediate_volatile_100_link_chain_without_runtime_on_1mib_stack` — `TEXT`, `upper == 100`. Covers
  AC-2.
- `immediate_failing_root_of_dynamic_100_link_chain_runs_once` — `dyn_chain("fail", 100)`: error
  `leaf failed`, `fail == 1`. Covers AC-4.
- Existing `immediate_cold_100_link_chain_without_runtime_on_1mib_stack`,
  `immediate_failing_root_of_100_link_chain_runs_once`, `immediate_warm_upstream_is_not_reloaded`.
  Cover AC-5.

In `liquers-core/tests/evaluation_session.rs`:

- `concurrent_dynamic_chain_evaluations_run_each_link_once_inline` — `multi_thread`; two
  `tokio::spawn`ed `outcome(&envref, "-R/data/l20.txt")` on `dyn_chain("make_text", 20)`, both
  `TEXT`, `upper == 20`. Covers AC-6.
- Existing `*_inline` tests. Cover AC-4.

Unit, `evaluation_session.rs` `mod tests`:

- `drive_polls_queued_runs_and_drains_before_returning` — `top` queues two runs that each queue one
  more; all four complete before `drive` returns. Covers AC-3.
- `run_on_driver_without_driver_returns_the_run` — `Err(run)` when no driver is active. Covers AC-3.

Run: `CARGO_INCREMENTAL=0 cargo test -p liquers-core --test deep_chain_evaluation --test
evaluation_session`, then `cargo test -p liquers-core --lib --tests`.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `liquers-core/tests/common/chain_fixture.rs` — `dyn_chain`, `volatile_chain`, `dyn_upper`; the new tests in `deep_chain_evaluation.rs` and `evaluation_session.rs`; the dynamic and volatile 100-link tests abort at HEAD — `cargo test -p liquers-core --test deep_chain_evaluation`. Rollback: delete the tests and fixture additions.
- [ ] 2. `liquers-core/src/evaluation_session.rs` — `InlineRuns`, `drive`, `run_on_driver` and their unit tests — `cargo test -p liquers-core --lib evaluation_session`. Rollback: revert.
- [ ] 3. `liquers-core/src/assets.rs` `ImmediateAssetManager` driver entries (`get`, `get_query_asset_run`, `apply`, `apply_queued`) — depends on 2 — `cargo test -p liquers-core --lib --tests`. Rollback: revert.
- [ ] 4. `assets.rs` queued runs (`run_dependency`, `apply_for`, `get_dependency_asset_with_payload`) — depends on 3 — step 1 tests pass; `cargo test -p liquers-core --lib --tests`. Rollback: revert steps 3-4 together.
- [ ] 5. Decision 1 as decided (drain, or drop on `top` completion) — in `drive` — `drive_polls_queued_runs_and_drains_before_returning` adjusted to the decision.
- [ ] 6. Docs: `DEPENDENCIES_STATUS.md`, `DOC_03_ASSETS_EXECUTION_LIFECYCLE.md` (History rows, `reviewed:`); close `INLINE-MANAGER-DYNAMIC-DEPENDENCY-CHAIN-NESTS` — `python3 scripts/docs_index.py --check`.

### Validation

`cargo test -p liquers-core --lib --tests`; `cargo test -p liquers-lib --lib --tests`;
`bash scripts/check-build-matrix.sh` (wasm32 rows); after `cargo clean`,
`cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles` (and with
`records`); `python3 scripts/docs_index.py --check`; diff review for edits outside the two source
files, the tests and the two references.
