//! Integration test for the `commands_doc` command.

use liquers_core::context::{Context, Environment};
use liquers_core::error::Error;
use liquers_core::interpreter::evaluate;
use liquers_core::state::State;
use liquers_macro::register_command;

use liquers_lib::environment::{CommandRegistryAccess, DefaultEnvironment};
use liquers_lib::value::Value;

type CommandEnvironment = DefaultEnvironment<Value>;

fn greet(_state: &State<Value>, greeting: String) -> Result<Value, Error> {
    Ok(Value::from(format!("{greeting}!")))
}

fn register_test_commands(env: &mut DefaultEnvironment<Value>) -> Result<(), Error> {
    let cr = env.get_mut_command_registry();
    liquers_lib::register_core_commands!(cr)?;
    register_command!(cr,
        fn greet(state, greeting: String = "Hello") -> result
        label: "Greet"
        doc: "Greeting command"
    )?;
    Ok(())
}

#[tokio::test]
async fn test_commands_doc_output() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = DefaultEnvironment::<Value>::new();
    env.with_trivial_recipe_provider();
    register_test_commands(&mut env)?;

    let envref = env.to_ref();

    let state = evaluate(envref, "commands_doc", None).await?;
    let md = state.try_into_string()?;

    // Should contain the top-level heading
    assert!(md.contains("# Commands"), "missing top-level heading");

    // Should contain namespace grouping
    assert!(md.contains("## Namespace:"), "missing namespace heading");
    assert!(md.contains("`root`"), "missing root namespace");

    // Should contain the registered commands
    assert!(md.contains("### `commands_doc`"), "missing commands_doc");
    assert!(md.contains("### `to_text`"), "missing to_text");
    assert!(md.contains("### `to_metadata`"), "missing to_metadata");
    assert!(md.contains("### `greet`"), "missing greet");

    // greet should have an argument table with Label, Argument, Multiplicity, Type, Default
    assert!(md.contains("| greeting | `greeting` | single | String | \"Hello\" |"), "missing greet argument row");

    // commands_doc label and doc
    assert!(
        md.contains("*Commands documentation*"),
        "missing commands_doc label"
    );
    assert!(
        md.contains("> Generate markdown documentation of registered commands"),
        "missing commands_doc doc string"
    );

    Ok(())
}
