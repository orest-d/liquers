//! `recipe-provider-chain-config`: an environment's recipe-provider chain, configured as a list.
//!
//! Builder resolution, its errors and the setters (`specs/design/recipe-provider-chain-config/`,
//! Phase 3 test plan). Lookups go through the built environment's recipe provider, so these tests
//! need no commands.
use std::sync::Arc;

use async_trait::async_trait;
use liquers_core::{
    context::{EnvRef, Environment, GenericEnvironment},
    environment_builder::{AssetManagerKind, EnvironmentBuilder, Inline, Queued},
    environment_config::EnvironmentConfig,
    error::{Error, ErrorType},
    metadata::Metadata,
    parse::parse_key,
    plan::Plan,
    query::{Key, ResourceName},
    recipes::{
        resolve_recipes, AsyncRecipeProvider, Recipe, RecipeProviderChoice, RecipeProviderEntry,
        RecipeProviderTypeInfo, RecipeProviderTypeMap, RecipesConfig,
    },
    store::{AsyncMemoryStore, AsyncStore},
    value::Value,
};

type Env = GenericEnvironment<Value, (), Inline>;
type Builder = EnvironmentBuilder<Value, (), Inline>;

const RECIPES_YAML: &[u8] = br#"recipes:
  - query: "hello/a.txt"
"#;

