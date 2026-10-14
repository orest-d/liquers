use crate::environment::CommandRegistryAccess;
use crate::value::{ExtValueInterface, Value};
use liquers_core::{
    commands::CommandRegistry, context::Context, error::Error, state::State, value::ValueInterface,
};
use liquers_macro::register_command;
use polars::prelude::*;

use super::util::{
    check_column_exists, parse_boolean, parse_date, parse_datetime, try_to_polars_dataframe,
};

/// A UTC instant from RFC 3339 (`2026-10-02T14:00:00Z`), a zone-less ISO date-time (read as UTC),
/// or a date (midnight UTC).
fn parse_utc_instant(value: &str) -> Result<chrono::DateTime<chrono::Utc>, Error> {
    if let Ok(instant) = chrono::DateTime::parse_from_rfc3339(value) {
        return Ok(instant.with_timezone(&chrono::Utc));
    }
    if let Ok(naive) = parse_datetime(value) {
        return Ok(naive.and_utc());
    }
    let date = parse_date(value)?;
    date.and_hms_opt(0, 0, 0)
        .map(|naive| naive.and_utc())
        .ok_or_else(|| Error::general_error(format!("Invalid date '{}'", value)))
}

/// Parse comparison value based on column data type
fn parse_comparison_value(df: &DataFrame, column: &str, value_str: &str) -> Result<Expr, Error> {
    let schema = df.schema();
    let dtype = schema
        .get(column)
        .ok_or_else(|| Error::general_error(format!("Column '{}' not found", column)))?;

    let value_str = value_str.trim();

    match dtype {
        DataType::Int8 => {
            let val = value_str.parse::<i8>().map_err(|_| {
                Error::general_error(format!(
                    "Cannot parse '{}' as Int8 for column '{}'",
                    value_str, column
                ))
            })?;
            Ok(lit(val))
        }
        DataType::Int16 => {
            let val = value_str.parse::<i16>().map_err(|_| {
                Error::general_error(format!(
                    "Cannot parse '{}' as Int16 for column '{}'",
                    value_str, column
                ))
            })?;
            Ok(lit(val))
        }
        DataType::Int32 => {
            let val = value_str.parse::<i32>().map_err(|_| {
                Error::general_error(format!(
                    "Cannot parse '{}' as Int32 for column '{}'",
                    value_str, column
                ))
            })?;
            Ok(lit(val))
        }
        DataType::Int64 => {
            let val = value_str.parse::<i64>().map_err(|_| {
                Error::general_error(format!(
                    "Cannot parse '{}' as Int64 for column '{}'",
                    value_str, column
                ))
            })?;
            Ok(lit(val))
        }
        DataType::UInt8 => {
            let val = value_str.parse::<u8>().map_err(|_| {
                Error::general_error(format!(
                    "Cannot parse '{}' as UInt8 for column '{}'",
                    value_str, column
                ))
            })?;
            Ok(lit(val))
        }
        DataType::UInt16 => {
            let val = value_str.parse::<u16>().map_err(|_| {
                Error::general_error(format!(
                    "Cannot parse '{}' as UInt16 for column '{}'",
                    value_str, column
                ))
            })?;
            Ok(lit(val))
        }
        DataType::UInt32 => {
            let val = value_str.parse::<u32>().map_err(|_| {
                Error::general_error(format!(
                    "Cannot parse '{}' as UInt32 for column '{}'",
                    value_str, column
                ))
            })?;
            Ok(lit(val))
        }
        DataType::UInt64 => {
            let val = value_str.parse::<u64>().map_err(|_| {
                Error::general_error(format!(
                    "Cannot parse '{}' as UInt64 for column '{}'",
                    value_str, column
                ))
            })?;
            Ok(lit(val))
        }
        DataType::Float32 => {
            let val = value_str.parse::<f32>().map_err(|_| {
                Error::general_error(format!(
                    "Cannot parse '{}' as Float32 for column '{}'",
                    value_str, column
                ))
            })?;
            Ok(lit(val))
        }
        DataType::Float64 => {
            let val = value_str.parse::<f64>().map_err(|_| {
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
        // A record Timestamp crosses the bridge as Datetime(µs, "UTC"). The value is RFC 3339, an
        // ISO date-time (read as UTC), or a date (midnight UTC) — as `ns-rec` widens a date. It
        // becomes an epoch count in the column's unit, cast to the column's type.
        DataType::Datetime(unit, _) => {
            let instant = parse_utc_instant(value_str)?;
            let count = match unit {
                TimeUnit::Nanoseconds => instant.timestamp_nanos_opt().ok_or_else(|| {
                    Error::general_error(format!(
                        "'{}' is outside the nanosecond range for column '{}'",
                        value_str, column
                    ))
                })?,
                TimeUnit::Microseconds => instant.timestamp_micros(),
                TimeUnit::Milliseconds => instant.timestamp_millis(),
            };
            Ok(lit(count).cast(dtype.clone()))
        }
        _ => Err(Error::general_error(format!(
            "Comparison not supported for column '{}' of type {:?}",
            column, dtype
        ))),
    }
}

/// Equal to filter
#[liquers_macro::command_version]
pub fn eq(state: &State<Value>, column: String, value: String) -> Result<Value, Error> {
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
#[liquers_macro::command_version]
pub fn ne(state: &State<Value>, column: String, value: String) -> Result<Value, Error> {
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
#[liquers_macro::command_version]
pub fn gt(state: &State<Value>, column: String, value: String) -> Result<Value, Error> {
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
#[liquers_macro::command_version]
pub fn gte(state: &State<Value>, column: String, value: String) -> Result<Value, Error> {
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
#[liquers_macro::command_version]
pub fn lt(state: &State<Value>, column: String, value: String) -> Result<Value, Error> {
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
#[liquers_macro::command_version]
pub fn lte(state: &State<Value>, column: String, value: String) -> Result<Value, Error> {
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

/// Applies `predicate` as a row filter.
fn filter_rows(df: &DataFrame, predicate: Expr) -> Result<Value, Error> {
    let result = df
        .clone()
        .lazy()
        .filter(predicate)
        .collect()
        .map_err(|e| Error::general_error(format!("Filter failed: {}", e)))?;
    Ok(Value::from_polars_dataframe(result))
}

/// Rows whose column equals one of `values` (each parsed to the column's type). An OR of
/// equalities, so it needs no polars `is_in` feature and matches `ns-rec/is_in` exactly.
#[liquers_macro::command_version]
pub fn is_in(state: &State<Value>, column: String, values: Vec<String>) -> Result<Value, Error> {
    let df = try_to_polars_dataframe(state)?;
    check_column_exists(&df, &column)?;
    let mut predicate: Option<Expr> = None;
    for value in &values {
        let test = col(&column).eq(parse_comparison_value(&df, &column, value)?);
        predicate = Some(match predicate {
            Some(previous) => previous.or(test),
            None => test,
        });
    }
    let predicate = predicate.ok_or_else(|| {
        Error::general_error("ns-pl/is_in requires at least one value".to_string())
    })?;
    filter_rows(&df, predicate)
}

#[liquers_macro::command_version]
pub fn is_null(state: &State<Value>, column: String) -> Result<Value, Error> {
    let df = try_to_polars_dataframe(state)?;
    check_column_exists(&df, &column)?;
    filter_rows(&df, col(&column).is_null())
}

#[liquers_macro::command_version]
pub fn is_not_null(state: &State<Value>, column: String) -> Result<Value, Error> {
    let df = try_to_polars_dataframe(state)?;
    check_column_exists(&df, &column)?;
    filter_rows(&df, col(&column).is_not_null())
}

/// Which literal text test a text filter applies.
#[derive(Debug, Clone, Copy)]
enum TextTest {
    StartsWith,
    EndsWith,
    Contains,
}

/// A literal text test on a String column; case-insensitive (both sides lowercased) unless
/// `case_sensitive` — the `ns-rec` default, deliberately unlike polars (conventions.md).
fn text_filter(
    state: &State<Value>,
    column: &str,
    text: &str,
    case_sensitive: bool,
    test: TextTest,
) -> Result<Value, Error> {
    let df = try_to_polars_dataframe(state)?;
    check_column_exists(&df, column)?;
    let dtype = df
        .schema()
        .get(column)
        .cloned()
        .ok_or_else(|| Error::general_error(format!("Column '{}' not found", column)))?;
    if dtype != DataType::String {
        return Err(Error::general_error(format!(
            "Column '{}' is {:?}; a text filter needs a String column",
            column, dtype
        )));
    }
    let (subject, pattern) = if case_sensitive {
        (col(column), text.to_string())
    } else {
        (col(column).str().to_lowercase(), text.to_lowercase())
    };
    let predicate = match test {
        TextTest::StartsWith => subject.str().starts_with(lit(pattern)),
        TextTest::EndsWith => subject.str().ends_with(lit(pattern)),
        TextTest::Contains => subject.str().contains_literal(lit(pattern)),
    };
    filter_rows(&df, predicate)
}

#[liquers_macro::command_version]
pub fn starts_with(
    state: &State<Value>,
    column: String,
    text: String,
    case_sensitive: bool,
) -> Result<Value, Error> {
    text_filter(state, &column, &text, case_sensitive, TextTest::StartsWith)
}

#[liquers_macro::command_version]
pub fn ends_with(
    state: &State<Value>,
    column: String,
    text: String,
    case_sensitive: bool,
) -> Result<Value, Error> {
    text_filter(state, &column, &text, case_sensitive, TextTest::EndsWith)
}

#[liquers_macro::command_version]
pub fn contains(
    state: &State<Value>,
    column: String,
    text: String,
    case_sensitive: bool,
) -> Result<Value, Error> {
    text_filter(state, &column, &text, case_sensitive, TextTest::Contains)
}

/// Register polars filtering commands via macro.
///
/// The caller must define `type CommandEnvironment = ...` in scope before invoking.
#[macro_export]
macro_rules! register_polars_filtering_commands {
    ($cr:expr) => {{
        use liquers_macro::register_command;
        use $crate::polars::filtering::*;

        register_command!($cr,
            fn eq(state, column: String, value: String) -> result
            namespace: "pl"
            label: "Equal to"
            doc: "Filter rows where column equals value"

        version: auto
        )?;

        register_command!($cr,
            fn ne(state, column: String, value: String) -> result
            namespace: "pl"
            label: "Not equal to"
            doc: "Filter rows where column does not equal value"

        version: auto
        )?;

        register_command!($cr,
            fn gt(state, column: String, value: String) -> result
            namespace: "pl"
            label: "Greater than"
            doc: "Filter rows where column is greater than value"

        version: auto
        )?;

        register_command!($cr,
            fn gte(state, column: String, value: String) -> result
            namespace: "pl"
            label: "Greater than or equal"
            doc: "Filter rows where column is greater than or equal to value"

        version: auto
        )?;

        register_command!($cr,
            fn lt(state, column: String, value: String) -> result
            namespace: "pl"
            label: "Less than"
            doc: "Filter rows where column is less than value"

        version: auto
        )?;

        register_command!($cr,
            fn lte(state, column: String, value: String) -> result
            namespace: "pl"
            label: "Less than or equal"
            doc: "Filter rows where column is less than or equal to value"

        version: auto
        )?;

        register_command!($cr,
            fn is_in(state, column: String, values: Vec<String> multiple) -> result
            namespace: "pl"
            label: "Is in"
            doc: "Filter rows where column equals one of the values"

        version: auto
        )?;

        register_command!($cr,
            fn is_null(state, column: String) -> result
            namespace: "pl"
            label: "Is null"
            doc: "Filter rows where column is null"

        version: auto
        )?;

        register_command!($cr,
            fn is_not_null(state, column: String) -> result
            namespace: "pl"
            label: "Is not null"
            doc: "Filter rows where column is not null"

        version: auto
        )?;

        register_command!($cr,
            fn starts_with(state, column: String, text: String, case_sensitive: bool = false) -> result
            namespace: "pl"
            label: "Starts with"
            doc: "Filter rows whose text starts with the given text, taken literally; case-insensitive unless case_sensitive"

        version: auto
        )?;

        register_command!($cr,
            fn ends_with(state, column: String, text: String, case_sensitive: bool = false) -> result
            namespace: "pl"
            label: "Ends with"
            doc: "Filter rows whose text ends with the given text, taken literally; case-insensitive unless case_sensitive"

        version: auto
        )?;

        register_command!($cr,
            fn contains(state, column: String, text: String, case_sensitive: bool = false) -> result
            namespace: "pl"
            label: "Contains"
            doc: "Filter rows whose text contains the given text, taken literally; case-insensitive unless case_sensitive"

        version: auto
        )?;

        Ok::<(), liquers_core::error::Error>(())
    }};
}

/// Backward-compatible wrapper calling the `register_polars_filtering_commands!` macro.
pub fn register_commands(
    cr: &mut liquers_core::commands::CommandRegistry<
        crate::environment::DefaultEnvironment<Value>,
    >,
) -> Result<(), Error> {
    type CommandEnvironment = crate::environment::DefaultEnvironment<Value>;
    register_polars_filtering_commands!(cr)?;
    Ok(())
}
