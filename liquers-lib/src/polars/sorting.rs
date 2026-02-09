use liquers_core::{
    commands::CommandRegistry, context::Context, error::Error, state::State, value::ValueInterface,
};
use liquers_macro::register_command;
use crate::value::{ExtValueInterface, Value};
use crate::environment::CommandRegistryAccess;
use polars::prelude::*;

use super::util::{check_column_exists, try_to_polars_dataframe};

/// Sort DataFrame by column
///
/// Arguments:
/// - column: Column name to sort by
/// - ascending (optional): "t" or "true" for ascending (default), "f" or "false" for descending
pub fn sort(state: &State<Value>, column: String, ascending: String) -> Result<Value, Error> {
    let df = try_to_polars_dataframe(state)?;
    check_column_exists(&df, &column)?;

    // Parse ascending flag (default true if empty)
    let is_ascending = if ascending.is_empty() {
        true
    } else {
        let asc_lower = ascending.trim().to_lowercase();
        match asc_lower.as_str() {
            "t" | "true" | "1" => true,
            "f" | "false" | "0" => false,
            _ => {
                return Err(Error::general_error(format!(
                    "Invalid ascending flag '{}'. Use 't', 'true', 'f', or 'false'",
                    ascending
                )));
            }
        }
    };

    // Use lazy evaluation for sorting in Polars 0.51.0
    use polars::prelude::*;
    let result = (*df)
        .clone()
        .lazy()
        .sort([column.as_str()], SortMultipleOptions::default().with_order_descending(!is_ascending))
        .collect()
        .map_err(|e| Error::general_error(format!("Failed to sort: {}", e)))?;

    Ok(Value::from_polars_dataframe(result))
}

/// Register polars sorting commands via macro.
///
/// The caller must define `type CommandEnvironment = ...` in scope before invoking.
#[macro_export]
macro_rules! register_polars_sorting_commands {
    ($cr:expr) => {{
        use liquers_macro::register_command;
        use $crate::polars::sorting::*;

        register_command!($cr,
            fn sort(state, column: String, ascending: String = "t") -> result
            namespace: "pl"
            label: "Sort by column"
            doc: "Sort DataFrame by column. Use 't'/'true' for ascending (default), 'f'/'false' for descending"
        )?;

        Ok::<(), liquers_core::error::Error>(())
    }};
}

/// Backward-compatible wrapper calling the `register_polars_sorting_commands!` macro.
pub fn register_commands(env: &mut crate::environment::DefaultEnvironment<Value>) -> Result<(), Error> {
    type CommandEnvironment = crate::environment::DefaultEnvironment<Value>;
    let cr = env.get_mut_command_registry();
    register_polars_sorting_commands!(cr)?;
    Ok(())
}
