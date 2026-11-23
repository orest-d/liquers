//! Self-contained recipe trees (design `recipe-tree`): source declarations, the tree check run by
//! the recipe provider, evaluation and relocation of a strict tree, and the tree editor.

use liquers_core::{
    assets::AssetManager,
    context::{Context, EnvRef, Environment, ImmediateEnvironment},
    error::{Error, ErrorType},
    metadata::{Metadata, MetadataRecord, Status},
    parse::{parse_key, parse_query},
    query::Key,
    recipes::{DefaultRecipeProvider, RecipeList},
    state::State,
    store::{AsyncMemoryStore, AsyncStore},
    value::{Value, ValueInterface},
};
use liquers_macro::register_command;

type CommandEnvironment = ImmediateEnvironment<Value>;

fn attach(state: &State<Value>, part: Value) -> Result<Value, Error> {
    Ok(Value::from(format!(
        "{}+{}",
        state.try_into_string()?,
        part.try_into_string()?
    )))
}

fn material(state: &State<Value>, m: Value) -> Result<Value, Error> {
    Ok(Value::from(format!(
        "{}~{}",
        state.try_into_string()?,
        m.try_into_string()?
    )))
}

fn make_env(store: AsyncMemoryStore) -> Result<EnvRef<CommandEnvironment>, Error> {
    let mut env = CommandEnvironment::new();
    {
        let registry = &mut env.command_registry;
        register_command!(registry, fn attach(state, part: Value (node: local, ext: "glb")) -> result)?;
        register_command!(registry, fn material(state, m: Value (node: local, ext: "mat")) -> result)?;
    }
    env.with_async_store(Box::new(store));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider::new()));
    Ok(env.to_ref())
}

fn text_metadata(key: &Key, title: &str) -> Metadata {
    let mut record = MetadataRecord::new();
    record
        .with_key(key.clone())
        .with_type_identifier("Text".to_owned())
        .with_title(title.to_owned())
        .with_status(Status::Source);
    // The `.glb` and `.mat` files here are text: tell the store how to read them.
    record.data_format = Some("txt".to_owned());
    Metadata::MetadataRecord(record)
}

async fn seed(store: &AsyncMemoryStore, path: &str, text: &str) -> Result<(), Error> {
    let key = parse_key(path)?;
    store
        .set(&key, text.as_bytes(), &text_metadata(&key, "stored title"))
        .await
}

async fn seed_yaml(store: &AsyncMemoryStore, path: &str, yaml: &str) -> Result<(), Error> {
    store
        .set(&parse_key(path)?, yaml.as_bytes(), &Metadata::new())
        .await
}

/// The car of the design's Example 1, in `folder`.
async fn seed_car(store: &AsyncMemoryStore, folder: &str, tree: &str) -> Result<(), Error> {
    seed(store, &format!("{folder}/body.glb"), "body").await?;
    seed(store, &format!("{folder}/tyre.glb"), "tyre").await?;
    seed(store, &format!("{folder}/rubber.mat"), "rubber").await?;
    seed_yaml(
        store,
        &format!("{folder}/recipes.yaml"),
        &format!(
            r#"{tree}recipes:
  - {{query: body.glb, title: Body, description: The car body}}
  - {{query: tyre.glb, title: Tyre}}
  - {{query: rubber.mat, title: Rubber}}
  - {{title: Wheel, query: "-R/./tyre.glb/-/material-rubber/wheel.glb"}}
  - {{title: Car, query: "-R/./body.glb/-/attach-wheel/car.glb"}}
"#
        ),
    )
    .await
}

async fn text(envref: &EnvRef<CommandEnvironment>, query: &str) -> Result<String, Error> {
    let asset = envref.evaluate(parse_query(query)?).await?;
    asset.get().await?.try_into_string()
}

/// AC-27: a source recipe serves the stored data, with the recipe's title and description and
/// the status `Source` in the asset metadata; the store's record is not changed.
#[tokio::test]
async fn source_declaration_serves_stored_data() -> Result<(), Box<dyn std::error::Error>> {
    let store = AsyncMemoryStore::new(&Key::new());
    seed_car(&store, "scenes/car", "").await?;
    let envref = make_env(store)?;
    let asset = envref.evaluate(parse_query("-R/scenes/car/body.glb")?).await?;
    let state = asset.get().await?;
    assert_eq!(state.try_into_string()?, "body");
    assert_eq!(state.metadata.status(), Status::Source);
    assert_eq!(state.metadata.title(), "Body");
    assert_eq!(state.metadata.description(), "The car body");
    let stored = envref
        .get_async_store()
        .get_metadata(&parse_key("scenes/car/body.glb")?)
        .await?;
    assert_eq!(stored.title(), "stored title");
    Ok(())
}

