//! `stored` / `cached` recipe flags, honoured by the asset manager
//! (`specs/design/record-streams/phase4-implementation.md`, Step 1.2).
//!
//! Every scenario is written once, generically over `E: Environment<Value = Value>`, and run
//! against both asset-manager kinds — `SimpleEnvironment<Value>` (queued,
//! `DefaultAssetManager`) and `ImmediateEnvironment<Value>` (inline, `ImmediateAssetManager`) —
//! since Step 1.2 changes both.
//!
//! **Counting commands without cross-talk.** `cargo test` runs the functions in this file in
//! parallel threads. A single `static AtomicUsize` shared by every test would race, so the
//! counting fixture command takes a `tag: String` argument and counts in a
//! `static Mutex<HashMap<String, usize>>` keyed by it; each test uses its own tag and reads only
//! its own entry.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use async_trait::async_trait;
use liquers_core::{
    assets::AssetManager,
    context::{Context, EnvRef, Environment, ImmediateEnvironment, SimpleEnvironment},
    error::Error,
    metadata::{Metadata, MetadataRecord, Status},
    parse::parse_key,
    plan::Plan,
    query::{Key, ResourceName},
    recipes::{AsyncRecipeProvider, Recipe},
    state::State,
    store::{AsyncMemoryStore, AsyncStore},
    value::Value,
};
use liquers_macro::register_command;

// ---------------------------------------------------------------------------
// The counting fixture command
// ---------------------------------------------------------------------------

fn counts_map() -> &'static Mutex<HashMap<String, usize>> {
    static COUNTS: OnceLock<Mutex<HashMap<String, usize>>> = OnceLock::new();
    COUNTS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn count_of(tag: &str) -> usize {
    counts_map()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(tag)
        .copied()
        .unwrap_or(0)
}

/// Increments this tag's counter and returns a value derived from it, so a test can also assert
/// on the value when that is useful.
fn counted(_state: &State<Value>, tag: String) -> Result<Value, Error> {
    let mut counts = counts_map()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *counts.entry(tag.clone()).or_insert(0) += 1;
    Ok(Value::from(format!("value-{tag}")))
}

// ---------------------------------------------------------------------------
// A recipe provider serving one fixed recipe per key
// ---------------------------------------------------------------------------

/// Serves a fixed `Key -> Recipe` map, modeled on `plan::tests::CountingRecipeProvider`.
#[derive(Clone)]
struct TaggedRecipeProvider {
    recipes: Arc<HashMap<Key, Recipe>>,
}

