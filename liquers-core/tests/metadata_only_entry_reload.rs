//! A stored entry with metadata and no data object — what `set_state` writes for a value with no
//! byte form — is re-derived from its recipe on the next request, on every store.
//!
//! Before the fix, `AssetData::try_fast_track` propagated the `KeyNotFound` a file store gives for
//! such an entry's data, so the request failed instead of recomputing
//! (`FAST-TRACK-FAILS-ON-METADATA-ONLY-FILE-STORE-ENTRY`). Design:
//! `specs/design/metadata-only-entry-reload/`.
//!
//! The memory store used to answer such an entry with empty bytes, which a text format
//! deserializes as an empty string, so the recipe never ran
//! (`MEMORY-STORE-METADATA-ONLY-ENTRY-READS-AS-EMPTY-BYTES`, design
//! `memory-store-metadata-only-entry`). It now reports `KeyNotFound` like the file store, and
//! `metadata_only_entry_on_memory_store_is_recomputed` pins it.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use liquers_core::{
    assets::AssetManager,
    command_metadata::CommandKey,
    context::{EnvRef, Environment, SimpleEnvironment},
    error::Error,
    metadata::{Metadata, MetadataRecord, Status},
    parse::parse_key,
    query::Key,
    recipes::{DefaultRecipeProvider, Recipe, RecipeList},
    store::{AsyncFileStore, AsyncMemoryStore, AsyncStore},
    value::Value,
};

/// A uniquely named temporary directory for one file store.
async fn temp_root(label: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let root = std::env::temp_dir().join(format!("lq-metadata-only-{label}-{nanos}"));
    tokio::fs::create_dir_all(&root).await.unwrap();
    root
}

/// The metadata `set_state` leaves for a value it could not serialize: a `Ready` text value whose
/// bytes were never written.
fn metadata_only_ready_text(key: &Key) -> Metadata {
    let mut record = MetadataRecord::new();
    record.with_key(key.clone());
    record.with_type_identifier("Text".to_owned());
    record.data_format = Some("txt".to_owned());
    record.with_status(Status::Ready);
    Metadata::MetadataRecord(record)
}

/// An environment over `store` with a root `recipes.yaml` declaring `x.txt` as `make_text`, and a
/// counter of how many times `make_text` ran.
async fn env_over(
    store: Box<dyn AsyncStore>,
) -> (EnvRef<SimpleEnvironment<Value>>, Arc<AtomicUsize>) {
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
    let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
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

/// Request `x.txt` and return its text.
async fn request_x(envref: &EnvRef<SimpleEnvironment<Value>>) -> Result<String, Error> {
    let key = parse_key("x.txt")?;
    let asset = envref.get_asset_manager().get(&key).await?;
    let state = asset.get().await?;
    state.try_into_string()
}

#[tokio::test]
async fn metadata_only_entry_on_file_store_is_recomputed() {
    let root = temp_root("file").await;
    let store = AsyncFileStore::new(root.to_string_lossy().as_ref(), &Key::new());
    let key = parse_key("x.txt").unwrap();
    store
        .set_metadata(&key, &metadata_only_ready_text(&key))
        .await
        .unwrap();
    assert!(store.contains(&key).await.unwrap(), "a metadata-only key is contained");

    let (envref, calls) = env_over(Box::new(store)).await;
    assert_eq!(request_x(&envref).await.unwrap(), "generated");
    assert_eq!(calls.load(Ordering::SeqCst), 1, "the recipe ran once");

    let _ = tokio::fs::remove_dir_all(&root).await;
}

#[tokio::test]
async fn metadata_only_entry_on_memory_store_is_recomputed() {
    let store = AsyncMemoryStore::new(&Key::new());
    let key = parse_key("x.txt").unwrap();
    store
        .set_metadata(&key, &metadata_only_ready_text(&key))
        .await
        .unwrap();
    assert!(store.contains(&key).await.unwrap(), "a metadata-only key is contained");

    let (envref, calls) = env_over(Box::new(store)).await;
    assert_eq!(request_x(&envref).await.unwrap(), "generated");
    assert_eq!(calls.load(Ordering::SeqCst), 1, "the recipe ran once");
}

/// Without a recipe there is nothing to re-derive the value from, so the key answers exactly as a
/// key the store does not hold: an `Error` state saying no recipe was found — not a failed `get`
/// and not a corrupted entry.
#[tokio::test]
async fn metadata_only_entry_without_recipe_answers_like_an_absent_key() {
    let root = temp_root("norecipe").await;
    let store = AsyncFileStore::new(root.to_string_lossy().as_ref(), &Key::new());
    let key = parse_key("y.txt").unwrap();
    store
        .set_metadata(&key, &metadata_only_ready_text(&key))
        .await
        .unwrap();
    let (envref, calls) = env_over(Box::new(store)).await;
    let absent = parse_key("z.txt").unwrap();

    for k in [&key, &absent] {
        let asset = envref.get_asset_manager().get(k).await.unwrap();
        let state = asset.get().await.unwrap();
        assert_eq!(state.metadata.status(), Status::Error, "{k}");
        assert!(
            state.metadata.message().contains("No recipe found"),
            "{k}: {}",
            state.metadata.message()
        );
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);

    let _ = tokio::fs::remove_dir_all(&root).await;
}
