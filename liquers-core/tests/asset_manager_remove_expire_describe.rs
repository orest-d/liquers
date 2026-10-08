//! Core behaviour behind the Assets API: status-aware `remove`, `expire`, `set_description`,
//! `removedir`, `lookup_query_asset`, the `Removed` notification and `to_override` on a `Source`.
//! Design: `specs/design/axum-assets-endpoints/` (Phase 3, AMR01–AMR61).
//!
//! Every call that takes the manager's key-mutation lock goes through `within`, so a
//! lock-discipline mistake (a reentrant lock) fails in seconds instead of hanging.

use liquers_core::{
    assets::{AssetData, AssetManager, AssetNotificationMessage},
    command_metadata::CommandKey,
    context::{Environment, EnvRef, SimpleEnvironment},
    error::ErrorType,
    metadata::{Metadata, MetadataRecord, Status, Version},
    parse::{parse_key, parse_query},
    query::Key,
    recipes::{DefaultRecipeProvider, Recipe, RecipeList},
    state::State,
    store::{AsyncMemoryStore, AsyncStore},
    value::Value,
};

/// Fail fast instead of hanging if a locking call deadlocks.
async fn within<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(std::time::Duration::from_secs(10), future)
        .await
        .expect("call did not finish within 10 s: key-mutation lock held reentrantly?")
}

/// Build an environment over a provided store (used directly by the restart test, AMR24).
fn env_over(store: AsyncMemoryStore) -> EnvRef<SimpleEnvironment<Value>> {
    let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
    env.command_registry
        .register_command(CommandKey::new_name("make_text"), |_, _, _| {
            Ok(Value::from("generated"))
        })
        .unwrap();
    env.command_registry
        .register_command(CommandKey::new_name("upper"), |state: &State<Value>, _, _| {
            Ok(Value::from(state.try_into_string()?.to_uppercase()))
        })
        .unwrap();
    env.with_async_store(Box::new(store));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider::new()));
    env.to_ref()
}

/// Environment with a `RecipeList` stored at `<dir>/recipes.yaml`. `DefaultRecipeProvider` reads
/// `<key's directory>/recipes.yaml` and joins the recipe's own filename to that directory — a
/// nested path segment written directly into the recipe query (e.g. `"make_text/data/x.txt"`)
/// does NOT act as a directory: `data` there parses as a second chained action and fails to
/// resolve (verified with `liquers-validate --no-registry`; see "Fixes Made Against the Drafts"
/// #4). A recipe targeting a key under `data/` therefore needs its `RecipeList` stored at
/// `data/recipes.yaml`, with a plain filename in the query (`"make_text/x.txt"`).
async fn env_with_at(dir: &Key, recipes: &[(&str, &str, &str)]) -> EnvRef<SimpleEnvironment<Value>> {
    let mut rl = RecipeList::new();
    for (q, t, d) in recipes {
        rl.add_recipe(Recipe::new(q.to_string(), t.to_string(), d.to_string()).unwrap());
    }
    let store = AsyncMemoryStore::new(&Key::new());
    store
        .set(
            &dir.join("recipes.yaml"),
            serde_yaml::to_string(&rl).unwrap().as_bytes(),
            &Metadata::new(),
        )
        .await
        .unwrap();
    env_over(store)
}

/// Root-level recipes (the common case).
async fn env_with(recipes: &[(&str, &str, &str)]) -> EnvRef<SimpleEnvironment<Value>> {
    env_with_at(&Key::new(), recipes).await
}

/// Minimal MetadataRecord for a plaintext Source write via `AssetManager::set_binary`.
/// `type_identifier`/`type_name` are `String`, not `Option<String>`; `set_binary` takes the
/// record BY VALUE.
fn metadata_text() -> MetadataRecord {
    MetadataRecord {
        type_identifier: "Text".to_string(),
        type_name: "text".to_string(),
        data_format: Some("txt".to_string()),
        ..Default::default()
    }
}

/// `AsyncStore::get_metadata` returns the `Metadata` enum; status reads through `.status()`.
fn stored_status(metadata: &Metadata) -> Status {
    metadata.status()
}

