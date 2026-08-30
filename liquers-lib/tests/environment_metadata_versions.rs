use liquers_core::command_metadata::CommandKey;
use liquers_core::context::{Context, Environment};
use liquers_core::error::Error;
use liquers_core::state::State;
use liquers_macro::register_command;

use liquers_lib::environment::{CommandRegistryAccess, DefaultEnvironment};
use liquers_lib::value::Value;

type CommandEnvironment = DefaultEnvironment<Value>;

fn repeat(state: &State<Value>, count: i64) -> Result<Value, Error> {
    let text = state.try_into_string()?;
    Ok(Value::from(text.repeat(count.max(0) as usize)))
}

#[tokio::test]
async fn default_environment_to_ref_refreshes_macro_metadata_versions() {
    let mut env = DefaultEnvironment::<Value>::new();
    let cr = env.get_mut_command_registry();
    register_command!(cr, fn repeat(state, count: i64) -> result)
        .expect("the macro registers the command");

    let key = CommandKey::new("", "root", "repeat");
    let stale = env
        .get_command_metadata_registry()
        .get(key.clone())
        .expect("the macro-registered command is in the registry")
        .metadata_version;

    let envref = env.to_ref();
    let refreshed = envref
        .get_command_metadata_registry()
        .get(key)
        .expect("the macro-registered command is in the shared registry")
        .metadata_version;

    assert_ne!(refreshed, stale);
}
