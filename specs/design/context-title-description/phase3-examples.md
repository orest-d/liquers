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

`-R/data/notes.txt/-/summarize` → `AssetInfo.title == "Summary"`,
`description == "3 lines"` (for a three-line file).

## Tests to Add — `liquers-core/tests/context_title_description.rs`

Setup follows `liquers-core/tests/async_hellow_world.rs` (environment with
`AsyncMemoryStore::new(&Key::new())`, command registration via `register_command!`, evaluation
through the asset manager). Commands: `titled` (no state; returns `"x"`, sets title `"T"` and
description `"D"`), `plain` (returns `"x"`, sets nothing).

| Test | Steps | Asserts | Criterion |
|---|---|---|---|
| `command_sets_title_and_description_of_a_query_asset` | evaluate query `titled` | metadata `title() == "T"`, `description() == "D"` | 1, 2 |
| `command_title_persists_for_a_keyed_asset` | `recipes.yaml` in `data/` declaring `out.txt` with query `titled`, no recipe title; `get(data/out.txt)`; read `store.get_metadata` | stored title/description `T`/`D`; a fresh environment over the same store reads them via `get_asset_info` | 3 |
| `command_title_overrides_recipe_title` | recipe declares `title: R`, `description: RD`, query `titled` | after evaluation `T`/`D`; with query `plain` instead, `R`/`RD` remain | 4 |
| `title_does_not_change_version` | evaluate `titled` and `plain` under two keys | `metadata.version()` equal (same bytes `"x"`) | 5 |

Unit test in `liquers-core/src/assets.rs` `mod tests` (where `AssetData` is constructible):
`set_description_fields_refuses_legacy_metadata` — an `AssetData` whose `metadata` is
`Metadata::LegacyMetadata(json!({}))`; `set_description_fields(Some("t".into()), None)` errs
with `ErrorType::NotSupported` (criterion 6; `Context` delegates to it).

Queries `titled`, `plain` and `-R/data/notes.txt/-/summarize` were checked against the grammar
(plain action names; `-R/…/-/` starts the action segment); validate with
`liquers-validate --command titled --command plain` before committing the test.

## Coverage Review

All six criteria covered; the precedence test encodes the recommended answer and is the test to
flip if the decision goes the other way.
