//! Recipe results in mounts whose records are inferred (`specs/design/storage-none-computed-values/`).
//!
//! A `storage: none` mount keeps the data and never a record, so every record it returns is built
//! by `on_default` and flagged `inferred`. Under a recipe the asset manager reads such data as a file
//! with no metadata: the environment's `ExternalChangePolicy` decides whether it is the user's
//! (`Override`) or recomputed. A "restart" is a second environment over the same router store.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use liquers_core::{
    assets::{AssetManager, ExternalChangePolicy},
    command_metadata::CommandKey,
    context::{EnvRef, SimpleEnvironment},
    environment_builder::{AssetManagerOptions, EnvironmentBuilder, Queued},
    error::Error,
    metadata::{Metadata, MetadataRecord, Status},
    parse::parse_key,
    query::Key,
    recipes::DefaultRecipeProvider,
    store::realm::StoreConfigFunctions,
    store::AsyncStore,
    store_factory::{core_store_factory, StoreRouterBuilder},
    value::Value,
};

type TestEnv = SimpleEnvironment<Value>;
type TestResult = Result<(), Box<dyn std::error::Error>>;

const RECIPES: &[u8] = b"recipes:\n  - query: counted/out.txt\n";

/// Fail fast instead of hanging if a call deadlocks.
async fn within<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(std::time::Duration::from_secs(20), future)
        .await
        .expect("call did not finish within 20 s")
}

fn key(name: &str) -> Key {
    parse_key(name).expect("key")
}

/// A router with one memory mount at `data` and the given `metadata` section.
fn router(metadata: &str) -> Result<Arc<dyn AsyncStore>, Error> {
    let yaml = format!("stores:\n  - type: memory\n    prefix: data\n    metadata: {metadata}\n");
    let (cmr, functions) = StoreConfigFunctions::builtin();
    StoreRouterBuilder::from_yaml(&yaml, Box::new(core_store_factory()))?
        .with_store_config(cmr, functions)
        .build_store()
}

/// A `storage: none` mount holding `data/recipes.yaml`.
async fn storage_none_store() -> Result<Arc<dyn AsyncStore>, Error> {
    let store = router("{ storage: none }")?;
    store
        .set(&key("data/recipes.yaml"), RECIPES, &Metadata::new())
        .await?;
    Ok(store)
}

fn env_with(
    store: Arc<dyn AsyncStore>,
    policy: ExternalChangePolicy,
    calls: Arc<AtomicUsize>,
) -> Result<EnvRef<TestEnv>, Error> {
    let mut builder = EnvironmentBuilder::<Value, (), Queued>::new()
        .with_asset_manager_options(AssetManagerOptions::default().with_external_change(policy))
        .with_async_store(store)
        .with_recipe_provider(Arc::new(DefaultRecipeProvider::new()));
    builder
        .command_registry
        .register_command(
            CommandKey::new_name("counted"),
            move |_state, _args, _ctx| -> Result<Value, Error> {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(Value::from("counted"))
            },
        )
        .expect("register counted");
    builder.build()
}

async fn read(envref: &EnvRef<TestEnv>, name: &str) -> Result<(String, Status), Box<dyn std::error::Error>> {
    let asset = within(envref.get_asset_manager().get(&key(name))).await?;
    let text = within(asset.get()).await?.try_into_string()?;
    Ok((text, asset.status().await))
}

