# Phase 4: Implementation Plan - dependency-scheduling

## Overview

**Feature:** dependency-scheduling (non-blocking, asset-local dependency queues)

**Architecture:** All in `liquers-core`. An atomic run-claim (`RunClaim` +
`AssetRef::try_claim_for_run`) makes "who runs this asset" a single CAS decision; the
`JobQueue` gains `try_to_start_immediately -> bool` plus per-dependent local queues used
as a capacity fallback; a `Context` scheduling API (`schedule_dependency_asset` +
`wait_for_dependency`) captures each dependency AssetRef once and cycle-checks at
schedule time via the `DependencyManager`; the interpreter pre-schedules all known plan
dependencies (`PlanDependencySchedule`) before executing steps. No new commands, value
types, endpoints, or crate dependencies.

**API note:** implement per the **approved Phase 2 corrections** — there is NO
`DependencyHandle` and NO public `schedule_dependency`; scheduling is the `pub(crate)`
`Context::schedule_dependency_asset` returning a bare `AssetRef`, and Phase 1's
open-question 2 (handle placement) is moot.

**Estimated complexity:** High (concurrency-critical: claim uniqueness, Drop-based
cancellation repair, no-deadlock at capacity 1).

**Prerequisites:** Phase 1, 2, 3 approved; open questions resolved; no new deps.

**Test discipline (from WP-1, adopted in Phase 3):** red→green per step — write the
step's unit tests first (new-API tests won't compile until the API lands); every
wait/cycle test wraps its await in `tokio::time::timeout` (10 s) as a hang guard; gate
timing deterministically with a shared `oneshot`/`Semaphore`, never `sleep`.

## Implementation Steps

Bottom-up so each step compiles and is independently checkable. Each step's tests are
the Phase 3 tests it makes green.

### Step 1: DependencyManager scheduling & expression expansion

**File:** `liquers-core/src/dependencies.rs`

**Action:**
- Add `enum ScheduleNode { Keyed(DependencyKey), Expression(DependencyKey) }` (`pub(crate)`).
- Add three transient maps to `DependencyManager`: `expression_dependents`,
  `expression_keyed_deps`, `expression_expr_deps`
  (`scc::HashMap<DependencyKey, scc::HashSet<DependencyKey>>`).
- Add `register_scheduled_dependency(&self, dependent: &ScheduleNode, dependency:
  &ScheduleNode, version: Version) -> Result<(), Error>`, private
  `propagate_attribution(...)`, and `remove_expression(&self, expr: &DependencyKey)`.
- **Reuse** existing `would_create_cycle` and `add_dependency` (do NOT add a second
  cycle detector); explicit match on `ScheduleNode`, no default arm.

**Signatures/logic:** Phase 2 §"Cycle Handling" (register/propagate/remove).

**Tests (red first):** unit in `dependencies.rs` — `test_register_scheduled_dependency_keyed_edge`,
`test_register_scheduled_dependency_detects_all_cycle_shapes` (self, `K→Q→K`,
`K2→Q→K2` late-join, `Q1→Q2→Q1`).

**Validation:** `cargo test -p liquers-core dependencies` · `cargo check -p liquers-core`

**Rollback:** `git checkout liquers-core/src/dependencies.rs`

**Agent:** **sonnet** · skills: liquers-unittest · knowledge: Phase 2 Cycle Handling,
DESIGN.md cycle-verification notes, existing `DependencyManager`/`would_create_cycle`/
`DependencyKey` (metadata.rs:254). Rationale: contained graph logic, no async races.

---

### Step 2: Atomic run-claim primitive

**File:** `liquers-core/src/assets.rs`

**Action:**
- Add `pub(crate) struct RunClaim<E>` (`asset`, `queue: Arc<JobQueue<E>>`, `armed`),
  `fn complete(self)`, and `impl Drop` (if armed: spawn repair that resets `Processing`→
  `Submitted`, `queue.submit` + `notify_one`).
- Add `AssetRef::try_claim_for_run(&self, queue: &Arc<JobQueue<E>>) ->
  Result<Option<RunClaim<E>>, Error>` — one `data.write()`; explicit status match
  (claimable: `None|Recipe|Submitted|Dependencies` → set `Processing`, `Some`; else
  `None`).
- Add `AssetRef::leave_dependencies_and_resume(&self) -> Result<(), Error>` — counterpart
  of existing `enter_dependencies` (assets.rs:748); keep `leave_dependencies_for_resubmit`
  (:767) for the genuine resubmission path.

**Invariant established:** `run()`/`run_immediately()` are called only by claim holders.

**Tests (red first):** unit — `test_try_claim_for_run_unique_under_race`,
`test_try_claim_for_run_none_when_finished_or_processing`,
`test_runclaim_complete_disarms`, `test_runclaim_drop_reparks_when_armed`.

