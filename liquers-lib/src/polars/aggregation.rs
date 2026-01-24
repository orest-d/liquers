use liquers_core::{
    commands::CommandRegistry, context::Context, error::Error, state::State, value::ValueInterface,
};
use liquers_macro::register_command;
use crate::value::{ExtValueInterface, Value};
use crate::environment::CommandRegistryAccess;

use super::util::try_to_polars_dataframe;

/// Sum of all numeric columns
fn sum(state: &State<Value>) -> Result<Value, Error> {
    let df = try_to_polars_dataframe(state)?;

    // Use lazy evaluation to compute sum
    use polars::prelude::*;
    let result = (*df)
        .clone()
        .lazy()
        .select([col("*").sum()])
        .collect()
        .map_err(|e| Error::general_error(format!("Failed to compute sum: {}", e)))?;

    Ok(Value::from_polars_dataframe(result))
}

/// Mean of all numeric columns
fn mean(state: &State<Value>) -> Result<Value, Error> {
    let df = try_to_polars_dataframe(state)?;

    // Use lazy evaluation to compute mean
    use polars::prelude::*;
    let result = (*df)
        .clone()
        .lazy()
        .select([col("*").mean()])
        .collect()
        .map_err(|e| Error::general_error(format!("Failed to compute mean: {}", e)))?;

    Ok(Value::from_polars_dataframe(result))
}

/// Median of all numeric columns
fn median(state: &State<Value>) -> Result<Value, Error> {
    let df = try_to_polars_dataframe(state)?;

    // Use lazy evaluation to compute median
    use polars::prelude::*;
    let result = (*df)
        .clone()
        .lazy()
        .select([col("*").median()])
        .collect()
        .map_err(|e| Error::general_error(format!("Failed to compute median: {}", e)))?;

    Ok(Value::from_polars_dataframe(result))
}

/// Minimum of all numeric columns
fn min(state: &State<Value>) -> Result<Value, Error> {
    let df = try_to_polars_dataframe(state)?;

    // Use lazy evaluation to compute min
    use polars::prelude::*;
    let result = (*df)
        .clone()
        .lazy()
        .select([col("*").min()])
        .collect()
        .map_err(|e| Error::general_error(format!("Failed to compute min: {}", e)))?;

    Ok(Value::from_polars_dataframe(result))
}

/// Maximum of all numeric columns
fn max(state: &State<Value>) -> Result<Value, Error> {
    let df = try_to_polars_dataframe(state)?;

    // Use lazy evaluation to compute max
    use polars::prelude::*;
    let result = (*df)
        .clone()
        .lazy()
        .select([col("*").max()])
        .collect()
        .map_err(|e| Error::general_error(format!("Failed to compute max: {}", e)))?;

    Ok(Value::from_polars_dataframe(result))
}

/// Standard deviation of all numeric columns
fn std(state: &State<Value>) -> Result<Value, Error> {
    let df = try_to_polars_dataframe(state)?;

    // Use lazy evaluation to compute std
    use polars::prelude::*;
    let result = (*df)
        .clone()
        .lazy()
        .select([col("*").std(1)]) // ddof=1 for sample std dev
        .collect()
        .map_err(|e| Error::general_error(format!("Failed to compute std: {}", e)))?;

    Ok(Value::from_polars_dataframe(result))
}

/// Count non-null values per column
fn count(state: &State<Value>) -> Result<Value, Error> {
    let df = try_to_polars_dataframe(state)?;

    // Create a new dataframe with count for each column
    use polars::prelude::*;
    let mut columns_vec = Vec::new();
    for col in (*df).get_columns() {
        let count_val = col.len() - col.null_count();
        let series = Series::from_vec(col.name().clone(), vec![count_val as i64]);
        columns_vec.push(series.into()); // Convert Series to Column
    }

    let result = DataFrame::new(columns_vec)
        .map_err(|e| Error::general_error(format!("Failed to compute count: {}", e)))?;

    Ok(Value::from_polars_dataframe(result))
}

/// Statistical summary of DataFrame
fn describe(state: &State<Value>) -> Result<Value, Error> {
    let df = try_to_polars_dataframe(state)?;

    // Compute basic statistics using lazy evaluation
    // We'll return a simpler version with count, mean, min, max
    use polars::prelude::*;

    // Get numeric columns only
    let numeric_cols: Vec<String> = (*df)
        .get_columns()
        .iter()
        .filter(|col| col.dtype().is_numeric())
        .map(|col| col.name().to_string())
        .collect();

    if numeric_cols.is_empty() {
        return Err(Error::general_error("No numeric columns found for describe".to_string()));
    }

    // Create expressions for each statistic
    let mut exprs = vec![];
    for col_name in &numeric_cols {
        let name_str = col_name.as_str();
        exprs.push(col(name_str).mean().alias(&format!("{}_mean", name_str)));
        exprs.push(col(name_str).min().alias(&format!("{}_min", name_str)));
        exprs.push(col(name_str).max().alias(&format!("{}_max", name_str)));
        exprs.push(col(name_str).std(1).alias(&format!("{}_std", name_str)));
    }

    let result = (*df)
        .clone()
        .lazy()
        .select(exprs)
        .collect()
        .map_err(|e| Error::general_error(format!("Failed to compute describe: {}", e)))?;

    Ok(Value::from_polars_dataframe(result))
}

/// Register aggregation commands
pub fn register_commands(env: &mut crate::environment::DefaultEnvironment<Value>) -> Result<(), Error> {
    type CommandEnvironment = crate::environment::DefaultEnvironment<Value>;
    let cr = env.get_mut_command_registry();
    register_command!(cr,
        fn sum(state) -> result
        namespace: "pl"
        label: "Sum"
        doc: "Sum of all numeric columns"
    )?;

    register_command!(cr,
        fn mean(state) -> result
        namespace: "pl"
        label: "Mean"
        doc: "Mean of all numeric columns"
    )?;

    register_command!(cr,
        fn median(state) -> result
        namespace: "pl"
        label: "Median"
        doc: "Median of all numeric columns"
    )?;

    register_command!(cr,
        fn min(state) -> result
        namespace: "pl"
        label: "Minimum"
        doc: "Minimum of all numeric columns"
    )?;

    register_command!(cr,
        fn max(state) -> result
        namespace: "pl"
        label: "Maximum"
        doc: "Maximum of all numeric columns"
    )?;

    register_command!(cr,
        fn std(state) -> result
        namespace: "pl"
        label: "Standard Deviation"
        doc: "Standard deviation of all numeric columns"
    )?;

    register_command!(cr,
        fn count(state) -> result
        namespace: "pl"
        label: "Count"
        doc: "Count non-null values per column"
    )?;

    register_command!(cr,
        fn describe(state) -> result
        namespace: "pl"
        label: "Describe"
        doc: "Statistical summary of DataFrame"
    )?;

    Ok(())
}
