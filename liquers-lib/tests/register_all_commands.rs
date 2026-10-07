//! `register_all_commands!` compiles and registers the enabled domains in every feature
//! configuration (`REGISTER-ALL-COMMANDS-MACRO-REQUIRES-EVERY-FEATURE`).
//!
//! Deliberately ungated: `scripts/check-build-matrix.sh` builds this target in each configuration,
//! which is the regression check for the macro's no-op stand-ins.
//!
//! `DefaultEnvironment::new()` builds a `DefaultAssetManager`, which calls `tokio::spawn`, so the
//! test needs a runtime.

use liquers_core::command_metadata::CommandMetadataRegistry;
use liquers_core::context::Environment;
use liquers_core::error::Error;
use liquers_lib::environment::{CommandRegistryAccess, DefaultEnvironment};
use liquers_lib::ui::payload::SimpleUIPayload;
use liquers_lib::value::Value;

// The `register_*_commands!` macros expand to code naming these unqualified.
#[allow(unused_imports)]
use liquers_core::{
    command_metadata::{ArgumentType, CommandDefinition, CommandParameterValue},
    context::{Context, Environment as _},
    state::State,
    value::ValueInterface,
};
#[allow(unused_imports)]
use liquers_lib::value::simple::SimpleValue;

type CommandEnvironment = DefaultEnvironment<Value, SimpleUIPayload>;

fn registered() -> Result<CommandMetadataRegistry, Error> {
    let mut env = CommandEnvironment::new();
    {
        let cr = env.get_mut_command_registry();
        liquers_lib::register_all_commands!(cr)?;
    }
    Ok(env.get_command_metadata_registry().clone())
}

fn has(registry: &CommandMetadataRegistry, namespace: &str, name: &str) -> bool {
    registry.find_command("", namespace, name).is_some()
}

#[tokio::test]
async fn register_all_commands_registers_the_enabled_domains() -> Result<(), Error> {
    let registry = registered()?;

    // Always compiled in.
    assert!(has(&registry, "root", "to_text"), "core commands");
    assert!(has(&registry, "lui", "add"), "lui commands");

    assert_eq!(
        has(&registry, "root", "text_editor"),
        cfg!(feature = "egui"),
        "egui commands"
    );
    assert_eq!(
        has(&registry, "img", "from_bytes"),
        cfg!(feature = "image-support"),
        "image commands"
    );
    assert_eq!(
        has(&registry, "pl", "from_csv"),
        cfg!(feature = "polars"),
        "polars commands"
    );
    assert_eq!(
        has(&registry, "rec", "rec_id"),
        cfg!(feature = "records"),
        "records commands"
    );
    Ok(())
}