#[tokio::test]
async fn amr01_remove_source_no_recipe_cascades() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[("-R/notes/a.txt/-/upper/summary.txt", "Summary Recipe", "Uppercase the note")]).await;
    let am = envref.get_asset_manager();
    let store = envref.get_async_store();

    let notes_key = parse_key("notes/a.txt")?;
    within(am.set_binary(&notes_key, b"hello world", metadata_text())).await?;
    assert_eq!(am.get_asset_info(&notes_key).await?.status, Status::Source);

    let summary_key = parse_key("summary.txt")?;
    let value = am.get(&summary_key).await?.get().await?;
    assert_eq!(value.try_into_string()?, "HELLO WORLD");
    assert_eq!(am.get_asset_info(&summary_key).await?.status, Status::Ready);

    within(am.remove(&notes_key)).await?;

    assert!(!store.contains(&notes_key).await?, "store must not hold the removed source");
    let err = am.get_asset_info(&notes_key).await.expect_err("removed source is gone");
    assert_eq!(err.error_type, ErrorType::KeyNotFound);
    assert_eq!(
        am.get_asset_info(&summary_key).await?.status,
        Status::Expired,
        "dependent cascades to Expired when its Source is removed"
    );
    Ok(())
}

#[tokio::test]
async fn amr02_remove_override_with_recipe_recomputes() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[("make_text/source.txt", "Text Source", "A text value")]).await;
    let am = envref.get_asset_manager();
    let source_key = parse_key("source.txt")?;

    within(am.set_binary(&source_key, b"my custom text", metadata_text())).await?;
    assert_eq!(am.get_asset_info(&source_key).await?.status, Status::Override);

    within(am.remove(&source_key)).await?;

    let value = am.get(&source_key).await?.get().await?;
    assert_eq!(value.try_into_string()?, "generated");
    assert_eq!(am.get_asset_info(&source_key).await?.status, Status::Ready);
    Ok(())
}

#[tokio::test]
async fn amr03_remove_ready_keeps_metadata_and_version() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[("-R/notes/a.txt/-/upper/summary.txt", "Summary Recipe", "Uppercase the note")]).await;
    let am = envref.get_asset_manager();
    let store = envref.get_async_store();

    let notes_key = parse_key("notes/a.txt")?;
    within(am.set_binary(&notes_key, b"hello", metadata_text())).await?;

    let summary_key = parse_key("summary.txt")?;
    let value = am.get(&summary_key).await?.get().await?;
    assert_eq!(value.try_into_string()?, "HELLO");
    let version_before = am.version(&summary_key).await?;
    assert!(!version_before.is_unknown());

    within(am.remove(&summary_key)).await?;

    assert!(store.contains(&summary_key).await?, "metadata must survive the remove");
    let stored = store.get_metadata(&summary_key).await?;
    assert_eq!(stored_status(&stored), Status::Recipe);
    assert_eq!(am.version(&summary_key).await?, version_before, "version must survive the remove");
    assert_eq!(am.get_asset_info(&notes_key).await?.status, Status::Source, "the source is untouched");
    Ok(())
}

#[tokio::test]
async fn amr04_reevaluate_after_remove_ready_no_cascade() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[
        ("-R/notes/a.txt/-/upper/summary.txt", "Summary", "depends on notes"),
        ("-R/summary.txt/-/upper/summary2.txt", "Summary2", "depends on summary"),
    ])
    .await;
    let am = envref.get_asset_manager();

    let notes_key = parse_key("notes/a.txt")?;
    within(am.set_binary(&notes_key, b"hello", metadata_text())).await?;

    let summary_key = parse_key("summary.txt")?;
    let summary2_key = parse_key("summary2.txt")?;
    let _ = am.get(&summary_key).await?.get().await?;
    let value2 = am.get(&summary2_key).await?.get().await?;
    assert_eq!(value2.try_into_string()?, "HELLO");
    assert_eq!(am.get_asset_info(&summary2_key).await?.status, Status::Ready);

    within(am.remove(&summary_key)).await?;
    let value_again = am.get(&summary_key).await?.get().await?;
    assert_eq!(value_again.try_into_string()?, "HELLO");

    assert_eq!(
        am.get_asset_info(&summary2_key).await?.status,
        Status::Ready,
        "the grandchild stays Ready: dropping a recipe-computed value does not cascade"
    );
    Ok(())
}

