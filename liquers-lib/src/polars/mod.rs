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

/// Register all Polars commands in the "pl" namespace.
///
/// Takes the command registry rather than the environment, so it serves both construction paths:
/// an `EnvironmentBuilder`'s `command_registry` field and an environment's own. The two are the
/// same type, because `DefaultEnvironment` is an alias.
pub fn register_commands(
    cr: &mut liquers_core::commands::CommandRegistry<
        crate::environment::DefaultEnvironment<Value>,
    >,
) -> Result<(), Error> {
    // I/O commands
    io::register_commands(cr)?;

    // Selection and slicing
    selection::register_commands(cr)?;

    // Filtering
    filtering::register_commands(cr)?;

    // Sorting
    sorting::register_commands(cr)?;

    // Aggregations
    aggregation::register_commands(cr)?;

    // Info commands
    info::register_commands(cr)?;

    Ok(())
}