**Validation:** `cargo test -p liquers-core claim` · `cargo check -p liquers-core`

**Rollback:** `git checkout liquers-core/src/assets.rs`

**Agent:** **opus** · skills: liquers-unittest · knowledge: Phase 2 RunClaim + AssetRef
sections, `run_with_future` finished-only guard (assets.rs:1373), status machine /
`enter_dependencies` (:748) / `fail_due_to_dependency` (:779), notification channel.
Rationale: cancellation/Drop safety and claim uniqueness are the correctness core.

---

### Step 3: JobQueue refactor over `try_to_start_immediately`

**File:** `liquers-core/src/assets.rs` (`JobQueue`, ~3483)

**Action:**
- Add field `local_deps: Arc<Mutex<HashMap<u64, VecDeque<AssetRef<E>>>>>` (init in `new`).
- Add `try_to_start_immediately(&self, asset) -> Result<bool, Error>` (dedup-register in
  `jobs`; CAS-reserve slot; `try_claim_for_run`; on claim `tokio::spawn` run→complete→
  decrement→notify→`cleanup_local_dependencies`; release reserved slot if claim fails).
  `true` = started-or-already-active, `false` = no capacity.
- Reimplement `submit` over it (`if !try_to_start_immediately {...park globally...}`).
- Reimplement worker `run()` over `try_to_start_immediately` + claim (removes the
  TOCTOU between status read :3630 and `set_status(Processing)` :3653).
- Add `push_local_dependency` / `pop_local_dependency` / `take_local_dependencies`
  (lazy create, FIFO, dedup by id, remove entry when empty).

**Tests (red/parity):** unit — `test_try_to_start_immediately_*` (U1),
`test_local_dependency_fifo_dedup_and_removal` (U4), `test_submit_bool_parity` (U6,
mirrors existing `test_jobqueue_submit_*`). Existing JobQueue tests (4492+) must stay green.

**Validation:** `cargo test -p liquers-core jobqueue` · `cargo test -p liquers-core`

**Rollback:** `git checkout liquers-core/src/assets.rs`

**Agent:** **opus** · skills: liquers-unittest · knowledge: Phase 2 JobQueue + Function
Signatures, existing `submit` (3510) / worker `run` (3607) / tests (4492-4707).
Rationale: shared-slot accounting + worker loop under concurrency; parity-critical.

---

### Step 4: AssetManager trait extension + DefaultAssetManager overrides

**File:** `liquers-core/src/assets.rs` (`AssetManager` ~2189, `DefaultAssetManager`)

**Action:**
- Add trait methods **with default impls** (CLAUDE.md — keep existing implementors incl.
  py wrappers compiling): `get_dependency_asset` (default `get_asset`),
  `drain_dependencies` (default `Ok(())`), `wait_for_dependency` (default:
  `enter_dependencies` + `dependency.get()` + `leave_dependencies_and_resume`).
- `DefaultAssetManager` overrides all three against its `JobQueue` (resolve-without-
  global-submit; inline claim+run drain; claim-aware wait with drain-then-direct-claim-
  then-subscribe; explicit status matches) + private `cleanup_local_dependencies(parent_id)`
  implementing the shared/non-shared leftover policy.

**Tests:** covered via Steps 6–8 flows; `cargo check -p liquers-py` here (trait defaults).

**Validation:** `cargo check -p liquers-core` · `cargo check -p liquers-py`

**Rollback:** `git checkout liquers-core/src/assets.rs`

**Agent:** **opus** · skills: liquers-unittest · knowledge: Phase 2 AssetManager +
DefaultAssetManager overrides + Cleanup/Lifecycle, existing `get`/`get_asset`
(2886/2983), `poll_state` Error/Cancelled fabrication (604-612), watch re-check pattern
(1941-1999). Rationale: the wait/drain state machine + leftover policy.

---

### Step 5: `evaluate_recipe` pure-key delegation migration

**File:** `liquers-core/src/assets.rs` (`evaluate_recipe`, 1412-1465)

**Action:** keep `record_dependency_on_asset(&asset)`; replace the ad-hoc
`matches!(status, Submitted|Dependencies)` + `Box::pin(asset.run())` + `asset.get()`
block with `manager.wait_for_dependency(self, &asset).await` (F-1 inline guard retires
onto the shared claim primitive).

**Tests:** existing delegation flows (`async_hellow_world` `-R/` recipes) stay green;
new `test_dependency_chain_deeper_than_capacity_completes` / `test_delegation_cycle_fails_fast`
land in Step 8.

**Validation:** `cargo test -p liquers-core` (delegation regressions)

