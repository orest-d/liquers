//! The built-in Rust command set, registered into every web environment.
//!
//! A page gets `to_text`, `to_metadata`, `commands_doc` and the `dep` introspection commands for
//! free. They matter beyond convenience: they are what makes structural conversion the right
//! default for a JavaScript command's result. `myJsCommand/to_text` only works because the Rust
//! command can read what JavaScript produced — an opaque value would stop at the boundary.
//!
//! # Why this is its own module
//!
//! `register_core_commands!` expands into the calling crate and names `futures`, `Context`,
//! `serde_json` and `liquers_macro` as if they were the caller's own imports. Isolating the
//! expansion here keeps those imports — and the `#[allow]`s the generated code needs — out of
//! `environment.rs`, where they would look unexplained.

// The macro expansion references these paths directly.
#[allow(unused_imports)]
use liquers_core::context::Context;

use liquers_core::error::Error;
use liquers_lib::environment::CommandRegistryAccess;

use crate::environment::WebEnvironment;

/// Registers the built-in Rust command set into an environment that is still mutable.
pub fn register_builtin_commands(env: &mut WebEnvironment) -> Result<(), Error> {
    let cr = env.get_mut_command_registry();
    type CommandEnvironment = WebEnvironment;
    liquers_lib::register_core_commands!(cr)
}