/// Wait until the store holds data at `key`: a queued manager writes after `get()` returns.
async fn wait_until_written(store: &Arc<dyn AsyncStore>, key: &Key) -> Result<(), Error> {
    for _ in 0..500 {
        if store.contains(key).await? {
            return Ok(());
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    Err(Error::general_error(format!("{key} was never written")))
}

/// Process one evaluates `out.txt`, which writes its data and no record; process two reads it.
async fn computed_then_restarted(
    policy: ExternalChangePolicy,
) -> Result<(EnvRef<TestEnv>, Arc<AtomicUsize>), Box<dyn std::error::Error>> {
    let store = storage_none_store().await?;
    let calls = Arc::new(AtomicUsize::new(0));
    let first = env_with(store.clone(), policy, calls.clone())?;
    assert_eq!(read(&first, "data/out.txt").await?.0, "counted");
    wait_until_written(&store, &key("data/out.txt")).await?;
    assert!(
        store.get_metadata(&key("data/out.txt")).await?.is_inferred(),
        "the mount stores no record, so the one it returns is inferred"
    );
    let second = env_with(store, policy, calls.clone())?;
    Ok((second, calls))
}

/// AC-7, `user_input`: the bytes are kept as the user's, so the recipe does not run again.
#[tokio::test]
async fn inferred_result_under_user_input_policy_becomes_override() -> TestResult {
    let (envref, calls) = computed_then_restarted(ExternalChangePolicy::UserInput).await?;
    let (text, status) = read(&envref, "data/out.txt").await?;
    assert_eq!(text, "counted");
    assert_eq!(status, Status::Override);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    Ok(())
}

/// AC-7, `corrupted`: the stored copy is deleted and the recipe recomputes it.
#[tokio::test]
async fn inferred_result_under_corrupted_policy_is_recomputed() -> TestResult {
    let (envref, calls) = computed_then_restarted(ExternalChangePolicy::Corrupted).await?;
    let (text, status) = read(&envref, "data/out.txt").await?;
    assert_eq!(text, "counted");
    assert_eq!(status, Status::Ready);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    Ok(())
}

/// AC-7: with no recipe, an inferred record stands; the file is a `Source`.
#[tokio::test]
async fn inferred_record_without_recipe_stays_source() -> TestResult {
    let store = storage_none_store().await?;
    store
        .set(&key("data/notes.txt"), b"my notes", &Metadata::new())
        .await?;
    let envref = env_with(
        store,
        ExternalChangePolicy::Corrupted,
        Arc::new(AtomicUsize::new(0)),
    )?;
    let (text, status) = read(&envref, "data/notes.txt").await?;
    assert_eq!(text, "my notes");
    assert_eq!(status, Status::Source);
    Ok(())
}

/// AC-8: removing a result whose only record is inferred drops it back to its recipe: the data
/// is deleted, nothing is written in its place.
#[tokio::test]
async fn remove_drops_an_inferred_result_back_to_its_recipe() -> TestResult {
    let (envref, _calls) = computed_then_restarted(ExternalChangePolicy::UserInput).await?;
    let am = envref.get_asset_manager();
    within(am.remove(&key("data/out.txt"))).await?;
    let store = envref.get_async_store();
    assert!(
        !store.contains(&key("data/out.txt")).await?,
        "the result's data is deleted"
    );
    let info = within(am.get_asset_info(&key("data/out.txt"))).await?;
    assert_eq!(info.status, Status::Recipe);
    Ok(())
}

/// AC-4, AC-5: `infer-Source` in `on_write` keeps a user file's record out of the store, while a
/// recipe result's record is stored as usual.
#[tokio::test]
async fn sparse_mount_stores_only_computed_records() -> TestResult {
    let store = router(
        "{ storage: per_entry-root, on_write: defaults/media_type-fill/file_facts/infer-Source }",
    )?;
    let mut source = MetadataRecord::new();
    source.with_status(Status::Source);
    store
        .set(&key("data/recipes.yaml"), RECIPES, &Metadata::from(source))
        .await?;
    let envref = env_with(
        store.clone(),
        ExternalChangePolicy::UserInput,
        Arc::new(AtomicUsize::new(0)),
    )?;
    assert_eq!(read(&envref, "data/out.txt").await?.0, "counted");
    let out = key("data/out.txt");
    for _ in 0..500 {
        if store.get_metadata(&out).await?.status() == Status::Ready {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let out_record = store.get_metadata(&out).await?;
    assert_eq!(out_record.status(), Status::Ready);
    assert!(!out_record.is_inferred(), "a computed record is stored");
    assert!(
        store
            .get_metadata(&key("data/recipes.yaml"))
            .await?
            .is_inferred(),
        "a Source record is not stored"
    );
    Ok(())
}

/// AC-9: `persist_defaults` is no longer a metadata field.
#[test]
fn persist_defaults_is_refused_at_build() {
    let error = match router("{ storage: per_entry, persist_defaults: true }") {
        Ok(store) => panic!("expected an error, built {}", store.store_name()),
        Err(error) => error.message.clone(),
    };
    assert!(error.contains("persist_defaults"), "{error}");
}
