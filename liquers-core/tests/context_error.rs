//! `Context::error` (`CONTEXT-ERROR-OUTCOME-DEPENDS-ON-SCHEDULING`).
//!
//! Logging an error does not interrupt the command. It sets `is_error` on the asset's metadata
//! while the status stays in flight, and once the step returns the evaluation fails with an
//! `ExecutionError` carrying the first logged error's message: the returned value is dropped and
//! the rest of the plan does not run. The outcome must not depend on when the asset's service
//! loop applies the log message, so every scenario runs on the queued manager under a
//! current-thread and a multi-thread runtime, and on the inline manager.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use liquers_core::{
    command_metadata::CommandKey,
    commands::CommandRegistry,
    context::{EnvRef, Environment, ImmediateEnvironment, SimpleEnvironment},
    error::{Error, ErrorType},
    metadata::Status,
    parse::parse_query,
    state::State,
    value::Value,
};

/// Registers the scenario commands; `after` counts its calls in `after_calls`.
fn register<E: Environment<Value = Value>>(
    cr: &mut CommandRegistry<E>,
    after_calls: Arc<AtomicUsize>,
) -> Result<(), Error> {
    // Synchronous: nothing yields between logging and returning.
    cr.register_command(CommandKey::new_name("logerr"), |_state: &State<Value>, _args, context| {
        context.error("logged error")?;
        Ok(Value::from("value"))
    })?;
    cr.register_command(
        CommandKey::new_name("logerr_info"),
        |_state: &State<Value>, _args, context| {
            context.error("logged error")?;
            context.info("later info")?;
            Ok(Value::from("value"))
        },
    )?;
    // Same as `logerr`, but never a cut boundary, so `logerr_inline/after` is one plan.
    let inline = cr.register_command(
        CommandKey::new_name("logerr_inline"),
        |_state: &State<Value>, _args, context| {
            context.error("logged error")?;
            Ok(Value::from("value"))
        },
    )?;
    inline.cached = Some(false);
    // Logs, then fails on its own: the returned error is the failure.
    cr.register_command(
        CommandKey::new_name("logerr_fail"),
        |_state: &State<Value>, _args, context| -> Result<Value, Error> {
            context.error("logged error")?;
            Err(Error::general_error("returned error".to_string()))
        },
    )?;
    cr.register_command(CommandKey::new_name("plain"), |_state: &State<Value>, _args, _context| {
        Ok(Value::from("plain"))
    })?;
    cr.register_command(CommandKey::new_name("after"), move |state: &State<Value>, _args, _context| {
        after_calls.fetch_add(1, Ordering::SeqCst);
        Ok(Value::from(format!("{}!", state.try_into_string().unwrap_or_default())))
    })?;
    // Logs, then waits for a dependency: the wait must not erase the error.
    cr.register_async_command(CommandKey::new_name("logerr_dep"), |_state, _args, context| {
        Box::pin(async move {
            context.error("logged error")?;
            let dependency = context.evaluate(&parse_query("plain")?).await?;
            context.wait_for_dependency(&dependency).await?;
            Ok(Value::from("value"))
        })
    })?;
    // Logs, then keeps running long enough to be observed mid-run.
    cr.register_async_command(CommandKey::new_name("logerr_slow"), |_state, _args, context| {
        Box::pin(async move {
            context.error("logged error")?;
            tokio::time::sleep(Duration::from_millis(500)).await;
            Ok(Value::from("value"))
        })
    })?;
    Ok(())
}

fn simple_env(after_calls: Arc<AtomicUsize>) -> Result<EnvRef<SimpleEnvironment<Value>>, Error> {
    let mut env = SimpleEnvironment::<Value>::new();
    register(&mut env.command_registry, after_calls)?;
    Ok(env.to_ref())
}

fn immediate_env(
    after_calls: Arc<AtomicUsize>,
) -> Result<EnvRef<ImmediateEnvironment<Value>>, Error> {
    let mut env = ImmediateEnvironment::<Value>::new();
    register(&mut env.command_registry, after_calls)?;
    Ok(env.to_ref())
}

