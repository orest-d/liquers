//! Bytes stored with a record that carries no status are reused, not refused.
//!
//! `AsyncMemoryStore::set` and `AsyncFileStore::set` keep the caller's record as given, and a
//! page-implemented store may answer with partial metadata, so a store can return data whose
//! status is `None`. That is a file with no metadata (`ASSETS.md`, "Content changed outside
//! Liquers"): it reads as `Source` without a recipe and as `Ready` with one, with a warning in the
//! asset's log. Before the fix the fast track refused it and the key fell through to the recipe
//! provider (`JS-STORE-RESOURCE-NOT-FOUND-WITHOUT-A-RECIPE`). Design:
//! `specs/design/js-store-page-metadata-status/`.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use liquers_core::{
    assets::{AssetManager, AssetRef},
    command_metadata::CommandKey,
    context::{EnvRef, Environment, SimpleEnvironment},
    metadata::{Metadata, MetadataRecord, Status},
    parse::parse_key,
    query::Key,
    recipes::{DefaultRecipeProvider, Recipe, RecipeList},
    store::{AsyncFileStore, AsyncMemoryStore, AsyncStore},
    value::Value,
};

type Env = SimpleEnvironment<Value>;

/// A uniquely named temporary directory for one file store.
async fn temp_root(label: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let root = std::env::temp_dir().join(format!("lq-status-none-{label}-{nanos}"));
    tokio::fs::create_dir_all(&root).await.unwrap();
    root
}

/// A record with no status and no version, as a caller writing raw bytes leaves it.
fn no_status() -> Metadata {
    let mut record = MetadataRecord::new();
    record.with_media_type("text/plain".to_string());
    Metadata::MetadataRecord(record)
}

/// Writes `text` under `key` with the given record.
async fn put(store: &dyn AsyncStore, key: &str, text: &str, metadata: &Metadata) {
    store
        .set(&parse_key(key).unwrap(), text.as_bytes(), metadata)
        .await
        .unwrap();
}

/// An environment over `store` with a root `recipes.yaml` declaring `x.txt` as `make_text`, and a
/// counter of how many times `make_text` ran.
async fn env_over(store: Box<dyn AsyncStore>) -> (EnvRef<Env>, Arc<AtomicUsize>) {
    let mut recipes = RecipeList::new();
    recipes.add_recipe(
        Recipe::new("make_text/x.txt".to_string(), "X".to_string(), String::new()).unwrap(),
    );
    store
        .set(
            &parse_key("recipes.yaml").unwrap(),
            serde_yaml::to_string(&recipes).unwrap().as_bytes(),
            &Metadata::new(),
        )
        .await
        .unwrap();

    let calls = Arc::new(AtomicUsize::new(0));
    let counter = calls.clone();
    let mut env: Env = SimpleEnvironment::new();
    env.command_registry
        .register_command(CommandKey::new_name("make_text"), move |_, _, _| {
            counter.fetch_add(1, Ordering::SeqCst);
            Ok(Value::from("generated"))
        })
        .unwrap();
    env.with_async_store(store);
    env.with_recipe_provider(Box::new(DefaultRecipeProvider::new()));
    (env.to_ref(), calls)
}

/// Requests `key` and returns the asset with its text.
async fn request(envref: &EnvRef<Env>, key: &str) -> (AssetRef<Env>, String) {
    let asset = envref
        .get_asset_manager()
        .get(&parse_key(key).unwrap())
        .await
        .unwrap();
    let text = asset.get().await.unwrap().try_into_string().unwrap();
    (asset, text)
}

/// True when the asset's log holds the warning for a mapped status.
async fn warned(asset: &AssetRef<Env>, key: &str, status: &str) -> bool {
    let expected = format!("no status was stored for {key}; read as {status}");
    let metadata = asset.get_metadata().await.unwrap();
    match metadata {
        Metadata::MetadataRecord(record) => record.log.iter().any(|e| e.message == expected),
        Metadata::LegacyMetadata(_) => false,
    }
}

#[tokio::test]
async fn none01_memory_store_without_recipe_reads_as_source() {
    let store = AsyncMemoryStore::new(&Key::new());
    put(&store, "y.txt", "hello", &no_status()).await;
    let (envref, calls) = env_over(Box::new(store)).await;

    let (asset, text) = request(&envref, "y.txt").await;
    assert_eq!(text, "hello");
    assert_eq!(asset.status().await, Status::Source);
    assert!(warned(&asset, "y.txt", "Source").await, "the mapping is logged as a warning");
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

/// With a recipe the bytes read as `Ready`. They carry no version, so the external-change check
/// that follows finds them changed outside Liquers and, under the default `user_input` policy,
/// keeps them as `Override` — the documented outcome for a file with no metadata under a recipe.
#[tokio::test]
async fn none02_memory_store_with_recipe_reads_as_ready() {
    let store = AsyncMemoryStore::new(&Key::new());
    put(&store, "x.txt", "stored", &no_status()).await;
    let (envref, calls) = env_over(Box::new(store)).await;

    let (asset, text) = request(&envref, "x.txt").await;
    assert_eq!(text, "stored", "the stored bytes are reused");
    assert_eq!(asset.status().await, Status::Override);
    assert!(warned(&asset, "x.txt", "Ready").await);
    assert_eq!(calls.load(Ordering::SeqCst), 0, "the recipe did not run");
}

#[tokio::test]
async fn none03_file_store_without_recipe_reads_as_source() {
    let root = temp_root("file").await;
    let store = AsyncFileStore::new(root.to_string_lossy().as_ref(), &Key::new());
    put(&store, "y.txt", "hello", &no_status()).await;
    let (envref, _calls) = env_over(Box::new(store)).await;

    let (asset, text) = request(&envref, "y.txt").await;
    assert_eq!(text, "hello");
    assert_eq!(asset.status().await, Status::Source);

    let _ = tokio::fs::remove_dir_all(&root).await;
}

#[tokio::test]
async fn none04_declared_status_is_kept() {
    let store = AsyncMemoryStore::new(&Key::new());
    let mut record = MetadataRecord::new();
    record.with_status(Status::Error);
    put(&store, "y.txt", "hello", &Metadata::MetadataRecord(record)).await;
    let (envref, _calls) = env_over(Box::new(store)).await;

    let asset = envref
        .get_asset_manager()
        .get(&parse_key("y.txt").unwrap())
        .await
        .unwrap();
    let state = asset.get().await.unwrap();
    assert!(state.is_error().unwrap(), "a stored Error is not reused");
    assert!(
        state.metadata.message().contains("No recipe"),
        "falls through to the recipe provider: {}",
        state.metadata.message()
    );
}

#[tokio::test]
async fn none05_get_any_status_reads_as_source() {
    let store = AsyncMemoryStore::new(&Key::new());
    put(&store, "y.txt", "hello", &no_status()).await;
    let (envref, _calls) = env_over(Box::new(store)).await;

    let manager = envref.get_asset_manager();
    let key = parse_key("y.txt").unwrap();
    let state = manager
        .get_any_status(&key)
        .await
        .unwrap()
        .expect("bytes with no status are a stored value");
    assert_eq!(state.try_into_string().unwrap(), "hello");
    assert_eq!(state.metadata.status(), Status::Source);

    let (bytes, metadata) = manager
        .get_binary_any_status(&key)
        .await
        .unwrap()
        .expect("and so are their bytes");
    assert_eq!(bytes.as_slice(), b"hello");
    assert_eq!(metadata.status(), Status::Source);
}
