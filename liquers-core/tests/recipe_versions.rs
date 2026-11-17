//! Editing a recipe expires what was computed from it
//! (`specs/design/recipe-dependency-versions/`, Phase 3).
//!
//! A keyed asset records a dependency on its own recipe (`-R-recipe/<key>`), whose version is a
//! hash of the recipe. Writing `recipes.yaml` through the asset manager re-resolves the recipes of
//! that folder, and a changed one expires the asset and everything that read it. Every scenario
//! runs on both built-in managers: `Queued` (`DefaultAssetManager`) and `Inline`
//! (`ImmediateAssetManager`).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use liquers_core::{
    assets::AssetManager,
    command_metadata::CommandKey,
    commands::CommandRegistry,
    context::{EnvRef, Environment},
    environment_builder::{
        AssetManagerKind, AssetManagerOptions, DependencyAuditPolicy, EnvironmentBuilder, Inline,
        Queued,
    },
    error::Error,
    metadata::{DependencyKey, ExpiryCause, ExpiryReason, Metadata, MetadataRecord, Status},
    parse::parse_key,
    query::Key,
    recipes::DefaultRecipeProvider,
    state::State,
    store::{AsyncMemoryStore, AsyncStore},
    value::Value,
};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const UPPER: &str = "recipes:\n  - query: make_text/upper/r.txt\n";
const LOWER: &str = "recipes:\n  - query: make_text/lower/r.txt\n";
const UPPER_PLUS_READER: &str =
    "recipes:\n  - query: make_text/upper/r.txt\n  - query: -R/data/r.txt/-/upper/s.txt\n";
const LOWER_PLUS_READER: &str =
    "recipes:\n  - query: make_text/lower/r.txt\n  - query: -R/data/r.txt/-/upper/s.txt\n";
const UPPER_PLUS_OTHER: &str =
    "recipes:\n  - query: make_text/upper/r.txt\n  - query: make_text/other.txt\n";
const ONLY_OTHER: &str = "recipes:\n  - query: make_text/other.txt\n";

/// Fail fast instead of hanging if a call deadlocks.
async fn within<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(std::time::Duration::from_secs(20), future)
        .await
        .expect("call did not finish within 20 s")
}

fn register<E: Environment<Value = Value>>(
    cr: &mut CommandRegistry<E>,
    calls: Arc<AtomicUsize>,
) -> Result<(), Error> {
    cr.register_command(CommandKey::new_name("make_text"), move |_, _, _| {
        calls.fetch_add(1, Ordering::SeqCst);
        Ok(Value::from("generated"))
    })?;
    cr.register_command(CommandKey::new_name("upper"), |state: &State<Value>, _, _| {
        Ok(Value::from(state.try_into_string()?.to_uppercase()))
    })?;
    cr.register_command(CommandKey::new_name("lower"), |state: &State<Value>, _, _| {
        Ok(Value::from(state.try_into_string()?.to_lowercase()))
    })?;
    Ok(())
}

fn text_metadata() -> MetadataRecord {
    MetadataRecord {
        type_identifier: "Text".to_string(),
        type_name: "text".to_string(),
        data_format: Some("txt".to_string()),
        ..Default::default()
    }
}

fn key(name: &str) -> Key {
    parse_key(name).expect("test key parses")
}

fn build<K: AssetManagerKind>(
    store: AsyncMemoryStore,
    policy: DependencyAuditPolicy,
    calls: Arc<AtomicUsize>,
) -> Result<EnvRef<liquers_core::context::GenericEnvironment<Value, (), K>>, Error> {
    let mut builder = EnvironmentBuilder::<Value, (), K>::new()
        .with_asset_manager_options(AssetManagerOptions::default().with_dependency_audit(policy))
        .with_async_store(Arc::new(store))
        .with_recipe_provider(Arc::new(DefaultRecipeProvider::new()));
    register(&mut builder.command_registry, calls)?;
    builder.build()
}

/// An environment whose `data/recipes.yaml` holds `recipes`, written through the asset manager
/// so it carries a version as a user's edit would.
async fn env<K: AssetManagerKind>(
    recipes: &str,
) -> Result<EnvRef<liquers_core::context::GenericEnvironment<Value, (), K>>, Error> {
    let envref = build::<K>(
        AsyncMemoryStore::new(&Key::new()),
        DependencyAuditPolicy::Explicit,
        Arc::new(AtomicUsize::new(0)),
    )?;
    write_recipes(&envref, recipes).await?;
    Ok(envref)
}