/// A provider answering exactly one key with a fixed recipe.
struct Fixed {
    key: Key,
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<E: Environment> AsyncRecipeProvider<E> for Fixed {
    async fn has_recipes(&self, key: &Key, _envref: EnvRef<E>) -> Result<bool, Error> {
        Ok(self.key.parent() == *key)
    }

    async fn assets_with_recipes(
        &self,
        _key: &Key,
        _envref: EnvRef<E>,
    ) -> Result<Vec<ResourceName>, Error> {
        Ok(Vec::new())
    }

    async fn recipe_plan(&self, key: &Key, envref: EnvRef<E>) -> Result<Plan, Error> {
        self.recipe(key, envref.clone())
            .await?
            .to_plan_for_key(envref.get_command_metadata_registry(), key)
    }

    async fn recipe(&self, key: &Key, envref: EnvRef<E>) -> Result<Recipe, Error> {
        self.recipe_opt(key, envref)
            .await?
            .ok_or_else(|| Error::key_not_found(key))
    }

    async fn recipe_opt(&self, key: &Key, _envref: EnvRef<E>) -> Result<Option<Recipe>, Error> {
        if *key == self.key {
            Ok(Some(Recipe::new(
                "fixed".to_string(),
                String::new(),
                String::new(),
            )?))
        } else {
            Ok(None)
        }
    }
}

/// An integration's entry types: `fixed`, with a `key` argument naming the one key it answers.
fn fixed_factory<E: Environment>() -> RecipeProviderTypeMap<E> {
    RecipeProviderTypeMap::new().with_provider_type(
        RecipeProviderTypeInfo::new("fixed").with_doc("Answers one key."),
        Box::new(|entry: &RecipeProviderEntry| {
            let key = entry
                .arguments
                .get("key")
                .and_then(|value| value.as_str())
                .ok_or_else(|| Error::configuration_error("fixed needs a key".to_string()))?;
            Ok(Arc::new(Fixed {
                key: parse_key(key)?,
            }) as Arc<dyn AsyncRecipeProvider<E>>)
        }),
    )
}

fn fixed(key: &str) -> RecipeProviderEntry {
    RecipeProviderEntry::named("fixed").with_argument("key", key)
}

fn recipes(yaml: &str) -> RecipesConfig {
    serde_yaml::from_str(yaml).expect("recipes document")
}

/// A builder over a memory store whose `data/recipes.yaml` defines `data/a.txt`.
async fn builder() -> Result<Builder, Error> {
    let store = AsyncMemoryStore::new(&Key::new());
    store
        .set(
            &parse_key("data/recipes.yaml")?,
            RECIPES_YAML,
            &Metadata::new(),
        )
        .await?;
    Ok(Builder::new().with_async_store(Arc::new(store)))
}

async fn recipe_query(envref: &EnvRef<Env>, key: &str) -> Result<Option<String>, Error> {
    Ok(envref
        .get_recipe_provider()
        .recipe_opt(&parse_key(key)?, envref.clone())
        .await?
        .map(|recipe| recipe.query))
}

fn build_error(builder: Builder) -> Error {
    match builder.build() {
        Ok(_) => panic!("the build should fail"),
        Err(error) => error,
    }
}

/// AC-1: the chain is the listed entries in list order; a two-owner error names them in order.
#[tokio::test]
async fn configured_list_is_the_chain_in_order() -> Result<(), Box<dyn std::error::Error>> {
    let factory = Inline::recipe_provider_factory::<Env>().chain_first(Box::new(fixed_factory()));
    let chain = resolve_recipes(
        &RecipesConfig::Chain(vec![
            fixed("data/b.txt"),
            RecipeProviderEntry::named("recipes_yaml"),
        ]),
        &Inline::default_recipes(),
        &factory,
    )?;
    assert_eq!(chain.names(), vec!["fixed", "recipes_yaml"]);

    for (list, first, second) in [
        (
            vec![
                RecipeProviderEntry::named("recipes_yaml"),
                fixed("data/a.txt"),
            ],
            "'recipes_yaml'",
            "'fixed'",
        ),
        (
            vec![
                fixed("data/a.txt"),
                RecipeProviderEntry::named("recipes_yaml"),
            ],
            "'fixed'",
            "'recipes_yaml'",
        ),
    ] {
        let envref = builder()
            .await?
            .with_recipe_provider_factory(Box::new(fixed_factory()))
            .with_recipes(RecipesConfig::Chain(list))
            .build()?;
        let error = recipe_query(&envref, "data/a.txt")
            .await
            .expect_err("two owners");
        assert_eq!(error.error_type, ErrorType::ConfigurationError);
        let message = error.message.clone();
        let (at_first, at_second) = (
            message.find(first).expect("first named"),
            message.find(second).expect("second named"),
        );
        assert!(at_first < at_second, "{message}");
    }
    Ok(())
}

/// AC-2: `[]` (or `trivial`) resolves no recipes, even where a `recipes.yaml` defines one.
#[tokio::test]
async fn empty_list_resolves_nothing() -> Result<(), Box<dyn std::error::Error>> {
    for document in ["[]", "trivial"] {
        let envref = builder().await?.with_recipes(recipes(document)).build()?;
        assert_eq!(
            recipe_query(&envref, "data/a.txt").await?,
            None,
            "{document}"
        );
    }
    Ok(())
}

/// AC-3: in core, an absent `recipes:` is `[recipes_yaml]`.
#[tokio::test]
async fn absent_recipes_is_recipes_yaml_in_core() -> Result<(), Box<dyn std::error::Error>> {
    let config = EnvironmentConfig::from_yaml("assets: {}\n")?;
    assert_eq!(config.recipes, RecipesConfig::KindDefault);
    assert_eq!(
        Inline::default_recipes(),
        vec![RecipeProviderEntry::named("recipes_yaml")]
    );
    let envref = builder().await?.with_recipes(config.recipes).build()?;
    assert_eq!(
        recipe_query(&envref, "data/a.txt").await?.as_deref(),
        Some("hello/a.txt")
    );
    Ok(())
}

/// AC-5, AC-12: an integration's entry type is placed in the list, with its arguments.
#[tokio::test]
async fn integration_entry_type_is_placed_in_the_list() -> Result<(), Box<dyn std::error::Error>> {
    let list = recipes("[recipes_yaml, {type: fixed, key: data/extra.txt}]");
    let envref = builder()
        .await?
        .with_recipe_provider_factory(Box::new(fixed_factory()))
        .with_recipes(list)
        .build()?;
    assert_eq!(
        recipe_query(&envref, "data/extra.txt").await?.as_deref(),
        Some("fixed")
    );
    assert_eq!(
        recipe_query(&envref, "data/a.txt").await?.as_deref(),
        Some("hello/a.txt")
    );
    Ok(())
}

/// AC-6: an entry type no factory declares fails the build, naming it, its position and the
/// types that exist.
#[tokio::test]
async fn unknown_entry_type_names_entry_position_and_known_types(
) -> Result<(), Box<dyn std::error::Error>> {
    let error = build_error(builder().await?.with_recipes(recipes(
        "[recipes_yaml, {type: fixed, key: data/extra.txt}]",
    )));
    assert_eq!(error.error_type, ErrorType::ConfigurationError);
    for part in ["'fixed'", "entry 1", "recipes_yaml"] {
        assert!(error.message.contains(part), "{part}: {}", error.message);
    }
    Ok(())
}

/// AC-6: an entry listed twice, directly or through `default`, fails the build.
#[tokio::test]
async fn duplicate_entry_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    for document in ["[recipes_yaml, recipes_yaml]", "[default, recipes_yaml]"] {
        let error = build_error(builder().await?.with_recipes(recipes(document)));
        assert_eq!(
            error.error_type,
            ErrorType::ConfigurationError,
            "{document}"
        );
        assert!(
            error.message.contains("more than once"),
            "{}",
            error.message
        );
    }
    Ok(())
}