#[tokio::test]
async fn amr05_remove_nonexistent_key_not_found() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[]).await;
    let am = envref.get_asset_manager();
    let key = parse_key("phantom/file.txt")?;

    let err = within(am.remove(&key)).await.expect_err("no value, no recipe");
    assert_eq!(err.error_type, ErrorType::KeyNotFound);
    Ok(())
}

#[tokio::test]
async fn amr06_remove_recipe_key_not_evaluated() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[("make_text/source.txt", "Text Source", "A text value")]).await;
    let am = envref.get_asset_manager();
    let store = envref.get_async_store();
    let source_key = parse_key("source.txt")?;

    within(am.remove(&source_key)).await.expect("remove of an un-evaluated recipe key succeeds");
    assert!(!store.contains(&source_key).await?);
    Ok(())
}

#[tokio::test]
async fn amr07_remove_directory_status_conflict() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[]).await;
    let am = envref.get_asset_manager();
    let dir_key = parse_key("a_directory")?;
    am.makedir(&dir_key).await?;

    let err = within(am.remove(&dir_key)).await.expect_err("removing a directory is refused");
    assert_eq!(err.error_type, ErrorType::StatusConflict);
    assert_eq!(err.key, Some(dir_key.encode()), "status_conflict sets the key");
    Ok(())
}

#[tokio::test]
async fn amr10_expire_live_ready_cascades() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[("make_text/a.txt", "A", "text A"), ("-R/a.txt/-/upper/b.txt", "B", "B depends on A")]).await;
    let am = envref.get_asset_manager();
    let (key_a, key_b) = (parse_key("a.txt")?, parse_key("b.txt")?);
    let _ = am.get(&key_a).await?.get().await?;
    let _ = am.get(&key_b).await?.get().await?;

    within(am.expire(&key_a)).await.expect("expire should succeed");

    assert_eq!(am.get_asset_info(&key_a).await?.status, Status::Expired);
    assert_eq!(am.get_asset_info(&key_b).await?.status, Status::Expired, "dependent cascades");
    Ok(())
}

#[tokio::test]
async fn amr11_expire_cascades_through_two_levels() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[
        ("make_text/root.txt", "Root", ""),
        ("-R/root.txt/-/upper/level1.txt", "L1", "depends on root"),
        ("-R/level1.txt/-/upper/level2.txt", "L2", "depends on level1"),
    ])
    .await;
    let am = envref.get_asset_manager();
    let (root, l1, l2) = (parse_key("root.txt")?, parse_key("level1.txt")?, parse_key("level2.txt")?);
    let _ = am.get(&root).await?.get().await?;
    let _ = am.get(&l1).await?.get().await?;
    let _ = am.get(&l2).await?.get().await?;

    within(am.expire(&root)).await?;

    assert_eq!(am.get_asset_info(&root).await?.status, Status::Expired);
    assert_eq!(am.get_asset_info(&l1).await?.status, Status::Expired);
    assert_eq!(am.get_asset_info(&l2).await?.status, Status::Expired);
    Ok(())
}

#[tokio::test]
async fn amr12_expire_idempotent_on_already_expired() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[("make_text/a.txt", "A", "")]).await;
    let am = envref.get_asset_manager();
    let key = parse_key("a.txt")?;
    let _ = am.get(&key).await?.get().await?;

    within(am.expire(&key)).await.expect("first expire");
    within(am.expire(&key)).await.expect("second expire is idempotent");
    assert_eq!(am.get_asset_info(&key).await?.status, Status::Expired);
    Ok(())
}

#[tokio::test]
async fn amr13_expire_source_no_recipe_rejects() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[]).await;
    let am = envref.get_asset_manager();
    let key = parse_key("stored_source.txt")?;
    within(am.set_binary(&key, b"source data", metadata_text())).await?;

    let err = within(am.expire(&key)).await.expect_err("Source has no recipe to recover from");
    assert_eq!(err.error_type, ErrorType::StatusConflict);
    assert_eq!(err.key, Some(key.encode()));
    let msg = err.message.to_lowercase();
    assert!(msg.contains("expire") && msg.contains("source"));
    Ok(())
}

