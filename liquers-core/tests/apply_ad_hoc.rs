//! `Context::apply_ad_hoc`: an ad-hoc evaluation whose dependencies are passed on to the asset
//! that ran it. Design: `specs/design/liquers-mustache/` (Phase 3, AC-7).

use liquers_core::{
    assets::AssetManager,
    command_metadata::CommandKey,
    commands::CommandRegistry,
    context::{Environment, SimpleEnvironment},
    error::Error,
    metadata::{DependencyKey, Metadata, MetadataRecord, Status},
    parse::{parse_key, parse_query},
    query::Key,
    recipes::{DefaultRecipeProvider, Recipe},
    state::State,
    store::{AsyncMemoryStore, AsyncStore},
    value::Value,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;
type TestEnv = SimpleEnvironment<Value>;

/// `apply_with(query)` applies `query` to the text `"input"` through `apply_ad_hoc`;
/// `depend_on(query)` evaluates `query` through `get_dependency_state`;
/// `read_b` reads `-R/data/b.txt` as a dependency and appends it to its input;
/// `apply_relative` applies the relative `-R-key/./b.txt`; `rnd` is volatile; `apply_recover`
/// applies `read_b_fail` (which reads `-R/data/b.txt`, then fails) and ignores the failure.
fn register(cr: &mut CommandRegistry<TestEnv>) -> Result<(), Error> {
    cr.register_async_command(CommandKey::new_name("apply_with"), |_, args, ctx| {
        Box::pin(async move {
            let query: String = args.get(0, "query")?;
            let recipe = Recipe::new(query, String::new(), String::new())?;
            let asset = ctx
                .apply_ad_hoc(recipe, State::new().with_data(Value::from("input")))
                .await?;
            let text = asset.get().await?.try_into_string()?;
            Ok(Value::from(text))
        })
    })?
    .with_argument(liquers_core::command_metadata::ArgumentInfo::string_argument("query"));
    cr.register_async_command(CommandKey::new_name("depend_on"), |_, args, ctx| {
        Box::pin(async move {
            let query: String = args.get(0, "query")?;
            let state = ctx.get_dependency_state(&parse_query(&query)?).await?;
            Ok(Value::from(state.try_into_string()?))
        })
    })?
    .with_argument(liquers_core::command_metadata::ArgumentInfo::string_argument("query"));
    cr.register_async_command(CommandKey::new_name("read_b"), |state, _, ctx| {
        let input = state.try_into_string();
        Box::pin(async move {
            let b = ctx
                .get_dependency_state(&parse_query("-R/data/b.txt")?)
                .await?
                .try_into_string()?;
            Ok(Value::from(format!("{}+{b}", input?)))
        })
    })?;
    cr.register_async_command(CommandKey::new_name("apply_recover"), |_, _, ctx| {
        Box::pin(async move {
            let recipe = Recipe::new("read_b_fail".to_string(), String::new(), String::new())?;
            let outcome = ctx.apply_ad_hoc(recipe, State::new()).await;
            Ok(Value::from(if outcome.is_err() { "recovered" } else { "succeeded" }))
        })
    })?;
    cr.register_async_command(CommandKey::new_name("read_b_fail"), |_, _, ctx| {
        Box::pin(async move {
            ctx.get_dependency_state(&parse_query("-R/data/b.txt")?).await?;
            Err(Error::general_error("read b, then failed".to_string()))
        })
    })?;
    cr.register_async_command(CommandKey::new_name("apply_relative"), |_, _, ctx| {
        Box::pin(async move {
            let recipe = Recipe::new("-R-key/./b.txt".to_string(), String::new(), String::new())?;
            let asset = ctx.apply_ad_hoc(recipe, State::new()).await?;
            Ok(Value::from(asset.get().await?.try_into_string()?))
        })
    })?;
    cr.register_command(CommandKey::new_name("rnd"), |_, _, _| Ok(Value::from("random")))?
        .volatile = true;
    Ok(())
}

async fn env() -> Result<liquers_core::context::EnvRef<TestEnv>, Error> {
    let store = AsyncMemoryStore::new(&Key::new());
    let mut ready = MetadataRecord::new();
    ready.status = Status::Ready;
    store
        .set(
            &parse_key("data/b.txt")?,
            b"bee",
            &Metadata::MetadataRecord(ready),
        )
        .await?;
    let mut env = TestEnv::new();
    register(&mut env.command_registry)?;
    env.with_async_store(Box::new(store));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider::new()));
    Ok(env.to_ref())
}

#[tokio::test]
async fn apply_ad_hoc_propagates_dependencies() -> TestResult {
    let envref = env().await?;
    let asset = envref
        .get_asset_manager()
        .get_asset(&parse_query("apply_with-read_b")?)
        .await?;
    let state = asset.get().await?;
    assert_eq!(state.try_into_string()?, "input+bee");
    let b = DependencyKey::from(&parse_key("data/b.txt")?);
    let deps = asset.get_metadata().await?.get_dependencies().to_vec();
    assert!(
        deps.iter().any(|d| d.key == b),
        "the ad-hoc read of data/b.txt is a dependency of the parent: {deps:?}"
    );
    Ok(())
}

/// A recovered ad-hoc failure still passes on what it read, so the parent is invalidated by it.
#[tokio::test]
async fn apply_ad_hoc_failure_propagates_dependencies() -> TestResult {
    let envref = env().await?;
    let asset = envref
        .get_asset_manager()
        .get_asset(&parse_query("apply_recover")?)
        .await?;
    assert_eq!(asset.get().await?.try_into_string()?, "recovered");
    let b = DependencyKey::from(&parse_key("data/b.txt")?);
    let deps = asset.get_metadata().await?.get_dependencies().to_vec();
    assert!(deps.iter().any(|d| d.key == b), "the failed read of data/b.txt is a dependency: {deps:?}");
    Ok(())
}

#[tokio::test]
async fn apply_ad_hoc_rejects_relative_query() -> TestResult {
    let envref = env().await?;
    let asset = envref
        .get_asset_manager()
        .get_asset(&parse_query("apply_relative")?)
        .await?;
    let error = asset
        .get()
        .await?
        .error_result()
        .err()
        .ok_or("a relative query must fail")?;
    assert!(
        error.to_string().contains("elative"),
        "the error names the relative query: {error}"
    );
    Ok(())
}

/// Volatility is fixed before evaluation (`design/volatility-system`, decision 4): neither
/// `get_dependency_state` nor `apply_ad_hoc` makes a non-volatile parent volatile. The parent
/// that wants a volatile result puts `v` ahead of its own action.
#[tokio::test]
async fn apply_ad_hoc_volatility_is_static() -> TestResult {
    let envref = env().await?;
    let manager = envref.get_asset_manager();
    for query in ["apply_with-rnd", "depend_on-rnd"] {
        let asset = manager.get_asset(&parse_query(query)?).await?;
        assert_eq!(asset.get().await?.try_into_string()?, "random");
        assert!(!asset.is_volatile().await, "{query} is not volatile");
    }
    let asset = manager.get_asset(&parse_query("v/apply_with-rnd")?).await?;
    asset.get().await?;
    assert!(asset.is_volatile().await, "a query starting with v is volatile");
    Ok(())
}
