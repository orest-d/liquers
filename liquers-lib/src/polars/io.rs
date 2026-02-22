use crate::environment::CommandRegistryAccess;
use crate::value::{ExtValueInterface, Value};
use liquers_core::{
    commands::CommandRegistry, context::Context, error::Error, state::State, value::ValueInterface,
};
use liquers_macro::register_command;
use std::io::Cursor;

use super::serde::{deserialize_dataframe_from_reader, serialize_dataframe_to_writer};
use super::util::try_to_polars_dataframe;

/// Load DataFrame from CSV
///
/// Arguments:
/// - separator: "comma" (default), "tab", "semicolon", "pipe", or single char
pub fn from_csv(state: &State<Value>, separator: String) -> Result<Value, Error> {
    let format = if separator.is_empty() {
        "csv".to_string()
    } else {
        format!("csv:{}", separator.trim())
    };

    // Get data as bytes
    let csv_data = if let Ok(text) = state.data.try_into_string() {
        text.into_bytes()
    } else {
        state
            .data
            .try_into_bytes()
            .map_err(|e| Error::general_error(format!("Cannot get CSV data as bytes: {}", e)))?
    };

    let df = deserialize_dataframe_from_reader(Cursor::new(csv_data), &format)
        .map_err(|e| Error::general_error(format!("Failed to parse CSV: {}", e)))?;

    Ok(Value::from_polars_dataframe(df))
}

/// Write DataFrame to CSV string
pub fn to_csv(state: &State<Value>) -> Result<Value, Error> {
    let df = try_to_polars_dataframe(state)?;

    let mut buffer = Vec::new();
    serialize_dataframe_to_writer(&df, "csv", &mut buffer)
        .map_err(|e| Error::general_error(format!("Failed to write CSV: {}", e)))?;

    let csv_string = String::from_utf8(buffer)
        .map_err(|e| Error::general_error(format!("Invalid UTF-8 in CSV output: {}", e)))?;

    Ok(Value::from(csv_string))
}

/// Register polars I/O commands via macro.
///
/// The caller must define `type CommandEnvironment = ...` in scope before invoking.
#[macro_export]
macro_rules! register_polars_io_commands {
    ($cr:expr) => {{
        use liquers_macro::register_command;
        use $crate::polars::io::*;

        register_command!($cr,
            fn from_csv(state, separator: String = "") -> result
            namespace: "pl"
            label: "Load DataFrame from CSV"
            doc: "Parse CSV data into a Polars DataFrame. Supports custom separators: comma (default), tab, semicolon, pipe."
            filename: "data.csv"
        )?;

        register_command!($cr,
            fn to_csv(state) -> result
            namespace: "pl"
            label: "Export DataFrame to CSV"
            doc: "Convert DataFrame to CSV string with comma separator"
            filename: "data.csv"
        )?;

        Ok::<(), liquers_core::error::Error>(())
    }};
}

/// Backward-compatible wrapper calling the `register_polars_io_commands!` macro.
pub fn register_commands(
    env: &mut crate::environment::DefaultEnvironment<Value>,
) -> Result<(), Error> {
    type CommandEnvironment = crate::environment::DefaultEnvironment<Value>;
    let cr = env.get_mut_command_registry();
    register_polars_io_commands!(cr)?;
    Ok(())
}
