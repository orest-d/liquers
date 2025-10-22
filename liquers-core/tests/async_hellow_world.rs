use liquers_core::{
    command_metadata::CommandKey, context2::{Context, Environment, SimpleEnvironment}, error::Error, interpreter2::evaluate, state::State, value::Value
};
use liquers_macro::*;

#[tokio::test]
async fn test_async_hello_world() -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = SimpleEnvironment<Value>;
    let mut env = SimpleEnvironment::<Value>::new();

    // Register "hello" command
    fn world(_state: &State<Value>) -> Result<Value, Error> {
        Ok(Value::from("world"))
    }
    async fn greet(state: State<Value>, greet: String) -> Result<Value, Error> {
        let what = state.try_into_string()?;
        Ok(Value::from(format!("{greet}, {what}!")))
    }
    let cr = &mut env.command_registry;
    register_command_v2!(cr, fn world(state) -> result).expect("register_command failed");
    register_command_v2!(cr, async fn greet(state, greet: String = "Hello") -> result)
         .expect("register_command failed");

    let envref = env.to_ref();

    let state = evaluate(envref.clone(), "world/greet", None).await?;

    let value = state.try_into_string()?;
    assert_eq!(value, "Hello, world!");
    Ok(())
}