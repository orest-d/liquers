//! Integration tests for the Polars command library.
//!
//! The whole file needs the optional `polars` dependency, so it is gated at file level: without
//! the feature the test target still compiles, it simply contains no tests.

#![cfg(feature = "polars")]

use liquers_core::{context::Environment, error::Error, metadata::MetadataRecord, state::State};
use liquers_lib::{
    // `register_polars_commands` is an extension-trait method since `DefaultEnvironment` became an
    // alias of a `liquers-core` type: Rust permits an inherent `impl` only in the defining crate.
    environment::{DefaultEnvironment, PolarsCommandRegistration},
    value::{simple::SimpleValue, ExtValueInterface, Value},
};
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

#[tokio::test(flavor = "multi_thread")]
async fn test_from_csv_basic() -> Result<(), Box<dyn std::error::Error>> {
    let csv_data = "name,age,city\nAlice,30,NYC\nBob,25,LA\nCharlie,35,Chicago";
    let state = create_csv_state(csv_data);

    // Use try_to_polars_dataframe from util
    let df = liquers_lib::polars::util::try_to_polars_dataframe(&state)?;

    assert_eq!(df.height(), 3); // 3 rows
    assert_eq!(df.width(), 3); // 3 columns

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_shape() -> Result<(), Box<dyn std::error::Error>> {
    use liquers_lib::polars::info;

    let csv_data = "a,b,c\n1,2,3\n4,5,6";
    let mut state = create_csv_state(csv_data);

    // Convert CSV to DataFrame first
    let df = liquers_lib::polars::util::try_to_polars_dataframe(&state)?;
    state = state.with_data(Value::from_polars_dataframe((*df).clone()));

    // Now test shape command - we need to call it through the command system
    // For now, let's just test the DataFrame directly
    assert_eq!(df.height(), 2);
    assert_eq!(df.width(), 3);

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_head() -> Result<(), Box<dyn std::error::Error>> {
    let csv_data = "a\n1\n2\n3\n4\n5";
    let state = create_csv_state(csv_data);

    let df = liquers_lib::polars::util::try_to_polars_dataframe(&state)?;

    // Test head operation
    let result_df = df.head(Some(2));
    assert_eq!(result_df.height(), 2); // Only first 2 rows

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_tail() -> Result<(), Box<dyn std::error::Error>> {
    let csv_data = "a\n1\n2\n3\n4\n5";
    let state = create_csv_state(csv_data);

    let df = liquers_lib::polars::util::try_to_polars_dataframe(&state)?;

    // Test tail operation
    let result_df = df.tail(Some(2));
    assert_eq!(result_df.height(), 2); // Only last 2 rows

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_slice() -> Result<(), Box<dyn std::error::Error>> {
    let csv_data = "a\n1\n2\n3\n4\n5";
    let state = create_csv_state(csv_data);

    let df = liquers_lib::polars::util::try_to_polars_dataframe(&state)?;

    // Test slice operation
    let result_df = df.slice(1, 2);
    assert_eq!(result_df.height(), 2); // 2 rows starting from index 1

    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Variadic column selection.
//
// These go THROUGH the commands - `create_test_env()` above builds an environment with the polars
// commands registered, and `eval_over_csv` evaluates a real query against it. The rest of this
// file calls the Polars API directly and would pass however the commands behaved; see
// POLARS-COMMAND-TESTS-BYPASS-COMMANDS.
//
// See specs/design/variadic-arguments-declaration/.
// ---------------------------------------------------------------------------------------------

/// Apply a `pl` query to a CSV state, going THROUGH the command registry.
///
/// `apply` runs a transform query against a supplied state, which is what these tests need: no
/// store, no recipe, just state -> command -> state. `create_test_env()` above has already
/// registered the polars commands.
async fn eval_over_csv(csv: &str, action: &str) -> Result<Arc<polars::prelude::DataFrame>, Error> {
    use liquers_core::assets::AssetManager;
    use liquers_core::parse::parse_query;

    let env = create_test_env();
    let envref = env.to_ref();
    let query = parse_query(&format!("ns-pl/{action}"))?;

    let assetref = envref
        .get_asset_manager()
        .apply(query.into(), create_csv_state(csv))
        .await?;
    let state = assetref.get().await?;
    liquers_lib::polars::util::try_to_polars_dataframe(&state)
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

#[tokio::test(flavor = "multi_thread")]
async fn test_filter_eq() -> Result<(), Box<dyn std::error::Error>> {
    use polars::prelude::*;

    let csv_data = "name,age\nAlice,30\nBob,25\nCharlie,30";
    let state = create_csv_state(csv_data);

    let df = liquers_lib::polars::util::try_to_polars_dataframe(&state)?;

    // Test equal filter using lazy evaluation
    let result = (*df)
        .clone()
        .lazy()
        .filter(col("age").eq(lit(30)))
        .collect()?;

    assert_eq!(result.height(), 2); // Alice and Charlie (both age 30)

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_sort() -> Result<(), Box<dyn std::error::Error>> {
    use polars::prelude::*;

    let csv_data = "name,age\nCharlie,35\nAlice,30\nBob,25";
    let state = create_csv_state(csv_data);

    let df = liquers_lib::polars::util::try_to_polars_dataframe(&state)?;

    // Test sorting
    let result = (*df)
        .clone()
        .lazy()
        .sort(["age"], SortMultipleOptions::default())
        .collect()?;

    // Check first row has lowest age
    let age_col = result.column("age")?;
    let first_age = age_col.get(0)?;
    assert_eq!(first_age.to_string(), "25");

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_aggregation_sum() -> Result<(), Box<dyn std::error::Error>> {
    use polars::prelude::*;

    let csv_data = "a,b\n1,2\n3,4\n5,6";
    let state = create_csv_state(csv_data);

    let df = liquers_lib::polars::util::try_to_polars_dataframe(&state)?;

    // Test sum aggregation
    let result = (*df).clone().lazy().select([col("*").sum()]).collect()?;

    assert_eq!(result.height(), 1);

    // Check the sum values
    let a_sum = result.column("a")?.get(0)?;
    assert_eq!(a_sum.to_string(), "9"); // 1+3+5 = 9

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_aggregation_mean() -> Result<(), Box<dyn std::error::Error>> {
    use polars::prelude::*;

    let csv_data = "a,b\n1,2\n3,4\n5,6";
    let state = create_csv_state(csv_data);

    let df = liquers_lib::polars::util::try_to_polars_dataframe(&state)?;

    // Test mean aggregation
    let result = (*df).clone().lazy().select([col("*").mean()]).collect()?;

    assert_eq!(result.height(), 1);

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_chained_operations() -> Result<(), Box<dyn std::error::Error>> {
    use polars::prelude::*;

    let csv_data = "name,age,city\nAlice,30,NYC\nBob,25,LA\nCharlie,35,NYC\nDave,28,Chicago";
    let state = create_csv_state(csv_data);

    let df = liquers_lib::polars::util::try_to_polars_dataframe(&state)?;

    // Chain operations: filter NYC, select name and age, sort by age
    let result = (*df)
        .clone()
        .lazy()
        .filter(col("city").eq(lit("NYC")))
        .select([col("name"), col("age")])
        .sort(["age"], SortMultipleOptions::default())
        .collect()?;

    // Should have 2 rows (Alice and Charlie from NYC)
    assert_eq!(result.height(), 2);
    // Should have 2 columns (name and age)
    assert_eq!(result.width(), 2);
    // Should be sorted by age ascending (Alice first with age 30)
    let age_col = result.column("age")?;
    let first_age = age_col.get(0)?;
    assert_eq!(first_age.to_string(), "30");

    Ok(())
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
