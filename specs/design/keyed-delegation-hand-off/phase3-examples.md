# Phase 3: Examples & Use-cases - keyed-delegation-hand-off

## Introduction

Phase 1's purpose is that an asset holding a keyed recipe it does not own should *receive the
owner's value*, not fail with a spurious cycle. Nothing here is user-facing: delegation is an
internal branch of `AssetRef::evaluate_recipe`, reached only when the key→asset map already holds a
different owner. So the "examples" are the two arrangements that reach the branch, and the tests
are the whole deliverable.

Examples are **conceptual** for the scenarios and **runnable** for the tests: the scenarios narrate
paths that already exist in the codebase, and every assertion below lands in a real test file.

## Overview Table

| # | Kind | Name | Location | What it demonstrates / checks |
|---|---|---|---|---|
| S1 | Scenario | Ad-hoc asset over a bare key | conceptual | The primary route: `AssetManager::apply(key.into(), state)` builds an untracked asset whose recipe is a bare key the map already owns. |
| S2 | Scenario | Stale owner after eviction | conceptual | The secondary route: an asset evaluating `K` loses its map entry, the entry is rebuilt, and the original asset now sees a different owner. |
| S3 | Pitfall | Reading the counter, not the value | conceptual | Why the call counter is the load-bearing assertion: a fix that turned delegation into self-evaluation would pass a value-only assertion and silently double-compute. |
| T1 | Unit | `record_dependency_on_asset_skips_same_node_hand_off` | `liquers-core/src/assets.rs` tests | Same key on both ends ⇒ `Ok(())`, no metadata record, no graph edge. |
| T2 | Unit | `record_dependency_on_asset_records_distinct_key` | `liquers-core/src/assets.rs` tests | Different keys ⇒ the record is still written (the guard is not over-broad). |
| T2b | Unit | `record_dependency_on_asset_hand_off_survives_owner_recipe_resolution` | `liquers-core/src/assets.rs` tests | The owner's recipe resolved to a pure-key alias ⇒ still one node, still nothing recorded. |
| T3b | Integration | `test_keyed_asset_evaluating_its_own_key_is_a_cycle` | `liquers-core/tests/dependency_scheduling.rs` | A *genuine* keyed self-dependency via `Context::evaluate` is still rejected as a cycle, and does not hang. |
| T3 | Unit (existing) | `test_record_dependency_on_asset_does_not_downgrade_known_metadata_version_to_unknown` | same | Regression: version-preservation behaviour survives the reordering. |
| T4 | Integration | `keyed_delegation_default` | `liquers-core/tests/manager_parametric.rs` | Queued manager: delegation yields the owner's value; recipe runs once. |
| T5 | Integration | `keyed_delegation_immediate` | same | Inline manager: same contract through the trait-default `wait_for_dependency`. |
| T6 | Integration (existing) | `keyed_eval_*`, `volatile_keyed_eval_immediate`, `stored_value_precedes_recipe_*` | same | Branch *selection* is unchanged: owners still self-evaluate, volatile keys still never delegate. |

## Example S1 — Ad-hoc asset over a bare key (primary)

`recipes.yaml` maps `dash.txt` to `counted/dash.txt`. A first `manager.get(&dash.txt)` registers an
owner and evaluates it; `counted` runs once.

Then `manager.apply((&dash.txt).into(), State::new())` builds a **second**, untracked asset from
that bare key. It is not in the key map, so during `evaluate_recipe` it asks
`owned_key_asset(&dash.txt)`, gets the first asset back, sees a different id, and takes the
delegation branch.

Component sequence:

1. `evaluate_recipe` — `recipe.key()` is `Some(dash.txt)`; `owned_key_asset` returns the owner.
2. `record_dependency_on_asset(&owner)` — both keys are `dash.txt`. **Same node: returns `Ok(())`
   having recorded nothing.** Before this change, it returned `Error::dependency_cycle` here.
3. `wait_for_dependency(self, &owner)` — the owner is already `Ready`, so `poll_state` returns its
   state immediately.
4. `evaluate_and_store` installs that state. `track_asset` finds `bound_owner_key() == None` (this
   asset is not the registered owner), so no version is re-registered.

Result: the ad-hoc asset's value is `"counted"` and the counter is still `1`.

## Example S2 — Stale owner after eviction (secondary)

Same recipe, but the delegating asset is not ad-hoc. Asset `A` is registered for `K` and starts
evaluating. Its map entry is dropped — expiry sweep, error eviction or cancellation — and a later
`get(&K)` inserts a fresh asset `B`. `A`'s ownership test now returns `B`.

Everything after step 1 of S1 is identical. What differs is the payoff: `A` no longer recomputes
`K` in parallel with `B` and no longer fails; it adopts `B`'s value. This is the case that makes
option (2) of the issue (always self-evaluate) the wrong choice — it would have `A` and `B` running
the same recipe at the same time.

Note the interaction with `QUEUED-MANAGER-EVICTION-RACE` (P2, open): that issue is about *how* the
map entry gets replaced. This design changes only what happens afterwards.

## Example S3 — Pitfall: assert the counter, not just the value

A tempting "fix" is to delete the delegation branch so every asset evaluates its own keyed recipe.
`assert_eq!(state.try_into_string()?, "counted")` passes under that fix — the value is right,
because the recipe genuinely produced it. What fails is
`assert_eq!(calls.load(Ordering::SeqCst), 1)`: the recipe would have run twice.

Symptom in the wild: a shared key computed once per reader instead of once, with double persistence
and, for a non-deterministic command, two different values under one key. The counter assertion in
T4/T5 is what pins this; it is carried over from the pre-fix test unchanged, which is exactly why
the original test was written to keep it.