/// Evaluates `query` and returns the error it failed with, checking the asset ended in `Error`.
async fn failure<E: Environment<Value = Value>>(envref: &EnvRef<E>, query: &str) -> Error {
    let asset = match envref.evaluate(parse_query(query).expect("query parses")).await {
        Ok(asset) => asset,
        Err(e) => return e,
    };
    let outcome = asset.get().await.and_then(|state| state.value_state());
    assert_eq!(asset.status().await, Status::Error, "{query}: the asset must end in Error");
    match outcome {
        Ok(state) => panic!("{query}: expected a failure, got a value: {:?}", state.try_into_string()),
        Err(e) => e,
    }
}

fn assert_logged_error(error: &Error, query: &str) {
    assert_eq!(error.error_type, ErrorType::ExecutionError, "{query}: {error}");
    assert_eq!(error.message, "logged error", "{query}: {error}");
}

async fn scenario_outcomes<E: Environment<Value = Value>>(
    envref: EnvRef<E>,
    after_calls: Arc<AtomicUsize>,
) {
    for query in ["logerr", "logerr_info", "logerr_dep"] {
        let error = failure(&envref, query).await;
        assert_logged_error(&error, query);
    }
    for query in ["logerr/after", "logerr_inline/after"] {
        let error = failure(&envref, query).await;
        assert_logged_error(&error, query);
    }
    assert_eq!(after_calls.load(Ordering::SeqCst), 0, "a step after the error must not run");
    // A command evaluating a failed dependency gets the dependency's own error.
    let error = failure(&envref, "logerr/after/after").await;
    assert_logged_error(&error, "logerr/after/after");
    let error = failure(&envref, "logerr_fail").await;
    assert_eq!(error.error_type, ErrorType::General, "logerr_fail: {error}");
    assert_eq!(error.message, "returned error", "logerr_fail: {error}");
    // A successful neighbour is unaffected.
    let asset = envref.evaluate(parse_query("plain/after").expect("query parses")).await;
    let value = match asset {
        Ok(asset) => asset.get().await.and_then(|s| s.try_into_string()),
        Err(e) => Err(e),
    };
    assert_eq!(value.ok().as_deref(), Some("plain!"));
}

#[tokio::test(flavor = "current_thread")]
async fn outcomes_on_the_queued_manager_current_thread() -> Result<(), Error> {
    let calls = Arc::new(AtomicUsize::new(0));
    scenario_outcomes(simple_env(calls.clone())?, calls).await;
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn outcomes_on_the_queued_manager_multi_thread() -> Result<(), Error> {
    let calls = Arc::new(AtomicUsize::new(0));
    scenario_outcomes(simple_env(calls.clone())?, calls).await;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn outcomes_on_the_inline_manager() -> Result<(), Error> {
    let calls = Arc::new(AtomicUsize::new(0));
    scenario_outcomes(immediate_env(calls.clone())?, calls).await;
    Ok(())
}

/// While the command still runs after logging, the asset is in flight and flagged, not failed.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mid_run_the_asset_is_flagged_but_still_processing() -> Result<(), Error> {
    let envref = simple_env(Arc::new(AtomicUsize::new(0)))?;
    let asset = envref.evaluate(parse_query("logerr_slow")?).await?;
    // Wait until the logged error is visible; the command then still sleeps.
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    let metadata = loop {
        let metadata = asset.get_metadata().await?;
        if metadata.is_error()? {
            break metadata;
        }
        assert!(tokio::time::Instant::now() < deadline, "the logged error never became visible");
        tokio::time::sleep(Duration::from_millis(5)).await;
    };
    assert_eq!(asset.status().await, Status::Processing);
    assert_eq!(metadata.status(), Status::Processing, "the metadata status must stay in flight");
    assert_eq!(metadata.is_error()?, true, "the logged error must be flagged");
    let error = match asset.get().await.and_then(|state| state.value_state()) {
        Ok(_) => panic!("expected the asset to fail after the command returned"),
        Err(e) => e,
    };
    assert_logged_error(&error, "logerr_slow");
    assert_eq!(asset.status().await, Status::Error);
    Ok(())
}
