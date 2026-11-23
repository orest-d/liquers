//! Self-contained recipe trees (design `recipe-tree`): source declarations, the tree check run by
//! the recipe provider, evaluation and relocation of a strict tree, and the tree editor.

use liquers_core::{
    assets::AssetManager,
    context::{Context, EnvRef, Environment, ImmediateEnvironment},
    error::{Error, ErrorType},
    metadata::{Metadata, MetadataRecord, Status},
    parse::{parse_key, parse_query},
    query::Key,
    recipe_tree::{DataRename, TreeEditor, TreeFinding},
    recipes::{DefaultRecipeProvider, Recipe, RecipeList},
    state::State,
    store::{AsyncMemoryStore, AsyncStore},
    value::{Value, ValueInterface},
};
use liquers_macro::register_command;
use std::sync::Arc;

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
    let editor = editor_for(&envref, &folder).await?;
    let key = editor.result_key(&folder).ok_or("no result")?;
    assert_eq!(key.encode(), "scenes/car/car.glb");
    assert_eq!(
        text(&envref, &format!("-R/{}", key.encode())).await?,
        "body+tyre~rubber"
    );
    Ok(())
}

/// A tree editor over the recipes and the store listing of `folder`.
async fn editor_for(
    envref: &EnvRef<CommandEnvironment>,
    folder: &Key,
) -> Result<TreeEditor, Error> {
    let list = DefaultRecipeProvider::new()
        .get_recipes(folder, envref.clone())
        .await?;
    let listing = envref.get_async_store().listdir(folder).await?;
    TreeEditor::new(
        list,
        Some(listing),
        Arc::new(envref.get_command_metadata_registry().clone()),
    )
}

async fn car_editor() -> Result<(EnvRef<CommandEnvironment>, Key, TreeEditor), Error> {
    let store = AsyncMemoryStore::new(&Key::new());
    seed_car(&store, "scenes/car", STRICT).await?;
    let envref = make_env(store)?;
    let folder = parse_key("scenes/car")?;
    let editor = editor_for(&envref, &folder).await?;
    Ok((envref, folder, editor))
}

fn query_of(editor: &TreeEditor, title: &str) -> String {
    editor
        .recipes()
        .recipes
        .iter()
        .find(|r| r.title == title)
        .map(|r| r.query.clone())
        .unwrap_or_default()
}

/// AC-1, AC-25: a rename rewrites the Node name, the renamed recipe's filename and the result,
/// and the tree evaluates to the same value after saving.
#[tokio::test]
async fn rename_rewrites_every_reference_and_keeps_value() -> Result<(), Box<dyn std::error::Error>>
{
    let (envref, folder, mut editor) = car_editor().await?;
    editor.rename("wheel", "front_wheel")?;
    assert_eq!(query_of(&editor, "Car"), "-R/./body.glb/-/attach-front_wheel/car.glb");
    assert_eq!(
        query_of(&editor, "Wheel"),
        "-R/./tyre.glb/-/material-rubber/front_wheel.glb"
    );
    assert!(editor.report().errors.is_empty(), "{:?}", editor.report().errors);
    assert!(editor.pending_data_renames().is_empty(), "a computed node has no stored file");
    editor.rename("car", "auto")?;
    assert_eq!(
        editor.recipes().tree.as_ref().and_then(|t| t.result.clone()).as_deref(),
        Some("auto.glb")
    );
    editor.save(&folder, envref.clone()).await?;
    assert_eq!(text(&envref, "-R/scenes/car/auto.glb").await?, "body+tyre~rubber");
    Ok(())
}

/// AC-1: renaming a source renames its stored file on save.
#[tokio::test]
async fn rename_renames_stored_file_on_save() -> Result<(), Box<dyn std::error::Error>> {
    let (envref, folder, mut editor) = car_editor().await?;
    editor.rename("tyre", "tire")?;
    assert_eq!(
        editor.pending_data_renames(),
        &[DataRename {
            from: "tyre.glb".into(),
            to: "tire.glb".into()
        }]
    );
    assert_eq!(query_of(&editor, "Wheel"), "-R/./tire.glb/-/material-rubber/wheel.glb");
    assert_eq!(query_of(&editor, "Tyre"), "tire.glb");
    editor.rename("rubber", "rubber_2")?;
    assert_eq!(query_of(&editor, "Wheel"), "-R/./tire.glb/-/material-rubber_2/wheel.glb");
    editor.save(&folder, envref.clone()).await?;
    let store = envref.get_async_store();
    assert!(store.contains(&parse_key("scenes/car/tire.glb")?).await?);
    assert!(!store.contains(&parse_key("scenes/car/tyre.glb")?).await?);
    assert_eq!(text(&envref, "-R/scenes/car/car.glb").await?, "body+tyre~rubber");
    Ok(())
}