/// AC-8: the keyword setter, the list setter and a document build the same chain.
#[tokio::test]
async fn setters_build_the_same_chain_as_documents() -> Result<(), Box<dyn std::error::Error>> {
    let with_default = [
        builder()
            .await?
            .with_recipe_provider_choice(RecipeProviderChoice::Default),
        builder().await?.with_recipes(RecipesConfig::KindDefault),
        builder().await?.with_recipes(recipes("[recipes_yaml]")),
        builder()
            .await?
            .with_recipes(EnvironmentConfig::from_yaml("recipes: default\n")?.recipes),
    ];
    for builder in with_default {
        let envref = builder.build()?;
        assert_eq!(
            recipe_query(&envref, "data/a.txt").await?.as_deref(),
            Some("hello/a.txt")
        );
    }
    let without = [
        builder()
            .await?
            .with_recipe_provider_choice(RecipeProviderChoice::Trivial),
        builder().await?.with_recipes(recipes("none")),
        builder().await?,
    ];
    for builder in without {
        let envref = builder.build()?;
        assert_eq!(recipe_query(&envref, "data/a.txt").await?, None);
    }
    Ok(())
}

fn kind_default_resolves<K: AssetManagerKind>() -> Result<(), Error> {
    type E<K> = GenericEnvironment<Value, (), K>;
    let factory = K::recipe_provider_factory::<E<K>>();
    resolve_recipes(&RecipesConfig::KindDefault, &K::default_recipes(), &factory)?;
    resolve_recipes(&K::unconfigured_recipes(), &K::default_recipes(), &factory)?;
    Ok(())
}

/// AC-3, AC-8: every core kind's default list and unconfigured chain resolve.
#[test]
fn kind_defaults_resolve() -> Result<(), Error> {
    kind_default_resolves::<Inline>()?;
    kind_default_resolves::<Queued>()?;
    Ok(())
}

/// AC-6, AC-8: appending an entry already in the list is a no-op, not a duplicate.
#[tokio::test]
async fn appended_entry_duplicate_of_a_listed_entry_is_skipped(
) -> Result<(), Box<dyn std::error::Error>> {
    let envref = builder()
        .await?
        .with_recipes(recipes("[recipes_yaml]"))
        .with_appended_recipe_entry(RecipeProviderEntry::named("recipes_yaml"))
        .build()?;
    assert_eq!(
        recipe_query(&envref, "data/a.txt").await?.as_deref(),
        Some("hello/a.txt")
    );

    // Through `default` too, and an entry not yet listed is appended.
    let envref = builder()
        .await?
        .with_recipe_provider_factory(Box::new(fixed_factory()))
        .with_recipes(RecipesConfig::KindDefault)
        .with_appended_recipe_entry(RecipeProviderEntry::named("default"))
        .with_appended_recipe_entry(fixed("data/b.txt"))
        .build()?;
    assert_eq!(
        recipe_query(&envref, "data/a.txt").await?.as_deref(),
        Some("hello/a.txt")
    );
    assert_eq!(
        recipe_query(&envref, "data/b.txt").await?.as_deref(),
        Some("fixed")
    );
    Ok(())
}

/// AC-8: a provider appended in code after a configured list is consulted.
#[tokio::test]
async fn appended_provider_after_a_configured_list_is_consulted(
) -> Result<(), Box<dyn std::error::Error>> {
    let envref = builder()
        .await?
        .with_recipes(recipes("[recipes_yaml]"))
        .with_appended_recipe_provider(Arc::new(Fixed {
            key: parse_key("data/b.txt")?,
        }))
        .build()?;
    assert_eq!(
        recipe_query(&envref, "data/b.txt").await?.as_deref(),
        Some("fixed")
    );
    assert_eq!(
        recipe_query(&envref, "data/a.txt").await?.as_deref(),
        Some("hello/a.txt")
    );
    Ok(())
}
