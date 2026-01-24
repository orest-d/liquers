use liquers_core::{
    commands::CommandRegistry, context::Context, error::Error, state::State, value::ValueInterface,
};
use liquers_macro::register_command;
use crate::value::{ExtValueInterface, Value};
use crate::environment::CommandRegistryAccess;
use polars::prelude::*;

use super::util::{check_column_exists, parse_boolean, parse_date, try_to_polars_dataframe};

/// Parse comparison value based on column data type
fn parse_comparison_value(
    df: &DataFrame,
    column: &str,
    value_str: &str,
) -> Result<Expr, Error> {
    let schema = df.schema();
    let dtype = schema
        .get(column)
        .ok_or_else(|| Error::general_error(format!("Column '{}' not found", column)))?;

    let value_str = value_str.trim();

    match dtype {
        DataType::Int8 => {
            let val = value_str
                .parse::<i8>()
                .map_err(|_| {
                    Error::general_error(format!(
                        "Cannot parse '{}' as Int8 for column '{}'",
                        value_str, column
                    ))
                })?;
            Ok(lit(val))
        }
        DataType::Int16 => {
            let val = value_str
                .parse::<i16>()
                .map_err(|_| {
                    Error::general_error(format!(
                        "Cannot parse '{}' as Int16 for column '{}'",
                        value_str, column
                    ))
                })?;
            Ok(lit(val))
        }
        DataType::Int32 => {
            let val = value_str
                .parse::<i32>()
                .map_err(|_| {
                    Error::general_error(format!(
                        "Cannot parse '{}' as Int32 for column '{}'",
                        value_str, column
                    ))
                })?;
            Ok(lit(val))
        }
        DataType::Int64 => {
            let val = value_str
                .parse::<i64>()
                .map_err(|_| {
                    Error::general_error(format!(
                        "Cannot parse '{}' as Int64 for column '{}'",
                        value_str, column
                    ))
                })?;
            Ok(lit(val))
        }
        DataType::UInt8 => {
            let val = value_str
                .parse::<u8>()
                .map_err(|_| {
                    Error::general_error(format!(
                        "Cannot parse '{}' as UInt8 for column '{}'",
                        value_str, column
                    ))
                })?;
            Ok(lit(val))
        }
        DataType::UInt16 => {
            let val = value_str
                .parse::<u16>()
                .map_err(|_| {
                    Error::general_error(format!(
                        "Cannot parse '{}' as UInt16 for column '{}'",
                        value_str, column
                    ))
                })?;
            Ok(lit(val))
        }
        DataType::UInt32 => {
            let val = value_str
                .parse::<u32>()
                .map_err(|_| {
                    Error::general_error(format!(
                        "Cannot parse '{}' as UInt32 for column '{}'",
                        value_str, column
                    ))
                })?;
            Ok(lit(val))
        }
        DataType::UInt64 => {
            let val = value_str
                .parse::<u64>()
                .map_err(|_| {
                    Error::general_error(format!(
                        "Cannot parse '{}' as UInt64 for column '{}'",
                        value_str, column
                    ))
                })?;
            Ok(lit(val))
        }
        DataType::Float32 => {
            let val = value_str
                .parse::<f32>()
                .map_err(|_| {
                    Error::general_error(format!(
                        "Cannot parse '{}' as Float32 for column '{}'",
                        value_str, column
                    ))
                })?;
            Ok(lit(val))
        }
        DataType::Float64 => {
            let val = value_str
                .parse::<f64>()
                .map_err(|_| {
                    Error::general_error(format!(
                        "Cannot parse '{}' as Float64 for column '{}'",
                        value_str, column
                    ))
                })?;
            Ok(lit(val))
        }
        DataType::Boolean => {
            let val = parse_boolean(value_str)?;
            Ok(lit(val))
        }
        DataType::String => {
            // Use string value as-is, no trimming
            Ok(lit(value_str))
        }
        DataType::Date => {
            let date = parse_date(value_str)?;
            // Convert NaiveDate to days since Unix epoch (1970-01-01)
            let unix_epoch = chrono::NaiveDate::from_ymd_opt(1970, 1, 1).unwrap();
            let days_since_epoch = date.signed_duration_since(unix_epoch).num_days() as i32;
            Ok(lit(days_since_epoch))
        }
        _ => Err(Error::general_error(format!(
            "Comparison not supported for column '{}' of type {:?}",
            column, dtype
        ))),
    }
}

