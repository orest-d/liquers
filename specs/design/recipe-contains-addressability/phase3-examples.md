# Phase 3: Examples and Tests - `contains` (Listed) and `can_make` (Producible)

## Example

`data/sales/daily.manifest.yaml` lists explicit chunk `summary.csv` and a template
`daily_NNNN.csv`.

| Call | `summary.csv` | `daily_0042.csv` | `other.csv` |
|---|---|---|---|
| `ManifestRecipeProvider::contains` | true | **false** (not listed) | false |
| `ManifestRecipeProvider::can_make` | true | true | false |
| `AssetManager::contains` | true | false | false |
| `AssetManager::can_make` | true | true | false |
| `AssetManager::get_asset_info` | recipe info | recipe info | `KeyNotFound` |

## Tests to Add or Change

### `liquers-core/src/recipes.rs` (`mod tests`, existing `TestEnv`/`MockProvider` scaffolding ≈2040-2110)

| Test | Setup | Asserts | Criterion |
|---|---|---|---|
| `on_demand_provider_splits_contains_and_can_make` | test `PatternProvider`: lists nothing, `recipe_opt` = `Some(Recipe::from(&key))` for names ending `.gen`; overrides neither method | `contains(a/x.gen) == false`, `can_make(a/x.gen) == true`, both false for `a/x.txt` | 1, 2 |
| `default_provider_contains_equals_can_make` | `DefaultRecipeProvider`, `data/recipes.yaml` declaring `a.txt`, `b.txt` | for `data/a.txt`, `data/b.txt`, `data/c.txt`, `other/a.txt`: `contains == can_make`, true only for the first two | 3 |
| `chain_forwards_contains_and_can_make` | chain of `MockProvider` (lists `dir/a`) and `PatternProvider` | `contains(dir/a)`, `!contains(dir/x.gen)`, `can_make(dir/x.gen)` | 5 |
| `can_make_propagates_recipe_errors` | malformed `recipes.yaml` | `can_make(data/a.txt)` is `Err` | 7 |
| existing `contains_is_true_if_any_provider_has_the_recipe` (≈2101) | — | re-run; if its mock does not list the keys, rename to `can_make_…` and assert `can_make` | 5 |

### `liquers-records/src/provider.rs` (`mod tests`)

| Test | Change | Criterion |
|---|---|---|
| `contains_answers_for_a_template_name_far_beyond_any_listing` | rename to `can_make_answers_for_a_template_name_far_beyond_any_listing`, assert `can_make`; add `assert!(!contains(...))` for the same key | 4 |
| new `contains_reports_only_explicit_chunks` | `contains` true for an explicit chunk, false for a template chunk | 4 |

### `liquers-core/src/assets.rs` (`mod tests`)

`manager_can_make_covers_unlisted_producible_keys`: environment whose recipe provider is the
`PatternProvider`; `contains(a/x.gen) == false`, `can_make(a/x.gen) == true`,
`get_asset_info(a/x.gen)` is `Ok` (criterion 6).

### `liquers-axum/tests/`

Extend the assets API integration tests: `GET {b}/key/can_make/a/x.gen` → `can_make: true`;
`GET {b}/key/contains/a/x.gen` → `contains: false`; `submit` of `a/x.gen` succeeds (guard uses
`can_make`).

## Setup

Memory store, `recipes.yaml` written with `store.set`. No commands executed: `contains` and
`can_make` neither plan nor evaluate.

## Coverage Review

Criteria 1-7 covered by unit tests; criterion 8 is a documentation step.
