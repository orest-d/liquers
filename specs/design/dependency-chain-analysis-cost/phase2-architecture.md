# Phase 2: Solution and Architecture

## Lever A — memoized dependency analysis (`liquers-core/src/plan.rs`)

```rust
/// Per-analysis memo: the dependency set already computed for a resolved key's recipe.
pub(crate) struct DependencyMemo {
    by_key: HashMap<Key, Vec<PlanDependency>>,
}

pub(crate) fn find_dependencies<'a, E: Environment>(
    envref: EnvRef<E>,
    plan: &'a Plan,
    stack: &'a mut Vec<Key>,
    cursor: &'a mut CwdCursor,
    memo: &'a mut DependencyMemo,           // new
) -> BoxFuture<'a, Result<Vec<PlanDependency>, Error>>
```

In the `GetAsset` arm: if `memo.by_key` has `resolved_key`, extend from it and skip the recipe
lookup and recursion. Otherwise compute as today and insert. Cycle detection is unaffected,
because a memoized key was fully analysed without a cycle, and the `stack` check happens before the
memo lookup. All callers create a fresh memo (`DependencyMemo::default()`). One memo per top-level
analysis is safe because recipes cannot change in the middle of an analysis.

To also share across the analyses of one evaluation, hold the memo on the `Context` of the asset
being evaluated, if step 1 of Phase 4 shows repeated analyses per evaluation.

## Lever B — recipe file cache (`liquers-core/src/recipes.rs`)

`DefaultRecipeProvider` gains an `scc::HashMap<Key, (Version, Arc<RecipeList>)>` keyed by the
directory. It is filled on read (the stored `recipes.yaml`'s version, or a content hash of its
bytes) and dropped in `directory_changed(dir)` for `dir` and its ancestors. `recipe_opt`/`get_recipes`
use it. The provider is currently a unit struct (`pub struct DefaultRecipeProvider;`), so adding a
field changes its construction: provide `DefaultRecipeProvider::new()` and keep a `Default` impl.
Check every `Box::new(DefaultRecipeProvider)` call site (tests and environments).

## Order

Measure first (Phase 4 step 1). Apply A, measure. Apply B only if the profile still shows recipe
parsing. Both are internal.

## Rejected alternatives

- Record direct edges only (Phase 1, question 1).
- Precompute a global dependency graph of all recipes at startup. Recipes are dynamic (manifests,
  stored `recipes.yaml`), so it would need its own invalidation design.

## Known-issue preflight

| Issue | Relation | Blocks? |
|---|---|---|
| `DEPENDENCY-EDGE-RECORDED-AGAINST-SUPERSEDED-VERSION-IS-NOT-EXPIRED` | Same area | No |

## Risk table

| Area | Assessment |
|---|---|
| Likely files | `plan.rs` (`find_dependencies` + callers), maybe `context.rs`, `recipes.rs` (lever B), new benchmark test |
| Semantics | None intended; acceptance 2 compares records before/after |
| Concurrency | Memo is local (`&mut`). The lever B cache uses `scc` like other maps. A stale entry after an external `recipes.yaml` edit is covered by the version in the key (re-read when the stored version differs). |
| Performance | The goal. Measured. |
| Recovery | Each lever reverts independently |
| Certainty | High for A's correctness; medium for which lever dominates (hence measure first) |
