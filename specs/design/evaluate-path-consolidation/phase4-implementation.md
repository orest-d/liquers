---
id: EVALUATE-PATH-CONSOLIDATION-PHASE4
kind: design
title: "Phase 4: Implementation plan — eight steps, each independently revertable"
status: draft
phase: implementation
area: [core/assets, core/plan, core/context]
created: 2026-09-03
---
# Phase 4: Implementation Plan — Evaluation Path Consolidation

## Overview

Seven implementation steps, ordered so that **the tree compiles and the suite passes after every
one**, and so the only step that changes **durable state** — what reaches a store — is isolated in
its own commit and can be reverted without touching the rest.

Three steps change observable behaviour, and it is worth being exact about how they differ:
Step 4 changes *timing* (the `ValueProduced` notification moves after status finalization), Step 5
changes a *guarantee* (`apply` completes before returning), and **only Step 3 changes what is
written to a store**. The first two are visible the moment a test looks; a lost store write is
silent, which is why Step 3 alone gets the isolated commit and the store diff below.

The order is deliberately not "biggest first". Steps 1–3 are additive or mechanical and close
`ASSET-PAYLOAD-REQUIREMENT-NOT-RECORDED` on their own; Step 4 is the consolidation proper and
lands on ground already prepared; Steps 5–6 remove the duplicate entry points and add the claim.
Nothing after Step 3 changes what is written to a store.

| Step | What | Behaviour change | Revert cost |
|---|---|---|---|
| 1 | Record the payload requirement | none (a field stops being empty) | trivial |
| 2 | Add `store_target`, set it at 14 sites | none (field unused) | trivial |
| 3 | **Switch the write predicate to `store_target`** | **3 persistence rows narrow** | isolated commit |
| 4 | One `evaluate(payload)`; `run`/`run_inline` | notification ordering only | large but self-contained |
| 5 | Merge `apply`/`apply_immediately`; simplify `Context::apply`; key+payload error | `apply` gains an inline guarantee | moderate |
| 6 | Inline execute-once claim | fixes a double-run window | independent |
| 7 | Cross-cutting suite, matrix, wasm | none | n/a |

Documentation is **not** an implementation step: it is Phase 5, which begins when Steps 1–7 are
complete. Its content is specified in `phase2-architecture.md` §"The Phase 5 explanation" and
restated under Phase 5 Entry Criteria below.

## Implementation Steps

### Step 1 — Record the payload requirement

Closes `ASSET-PAYLOAD-REQUIREMENT-NOT-RECORDED`. Entirely additive: a field that always read
`None` starts carrying what the plan already knew.

**Files and changes**

```rust
// liquers-core/src/context.rs — new, mirrors set_expires
impl<E: Environment> Context<E> {
    /// Records that this evaluation's plan requires a payload, so the fact reaches
    /// `MetadataRecord.payload_required` and from there `AssetInfo`.
    pub async fn set_payload_required(&self) -> Result<(), Error>;
}
```

- `liquers-core/src/interpreter.rs`, in `apply_plan`, immediately after the existing authoritative
  gate: when `plan.payload_required.is_required()`, call `context.set_payload_required().await?`.
  Placed here because this is the one point every execution path passes through, and because
  `RECIPE-PLAN-ANALYSIS-RUNS-OUTSIDE-PLAN-BUILDING` reports the alternative site as misplaced.
- `liquers-core/src/recipes.rs`, in `AsyncRecipeProvider::get_asset_info` (`:526`): add
  `asset_info.payload_required = plan.payload_required;` beside the existing `is_volatile` and
  `expires` projections.
- `liquers-core/src/assets.rs`: add the read accessor.

```rust
impl<E: Environment> AssetRef<E> {
    /// Whether this asset's plan required an evaluation payload.
    pub async fn payload_required(&self) -> PayloadRequirement;
}
```

**Tests:** `payload_requirement_recorded_in_metadata`, `payload_requirement_reaches_asset_info`,
`payload_supplied_but_not_required_records_none`, `get_asset_info_projects_payload_required`, and
`apply_plan_rejects_missing_payload` — the last is a regression guard on the gate this step edits
beside: it pins the error to the interpreter layer, so the `Context::apply` pre-check that Step 5
deletes cannot creep back.

**Validation:** `cargo test -p liquers-core --lib --tests`

---

### Step 2 — Add `store_target` and set it at every construction site

Mechanical and unused on completion: nothing reads the field yet, so no behaviour can change.

