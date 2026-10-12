---
id: DEEP-CHAIN-EVALUATION-STACK-DEPTH
kind: design
title: Evaluation stack depth independent of recipe-chain length
form: compact
status: superseded
area: [core/assets, core/plan]
issues: [EVALUATING-A-DEEP-CHAIN-TOP-DOWN-OVERFLOWS-THE-STACK]
affects_docs: [reference/DEPENDENCIES_STATUS.md, reference/api/DOC_04_ENVIRONMENT_CONTEXT_EVALUATION.md, guides/ASSET_MANAGER_IMPLEMENTATION_GUIDE.md]
created: 2026-10-12
superseded_by: dependency-resolved-once-per-evaluation
---
# Evaluation stack depth independent of recipe-chain length

> **Superseded on 2026-10-12** by [`dependency-resolved-once-per-evaluation`](../dependency-resolved-once-per-evaluation/DESIGN.md).
> The maintainer merged the two designs: decision 1 below was refined to cover a whole top-level
> evaluation, and an *evaluation session* now carries it. The session is the mechanism both designs
> share. This design's AC-1, AC-2, AC-3, AC-5 and AC-6 are AC-9 to AC-13 there. Its AC-4 is replaced
> by AC-1 to AC-3, and decisions 2 and 3 are that design's decision 4. The merged design owns
> `EVALUATING-A-DEEP-CHAIN-TOP-DOWN-OVERFLOWS-THE-STACK`. This folder is kept for the reasoning; do
> not implement from it.

Bulk design (`specs/guides/autonomous_bulk_design.md`): Phases 1-4 written, not approved, not
implemented.

## Phase 1: High-Level Design

### Purpose

A cold evaluation of the end of a keyed-recipe chain nests each dependency's evaluation inside its
dependent's in one task, so the stack grows with the chain until the process aborts. Make the depth
independent of chain length under both managers, natively and on wasm, without changing what runs,
how often, or the error a caller sees.

### Problem Example

`data/recipes.yaml` holds `l0.txt = make_text` and `l{i}.txt = -R/data/l{i-1}.txt/-/upper`. Nothing
is evaluated yet. Then:

```rust
let asset = envref.evaluate("-R/data/l30.txt").await?;   // SimpleEnvironment, debug build
asset.get().await?;
```

Today: `has overflowed its stack`, process aborted. Measured at HEAD, 2026-10-12 (debug,
`current_thread` runtime on a 2 MiB thread):

| Manager | Works up to | Aborts at | Per nested link |
|---|---|---|---|
| queued (`DefaultAssetManager`, capacity 4) | 26 | 27 | ~90 KiB; release: 190 ok, 200 aborts (~10 KiB) |
| queued, `multi_thread`, 2 MiB workers | 25 | 30 | |
| queued, capacity 1000 | 200 (not exceeded) | — | every dependency is its own task |
| inline (`ImmediateAssetManager`) | 9 | 10 | ~200 KiB; release: 35 ok, 40 aborts (~55 KiB) |

Should: `"TEXT"` for any length, with each `upper` run once. The capacity-1000 row is the evidence
for the queued fix. A browser's 1 MiB wasm stack makes the inline row worse there.

### Scope and Acceptance Criteria

- **AC-1** Deep cold chain, queued manager
  WHEN a 100-link chain's last link is evaluated cold under `SimpleEnvironment` on a 1 MiB stack, at default capacity and at capacity 1
  THEN it returns `"TEXT"`, each link's command runs once, nothing overflows
- **AC-2** Deep cold chain, inline manager without a runtime
  WHEN the same runs under `ImmediateEnvironment` in `futures::executor::block_on` (no tokio) on a 1 MiB stack
  THEN it returns `"TEXT"`, each link's command runs once, nothing overflows
- **AC-3** Warm upstream is not reloaded
  WHEN `l0..l99` are persisted and a fresh `ImmediateEnvironment` over that store evaluates `-R/data/l100.txt`
  THEN only `l100`'s command runs and no link before `l99` becomes a live asset
- **AC-4** An upstream failure still propagates
  WHEN `l0` fails in a 100-link chain (inline) or a 2-link chain (queued) evaluated cold
  THEN it fails with `l0`'s own error; inline, the failing command runs once
- **AC-5** Cycles are still reported
  WHEN a link of a 40-link recipe cycle is evaluated, under either manager
  THEN it fails with HEAD's `Circular dependency detected` error, without hanging or overflowing
