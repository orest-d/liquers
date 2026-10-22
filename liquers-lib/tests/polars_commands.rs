//! Integration tests for the Polars command library.
//!
//! The whole file needs the optional `polars` dependency, so it is gated at file level: without
//! the feature the test target still compiles, it simply contains no tests.

#![cfg(feature = "polars")]

use liquers_core::{
    context::Environment, error::Error, metadata::MetadataRecord, state::State,
    value::ValueInterface,
};
use liquers_lib::{
    // `register_polars_commands` is an extension-trait method since `DefaultEnvironment` became an
    // alias of a `liquers-core` type: Rust permits an inherent `impl` only in the defining crate.
    environment::{DefaultEnvironment, PolarsCommandRegistration},
    value::{ExtValueInterface, Value},
};
use polars::prelude::{AnyValue, DataFrame};
use std::sync::Arc;

/// Helper function to create environment with polars commands registered
fn create_test_env() -> DefaultEnvironment<Value> {
    let mut env = DefaultEnvironment::<Value>::new();
    env.with_default_recipe_provider();
    env.register_polars_commands()
        .expect("Failed to register polars commands");
    env
}

/// Helper to create a State with CSV text data
fn create_csv_state(csv_text: &str) -> State<Value> {
    let mut metadata = MetadataRecord::new();
    metadata.data_format = Some("csv".to_string());
    metadata.with_type_identifier("Text".to_string());

    State::from_parts(
        Arc::new(Value::from(csv_text.to_string())),
        Arc::new(metadata.into()),
    )
}

// ---------------------------------------------------------------------------------------------
// Variadic column selection.
//
// These go THROUGH the commands - `create_test_env()` above builds an environment with the polars
// commands registered, and `eval_over_csv` evaluates a real query against it.
//
// See specs/design/variadic-arguments-declaration/.
// ---------------------------------------------------------------------------------------------

/// Apply a `pl` query to a CSV state, going THROUGH the command registry; return the raw state.
///
/// `apply` runs a transform query against a supplied state, which is what these tests need: no
/// store, no recipe, just state -> command -> state. `create_test_env()` above has already
/// registered the polars commands.
async fn eval_state_over_csv(csv: &str, action: &str) -> Result<State<Value>, Error> {
    use liquers_core::assets::AssetManager;
    use liquers_core::parse::parse_query;

    let env = create_test_env();
    let envref = env.to_ref();
    let query = parse_query(&format!("ns-pl/{action}"))?;

    let assetref = envref
        .get_asset_manager()
        .apply(query.into(), create_csv_state(csv), None)
        .await?;
    assetref.get().await
}

/// As [`eval_state_over_csv`], and require the result to be a `polars.DataFrame`.
///
/// The value is read with `as_polars_dataframe`, not converted: a command that wrongly returned
/// its CSV input as text would still parse as a DataFrame under `try_to_polars_dataframe`.
async fn eval_over_csv(csv: &str, action: &str) -> Result<Arc<DataFrame>, Error> {
    eval_state_over_csv(csv, action).await?.value()?.as_polars_dataframe()
}

/// Column `name` of `df`, each cell rendered with `AnyValue`'s `Display` (strings unquoted).
fn column_strings(df: &DataFrame, name: &str) -> Vec<String> {
    let column = df.column(name).expect("column exists");
    (0..column.len())
        .map(|i| match column.get(i).expect("row in range") {
            AnyValue::String(s) => s.to_string(),
            other => other.to_string(),
        })
        .collect()
}

/// The single cell of column `name` in a one-row aggregation result, as `f64`.
fn cell_f64(df: &DataFrame, name: &str) -> f64 {
    df.column(name)
        .expect("column exists")
        .get(0)
        .expect("one row")
        .extract::<f64>()
        .expect("numeric cell")
}