#[tokio::test]
async fn amr14_expire_never_evaluated_recipe_key_rejects() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[("make_text/never_touched.txt", "Never", "recipe only")]).await;
    let am = envref.get_asset_manager();
    let key = parse_key("never_touched.txt")?;

    let err = within(am.expire(&key)).await.expect_err("Recipe status cannot expire");
    assert_eq!(err.error_type, ErrorType::StatusConflict);
    assert_eq!(err.key, Some(key.encode()));
    Ok(())
}

#[tokio::test]
async fn amr15_expire_absent_key_not_found() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[]).await;
    let am = envref.get_asset_manager();
    let key = parse_key("nonexistent.txt")?;

    let err = within(am.expire(&key)).await.expect_err("nothing live, nothing stored, no recipe");
    assert_eq!(err.error_type, ErrorType::KeyNotFound);
    Ok(())
}

#[test]
fn amr16_status_conflict_constructor_shape() {
    let key = parse_key("test.txt").unwrap();
    let err = liquers_core::error::Error::status_conflict(&key, Status::Source, "expire");

    assert_eq!(err.error_type, ErrorType::StatusConflict);
    assert_eq!(err.key, Some(key.encode()), "Error.key is Option<String>, from key.encode()");
    let msg = err.message.to_lowercase();
    assert!(msg.contains("expire"));
    assert!(msg.contains("source"));
}

#[tokio::test]
async fn amr17_set_description_on_source_updates_fields_version_unchanged() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[]).await;
    let am = envref.get_asset_manager();
    let key = parse_key("source.txt")?;
    within(am.set_binary(&key, b"content", metadata_text())).await?;
    let version_before = am.version(&key).await?;

    within(am.set_description(&key, Some("New Title".to_string()), Some("New Desc".to_string()))).await?;

    let info = am.get_asset_info(&key).await?;
    assert_eq!(info.title, "New Title");
    assert_eq!(info.description, "New Desc");
    assert_eq!(am.version(&key).await?, version_before, "version unchanged (also covers set_description's happy-path idempotence on version)");
    Ok(())
}

#[tokio::test]
async fn amr18_set_description_both_none_is_parameter_error() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[]).await;
    let am = envref.get_asset_manager();
    let key = parse_key("source.txt")?;
    within(am.set_binary(&key, b"content", metadata_text())).await?;

    let err = within(am.set_description(&key, None, None)).await.expect_err("both None must be rejected");
    assert_eq!(err.error_type, ErrorType::ParameterError);
    Ok(())
}

#[tokio::test]
async fn amr19_set_description_on_computed_ready_rejects() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[("make_text/computed.txt", "Computed", "a recipe")]).await;
    let am = envref.get_asset_manager();
    let key = parse_key("computed.txt")?;
    let _ = am.get(&key).await?.get().await?;

    let err = within(am.set_description(&key, Some("Title".to_string()), None)).await.expect_err("only Source may be described");
    assert_eq!(err.error_type, ErrorType::StatusConflict);
    Ok(())
}

#[tokio::test]
async fn amr20_set_description_absent_key_not_found() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[]).await;
    let am = envref.get_asset_manager();
    let key = parse_key("nonexistent.txt")?;

    let err = within(am.set_description(&key, Some("Title".to_string()), None)).await.expect_err("absent key");
    assert_eq!(err.error_type, ErrorType::KeyNotFound);
    Ok(())
}

#[tokio::test]
async fn amr22_get_asset_info_expired_live_reports_expired_without_reevaluating() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[("make_text/a.txt", "A", "")]).await;
    let am = envref.get_asset_manager();
    let key = parse_key("a.txt")?;
    let _ = am.get(&key).await?.get().await?;
    within(am.expire(&key)).await?;

    let info = am.get_asset_info(&key).await?;
    assert_eq!(info.status, Status::Expired, "get_asset_info must not call get() and re-evaluate");
    Ok(())
}

