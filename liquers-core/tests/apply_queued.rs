//! `AssetManager::apply_queued`: an ad-hoc asset returned without waiting for its evaluation
//! (queued manager) or returned even when it failed (inline manager). Design:
//! `specs/design/axum-query-named-arguments/` (Phase 3, AQ01–AQ03).

use std::sync::Arc;

use liquers_core::{
    assets::AssetManager,
    command_metadata::{ArgumentInfo, CommandKey},
    commands::CommandRegistry,
    context::{Environment, ImmediateEnvironment, SimpleEnvironment},
    error::Error,
    metadata::Status,
    parse::parse_query,
    recipes::Recipe,
    state::State,
    value::Value,
};
use tokio::sync::Notify;

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// `gated(tag: string = "")` waits for `gate`, then returns `"gated:<tag>"`; `fail_with` fails.
fn register<E: Environment<Value = Value>>(
    cr: &mut CommandRegistry<E>,
    gate: Arc<Notify>,
) -> Result<(), Error> {
    cr.register_async_command(CommandKey::new_name("gated"), move |_, args, _| {
        let gate = gate.clone();
        Box::pin(async move {
            gate.notified().await;
            let tag: String = args.get(0, "tag")?;
            Ok(Value::from(format!("gated:{tag}")))
        })
    })?
    .with_argument(ArgumentInfo::string_argument("tag").with_default(""));
    cr.register_command(CommandKey::new_name("fail_with"), |_, _, _| {
        Err(Error::general_error("failed on purpose".to_string()))
    })?;
    Ok(())
}

fn recipe(query: &str, arguments: &[(&str, &str)]) -> Result<Recipe, Error> {
    let mut recipe = Recipe::new(query.to_string(), String::new(), String::new())?;
    for (name, value) in arguments {
        recipe
            .arguments
            .insert(name.to_string(), serde_json::Value::from(*value));
    }
    Ok(recipe)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn aq01_default_manager_returns_before_completion_and_maps_nothing() -> TestResult {
    let gate = Arc::new(Notify::new());
    let mut env = SimpleEnvironment::<Value>::new();
    register(&mut env.command_registry, gate.clone())?;
    let envref = env.to_ref();
    let manager = envref.get_asset_manager();

    let asset = manager
        .apply_queued(recipe("gated", &[("tag", "a")])?, State::new())
        .await?;
    let status = asset.status().await;
    assert!(
        matches!(status, Status::None | Status::Submitted | Status::Processing),
        "returned before the gated evaluation could finish, got {status:?}"
    );
    assert!(
        manager.lookup_query_asset(&parse_query("gated")?).is_none(),
        "an ad-hoc asset is in no map"
    );
    gate.notify_one();
    let state = tokio::time::timeout(std::time::Duration::from_secs(5), asset.get()).await??;
    assert_eq!(state.try_into_string()?, "gated:a");
    Ok(())
}

#[tokio::test]
async fn aq02_inline_manager_returns_the_failed_asset() -> TestResult {
    let mut env = ImmediateEnvironment::<Value>::new();
    register(&mut env.command_registry, Arc::new(Notify::new()))?;
    let envref = env.to_ref();
    let asset = envref
        .get_asset_manager()
        .apply_queued(recipe("fail_with", &[])?, State::new())
        .await?;
    assert_eq!(asset.status().await, Status::Error);
    let info = asset.get_asset_info().await?;
    assert!(info.is_error);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn aq03_queued_asset_uses_its_recipe_arguments() -> TestResult {
    let gate = Arc::new(Notify::new());
    let mut env = SimpleEnvironment::<Value>::new();
    register(&mut env.command_registry, gate.clone())?;
    let envref = env.to_ref();
    let asset = envref
        .get_asset_manager()
        .apply_queued(recipe("gated", &[("tag", "override")])?, State::new())
        .await?;
    gate.notify_one();
    let state = tokio::time::timeout(std::time::Duration::from_secs(5), asset.get()).await??;
    assert_eq!(state.try_into_string()?, "gated:override");
    Ok(())
}
