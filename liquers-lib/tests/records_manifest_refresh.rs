//! A manifest written to (or removed from) a running environment through the asset manager is
//! seen at once by the folder-listing cache of `ManifestRecipeProvider`
//! (`specs/design/recipe-provider-listing-contract/`, Part B), and template chunks are producible
//! (`can_make`) without being listed (`contains`).
#![cfg(feature = "records")]

use liquers_core::assets::AssetManager;
use liquers_core::context::{EnvRef, Environment};
use liquers_core::error::Error;
use liquers_core::metadata::MetadataRecord;
use liquers_core::parse::parse_key;
use liquers_core::query::Key;
use liquers_core::store::AsyncMemoryStore;

use liquers_lib::environment::DefaultEnvironment;
use liquers_lib::value::Value;

const MANIFEST: &str = "
manifest: record-stream
chunks:
  - query: ns-fixture/fixture_rows-0-1/summary.csv
template:
  query: ns-fixture/fixture_rows
  first_offset: 1000
  step: 10
  batch_size: 10
";

fn text_metadata() -> MetadataRecord {
    MetadataRecord {
        type_identifier: "Text".to_string(),
        type_name: "text".to_string(),
        data_format: Some("txt".to_string()),
        ..Default::default()
    }
}

fn env() -> EnvRef<DefaultEnvironment<Value>> {
    let mut env = DefaultEnvironment::<Value>::new();
    env.with_async_store(Box::new(AsyncMemoryStore::new(&Key::new())));
    env.to_ref()
}

#[tokio::test]
async fn a_manifest_written_through_the_manager_is_seen_and_removed_again() -> Result<(), Error> {
    let envref = env();
    let manager = envref.get_asset_manager();
    let explicit = parse_key("data/sales/summary.csv")?;
    let template = parse_key("data/sales/weekly_0001.csv")?;
    let manifest = parse_key("data/sales/weekly.manifest.yaml")?;

    // Looked up before the manifest exists: the folder's (empty) listing is cached.
    assert!(!manager.contains(&explicit).await?);
    assert!(!manager.can_make(&template).await?);

    manager
        .set_binary(&manifest, MANIFEST.as_bytes(), text_metadata())
        .await?;
    assert!(manager.contains(&explicit).await?, "an explicit chunk is listed");
    assert!(manager.can_make(&template).await?, "a template chunk is producible");
    assert!(!manager.contains(&template).await?, "a template chunk is not listed");

    manager.remove(&manifest).await?;
    assert!(!manager.contains(&explicit).await?);
    assert!(!manager.can_make(&template).await?);
    Ok(())
}