async fn write_recipes<E: Environment>(envref: &EnvRef<E>, recipes: &str) -> Result<(), Error> {
    within(envref.get_asset_manager().set_binary(
        &key("data/recipes.yaml"),
        recipes.as_bytes(),
        text_metadata(),
    ))
    .await
}

async fn get_text<E: Environment<Value = Value>>(
    envref: &EnvRef<E>,
    name: &str,
) -> Result<String, Error> {
    let asset = within(envref.get_asset_manager().get(&key(name))).await?;
    let state = within(asset.get()).await?;
    state.try_into_string()
}

/// Wait until the store holds `name` with `status` (a queued manager persists after `get`
/// returns).
async fn wait_stored<E: Environment>(
    envref: &EnvRef<E>,
    name: &str,
    status: Status,
) -> Result<Metadata, Error> {
    let store = envref.get_async_store();
    for _ in 0..400 {
        if store.contains(&key(name)).await? {
            let metadata = store.get_metadata(&key(name)).await?;
            if metadata.status() == status {
                return Ok(metadata);
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    Err(Error::general_error(format!(
        "{name} was not stored with status {status:?}"
    )))
}

async fn status_of<E: Environment>(envref: &EnvRef<E>, name: &str) -> Result<Status, Error> {
    Ok(within(envref.get_asset_manager().get_asset_info(&key(name)))
        .await?
        .status)
}

macro_rules! on_both_managers {
    ($scenario:ident, $queued:ident, $inline:ident) => {
        #[tokio::test]
        async fn $queued() -> TestResult {
            $scenario::<Queued>().await
        }
        #[tokio::test]
        async fn $inline() -> TestResult {
            $scenario::<Inline>().await
        }
    };
}

/// AC-1: the Problem Example. The edit expires `data/r.txt`, rooted at its recipe, in memory and
/// in the store; the next `get` evaluates the new recipe.
async fn editing_a_recipe_expires_the_key<K: AssetManagerKind>() -> TestResult {
    let envref = env::<K>(UPPER).await?;
    assert_eq!(get_text(&envref, "data/r.txt").await?, "GENERATED");
    wait_stored(&envref, "data/r.txt", Status::Ready).await?;

    write_recipes(&envref, LOWER).await?;

    let recipe_key = DependencyKey::from_recipe_key(&key("data/r.txt"));
    let stored = wait_stored(&envref, "data/r.txt", Status::Expired).await?;
    match stored.expiry_reason() {
        Some(ExpiryReason::Cascaded {
            cause: ExpiryCause::Updated { .. },
            root,
            ..
        }) => assert_eq!(root, recipe_key),
        other => panic!("unexpected expiry reason {other:?}"),
    }
    assert_eq!(status_of(&envref, "data/r.txt").await?, Status::Expired);
    assert_eq!(get_text(&envref, "data/r.txt").await?, "generated");
    Ok(())
}
on_both_managers!(
    editing_a_recipe_expires_the_key,
    editing_a_recipe_expires_the_key_queued,
    editing_a_recipe_expires_the_key_inline
);

/// AC-2, AC-4: a key that read the edited key through `-R/` expires in the same cascade.
async fn editing_a_recipe_expires_readers<K: AssetManagerKind>() -> TestResult {
    let envref = env::<K>(UPPER_PLUS_READER).await?;
    assert_eq!(get_text(&envref, "data/s.txt").await?, "GENERATED");
    wait_stored(&envref, "data/s.txt", Status::Ready).await?;

    write_recipes(&envref, LOWER_PLUS_READER).await?;

    wait_stored(&envref, "data/s.txt", Status::Expired).await?;
    assert_eq!(status_of(&envref, "data/s.txt").await?, Status::Expired);
    assert_eq!(get_text(&envref, "data/s.txt").await?, "GENERATED");
    assert_eq!(get_text(&envref, "data/r.txt").await?, "generated");
    Ok(())
}
on_both_managers!(
    editing_a_recipe_expires_readers,
    editing_a_recipe_expires_readers_queued,
    editing_a_recipe_expires_readers_inline
);

/// AC-3: rewriting the file with `r.txt` unchanged (and another recipe added) expires nothing.
async fn rewriting_an_unchanged_recipe_keeps_the_key<K: AssetManagerKind>() -> TestResult {
    let calls = Arc::new(AtomicUsize::new(0));
    let envref = build::<K>(
        AsyncMemoryStore::new(&Key::new()),
        DependencyAuditPolicy::Explicit,
        calls.clone(),
    )?;
    write_recipes(&envref, UPPER).await?;
    assert_eq!(get_text(&envref, "data/r.txt").await?, "GENERATED");
    wait_stored(&envref, "data/r.txt", Status::Ready).await?;

    write_recipes(&envref, UPPER_PLUS_OTHER).await?;

    assert_eq!(status_of(&envref, "data/r.txt").await?, Status::Ready);
    assert_eq!(get_text(&envref, "data/r.txt").await?, "GENERATED");
    assert_eq!(calls.load(Ordering::SeqCst), 1, "not recomputed");
    Ok(())
}
on_both_managers!(
    rewriting_an_unchanged_recipe_keeps_the_key,
    rewriting_an_unchanged_recipe_keeps_the_key_queued,
    rewriting_an_unchanged_recipe_keeps_the_key_inline
);

/// AC-6: removing the recipe expires the key, as an edit would.
async fn removed_recipe_expires_the_key<K: AssetManagerKind>() -> TestResult {
    let envref = env::<K>(UPPER).await?;
    assert_eq!(get_text(&envref, "data/r.txt").await?, "GENERATED");
    wait_stored(&envref, "data/r.txt", Status::Ready).await?;

    write_recipes(&envref, ONLY_OTHER).await?;

    wait_stored(&envref, "data/r.txt", Status::Expired).await?;
    assert_eq!(status_of(&envref, "data/r.txt").await?, Status::Expired);
    Ok(())
}
on_both_managers!(
    removed_recipe_expires_the_key,
    removed_recipe_expires_the_key_queued,
    removed_recipe_expires_the_key_inline
);

/// AC-5: the recipe edited while no process ran. Under `on_load`, the stored copy's own-recipe
/// record no longer matches, so it is not fast-tracked and the new recipe is evaluated.
async fn edited_recipe_is_not_fast_tracked_after_restart<K: AssetManagerKind>() -> TestResult {
    let names = ["data/recipes.yaml", "data/r.txt"];
    let first = env::<K>(UPPER).await?;
    assert_eq!(get_text(&first, "data/r.txt").await?, "GENERATED");
    wait_stored(&first, "data/r.txt", Status::Ready).await?;

    // The "restart": a fresh store holding what the first process stored, with the recipe file
    // edited behind its back (bytes only, sidecar kept, as a text editor would).
    let second_store = AsyncMemoryStore::new(&Key::new());
    let first_store = first.get_async_store();
    for name in names {
        let (bytes, metadata) = first_store.get(&key(name)).await?;
        second_store.set(&key(name), &bytes, &metadata).await?;
    }
    let metadata = second_store.get_metadata(&key("data/recipes.yaml")).await?;
    second_store
        .set(&key("data/recipes.yaml"), LOWER.as_bytes(), &metadata)
        .await?;

    let calls = Arc::new(AtomicUsize::new(0));
    let second = build::<K>(second_store, DependencyAuditPolicy::OnLoad, calls.clone())?;
    assert_eq!(get_text(&second, "data/r.txt").await?, "generated");
    assert_eq!(calls.load(Ordering::SeqCst), 1, "recomputed, not served as stored");
    Ok(())
}
on_both_managers!(
    edited_recipe_is_not_fast_tracked_after_restart,
    edited_recipe_is_not_fast_tracked_after_restart_queued,
    edited_recipe_is_not_fast_tracked_after_restart_inline
);

/// The control for AC-5: with the recipe unchanged, the stored copy is still served.
async fn unchanged_recipe_is_fast_tracked_after_restart<K: AssetManagerKind>() -> TestResult {
    let names = ["data/recipes.yaml", "data/r.txt"];
    let first = env::<K>(UPPER).await?;
    assert_eq!(get_text(&first, "data/r.txt").await?, "GENERATED");
    wait_stored(&first, "data/r.txt", Status::Ready).await?;

    let second_store = AsyncMemoryStore::new(&Key::new());
    let first_store = first.get_async_store();
    for name in names {
        let (bytes, metadata) = first_store.get(&key(name)).await?;
        second_store.set(&key(name), &bytes, &metadata).await?;
    }
    let calls = Arc::new(AtomicUsize::new(0));
    let second = build::<K>(second_store, DependencyAuditPolicy::OnLoad, calls.clone())?;
    assert_eq!(get_text(&second, "data/r.txt").await?, "GENERATED");
    assert_eq!(calls.load(Ordering::SeqCst), 0, "served as stored");
    Ok(())
}
on_both_managers!(
    unchanged_recipe_is_fast_tracked_after_restart,
    unchanged_recipe_is_fast_tracked_after_restart_queued,
    unchanged_recipe_is_fast_tracked_after_restart_inline
);