impl TaggedRecipeProvider {
    fn new(recipes: impl IntoIterator<Item = (Key, Recipe)>) -> Self {
        Self {
            recipes: Arc::new(recipes.into_iter().collect()),
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<E: Environment> AsyncRecipeProvider<E> for TaggedRecipeProvider {
    async fn has_recipes(&self, _key: &Key, _envref: EnvRef<E>) -> Result<bool, Error> {
        Ok(false)
    }

    async fn assets_with_recipes(
        &self,
        _key: &Key,
        _envref: EnvRef<E>,
    ) -> Result<Vec<ResourceName>, Error> {
        Ok(Vec::new())
    }

    async fn recipe_plan(&self, key: &Key, envref: EnvRef<E>) -> Result<Plan, Error> {
        let recipe = self
            .recipes
            .get(key)
            .cloned()
            .ok_or_else(|| Error::key_not_found(key))?;
        recipe.to_plan_for_key(envref.get_command_metadata_registry(), key)
    }

    async fn recipe(&self, key: &Key, _envref: EnvRef<E>) -> Result<Recipe, Error> {
        self.recipes
            .get(key)
            .cloned()
            .ok_or_else(|| Error::key_not_found(key))
    }

    async fn recipe_opt(&self, key: &Key, _envref: EnvRef<E>) -> Result<Option<Recipe>, Error> {
        Ok(self.recipes.get(key).cloned())
    }
}

/// Builds a recipe invoking `counted-<tag>`, with `stored`/`cached` set as given.
fn counting_recipe(tag: &str, stored: Option<bool>, cached: Option<bool>) -> Result<Recipe, Error> {
    let mut recipe = Recipe::new(format!("counted-{tag}"), String::new(), String::new())?;
    recipe.stored = stored;
    recipe.cached = cached;
    Ok(recipe)
}

// ---------------------------------------------------------------------------
// Environment construction
// ---------------------------------------------------------------------------

fn build_default_env(
    store: AsyncMemoryStore,
    provider: TaggedRecipeProvider,
) -> Result<EnvRef<SimpleEnvironment<Value>>, Error> {
    type CommandEnvironment = SimpleEnvironment<Value>;
    let mut env = SimpleEnvironment::<Value>::new();
    {
        let cr = &mut env.command_registry;
        register_command!(cr, fn counted(state, tag: String) -> result)?;
    }
    env.with_async_store(Box::new(store));
    env.with_recipe_provider(Box::new(provider));
    Ok(env.to_ref())
}

fn build_immediate_env(
    store: AsyncMemoryStore,
    provider: TaggedRecipeProvider,
) -> Result<EnvRef<ImmediateEnvironment<Value>>, Error> {
    type CommandEnvironment = ImmediateEnvironment<Value>;
    let mut env = ImmediateEnvironment::<Value>::new();
    {
        let cr = &mut env.command_registry;
        register_command!(cr, fn counted(state, tag: String) -> result)?;
    }
    env.with_async_store(Box::new(store));
    env.with_recipe_provider(Box::new(provider));
    Ok(env.to_ref())
}

// ---------------------------------------------------------------------------
// Scenario bodies (written once, run against both managers)
// ---------------------------------------------------------------------------

/// `stored_false_value_is_not_written`: after evaluating a key whose recipe has
/// `stored: Some(false)`, the store holds neither data nor a metadata-only entry — asserted only
/// after the `MetadataSaver` interval (100 ms) has passed, since its writes are debounced in a
/// spawned task and an immediate assertion would pass before the very write it guards against.
async fn scenario_stored_false_not_written<E>(envref: EnvRef<E>, key: Key) -> Result<(), Error>
where
    E: Environment<Value = Value>,
{
    let manager = envref.get_asset_manager();
    let asset = manager.get(&key).await?;
    let state = asset.get().await?;
    assert_eq!(state.status(), Status::Ready);

    tokio::time::sleep(Duration::from_millis(300)).await;

    let store = envref.get_async_store();
    assert!(
        !store.contains(&key).await?,
        "stored: false must leave no data and no metadata-only entry in the store"
    );
    Ok(())
}

/// `stored_false_still_reads_an_existing_copy`: with data already stored under the key, a
/// request returns it and the recipe's counter command does not run.
async fn scenario_stored_false_reads_existing<E>(
    envref: EnvRef<E>,
    key: Key,
    tag: String,
) -> Result<(), Error>
where
    E: Environment<Value = Value>,
{
    let manager = envref.get_asset_manager();
    let asset = manager.get(&key).await?;
    let state = asset.get().await?;
    assert_eq!(state.try_into_string()?, "existing-value");
    assert_eq!(
        count_of(&tag),
        0,
        "an existing stored copy must be preferred to recomputation"
    );
    Ok(())
}

/// `cached_false_asset_is_not_reused`: with `stored: Some(false), cached: Some(false)`, two
/// requests run the counter command twice. `stored` must be false too — with it absent the
/// second request would be served from the store and prove nothing about the cache.
async fn scenario_cached_false_not_reused<E>(
    envref: EnvRef<E>,
    key: Key,
    tag: String,
) -> Result<(), Error>
where
    E: Environment<Value = Value>,
{
    let manager = envref.get_asset_manager();
    let first = manager.get(&key).await?;
    first.get().await?;
    let second = manager.get(&key).await?;
    second.get().await?;
    assert_eq!(
        count_of(&tag),
        2,
        "cached: false must evaluate the recipe on every request"
    );
    Ok(())
}

/// `cached_true_asset_is_reused`: the control case, with `stored: Some(false)` so reuse can only
/// come from the cache: two requests run it once.
async fn scenario_cached_true_reused<E>(
    envref: EnvRef<E>,
    key: Key,
    tag: String,
) -> Result<(), Error>
where
    E: Environment<Value = Value>,
{
    let manager = envref.get_asset_manager();
    let first = manager.get(&key).await?;
    first.get().await?;
    let second = manager.get(&key).await?;
    second.get().await?;
    assert_eq!(
        first.id(),
        second.id(),
        "a cached (default) key must hand back the same registered asset"
    );
    assert_eq!(
        count_of(&tag),
        1,
        "a cached asset must be reused, not recomputed, on the second request"
    );
    Ok(())
}

/// `both_false_is_not_volatile`: the resulting metadata has `is_volatile() == false`.
async fn scenario_both_false_not_volatile<E>(envref: EnvRef<E>, key: Key) -> Result<(), Error>
where
    E: Environment<Value = Value>,
{
    let manager = envref.get_asset_manager();
    let asset = manager.get(&key).await?;
    asset.get().await?;
    assert!(
        !asset.is_volatile().await,
        "stored: false and cached: false must not make the asset volatile"
    );
    Ok(())
}

/// `flags_are_recorded_in_metadata_and_asset_info`: `MetadataRecord::stored()`/`cached()` and
/// `AssetInfo`'s equivalents carry the recipe's values. Uses two different values (`false` and
/// `true`) so a bug swapping the two fields would be caught.
async fn scenario_flags_are_recorded<E>(envref: EnvRef<E>, key: Key) -> Result<(), Error>
where
    E: Environment<Value = Value>,
{
    let manager = envref.get_asset_manager();
    let asset = manager.get(&key).await?;
    let state = asset.get().await?;
    assert_eq!(state.metadata.stored(), false, "MetadataRecord::stored()");
    assert_eq!(state.metadata.cached(), true, "MetadataRecord::cached()");

    let info = asset.get_asset_info().await?;
    assert_eq!(info.stored, Some(false), "AssetInfo::stored");
    assert_eq!(info.cached, Some(true), "AssetInfo::cached");
    Ok(())
}

// ---------------------------------------------------------------------------
// stored_false_value_is_not_written
// ---------------------------------------------------------------------------

#[tokio::test]
async fn stored_false_value_is_not_written_default() -> Result<(), Error> {
    let tag = "s1d".to_string();
    let key = parse_key("s1d.txt")?;
    let provider = TaggedRecipeProvider::new([(
        key.clone(),
        counting_recipe(&tag, Some(false), None)?,
    )]);
    let envref = build_default_env(AsyncMemoryStore::new(&Key::new()), provider)?;
    scenario_stored_false_not_written(envref, key).await
}

#[tokio::test]
async fn stored_false_value_is_not_written_immediate() -> Result<(), Error> {
    let tag = "s1i".to_string();
    let key = parse_key("s1i.txt")?;
    let provider = TaggedRecipeProvider::new([(
        key.clone(),
        counting_recipe(&tag, Some(false), None)?,
    )]);
    let envref = build_immediate_env(AsyncMemoryStore::new(&Key::new()), provider)?;
    scenario_stored_false_not_written(envref, key).await
}

// ---------------------------------------------------------------------------
// stored_false_still_reads_an_existing_copy
// ---------------------------------------------------------------------------

async fn store_with_existing_value(key: &Key) -> Result<AsyncMemoryStore, Error> {
    let store = AsyncMemoryStore::new(&Key::new());
    store
        .set(
            key,
            b"existing-value",
            &Metadata::MetadataRecord(
                MetadataRecord::new()
                    .with_key(key.clone())
                    .with_type_identifier("Text".to_owned())
                    .with_status(Status::Source)
                    .clone(),
            ),
        )
        .await?;
    Ok(store)
}

#[tokio::test]
async fn stored_false_still_reads_an_existing_copy_default() -> Result<(), Error> {
    let tag = "s2d".to_string();
    let key = parse_key("s2d.txt")?;
    let provider = TaggedRecipeProvider::new([(
        key.clone(),
        counting_recipe(&tag, Some(false), None)?,
    )]);
    let envref = build_default_env(store_with_existing_value(&key).await?, provider)?;
    scenario_stored_false_reads_existing(envref, key, tag).await
}

#[tokio::test]
async fn stored_false_still_reads_an_existing_copy_immediate() -> Result<(), Error> {
    let tag = "s2i".to_string();
    let key = parse_key("s2i.txt")?;
    let provider = TaggedRecipeProvider::new([(
        key.clone(),
        counting_recipe(&tag, Some(false), None)?,
    )]);
    let envref = build_immediate_env(store_with_existing_value(&key).await?, provider)?;
    scenario_stored_false_reads_existing(envref, key, tag).await
}

// ---------------------------------------------------------------------------
// cached_false_asset_is_not_reused
// ---------------------------------------------------------------------------

#[tokio::test]
async fn cached_false_asset_is_not_reused_default() -> Result<(), Error> {
    let tag = "s3d".to_string();
    let key = parse_key("s3d.txt")?;
    let provider = TaggedRecipeProvider::new([(
        key.clone(),
        counting_recipe(&tag, Some(false), Some(false))?,
    )]);
    let envref = build_default_env(AsyncMemoryStore::new(&Key::new()), provider)?;
    scenario_cached_false_not_reused(envref, key, tag).await
}

#[tokio::test]
async fn cached_false_asset_is_not_reused_immediate() -> Result<(), Error> {
    let tag = "s3i".to_string();
    let key = parse_key("s3i.txt")?;
    let provider = TaggedRecipeProvider::new([(
        key.clone(),
        counting_recipe(&tag, Some(false), Some(false))?,
    )]);
    let envref = build_immediate_env(AsyncMemoryStore::new(&Key::new()), provider)?;
    scenario_cached_false_not_reused(envref, key, tag).await
}

// ---------------------------------------------------------------------------
// cached_true_asset_is_reused
// ---------------------------------------------------------------------------

#[tokio::test]
async fn cached_true_asset_is_reused_default() -> Result<(), Error> {
    let tag = "s4d".to_string();
    let key = parse_key("s4d.txt")?;
    let provider = TaggedRecipeProvider::new([(
        key.clone(),
        counting_recipe(&tag, Some(false), None)?,
    )]);
    let envref = build_default_env(AsyncMemoryStore::new(&Key::new()), provider)?;
    scenario_cached_true_reused(envref, key, tag).await
}

#[tokio::test]
async fn cached_true_asset_is_reused_immediate() -> Result<(), Error> {
    let tag = "s4i".to_string();
    let key = parse_key("s4i.txt")?;
    let provider = TaggedRecipeProvider::new([(
        key.clone(),
        counting_recipe(&tag, Some(false), None)?,
    )]);
    let envref = build_immediate_env(AsyncMemoryStore::new(&Key::new()), provider)?;
    scenario_cached_true_reused(envref, key, tag).await
}

// ---------------------------------------------------------------------------
// both_false_is_not_volatile
// ---------------------------------------------------------------------------

#[tokio::test]
async fn both_false_is_not_volatile_default() -> Result<(), Error> {
    let tag = "s5d".to_string();
    let key = parse_key("s5d.txt")?;
    let provider = TaggedRecipeProvider::new([(
        key.clone(),
        counting_recipe(&tag, Some(false), Some(false))?,
    )]);
    let envref = build_default_env(AsyncMemoryStore::new(&Key::new()), provider)?;
    scenario_both_false_not_volatile(envref, key).await
}

#[tokio::test]
async fn both_false_is_not_volatile_immediate() -> Result<(), Error> {
    let tag = "s5i".to_string();
    let key = parse_key("s5i.txt")?;
    let provider = TaggedRecipeProvider::new([(
        key.clone(),
        counting_recipe(&tag, Some(false), Some(false))?,
    )]);
    let envref = build_immediate_env(AsyncMemoryStore::new(&Key::new()), provider)?;
    scenario_both_false_not_volatile(envref, key).await
}

// ---------------------------------------------------------------------------
// flags_are_recorded_in_metadata_and_asset_info
// ---------------------------------------------------------------------------

#[tokio::test]
async fn flags_are_recorded_in_metadata_and_asset_info_default() -> Result<(), Error> {
    let tag = "s6d".to_string();
    let key = parse_key("s6d.txt")?;
    let provider = TaggedRecipeProvider::new([(
        key.clone(),
        counting_recipe(&tag, Some(false), Some(true))?,
    )]);
    let envref = build_default_env(AsyncMemoryStore::new(&Key::new()), provider)?;
    scenario_flags_are_recorded(envref, key).await
}

#[tokio::test]
async fn flags_are_recorded_in_metadata_and_asset_info_immediate() -> Result<(), Error> {
    let tag = "s6i".to_string();
    let key = parse_key("s6i.txt")?;
    let provider = TaggedRecipeProvider::new([(
        key.clone(),
        counting_recipe(&tag, Some(false), Some(true))?,
    )]);
    let envref = build_immediate_env(AsyncMemoryStore::new(&Key::new()), provider)?;
    scenario_flags_are_recorded(envref, key).await
}