const PEOPLE: &str = "name,age,city\nAlice,30,NYC\nBob,25,LA\nCharlie,35,NYC\nDave,28,Chicago";
const NUMS: &str = "a,b\n1,2\n3,4\n5,6";
const WITH_NULL: &str = "name,age\nAlice,30\nBob,\nCarol,35";
const SEMI: &str = "a;b\n1;2";
const FIVE: &str = "a\n1\n2\n3\n4\n5";

/// Phase 3 test 18 (AC-1, AC-9) - `pl/head` is an alias of `pl/slice` with `offset = 0`, run
/// through the command path: `head-2` gives exactly the rows of `slice-0-2`, and the default
/// `n = 5` on three rows gives all three. See specs/design/command-alias-contract/.
#[tokio::test(flavor = "multi_thread")]
async fn test_head_alias_matches_slice() -> Result<(), Box<dyn std::error::Error>> {
    let csv = "a\n1\n2\n3";
    let head = eval_over_csv(csv, "head-2").await?;
    let slice = eval_over_csv(csv, "slice-0-2").await?;
    assert_eq!(head.height(), 2);
    assert!(head.equals(&slice), "head-2 must equal slice-0-2");

    let default = eval_over_csv(csv, "head").await?;
    assert_eq!(default.height(), 3);
    Ok(())
}

/// I1 - one parameter per column. This spelling is an arity error before the commands became
/// variadic ("accepts 1, but parameter #2 'c' was supplied").
#[tokio::test(flavor = "multi_thread")]
async fn test_select_columns() -> Result<(), Box<dyn std::error::Error>> {
    let df = eval_over_csv("a,b,c\n1,2,3\n4,5,6", "select_columns-a-c").await?;

    assert_eq!(df.width(), 2);
    assert!(df.get_column_names().iter().any(|s| *s == "a"));
    assert!(df.get_column_names().iter().any(|s| *s == "c"));
    assert!(!df.get_column_names().iter().any(|s| *s == "b"));
    Ok(())
}

/// I2 - the capability that did not exist before. `~_` escapes the parameter separator, so this
/// is ONE parameter naming the single column `a-b`. Under the deleted `split('-')` workaround
/// this was indistinguishable from selecting two columns `a` and `b`.
#[tokio::test(flavor = "multi_thread")]
async fn test_select_columns_escaped_dash_names_one_column(
) -> Result<(), Box<dyn std::error::Error>> {
    let df = eval_over_csv("a-b,c\n1,2\n3,4", "select_columns-a~_b").await?;

    assert_eq!(df.width(), 1);
    assert_eq!(df.get_column_names(), vec!["a-b"]);
    Ok(())
}

/// I3 - a variadic argument with no parameters is well-formed at plan level (it resolves to an
/// empty MultipleParameters), so the COMMAND must reject it, and the message must be about
/// columns rather than about arity.
#[tokio::test(flavor = "multi_thread")]
async fn test_select_columns_with_no_columns_is_rejected() {
    let err = eval_over_csv("a,b\n1,2", "select_columns")
        .await
        .expect_err("selecting no columns is not a meaningful request");

    assert!(
        err.message.contains("at least one column"),
        "message: {}",
        err.message
    );
}

/// I4 - the second converted command.
#[tokio::test(flavor = "multi_thread")]
async fn test_drop_columns() -> Result<(), Box<dyn std::error::Error>> {
    let df = eval_over_csv("a,b,c\n1,2,3", "drop_columns-b-c").await?;

    assert_eq!(df.width(), 1);
    assert_eq!(df.get_column_names(), vec!["a"]);
    Ok(())
}

/// I5 - per-element validation: `check_column_exists` runs for each element, so an unknown
/// column is named rather than silently ignored.
#[tokio::test(flavor = "multi_thread")]
async fn test_select_columns_reports_unknown_column_by_name() {
    let err = eval_over_csv("a,b\n1,2", "select_columns-a-zz")
        .await
        .expect_err("zz does not exist");

    assert!(err.message.contains("zz"), "message: {}", err.message);
}