- **AC-6** Cancellation keeps its contract
  WHEN, at job capacity 1, a dependent waiting for a locally queued dependency is cancelled
  THEN the dependent ends `Cancelled` and the dependency still finishes `Ready` (`DEPENDENCIES_STATUS.md` Flow D)

Non-goals: chains built from *dynamic* dependencies (a command calling `Context::get_dependency_state`)
and chains through a volatile key, under the inline manager; payload-bearing dependencies
(nesting follows the query); analysis cost (`DEPENDENCY-ANALYSIS-REWALKS-UPSTREAM-PER-EVALUATION`).

Systems: `liquers-core` (`assets.rs`, `plan.rs`); `liquers-web` inherits the inline fix. No command,
syntax, format or `pub` signature changes.

### Design Readiness

- **Readiness:** ready
- **Automatic fixing:** not-eligible — the inline half adds an evaluation phase rather than restoring
  behaviour (rule 2).
- **Leading issue:** None
- **Explanation:** Both mechanisms fit the inspected code (the queued one is measured); the one
  observable choice was decided by the maintainer on 2026-10-12, and it is what Phases 3-4 assume.
- **Open questions:** None.
- **Decided 2026-10-12 (maintainer):**
  1. **Dependency evaluation rule.** Within one evaluation of an asset, each dependency is
     evaluated **at most once**. When a dependency fails, its dependents fail as quickly as
     possible, without re-running it. An asset in status `Error` may be evaluated again by a
     *later* evaluation (a command can be acceptably non-deterministic without being volatile, such
     as an LLM call, or fail on a network error), so a failure is not remembered across
     evaluations. Re-running failing commands, as the Python reference implementation did, is the
     behaviour to avoid.
  2. **Inline failure outcome (follows from 1).** When priming fails at `l_k`, the top-level call
     returns `l_k`'s error at once; no fallback to the normal run, which would run `l_k` again
     and nest `n-k` deep. `l_{k+1}..l_n` stay unevaluated in memory; for the next evaluation that
     is equivalent to `Error`, since both are evaluated afresh.
  3. **Implementation detail - two mechanisms:** a task per dependency (queued, keeps parallelism),
     bottom-up priming (inline, cannot spawn). Resolved in Phase 2.

### Design Dependencies

- `dependency-chain-analysis-cost` (complete) — **requires**: priming reuses its iterative
  `DependencyWalk`; nothing to order.
- `dependency-scheduling` (complete) — **overlaps**: local queues and the no-deadlock argument stay;
  only *where* a locally queued run executes changes.
- `asset-cancellation-outcome` (`in_review`) — **overlaps**: AC-6 re-checks its Flow D promise; its
  tests must stay green.
- `core-value-enum-oversized` (draft) — **overlaps**: smaller frames raise the limit by a constant.

No strong overlap (T1-T4): no open design edits `drain_dependencies`, `wait_for_dependency` or
`ImmediateAssetManager::get`. - `FAILING-UPSTREAM-RERUNS-EXPONENTIALLY-AT-QUEUE-CAPACITY` (issue, P1; design `dependency-resolved-once-per-evaluation`, which this design is required-by for AC-4) —
  **overlaps** (T2, shared contract: decision 1 is its expected behaviour). Not merged: its cause
  sits in `DefaultAssetManager::get_dependency_asset` and needs its own root-cause analysis, and
  this design neither fixes nor worsens it. Once it is fixed, AC-4's queued chain is lengthened
  and gains the run-once count.

### Consolidated Findings

- **Where the recursion is.** Queued: `DefaultAssetManager::drain_dependencies` and the direct-claim
  branch of `wait_for_dependency` await `Box::pin(dep.run(None))` inside the dependent's future once
  the queue is full (`JobQueue::try_to_start_immediately` spawns only below capacity), so every link
  after the fourth nests. Inline: `ImmediateAssetManager::get` → `run_inline` → plan pre-pass
  (`interpreter.rs` `schedule_plan_dependencies`) → `get_dependency_asset` → `get` → `run_inline`.
- **New defect, filed as `FAILING-UPSTREAM-RERUNS-EXPONENTIALLY-AT-QUEUE-CAPACITY`:** at queue capacity a failing upstream reruns exponentially: with
  `l0 = fail`, `fail` ran 2, 4, 8, 16, 29, 54, 240 times for n = 3..8, 10 (n = 20: over 60 s); once
  at capacity 1000 and inline. Likely cause: the plan pre-pass drains the dependency to `Error`, the
  `GetAsset` step re-resolves it, and `DefaultAssetManager::get_dependency_asset` evicts the fresh
  `Error` and recomputes. It violates decision 1.
  Not fixed here; AC-4 keeps the queued chain at 2 links because of it.
