//! `context` may appear at any position in a `register_command!` signature.
//!
//! Design: `specs/design/context-param-order/`. Each command below declares `context` somewhere
//! other than last, and each Rust function takes its parameters in that same order: if the
//! generated wrapper passed them in a different order, the file would not compile. The tests then
//! evaluate a query, proving the arguments still land in the right slots.

use liquers_core::{
    command_metadata::CommandKey,
    commands::InjectedFromContext,
    context::{Context, Environment, SimpleEnvironment},
    error::Error,
    interpreter::evaluate,
    state::State,
    value::Value,
};
use liquers_macro::register_command;

type CommandEnvironment = SimpleEnvironment<Value>;

/// An injected argument with a fixed value, so its slot can be told apart from the others.
struct Marker(&'static str);

impl InjectedFromContext<CommandEnvironment> for Marker {
    fn from_context(_name: &str, _context: Context<CommandEnvironment>) -> Result<Self, Error> {
        Ok(Marker("marker"))
    }
}

fn data(_state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from("x"))
}

async fn load(
    context: Context<CommandEnvironment>,
    state: State<Value>,
    limit: i64,
) -> Result<Value, Error> {
    context.info("loading")?;
    Ok(Value::from(format!("{}:{limit}", state.try_into_string()?)))
}

fn f(context: Context<CommandEnvironment>, state: &State<Value>, n: i64) -> Result<Value, Error> {
    context.info("f")?;
    Ok(Value::from(format!("{}:{n}", state.try_into_string()?)))
}

fn g(
    state: &State<Value>,
    a: i64,
    context: Context<CommandEnvironment>,
    b: Marker,
    c: String,
) -> Result<Value, Error> {
    context.info("g")?;
    Ok(Value::from(format!(
        "{}:{a}:{}:{c}",
        state.try_into_string()?,
        b.0
    )))
}

fn h(context: Context<CommandEnvironment>, n: i64) -> Result<Value, Error> {
    context.info("h")?;
    Ok(Value::from(format!("{n}")))
}

fn echo(value: String) -> Result<Value, Error> {
    Ok(Value::from(value))
}

fn m(
    state: &State<Value>,
    context: Context<CommandEnvironment>,
    xs: Vec<String>,
) -> Result<Value, Error> {
    context.info("m")?;
    Ok(Value::from(format!(
        "{}:{}",
        state.try_into_string()?,
        xs.join(",")
    )))
}

fn environment() -> Result<CommandEnvironment, Error> {
    let mut env = CommandEnvironment::new();
    let cr = &mut env.command_registry;
    register_command!(cr, fn data(state) -> result)?;
    register_command!(cr, async fn load(context, state, limit: i64) -> result)?;
    register_command!(cr, fn f(context, state, n: i64) -> result)?;
    register_command!(cr, fn g(state, a: i64, context, b: Marker injected, c: String) -> result)?;
    register_command!(cr, fn h(context, n: i64) -> result)?;
    register_command!(cr, fn echo(value: String) -> result)?;
    register_command!(cr, fn m(state, context, xs: Vec<String> multiple) -> result)?;
    Ok(env)
}

fn argument_names(env: &CommandEnvironment, command: &str) -> Vec<String> {
    let metadata = env
        .command_registry
        .command_metadata_registry
        .get(CommandKey::new_name(command))
        .expect("the command is registered");
    metadata.arguments.iter().map(|a| a.name.clone()).collect()
}

async fn run(env: CommandEnvironment, query: &str) -> Result<String, Error> {
    evaluate(env.to_ref(), query, None).await?.try_into_string()
}

/// AC-1, the design's problem example: an async command with `context` before the state.
#[tokio::test]
async fn context_before_state_async_problem_example() -> Result<(), Error> {
    let env = environment()?;
    assert_eq!(argument_names(&env, "load"), ["limit"]);
    assert_eq!(run(env, "data/load-5").await?, "x:5");
    Ok(())
}

/// AC-1: the same with a sync command, which borrows its state.
#[tokio::test]
async fn context_before_state_sync() -> Result<(), Error> {
    let env = environment()?;
    assert_eq!(argument_names(&env, "f"), ["n"]);
    assert_eq!(run(env, "data/f-5").await?, "x:5");
    Ok(())
}

/// AC-2: `context` between arguments takes no slot; the injected argument keeps its own.
#[tokio::test]
async fn context_between_arguments_with_injected() -> Result<(), Error> {
    let env = environment()?;
    assert_eq!(argument_names(&env, "g"), ["a", "b", "c"]);
    assert_eq!(run(env, "data/g-1-x").await?, "x:1:marker:x");
    Ok(())
}

/// AC-3: `context` first in a command without a state.
#[tokio::test]
async fn context_first_without_state() -> Result<(), Error> {
    let env = environment()?;
    assert_eq!(argument_names(&env, "h"), ["n"]);
    assert_eq!(run(env, "h-5").await?, "5");
    Ok(())
}

/// AC-5: `value: String` with no state is an argument named `value`, not the state keyword.
#[tokio::test]
async fn stateless_argument_named_value() -> Result<(), Error> {
    let env = environment()?;
    assert_eq!(argument_names(&env, "echo"), ["value"]);
    assert_eq!(run(env, "echo-abc").await?, "abc");
    Ok(())
}

/// AC-9: the recommended position before a `multiple` argument.
#[tokio::test]
async fn context_before_multiple_argument() -> Result<(), Error> {
    let env = environment()?;
    assert_eq!(argument_names(&env, "m"), ["xs"]);
    assert_eq!(run(env, "data/m-a-b").await?, "x:a,b");
    Ok(())
}
