//! The built-in Rust command set, registered into every Python environment.
//!
//! A fresh `lq.Environment()` evaluates `liquers-lib` queries with nothing registered (AC-2): the
//! core commands, `ns-pl`, images, records, search and templates, and the `lui` commands that
//! build and navigate a UI model (`lq.UI`, `ui.rs`); they need the UI payload, which only a
//! UI's actions pass, and fail with their own error elsewhere.
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
    liquers_lib::register_lui_commands!(cr)?;
    #[cfg(feature = "debug-handles")]
    register_test_commands(cr)?;
    Ok(())
}

/// Test-only commands, behind `debug-handles`: `test_panic` panics inside a Rust command, so
/// `ERROR05` and `RUNTIME06` can check that a panic surfaces as an error rather than aborting the
/// process or leaving the evaluation waiting forever.
#[cfg(feature = "debug-handles")]
fn register_test_commands(cr: &mut CommandRegistry<PyEnvironment>) -> Result<(), Error> {
    cr.register_command("test_panic", |_state, _arguments, _context| {
        panic!("test_panic was evaluated")
    })?;
    Ok(())
}
