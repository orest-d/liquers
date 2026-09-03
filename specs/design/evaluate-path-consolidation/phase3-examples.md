---
id: EVALUATE-PATH-CONSOLIDATION-PHASE3
kind: design
title: "Phase 3: Examples and tests — what equivalence means, and what would hide a regression"
status: draft
phase: examples
area: [core/assets, core/plan, core/context]
created: 2026-09-03
---
# Phase 3: Examples & Use-cases — Evaluation Path Consolidation

This design removes duplication rather than adding a capability, so its examples are not "how to
use the new feature" — they are **demonstrations that the entry points now behave alike**, and the
tests are the real deliverable. Examples are conceptual Rust in house idiom: the API they show does
not exist yet, so nothing here compiles until Phase 4.

The progression is: one scenario establishing what "equivalent" does and does not mean (§Example
1); one showing the payload requirement becoming observable, which is the only *new* fact the
design produces (§Example 2); then the corner cases where a careless implementation silently breaks
something, and the test plan that catches each.

## Overview Table

| # | Item | Kind | Demonstrates / checks | Fails today? |
|---|---|---|---|---|
| E1 | Entry-point equivalence | example | The same recipe through `get_asset`, `apply`, and `apply`-with-payload records the same dependencies, type and payload requirement; persistence and reuse differ by *construction*, not by evaluation | — |
| E2 | The payload requirement becomes observable | example | `MetadataRecord.payload_required` and `AssetInfo.payload_required` finally carry what the plan knew all along | — |
| C1–C9 | Corner cases | analysis | Where consolidation can silently break something | — |
| U1 | `evaluate(None)` keyed happy path | unit | value + status + dependency records + persistence in one body | no |
| U2 | Payload requirement recorded | unit | `payload_required` is `Required` in metadata after evaluating a payload plan | **yes** |
| U3 | Requirement, not presence | unit | a plan needing no payload records `None` even when a payload was supplied | **yes** |
| U4 | Recipe-preview projection | unit | `get_asset_info` projects `plan.payload_required` beside `is_volatile`/`expires` | **yes** |
| U5 | Payload gate is `apply_plan`'s | unit | the error comes from the interpreter gate; `Context::apply` no longer pre-checks | no¹ |
| U6 | `store_target` per construction | unit | the 14 construction sites record the right target | **yes** |
| U7 | Delegation suppresses the write | unit | a delegating asset does not persist; the owner does | no |
| I1–I8 | The eight persistence rows | integration | store contents after evaluation, both managers | I5–I7 **yes** |
| I9 | Entry-point equivalence | integration | identical dependency records and metadata facts across three entry points | **yes** |
| I10 | Reusability invariant | integration | no payload-evaluated asset is in the key map or query map | **yes**² |
| I11 | Key + payload is an error | integration | the latent violation becomes a named error | **yes** |
| I12 | Execute-once on the inline path | integration | two concurrent callers of the same asset run the body once, with a command that yields | **yes** |

¹ The behaviour is unchanged; the test pins *which* layer produces it, so the pre-check cannot come back.
² Passes today by coincidence (payload implies volatile implies unmapped); the test asserts it against the maps so it stops depending on that chain.

## Example

### Example 1 — Entry-point equivalence, and its limits

**What it demonstrates.** Phase 1's purpose in one runnable shape: three entry points, one body.
The same recipe is evaluated as a keyed resource, as an ad-hoc `apply`, and as an `apply` carrying
a payload. Everything produced *inside* `evaluate` is identical; everything that differs is decided
at *construction*.

**The component sequence.**

1. **Construction** (manager, differs per entry point): `get(key)` builds an asset with
   `store_target: Some(key)` and registers it in the key map; `apply(recipe, state, payload)` builds
   an ad-hoc asset with `store_target: None` that enters no map.
