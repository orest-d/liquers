//! A recipe's preview (`get_asset_info`, `recipe_plan`) is built and analysed by the same
//! function as the evaluated asset's plan, so their flags agree.
//! `specs/design/recipe-preview-shares-plan-analysis/`.

use std::time::Duration;

use liquers_core::{
    assets::AssetManager,
    context::{Context, EnvRef, Environment, SimpleEnvironment},
    error::Error,
    expiration::Expires,
    metadata::Metadata,
    parse::parse_key,
    query::Key,
    recipes::{DefaultRecipeProvider, Recipe, RecipeList},
    state::State,
    store::{AsyncMemoryStore, AsyncStore},
    value::Value,
};
use liquers_macro::register_command;

type TestEnv = SimpleEnvironment<Value>;
type CommandEnvironment = TestEnv;
type TestResult = Result<(), Box<dyn std::error::Error>>;

fn upper(state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from(state.try_into_string()?.to_uppercase()))
}

fn source() -> Result<Value, Error> {
    Ok(Value::from("source"))
}

async fn env() -> Result<EnvRef<TestEnv>, Box<dyn std::error::Error>> {
    let mut env = TestEnv::new();
    let cr = &mut env.command_registry;
    register_command!(cr, fn upper(state) -> result)?;
    register_command!(cr, fn source() -> result expires: "in 10 min")?;
    let store = AsyncMemoryStore::new(&Key::new());
    let mut list = RecipeList::new();
    for (query, title) in [
        ("source/s.txt", "S"),
        ("-R/./s.txt/-/upper/r.txt", "R"),
        ("-R/./b.txt/-/upper/a.txt", "A"),
        ("-R/./a.txt/-/upper/b.txt", "B"),
    ] {
        list.add_recipe(Recipe::new(query.to_string(), title.to_string(), String::new())?);
    }
    // A volatile upstream declared by its recipe (`volatile: true`), as the design's example has it.
    let mut declared = Recipe::new("source/d.txt".to_string(), "D".to_string(), String::new())?;
    declared.volatile = true;
    list.add_recipe(declared);
    list.add_recipe(Recipe::new("-R/./d.txt/-/upper/dv.txt".to_string(), "DV".to_string(), String::new())?);
    let yaml = serde_yaml::to_string(&list)?;
    store
        .set(&parse_key("data/recipes.yaml")?, yaml.as_bytes(), &Metadata::new())
        .await?;
    env.with_async_store(Box::new(store));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider::new()));
    Ok(env.to_ref())
}

/// AC-2: for a relative operand with an expiring upstream (`r.txt`) and with an upstream whose
/// recipe declares `volatile: true` (`dv.txt`), the preview's `is_volatile` and `expires` equal the evaluated asset's metadata.
#[tokio::test]
async fn asset_info_flags_equal_the_evaluated_assets() -> TestResult {
    let envref = env().await?;
    for (name, volatile, expires) in [
        ("data/r.txt", false, Expires::InDuration(Duration::from_secs(600))),
        ("data/dv.txt", true, Expires::Never),
    ] {
        let key = parse_key(name)?;
        let info = envref.get_recipe_provider().get_asset_info(&key, envref.clone()).await?;
        assert!(!info.is_error, "{name}: {}", info.message);

        let asset = envref.get_asset_manager().get(&key).await?;
        let _ = asset.get().await?;
        let metadata = asset.get_metadata().await?;

        assert_eq!(info.is_volatile, metadata.is_volatile(), "{name}: preview vs run volatility");
        assert_eq!(info.is_volatile, volatile, "{name}: preview volatility");
        if !volatile {
            assert_eq!(info.expires, expires, "{name}: preview expiry");
            assert_eq!(info.expires, metadata.expires(), "{name}: preview vs run expiry");
        }
    }
    Ok(())
}

/// AC-3: a recipe whose analysis finds a cycle is reported as an error naming the cycle.
#[tokio::test]
async fn asset_info_reports_an_analysis_cycle() -> TestResult {
    let envref = env().await?;
    let info = envref
        .get_recipe_provider()
        .get_asset_info(&parse_key("data/a.txt")?, envref.clone())
        .await?;
    assert!(info.is_error);
    assert!(info.message.to_lowercase().contains("circular"), "{}", info.message);
    assert!(info.error_data.is_some());
    Ok(())
}

/// AC-1: the recipe plan propagates an analysis error instead of returning a plan carrying it.
#[tokio::test]
async fn recipe_plan_propagates_an_analysis_error() -> TestResult {
    let envref = env().await?;
    let key = parse_key("data/a.txt")?;
    let result = envref.get_recipe_provider().recipe_plan(&key, envref.clone()).await;
    let err = result.err().ok_or("a cyclic recipe's plan must be an error")?;
    assert!(err.message.to_lowercase().contains("circular"), "{}", err.message);

    let recipe = envref.get_recipe_provider().recipe(&key, envref.clone()).await?;
    assert!(recipe.plan_with_dependencies(envref, Some(&key)).await.is_err());
    Ok(())
}

