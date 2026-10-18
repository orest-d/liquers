//! The built-in Rust command set, registered into every Python environment.
//!
//! A fresh `lq.Environment()` evaluates `liquers-lib` queries with nothing registered (AC-2): the
//! core commands, `ns-pl`, images, records, search and templates. `egui` is not linked, because a
//! Python process has no egui event loop, and `lui` needs a UI payload this environment does not
//! have.
//!
//! # Why this is its own module
//!
//! The `register_*_commands!` macros expand into the calling crate and name `futures`, `Context`,
//! `serde_json` and `liquers_macro` as if they were the caller's own imports, and they need a
//! `type CommandEnvironment` in scope. Isolating the expansion here keeps those imports out of
//! `environment.rs`, where they would look unexplained (`liquers-web/src/builtins.rs` is the
//! precedent).

// The macro expansions reference these paths directly.
#[allow(unused_imports)]
use liquers_core::context::Context;

use liquers_core::commands::CommandRegistry;
use liquers_core::error::Error;

use crate::environment::PyEnvironment;

/// Registers the built-in Rust command set into a builder's registry.
pub fn register(cr: &mut CommandRegistry<PyEnvironment>) -> Result<(), Error> {
    type CommandEnvironment = PyEnvironment;
    liquers_lib::register_core_commands!(cr)?;
    liquers_lib::register_image_commands!(cr)?;
    liquers_lib::register_polars_commands!(cr)?;
    liquers_lib::register_records_commands!(cr)?;
    liquers_lib::register_search_commands!(cr)?;
    liquers_lib::register_template_commands!(cr)?;
    Ok(())
}
