//! The library environment's service defaults.
//!
//! `liquers-lib` and `liquers-core` deliberately disagree about the default recipe provider, and
//! the disagreement is invisible at compile time: both produce an environment, and the difference
//! only shows when a `-R/` query does or does not resolve a recipe. That is exactly the kind of
//! regression consolidation can introduce silently, so it is pinned here behaviourally — does a
//! recipe in the store actually resolve? — and at the level that matters: the plain constructor,
//! not only the builder.

use liquers_core::{
    context::Environment,
    error::Error,
    metadata::Metadata,
    parse::parse_key,
    query::Key,
    store::{AsyncMemoryStore, AsyncStore},
};
use liquers_lib::environment::{default_environment_builder, DefaultEnvironment};
use liquers_lib::value::Value;

/// A store holding one recipe, so a store-backed provider has something to find.
async fn recipe_store() -> Result<AsyncMemoryStore, Error> {
    let store = AsyncMemoryStore::new(&Key::new());
    store
        .set(
            &parse_key("recipes.yaml")?,
            b"recipes:\n  - query: greet/dash.txt\n",
            &Metadata::new(),
        )
        .await?;
    Ok(store)
}

/// `DefaultEnvironment::new()` must resolve recipes through the store.
///
/// Regression guard. `liquers-lib`'s constructor has always installed `DefaultRecipeProvider`,
/// while `liquers-core`'s installs `TrivialRecipeProvider`. An application constructing a
/// `DefaultEnvironment` directly and relying on store-backed recipes would see every `-R/` query
/// fail with `KeyNotFound` if this became `Trivial` — with nothing failing to compile.
#[tokio::test]
async fn default_environment_new_resolves_recipes_through_the_store() -> Result<(), Error> {
    let mut env = DefaultEnvironment::<Value>::new();
    env.with_async_store(Box::new(recipe_store().await?));
    let envref = env.to_ref();

    let recipe = envref
        .get_recipe_provider()
        .recipe(&parse_key("dash.txt")?, envref.clone())
        .await;
    assert!(
        recipe.is_ok(),
        "DefaultEnvironment::new() must resolve recipes through the store, got {recipe:?}"
    );
    Ok(())
}

/// The builder carrying the library defaults agrees with the constructor.
#[tokio::test]
async fn default_environment_builder_matches_the_constructor() -> Result<(), Error> {
    let envref = default_environment_builder::<Value, ()>()
        .with_async_store(std::sync::Arc::new(recipe_store().await?))
        .build()?;

    let recipe = envref
        .get_recipe_provider()
        .recipe(&parse_key("dash.txt")?, envref.clone())
        .await;
    assert!(recipe.is_ok(), "the library builder must agree with the library constructor");
    Ok(())
}

/// The core builder keeps its own default, which is deliberately *not* the library's.
///
/// Asserted so a later change that collapses the two defaults fails here rather than in an
/// application.
#[tokio::test]
async fn the_core_builder_default_is_still_trivial() -> Result<(), Error> {
    use liquers_core::environment_builder::{EnvironmentBuilder, Inline};

    let envref = EnvironmentBuilder::<Value, (), Inline>::new()
        .with_async_store(std::sync::Arc::new(recipe_store().await?))
        .build()?;

    let recipe = envref
        .get_recipe_provider()
        .recipe(&parse_key("dash.txt")?, envref.clone())
        .await;
    assert!(
        recipe.is_err(),
        "the core builder resolves no recipes, even with a recipe in the store"
    );
    Ok(())
}
