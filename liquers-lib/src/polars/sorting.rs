use crate::environment::CommandRegistryAccess;
use crate::value::{ExtValueInterface, Value};
use liquers_core::{
    commands::CommandRegistry, context::Context, error::Error, state::State, value::ValueInterface,
};
use liquers_macro::register_command;
use polars::prelude::*;

use super::util::{check_column_exists, try_to_polars_dataframe, CollectOnAnyRuntime};

/// Sort DataFrame by column — stable, nulls first unless `nulls_last` (polars' default placement),
/// so `ns-pl/sort` and `ns-rec/sort` order every table identically.
#[liquers_macro::command_version]
pub fn sort(
    state: &State<Value>,
    column: String,
    ascending: bool,
    nulls_last: bool,
) -> Result<Value, Error> {
    let df = try_to_polars_dataframe(state)?;
    check_column_exists(&df, &column)?;
    let options = SortMultipleOptions::default()
        .with_order_descending(!ascending)
        .with_nulls_last(nulls_last)
        .with_maintain_order(true);
    let result = (*df)
        .clone()
        .lazy()
        .sort([column.as_str()], options)
        .collect_on_any_runtime()
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
            fn sort(state, column: String, ascending: bool = true, nulls_last: bool = false) -> result
            namespace: "pl"
            label: "Sort by column"
            doc: "Stable sort by one column; ascending unless ascending=f, nulls first unless nulls_last"

        version: auto
        )?;

        Ok::<(), liquers_core::error::Error>(())
    }};
}

/// Backward-compatible wrapper calling the `register_polars_sorting_commands!` macro.
pub fn register_commands(
    cr: &mut liquers_core::commands::CommandRegistry<
        crate::environment::DefaultEnvironment<Value>,
    >,
) -> Result<(), Error> {
    type CommandEnvironment = crate::environment::DefaultEnvironment<Value>;
    register_polars_sorting_commands!(cr)?;
    Ok(())
}
