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
    assets::AssetManager,
    command_metadata::CommandKey,
    context::{Context, Environment, SimpleEnvironment},
    error::Error,
    interpreter::evaluate,
    metadata::Metadata,
    parse::{parse_key, parse_query},
    query::Key,
    recipes::DefaultRecipeProvider,
    state::State,
    store::{AsyncMemoryStore, AsyncStore},
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
        let _a = context
            .get_dependency_state(&parse_query("counted")?)
            .await?;
        let _b = context
            .get_dependency_state(&parse_query("counted")?)
            .await?;
        Ok(Value::from("done"))
    }
    let cr = &mut env.command_registry;
    register_command!(cr, async fn use_twice(state, context) -> result)
        .expect("register use_twice");

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
        let s = context
            .get_dependency_state(&parse_query("level_c")?)
            .await?;
        Ok(Value::from(format!("b({})", s.try_into_string()?)))
    }
    async fn level_a(
        _state: State<Value>,
        context: Context<CommandEnvironment>,
    ) -> Result<Value, Error> {
        let s = context
            .get_dependency_state(&parse_query("level_b")?)
            .await?;
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

/// A **keyed** asset whose command evaluates its own key must be rejected as a cycle.
///
/// This is the self-dependency that matters most, and it is deliberately kept distinct from
/// the delegation hand-off. `specs/design/keyed-delegation-hand-off/` exempts the same-node
/// case from `AssetRef::record_dependency_on_asset`, so it is worth pinning that this
/// exemption does **not** reach genuine self-dependency: a runtime dependency discovered by a
/// command travels a different path entirely — `Context::evaluate` →
/// `schedule_dependency_asset` → `DependencyManager::register_scheduled_dependency` →
/// `would_create_cycle` — which still reports `K -> K` and fails fast.
///
/// The hang guard is the second half of the assertion: before schedule-time cycle detection
/// existed this arrangement deadlocked rather than erroring.
#[tokio::test]
async fn test_keyed_asset_evaluating_its_own_key_is_a_cycle(
) -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = SimpleEnvironment<Value>;
    let mut env = SimpleEnvironment::<Value>::new();

    // `self_ref.txt` is produced by `self_cycle`, which asks for `self_ref.txt`.
    let store = AsyncMemoryStore::new(&Key::new());
    store
        .set(
            &parse_key("recipes.yaml")?,
            b"recipes:\n  - query: self_cycle/self_ref.txt\n",
            &Metadata::new(),
        )
        .await?;

    async fn self_cycle(
        _state: State<Value>,
        context: Context<CommandEnvironment>,
    ) -> Result<Value, Error> {
        let s = context
            .get_dependency_state(&parse_query("-R/self_ref.txt")?)
            .await?;
        Ok(Value::from(format!("self:{}", s.try_into_string()?)))
    }
    let cr = &mut env.command_registry;
    register_command!(cr, async fn self_cycle(state, context) -> result)
        .expect("register self_cycle");

    env.with_async_store(Box::new(store));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    let envref = env.to_ref();

    let asset = envref
        .get_asset_manager()
        .get(&parse_key("self_ref.txt")?)
        .await?;
    let outcome = tokio::time::timeout(GUARD, asset.get()).await;
    let result = outcome.expect("a keyed self-dependency must not hang");

    let err: Error = match result {
        Ok(state) => match state.value_state() {
            Ok(_) => panic!("a keyed asset depending on itself must not produce a value"),
            Err(e) => e,
        },
        Err(e) => e,
    };
    assert!(
        err.to_string().to_lowercase().contains("cycle"),
        "expected a dependency cycle for a keyed asset evaluating its own key, got: {}",
        err
    );
    Ok(())
}