## T1 — `record_dependency_on_asset_skips_same_node_hand_off`

```rust
#[tokio::test]
async fn record_dependency_on_asset_skips_same_node_hand_off() {
    let env: SimpleEnvironment<Value> = SimpleEnvironment::new();
    let envref = env.to_ref();
    let key = parse_key("shared.txt").unwrap();
    let delegating = AssetData::<SimpleEnvironment<Value>>::new(2240, key.clone().into(), envref.clone()).to_ref();
    let owner = AssetData::<SimpleEnvironment<Value>>::new(2241, key.clone().into(), envref.clone()).to_ref();

    delegating.record_dependency_on_asset(&owner).await.unwrap();

    // No metadata record: the two assets are one dependency-graph node.
    assert!(delegating.get_metadata().await.unwrap().get_dependencies().is_empty());

    // And no graph edge. `expire_dependents` excludes the root, so a self-edge would show up as
    // the key expiring itself — `expire` would not, because it always includes the root.
    let manager = envref.get_asset_manager();
    let expired = manager.dependency_manager().expire_dependents(&DependencyKey::from(&key)).await;
    assert!(expired.keys.is_empty());
}
```

Error path: the pre-fix behaviour returned `Err(dependency cycle)` from this exact call, so the
`unwrap()` is itself the regression assertion.

## T2 — `record_dependency_on_asset_records_distinct_key`

The complement, guarding against an over-broad early return: parent keyed `parent.txt`, dependency
keyed `dep.txt`, one record written with `dep.txt` as its key. Without this, a guard that returned
early unconditionally would pass T1.

## T4/T5 — inverting `scenario_keyed_delegation`

The existing scenario is shared by both managers and already panics with instructions to invert it.
The inversion replaces the "expect a `Dependency cycle` error" block with:

```rust
let adhoc = manager.apply((&key).into(), State::new()).await?;
assert_ne!(adhoc.id(), owner.id(), "precondition: a different asset");
assert_eq!(adhoc.get().await?.try_into_string()?, "counted");
assert_eq!(
    calls.load(Ordering::SeqCst), 1,
    "delegation hands off the owner's value; it must not re-run the recipe"
);
```

Both preconditions are kept: the owner evaluates first (counter `1`), and the ad-hoc asset is a
genuinely different asset. Keeping `assert_ne!` matters — if a future change made `apply` return
the registered owner, the test would pass trivially without ever entering the branch.

## Corner Cases

| Case | Behaviour | Covered by |
|---|---|---|
| Owner already `Ready` | `wait_for_dependency` returns immediately from `poll_state`. | T4, T5 |
| Owner still running (queued) | `DefaultAssetManager::wait_for_dependency` drains its local queue, direct-claims, else subscribes. Unchanged code, now reachable. | T4 (timing-dependent), existing dependency-scheduling suite |
| Owner still running (inline) | Trait-default `wait_for_dependency` calls `owner.get()`, which runs it inline. Relies on `is_finished` rather than a claim — `INLINE-PATH-LACKS-EXECUTE-ONCE`, pre-existing. | T5 |
| Owner ends in `Error`/`Cancelled` | `wait_for_dependency` calls `fail_due_to_dependency`; the delegate fails with the owner's failure rather than a cycle. Strictly better than today. | not newly tested — existing `wait_for_dependency` coverage |
| Owner `Expired` with a stale value | Stale value used, `note_expired_dependency` marks the delegate expired. Unchanged policy. | existing expiration suite |
| Volatile key | Never registered as an owner, so `owned_key_asset` returns `None` and the asset self-evaluates — the branch is not entered at all. | T6 `volatile_keyed_eval_immediate` |
| Dependency has a query but no key | `dep_key` comes from the query; a keyed parent's `current_dep_key` cannot equal it, so the guard does not fire. | T2 shape, `test_context_add_dependency_*` |
| Parent has no key | Neither identity resolves; the guard does not fire and the metadata record is still written, as before. | T3 |
| Owner's recipe resolved to a pure-key alias `L` | Identity is the construction-time key on both sides, so the hand-off is still recognised. A recipe-only test would record `K -> L` with the owner's version *for `K`*, which `add_dependency` can read as staleness on `L` and expire `K` for. | T2b |
| Genuine keyed self-dependency (`Context::evaluate` on own key) | Untouched: a different path (`schedule_dependency_asset` → `register_scheduled_dependency` → `would_create_cycle`) still returns `Error::dependency_cycle`. | T3b |
| Serialization / reload | No new `DependencyRecord` shape. The guard *prevents* a self-record from being persisted and replayed through `load_from_records`. | reasoning; `load_from_records_registers_known` unchanged |
| Concurrency | Two delegates waiting on one owner is the ordinary many-dependents case; no new lock is held across an await. | existing manager concurrency tests |

## Test Plan

```bash
cargo test -p liquers-core --lib assets::                     # T1-T3 and the assets unit module
cargo test -p liquers-core --test manager_parametric          # T4-T6
cargo test -p liquers-core --lib --tests                      # full core regression
cargo test -p liquers-lib --lib --tests                       # the standard loop (CLAUDE.md)
```

No query strings are introduced by this change, so there is nothing for `liquers-validate` to
check: the tests use `parse_key("dash.txt")` and locally registered test commands, and
`specs/command_registry.yaml` is untouched.

## Documentation and Learning Log

### Guide candidates

None. Delegation has no user-facing workflow and answers no "how do I …" question; the rule belongs
in `specs/reference/DEPENDENCIES_STATUS.md` (Phase 2) and in the call-site comment. The executable
evidence a future reader should follow is `scenario_keyed_delegation` in
`liquers-core/tests/manager_parametric.rs`.
