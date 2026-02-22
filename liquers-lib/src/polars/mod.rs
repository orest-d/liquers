// Polars DataFrame command library
// Implements commands for data manipulation using Polars DataFrames

pub mod aggregation;
pub mod filtering;
pub mod info;
pub mod io;
pub mod selection;
pub mod serde;
pub mod sorting;
pub mod util;

use crate::{environment::CommandRegistryAccess, value::Value};
use liquers_core::error::Error;

/// Register all polars commands via macro.
///
/// The caller must define `type CommandEnvironment = ...` in scope before invoking.
#[macro_export]
macro_rules! register_polars_commands {
    ($cr:expr) => {{
        $crate::register_polars_io_commands!($cr)?;
        $crate::register_polars_selection_commands!($cr)?;
        $crate::register_polars_filtering_commands!($cr)?;
        $crate::register_polars_sorting_commands!($cr)?;
        $crate::register_polars_aggregation_commands!($cr)?;
        $crate::register_polars_info_commands!($cr)?;
        Ok::<(), liquers_core::error::Error>(())
    }};
}

/// Register all Polars commands in the "pl" namespace
pub fn register_commands(
    env: &mut crate::environment::DefaultEnvironment<Value>,
) -> Result<(), Error> {
    // I/O commands
    io::register_commands(env)?;

    // Selection and slicing
    selection::register_commands(env)?;

    // Filtering
    filtering::register_commands(env)?;

    // Sorting
    sorting::register_commands(env)?;

    // Aggregations
    aggregation::register_commands(env)?;

    // Info commands
    info::register_commands(env)?;

    Ok(())
}