/// C1 - a trailing dash is one EMPTY parameter, not an absent one, so it is still an element
/// and is rejected by name-checking rather than silently dropped.
#[tokio::test(flavor = "multi_thread")]
async fn test_select_columns_trailing_dash_is_an_empty_element() {
    let err = eval_over_csv("a,b\n1,2", "select_columns-a-")
        .await
        .expect_err("the empty string is not a column");

    assert!(!err.message.is_empty());
}

// ---------------------------------------------------------------------------------------------
// Every `ns-pl` command, evaluated through the registry
// (specs/design/polars-command-tests-via-queries/).
// ---------------------------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn test_from_csv_basic() -> Result<(), Box<dyn std::error::Error>> {
    let state = eval_state_over_csv(PEOPLE, "from_csv").await?;
    let df = state.value()?.as_polars_dataframe()?;
    assert_eq!(df.height(), 4);
    assert_eq!(df.width(), 3);
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_from_csv_separator() -> Result<(), Box<dyn std::error::Error>> {
    let comma = eval_over_csv(SEMI, "from_csv").await?;
    assert_eq!(comma.width(), 1, "the default separator is a comma");
    let semicolon = eval_over_csv(SEMI, "from_csv-semicolon").await?;
    assert_eq!(semicolon.width(), 2);
    assert_eq!(column_strings(&semicolon, "b"), vec!["2"]);
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_shape() -> Result<(), Box<dyn std::error::Error>> {
    let state = eval_state_over_csv(NUMS, "shape").await?;
    assert_eq!(
        state.value()?.try_into_json_value()?,
        serde_json::json!({"nrows": 3, "ncols": 2})
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_info_commands() -> Result<(), Box<dyn std::error::Error>> {
    let nrows = eval_state_over_csv(NUMS, "nrows").await?;
    assert_eq!(nrows.value()?.try_into_i64()?, 3);
    let ncols = eval_state_over_csv(NUMS, "ncols").await?;
    assert_eq!(ncols.value()?.try_into_i64()?, 2);
    let schema = eval_state_over_csv(NUMS, "schema").await?.value()?.try_into_json_value()?;
    let keys: Vec<&String> = schema
        .as_object()
        .expect("schema is an object")
        .keys()
        .collect();
    assert_eq!(keys, vec!["a", "b"]);
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_to_csv_round_trip() -> Result<(), Box<dyn std::error::Error>> {
    let text = eval_state_over_csv(NUMS, "to_csv")
        .await?
        .value()?
        .try_into_string()?;
    assert!(
        eval_state_over_csv(NUMS, "to_csv")
            .await?
            .value()?
            .as_polars_dataframe()
            .is_err(),
        "to_csv returns text, not a DataFrame"
    );
    let original = eval_over_csv(NUMS, "from_csv").await?;
    let reparsed = eval_over_csv(&text, "from_csv").await?;
    assert!(reparsed.equals(&original), "{text}");
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_head_returns_first_rows() -> Result<(), Box<dyn std::error::Error>> {
    let df = eval_over_csv(FIVE, "head-2").await?;
    assert_eq!(column_strings(&df, "a"), vec!["1", "2"]);
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_tail() -> Result<(), Box<dyn std::error::Error>> {
    let df = eval_over_csv(FIVE, "tail-2").await?;
    assert_eq!(column_strings(&df, "a"), vec!["4", "5"]);
    let default = eval_over_csv(FIVE, "tail").await?;
    assert_eq!(default.height(), 5, "the default n is 5");
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_slice() -> Result<(), Box<dyn std::error::Error>> {
    let df = eval_over_csv(FIVE, "slice-1-2").await?;
    assert_eq!(column_strings(&df, "a"), vec!["2", "3"]);
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_filter_eq() -> Result<(), Box<dyn std::error::Error>> {
    let df = eval_over_csv(PEOPLE, "eq-age-30").await?;
    assert_eq!(column_strings(&df, "name"), vec!["Alice"]);
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_comparison_filters() -> Result<(), Box<dyn std::error::Error>> {
    for (action, rows) in [
        ("ne-age-30", 3),
        ("gt-age-28", 2),
        ("gte-age-30", 2),
        ("lt-age-28", 1),
        ("lte-age-28", 2),
        ("is_in-name-Alice-Bob", 2),
    ] {
        let df = eval_over_csv(PEOPLE, action).await?;
        assert_eq!(df.height(), rows, "{action}");
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_null_filters() -> Result<(), Box<dyn std::error::Error>> {
    let nulls = eval_over_csv(WITH_NULL, "is_null-age").await?;
    assert_eq!(column_strings(&nulls, "name"), vec!["Bob"]);
    let not_nulls = eval_over_csv(WITH_NULL, "is_not_null-age").await?;
    assert_eq!(column_strings(&not_nulls, "name"), vec!["Alice", "Carol"]);
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_text_filters() -> Result<(), Box<dyn std::error::Error>> {
    for (action, names) in [
        ("starts_with-name-al", vec!["Alice"]),
        ("starts_with-name-al-t", vec![]),
        ("ends_with-name-E", vec!["Alice", "Charlie", "Dave"]),
        ("contains-name-LI", vec!["Alice", "Charlie"]),
        ("contains-name-LI-t", vec![]),
    ] {
        let df = eval_over_csv(PEOPLE, action).await?;
        assert_eq!(column_strings(&df, "name"), names, "{action}");
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_sort() -> Result<(), Box<dyn std::error::Error>> {
    let ascending = eval_over_csv(PEOPLE, "sort-age").await?;
    assert_eq!(column_strings(&ascending, "age"), vec!["25", "28", "30", "35"]);
    let descending = eval_over_csv(PEOPLE, "sort-age-f").await?;
    assert_eq!(column_strings(&descending, "age"), vec!["35", "30", "28", "25"]);
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_aggregation_sum() -> Result<(), Box<dyn std::error::Error>> {
    let df = eval_over_csv(NUMS, "sum").await?;
    assert_eq!(df.height(), 1);
    assert_eq!(cell_f64(&df, "a"), 9.0);
    assert_eq!(cell_f64(&df, "b"), 12.0);
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_aggregation_mean() -> Result<(), Box<dyn std::error::Error>> {
    let df = eval_over_csv(NUMS, "mean").await?;
    assert_eq!(df.height(), 1);
    assert!((cell_f64(&df, "a") - 3.0).abs() < 1e-9);
    assert!((cell_f64(&df, "b") - 4.0).abs() < 1e-9);
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_remaining_aggregations() -> Result<(), Box<dyn std::error::Error>> {
    for (action, a) in [
        ("median", 3.0),
        ("min", 1.0),
        ("max", 5.0),
        ("std", 2.0),
        ("count", 3.0),
    ] {
        let df = eval_over_csv(NUMS, action).await?;
        assert!((cell_f64(&df, "a") - a).abs() < 1e-9, "{action}: {df}");
    }
    let describe = eval_over_csv(NUMS, "describe").await?;
    let names: Vec<String> = describe
        .get_column_names()
        .iter()
        .map(|n| n.to_string())
        .collect();
    for name in ["a_mean", "a_min", "a_max", "a_std"] {
        assert!(names.iter().any(|n| n == name), "{name} in {names:?}");
    }
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_chained_operations() -> Result<(), Box<dyn std::error::Error>> {
    let df = eval_over_csv(PEOPLE, "eq-city-NYC/select_columns-name-age/sort-age").await?;
    assert_eq!(df.height(), 2);
    assert_eq!(df.width(), 2);
    assert_eq!(column_strings(&df, "name"), vec!["Alice", "Charlie"]);
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_sort_unknown_column_is_rejected() {
    let err = eval_over_csv(PEOPLE, "sort-zz")
        .await
        .expect_err("zz does not exist");
    assert!(err.message.contains("zz"), "message: {}", err.message);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_eq_unknown_column_is_rejected() {
    let err = eval_over_csv(PEOPLE, "eq-zz-1")
        .await
        .expect_err("zz does not exist");
    assert!(err.message.contains("zz"), "message: {}", err.message);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_text_filter_on_numeric_column_is_rejected() {
    let err = eval_over_csv(PEOPLE, "starts_with-age-3")
        .await
        .expect_err("age is not a String column");
    assert!(err.message.contains("age"), "message: {}", err.message);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_parse_utilities() -> Result<(), Box<dyn std::error::Error>> {
    use liquers_lib::polars::util::*;

    // Test date parsing
    let date1 = parse_date("20240115")?;
    assert_eq!(date1.to_string(), "2024-01-15");

    let date2 = parse_date("2024-01-15")?;
    assert_eq!(date2.to_string(), "2024-01-15");

    // Test boolean parsing
    assert_eq!(parse_boolean("true")?, true);
    assert_eq!(parse_boolean("false")?, false);
    assert_eq!(parse_boolean("1")?, true);
    assert_eq!(parse_boolean("0")?, false);

    // Test separator parsing
    assert_eq!(parse_separator("comma")?, b',');
    assert_eq!(parse_separator("tab")?, b'\t');
    assert_eq!(parse_separator("semicolon")?, b';');
    assert_eq!(parse_separator("|")?, b'|');

    Ok(())
}

// ---------------------------------------------------------------------------------------------
// The input state a command receives (specs/design/plan-step-state-metadata/).
// ---------------------------------------------------------------------------------------------

/// Phase 3 test 22 (AC-1, AC-9) - the source issue: a step that produces no value before the
/// action used to replace the input's `data_format: csv` with the asset's `bin`, and polars
/// refused it ("Unsupported polars data_format 'bin'").
#[tokio::test(flavor = "multi_thread")]
async fn slice_after_info_step_keeps_csv_format() -> Result<(), Box<dyn std::error::Error>> {
    use liquers_core::assets::AssetRef;
    use liquers_core::context::Context;
    use liquers_core::interpreter::apply_plan;
    use liquers_core::parse::parse_query;
    use liquers_core::plan::{PlanBuilder, Step};

    let envref = create_test_env().to_ref();
    let mut plan = PlanBuilder::new(
        parse_query("ns-pl/slice-0-2")?,
        envref.get_command_metadata_registry(),
    )
    .build()?;
    plan.steps.insert(0, Step::Info("before the action".to_string()));
    let context = Context::new(AssetRef::new_temporary(envref.clone()), false).await;
    let value = apply_plan(plan, create_csv_state("a\n1\n2\n3"), context, envref).await?;
    assert_eq!(value.as_polars_dataframe()?.height(), 2);
    Ok(())
}

/// Phase 3 test 23 (AC-9) - a CSV placed in the store as untyped bytes reaches `slice` with the
/// stored `data_format`, not the `bin` the asset of an unnamed query used to declare.
#[tokio::test(flavor = "multi_thread")]
async fn slice_over_stored_csv_bytes_reads_the_stored_format(
) -> Result<(), Box<dyn std::error::Error>> {
    use liquers_core::metadata::{Metadata, Status};
    use liquers_core::parse::{parse_key, parse_query};
    use liquers_core::query::Key;
    use liquers_core::store::{AsyncMemoryStore, AsyncStore};

    let store = AsyncMemoryStore::new(&Key::new());
    let key = parse_key("data/x.csv")?;
    let mut metadata = Metadata::new();
    metadata.set_filename("x.csv")?;
    metadata.set_status(Status::Source)?;
    store.set(&key, b"a\n1\n2\n3\n", &metadata).await?;

    let mut env = create_test_env();
    env.with_async_store(Box::new(store));
    let envref = env.to_ref();
    let state = envref
        .evaluate(parse_query("-R/data/x.csv/-/ns-pl/slice-0-2")?)
        .await?
        .get()
        .await?;
    assert_eq!(state.value()?.as_polars_dataframe()?.height(), 2);
    Ok(())
}
