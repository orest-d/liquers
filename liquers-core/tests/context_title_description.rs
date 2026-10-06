//! `Context::set_title` / `Context::set_description`
//! (`specs/design/context-title-description/`). A recipe's non-empty title or description wins,
//! per field; a command fills what the recipe left empty.
//!
//! Scenarios are generic over `E: Environment<Value = Value>` and run on both asset-manager kinds.

use liquers_core::{
    assets::AssetManager,
    context::{Context, EnvRef, Environment, ImmediateEnvironment, SimpleEnvironment},
    error::Error,
    metadata::Metadata,
    parse::{parse_key, parse_query},
    query::Key,
    recipes::DefaultRecipeProvider,
    state::State,
    store::{AsyncMemoryStore, AsyncStore},
    value::Value,
};
use liquers_macro::register_command;

async fn titled<E: Environment<Value = Value>>(
    _state: State<Value>,
    context: Context<E>,
) -> Result<Value, Error> {
    context.set_title("T").await?;
    context.set_description("D").await?;
    Ok(Value::from("x"))
}

async fn retitled<E: Environment<Value = Value>>(
    state: State<Value>,
    context: Context<E>,
) -> Result<Value, Error> {
    context.set_title("T2").await?;
    Ok(Value::from(state.try_into_string()?))
}

fn plain(_state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from("x"))
}

const RECIPES: &str = r#"recipes:
  - query: titled/out.txt
    title: R
    description: RD
  - query: titled/part.txt
    title: R
  - query: titled/bare.txt
  - query: plain/p.txt
  - query: titled/t.txt
"#;

async fn seeded_store() -> Result<AsyncMemoryStore, Error> {
    let store = AsyncMemoryStore::new(&Key::new());
    store
        .set(
            &parse_key("data/recipes.yaml")?,
            RECIPES.as_bytes(),
            &Metadata::new(),
        )
        .await?;
    Ok(store)
}

fn build_default_env(store: AsyncMemoryStore) -> Result<EnvRef<SimpleEnvironment<Value>>, Error> {
    type CommandEnvironment = SimpleEnvironment<Value>;
    let mut env = SimpleEnvironment::<Value>::new();
    {
        let cr = &mut env.command_registry;
        register_command!(cr, async fn titled(state, context) -> result)?;
        register_command!(cr, async fn retitled(state, context) -> result)?;
        register_command!(cr, fn plain(state) -> result)?;
    }
    env.with_async_store(Box::new(store));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    Ok(env.to_ref())
}

fn build_immediate_env(
    store: AsyncMemoryStore,
) -> Result<EnvRef<ImmediateEnvironment<Value>>, Error> {
    type CommandEnvironment = ImmediateEnvironment<Value>;
    let mut env = ImmediateEnvironment::<Value>::new();
    {
        let cr = &mut env.command_registry;
        register_command!(cr, async fn titled(state, context) -> result)?;
        register_command!(cr, async fn retitled(state, context) -> result)?;
        register_command!(cr, fn plain(state) -> result)?;
    }
    env.with_async_store(Box::new(store));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    Ok(env.to_ref())
}

async fn title_and_description<E: Environment<Value = Value>>(
    envref: &EnvRef<E>,
    query: &str,
) -> Result<(String, String), Error> {
    let asset = envref.evaluate(query).await?;
    asset.get().await?;
    let metadata = asset.get_metadata().await?;
    Ok((
        metadata.title().to_string(),
        metadata.description().to_string(),
    ))
}

async fn scenario_command_sets_query_asset<E: Environment<Value = Value>>(
    envref: EnvRef<E>,
) -> Result<(), Error> {
    let (t, d) = title_and_description(&envref, "titled").await?;
    assert_eq!((t.as_str(), d.as_str()), ("T", "D"));
    Ok(())
}

async fn scenario_recipe_wins<E: Environment<Value = Value>>(
    envref: EnvRef<E>,
) -> Result<(), Error> {
    let (t, d) = title_and_description(&envref, "-R/data/out.txt").await?;
    assert_eq!((t.as_str(), d.as_str()), ("R", "RD"));
    Ok(())
}

