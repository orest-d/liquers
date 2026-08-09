---
id: VOLATILE-KEYED-RECIPE-SELF-DELEGATION
kind: issue
title: Volatile keyed recipe delegates to itself
status: closed
priority: P1
complexity: M
area: [core/assets]
design: keyed-recipe-ownership
created: 2026-08-08
github:
---
## Problem

Evaluating a keyed asset whose recipe is volatile fails with a spurious
`ErrorType::DependencyCycle` instead of producing a value.

`AssetManager::get` resolves a volatile key through `get_volatile_resource_asset`, which
builds a **fresh** `AssetRef` and deliberately does not insert it into the `assets` map
(`liquers-core/src/assets.rs`). `AssetRef::evaluate_recipe` then calls `manager.get(&key)`
to decide whether it owns the recipe, and compares asset ids: because the volatile path
mints a new asset on every call, the returned id never equals the caller's, so the branch
always takes the *delegation* path. The delegation records a dependency of the asset on
what is effectively itself, and `register_scheduled_dependency` correctly reports a cycle.

Non-volatile keyed recipes are unaffected: their assets are shared through the map, so the
id comparison succeeds and the asset evaluates its own recipe.

## Reproduction

`liquers-core/tests/payload_inheritance.rs::test_volatile_keyed_recipe_cycles_preexisting_defect`
registers a command with `volatile: true`, stores a recipe using it, and evaluates
`-R/<key>`. No payload is involved. The test currently asserts the broken behaviour so that
a fix fails loudly.

## Impact

Any keyed recipe using a volatile command is unusable. This also blocks the natural
evaluation-path test for the keyed-payload boundary, since `payload: required` implies
`volatile` — see PAYLOAD-NESTED-EVALUATION-INHERITANCE. That rejection is therefore verified
through recipe resolution and asset introspection instead.

## Fix direction

The ownership test in `evaluate_recipe` should not rely on asset-id identity for volatile
keys, since that identity is not stable by design. Consider comparing keys, or having the
volatile path return the calling asset when one is already evaluating that key.

## Verification

1. A keyed volatile recipe evaluates to its value rather than a cycle error.
2. Non-volatile keyed recipes are unchanged.
3. Invert `test_volatile_keyed_recipe_cycles_preexisting_defect` and re-enable the
   `evaluate()` path in `test_keyed_recipe_requiring_payload_is_rejected`.

## Resolution

Fixed by `specs/design/keyed-recipe-ownership/`, which replaced the id-identity ownership test with
`AssetManager::owned_key_asset`. A volatile key has no registered owner, and "no owner" means
"evaluate the recipe here", so the delegation that caused the spurious cycle no longer happens.
This follows the *Fix direction*'s second suggestion — having the volatile path resolve to the
calling asset — expressed as an absence rather than an identity.

All three verification points are met:
`payload_inheritance.rs::test_volatile_keyed_recipe_cycles_preexisting_defect` is inverted to
`test_volatile_keyed_recipe_evaluates`; the `evaluate()` path is restored in
`test_keyed_recipe_requiring_payload_is_rejected`; and non-volatile keyed recipes are unchanged,
covered by `manager_parametric.rs::keyed_eval_{default,immediate}`.

**What this did not fix.** The delegation branch itself still cannot succeed — it is only reached
when the delegate is registered under the caller's own key, so `record_dependency_on_asset` always
sees a self-edge. This issue's diagnosis of that mechanism was right and broader than the volatile
case; it is now tracked as `ASSET-KEYED-DELEGATION-ALWAYS-CYCLES`.
