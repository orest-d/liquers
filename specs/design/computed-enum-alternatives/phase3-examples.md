# Phase 3: Examples & Use-cases — computed enum alternatives

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | Columns of the input frame | input scope, listing | AC-2, AC-4 |
| 2 | Example | Stored tables | environment scope at plan time | AC-1 |
| 3 | Test | `enum_alternatives::tests::environment_enum_refuses_unlisted_value` | plan-time refusal, message | AC-1 |
| 4 | Test | `…::tests::input_enum_refuses_before_command_runs` | command not called | AC-2 |
| 5 | Test | `…::tests::listed_value_reaches_command_as_text` | value unchanged, others semantics | AC-3 |
| 6 | Test | `…::tests::listing_merges_static_and_computed` | order, de-duplication, `complete` | AC-4 |
| 7 | Test | `…::tests::provider_error_warns_and_accepts` | both scopes | AC-5 |
| 8 | Test | `plan::tests::computed_enum_without_provider_plans_any_value` | planner only | AC-6 |
| 9 | Test | `environment_builder` `validate_warns_on_computed_enum_without_provider` | builder report | AC-6 |
| 10 | Test | `command_metadata::tests::static_enum_serialization_unchanged` | no `computed` key, same version | AC-7 |
| 11 | Test | `…::tests::variadic_elements_checked_individually` | `multiple` argument | AC-8 |

## Example 1: columns of the input frame

```rust
struct Columns;
#[async_trait]
impl EnumAlternativesProvider<E> for Columns {
    async fn alternatives(&self, _env: EnvRef<E>, input: Option<&State<Value>>)
        -> Result<Vec<EnumArgumentAlternative>, Error> {
        let Some(state) = input else { return Ok(vec![]) };
        let df = try_to_polars_dataframe(state)?;          // liquers-lib helper
        Ok(df.get_column_names().iter().map(|c| EnumArgumentAlternative::new(c,
            CommandParameterValue::from_value(Value::from(c.as_str())))).collect())
    }
}
cr.register_enum_provider(EnumArgument::new("pl.columns").with_value_type(EnumArgumentType::String),
    EnumProviderScope::Input, Arc::new(Columns))?;
register_command!(cr, fn sort(state, column: String (enum_ref: "pl.columns"), …) -> result
    namespace: "pl")?;
```

`-R/data/sales.csv/-/ns-pl/sort-amont` plans (today it also validates `Ok` with
`liquers-validate`), then fails before `sort` runs: *Undefined enum value 'amont' for argument
column. Valid values: region, amount, date*. `-R/data/sales.csv/-/ns-pl/sort-amount` runs as
today. A UI holding the frame's state calls `enum_alternatives(envref, &key, "column",
Some(&state))` and receives the three columns with `complete: true`; without the state it gets
`[]` with `complete: false`.

## Example 2: stored tables

A provider of scope `Environment` named `demo.tables` lists the keys under `tables/`
(`envref.get_async_store().listdir(...)`). With `open_table(table: String (enum_ref:
"demo.tables"))`, `open_table-salse` fails when planned, naming `sales` and `costs`, and nothing is
evaluated.

## Edge and Error Cases

- `others: true` on a computed enum: listed values are suggestions only; nothing is refused.
- A static alias in a computed enum still expands to its static value at plan time.
- A value supplied through a link parameter or a metadata default is not checked (non-goal).
- A recipe override (`OverrideValue`) is checked like a query value.
- An input-scoped provider given `None` (listing without a state) returns what it can; the check
  itself always passes a state.
- Provider error: warning in the plan or the asset log, value accepted.
- An inline enum with `computed` (imported metadata): warning at validate, never checked.

## Test Plan

Unit tests in `liquers-core/src/enum_alternatives.rs` (tests 3-7, 11) build a `SimpleEnvironment`
with a memory store, register a test command `pick(state, choice: String (enum_ref: "t.choice"))`
or its variadic form, and providers that return fixed lists, count their calls, or fail. They
evaluate with `envref.evaluate("…")` and inspect the asset's error or log. Test 4 asserts the
command's call counter stays zero. Test 8 is in `plan.rs` `tests` with a bare
`CommandMetadataRegistry`. Test 9 in `liquers-core/tests/environment_builder.rs`. Test 10 pins a
`metadata_version` computed before the change.

Run: `CARGO_INCREMENTAL=0 cargo test -p liquers-core --lib enum_alternatives`,
`--lib computed_enum`, `--test environment_builder`; `cargo test -p liquers-macro`;
`cargo test -p liquers-lib --test registry_export` (unchanged registry, AC-7).