async fn scenario_command_fills_empty_field<E: Environment<Value = Value>>(
    envref: EnvRef<E>,
) -> Result<(), Error> {
    let store = envref.get_async_store();
    let (t, d) = title_and_description(&envref, "-R/data/part.txt").await?;
    assert_eq!((t.as_str(), d.as_str()), ("R", "D"));
    // Persisted in the store (the save is in the background; wait for it).
    let key = parse_key("data/part.txt")?;
    let mut stored = None;
    for _ in 0..50 {
        if let Ok((_, metadata)) = store.get(&key).await {
            if metadata.description() == "D" {
                stored = Some(metadata);
                break;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let stored = stored.expect("description not persisted");
    assert_eq!(stored.title(), "R");
    Ok(())
}

async fn scenario_command_fills_both<E: Environment<Value = Value>>(
    envref: EnvRef<E>,
) -> Result<(), Error> {
    let (t, d) = title_and_description(&envref, "-R/data/bare.txt").await?;
    assert_eq!((t.as_str(), d.as_str()), ("T", "D"));
    Ok(())
}

async fn scenario_version_unchanged<E: Environment<Value = Value>>(
    envref: EnvRef<E>,
) -> Result<(), Error> {
    let with = envref.evaluate("-R/data/t.txt").await?;
    with.get().await?;
    let without = envref.evaluate("-R/data/p.txt").await?;
    without.get().await?;
    assert_eq!(
        with.get_metadata().await?.version(),
        without.get_metadata().await?.version()
    );
    Ok(())
}

async fn scenario_later_step_wins<E: Environment<Value = Value>>(
    envref: EnvRef<E>,
) -> Result<(), Error> {
    // Applied to an input state the plan stays fully expanded, so both steps share one asset and
    // one `Context`. Plain evaluation would cut `titled` off as a predecessor asset of its own.
    let asset = envref
        .get_asset_manager()
        .apply(
            parse_query("titled/retitled")?.into(),
            State::new().with_data(Value::from("in")),
            None,
        )
        .await?;
    asset.get().await?;
    let metadata = asset.get_metadata().await?;
    let (t, d) = (
        metadata.title().to_string(),
        metadata.description().to_string(),
    );
    assert_eq!((t.as_str(), d.as_str()), ("T2", "D"));
    Ok(())
}

#[tokio::test]
async fn command_sets_title_and_description_of_a_query_asset() -> Result<(), Error> {
    scenario_command_sets_query_asset(build_default_env(seeded_store().await?)?).await?;
    scenario_command_sets_query_asset(build_immediate_env(seeded_store().await?)?).await
}

#[tokio::test]
async fn recipe_title_and_description_win_over_the_command() -> Result<(), Error> {
    scenario_recipe_wins(build_default_env(seeded_store().await?)?).await
}

#[tokio::test]
async fn recipe_title_wins_on_the_immediate_manager() -> Result<(), Error> {
    scenario_recipe_wins(build_immediate_env(seeded_store().await?)?).await
}

#[tokio::test]
async fn command_fills_a_field_the_recipe_left_empty() -> Result<(), Error> {
    scenario_command_fills_empty_field(build_default_env(seeded_store().await?)?).await
}

#[tokio::test]
async fn command_fills_both_fields_when_the_recipe_declares_neither() -> Result<(), Error> {
    scenario_command_fills_both(build_default_env(seeded_store().await?)?).await?;
    scenario_command_fills_both(build_immediate_env(seeded_store().await?)?).await
}

#[tokio::test]
async fn title_does_not_change_version() -> Result<(), Error> {
    scenario_version_unchanged(build_default_env(seeded_store().await?)?).await?;
    scenario_version_unchanged(build_immediate_env(seeded_store().await?)?).await
}

#[tokio::test]
async fn a_later_step_in_the_same_query_wins() -> Result<(), Error> {
    scenario_later_step_wins(build_default_env(seeded_store().await?)?).await?;
    scenario_later_step_wins(build_immediate_env(seeded_store().await?)?).await
}