```rust
// liquers-core/src/assets.rs, in AssetData<E>
/// Key this asset is responsible for writing, or `None` for a query or ad-hoc asset.
/// Set at construction by the manager; never inferred from the recipe afterwards, because
/// provider resolution replaces the recipe mid-evaluation.
store_target: Option<Key>,
```

Constructors gain the target rather than receiving it afterwards, so no asset ever exists with the
wrong one:

```rust
impl<E: Environment> AssetData<E> {
    pub(crate) fn new_ext(
        id: u64,
        recipe: Recipe,
        initial_state: State<E::Value>,
        store_target: Option<Key>,
        envref: EnvRef<E>,
    ) -> Self;
}

impl<E: Environment> AssetRef<E> {
    pub(crate) fn new_from_recipe(
        id: u64,
        recipe: Recipe,
        store_target: Option<Key>,
        envref: EnvRef<E>,
    ) -> Self;
}

impl<E: Environment> ImmediateAssetManager<E> {
    // serves a key caller and a query caller; the target cannot be derived inside
    async fn make_volatile(&self, recipe_src: Recipe, store_target: Option<Key>) -> AssetRef<E>;
}
```

The 14 sites and their values are the table in `phase2-architecture.md` §"Construction sites". The
two that matter: `get_volatile_resource_asset` (`:4294`) records `Some(key)` — the case a
map-derived predicate misses — and `make_volatile` takes the target from its caller.

**Test call sites, counted.** The Phase 2 table lists *production* constructions. Changing these
signatures also touches call sites inside `#[cfg(test)]` modules, which must be updated in the same
commit or the step does not compile:

| Constructor | Production | In `#[cfg(test)]` |
|---|---:|---:|
| `AssetRef::new_from_recipe` | 9 in `assets.rs` | 8 in `assets.rs`, 4 in `context.rs`, 1 in `interpreter.rs` |
| `AssetData::new_ext` | 8 in `assets.rs` | 0 |
| `make_volatile` | 2 in `assets.rs` | 0 |

**13 test call sites**, all in `liquers-core`. None in `liquers-lib`, `liquers-axum`, `liquers-web`
or `liquers-py` — these constructors are `pub(crate)`.

**Tests:** `store_target_some_for_keyed_construction`,
`store_target_none_for_query_and_adhoc_construction`, `make_volatile_takes_target_from_caller`.

**Validation:** `cargo test -p liquers-core --lib --tests`

---

### Step 3 — Switch the write predicate  ⚠ the behaviour change

The only step that changes what reaches a store. Its own commit, revertable alone.

- `liquers-core/src/assets.rs:2447` and `:2479` (`save_to_store`) and `:833`, `:861`
  (`save_metadata_to_store` and its sibling): replace
  `recipe.key()?.or(recipe.store_to_key()?)` with the recorded `store_target`.
- The write condition becomes `store_target.is_some() && !delegated`.

Three persistence rows narrow: a query asset resolving `store_to_key`, an `apply` with a bare-key
recipe, and an `apply` whose recipe carries a filename all stop writing. Row 2 — a **volatile keyed
asset** — must keep writing, with status `Volatile`; that is the regression the current suite
cannot see.

**Tests:** the eight `scenario_persist_*` bodies, both managers. Run
`scenario_persist_keyed_volatile` **first**: if it fails, the predicate is map-derived rather than
recorded, which is the Phase 1 mistake.

**Validation:** `cargo test -p liquers-core --lib --tests` then
`cargo test -p liquers-lib --lib --tests`

---

### Step 4 — One evaluation body

The consolidation proper.

```rust
impl<E: Environment> AssetRef<E> {
    /// The single evaluation body. Private: evaluation is entered through the asset manager.
    async fn evaluate(&self, payload: Option<E::Payload>) -> Result<(), Error>;

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) async fn run(&self, payload: Option<E::Payload>) -> Result<(), Error>;
    pub(crate) async fn run_inline(&self, payload: Option<E::Payload>) -> Result<(), Error>;
}
```

Removed: `evaluate_recipe`, `evaluate_recipe_outcome`, `evaluate_and_store`, `evaluate_immediately`,
`run_immediately`, `run_immediately_inline`. The two harnesses (`run_with_future`,
`run_with_future_inline`) are unchanged.