2. **Evaluation** (private `evaluate(payload)`, identical for all three): resolve volatility →
   resolve the recipe (delegate to the key's owner if one is registered) → `apply_recipe` →
   collect dependencies into metadata → install value, type identifier and type name →
   `try_to_set_ready()` → notify → persist if `store_target.is_some() && !delegated` →
   `track_asset`.
3. **Observation** (caller): identical metadata facts; different persistence and reuse.

```rust
// Conceptual — the API does not exist yet.
let manager = envref.get_asset_manager();
let key = parse_key("data/report.txt")?;

// 1. keyed resource: store_target = Some(key), enters the key map
let keyed = manager.get(&key).await?;

// 2. ad-hoc apply: store_target = None, in no map
let applied = manager.apply(Recipe::from(q("transform")), State::new(), None).await?;

// 3. ad-hoc apply with a payload: store_target = None, and never reusable
let with_payload = manager
    .apply(Recipe::from(q("transform")), State::new(), Some(payload))
    .await?;

for asset in [&keyed, &applied, &with_payload] {
    let md = asset.get_metadata().await?;
    assert_eq!(md.get_dependencies(), expected_dependencies);   // recorded in one place now
    assert_eq!(md.type_identifier, "text");                     // installed in one place now
    assert_eq!(md.payload_required(), PayloadRequirement::None);
    assert!(asset.status().await.is_finished());
}

// Differences, all decided at construction:
assert!(store.contains(&key).await?);            // keyed asset owns a target
assert_eq!(manager.query_map_len(), 0);          // neither apply asset is registered
```

**What "equivalent" does *not* mean.** The **status sequence differs legitimately** and no test
should assert it is identical: a queued keyed asset passes through `Submitted`, which an inline
`apply` never enters, because scheduling is manager policy. What must match is the set of facts
`evaluate` produces — dependency records, type identifier and name, payload requirement, the
failure routine, and the *final* status class (`Ready` vs `Volatile` vs `Error`). Asserting the
literal status sequence across entry points would produce a test that cannot pass and would be
"fixed" by weakening the real invariant.

### Example 2 — The payload requirement becomes observable

**What it demonstrates.** The one genuinely new fact. A command declares `payload: required`; the
plan has recorded that since `PlanBuilder` ran; and until now nothing carried it to the asset, so
every evaluated asset reported `PayloadRequirement::None`
(`ASSET-PAYLOAD-REQUIREMENT-NOT-RECORDED`).

```rust
// Conceptual.
register_command!(cr, async fn personalize(state, context) -> result
    payload: required
    doc: "Renders using the caller's session payload"
)?;

let asset = envref.evaluate_immediately(&q("personalize"), payload).await?;

assert_eq!(asset.payload_required().await, PayloadRequirement::Required);
assert_eq!(
    asset.get_asset_info().await?.payload_required,
    PayloadRequirement::Required
);

// And the recipe preview agrees, before anything is evaluated:
let info = recipe_provider.get_asset_info(&key, envref.clone()).await?;
assert_eq!(info.payload_required, PayloadRequirement::Required);
```

**The subtle half.** Reproducibility follows the *requirement*, not the presence of a payload. A
plain query evaluated through `evaluate_immediately` has a payload in scope that no command
consumes; it must still report `None` and stay reproducible. U3 exists to stop an implementation
that records "a payload was supplied" instead.

## Corner Cases

| # | Case | Symptom if wrong | Cause | Correction | The assertion that catches it |
|---|---|---|---|---|---|
| C1 | **Volatile keyed asset stops being stored** | A volatile key's file silently stops appearing in the store | `bound_owner_key()` returns `None` for it — the manager deliberately never registers volatile keyed assets | `store_target` recorded at construction (`get_volatile_resource_asset`) | store contains the key with status `Volatile` after evaluation. **The existing `scenario_volatile_keyed_eval` asserts the value only, so this regression passes the suite today** |
| C2 | **`make_volatile` records the wrong target** | Volatile keyed assets stop writing, or volatile query assets start writing | `ImmediateAssetManager::make_volatile` serves a key caller (`:5756`) and a query caller (`:5737`); it cannot derive the target | Take the target as a parameter | both volatile shapes checked separately on the inline manager |
| C3 | **`set_state` treated as an evaluation** | Installed values acquire dependency records, or lose their supplied status | `set_state(key, state)` pairs a store target with supplied data but never calls `evaluate` | Leave it outside the one body | after `set_state`, status is the supplied one and no dependency record appeared |
| C4 | **Delegation double-writes or double-registers** | Two writes to one key, or a duplicated dependency edge | `delegated` is redundant for persistence but still gates DM suppression | Keep the flag; pin the persistence equivalence | exactly one store write for the key; the delegating asset registers nothing |
| C5 | **Payload asset reachable from a map** | A second request returns a payload-evaluated value with no payload supplied | the key branch of `get_dependency_asset_with_payload` resolves through `get_resource_asset`, whose non-volatile path returns the *map-registered* asset | that branch becomes an explicit error | key + payload returns an error naming the boundary; no asset created, no map touched |
| C6 | **`ValueProduced` before finalization** | A subscriber polls on the notification and sees `None`/`Processing` | today's immediate path notifies before `try_to_set_ready` | notify after finalization in the one body | on receipt of `ValueProduced`, status is already terminal |
| C7 | **A spawn leaks into the shared body** | wasm build breaks, or the inline manager hangs with no reactor | the one body runs under both harnesses | only the harnesses are platform-specific; the body uses no `tokio::` primitive | `immediate_runs_without_tokio_runtime` (exists) stays green, plus the wasm build matrix |
| C8 | **`try_to_set_ready` runs too late** | An asset with a stale dependency is stored as `Ready` instead of `Expired` | status must be final before notify, before persist, before `track_asset` | fix the order in the one body | a stale-dependency asset is persisted as `Expired`, not `Ready` |
| C10 | **Stale-dependency asset persisted as `Ready`** | The store says `Ready` for a value the run concluded was expired | the stale-dependency rule runs in `finish_run_with_result` (`:2050`), *after* `evaluate_and_store` persisted (`:2345`), and no save follows | move the rule into status finalization, before persistence | a stale-dependency asset's **stored** metadata says `Expired`. Pre-existing at HEAD — filed as `ASSET-STALE-DEPENDENCY-PERSISTED-AS-READY` |
| C9 | **Inline claim resets to the wrong status** | A dropped claim leaves an asset unrunnable, or lets a second caller re-enter | the queued `RunClaim` re-parks to a queue the inline path does not have | the inline claim restores a re-runnable status | after a failed claimed run, a second `run_inline` completes the evaluation |

## Test Plan

Conventions per `.claude/skills/liquers-unittest/`: `#[tokio::test]` for async, `#[test]` for
plan-only work, `-> Result<(), Box<dyn std::error::Error>>` where `?` is used, no `unwrap`/`expect`
outside tests, typed error constructors, `#[cfg(test)] mod tests` at file end.

### Unit tests

| Test | File | Kind | Assertion |
|---|---|---|---|
| `evaluate_keyed_records_value_status_dependencies_and_persists` | `assets.rs` | `#[tokio::test]` | one body produces value, `Ready`, dependency records in metadata, and a store entry |
| `payload_requirement_recorded_in_metadata` | `assets.rs` | `#[tokio::test]` | after evaluating a `payload: required` plan, `metadata.payload_required()` is `Required` |
| `payload_requirement_reaches_asset_info` | `assets.rs` | `#[tokio::test]` | the same value appears in `get_asset_info()` |
| `payload_supplied_but_not_required_records_none` | `assets.rs` | `#[tokio::test]` | a plan needing no payload records `None` **with a payload supplied** — reproducibility follows the requirement |
| `store_target_some_for_keyed_construction` | `assets.rs` | `#[tokio::test]` | `get_nonvolatile_resource_asset` and `get_volatile_resource_asset` both record `Some(key)` |
| `store_target_none_for_query_and_adhoc_construction` | `assets.rs` | `#[tokio::test]` | query assets, `apply` assets, `create_asset`, `new_temporary` all record `None` |
| `make_volatile_takes_target_from_caller` | `assets.rs` | `#[tokio::test]` | the key caller yields `Some(key)`, the query caller `None` |
| `delegating_asset_does_not_persist` | `assets.rs` | `#[tokio::test]` | one store write for the key, performed by the owner |
| `value_produced_fires_after_status_finalization` | `assets.rs` | `#[tokio::test]` | on `ValueProduced`, `status().is_finished()` already holds (C6) |
| `set_state_does_not_enter_the_evaluation_body` | `assets.rs` | `#[tokio::test]` | after `set_state`, the status is the supplied one and no dependency record appeared (C3 — a guard, since the behaviour is unchanged) |
| `status_is_final_before_persistence` | `assets.rs` | `#[tokio::test]` | the status written to the store equals the asset's terminal status; with a stale dependency both are `Expired`, not `Ready` (C8/C10) |
| `apply_plan_rejects_missing_payload` | `interpreter.rs` | `#[tokio::test]` | the gate's error, with the query attached |
| `context_apply_defers_payload_check_to_apply_plan` | `context.rs` | `#[tokio::test]` | the error originates in `apply_plan`; `Context::apply` carries no pre-check |
| `get_asset_info_projects_payload_required` | `recipes.rs` | `#[tokio::test]` | recipe preview projects `plan.payload_required` beside `is_volatile` and `expires` |

### Integration tests

All scenario bodies are generic over the environment and run against **both** managers, following
`manager_parametric.rs`'s existing `scenario_*` + `*_default` / `*_immediate` pattern.

| Test | Row / concern | Assertion | Today |
|---|---|---|---|
| `scenario_persist_keyed_nonvolatile` | row 1 | store contains the key, status `Ready`, exactly one write | passes — guard |
| `scenario_persist_keyed_volatile` | row 2 | store contains the key, status `Volatile`, not fast-trackable on re-request | **assertion missing today** |
| `scenario_persist_keyed_delegating` | row 3 | exactly one write, by the owner | passes — guard |
| `scenario_persist_query_plain` | row 4 | no write | passes — guard |
| `scenario_persist_query_with_store_to_key` | row 5 | `store_target` is `None` and nothing is written | construction-level¹ |
| `scenario_persist_apply_bare_key_recipe` | row 6 | nothing written under that key | **fails today** |
| `scenario_persist_apply_recipe_with_filename` | row 7 | nothing written under `cwd/filename` | **fails today** |
| `scenario_persist_apply_with_payload` | row 8 | no write | passes — guard |
| `scenario_entry_point_equivalence` | E1 | identical dependency records, type identifier, `payload_required` and final status class across `get`/`apply`/`apply`-with-payload — **not** the literal status sequence | **fails today** |
| `scenario_payload_asset_absent_from_maps` | I10 | after a payload evaluation, neither the key map nor the query map contains the asset — asserted against the maps, not inferred from `is_volatile` | passes by coincidence today² |
| `scenario_key_with_payload_is_an_error` | C5 | error naming the key boundary; no asset created | **fails today** |
| `inline_execute_once_with_yielding_command` | C9 / co-delivery | two concurrent `get_asset` of the same query, command contains a real `.await` yield point → body runs once | **fails today**³ |

¹ Row 5 is unreachable at HEAD — a plain query asset has no cwd, so `store_to_key()` yields `None`.
The test asserts the construction fact rather than pretending to exercise a store write, and the
rule makes the row unreachable by design rather than by accident.

² The chain "payload ⇒ volatile ⇒ never mapped" holds at HEAD, so the assertion passes now. It is
worth writing because the invariant is a property of the payload, and a command registered
`payload: required` without `volatile` would break the chain silently.

³ **Corrected from the draft.** Two concurrent `apply` calls cannot demonstrate execute-once: each
builds a *separate* ad-hoc asset (`new_ext`, fresh id, in no map), so the body legitimately runs
twice. Execute-once concerns two callers converging on **one** asset, which is what `get_asset`
does through the query map. The existing `immediate_concurrent_same_query_runs_once`
(`manager_parametric.rs:478`) already has the right shape and misses the gap only because its
command is a sync closure that never yields — so the second caller is never polled while the first
is in flight. The new test keeps the shape and adds a genuine yield point.

### Test-only support needed

`scenario_payload_asset_absent_from_maps` asserts against manager internals. Phase 4 adds
`#[cfg(test)]` accessors (`key_map_contains`, `query_map_contains`) on both managers rather than
widening the public surface — consistent with the encapsulation rule that assets are managed only
through the manager.

### Regression gate

Before the change, run the existing suite against a `save_to_store` that uses `bound_owner_key()`.
It passes. That is the demonstration that the volatile-keyed store write is currently unguarded,
and the reason C1 is the first corner case rather than a footnote.

## Query Validation

Every query appearing in this document was checked with `liquers-validate` against the real
97-command registry (`transform` and `personalize` declared with `--command`, since they are
illustrative):

```
transform            -> transform            Ok
personalize          -> personalize          Ok
-R/data/report.txt   -> -R/data/report.txt   Ok
```

Exit 0. Worth doing even for queries this simple: `-R/` consumes the rest of the string as a key,
so `-R/data/report.txt` would have meant something different had a segment been appended without
`/-/`.

## Review Outcome

**Reviewer 1 — conformity with Phases 1 and 2.** No blocking findings. Every Phase 2 commitment has
coverage, and the eight-row persistence table maps one-to-one onto I1–I8. Two gaps, **both fixed
above**: corner cases C3 and C8 had no tests. C8 turned out to be the more valuable of the two —
writing its test revealed that HEAD does not honour the ordering invariant at all (see C10 and the
issue filed).

It also judged Phase 1's "thin wrappers" phrasing incomplete, which is fair: the wrappers are thin
*in evaluation logic*, not in construction, and construction is what decides whether two concurrent
callers converge on one asset. That is exactly what the execute-once correction exposed. Phase 1 is
amended rather than left for a future reader to rediscover.

**Reviewer 2 — test realism against the codebase.** Every claim verified: `scenario_volatile_keyed_eval`
asserts the value and no store write (`manager_parametric.rs:327`); `immediate_concurrent_same_query_runs_once`
registers a synchronous closure that cannot yield (`:484`); both `apply` implementations call
`AssetData::new_ext` per invocation, so concurrent applies are separate assets (`:4743`, `:5866`);
`Submitted` is queued-only (`:1318`); `Recipe::from(query)` leaves `cwd: None`, so `store_to_key()`
yields `None` for a plain query asset (`recipes.rs:404`, `:421`, `:356`); `CountingStore` and
`Metadata::get_dependencies` make every proposed assertion observable; the test names match the
file's conventions. Its one open item — that the `#[cfg(test)]` map accessors do not exist yet — is
Phase 4 work this document already schedules.

## Documentation and Learning Log

**Guide-worthy?** No new guide. Nothing here answers "how do I achieve X" for a command author —
the workflows are internal. Two items belong in the reference (`ASSET_LIFECYCLE.md`), and Phase 5
should carry them as prose rather than as a test list:

- **The equivalence table**: what is identical across entry points (facts produced by `evaluate`)
  versus what legitimately differs (scheduling and the status sequence, persistence, reuse). This
  is the distinction a reader needs and the one a naive reading of "one evaluation path" gets wrong.
- **The eight persistence outcomes**, as the answer to "when does an asset get written?".

**Learning recorded during Phase 3:**

1. `scenario_volatile_keyed_eval` asserts a value but not a store write, so the design's most
   dangerous regression is invisible to the current suite. Value-only assertions on a path whose
   *point* is persistence are worth auditing beyond this design.
2. Two `apply` calls are not a concurrency test. Ad-hoc assets are unshared by construction, so any
   execute-once test must converge two callers on one *mapped* asset.
3. **The ordering invariant is not honoured at HEAD.** Writing the test for "status is finalized
   before persistence" showed that an asset using a stale dependency is persisted as `Ready` and
   only then labelled `Expired` in memory, with no save after. Filed as
   `ASSET-STALE-DEPENDENCY-PERSISTED-AS-READY` (P2, M). The consolidation states the invariant and
   makes the violation visible, but does not by itself fix it — the rule lives in the harness the
   design keeps. A test written to confirm an assumption is worth more than one written to confirm
   a change.
4. **"Thin wrapper" needs the qualifier.** Entry points are thin in evaluation logic; they are not
   interchangeable in construction, and construction decides what concurrent access means. Two
   `apply` calls are two assets; two `get_asset` calls are one.
5. `payload: required` is implemented and tested but documented in no reference or guide — filed as
   `REGISTER-COMMAND-PAYLOAD-STATEMENT-UNDOCUMENTED` (P2, S). It was found because a test needed
   the declaration and it had to be recovered from the macro parser.
