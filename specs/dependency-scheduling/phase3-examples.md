# Phase 3: Examples & Use-cases - dependency-scheduling

## Example Type

**User choice:** Conceptual code — illustrative Rust snippets showing the scheduling
API and scenario flows, paired with a concrete test plan (test names, files,
assertions). Implementation lands in Phase 4.

## Overview Table

| # | Type | Name | Purpose / what it checks |
|---|------|------|--------------------------|
| E1 | Example | Diamond, non-blocking | Two independent sub-queries over a shared base are scheduled up front and run concurrently; the shared base executes exactly once (claim). The headline win over today's sequential `get_asset().get()`. |
| E2 | Example | Capacity-bounded fan-out | At queue capacity 1 a parent schedules N deps; #1 starts, the rest land on the parent's local queue and drain inline in the parent's own future — no deadlock, zero extra slots. |
| E3 | Example | Cycle rejected at schedule time | Keyed self-cycle, `K→Q→K` through an expression, and purely-dynamic `K1↔K2` mutual evaluation all fail fast with `Error::dependency_cycle` instead of hanging. |
| U1 | Unit | `try_to_start_immediately` bool | `true`+spawn under capacity; `true` no-spawn when already claimed; `false` at capacity. |
| U2 | Unit | `try_claim_for_run` uniqueness | Exactly one of two concurrent claimers gets `Some`; finished/Processing → `None`; status → `Processing`. |
| U3 | Unit | `RunClaim` complete / Drop | `complete()` disarms (no repair); armed Drop re-parks `Submitted` + notifies. |
| U4 | Unit | local-queue primitives | `push`/`pop`/`take_local_dependency` FIFO, dedup by id, entry removed when empty. |
| U5 | Unit | `register_scheduled_dependency` | Keyed edge registered; all four cycle shapes detected; expression attribution + late-joining parent. |
| U6 | Unit | `submit` parity | Reimplemented-on-`bool` `submit` keeps: no duplicates, respects capacity, immediate when capacity. |
| I1 | Integration | Diamond concurrency | E1 end-to-end via `evaluate`; base command invoked once (call counter). |
| I2 | Integration | Local-queue drain | E2 end-to-end at capacity 1; all deps produced, FIFO order, no hang (timeout). |
| I3 | Integration | Cycle no-hang | E3 end-to-end; each shape returns `Err(dependency_cycle)` within a timeout. |
| I4 | Integration | Volatile once-per-eval | A volatile dep referenced by several steps evaluates once per parent eval; re-eval re-schedules. |
| I5 | Integration | Dependency failure | Failing dep drives the parent to `Error` with dependency context (`fail_due_to_dependency`). |
| I6 | Integration | Status flow | Parent transitions `Processing → Dependencies → Processing` while waiting (observed via notifications). |
| I7 | Integration | Cancellation repair | Parent cancelled mid-drain; `RunClaim` Drop re-parks the dep; a second waiter recovers it. |
| I8 | Integration | Backward compatibility | Existing `async_hellow_world` / q-instruction suites pass unchanged (the `evaluate` surface is stable). |