#[tokio::test]
async fn amr23_get_asset_info_never_evaluated_recipe_reports_recipe() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[("make_text/never_eval.txt", "Never", "recipe")]).await;
    let am = envref.get_asset_manager();
    let key = parse_key("never_eval.txt")?;

    let info = am.get_asset_info(&key).await?;
    assert_eq!(info.status, Status::Recipe, "get_asset_info must not evaluate the recipe");
    Ok(())
}

#[tokio::test]
async fn amr24_dropped_intermediate_does_not_block_dependent_after_restart() -> Result<(), Box<dyn std::error::Error>> {
    let keys = ["recipes.yaml", "notes/a.txt", "summary.txt", "summary2.txt"]
        .iter()
        .map(|k| parse_key(k))
        .collect::<Result<Vec<_>, _>>()?;
    let summary_key = parse_key("summary.txt")?;
    let summary2_key = parse_key("summary2.txt")?;

    let persisted = {
        let envref = env_with(&[
            ("-R/notes/a.txt/-/upper/summary.txt", "Summary", "depends on notes"),
            ("-R/summary.txt/-/upper/summary2.txt", "Summary2", "depends on summary"),
        ])
        .await;
        let am = envref.get_asset_manager();
        within(am.set_binary(&keys[1], b"hello", metadata_text())).await?;
        let _ = am.get(&summary2_key).await?.get().await?;
        within(am.remove(&summary_key)).await?;
        let store = envref.get_async_store();
        let mut entries = Vec::new();
        for key in &keys {
            entries.push((key.clone(), store.get(key).await?));
        }
        entries
    };

    let store2 = AsyncMemoryStore::new(&Key::new());
    for (key, (bytes, metadata)) in &persisted {
        store2.set(key, bytes, metadata).await?;
    }
    let envref2 = env_over(store2);
    assert_eq!(
        stored_status(&envref2.get_async_store().get_metadata(&summary_key).await?),
        Status::Recipe,
        "precondition: the intermediate was dropped, not deleted"
    );

    let mut reloaded = AssetData::<SimpleEnvironment<Value>>::new(
        9601,
        summary2_key.clone().into(),
        Some(summary2_key.clone()),
        envref2.clone(),
    );
    assert!(
        reloaded.try_fast_track().await?,
        "a dependency dropped to Recipe (version kept) must not block the dependent's fast track"
    );
    Ok(())
}

#[tokio::test]
async fn amr30_removedir_deletes_stored_keys_recursively() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[]).await;
    let am = envref.get_asset_manager();
    let store = envref.get_async_store();

    let root_key = parse_key("data")?;
    let file1 = parse_key("data/file1.txt")?;
    let file2 = parse_key("data/file2.txt")?;
    let subdir_file = parse_key("data/sub/file3.txt")?;

    within(am.set_binary(&file1, b"content1", metadata_text())).await?;
    within(am.set_binary(&file2, b"content2", metadata_text())).await?;
    within(am.set_binary(&subdir_file, b"content3", metadata_text())).await?;
    am.makedir(&root_key).await?;

    within(am.removedir(&root_key)).await?;

    assert!(!store.contains(&file1).await?);
    assert!(!store.contains(&file2).await?);
    assert!(!store.contains(&subdir_file).await?);
    let err = am.get_asset_info(&root_key).await.expect_err("directory removed");
    assert_eq!(err.error_type, ErrorType::KeyNotFound);
    Ok(())
}

#[tokio::test]
async fn amr31_removedir_source_child_cascades_dependents() -> Result<(), Box<dyn std::error::Error>> {
    // The dependent lives outside the directory: under O15 = (a) anything inside it is removed.
    let envref = env_with(&[("-R/data/source.txt/-/upper/derived.txt", "Derived", "")]).await;
    let am = envref.get_asset_manager();

    let data_key = parse_key("data")?;
    let source_key = parse_key("data/source.txt")?;
    let derived_key = parse_key("derived.txt")?;

    within(am.set_binary(&source_key, b"hello", metadata_text())).await?;
    let _ = am.get(&derived_key).await?.get().await?;

    am.makedir(&data_key).await?;
    within(am.removedir(&data_key)).await?;

    let err = am.get_asset_info(&source_key).await.expect_err("source is gone");
    assert_eq!(err.error_type, ErrorType::KeyNotFound);
    assert_eq!(am.get_asset_info(&derived_key).await?.status, Status::Expired, "dependent cascades");
    Ok(())
}