**Caller scope, verified.** `AssetRef::evaluate_immediately` has exactly two callers, both the run
wrappers in `assets.rs` (`:2117`, `:2170`). `run_immediately` / `run_immediately_inline` have four
production callers (`:4560`, `:4762`, `:5891`, `:5902`), all inside the two managers and all
rewritten by Steps 4–5, plus one test call (`:6721`).

> **A trap worth naming.** `EnvRef::evaluate_immediately` (`context.rs:377`) is a *different method
> with the same name*: it is the public query-evaluation API, it is **kept unchanged**, and it has
> 30+ callers across `injection.rs` and `payload_inheritance.rs`. A Phase 4 reviewer conflated the
> two and reported those tests as breakage. They are not affected. Grep for the removed methods by
> receiver type, not by name.

The invariant order inside `evaluate` is the nine steps in `phase2-architecture.md`
§"`evaluate` — the invariant order". Two ordering points are not optional:

- `try_to_set_ready()` runs **before** the `ValueProduced` notification and **before** persistence.
  Today the immediate path notifies while the status is still `None`/`Processing`.
- The payload is **moved** into the context, never cloned — `PayloadType` is not required to be
  `Clone`.

**Tests:** `evaluate_keyed_records_value_status_dependencies_and_persists`,
`value_produced_fires_after_status_finalization`, `delegating_asset_does_not_persist`,
`status_is_final_before_persistence`, `set_state_does_not_enter_the_evaluation_body`, plus
`scenario_entry_point_equivalence` (assert the facts `evaluate` produces — **not** the literal
status sequence, which differs by scheduling).

**Validation:** `cargo test -p liquers-core --lib --tests`, then the wasm loop
(`cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles` after
`cargo clean`), because this step is where a `tokio::` primitive could leak into shared code.

---

### Step 5 — Merge the ad-hoc entry points

```rust
async fn apply(
    &self,
    recipe: Recipe,
    to: State<E::Value>,
    payload: Option<E::Payload>,
) -> Result<AssetRef<E>, Error>;

#[deprecated(note = "use `apply(recipe, to, payload)`; every apply now evaluates before returning")]
async fn apply_immediately(
    &self,
    recipe: Recipe,
    to: State<E::Value>,
    payload: Option<E::Payload>,
) -> Result<AssetRef<E>, Error> {
    self.apply(recipe, to, payload).await
}
```

- `DefaultAssetManager::apply` → construct ad hoc, `run(payload)` (no `job_queue.submit`).
- `ImmediateAssetManager::apply` → construct ad hoc, `run_inline(payload)`.
- `Context::apply` loses the `requires_payload` pre-check and its duplicated error message; one
  call, `manager.apply(recipe, to, self.payload.clone())`.
- `get_dependency_asset_with_payload` (both managers): the `query.key()` branch becomes the
  explicit error in `phase2-architecture.md` §"A latent violation this exposes".
- `liquers-lib/src/ui/runner.rs:229` moves from `apply_immediately` to `apply`.

**Tests:** `context_apply_defers_payload_check_to_apply_plan`,
`scenario_key_with_payload_is_an_error`, `scenario_payload_asset_absent_from_maps`,
`scenario_persist_apply_with_payload`.

**Validation:** `cargo test -p liquers-core --lib --tests` then
`cargo test -p liquers-lib --lib --tests`

---

### Step 6 — Inline execute-once claim

Co-delivers `INLINE-PATH-LACKS-EXECUTE-ONCE`.

```rust
/// Atomic execute-once claim available on both targets. `RunClaim` becomes the queued
/// specialization; this variant's `Drop` restores a re-runnable status instead of re-submitting
/// to a queue it does not have.
pub(crate) async fn try_claim_for_run_inline(&self) -> Result<Option<InlineRunClaim<'_>>, Error>;
```

Scope discipline from that issue's own evidence: a claim that only *refuses* a second caller is the
wrong answer and broke `liquers-web`'s async-command test when it was tried. The second caller must
**wait**, which `run_with_future_inline` already improvises through its `select!` between
`wait_to_finish()` and the evaluation future. This makes that correct rather than improvised.

**Tests:** `inline_execute_once_with_yielding_command` — two concurrent `get_asset` of the same
query, with a command carrying a real `.await`. Not two `apply` calls: those build two separate
ad-hoc assets and legitimately run twice.

**Validation:** `cargo test -p liquers-core --lib --tests`, plus the wasm loop.

---

### Step 7 — Cross-cutting validation

