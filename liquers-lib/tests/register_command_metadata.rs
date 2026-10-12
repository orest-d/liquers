//! `register_command!` carries command-level hints and per-argument descriptions into the
//! registered metadata (design `command-metadata-descriptions-and-hints`).

use liquers_core::context::Context;
use liquers_core::error::Error;
use liquers_core::state::State;
use liquers_macro::register_command;

use liquers_lib::environment::{CommandRegistryAccess, DefaultEnvironment};
use liquers_lib::ui::payload::SimpleUIPayload;
use liquers_lib::value::Value;

type CommandEnvironment = DefaultEnvironment<Value, SimpleUIPayload>;

fn export(_state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from("exported"))
}

fn wrap(state: &State<Value>, width: i32) -> Result<Value, Error> {
    let text = state.try_into_string()?;
    Ok(Value::from(format!("{width}:{text}")))
}

/// AC-2: a command statement `hint icon: "download"` reaches `CommandMetadata::hints`.
#[test]
fn register_command_sets_command_hint() -> Result<(), Error> {
    let mut env = CommandEnvironment::new();
    let cr = env.get_mut_command_registry();
    let cm = register_command!(cr,
        fn export(state) -> result
        label: "Export"
        hint icon: "download"
        hint toolbar: "main"
    )?;
    assert_eq!(cm.hints["icon"], serde_json::json!("download"));
    assert_eq!(cm.hints["toolbar"], serde_json::json!("main"));
    Ok(())
}

/// AC-5: `description:` in an argument's option list reaches `ArgumentInfo::description`.
#[test]
fn register_command_sets_argument_description() -> Result<(), Error> {
    let mut env = CommandEnvironment::new();
    let cr = env.get_mut_command_registry();
    let cm = register_command!(cr,
        fn wrap(state, width: i32 = 80 (label: "Width", description: "Maximum line width in characters")) -> result
    )?;
    assert_eq!(cm.arguments[0].label, "Width");
    assert_eq!(
        cm.arguments[0].description,
        "Maximum line width in characters"
    );
    assert!(cm.hints.is_empty());
    Ok(())
}