/// Equal to filter
fn eq(state: &State<Value>, column: String, value: String) -> Result<Value, Error> {
    let df = try_to_polars_dataframe(state)?;
    check_column_exists(&df, &column)?;

    let value_expr = parse_comparison_value(&df, &column, &value)?;
    let filter_expr = col(&column).eq(value_expr);

    let result = (*df)
        .clone()
        .lazy()
        .filter(filter_expr)
        .collect()
        .map_err(|e| Error::general_error(format!("Filter failed: {}", e)))?;

    Ok(Value::from_polars_dataframe(result))
}

/// Not equal to filter
fn ne(state: &State<Value>, column: String, value: String) -> Result<Value, Error> {
    let df = try_to_polars_dataframe(state)?;
    check_column_exists(&df, &column)?;

    let value_expr = parse_comparison_value(&df, &column, &value)?;
    let filter_expr = col(&column).neq(value_expr);

    let result = (*df)
        .clone()
        .lazy()
        .filter(filter_expr)
        .collect()
        .map_err(|e| Error::general_error(format!("Filter failed: {}", e)))?;

    Ok(Value::from_polars_dataframe(result))
}

/// Greater than filter
fn gt(state: &State<Value>, column: String, value: String) -> Result<Value, Error> {
    let df = try_to_polars_dataframe(state)?;
    check_column_exists(&df, &column)?;

    let value_expr = parse_comparison_value(&df, &column, &value)?;
    let filter_expr = col(&column).gt(value_expr);

    let result = (*df)
        .clone()
        .lazy()
        .filter(filter_expr)
        .collect()
        .map_err(|e| Error::general_error(format!("Filter failed: {}", e)))?;

    Ok(Value::from_polars_dataframe(result))
}

/// Greater than or equal filter
fn gte(state: &State<Value>, column: String, value: String) -> Result<Value, Error> {
    let df = try_to_polars_dataframe(state)?;
    check_column_exists(&df, &column)?;

    let value_expr = parse_comparison_value(&df, &column, &value)?;
    let filter_expr = col(&column).gt_eq(value_expr);

    let result = (*df)
        .clone()
        .lazy()
        .filter(filter_expr)
        .collect()
        .map_err(|e| Error::general_error(format!("Filter failed: {}", e)))?;

    Ok(Value::from_polars_dataframe(result))
}

/// Less than filter
fn lt(state: &State<Value>, column: String, value: String) -> Result<Value, Error> {
    let df = try_to_polars_dataframe(state)?;
    check_column_exists(&df, &column)?;

    let value_expr = parse_comparison_value(&df, &column, &value)?;
    let filter_expr = col(&column).lt(value_expr);

    let result = (*df)
        .clone()
        .lazy()
        .filter(filter_expr)
        .collect()
        .map_err(|e| Error::general_error(format!("Filter failed: {}", e)))?;

    Ok(Value::from_polars_dataframe(result))
}

/// Less than or equal filter
fn lte(state: &State<Value>, column: String, value: String) -> Result<Value, Error> {
    let df = try_to_polars_dataframe(state)?;
    check_column_exists(&df, &column)?;

    let value_expr = parse_comparison_value(&df, &column, &value)?;
    let filter_expr = col(&column).lt_eq(value_expr);

    let result = (*df)
        .clone()
        .lazy()
        .filter(filter_expr)
        .collect()
        .map_err(|e| Error::general_error(format!("Filter failed: {}", e)))?;

    Ok(Value::from_polars_dataframe(result))
}

/// Register filtering commands
pub fn register_commands(env: &mut crate::environment::DefaultEnvironment<Value>) -> Result<(), Error> {
    type CommandEnvironment = crate::environment::DefaultEnvironment<Value>;
    let cr = env.get_mut_command_registry();
    register_command!(cr,
        fn eq(state, column: String, value: String) -> result
        namespace: "pl"
        label: "Equal to"
        doc: "Filter rows where column equals value"
    )?;

    register_command!(cr,
        fn ne(state, column: String, value: String) -> result
        namespace: "pl"
        label: "Not equal to"
        doc: "Filter rows where column does not equal value"
    )?;

    register_command!(cr,
        fn gt(state, column: String, value: String) -> result
        namespace: "pl"
        label: "Greater than"
        doc: "Filter rows where column is greater than value"
    )?;

    register_command!(cr,
        fn gte(state, column: String, value: String) -> result
        namespace: "pl"
        label: "Greater than or equal"
        doc: "Filter rows where column is greater than or equal to value"
    )?;

    register_command!(cr,
        fn lt(state, column: String, value: String) -> result
        namespace: "pl"
        label: "Less than"
        doc: "Filter rows where column is less than value"
    )?;

    register_command!(cr,
        fn lte(state, column: String, value: String) -> result
        namespace: "pl"
        label: "Less than or equal"
        doc: "Filter rows where column is less than or equal to value"
    )?;

    Ok(())
}
