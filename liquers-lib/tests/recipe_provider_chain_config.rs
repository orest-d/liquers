//! `recipe-provider-chain-config`: `LibKind`'s recipe lists, the `manifest` entry type and the
//! setters (`specs/design/recipe-provider-chain-config/`, Phase 3 test plan).
//!
//! Each test is gated on its own, so a build without `records` runs the subset that applies.
use liquers_core::environment_builder::AssetManagerKind;
#[cfg(not(feature = "records"))]
use liquers_core::recipes::RecipeProviderEntry;
use liquers_core::recipes::{resolve_recipes, RecipesConfig};
use liquers_lib::environment::{DefaultEnvironment, LibKind};
use liquers_lib::value::Value;

type Env = DefaultEnvironment<Value>;

/// AC-3: the kind's default list and its unconfigured chain resolve in this build.
#[test]
fn lib_kind_defaults_resolve() -> Result<(), liquers_core::error::Error> {
    let factory = LibKind::recipe_provider_factory::<Env>();
    let chain = resolve_recipes(
        &RecipesConfig::KindDefault,
        &LibKind::default_recipes(),
        &factory,
    )?;
    if cfg!(feature = "records") {
        assert_eq!(chain.names(), vec!["recipes_yaml", "manifest"]);
    } else {
        assert_eq!(chain.names(), vec!["recipes_yaml"]);
    }
    assert_eq!(LibKind::unconfigured_recipes(), RecipesConfig::KindDefault);
    Ok(())
}

/// AC-6: without `records`, a list naming `manifest` fails, naming the feature.
#[cfg(not(feature = "records"))]
#[test]
fn manifest_without_records_names_the_feature() {
    use liquers_core::error::ErrorType;
    let factory = LibKind::recipe_provider_factory::<Env>();
    let error = match resolve_recipes(
        &RecipesConfig::Chain(vec![RecipeProviderEntry::named("manifest")]),
        &LibKind::default_recipes(),
        &factory,
    ) {
        Ok(_) => panic!("manifest is unavailable without records"),
        Err(error) => error,
    };
    assert_eq!(error.error_type, ErrorType::ConfigurationError);
    assert!(
        error.message.contains("manifest") && error.message.contains("records"),
        "{}",
        error.message
    );
}

#[cfg(feature = "records")]
mod records {
    use super::*;
    use std::sync::Arc;

    use liquers_core::context::{Context, EnvRef, Environment};
    use liquers_core::environment_builder::EnvironmentBuilder;
    use liquers_core::environment_config::EnvironmentConfig;
    use liquers_core::error::Error;
    use liquers_core::interpreter::evaluate;
    use liquers_core::metadata::Metadata;
    use liquers_core::parse::parse_key;
    use liquers_core::query::Key;
    use liquers_core::state::State;
    use liquers_core::store::{AsyncMemoryStore, AsyncStore};
    use liquers_lib::environment::{default_environment_builder, RecordsRecipeProvider};
    use liquers_macro::register_command;

    type CommandEnvironment = Env;
    type Builder = EnvironmentBuilder<Value, (), LibKind>;

    const STORE_YAML: &str = "store:\n  stores:\n    - type: memory\n      prefix: data\n";
    const MANIFEST: &[u8] = b"manifest: record-stream\nchunks:\n  - query: greet/greet.csv\n";
    const RECIPES: &[u8] = b"recipes:\n  - query: hello/repeat-3/triple.txt\n";
    const CLASHING_RECIPES: &[u8] = b"recipes:\n  - query: hello/greet.csv\n";

    fn hello() -> Result<Value, Error> {
        Ok(Value::from("hello"))
    }

    fn repeat(state: &State<Value>, n: i32) -> Result<Value, Error> {
        Ok(Value::from(
            state.try_into_string()?.repeat(n.max(0) as usize),
        ))
    }

    fn greet() -> Result<Value, Error> {
        Ok(Value::from("hello"))
    }

    fn register_fixture_commands(builder: &mut Builder) -> Result<(), Error> {
        let cr = &mut builder.command_registry;
        register_command!(cr, fn hello() -> result)?;
        register_command!(cr, fn repeat(state, n: i32) -> result)?;
        register_command!(cr, fn greet() -> result)?;
        Ok(())
    }

    async fn write(store: &dyn AsyncStore, key: &str, bytes: &[u8]) -> Result<(), Error> {
        let key = parse_key(key)?;
        let mut metadata = Metadata::new();
        if let Some(name) = key.filename() {
            metadata.set_filename(&name.encode())?;
        }
        store.set(&key, bytes, &metadata).await
    }

    async fn write_fixtures(envref: &EnvRef<Env>, recipes: &[u8]) -> Result<(), Error> {
        let store = envref.get_async_store();
        write(store.as_ref(), "data/recipes.yaml", recipes).await?;
        write(store.as_ref(), "data/chunks.manifest.yaml", MANIFEST).await
    }

    /// A builder over a memory store at `data`, configured by `recipes` (a `recipes:` line or
    /// nothing).
    fn configured(recipes: &str) -> Result<Builder, Error> {
        let config = EnvironmentConfig::from_yaml(&format!("{STORE_YAML}{recipes}"))?;
        let mut builder = default_environment_builder::<Value, ()>().with_config(
            config,
            Box::new(liquers_store::store_factory::default_store_factory()),
        );
        register_fixture_commands(&mut builder)?;
        Ok(builder)
    }