Additional unit and integration tests carried over from the predecessor **WP-1** plan
(`plan20260707.md`) are listed in the Test Plan below and mapped in
[WP-1 Test Reconciliation](#wp-1-test-reconciliation-predecessor-plan-plan20260707md):
runtime-dependency recording + dedup, `Status::Dependencies` contract, depth-chain
no-deadlock, keyed delegation-cycle, exactly-once parent resume, and shared-child
cancellation.

## Example 1: Diamond dependency, non-blocking concurrent execution

**Scenario:** A `combine` command evaluates two independent sub-queries `left` and
`right`, each of which depends on a shared `base`. With the interpreter pre-pass, both
branches are scheduled before the step loop runs, so they proceed concurrently, and
`base` runs exactly once regardless of how many dependents reference it.

**Context:** The common "diamond" shape — the canonical case where today's sequential
`get_asset(q).await?.get().await?` per step needlessly serializes independent work.

**Code (command side — unchanged surface):**
```rust
// `combine` pulls two sibling queries. It does NOT know about scheduling; it just
// asks the Context for dependency states. The interpreter already pre-scheduled them.
async fn combine(state: State<Value>, context: Context<CommandEnvironment>)
    -> Result<Value, Error>
{
    let l = context.get_dependency_state(&parse_query("base/left")?).await?;
    let r = context.get_dependency_state(&parse_query("base/right")?).await?;
    Ok(Value::from(format!("{}+{}",
        l.try_into_string()?, r.try_into_string()?)))
}
```

**Code (what the interpreter does under the hood, conceptually):**
```rust
// apply_plan pre-pass, before the step loop:
let schedule = schedule_plan_dependencies(&plan, &context).await?; // starts base/left,
//   base/right, and base (up to capacity), storing their AssetRefs by Query
context.evaluate_local_queue().await?; // drain anything parked at capacity, inline
// do_step then, per step, waits on the SAME captured AssetRef:
let asset = schedule.asset(&q).expect("known dep");
let state = context.wait_for_dependency(asset).await?;
```

**Expected output:**
```
combine over "base/left" and "base/right" => "L+R"
# `base` command body invoked exactly once (shared claim); left and right ran
# concurrently rather than one-after-the-other.
```

## Example 2: Capacity-bounded fan-out with local-queue fallback

**Scenario:** Queue capacity is 1. A parent schedules three dependencies. The first is
started immediately on the one slot; the other two get `false` from
`try_to_start_immediately` and are pushed onto the parent's local dependency queue.
`evaluate_local_queue` then claims and runs them *inline inside the parent's own
future* (consuming no additional queue slots), so evaluation completes with no
deadlock even though global capacity is exhausted.

**Context:** Proves the progress guarantee at the hardest setting (capacity 1) — the
reason the design uses per-dependent local queues rather than only global parking.

**Code (manager path, conceptual):**
```rust
// DefaultAssetManager::get_dependency_asset, per scheduled dependency:
let asset = /* resolve once, volatile-safe */;
if !job_queue.try_to_start_immediately(&asset).await? { // false = no capacity
    asset.submitted().await?;                            // "queued" (local, not global)
    job_queue.push_local_dependency(parent.id(), &asset);
}
// later, drain_dependencies(parent): pop each, try_claim_for_run, inline run:
while let Some(dep) = job_queue.pop_local_dependency(parent.id()).await {
    if let Some(claim) = dep.try_claim_for_run(&job_queue).await? {
        parent.enter_dependencies(&dep).await?;
        let _ = Box::pin(dep.run()).await; // inline; failures surface at wait time
        claim.complete();
    } // None => already running/finished elsewhere; skip
}
parent.leave_dependencies_and_resume().await?;
```

**Expected output:**
```
3 dependencies produced at queue capacity 1; no hang.
running_count never exceeds 1 (inline drains use the parent's future, not a slot).
Drain order is FIFO of the schedule order.
```

## Example 3: Cycle rejected at schedule time (no hang)

**Scenario:** Three shapes that either hang or (for the purely-dynamic keyed case)
deadlock today are rejected the moment the offending dependency is scheduled:
1. keyed self-cycle — `k` whose recipe evaluates `k`;
2. `K → Q → K` — keyed `k` evaluates expression `q`, whose command evaluates `k`;
3. purely-dynamic mutual — keyed `k1` evaluates `k2` and `k2` evaluates `k1`, with no
   pre-registered edges (the case that slips past today's checks and deadlocks).

**Context:** The cycle-handling correctness goal from Phase 1/DESIGN.md — enforced via
`register_scheduled_dependency` under the keyed-expansion model, reusing the existing
`would_create_cycle`, with no second detector.

**Code (conceptual):**
```rust
// schedule_dependency_asset → register_scheduled_dependency(dependent, dependency, v):
//   dependent=Keyed(k1), dependency=Keyed(k2) registers k1->k2 with a cycle check;
//   when k2 later schedules k1, would_create_cycle(k2, k1) is true → Err.
let err = context.get_dependency_state(&parse_query("k2")?).await.unwrap_err();
assert_eq!(err.error_type, ErrorType::DependencyCycle);
```

**Expected output:**
```
Each cycle shape returns Err(Error::dependency_cycle(..)) promptly; no test timeout.
```

## Corner Cases

### 1. Memory
- `local_deps` entries are created lazily only on the no-capacity fallback and removed
  when the dependent drains them or reaches a terminal status → zero per-asset cost on
  the common path. Verify: after a capacity-1 fan-out completes, `local_deps` holds no
  entry for the parent id (`take_local_dependencies(parent_id)` returns empty).
- Volatile children never enter the manager maps; leftover cleanup classifies them
  non-shared and discards them (debug log) → no cache pollution, no leak.
- `PlanDependencySchedule` and its AssetRefs are dropped at the end of `apply_plan`.

### 2. Concurrency
- **Execute-once:** two paths racing `try_claim_for_run` on one asset — exactly one
  gets the claim and runs; the other observes `None` and waits. Closes the existing
  double-run window (`run_with_future` guards only `is_finished()`, assets.rs:1373).
- **Shared dep, two parents at capacity:** both local queues hold the same AssetRef;
  the claim arbitrates; the loser's drain skips it and its wait loop subscribes — no
  double execution, no lost wakeup (authoritative state re-checked after every claim
  failure and every `rx.changed()`).
- **Cancellation liveness:** a parent future dropped mid-inline-drain leaves the dep
  `Processing`; `RunClaim` Drop (armed) resets it to `Submitted`, `submit`s it
  globally, and notifies, so another waiter recovers it.
- `local_deps` mutex is never held across an `.await` of asset execution (pop releases
  the lock before `run`).

### 3. Errors
- Dependency reaches `Error`/`Cancelled` → `wait_for_dependency` extracts the stored
  error (or constructs a dependency-failure error with asset id + query), calls
  `parent.fail_due_to_dependency(e)`, returns `Err` — parent ends `Error` with context.
- Inline run failure during `drain_dependencies` is logged on the parent and draining
  continues; the failure surfaces at the wait of whichever waiter needs that asset.
- Cycle at schedule time → `Error::dependency_cycle` aborts the schedule (no partial
  edge left registered).
- `RunClaim` Drop repair must never panic (best-effort, logged).

### 4. Serialization
- None of the new types (`RunClaim`, `PlanDependencySchedule`, `local_deps`) is
  serialized — all runtime-only, no serde derives. `Metadata`/`DependencyRecord`
  serialization is unchanged (regression-guard the existing round-trip tests).
- Observable protocol change: the status flow gains a truthful
  `Processing → Dependencies → Processing` transition while waiting; documented in the
  `DEPENDENCIES_STATUS.md` update.

### 5. Integration
- Interpreter: pre-pass + `do_step` handle lookup; `Context::evaluate` reimplemented on
  `schedule_dependency_asset` + `evaluate_local_queue` (public signature unchanged).
- `evaluate_recipe` pure-key delegation migrates onto `wait_for_dependency` (F-1 inline
  guard retired onto the shared claim-based primitive).
- `AssetManager` trait extension methods have default impls → `liquers-py` wrappers and
  any exotic managers keep compiling (`cargo check -p liquers-py`).
- `liquers-store` / `liquers-lib` / `liquers-axum`: behavior-transparent, no code change.

## Test Plan

### Unit Tests (inline `#[cfg(test)] mod tests` — `assets.rs`, `dependencies.rs`)
Follow existing JobQueue unit-test conventions (`JobQueue::new(capacity)`, `submit`,
`run()`, `running_count()`, the asset-builder helper at assets.rs:2369).

| Test | Asserts |
|------|---------|
| `test_try_to_start_immediately_starts_under_capacity` | returns `true`, spawns, `running_count == 1` |
| `test_try_to_start_immediately_false_at_capacity` | returns `false`, no spawn, asset left for local parking |
| `test_try_to_start_immediately_true_when_already_active` | returns `true`, does NOT double-spawn (claim already held) |
| `test_try_claim_for_run_unique_under_race` | two concurrent claims → exactly one `Some`; status becomes `Processing` |
| `test_try_claim_for_run_none_when_finished_or_processing` | explicit status match returns `None` |
| `test_runclaim_complete_disarms` | after `complete()`, dropping does no repair |
| `test_runclaim_drop_reparks_when_armed` | armed Drop → status back to `Submitted`, present in `jobs`, notify sent |
| `test_local_dependency_fifo_dedup_and_removal` | push/pop FIFO, dedup by asset id, entry gone when empty; `take_*` returns leftovers |
| `test_register_scheduled_dependency_keyed_edge` | `k1→k2` registered; `would_create_cycle` sees it |
| `test_register_scheduled_dependency_detects_all_cycle_shapes` | self, `K→Q→K`, `K2→Q→K2` late-join, `Q1→Q2→Q1` → `Err(dependency_cycle)` |
| `test_submit_bool_parity` | no-duplicates / respects-capacity / immediate-when-capacity (mirrors existing `test_jobqueue_submit_*`) |
| `test_dependencies_status_has_no_data` *(WP-1)* | asset set to `Status::Dependencies` → `poll_state()` is `None` and status is not finished (pins the "no extra waiting status, Dependencies has no data" principle) |
| `test_leftover_local_queue_cleanup_at_terminal` *(WP-1)* | at parent terminal status, a shared leftover (present in maps) is re-`submit`ted globally; a non-shared/volatile leftover is discarded — no strong ref retained (`take_local_dependencies` then empty) |

### Integration Tests (`liquers-core/tests/dependency_scheduling.rs`)
`SimpleEnvironment<Value>` + `register_command!`; keyed/cycle cases add a
`MemoryStore` (wrapped via `AsyncStoreWrapper`) + `RecipeProvider` so keyed assets
exist (per `async_hellow_world.rs`). Entry point: `evaluate(envref, "query", None)`.
Cycle / no-hang tests wrap the await in `tokio::time::timeout` so a regression fails
loudly instead of hanging.

| Test | Flow |
|------|------|
| `test_diamond_shared_runs_once` | E1: `combine` over `base/left`+`base/right`; `base` call-counter == 1; result `"L+R"` |
| `test_capacity_one_fanout_drains_local_queue` | E2: capacity 1, 3 deps; all produced, FIFO order, `running_count <= 1`, completes within timeout |
| `test_cycle_self_is_rejected` | E3(1): `Err(dependency_cycle)`, within timeout |
| `test_cycle_keyed_through_expression` | E3(2): `K→Q→K` → `Err(dependency_cycle)` |
| `test_cycle_dynamic_keyed_mutual` | E3(3): `K1↔K2` → `Err(dependency_cycle)` (today's deadlock case) |
| `test_volatile_dependency_evaluates_once_per_eval` | volatile dep referenced twice in one plan → one evaluation; re-`evaluate` → fresh evaluation |
| `test_dependency_failure_propagates_to_parent` | failing dep → parent `Error` carrying dependency context |
| `test_status_flow_processing_dependencies_processing` | gated child not ready → parent **enters** `Status::Dependencies` (`poll_state()` `None`); release child → parent **leaves** and resumes `Processing`→`Ready`; observed via notifications *(subsumes WP-1 enter/leave + delegated-status tests)* |
| `test_cancellation_repairs_stranded_dependency` | drop parent mid-**inline-drain**; second waiter still resolves the dep (`RunClaim` Drop repair) |
| `test_context_evaluate_records_runtime_dependency` *(WP-1)* | command calls `context.evaluate("dep")` / `get_dependency_state`; result metadata contains the `dep` dependency record |
| `test_immediate_and_queued_record_same_dependencies` *(WP-1)* | same command via immediate vs queued evaluation records identical dependency metadata (path-independent) |
| `test_static_and_runtime_dependencies_deduplicated` *(WP-1)* | recipe has static `GetAsset(dep)` and the command also evaluates `dep` → one record, best non-unknown version (pre-pass vs static upsert) |
| `test_dependency_chain_deeper_than_capacity_completes` *(WP-1)* | delegation/dependency chain `a→b→c→…` longer than queue capacity (incl. capacity 1) completes; all `Ready`; within timeout — the depth no-deadlock property |
| `test_delegation_cycle_fails_fast` *(WP-1)* | keyed resource recipes `a.txt→-R/b.txt`, `b.txt→-R/a.txt` (store present) → `Err(dependency_cycle)`, no hang |
| `test_parent_body_runs_once_across_dependency_wait` *(WP-1, reframed)* | parent waits on a child then resumes; parent command body executes exactly once (claim-guaranteed; replaces the old "resubmits parent once") |
| `test_cancel_parent_does_not_cancel_shared_child` *(WP-1)* | parent waits on a **shared** gated child, then parent cancelled → parent `Cancelled`; child still completes and is reusable by another asset |
| `test_parent_cancelled_before_child_completion_not_resubmitted` *(WP-1)* | parent cancelled while waiting; child completes later → parent stays `Cancelled`, no new parent work scheduled |
| `test_backward_compat_existing_suites` | re-assert `async_hellow_world` + q-instruction flows are unaffected |

### WP-1 Test Reconciliation (predecessor plan `plan20260707.md`)

This design supersedes WP-1 Phase 2A (`EvaluationOutcome::Delegated`), so WP-1's
dependency-waiting and scheduler-no-deadlock tests are the behavioral contract this
feature must still satisfy. Each WP-1 test was evaluated for relevance under the new
inline-wait + claim model:

| WP-1 test | Disposition | Where |
|-----------|-------------|-------|
| `test_context_dependency_records_runtime_dependency` | **Incorporated** | `test_context_evaluate_records_runtime_dependency` |
| `test_immediate_evaluation_records_runtime_dependencies` | **Incorporated** | `test_immediate_and_queued_record_same_dependencies` |
| `test_static_and_runtime_dependencies_are_deduplicated` | **Incorporated** | `test_static_and_runtime_dependencies_deduplicated` |
| `test_parent_enters_dependencies_when_context_dependency_not_ready` | **Incorporated (merged)** | `test_status_flow_processing_dependencies_processing` (enter half) |
| `test_parent_leaves_dependencies_after_child_ready` | **Incorporated (merged)** | `test_status_flow_…` (leave half) |
| `test_dependency_error_propagates_to_parent` | Already covered | `test_dependency_failure_propagates_to_parent` (I5) |
| `test_cancel_while_in_dependencies_does_not_cancel_child` | **Incorporated** | `test_cancel_parent_does_not_cancel_shared_child` |
| `test_dynamic_dependency_cycle_fails_via_dependency_manager` | Already covered | `test_cycle_dynamic_keyed_mutual` (I5c) |
| `test_dependencies_status_has_no_data` | **Incorporated** | unit `test_dependencies_status_has_no_data` |
| `test_delegation_chain_deeper_than_capacity` | **Incorporated** | `test_dependency_chain_deeper_than_capacity_completes` |
| `test_delegation_completes_with_capacity_1` | **Incorporated (merged)** | same (chain incl. capacity 1) + `test_capacity_one_fanout_…` (breadth) |
| `test_parent_status_dependencies_while_delegated` | **Incorporated (merged)** | `test_status_flow_…` (delegation variant) |
| `test_delegation_cycle_fails_fast` | **Incorporated** | `test_delegation_cycle_fails_fast` (keyed `-R/` recipes) |
| `test_delegation_error_propagates_to_parent` | Already covered (delegation regression) | `test_dependency_failure_propagates_to_parent` (I5), delegation input |
| `test_dependency_completion_resubmits_parent_once` | **Incorporated (reframed)** | `test_parent_body_runs_once_across_dependency_wait` |
| `test_parent_cancelled_before_child_completion_is_not_resubmitted` | **Incorporated** | `test_parent_cancelled_before_child_completion_not_resubmitted` |
| `test_queue_shutdown_stops_worker` | **Out of scope** | shutdown semantics unchanged by this feature; remains a pre-existing JobQueue regression concern, not re-specified here |
| `test_queue_does_not_retain_finished_assets` | **Incorporated (adapted)** | unit `test_leftover_local_queue_cleanup_at_terminal` (+ existing `test_jobqueue_cleanup_removes_finished`) |

Notes carried from WP-1 discipline (adopted here): red→green ordering (new-API tests
cannot compile until the API exists); `tokio::time::timeout` (10 s) as a hang guard on
every wait/cycle test; deterministic gating via a shared `tokio::sync::oneshot`/
`Semaphore` rather than sleeps; delegation/keyed tests require a `-R/` store
(`MemoryStore` + `RecipeProvider`).

### Manual Validation
```bash
cargo test -p liquers-core dependency_scheduling   # new integration suite
cargo test -p liquers-core                          # full core suite (regressions)
cargo check -p liquers-py                           # trait-default compatibility
cargo check --workspace                             # downstream crates transparent
```
Expected: all green; no test exceeds its `timeout` guard (proves no-deadlock at
capacity 1 and on every cycle shape).

## Auto-Invoke: liquers-unittest Skill Output

Conventions applied from the `liquers-unittest` skill:
- `#[tokio::test]`; test fns return `Result<(), Box<dyn std::error::Error>>` and use `?`.
- `type CommandEnvironment = SimpleEnvironment<Value>;` alias declared before any
  `register_command!` call.
- No `unwrap()`/`expect()` except in tests; no `_ =>` default arms; typed error
  constructors only (`Error::dependency_cycle`, `Error::general_error`).
- Unit tests inline in `assets.rs`/`dependencies.rs`; end-to-end flows in
  `liquers-core/tests/dependency_scheduling.rs`.
- Memory store for keyed cases: `MemoryStore::new(&Key::new())` via `AsyncStoreWrapper`
  + `RecipeProvider`, mirroring `liquers-core/tests/async_hellow_world.rs`.

Concrete template (integration, E1):
```rust
#[tokio::test]
async fn test_diamond_shared_runs_once() -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = SimpleEnvironment<Value>;
    let mut env = SimpleEnvironment::<Value>::new();
    let calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));

    let cr = env.get_mut_command_registry();
    // base counts its invocations; left/right transform it; combine joins the two.
    register_command!(cr, fn base(state) -> result)?;
    register_command!(cr, fn left(state) -> result)?;
    register_command!(cr, fn right(state) -> result)?;
    register_command!(cr, async fn combine(state, context) -> result)?;

    let envref = env.to_ref();
    let state = evaluate(envref, "base/left/q/combine", None).await?; // no spaces
    assert_eq!(state.try_into_string()?, "L+R");
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1); // base ran once
    Ok(())
}
```
(Exact query wiring and the shared-`base` call counter are finalized in Phase 4 when
the commands are written; the assertion contract — single base invocation, concurrent
branches, stable `evaluate` surface — is fixed here.)
