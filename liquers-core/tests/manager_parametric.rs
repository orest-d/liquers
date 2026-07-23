//! Manager-parametric suite (async-wasm-refactor M-D).
//!
//! The same `AssetManager` trait contract is exercised over BOTH implementations —
//! `DefaultAssetManager` (via `SimpleEnvironment`, queued) and `ImmediateAssetManager` (via
//! `ImmediateEnvironment`, inline) — proving b1's manager is swappable behind the trait and
//! that `ImmediateAssetManager` evaluates correctly at runtime. Plus immediate-only checks:
//! concurrency dedup and the no-tokio-runtime proof (browser-readiness on native).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use liquers_core::{
    assets::AssetManager,
    command_metadata::CommandKey,
    context::{Environment, EnvRef, ImmediateEnvironment, SimpleEnvironment},
    error::Error,
    metadata::Status,
    query::{Query, TryToQuery},
    value::Value,
};

fn q(s: &str) -> Query {
    s.try_to_query().expect("query parse")
}

// --- generic scenario bodies (written once, run against both managers) ---

async fn scenario_basic_eval<E>(envref: EnvRef<E>) -> Result<(), Error>
where
    E: Environment<Value = Value>,
{
    let asset = envref.get_asset_manager().get_asset(&q("greet")).await?;
    let state = asset.get().await?;
    assert_eq!(state.status(), Status::Ready);
    assert_eq!(state.try_into_string()?, "hello");
    Ok(())
}

/// `eval_mode()` reports the manager's constant, and a second `get_asset` of the same query
/// returns a finished asset (cache path).
async fn scenario_cache_and_mode<E>(envref: EnvRef<E>) -> Result<(), Error>
where
    E: Environment<Value = Value>,
{
    let m = envref.get_asset_manager();
    let a1 = m.get_asset(&q("greet")).await?;
    assert!(a1.get().await?.status().is_finished());
    let a2 = m.get_asset(&q("greet")).await?;
    assert_eq!(a2.get().await?.try_into_string()?, "hello");
    Ok(())
}

fn register_greet<E>(cr: &mut liquers_core::commands::CommandRegistry<E>)
where
    E: Environment<Value = Value>,
{
    cr.register_command(
        CommandKey::new_name("greet"),
        |_state, _args, _ctx| -> Result<Value, Error> { Ok(Value::from("hello")) },
    )
    .expect("register greet");
}

// --- Default manager (queued) ---

#[tokio::test]
async fn basic_eval_default() -> Result<(), Error> {
    let mut env = SimpleEnvironment::<Value>::new();
    register_greet(&mut env.command_registry);
    scenario_basic_eval(env.to_ref()).await
}

#[tokio::test]
async fn cache_and_mode_default() -> Result<(), Error> {
    let mut env = SimpleEnvironment::<Value>::new();
    register_greet(&mut env.command_registry);
    let envref = env.to_ref();
    assert_eq!(
        envref.get_asset_manager().eval_mode(),
        liquers_core::assets::EvalMode::Queued
    );
    scenario_cache_and_mode(envref).await
}

// --- Immediate manager (inline) ---

#[tokio::test]
async fn basic_eval_immediate() -> Result<(), Error> {
    let mut env = ImmediateEnvironment::<Value>::new();
    register_greet(&mut env.command_registry);
    scenario_basic_eval(env.to_ref()).await
}

#[tokio::test]
async fn cache_and_mode_immediate() -> Result<(), Error> {
    let mut env = ImmediateEnvironment::<Value>::new();
    register_greet(&mut env.command_registry);
    let envref = env.to_ref();
    assert_eq!(
        envref.get_asset_manager().eval_mode(),
        liquers_core::assets::EvalMode::Inline
    );
    scenario_cache_and_mode(envref).await
}

// --- immediate-only ---

/// Two concurrent `get_asset` for the same query share one evaluation (the command body runs once).
#[tokio::test]
async fn immediate_concurrent_same_query_runs_once() -> Result<(), Error> {
    static COUNT: AtomicUsize = AtomicUsize::new(0);
    let mut env = ImmediateEnvironment::<Value>::new();
    env.command_registry
        .register_command(
            CommandKey::new_name("counted"),
            |_state, _args, _ctx| -> Result<Value, Error> {
                COUNT.fetch_add(1, Ordering::SeqCst);
                Ok(Value::from("x"))
            },
        )
        .expect("register");
    let envref = env.to_ref();
    let m = envref.get_asset_manager();
    let query = q("counted");
    let (a, b) = futures::join!(m.get_asset(&query), m.get_asset(&query));
    a?.get().await?;
    b?.get().await?;
    assert_eq!(COUNT.load(Ordering::SeqCst), 1, "command body must run once");
    Ok(())
}

/// **No-tokio-runtime proof.** The immediate path runs under `futures::executor::block_on`
/// with NO tokio runtime present. A reintroduced `tokio::spawn` on the inline path would panic
/// here ("no reactor running") — green means browser-ready. (Non-keyed query ⇒ no persistence.)
#[test]
fn immediate_runs_without_tokio_runtime() -> Result<(), Error> {
    let mut env = ImmediateEnvironment::<Value>::new();
    register_greet(&mut env.command_registry);
    let envref: EnvRef<ImmediateEnvironment<Value>> = env.to_ref();

    let text: String = futures::executor::block_on(async move {
        let asset = envref.get_asset_manager().get_asset(&q("greet")).await?;
        let state = asset.get().await?;
        state.try_into_string()
    })?;
    assert_eq!(text, "hello");
    Ok(())
}