No production change. Complete the integration suite in `manager_parametric.rs`, add the
`#[cfg(test)]` map accessors (`key_map_contains`, `query_map_contains`) on both managers, and run
the full matrix.

**Validation:**

```bash
cargo test -p liquers-core --lib --tests
cargo test -p liquers-lib --lib --tests
bash scripts/check-build-matrix.sh
cargo clean && cargo test -p liquers-web --target wasm32-unknown-unknown --features debug-handles
```

`cargo clean` before the wasm loop is not optional in a 30 GB session (`CLAUDE.md` §Building and
testing). Do not run `cargo test --workspace`.

---

### After Step 7 — Phase 5 (not an implementation step)

Per `phase2-architecture.md` §"The Phase 5 explanation": rewrite `ASSET_LIFECYCLE.md` into the
flow-and-public-surface reference, promote its audit content to
`specs/archive/<date>-asset-lifecycle-duplication-audit.md`, update `DOC_03`
§"Public versus infrastructure APIs" and §"Persistence contract", update `ASSETS.md` and
`PAYLOAD_GUIDE.md`, rewrite the `assets.rs` `//!` entry-point table as the module-level public API,
add `## History` rows and bump `reviewed:`, update `specs/README.md`, and close the issues.

## Testing Plan

| When | Command | Gate |
|---|---|---|
| After every step | `cargo test -p liquers-core --lib --tests` | all green |
| After steps 3 and 5 | `cargo test -p liquers-lib --lib --tests` | dependent crates unaffected |
| After steps 4 and 6 | wasm loop, after `cargo clean` | no spawn leaked into shared code |
| After step 7 | `bash scripts/check-build-matrix.sh` | 11 configurations |
| Before the PR | all of the above | — |

**Order-sensitive check.** In Step 3, run `scenario_persist_keyed_volatile` before the others: it is
the one that distinguishes a *recorded* target from a *map-derived* one, and it is the regression
the current suite cannot see.

**Tests that must fail first**, and the distinction that makes the discipline mean something:

*Behaviour tests* — these compile against HEAD and go **red**, which is real evidence they test the
thing: `scenario_persist_apply_bare_key_recipe`, `scenario_persist_apply_recipe_with_filename`,
`scenario_entry_point_equivalence`, `scenario_key_with_payload_is_an_error`,
`inline_execute_once_with_yielding_command`, `payload_requirement_recorded_in_metadata`,
`payload_supplied_but_not_required_records_none`, `get_asset_info_projects_payload_required`.
Write each and watch it fail before implementing its step; one that passes on arrival is testing
something else.

*New-API tests* — these cannot compile against HEAD because they name a field or method that does
not exist yet (`store_target_some_for_keyed_construction`,
`store_target_none_for_query_and_adhoc_construction`, `make_volatile_takes_target_from_caller`).
A compile error is **not** evidence the assertion is meaningful. For these the discipline is
different: after implementing, invert the assertion once and confirm it goes red.

## Agent Assignment

| Step | Model | Skills | Knowledge it must load |
|---|---|---|---|
| 1 | Sonnet | rust-best-practices, liquers-unittest | Phase 2 §"Payload requirement — where it is recorded"; `interpreter.rs` `apply_plan` gate; `recipes.rs:526`; `metadata.rs` payload accessors |
| 2 | Sonnet | rust-best-practices | Phase 2 §"Construction sites" (all 14 rows); `assets.rs` constructors |
| 3 | **Opus** | rust-best-practices, liquers-unittest | Phase 2 §"Persistence outcomes"; Phase 3 C1; `save_to_store`, `save_metadata_to_store` | 
| 4 | **Opus** | rust-best-practices, liquers-unittest | Phase 2 §"`evaluate` — the invariant order"; both current bodies; both harnesses; Phase 3 C6, C7, C8 |
| 5 | Sonnet | rust-best-practices, liquers-unittest | Phase 2 §"Merge the ad-hoc entry points" and §"Reusability invariant"; `Context::apply`; `ui/runner.rs` |
| 6 | **Opus** | rust-best-practices | `INLINE-PATH-LACKS-EXECUTE-ONCE` in full, including why the cheap guard was reverted; `RunClaim`; `run_with_future_inline` |
| 7 | Haiku | liquers-unittest | Phase 3 test plan; `manager_parametric.rs` conventions |
| 8 | Sonnet | — | Phase 2 §"The Phase 5 explanation"; `DOCS_STRUCTURE_GUIDE.md` §4.3, §9 |

