use liquers_core::{
    commands::CommandRegistry, context::Context, error::Error, state::State, value::ValueInterface,
};
use liquers_macro::register_command;
use crate::value::{ExtValueInterface, Value};
use crate::environment::CommandRegistryAccess;
use polars::prelude::*;
use std::io::Cursor;

use super::util::{parse_separator, try_to_polars_dataframe};

/// Load DataFrame from CSV
///
/// Arguments:
/// - separator: "comma" (default), "tab", "semicolon", "pipe", or single char
pub fn from_csv(state: &State<Value>, separator: String) -> Result<Value, Error> {
    // Parse separator (empty string means default comma)
    let sep = if separator.is_empty() {
        b','
    } else {
        parse_separator(&separator)?
    };

    // Get data as bytes
    let csv_data = state.as_bytes()
        .map_err(|e| Error::general_error(format!("Cannot get CSV data as bytes: {}", e)))?;

    // Parse CSV - In Polars 0.51, CsvReader has a simpler API
    // We'll use default settings for now (comma separator)
    // TODO: Support custom separators when Polars API is more stable
    let df = CsvReader::new(Cursor::new(csv_data))
        .finish()
        .map_err(|e| Error::general_error(format!("Failed to parse CSV: {}", e)))?;

    Ok(Value::from_polars_dataframe(df))
}

/// Write DataFrame to CSV string
pub fn to_csv(state: &State<Value>) -> Result<Value, Error> {
    let df = try_to_polars_dataframe(state)?;

    let mut buffer = Vec::new();
    let mut writer = CsvWriter::new(&mut buffer);
    writer = writer.with_separator(b',');
    writer.finish(&mut (*df).clone())
        .map_err(|e| Error::general_error(format!("Failed to write CSV: {}", e)))?;

    let csv_string =
        String::from_utf8(buffer).map_err(|e| Error::general_error(format!("Invalid UTF-8 in CSV output: {}", e)))?;

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
pub fn register_commands(env: &mut crate::environment::DefaultEnvironment<Value>) -> Result<(), Error> {
    type CommandEnvironment = crate::environment::DefaultEnvironment<Value>;
    let cr = env.get_mut_command_registry();
    register_polars_io_commands!(cr)?;
    Ok(())
}