**Rollback:** `git checkout liquers-core/src/assets.rs`

**Agent:** **sonnet** · knowledge: Phase 2 "Migrated: evaluate_recipe" note, existing
`evaluate_recipe`. Rationale: localized substitution onto Step 4's primitive.

---

### Step 6: Context scheduling API

**File:** `liquers-core/src/context.rs`

**Action:**
- Add `pub(crate) schedule_dependency_asset(&self, query) -> Result<AssetRef<E>, Error>`
  (ScheduleNode classify → `register_scheduled_dependency` → `get_dependency_asset`
  single capture → record `DependencyRecord` in `pending_dependencies` +
  `add_dependent_asset`).
- Add `pub(crate) wait_for_dependency(&self, asset) -> Result<State<E::Value>, Error>`
  (= `manager.wait_for_dependency(&self.get_asset_ref(), asset)`).
- Add `evaluate_local_queue(&self)` (= `manager.drain_dependencies(current)`) and
  `get_dependency_state(&self, query)` (schedule + wait).
- Reimplement `evaluate` (198): `schedule_dependency_asset` + `evaluate_local_queue` +
  `Ok(asset)` (public signature unchanged).
- **Reuse** existing `add_dependency` (376), `pending_dependencies` (172),
  `get_asset_ref` (361), `DependencyKey::from`.

**Tests:** integration `test_context_evaluate_records_runtime_dependency`,
`test_immediate_and_queued_record_same_dependencies`,
`test_static_and_runtime_dependencies_deduplicated` (scaffold in Step 8 file).

**Validation:** `cargo test -p liquers-core` · `cargo check -p liquers-py`

**Rollback:** `git checkout liquers-core/src/context.rs`

**Agent:** **sonnet** · skills: liquers-unittest · knowledge: Phase 2 Context section,
existing `evaluate` (198) / `add_dependency` (376) / cycle check (209-219). Rationale:
sequencing over Steps 1 & 4; not itself race-bearing.

---

### Step 7: Interpreter pre-pass + do_step migration

**File:** `liquers-core/src/interpreter.rs`

**Action:**
- Add `pub(crate) struct PlanDependencySchedule<E> { assets: HashMap<Query, AssetRef<E>> }`
  with `asset(&self, query) -> Option<&AssetRef<E>>`.
- Add `schedule_plan_dependencies(plan, context) -> Result<PlanDependencySchedule<E>, Error>`
  (walk `GetAsset*`→query, `Evaluate(q)`, `Action` param links; dedup by `Query`; skip
  `Plan` sub-plans — recursive `apply_plan` does its own pass). Mirror `find_dependencies`
  (plan.rs:1667).
- Modify `apply_plan` (82): pre-pass → `context.evaluate_local_queue()` → step loop with
  `&PlanDependencySchedule`.
- Modify `do_step` (109): replace each `get_asset(q).await?.get().await?` (Action links
  191-201, Evaluate 168-175, GetAsset* 243-305) with `schedule.asset(q)` →
  `context.wait_for_dependency(asset)` (fallback `context.get_dependency_state(q)`).

**Tests:** integration `test_diamond_shared_runs_once` (I1),
`test_capacity_one_fanout_drains_local_queue` (I2),
`test_volatile_dependency_evaluates_once_per_eval` (I4).

**Validation:** `cargo test -p liquers-core` (incl. existing `async_hellow_world`,
q-instruction suites for backward compat)

**Rollback:** `git checkout liquers-core/src/interpreter.rs`

**Agent:** **opus** · skills: liquers-unittest · knowledge: Phase 2 interpreter section,
existing `apply_plan`/`do_step` (82/109), `find_dependencies` (plan.rs:1667), `evaluate`
entry (interpreter.rs:314). Rationale: the pre-pass is the behavior-change surface;
must preserve the stable `evaluate` result contract.

---

### Step 8: Integration test suite

**File:** `liquers-core/tests/dependency_scheduling.rs` (new)

**Action:** implement all Phase 3 integration tests I1–I16 (diamond, capacity-1 fan-out,
three cycle shapes, volatile-once, dependency-failure, status-flow enter/leave,
cancellation Drop-repair, and the WP-1 carry-overs: runtime-dependency recording +
parity + dedup, chain-deeper-than-capacity, delegation cycle, exactly-once parent
resume, shared-child + not-resubmitted cancellation, backward-compat). `SimpleEnvironment<Value>`
+ `register_command!`; keyed/`-R/` cases add `MemoryStore` (`AsyncStoreWrapper`) +
`RecipeProvider` per `async_hellow_world.rs`; `tokio::time::timeout` on every wait/cycle.

**Validation:** `cargo test -p liquers-core --test dependency_scheduling`

**Rollback:** `git rm liquers-core/tests/dependency_scheduling.rs`

