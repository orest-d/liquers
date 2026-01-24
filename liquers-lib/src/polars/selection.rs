use liquers_core::{
    commands::CommandRegistry, context::Context, error::Error, state::State, value::ValueInterface,
};
use liquers_macro::register_command;
use crate::value::{ExtValueInterface, Value};
use crate::environment::CommandRegistryAccess;

use super::util::{check_column_exists, try_to_polars_dataframe};

/// Select specific columns by name
///
/// Arguments:
/// - columns: Column names separated by dashes (e.g., "col1-col2-col3")
fn select_columns(state: &State<Value>, columns: String) -> Result<Value, Error> {
    let df = try_to_polars_dataframe(state)?;

    let col_names: Vec<&str> = columns.split('-').map(|s| s.trim()).collect();

    // Validate all columns exist
    for col in &col_names {
        check_column_exists(&df, col)?;
    }

    let result = df
        .select(col_names)
        .map_err(|e| Error::general_error(format!("Failed to select columns: {}", e)))?;

    Ok(Value::from_polars_dataframe(result))
}

/// Drop specific columns by name
///
/// Arguments:
/// - columns: Column names separated by dashes (e.g., "col1-col2")
fn drop_columns(state: &State<Value>, columns: String) -> Result<Value, Error> {
    let df = try_to_polars_dataframe(state)?;

    let col_names: Vec<&str> = columns.split('-').map(|s| s.trim()).collect();

    // Validate all columns exist
    for col in &col_names {
        check_column_exists(&df, col)?;
    }

    // drop_many returns DataFrame, not Result
    let result = (*df).clone().drop_many(col_names);

    Ok(Value::from_polars_dataframe(result))
}

/// Get first N rows
///
/// Arguments:
/// - n: Number of rows (default: 5)
fn head(state: &State<Value>, n: i32) -> Result<Value, Error> {
    let df = try_to_polars_dataframe(state)?;
    let num_rows = n.max(0) as usize;

    let result = df.head(Some(num_rows));
    Ok(Value::from_polars_dataframe(result))
}

/// Get last N rows
///
/// Arguments:
/// - n: Number of rows (default: 5)
fn tail(state: &State<Value>, n: i32) -> Result<Value, Error> {
    let df = try_to_polars_dataframe(state)?;
    let num_rows = n.max(0) as usize;

    let result = df.tail(Some(num_rows));
    Ok(Value::from_polars_dataframe(result))
}

/// Extract rows by range
///
/// Arguments:
/// - offset: Starting row index (0-based)
/// - length: Number of rows to extract
fn slice(state: &State<Value>, offset: i32, length: i32) -> Result<Value, Error> {
    let df = try_to_polars_dataframe(state)?;

    let offset_i64 = offset.max(0) as i64;
    let length_usize = length.max(0) as usize;

    let result = df.slice(offset_i64, length_usize);
    Ok(Value::from_polars_dataframe(result))
}

/// Register selection and slicing commands
pub fn register_commands(env: &mut crate::environment::DefaultEnvironment<Value>) -> Result<(), Error> {
    type CommandEnvironment = crate::environment::DefaultEnvironment<Value>;
    let cr = env.get_mut_command_registry();
    register_command!(cr,
        fn select_columns(state, columns: String) -> result
        namespace: "pl"
        label: "Select columns"
        doc: "Select columns by name (separated by dashes)"
    )?;

    register_command!(cr,
        fn drop_columns(state, columns: String) -> result
        namespace: "pl"
        label: "Drop columns"
        doc: "Remove columns by name (separated by dashes)"
    )?;

    register_command!(cr,
        fn head(state, n: i32 = 5) -> result
        namespace: "pl"
        label: "Get first rows"
        doc: "Return first N rows (default: 5)"
    )?;

    register_command!(cr,
        fn tail(state, n: i32 = 5) -> result
        namespace: "pl"
        label: "Get last rows"
        doc: "Return last N rows (default: 5)"
    )?;

    register_command!(cr,
        fn slice(state, offset: i32, length: i32) -> result
        namespace: "pl"
        label: "Slice rows"
        doc: "Extract rows by range (offset, length)"
    )?;

    Ok(())
}