Steps 3, 4 and 6 are Opus work: 3 is the only behaviour change, 4 is the consolidation itself, and
6 has a documented history of a plausible-looking fix that broke a working test. Steps 1, 2, 5 and
8 are mechanical enough for Sonnet given the sections named; 7 is assembly.

## Rollback Plan

One commit per step, so `git revert` of a single commit is always a valid state.

| Step | If it goes wrong | Revert impact |
|---|---|---|
| 1 | `payload_required` reads `None` again | none — the field was already dead |
| 2 | field unused | none |
| 3 | **a store write is lost or an unwanted one appears** | reverting restores `recipe.key().or(store_to_key())`; Steps 1–2 stay | 
| 4 | evaluation misbehaves on one manager | revert restores both bodies; Steps 1–3 stay, since none of them depends on the merge |
| 5 | an `apply` caller depended on queued (non-blocking) behaviour | revert restores `apply_immediately`; the deprecated default means callers still compile |
| 6 | the claim deadlocks or refuses a legitimate second caller | revert independently — nothing else depends on it |

**The signal to watch on Step 3:** a *disappeared* store entry is silent. Before merging, diff the
store contents produced by the eight persistence scenarios against the same scenarios at the parent
commit; only the three intended rows may differ.

**Not a rollback path:** disabling or skipping a failing test. If `scenario_persist_keyed_volatile`
fails, the predicate is wrong, not the test.

## Review Outcome

Three reviews ran against this plan. All findings are applied above.

| Reviewer | Finding | Resolution |
|---|---|---|
| Phase 1+2 conformity | "The single step with externally visible behaviour change" is inaccurate — Steps 4 and 5 also change observable behaviour | Reworded: Step 4 changes timing, Step 5 changes a guarantee, and only Step 3 changes **durable state**. The isolated commit is justified by silence, not by uniqueness |
| Phase 1+2 conformity | Step 8 was listed as an implementation step while the entry criteria required only Steps 1–7, making the documentation look optional | Documentation is no longer numbered as a step; it is Phase 5, entered when Steps 1–7 are complete |
| Phase 3 conformity | `apply_plan_rejects_missing_payload` specified in Phase 3, absent from Phase 4 | Added to Step 1, which edits beside that gate |
| Phase 3 conformity | The must-fail-first list omitted two tests | Completed, and split into behaviour tests (go red — real evidence) and new-API tests (fail to compile — *not* evidence; invert the assertion once instead) |
| Codebase compatibility | Signature changes in Step 2 touch `#[cfg(test)]` call sites the plan did not count | 13 test call sites counted and tabulated; all in `liquers-core` |
| Codebase compatibility | *"31+ callers of `evaluate_immediately` will break"* | **False alarm, and recorded as a trap.** Those call `EnvRef::evaluate_immediately` (`context.rs:377`), the public API this design keeps — not `AssetRef::evaluate_immediately` (`assets.rs:2381`), which has exactly two callers. Verified before acting |

The last row is the reason the plan now says to grep by receiver type rather than by name: two methods share a name, one is public and kept, the other private and removed. A reviewer with the whole codebase in front of it still conflated them.

## Phase 5 Entry Criteria

Phase 5 begins when all of the following hold:

1. Steps 1–7 are implemented and committed; `cargo test -p liquers-core --lib --tests` and
   `cargo test -p liquers-lib --lib --tests` are green.
2. `bash scripts/check-build-matrix.sh` passes all 11 configurations, and the wasm loop passes after
   `cargo clean`.
3. Every test in the Phase 3 plan exists and passes, and each test listed as "must fail first" was
   observed failing before its step.
4. No `TODO`, `FIXME` or `todo!()` is introduced; `ASSETS-FIX1`'s markers in the rewritten region
   are resolved or re-recorded.
5. All review comments — from the multi-agent reviews and from the user — are answered or
   incorporated.
6. The issues this design closes are ready to move to `closed` with a resolution note:
   `CORE-EVALUATE-PATH-CONSOLIDATION`, `ASSET-PAYLOAD-REQUIREMENT-NOT-RECORDED`,
   `INLINE-PATH-LACKS-EXECUTE-ONCE`. `ASSET-STALE-DEPENDENCY-PERSISTED-AS-READY` and
   `REGISTER-COMMAND-PAYLOAD-STATEMENT-UNDOCUMENTED` stay open — they are out of scope, and Phase 5
   records that explicitly rather than letting them look forgotten.