    async fn text(envref: &EnvRef<Env>, query: &str) -> Result<String, Error> {
        evaluate(envref.clone(), query, None)
            .await?
            .try_into_string()
    }

    /// AC-3, AC-9: absent, `default` and an explicit list naming both keep manifests.
    #[tokio::test]
    async fn configured_lib_build_keeps_manifests() -> Result<(), Box<dyn std::error::Error>> {
        for recipes in [
            "",
            "recipes: default\n",
            "recipes: [recipes_yaml, manifest]\n",
        ] {
            let envref = configured(recipes)?.build()?;
            write_fixtures(&envref, RECIPES).await?;
            assert_eq!(
                text(&envref, "-R/data/triple.txt").await?,
                "hellohellohello"
            );
            assert_eq!(
                text(&envref, "-R/data/greet.csv").await?,
                "hello",
                "{recipes}"
            );
        }
        Ok(())
    }

    /// AC-1, AC-5: a list without `manifest` serves `recipes.yaml` and no chunk; `[]` neither.
    #[tokio::test]
    async fn explicit_list_without_manifest_drops_chunks() -> Result<(), Box<dyn std::error::Error>>
    {
        let envref = configured("recipes: [recipes_yaml]\n")?.build()?;
        write_fixtures(&envref, RECIPES).await?;
        assert_eq!(
            text(&envref, "-R/data/triple.txt").await?,
            "hellohellohello"
        );
        assert!(text(&envref, "-R/data/greet.csv").await.is_err());

        let envref = configured("recipes: []\n")?.build()?;
        write_fixtures(&envref, RECIPES).await?;
        assert!(text(&envref, "-R/data/triple.txt").await.is_err());
        assert!(text(&envref, "-R/data/greet.csv").await.is_err());
        Ok(())
    }

    /// AC-1: a `default` entry splices the kind's list, so `[default]` behaves as absent.
    #[tokio::test]
    async fn default_entry_splices_the_kind_list() -> Result<(), Box<dyn std::error::Error>> {
        let envref = configured("recipes: [default]\n")?.build()?;
        write_fixtures(&envref, RECIPES).await?;
        assert_eq!(
            text(&envref, "-R/data/triple.txt").await?,
            "hellohellohello"
        );
        assert_eq!(text(&envref, "-R/data/greet.csv").await?, "hello");
        Ok(())
    }

    /// AC-8, AC-9: `DefaultEnvironment::new()` + `with_default_recipe_provider()` — the
    /// `liquers-web` construction path — keeps manifests.
    #[tokio::test]
    async fn default_recipe_provider_setter_keeps_manifests(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut env = Env::new();
        {
            let cr = &mut env.command_registry;
            register_command!(cr, fn hello() -> result)?;
            register_command!(cr, fn repeat(state, n: i32) -> result)?;
            register_command!(cr, fn greet() -> result)?;
        }
        env.with_async_store(Box::new(AsyncMemoryStore::new(&Key::new())));
        env.with_default_recipe_provider();
        let envref = env.to_ref();
        write_fixtures(&envref, RECIPES).await?;
        assert_eq!(
            text(&envref, "-R/data/triple.txt").await?,
            "hellohellohello"
        );
        assert_eq!(text(&envref, "-R/data/greet.csv").await?, "hello");
        Ok(())
    }

    /// AC-8, AC-11: `with_records_recipe_provider` never adds a second manifest provider, which
    /// would make every chunk a two-owner error.
    #[tokio::test]
    async fn records_recipe_provider_is_not_added_twice() -> Result<(), Box<dyn std::error::Error>>
    {
        for recipes in ["", "recipes: [manifest]\n", "recipes: [recipes_yaml]\n"] {
            let envref = configured(recipes)?
                .with_records_recipe_provider()
                .build()?;
            write_fixtures(&envref, RECIPES).await?;
            assert_eq!(
                text(&envref, "-R/data/greet.csv").await?,
                "hello",
                "{recipes}"
            );
        }
        Ok(())
    }

    /// AC-11, the documented migration hazard: a raw `ManifestRecipeProvider` appended after a
    /// default that already holds `manifest` makes every chunk a two-owner error.
    #[tokio::test]
    async fn raw_manifest_appended_after_the_default_is_a_two_owner_error(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let envref = configured("")?
            .with_appended_recipe_provider(Arc::new(liquers_records::ManifestRecipeProvider::new()))
            .build()?;
        write_fixtures(&envref, RECIPES).await?;
        let error = text(&envref, "-R/data/greet.csv")
            .await
            .expect_err("two manifest providers own the chunk");
        assert!(
            error.message.contains("two recipe providers"),
            "{}",
            error.message
        );
        Ok(())
    }

    /// AC-11: a chunk name the folder's `recipes.yaml` also defines is refused, naming both
    /// providers, in either list order.
    #[tokio::test]
    async fn a_key_two_members_define_is_refused_in_any_order(
    ) -> Result<(), Box<dyn std::error::Error>> {
        for recipes in [
            "recipes: [recipes_yaml, manifest]\n",
            "recipes: [manifest, recipes_yaml]\n",
        ] {
            let envref = configured(recipes)?.build()?;
            write_fixtures(&envref, CLASHING_RECIPES).await?;
            let error = text(&envref, "-R/data/greet.csv")
                .await
                .expect_err("two owners");
            assert!(
                error.message.contains("'recipes_yaml'") && error.message.contains("'manifest'"),
                "{recipes}: {}",
                error.message
            );
        }
        Ok(())
    }
}
