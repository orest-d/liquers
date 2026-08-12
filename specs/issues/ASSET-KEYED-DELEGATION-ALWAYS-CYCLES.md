---
id: ASSET-KEYED-DELEGATION-ALWAYS-CYCLES
kind: issue
title: The keyed delegation branch cannot succeed - it always records a self-dependency
status: closed
priority: P0
complexity: M
area: [core/assets]
design: keyed-delegation-hand-off
created: 2026-08-09
github:
---

## Problem

`AssetRef::evaluate_recipe` has two branches for a keyed recipe: evaluate it here, or delegate to
the asset registered for that key. **The delegation branch always fails**, and cannot do otherwise
by construction.

The branch is reached when the key's registered owner is some asset other than the caller. The key
it delegates on comes from *the caller's own recipe*, so the delegate is by definition registered
under the caller's own key. `record_dependency_on_asset` (`liquers-core/src/assets.rs:1104`) then
derives:

- `current_dep_key` from `self.recipe.key()` — the caller's key,
- `dep_key` from `dependency.recipe.key()` — the same key,

and asks `would_create_cycle(&current_dep_key, &dep_key)` (`:1157`). That is a self-edge, so it
reports a cycle and the branch returns `Error::dependency_cycle` before
`wait_for_dependency` is ever reached.

There is no arrangement of assets that avoids this, because the lookup key and the caller's key are
the same value by the time the branch is entered.

## Impact

Any asset that holds a keyed recipe it does not own fails with a spurious
`ErrorType::DependencyCycle` instead of receiving the owner's value. Two ways to get there:

1. **A stale owner.** An asset starts evaluating key `K`, the map entry for `K` is evicted
   (expiry, error, cancellation) and rebuilt, and the original asset's ownership test now finds
   the replacement.
2. **An ad-hoc asset over a bare key.** `AssetManager::apply(key.into(), state)` builds an
   untracked asset from a key recipe — reachable from `Context::apply(&pure_key_query, state)`,
   which is separately ill-defined (`CONTEXT-APPLY-BARE-KEY-ILL-DEFINED`).

Neither is a common path, which is why this has not been reported as a symptom. It became visible
only when a test tried to exercise the branch deliberately.

The error surfaces at different points on the two managers, which is worth knowing when reading a
report: the inline manager runs the asset during `apply` and returns the error there, while the
queued manager submits and the error appears when the value is read.

## Relationship to VOLATILE-KEYED-RECIPE-SELF-DELEGATION

That issue described the same mechanism — *"The delegation records a dependency of the asset on
what is effectively itself, and `register_scheduled_dependency` correctly reports a cycle"* — but
attributed it to the volatile path, where a fresh asset was minted on every lookup so the identity
test never matched. `specs/design/keyed-recipe-ownership/` fixed that path: a volatile key has no
registered owner, so it self-evaluates and never reaches delegation.

What that design did **not** fix is the branch itself. It made it rarer, not correct.

## Expected behaviour

Either:

1. **Delegation stops recording a self-dependency.** The dependent and the dependency are the same
   graph node, so there is no edge to record; waiting on the other asset is not a dependency
   relation but a hand-off. `wait_for_dependency` could be reached directly, with the dependency
   record skipped when `current_dep_key == dep_key`.
2. **Or the branch is removed** and such an asset evaluates the recipe itself. Simpler, but it
   gives up value sharing in the stale-owner case and would double-persist under the key.

(1) looks right and (2) looks cheap; the choice needs somebody with the dependency graph in their
head, which is why this is filed rather than folded into the ownership fix.

## Verification

`liquers-core/tests/manager_parametric.rs::keyed_delegation_{default,immediate}` currently assert
the broken outcome, and panic with instructions to invert them if delegation ever produces a value.
They also assert the recipe still runs exactly once, so branch *selection* stays pinned either way.

## Discovery

Found on 2026-08-09 while implementing `specs/design/keyed-recipe-ownership/`. The test written to
prove the delegation branch still fires after the ownership change found that it fires and then
always fails.

## Resolution

Fixed by `specs/design/keyed-delegation-hand-off/`, taking option (1) of *Expected behaviour*.

The rule now stated in code is that **two assets holding the same key are one node of the
dependency graph**, and a node has no edge to itself. `AssetRef::record_dependency_on_asset`
derives `current_dep_key` before it writes anything and returns `Ok(())` when it equals `dep_key`:
no `DependencyRecord` in metadata, no edge offered to the `DependencyManager`, so
`would_create_cycle` is never asked about the self case and the delegation branch reaches
`AssetManager::wait_for_dependency` for the first time.

The exemption lives in `record_dependency_on_asset` rather than at the delegation call site because
the invariant belongs to the dependency graph, not to delegation. `would_create_cycle` is
unchanged — returning `true` for `dependent == dependency` is the correct answer to the question it
was asked; the fix is to stop asking it. The call site keeps calling
`record_dependency_on_asset`, which stays correct should a delegate ever be registered under a key
other than its own recipe key.

`DependencyManager::track_asset` was checked and needed no change: it resolves the delegating
asset's key through `bound_owner_key()`, which returns `None` for a non-owner, so a delegating
asset does not re-register a version for the key or expire the owner's dependents.

### Verification

The two verification tests are inverted as instructed:
`manager_parametric.rs::keyed_delegation_{default,immediate}` now assert that the delegating asset
receives the owner's value (`"counted"`) while the call counter stays at `1`. Both the
`assert_ne!` on asset ids and the counter are kept, so branch *selection* remains pinned — a
regression to self-evaluation would still produce the right value but would run the recipe twice.

Two unit tests were added in `liquers-core/src/assets.rs`:
`record_dependency_on_asset_skips_same_node_hand_off` (nothing recorded in metadata, no self-edge
in the graph) and `record_dependency_on_asset_records_distinct_key` (the guard is not over-broad).

### Known remainder

The delegating asset still re-persists the owner's value to the store under the same key —
idempotent but wasteful, and a property of `evaluate_and_store` rather than of the cycle check.
Filed separately as `DELEGATED-VALUE-REPERSISTED`.