- **Residual limit:** dynamic and volatile inline chains still nest (Phase 4 steps 6-7).

## Phase 2: Architecture

### Solution

**Queued manager: every locally queued run gets its own task.** In
`DefaultAssetManager::drain_dependencies` and the direct-claim branch of `wait_for_dependency`,
replace `Box::pin(dep.run(None)).await; claim.complete()` with a `tokio::spawn` of both and await the
`JoinHandle`. The dependent still waits, so the run borrows its slot as before: `running_count`
unchanged, the drain sequential, the `dependency-scheduling` no-deadlock argument intact.
The child is polled by the runtime, not by the parent, so each task holds one link. A dropped
dependent detaches the handle and the child finishes in its own task: the Flow D outcome without the
`RunClaim` re-park. A `JoinError` (panic) is ignored like today's `let _`; the dropped `RunClaim`
re-parks the dependency and the wait loop re-reads its status.

**Inline manager: prime the static upstream bottom-up.** `ImmediateAssetManager` cannot spawn.
Before a top-level entry runs an asset inline, it settles the static upstream leaves-first, so the
run finds every dependency finished and nests one level:

1. `plan.rs`: `DependencyWalk` (iterative) pops recipes in post-order — the order today's inline
   run uses. Record each popped non-volatile `State`/`Link` read served by a recipe, with its children.
2. `assets.rs`: walk that graph from the root's children: a key live and finished, or served by
   `fast_track`, is *available* and not entered; any other is *needed* and its children are examined.
   So a warm `l99` stops the walk and nothing is loaded that today's run would not load (AC-3).
3. Evaluate the needed keys in post-order, unprimed; each finds its dependencies available. Return
   the first error (decision 2).
4. A walk error (cycle, bad plan) skips priming; the normal run reports it unchanged (AC-5).

Rejected: a task per dependency for the inline manager (no executor natively; wasm `spawn_local`
breaks "finished on return"); priming for the queued manager too (serialises fan-out it runs in
parallel); `stacker` (new dependency, no wasm); more boxing or a smaller `Value` (constant factor);
a depth limit that errors (still fails deep chains).

**Known-issue preflight.** `INLINE-DROP-REPAIR-STRANDS-EXISTING-WAITERS` (P2): the inline run is
unchanged. `REFUSED-DEPENDENCY-RECOMPUTED-TWICE-…` (P3): another path. The exponential-rerun defect:
limits AC-4 only. `CORE-TOKIO-REMOVAL` (P3): the new spawn is in the native-only `DefaultAssetManager`.
None blocks or must go first.

### Changes

