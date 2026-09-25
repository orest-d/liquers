---
id: DEFAULT-ASSET-MANAGER-RECIPE-OPT-SKIPS-PAYLOAD-CHECK
kind: issue
title: DefaultAssetManager's recipe_opt override skips the payload-required rejection
status: draft
priority: P2
complexity: M
area: [core/assets]
design:
created: 2026-09-25
github:
---
## Problem

`AssetManager::recipe_opt` documents itself as "the earliest funnel where a recipe is resolved for
a key, so the rejection precedes asset creation and volatility resolution"
(`liquers-core/src/assets.rs`, trait default, near the `get` method doc). The default
implementation honours that: it calls `Recipe::to_plan_for_key`, which rejects a recipe whose plan
requires an evaluation payload, since a key is a payload boundary.

`DefaultAssetManager` overrides both `recipe_opt` and `is_volatile` inside its
`impl AssetManager<E> for DefaultAssetManager<E>` block, and the override is a bare pass-through:

```rust
async fn recipe_opt(&self, key: &Key) -> Result<Option<Recipe>, Error> {
    self.get_recipe_provider()
        .recipe_opt(key, self.get_envref())
        .await
}
```

No `to_plan_for_key` call. `ImmediateAssetManager` has no such override, so it runs the trait
default and does perform the check. The two managers therefore disagree about *when* — and through
which code path — a keyed recipe requiring a payload is refused:

- `ImmediateAssetManager`: rejected inside `recipe_opt`/`is_volatile`, before any asset is
  constructed, exactly as documented.
- `DefaultAssetManager`: `recipe_opt`/`is_volatile` both silently succeed (volatility is computed
  from a recipe whose plan will later be refused); an asset is constructed, and the rejection
  happens only inside `evaluate`, at the `recipe.to_plan_for_key(...)` call already present at the
  point the provider's recipe becomes authoritative (`assets.rs`, the recipe-adoption block inside
  `AssetRef::evaluate`, right before the `stored`/`cached` copy Step 1.2 added there).

The end-to-end outcome for a request is the same (an error), which is presumably why
`test_keyed_recipe_requiring_payload_is_rejected` (`liquers-core/tests/payload_inheritance.rs`,
`QueuedEnv` only) passes today — it never asserted *where* the rejection happens. What differs is
everything that runs in between: an asset is created and registered before `DefaultAssetManager`
discovers the recipe is invalid, `is_volatile()` is computed from a doomed recipe, and (as of Step
1.2 of `record-streams` Phase 4) `get_resource_asset` reads `stored`/`cached` from that same
unchecked recipe before the later, correct rejection fires.

## Impact

No known user-visible incorrectness today: the request still ends in an error either way, and no
test in the suite distinguishes the two paths. The risk is future code that trusts
`AssetManager::recipe_opt`'s doc comment ("precedes asset creation") for `DefaultAssetManager`
specifically — for example, code that checks `is_volatile(key)` or reads recipe flags before
`get`/`get_asset` and assumes an invalid keyed recipe cannot have reached that point. Step 1.2 of
`record-streams` Phase 4 (`ASSETS-CANNOT-BE-DECLARED-NON-PERSISTENT`) is exactly such code: it now
calls `recipe_opt` directly in both managers' `get_resource_asset`, on the assumption that the two
overrides behave the same way. They do not — only found because it was worth checking, not because
anything broke.

## Expected behaviour

`DefaultAssetManager::recipe_opt` (and, since `is_volatile` is a thin wrapper over it, transitively
`is_volatile`) should call `Recipe::to_plan_for_key` exactly as the trait default and
`ImmediateAssetManager` do, so both managers reject a payload-requiring keyed recipe at the same
point and neither manufactures an asset, a volatility answer, or (post-Step-1.2) `stored`/`cached`
flags from a recipe that is about to be refused.

Two ways to get there, in decreasing order of risk:
1. Delete the two overrides and let `DefaultAssetManager` fall back to the trait defaults — the
   safer fix if nothing in `DefaultAssetManager` relies on the override skipping the check.
2. Add the same `to_plan_for_key` call to the override, if the override exists for a reason (e.g.
   avoiding double validation elsewhere) that a reviewer should surface before deleting it.

A regression test should assert *where* the rejection happens (e.g. that `recipe_opt`/`is_volatile`
themselves return `Err` before any asset is constructed) for both managers, not only that the
end-to-end request errors.

## Discovery

Found while implementing Step 1.2 of `specs/design/record-streams/phase4-implementation.md`
(honouring `stored`/`cached` in both asset managers), while reading `DefaultAssetManager` and
`ImmediateAssetManager`'s `recipe_opt`/`is_volatile` implementations side by side to resolve the
key's recipe once in `get_resource_asset`. Not fixed here: out of scope for that step, and the
existing test suite does not regress either way.
