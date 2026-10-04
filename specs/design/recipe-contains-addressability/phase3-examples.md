# Phase 3: Examples and Tests - `AsyncRecipeProvider::contains` Answers Addressability

## Example

A conversion provider: for any `X.parquet` whose `X.csv` exists, `recipe_opt` returns a recipe
`-R/<dir>/X.csv/-/to_parquet`, while `assets_with_recipes` lists nothing. Before: `contains` →
`false`; `AssetManager::contains(data/table.parquet)` → `false` unless stored. After: `true`.

## Tests to Add (`liquers-core/src/recipes.rs`, `mod tests`)

Use the existing `TestEnv` and `MockProvider` scaffolding (≈2040-2110).

| Test | Setup | Asserts | Criterion |
|---|---|---|---|
| `default_contains_answers_for_an_unlisted_producible_key` | new test provider `PatternProvider`: `has_recipes → Ok(false)`, `assets_with_recipes → Ok(vec![])`, `recipe_opt(key) → Some(Recipe::from(&key))` when `key.filename()` ends with `.gen`, else `None`; **no `contains` override** | `contains(a/x.gen) == true`, `contains(a/x.txt) == false` | 1, 2 |
| `default_provider_contains_matches_its_listing` | `DefaultRecipeProvider` with a `recipes.yaml` in `data/` declaring `a.txt`, `b.txt` | `contains(data/a.txt)`, `contains(data/b.txt)` true; `contains(data/c.txt)` false; `contains(other/a.txt)` false | 3 |
| `contains_propagates_recipe_errors` | `recipes.yaml` with malformed YAML | `contains(data/a.txt)` is `Err` | 4 |

Existing tests to keep green: `contains_is_true_if_any_provider_has_the_recipe` (≈2101),
`liquers-records` `contains_answers_for_a_template_name_far_beyond_any_listing` (≈565), and the
`plan.rs` `CountingRecipeProvider` tests.

For the alternative option, add `trivial_provider_contains_nothing` and keep the three above.

## Setup

Memory store; `recipes.yaml` written with `store.set`. No commands need executing: `contains`
does not plan or evaluate. `Recipe::from(&Key)` builds a valid ad-hoc recipe.

## Coverage Review

Criteria 1-4 covered by unit tests; 5 is a documentation step in Phase 4.