// O15 = (a): a removed directory takes its recipes and kept versions with it.
#[tokio::test]
async fn amr32_removedir_takes_computed_child_and_its_version() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with_at(&parse_key("data")?, &[("make_text/computed.txt", "Computed", "")]).await;
    let am = envref.get_asset_manager();
    let store = envref.get_async_store();

    let data_key = parse_key("data")?;
    let computed_key = parse_key("data/computed.txt")?;

    let _ = am.get(&computed_key).await?.get().await?;
    assert!(!am.version(&computed_key).await?.is_unknown());

    within(am.removedir(&data_key)).await?;

    assert!(!store.contains(&computed_key).await?, "the kept entry goes with the directory");
    assert!(!store.contains(&parse_key("data/recipes.yaml")?).await?, "recipes.yaml goes too");
    assert_eq!(am.version(&computed_key).await?, Version::unknown());
    Ok(())
}

#[tokio::test]
async fn amr33_removedir_takes_recipe_declared_keys() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with_at(&parse_key("data")?, &[("make_text/recipe_only.txt", "RecipeOnly", "")]).await;
    let am = envref.get_asset_manager();

    let data_key = parse_key("data")?;
    let recipe_only_key = parse_key("data/recipe_only.txt")?;
    assert_eq!(am.get_asset_info(&recipe_only_key).await?.status, Status::Recipe);

    within(am.removedir(&data_key)).await?;

    let err = am.get_asset_info(&recipe_only_key).await.expect_err("recipe went with the directory");
    assert_eq!(err.error_type, ErrorType::KeyNotFound);
    Ok(())
}

#[tokio::test]
async fn amr34_removedir_absent_directory_not_found() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[]).await;
    let am = envref.get_asset_manager();
    let nonexistent_key = parse_key("nonexistent_dir")?;

    let err = within(am.removedir(&nonexistent_key)).await.expect_err("no such directory");
    assert_eq!(err.error_type, ErrorType::KeyNotFound);
    Ok(())
}

#[tokio::test]
async fn amr40_lookup_query_asset_none_before_request() {
    let envref = env_with(&[]).await;
    let am = envref.get_asset_manager();
    let query = parse_query("make_text").unwrap();
    assert!(am.lookup_query_asset(&query).is_none(), "query must not exist before get_asset");
}

#[tokio::test]
async fn amr41_lookup_query_asset_some_after_get_asset() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[]).await;
    let am = envref.get_asset_manager();
    let query = parse_query("make_text")?;
    let _ = am.get_asset(&query).await?;
    assert!(am.lookup_query_asset(&query).is_some(), "query is cached after get_asset");
    Ok(())
}

#[tokio::test]
async fn amr42_lookup_query_asset_never_creates() {
    let envref = env_with(&[]).await;
    let am = envref.get_asset_manager();
    let query = parse_query("make_text").unwrap();
    assert!(am.lookup_query_asset(&query).is_none());
    assert!(am.lookup_query_asset(&query).is_none(), "second lookup is also None: no side effect");
}

#[tokio::test]
async fn amr43_lookup_query_asset_pure_key_delegates() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[]).await;
    let am = envref.get_asset_manager();
    let key = parse_key("notes/a.txt")?;
    within(am.set_binary(&key, b"content", metadata_text())).await?;
    // `set_binary` is store-only; `get` makes the asset live.
    let _ = am.get(&key).await?;

    let pure_key_query = liquers_core::parse::parse_query("-R/notes/a.txt")?;
    assert!(am.lookup_query_asset(&pure_key_query).is_some(), "pure-key query delegates to lookup_key_asset");
    Ok(())
}

#[tokio::test]
async fn amr44_lookup_query_asset_after_removal_none() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[]).await;
    let am = envref.get_asset_manager();
    let key = parse_key("notes/a.txt")?;
    within(am.set_binary(&key, b"content", metadata_text())).await?;

    let query = liquers_core::parse::parse_query("-R/notes/a.txt")?;
    let _ = am.get_asset(&query).await?;
    within(am.remove(&key)).await?;

    assert!(am.lookup_query_asset(&query).is_none(), "after removal, lookup returns None");
    Ok(())
}

