# Phase 3: Examples and Tests - `Context::set_title` and `Context::set_description`

## Example

```rust
async fn summarize(state: State<Value>, context: Context<E>) -> Result<Value, Error> {
    let text = state.try_into_string()?;
    let lines = text.lines().count();
    context.set_title("Summary").await?;
    context.set_description(&format!("{lines} lines")).await?;
    Ok(Value::from(text))
}
register_command!(cr, async fn summarize(state, context) -> result)?;
```

| Asset | Recipe declares | Resulting title / description |
|---|---|---|
| query `-R/data/notes.txt/-/summarize` | — (ad-hoc) | `Summary` / `3 lines` |
| key `reports/notes.txt`, recipe query `…/summarize`, `title: Meeting notes` | title only | `Meeting notes` / `3 lines` |
| same, plus `description: Weekly sync` | both | `Meeting notes` / `Weekly sync` |

## Tests to Add — `liquers-core/tests/context_title_description.rs`

Setup follows `liquers-core/tests/async_hellow_world.rs` (environment with
`AsyncMemoryStore::new(&Key::new())`, command registration via `register_command!`, evaluation
through the asset manager). Commands: `retitled` (takes the state, returns it, sets title `"T2"`), `titled` (no state; returns `"x"`, sets title `"T"` and
description `"D"`), `plain` (returns `"x"`, sets nothing). Recipes are written to
`data/recipes.yaml` with `store.set`, as the existing recipe tests do.

| Test | Steps | Asserts | Criterion |
|---|---|---|---|
| `command_sets_title_and_description_of_a_query_asset` | evaluate query `titled` | metadata `title() == "T"`, `description() == "D"` | 1, 2 |
| `recipe_title_and_description_win_over_the_command` | recipe `out.txt`: query `titled`, `title: R`, `description: RD`; `get(data/out.txt)` | `R` / `RD`; evaluation succeeded (the setters returned `Ok`) | 3 |
| `command_fills_a_field_the_recipe_left_empty` | recipe `part.txt`: query `titled`, `title: R`, no description; `get(data/part.txt)`; then `store.get_metadata`, then a fresh environment over the same store and `get_asset_info` | `R` / `D` live, in the store, and after reload | 4 |
| `command_fills_both_fields_when_the_recipe_declares_neither` | recipe `bare.txt`: query `titled`, no title or description | `T` / `D` | 4 |
| `title_does_not_change_version` | evaluate `titled` and `plain` under two recipe keys with no titles | `metadata.version()` equal (same bytes `"x"`) | 5 |
| `recipe_title_wins_on_the_immediate_manager` | as `recipe_title_and_description_win_over_the_command`, after installing `ImmediateAssetManager` | `R` / `RD` | 3 (both managers) |
| `a_later_step_in_the_same_query_wins` | evaluate query `titled/retitled` (both commands run in one plan and share the asset's `Context`) | title `T2`, description `D` | Phase 1 finding on shared contexts |

Unit tests in `liquers-core/src/assets.rs` `mod tests` (where `AssetData` is constructible):

| Test | Asserts | Criterion |
|---|---|---|
| `command_description_fields_refuse_legacy_metadata` | `AssetData` with `Metadata::LegacyMetadata(json!({}))`; `set_description_fields_from_command(Some("t".into()), None)` errs `NotSupported` | 6 |
| `command_description_fields_are_kept_when_the_recipe_set_them` | flags set by hand, call returns `Ok(())`, metadata unchanged, even on legacy metadata (nothing to write) | 3, 6 |
| `reset_clears_recipe_description_flags` | set flags, `reset()`, both `false` | Phase 1 finding |

Queries `titled`, `plain`, `titled/retitled` and `-R/data/notes.txt/-/summarize` validate with
`liquers-validate --command titled --command plain --command retitled --command summarize` (checked 2026-10-05: all
four ok, encoded as written; `titled/retitled` plans as two actions).

## Coverage Review

Every criterion is covered, including both halves of the per-field rule. The two recipe tests
encode the maintainer's decision.