/// AC-11: a child is added and named by its parent; a used name is refused.
#[tokio::test]
async fn add_child_refuses_used_name() -> Result<(), Box<dyn std::error::Error>> {
    let (_, _, mut editor) = car_editor().await?;
    let used = Recipe::new("-R/./tyre.glb/-/wheel.mat".into(), "W".into(), String::new())?;
    let error = editor
        .add_child("car", used, 0, "part")
        .expect_err("name 'wheel' is used");
    assert!(error.message.contains("wheel"), "{error}");
    let spare = Recipe::new(
        "-R/./tyre.glb/-/material-rubber/spare.glb".into(),
        "Spare".into(),
        String::new(),
    )?;
    editor.add_child("car", spare, 0, "part")?;
    assert_eq!(query_of(&editor, "Car"), "-R/./body.glb/-/attach-spare/car.glb");
    assert!(editor.report().errors.is_empty(), "{:?}", editor.report().errors);
    assert!(editor
        .report()
        .warnings
        .iter()
        .any(|w| matches!(w, TreeFinding::Orphan { node } if node == "wheel.glb")));
    Ok(())
}

/// AC-12: a referenced node is not deleted.
#[tokio::test]
async fn delete_refused_while_referenced() -> Result<(), Box<dyn std::error::Error>> {
    let (_, _, mut editor) = car_editor().await?;
    for cascade in [false, true] {
        let error = editor.delete("wheel", cascade).expect_err("car uses it");
        assert!(error.message.contains("car.glb"), "{error}");
    }
    assert_eq!(editor.recipes().recipes.len(), 5);
    Ok(())
}

/// AC-12: a cascade deletes what only the node reaches; stored data stays.
#[tokio::test]
async fn delete_cascades() -> Result<(), Box<dyn std::error::Error>> {
    let (envref, folder, mut editor) = car_editor().await?;
    editor.delete("car", true)?;
    assert!(editor.recipes().recipes.is_empty(), "{:?}", editor.recipes().recipes);
    assert_eq!(editor.recipes().tree.as_ref().and_then(|t| t.result.clone()), None);
    editor.save(&folder, envref.clone()).await?;
    assert!(envref
        .get_async_store()
        .contains(&parse_key("scenes/car/body.glb")?)
        .await?);
    Ok(())
}

const MOVABLE: &str = r#"recipes:
  - {query: s.glb}
  - {query: d.glb}
  - {query: "-R/./s.glb/-/attach-d/b.glb"}
  - {query: "-R/./b.glb/-/y.glb"}
  - {query: "-R/./s.glb/-/attach-y/z.glb"}
"#;

async fn movable_editor() -> Result<TreeEditor, Error> {
    let envref = make_env(AsyncMemoryStore::new(&Key::new()))?;
    let list: RecipeList = serde_yaml::from_str(MOVABLE)
        .map_err(|e| Error::general_error(e.to_string()))?;
    TreeEditor::new(
        list,
        Some(vec!["s.glb".into(), "d.glb".into()]),
        Arc::new(envref.get_command_metadata_registry().clone()),
    )
}

/// AC-13: a reference moves between parents; a move that breaks the check is refused.
#[tokio::test]
async fn move_reference_refused_when_shape_breaks() -> Result<(), Box<dyn std::error::Error>> {
    let mut editor = movable_editor().await?;
    let error = editor
        .move_reference("y", "z", "b", 0, "part")
        .expect_err("b -> y -> b is a cycle");
    assert!(error.message.contains("cycle"), "{error}");
    editor.move_reference("d", "b", "z", 0, "part")?;
    let queries: Vec<&str> = editor.recipes().recipes.iter().map(|r| r.query.as_str()).collect();
    assert!(queries.contains(&"-R/./s.glb/-/attach/b.glb"), "{queries:?}");
    assert!(queries.contains(&"-R/./s.glb/-/attach-d/z.glb"), "{queries:?}");
    let error = editor
        .move_reference("s", "b", "z", 0, "part")
        .expect_err("a resource segment cannot be moved");
    assert!(error.message.contains("cannot be moved"), "{error}");
    Ok(())
}

/// AC-14: a refused edit leaves the recipes as they were.
#[tokio::test]
async fn refused_edit_changes_nothing() -> Result<(), Box<dyn std::error::Error>> {
    let mut editor = movable_editor().await?;
    let before = serde_yaml::to_string(editor.recipes())?;
    assert!(editor.move_reference("y", "z", "b", 0, "part").is_err());
    assert!(editor.rename("y", "z").is_err());
    assert!(editor.delete("b", false).is_err());
    assert_eq!(serde_yaml::to_string(editor.recipes())?, before);
    assert!(editor.pending_data_renames().is_empty());
    Ok(())
}

/// AC-15: a reference that cannot be read refuses the rename.
#[tokio::test]
async fn rename_refuses_unrewritable_reference() -> Result<(), Box<dyn std::error::Error>> {
    let (_, _, editor) = car_editor().await?;
    let mut list = editor.recipes().clone();
    list.tree = None;
    list.recipes.push(Recipe {
        query: "-R/./wheel.glb/-/attach-~X~-R/./x".into(),
        ..Recipe::default()
    });
    let envref = make_env(AsyncMemoryStore::new(&Key::new()))?;
    let mut editor = TreeEditor::new(
        list,
        None,
        Arc::new(envref.get_command_metadata_registry().clone()),
    )?;
    let before = serde_yaml::to_string(editor.recipes())?;
    let error = editor.rename("wheel", "w").expect_err("unparseable reference");
    assert!(error.message.contains("cannot be rewritten"), "{error}");
    assert_eq!(serde_yaml::to_string(editor.recipes())?, before);
    Ok(())
}
