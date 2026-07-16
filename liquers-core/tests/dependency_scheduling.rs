//! Integration tests for non-blocking dependency scheduling (Phase 3 / WP-1).
//!
//! These exercise the end-to-end behaviour of the scheduling mechanism through the
//! public `evaluate` surface: execute-once for a shared dependency, recursive
//! dependency chains that complete, and schedule-time cycle rejection that fails fast
//! instead of hanging. Every wait is wrapped in a `tokio::time::timeout` hang guard.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use liquers_core::{
    command_metadata::CommandKey,
    context::{Context, Environment, SimpleEnvironment},
    error::Error,
    interpreter::evaluate,
    parse::parse_query,
    state::State,
    value::Value,
};
use liquers_macro::register_command;

const GUARD: Duration = Duration::from_secs(10);

/// A command that evaluates the same sub-query twice must run that dependency exactly
/// once (shared, cached, claim-arbitrated) — the execute-once guarantee.
#[tokio::test]
async fn test_shared_dependency_runs_once() -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = SimpleEnvironment<Value>;
    let mut env = SimpleEnvironment::<Value>::new();

    let counter = Arc::new(AtomicUsize::new(0));
    let c = counter.clone();
    env.command_registry
        .register_command(CommandKey::new_name("counted"), move |_, _, _| {
            c.fetch_add(1, Ordering::SeqCst);
            Ok(Value::from("v"))
        })
        .expect("register counted");

    async fn use_twice(
        _state: State<Value>,
        context: Context<CommandEnvironment>,
    ) -> Result<Value, Error> {
        let _a = context.get_dependency_state(&parse_query("counted")?).await?;
        let _b = context.get_dependency_state(&parse_query("counted")?).await?;
        Ok(Value::from("done"))
    }
    let cr = &mut env.command_registry;
    register_command!(cr, async fn use_twice(state, context) -> result).expect("register use_twice");

    let envref = env.to_ref();
    let state = tokio::time::timeout(GUARD, evaluate(envref, "use_twice", None)).await??;
    assert_eq!(state.try_into_string()?, "done");
    assert_eq!(
        counter.load(Ordering::SeqCst),
        1,
        "shared dependency must be evaluated exactly once"
    );
    Ok(())
}

/// A chain of commands each evaluating the next as a dependency must complete (recursive
/// scheduling / inline drain), never deadlocking.
#[tokio::test]
async fn test_nested_dependency_chain_completes() -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = SimpleEnvironment<Value>;
    let mut env = SimpleEnvironment::<Value>::new();

    fn level_c(_state: &State<Value>) -> Result<Value, Error> {
        Ok(Value::from("c"))
    }
    async fn level_b(
        _state: State<Value>,
        context: Context<CommandEnvironment>,
    ) -> Result<Value, Error> {
        let s = context.get_dependency_state(&parse_query("level_c")?).await?;
        Ok(Value::from(format!("b({})", s.try_into_string()?)))
    }
    async fn level_a(
        _state: State<Value>,
        context: Context<CommandEnvironment>,
    ) -> Result<Value, Error> {
        let s = context.get_dependency_state(&parse_query("level_b")?).await?;
        Ok(Value::from(format!("a({})", s.try_into_string()?)))
    }
    let cr = &mut env.command_registry;
    register_command!(cr, fn level_c(state) -> result).expect("register level_c");
    register_command!(cr, async fn level_b(state, context) -> result).expect("register level_b");
    register_command!(cr, async fn level_a(state, context) -> result).expect("register level_a");

    let envref = env.to_ref();
    let state = tokio::time::timeout(GUARD, evaluate(envref, "level_a", None)).await??;
    assert_eq!(state.try_into_string()?, "a(b(c))");
    Ok(())
}

/// Two commands that dynamically evaluate each other form a cycle. It must fail fast
/// (schedule-time detection) rather than hang.
#[tokio::test]
async fn test_dynamic_expression_cycle_does_not_hang() -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = SimpleEnvironment<Value>;
    let mut env = SimpleEnvironment::<Value>::new();

    async fn cyc_a(
        _state: State<Value>,
        context: Context<CommandEnvironment>,
    ) -> Result<Value, Error> {
        let s = context.get_dependency_state(&parse_query("cyc_b")?).await?;
        Ok(Value::from(format!("a:{}", s.try_into_string()?)))
    }
    async fn cyc_b(
        _state: State<Value>,
        context: Context<CommandEnvironment>,
    ) -> Result<Value, Error> {
        let s = context.get_dependency_state(&parse_query("cyc_a")?).await?;
        Ok(Value::from(format!("b:{}", s.try_into_string()?)))
    }
    let cr = &mut env.command_registry;
    register_command!(cr, async fn cyc_a(state, context) -> result).expect("register cyc_a");
    register_command!(cr, async fn cyc_b(state, context) -> result).expect("register cyc_b");

    let envref = env.to_ref();
    let outcome = tokio::time::timeout(GUARD, evaluate(envref, "cyc_a", None)).await;
    // Must not hang (timeout did not elapse) and must report an error rather than a value.
    let result = outcome.expect("cycle evaluation must not hang");
    assert!(
        result.is_err(),
        "a dynamic dependency cycle must fail, not produce a value"
    );
    Ok(())
}