#[tokio::test]
async fn amr50_notification_removed_on_remove() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[]).await;
    let am = envref.get_asset_manager();
    let key = parse_key("data/asset.txt")?;

    within(am.set_binary(&key, b"content", metadata_text())).await?;
    let asset_ref = am.get(&key).await?;
    let mut notification_rx = asset_ref.subscribe_to_notifications().await;

    within(am.remove(&key)).await?;

    tokio::time::timeout(std::time::Duration::from_secs(1), notification_rx.changed()).await??;
    assert!(matches!(*notification_rx.borrow(), AssetNotificationMessage::Removed), "got {:?}", *notification_rx.borrow());
    Ok(())
}

#[tokio::test]
async fn amr51_notification_removed_on_set_binary_replacement() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[]).await;
    let am = envref.get_asset_manager();
    let key = parse_key("data/asset.txt")?;

    within(am.set_binary(&key, b"content1", metadata_text())).await?;
    let asset_ref = am.get(&key).await?;
    let mut notification_rx = asset_ref.subscribe_to_notifications().await;

    within(am.set_binary(&key, b"content2", metadata_text())).await?;

    tokio::time::timeout(std::time::Duration::from_secs(1), notification_rx.changed()).await??;
    assert!(matches!(*notification_rx.borrow(), AssetNotificationMessage::Removed), "got {:?}", *notification_rx.borrow());
    Ok(())
}

#[tokio::test]
async fn amr52_notification_removed_not_sent_by_expiration() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[("make_text/derived.txt", "Derived", "")]).await;
    let am = envref.get_asset_manager();
    let key = parse_key("derived.txt")?;

    let _ = am.get(&key).await?.get().await?;
    let asset_ref = am.get(&key).await?;
    let mut notification_rx = asset_ref.subscribe_to_notifications().await;

    within(am.expire(&key)).await?;

    tokio::time::timeout(std::time::Duration::from_secs(1), notification_rx.changed()).await??;
    assert!(matches!(*notification_rx.borrow(), AssetNotificationMessage::Expired), "got {:?}", *notification_rx.borrow());
    assert!(!matches!(*notification_rx.borrow(), AssetNotificationMessage::Removed));
    Ok(())
}

#[tokio::test]
async fn amr53_subscription_ends_after_removed() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[]).await;
    let am = envref.get_asset_manager();
    let key = parse_key("data/asset.txt")?;

    within(am.set_binary(&key, b"content", metadata_text())).await?;
    let asset_ref = am.get(&key).await?;
    let mut notification_rx = asset_ref.subscribe_to_notifications().await;

    within(am.remove(&key)).await?;
    tokio::time::timeout(std::time::Duration::from_secs(1), notification_rx.changed()).await??;

    let result = tokio::time::timeout(std::time::Duration::from_millis(100), notification_rx.changed()).await;
    assert!(result.is_err(), "no further changes after Removed: the watch has no more senders");
    Ok(())
}

#[tokio::test]
async fn amr60_to_override_stored_only_source_noop() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[]).await;
    let am = envref.get_asset_manager();
    let key = parse_key("data/source.txt")?;
    within(am.set_binary(&key, b"content", metadata_text())).await?;
    assert_eq!(am.get_asset_info(&key).await?.status, Status::Source);

    within(am.to_override(&key)).await?;

    assert_eq!(am.get_asset_info(&key).await?.status, Status::Source, "to_override on stored Source is a no-op");
    Ok(())
}

#[tokio::test]
async fn amr61_to_override_live_source_noop() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[]).await;
    let am = envref.get_asset_manager();
    let key = parse_key("data/source.txt")?;
    within(am.set_binary(&key, b"content", metadata_text())).await?;
    let _ = am.get(&key).await?; // make it live

    assert_eq!(am.get_asset_info(&key).await?.status, Status::Source);
    within(am.to_override(&key)).await?;
    assert_eq!(am.get_asset_info(&key).await?.status, Status::Source, "to_override on a live Source is a no-op");
    Ok(())
}