**Agent:** **sonnet** · skills: liquers-unittest · knowledge: Phase 3 doc (test tables +
WP-1 reconciliation), `async_hellow_world.rs`, JobQueue tests. Rationale: high-volume,
convention-driven test authoring.

---

### Step 9: Documentation updates

**Files:** `specs/DEPENDENCIES_STATUS.md` (document truthful `Processing → Dependencies →
Processing` wait flow; "no extra waiting status"), `specs/PROJECT_OVERVIEW.md` (asset
scheduling/lifecycle: non-blocking dependency evaluation, local queues, run-claim).
CLAUDE.md needs no change (no new conventions). Update DESIGN.md status → Phase 4 approved.

**Validation:** prose review; links resolve.

**Rollback:** `git checkout specs/`

**Agent:** **haiku** · knowledge: Phase 1/2 status-flow + Overview sections.

---

### Step 10: Full workspace validation

**Action / Validation:**
```bash
cargo test -p liquers-core
cargo test --workspace --exclude liquers-py
cargo check -p liquers-py
cargo clippy --workspace --exclude liquers-py --all-targets
```
Expected: all green; no test exceeds its 10 s timeout guard (proves no-deadlock at
capacity 1 and on every cycle shape).

**Agent:** **sonnet** · Rationale: triage any cross-crate/py fallout.

## Testing Plan

- **Unit tests** (inline, Steps 1–3): run after each step —
  `cargo test -p liquers-core <module>`. Red before the step's code, green after.
- **Integration tests** (Step 8, `tests/dependency_scheduling.rs`): scaffold API-shape
  tests red during Steps 6–7; complete and green in Step 8 —
  `cargo test -p liquers-core --test dependency_scheduling`.
- **Regression:** existing `async_hellow_world` + q-instruction suites and JobQueue unit
  tests (4492-4707) must stay green throughout (backward-compat contract).
- **Compatibility:** `cargo check -p liquers-py` after Steps 4 and 6 (trait defaults).
- **Manual/no-hang:** full Step 10 block; timeouts are the deadlock guard.

## Agent Assignment Summary

| Step | Model | Skills | Rationale |
|------|-------|--------|-----------|
| 1 DependencyManager | sonnet | liquers-unittest | contained graph logic, no async races |
| 2 RunClaim | opus | liquers-unittest | claim uniqueness + Drop/cancellation safety |
| 3 JobQueue | opus | liquers-unittest | shared-slot accounting + worker loop, parity |
| 4 AssetManager | opus | liquers-unittest | wait/drain state machine + leftover policy |
| 5 evaluate_recipe | sonnet | — | localized substitution onto Step 4 primitive |
| 6 Context API | sonnet | liquers-unittest | sequencing over Steps 1 & 4 |
| 7 Interpreter | opus | liquers-unittest | behavior-change surface; contract preservation |
| 8 Integration tests | sonnet | liquers-unittest | high-volume convention-driven authoring |
| 9 Docs | haiku | — | prose updates |
| 10 Validation | sonnet | — | cross-crate triage |

**Skills note:** the designer workflow nominally auto-invokes `rust-best-practices`; that
skill is not installed in this environment, so its intent is met by applying CLAUDE.md
Rust conventions directly (no `unwrap`/`expect` outside tests, explicit enum matches / no
default arm, typed `Error` constructors, async-first, `#[async_trait]`).

## Rollback Plan

- **Per step:** `git checkout <file>` (single-file steps) — the bottom-up order means a
  reverted step only breaks steps above it, which aren't written yet.
- **Per commit:** each step is its own commit on `claude/dependency-eval-blocking-e7f7ba`;
  `git revert <sha>` backs out one step.
- **Whole feature:** the work is isolated on the feature branch; abandon by not merging,
  or `git revert` the step range. No schema/serialization/store migration exists to undo
  (all new types are runtime-only).

## Documentation Updates

- `specs/DEPENDENCIES_STATUS.md` — the wait status flow (Step 9).
- `specs/PROJECT_OVERVIEW.md` — non-blocking dependency scheduling in the asset lifecycle
  (Step 9); required because a core asset concept changes (CLAUDE.md "update
  PROJECT_OVERVIEW if core concepts change").
- `specs/dependency-scheduling/DESIGN.md` — mark Phase 4 approved / implementation status.
- CLAUDE.md — no change.

## Execution Options

On approval:
1. **Execute now** — implement Steps 1→10 in order on `claude/dependency-eval-blocking-e7f7ba`,
   committing per step, running each step's validation before proceeding.
2. **Create task list** — defer; capture Steps 1–10 as tracked tasks.
3. **Revise plan** — return to Phase 4.
4. **Exit** — implement manually.
