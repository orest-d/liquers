// Polars DataFrame command library
// Implements commands for data manipulation using Polars DataFrames

pub mod util;
pub mod io;
pub mod selection;
pub mod filtering;
pub mod sorting;
pub mod aggregation;
pub mod info;

use liquers_core::error::Error;
use crate::{environment::CommandRegistryAccess, value::Value};

/// Register all Polars commands in the "pl" namespace
pub fn register_commands(env: &mut crate::environment::DefaultEnvironment<Value>) -> Result<(), Error> {
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