/// AC-27: a source recipe with no stored data fails with `KeyNotFound` and stores nothing.
#[tokio::test]
async fn source_declaration_without_data_is_key_not_found() -> Result<(), Box<dyn std::error::Error>>
{
    let store = AsyncMemoryStore::new(&Key::new());
    seed_yaml(
        &store,
        "scenes/car/recipes.yaml",
        "recipes:\n  - {query: body.glb, title: Body}\n",
    )
    .await?;
    let envref = make_env(store)?;
    let key = parse_key("scenes/car/body.glb")?;
    let error = text(&envref, "-R/scenes/car/body.glb")
        .await
        .expect_err("no stored data");
    assert_eq!(error.error_type, ErrorType::KeyNotFound, "{error}");
    let info = envref.get_asset_manager().get_asset_info(&key).await?;
    assert_eq!(info.status, Status::Error);
    assert!(
        !envref.get_async_store().contains(&key).await?,
        "a failed source stores nothing"
    );
    Ok(())
}

/// AC-2, AC-26: without a `tree:` section the car evaluates through its recipes and source
/// declarations; with an open one, identically.
#[tokio::test]
async fn folder_without_tree_unchanged() -> Result<(), Box<dyn std::error::Error>> {
    for tree in ["", "tree: {result: car.glb}\n"] {
        let store = AsyncMemoryStore::new(&Key::new());
        seed_car(&store, "scenes/car", tree).await?;
        let envref = make_env(store)?;
        assert_eq!(text(&envref, "-R/scenes/car/car.glb").await?, "body+tyre~rubber", "{tree}");
    }
    Ok(())
}

/// AC-2: a `tree:` section is kept when the list is read and written back.
#[tokio::test]
async fn tree_declaration_round_trips() -> Result<(), Box<dyn std::error::Error>> {
    let yaml = "tree:\n  roots:\n  - car.glb\n  shape: tree\n  dependencies: strict\nrecipes:\n- query: body.glb\n  title: Body\n";
    let list: RecipeList = serde_yaml::from_str(yaml)?;
    let written = serde_yaml::to_string(&list)?;
    // The tree section is written back as it was read (recipes add their own default fields).
    assert!(written.starts_with(&yaml[..yaml.find("recipes:").unwrap_or(0)]), "{written}");
    let back: RecipeList = serde_yaml::from_str(&written)?;
    assert_eq!(back.tree, list.tree);
    Ok(())
}

const STRICT: &str = "tree: {dependencies: strict, result: car.glb}\n";

/// AC-24: one bad recipe in a strict folder refuses every recipe of the folder, and the file is
/// not cached: a fix is seen at once.
#[tokio::test]
async fn strict_folder_fails_as_a_whole() -> Result<(), Box<dyn std::error::Error>> {
    let store = AsyncMemoryStore::new(&Key::new());
    seed_car(&store, "scenes/car", STRICT).await?;
    seed(&store, "data/paint.mat", "paint").await?;
    let bad = "tree: {dependencies: strict}\nrecipes:\n  - {query: body.glb}\n  - {query: tyre.glb}\n  - {query: \"-R/./body.glb/-/attach-tyre/ok.glb\"}\n  - {query: \"-R/./body.glb/-/material-~X~-R/data/paint.mat~E/bad.glb\"}\n";
    seed_yaml(&store, "scenes/car/recipes.yaml", bad).await?;
    let envref = make_env(store)?;
    let error = text(&envref, "-R/scenes/car/ok.glb")
        .await
        .expect_err("the whole folder is refused");
    assert!(error.message.contains("bad.glb"), "{error}");
    assert!(error.message.contains("'m'"), "{error}");
    let fixed = bad.replace("material-~X~-R/data/paint.mat~E", "attach-tyre");
    envref
        .get_async_store()
        .set(
            &parse_key("scenes/car/recipes.yaml")?,
            fixed.as_bytes(),
            &Metadata::new(),
        )
        .await?;
    assert_eq!(text(&envref, "-R/scenes/car/ok.glb").await?, "body+tyre");
    Ok(())
}

/// AC-10: a strict tree copied to another folder evaluates there to the same value.
#[tokio::test]
async fn strict_tree_relocates() -> Result<(), Box<dyn std::error::Error>> {
    let store = AsyncMemoryStore::new(&Key::new());
    seed_car(&store, "scenes/car", STRICT).await?;
    seed_car(&store, "garage/truck", STRICT).await?;
    let envref = make_env(store)?;
    let car = text(&envref, "-R/scenes/car/car.glb").await?;
    assert_eq!(car, "body+tyre~rubber");
    assert_eq!(text(&envref, "-R/garage/truck/car.glb").await?, car);
    Ok(())
}

/// AC-19: the declared result is the key the tree is evaluated as.
#[tokio::test]
async fn result_key_evaluates_the_tree() -> Result<(), Box<dyn std::error::Error>> {
    let store = AsyncMemoryStore::new(&Key::new());
    seed_car(&store, "scenes/car", STRICT).await?;
    let envref = make_env(store)?;
    let folder = parse_key("scenes/car")?;
    let list = DefaultRecipeProvider::new()
        .get_recipes(&folder, envref.clone())
        .await?;
    let result = list
        .tree
        .as_ref()
        .and_then(|t| t.result.clone())
        .ok_or("no result")?;
    let key = folder.join(result);
    assert_eq!(
        text(&envref, &format!("-R/{}", key.encode())).await?,
        "body+tyre~rubber"
    );
    Ok(())
}