- `liquers-core/src/assets.rs`
  - `DefaultAssetManager::drain_dependencies`, `wait_for_dependency`: spawn as above (native-only
    already; the future is the one `JobQueue::try_to_start_immediately` spawns).
  - `ImmediateAssetManager`: split `get` into a private `settle(&self, key) -> Result<Option<AssetRef<E>>, Error>`
    (the eviction, lazy-expiry, finished and `fast_track` part of today's loop) and a private
    `get_unprimed(key)` (settle, else `run_inline`); private `prime_upstream(&self, plan, cwd) -> Result<(), Error>`
    implements steps 2-3. Top-level entries prime first: `get`, `get_query_asset_run` with
    `QueryAssetOrigin::TopLevel`, and `apply` (`apply_immediately` forwards to it).
    `get_dependency_asset` (keyed branch) calls `get_unprimed`, so dependencies never re-prime.
- `liquers-core/src/plan.rs`: `pub(crate) async fn upstream_graph<E: Environment>(envref: EnvRef<E>, plan: &Plan, initial_cwd: Option<Key>) -> Result<Vec<(Key, Vec<Key>)>, Error>`
  (post-order); an `Option<Vec<…>>` recorder field on `DependencyWalk`, filled in `DependencyWalk::run`
  at frame pop. No new type, no `pub` item, no command, no format change.
- Docs: `DEPENDENCIES_STATUS.md` (step 5 "invokes `B.run()` inline", Flow D item 2, summary),
  `DOC_04_…` ("may inline-run locally queued work"), `ASSET_MANAGER_IMPLEMENTATION_GUIDE.md` §11
  (residual limit; an inline manager must not nest per chain link). History rows, `reviewed:` bumps.

### Risks

| Risk | Assessment |
|---|---|
| Concurrency (queued) | Same slot accounting, sequential drain; regression: `dependency_scheduling`, `asset_cancellation`, `assets.rs` `test_jobqueue_*` |
| Cancellation | Detached child instead of re-park; same outcome. AC-6 plus `asset_cancellation` `ac03`, `ac09` |
| Inline cost | One extra walk per cold top-level evaluation (analysis already walks three times) |
| Inline failure | Intermediate links stay unevaluated in memory (decision 2) |
| wasm | No spawn on the inline path: AC-2 runs without a runtime; `liquers-web` Node loop |
| Recovery | Halves revert separately; certainty high (queued), medium-high (inline) |

## Phase 3: Examples and Tests

### Examples

Primary: the Problem Example. Secondary: a link-parameter dependency is primed like a `GetAsset`.
Edge: a volatile key mid-chain is not primed, nor is anything upstream of it.

### Tests

New `liquers-core/tests/deep_chain_evaluation.rs` (own binary, so an abort hides nothing else);
recipes as in `dependency_chain_scaling.rs` `chain_env`; `upper`/`fail` count runs; deep tests run on
a `std::thread` with `stack_size(1 << 20)`.

- `queued_cold_100_link_chain_on_1mib_stack` — default capacity: AC-1
- `queued_capacity_one_cold_100_link_chain_on_1mib_stack` — `with_job_capacity(1)`: AC-1
- `immediate_cold_100_link_chain_without_runtime_on_1mib_stack` — `futures::executor::block_on`: AC-2
- `immediate_warm_upstream_is_not_reloaded` — `upper` ran once, `lookup_key_asset(l98)` is `None`: AC-3
- `immediate_failing_root_of_100_link_chain_runs_once` — error contains `leaf failed`, `fail` ran
  once: AC-4
- `queued_failing_root_of_2_link_chain_propagates` — capacity 1, `leaf failed`, no run count: AC-4
- `cycle_is_reported_by_both_managers` — `l0` reads `l40`; queued under a 20 s timeout: AC-5
- `queued_cancelled_dependent_lets_dependency_finish` — capacity 1, `l0` an async command gated on a
  flag, `l2` evaluated, cancelled once `l1` waits; after release `l1` is `Ready` and `l2` `Cancelled`: AC-6
- `plan.rs` unit test `upstream_graph_is_post_order_without_volatile_keys` — supports AC-2, AC-3

Run: `cargo test -p liquers-core --test deep_chain_evaluation`; regression: `cargo test -p
liquers-core --lib --tests` and the `liquers-web` Node loop.

## Phase 4: Implementation Plan

### Steps

- [ ] 1. `liquers-core/tests/deep_chain_evaluation.rs` — the Phase 3 tests; at HEAD the AC-1, AC-2 and inline AC-4 tests abort (run each by name), the rest pass — `cargo test -p liquers-core --test deep_chain_evaluation`.
- [ ] 2. `assets.rs` `DefaultAssetManager::drain_dependencies`, `wait_for_dependency` — spawn the claimed run and await the handle — `cargo test -p liquers-core --test deep_chain_evaluation queued` and `cargo test -p liquers-core --test asset_cancellation --test dependency_scheduling`. Rollback: restore the inline `Box::pin(...).await`.
- [ ] 3. `plan.rs` `DependencyWalk` recorder, `upstream_graph`, unit test — `cargo test -p liquers-core --lib upstream_graph`. Depends on nothing.
- [ ] 4. `assets.rs` `ImmediateAssetManager::settle`, `get_unprimed`, `prime_upstream`; prime in `get`, top-level `get_query_asset_run`, `apply`; route `get_dependency_asset` and any other internal `self.get(` to `get_unprimed` — `cargo test -p liquers-core --test deep_chain_evaluation --test manager_parametric`. Depends on 3. Rollback: drop the priming calls.
- [ ] 5. Core and wasm — `cargo test -p liquers-core --lib --tests`; after `cargo clean`, `cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles`.
- [ ] 6. Docs — the three Phase 2 documents — `python3 scripts/docs_index.py --check`.
- [ ] 7. Records — file the residual `INLINE-MANAGER-DYNAMIC-DEPENDENCY-CHAIN-NESTS`; close the source issue; update the link-by-link comment in `dependency_audit_integration.rs` `chain_first_process` — `python3 scripts/docs_index.py --check`.

### Validation

Core loop and wasm loop green, `chain_20_links_smoke` within bound; `git diff` shows nothing outside
`liquers-core` and `specs/`.
