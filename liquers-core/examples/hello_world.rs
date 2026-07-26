use liquers_core::{
    context::{Context, Environment, SimpleEnvironment},
    error::Error,
    state::State,
    value::Value,
};
use liquers_macro::register_command;

// The registration macro uses this alias to bind commands to an environment.
type CommandEnvironment = SimpleEnvironment<Value>;

fn world(_state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from("world"))
}

fn greet(state: &State<Value>, greeting: String) -> Result<Value, Error> {
    let name = state.try_into_string()?;
    Ok(Value::from(format!("{greeting}, {name}!")))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut environment = SimpleEnvironment::<Value>::new();

    let registry = &mut environment.command_registry;
    register_command!(
        registry,
        fn world(state) -> result
        doc: "Produce the value 'world'"
    )?;
    register_command!(
        registry,
        fn greet(state, greeting: String = "Hello") -> result
        doc: "Add a greeting to the input value"
    )?;

    let environment = environment.to_ref();
    let asset = environment.evaluate("world/greet").await?;
    let state = asset.get().await?;

    println!("{}", state.try_into_string()?);
    Ok(())
}
