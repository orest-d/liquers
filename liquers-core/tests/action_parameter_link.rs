//! End-to-end coverage for action-parameter links written in query text.
//!
//! `ActionParameter::Link` and the planner path that resolves it already worked for
//! programmatically constructed links. What was impossible before
//! `QUERY-ACTION-PARAMETER-LINK-PARSER` was reaching any of it from a query string.
//! These tests exercise that newly reachable route all the way to a value.

use liquers_core::{
    context::{Context, Environment, SimpleEnvironment},
    error::Error,
    interpreter::evaluate,
    state::State,
    value::Value,
};
use liquers_macro::*;

/// An environment with a command whose argument can be supplied by a link.
fn environment() -> Result<SimpleEnvironment<Value>, Box<dyn std::error::Error>> {
    type CommandEnvironment = SimpleEnvironment<Value>;
    let mut env = SimpleEnvironment::<Value>::new();

    fn world(_state: &State<Value>) -> Result<Value, Error> {
        Ok(Value::from("world"))
    }
    fn greeting(_state: &State<Value>) -> Result<Value, Error> {
        Ok(Value::from("Hello"))
    }
    fn shout(_state: &State<Value>) -> Result<Value, Error> {
        Ok(Value::from("HELLO"))
    }
    async fn greet(state: State<Value>, greet: String) -> Result<Value, Error> {
        let what = state.try_into_string()?;
        Ok(Value::from(format!("{greet}, {what}!")))
    }

    let cr = &mut env.command_registry;
    register_command!(cr, fn world(state) -> result)?;
    register_command!(cr, fn greeting(state) -> result)?;
    register_command!(cr, fn shout(state) -> result)?;
    register_command!(cr, async fn greet(state, greet: String = "Hi") -> result)?;
    Ok(env)
}

/// D5 — a command argument supplied by a link written in the query text.
#[tokio::test]
async fn evaluate_query_with_textual_link() -> Result<(), Box<dyn std::error::Error>> {
    let envref = environment()?.to_ref();

    // The `greet` argument is not a literal: it is the result of the query `greeting`.
    let state = evaluate(envref.clone(), "world/greet-~X~greeting~E", None).await?;
    assert_eq!(state.try_into_string()?, "Hello, world!");

    // The same query with a literal argument, for contrast.
    let state = evaluate(envref, "world/greet-Hello", None).await?;
    assert_eq!(state.try_into_string()?, "Hello, world!");
    Ok(())
}

/// D6 — a link whose embedded query itself contains a link.
#[tokio::test]
async fn evaluate_nested_textual_link() -> Result<(), Box<dyn std::error::Error>> {
    let envref = environment()?.to_ref();

    // greet-~X~ world/greet-~X~greeting~E ~E  =>  argument is "Hello, world!"
    let state = evaluate(
        envref,
        "world/greet-~X~world/greet-~X~greeting~E~E",
        None,
    )
    .await?;
    assert_eq!(state.try_into_string()?, "Hello, world!, world!");
    Ok(())
}

/// A shorthand body is refused end to end, not merely at parse level.
#[tokio::test]
async fn evaluate_rejects_shorthand_inside_link() -> Result<(), Box<dyn std::error::Error>> {
    let envref = environment()?.to_ref();

    let err = evaluate(envref.clone(), "world/greet-~X~data/report/-/to_text~E", None)
        .await
        .expect_err("shorthand must be rejected inside a link");
    assert!(
        err.message.contains("-R/"),
        "message must name the explicit form, got: {}",
        err.message
    );

    // The explicit form parses; it fails later, on the missing resource, not on syntax.
    let err = evaluate(envref, "world/greet-~X~-R/data/report~E", None)
        .await
        .expect_err("no such resource");
    assert!(
        !err.message.contains("-R/data/report/-/to_text"),
        "should not be a shorthand complaint: {}",
        err.message
    );
    Ok(())
}
