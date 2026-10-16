//! Store adapters built from configuration (`specs/design/store-adapters/`, Phase 3 integration
//! tests): `read_only`, the `metadata` section, its defaults, and central storage on a router.
#![cfg(feature = "store-conformance")]

use std::collections::HashMap;
use std::sync::Arc;

use liquers_core::error::Error;
use liquers_core::metadata::{Metadata, MetadataRecord};
use liquers_core::parse::parse_key;
use liquers_core::query::Key;
use liquers_core::store::realm::{register_test_store_config, StoreConfigFunctions};
use liquers_core::store::test_support::register_front_matter_stand_in;
use liquers_core::store::{AsyncMemoryStore, AsyncStore, NoAsyncStore, SharedStore};
use liquers_core::store_config::{
    MetadataConfig, StoreConfig, StoreRouterConfig, METADATA_DEFAULTS,
};
use liquers_core::store_factory::{
    core_store_factory, ChainedStoreFactory, StoreArgumentInfo, StoreArgumentType,
    StoreRouterBuilder, StoreTypeInfo, StoreTypeMap,
};

type TestResult = Result<(), Error>;

fn key(text: &str) -> Key {
    parse_key(text).expect("test key parses")
}

fn record(media_type: &str) -> Metadata {
    let mut record = MetadataRecord::new();
    record.with_media_type(media_type.to_string());
    Metadata::from(record)
}

/// A factory with the core types plus `shared` (a store the test holds, by name) and `nodata`
/// (a store without data-only access).
fn factory(shared: &[(&str, Arc<dyn AsyncStore>)]) -> Box<ChainedStoreFactory> {
    let stores: HashMap<String, Arc<dyn AsyncStore>> = shared
        .iter()
        .map(|(name, store)| (name.to_string(), store.clone()))
        .collect();
    let test_types = StoreTypeMap::new()
        .with_store_type(
            StoreTypeInfo::new("shared").with_argument(
                StoreArgumentInfo::new("name", StoreArgumentType::String).required(),
            ),
            Box::new(move |config: &StoreConfig| {
                let name = config.require_config_string_expanded("name")?;
                let store = stores
                    .get(&name)
                    .cloned()
                    .ok_or_else(|| Error::general_error(format!("no shared store named {name}")))?;
                Ok(Box::new(SharedStore(store)) as Box<dyn AsyncStore>)
            }),
        )
        .with_store_type(
            StoreTypeInfo::new("nodata"),
            Box::new(|_config: &StoreConfig| Ok(Box::new(NoAsyncStore) as Box<dyn AsyncStore>)),
        );
    Box::new(
        ChainedStoreFactory::new()
            .chain(Box::new(core_store_factory()))
            .chain(Box::new(test_types)),
    )
}

/// The built-in entries plus `memory_central`.
fn test_entries() -> (
    liquers_core::command_metadata::CommandMetadataRegistry,
    StoreConfigFunctions,
) {
    let (mut cmr, mut functions) = StoreConfigFunctions::builtin();
    register_test_store_config(&mut cmr, &mut functions).expect("test entries register");
    register_front_matter_stand_in(&mut cmr, &mut functions).expect("stand-in registers");
    (cmr, functions)
}

fn build(yaml: &str, shared: &[(&str, Arc<dyn AsyncStore>)]) -> Result<Arc<dyn AsyncStore>, Error> {
    let (cmr, functions) = test_entries();
    StoreRouterBuilder::from_yaml(yaml, factory(shared))?
        .with_store_config(cmr, functions)
        .build_store()
}

fn memory(prefix: &str) -> Arc<dyn AsyncStore> {
    Arc::new(AsyncMemoryStore::new(&key(prefix)))
}

fn error_of(result: Result<Arc<dyn AsyncStore>, Error>) -> String {
    match result {
        Ok(store) => panic!("expected an error, built {}", store.store_name()),
        Err(error) => error.message.clone(),
    }
}

// --- METADATA_DEFAULTS against the documentation ---

/// AC-22: the defaults constant and the table in `STORE_CONFIG_FSD.md` say the same thing.
#[test]
fn metadata_config_defaults_match_the_documented_table() -> TestResult {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../specs/reference/STORE_CONFIG_FSD.md"
    );
    let text = std::fs::read_to_string(path).map_err(|e| Error::general_error(e.to_string()))?;
    let section = text
        .split("### Metadata section")
        .nth(1)
        .ok_or_else(|| Error::general_error("no Metadata section".to_string()))?;
    let mut documented = HashMap::new();
    for line in section
        .lines()
        .skip_while(|l| !l.starts_with("| Field"))
        .skip(2)
    {
        if !line.starts_with('|') {
            break;
        }
        let cells: Vec<&str> = line
            .split('|')
            .map(|c| c.trim().trim_matches('`'))
            .collect();
        let value = if cells[2] == "\"\"" { "" } else { cells[2] };
        documented.insert(cells[1].to_string(), value.to_string());
    }
    let d = METADATA_DEFAULTS;
    let expected = [
        ("storage", d.storage.to_string()),
        ("writes", d.writes.to_string()),
        ("persist_defaults", d.persist_defaults.to_string()),
        ("on_default", d.on_default.to_string()),
        ("on_validate", d.on_validate.to_string()),
        ("on_write", d.on_write.to_string()),
        ("on_read", d.on_read.to_string()),
    ];
    assert_eq!(documented.len(), expected.len(), "{documented:?}");
    for (field, value) in expected {
        assert_eq!(documented.get(field), Some(&value), "field {field}");
    }
    Ok(())
}

/// AC-22: a set field replaces its default whole; nothing is merged.
#[test]
fn set_field_replaces_its_default_whole() {
    let section = MetadataConfig {
        on_read: Some("file_facts".to_string()),
        ..Default::default()
    };
    let resolved = section.resolved();
    assert_eq!(resolved.on_read, "file_facts");
    assert_eq!(resolved.on_write, METADATA_DEFAULTS.on_write);
    let empty = MetadataConfig {
        on_write: Some(String::new()),
        ..Default::default()
    };
    assert_eq!(empty.resolved().on_write, "");
}

// --- wrapping ---

/// AC-6: a mount without a section is the factory's store, unwrapped.
#[tokio::test]
async fn config_without_metadata_section_is_not_wrapped() -> TestResult {
    let member = memory("m");
    let store = build(
        "stores:\n  - type: shared\n    prefix: m\n    config: { name: m }\n",
        &[("m", member.clone())],
    )?;
    store
        .set(&key("m/a.txt"), b"hello", &record("text/plain"))
        .await?;
    // Unwrapped: the record went to the member through `set`, so the member holds it as stored.
    assert!(member.get_stored_metadata(&key("m/a.txt")).await?.is_some());
    assert!(!store.is_read_only());
    Ok(())
}

/// AC-6, AC-22: `metadata: {}` is no section.
#[tokio::test]
async fn empty_metadata_section_is_no_section() -> TestResult {
    let member = memory("m");
    let yaml = "stores:\n  - type: nodata\n    prefix: n\n    metadata: {}\n  - type: shared\n    prefix: m\n    config: { name: m }\n    metadata: {}\n";
    // A section on `nodata` would be refused; an empty one is not a section.
    let store = build(yaml, &[("m", member.clone())])?;
    store
        .set(&key("m/a.txt"), b"hello", &record("text/plain"))
        .await?;
    assert_eq!(store.get_bytes(&key("m/a.txt")).await?, b"hello".to_vec());
    Ok(())
}

/// AC-9: the old free-form map is refused, naming the field.
#[test]
fn old_metadata_map_is_refused() {
    let yaml = "stores:\n  - type: memory\n    prefix: m\n    metadata: { layout: sidecar }\n";
    let error = StoreRouterConfig::from_yaml(yaml)
        .err()
        .map(|e| e.message.clone());
    assert!(
        error.as_deref().is_some_and(|m| m.contains("layout")),
        "{error:?}"
    );
}

/// AC-1, AC-8: a read-only mount refuses writes and serves reads.
#[tokio::test]
async fn read_only_mount_from_config() -> TestResult {
    let member = memory("m");
    member
        .set(&key("m/a.txt"), b"hello", &record("text/plain"))
        .await?;
    let yaml = "stores:\n  - type: shared\n    prefix: m\n    config: { name: m }\n    read_only: true\n    metadata: { storage: native }\n  - type: memory\n    prefix: w\n";
    let store = build(yaml, &[("m", member.clone())])?;
    assert_eq!(store.get_bytes(&key("m/a.txt")).await?, b"hello".to_vec());
    let refused = store
        .set(&key("m/b.txt"), b"x", &record("text/plain"))
        .await;
    assert!(refused.is_err());
    assert!(!member.contains(&key("m/b.txt")).await?);
    // The other mount still writes, so the router is not read-only as a whole.
    store
        .set(&key("w/b.txt"), b"x", &record("text/plain"))
        .await?;
    assert!(!store.is_read_only());
    Ok(())
}

/// AC-1: `read_only` over a store without data access is built, and the build reports a warning
/// naming the mount (returned, so an integration without stderr can show it).
#[tokio::test]
async fn read_only_over_store_without_data_access_warns() -> TestResult {
    let (cmr, functions) = test_entries();
    let built = StoreRouterBuilder::from_yaml(
        "stores:\n  - type: nodata\n    prefix: n\n    read_only: true\n",
        factory(&[]),
    )?
    .with_store_config(cmr, functions)
    .build_store_reporting()?;
    assert_eq!(built.warnings.len(), 1, "{:?}", built.warnings);
    assert!(built.warnings[0].contains("Mount 'n'"), "{:?}", built.warnings);
    assert!(built.warnings[0].contains("no data-only access"), "{:?}", built.warnings);
    let store = built.store;
    assert!(store.is_read_only());
    assert!(store
        .set(&key("n/a.txt"), b"x", &record("text/plain"))
        .await
        .is_err());
    Ok(())
}

/// AC-9: an unknown storage model names the mount and the field.
#[test]
fn unknown_storage_model_names_mount_and_field() {
    let yaml = "stores:\n  - type: memory\n    prefix: m\n    metadata: { storage: sidecars }\n";
    let message = error_of(build(yaml, &[]));
    assert!(message.contains("Mount 'm'"), "{message}");
    assert!(message.contains("storage"), "{message}");
    assert!(message.contains("sidecars"), "{message}");
}

/// AC-9: a section on a store without data-only access is refused, naming the mount.
#[test]
fn metadata_on_store_without_data_access_is_refused() {
    let yaml = "stores:\n  - type: nodata\n    prefix: n\n    metadata: { storage: none }\n";
    let message = error_of(build(yaml, &[]));
    assert!(message.contains("Mount 'n'"), "{message}");
    assert!(message.contains("data-only access"), "{message}");
}

/// AC-9: the bare router cannot carry router-level sections and says which call can.
#[test]
fn bare_router_build_refuses_router_sections() -> TestResult {
    for yaml in [
        "read_only: true\nstores:\n  - type: memory\n    prefix: m\n",
        "metadata: { storage: memory_central }\nstores:\n  - type: memory\n    prefix: m\n",
    ] {
        let result = StoreRouterBuilder::from_yaml(yaml, factory(&[]))?.build();
        let message = result.err().map(|e| e.message.clone()).unwrap_or_default();
        assert!(message.contains("build_store"), "{message}");
    }
    Ok(())
}

/// AC-8: an adapted member answers through the router as it does on its own.
#[tokio::test]
async fn adapted_member_behaves_the_same_through_the_router() -> TestResult {
    let yaml = "stores:\n  - type: memory\n    prefix: m\n    metadata: { storage: per_entry }\n  - type: memory\n    prefix: p\n";
    let store = build(yaml, &[])?;
    for prefix in ["m", "p"] {
        store
            .set(
                &key(&format!("{prefix}/d/a.txt")),
                b"hello",
                &record("text/plain"),
            )
            .await?;
    }
    let adapted = store.get_asset_info(&key("m/d/a.txt")).await?;
    let plain = store.get_asset_info(&key("p/d/a.txt")).await?;
    assert_eq!(adapted.media_type, plain.media_type);
    assert_eq!(adapted.filename, plain.filename);
    assert_eq!(adapted.file_size, Some(5));
    assert_eq!(store.get_bytes(&key("m/d/a.txt")).await?, b"hello".to_vec());
    // The per-entry sidecar is not listed.
    assert_eq!(store.listdir(&key("m/d")).await?, vec!["a.txt".to_string()]);
    assert_eq!(store.listdir(&key("p/d")).await?, vec!["a.txt".to_string()]);
    let listed = store.listdir_asset_info(&key("m/d")).await?;
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].media_type, "text/plain");
    Ok(())
}

/// AC-22: a section written out with every default behaves as the section that leaves them out.
#[tokio::test]
async fn same_section_same_behaviour() -> TestResult {
    let d = METADATA_DEFAULTS;
    let explicit = format!(
        "{{ storage: {}, writes: {}, persist_defaults: {}, on_default: {}, on_validate: \"{}\", on_write: {}, on_read: {} }}",
        d.storage, d.writes, d.persist_defaults, d.on_default, d.on_validate, d.on_write, d.on_read
    );
    let yaml = format!(
        "stores:\n  - type: memory\n    prefix: a\n    metadata: {{ storage: native }}\n  - type: memory\n    prefix: b\n    metadata: {explicit}\n"
    );
    let store = build(&yaml, &[])?;
    for prefix in ["a", "b"] {
        store
            .set(&key(&format!("{prefix}/x.csv")), b"a,b\n1,2\n", &record(""))
            .await?;
    }
    let a = store.get_asset_info(&key("a/x.csv")).await?;
    let b = store.get_asset_info(&key("b/x.csv")).await?;
    assert_eq!(a.media_type, b.media_type);
    assert_eq!(a.file_size, b.file_size);
    assert_eq!(a.filename, b.filename);
    assert_eq!(a.status, b.status);
    Ok(())
}

// --- central storage ---

const CENTRAL: &str = "metadata: { storage: memory_central }\nstores:\n  - type: shared\n    prefix: m\n    config: { name: m }\n    metadata: { storage: per_entry }\n  - type: shared\n    prefix: q\n    config: { name: q }\n";

/// AC-12: reads and writes answered from central storage; the members' data is unchanged.
#[tokio::test]
async fn central_storage_answers_reads_and_writes() -> TestResult {
    let (m, q) = (memory("m"), memory("q"));
    let store = build(CENTRAL, &[("m", m.clone()), ("q", q.clone())])?;
    store
        .set(&key("q/a.txt"), b"hello", &record("text/plain"))
        .await?;
    let metadata = store.get_metadata(&key("q/a.txt")).await?;
    assert_eq!(metadata.get_media_type(), "text/plain");
    assert_eq!(q.get_data(&key("q/a.txt")).await?, b"hello".to_vec());
    assert_eq!(store.get_bytes(&key("q/a.txt")).await?, b"hello".to_vec());
    Ok(())
}

/// AC-12: a member with no section of its own keeps no copy of the record.
#[tokio::test]
async fn central_member_without_section_stores_nothing() -> TestResult {
    let (m, q) = (memory("m"), memory("q"));
    let store = build(CENTRAL, &[("m", m.clone()), ("q", q.clone())])?;
    store
        .set(&key("q/a.txt"), b"hello", &record("text/plain"))
        .await?;
    assert_eq!(q.listdir_data(&key("q")).await?, vec!["a.txt".to_string()]);
    assert!(q.get_stored_metadata(&key("q/a.txt")).await?.is_none());
    Ok(())
}

/// AC-12: a member that names a storage model gets a backup with every central write.
#[tokio::test]
async fn central_backup_written_with_every_central_write() -> TestResult {
    let (m, q) = (memory("m"), memory("q"));
    let store = build(CENTRAL, &[("m", m.clone()), ("q", q.clone())])?;
    store
        .set(&key("m/a.txt"), b"hello", &record("text/plain"))
        .await?;
    let mut names = m.listdir_data(&key("m")).await?;
    names.sort();
    assert_eq!(
        names,
        vec!["a.txt".to_string(), "a.txt.__metadata__".to_string()]
    );
    // The sidecar is the backup, not data the router lists.
    assert_eq!(store.listdir(&key("m")).await?, vec!["a.txt".to_string()]);
    Ok(())
}

/// AC-12: a missing central record is restored from the member's backup, with a warning.
#[tokio::test]
async fn central_backup_restores_a_missing_record() -> TestResult {
    let (m, q) = (memory("m"), memory("q"));
    let first = build(CENTRAL, &[("m", m.clone()), ("q", q.clone())])?;
    first
        .set(&key("m/a.txt"), b"hello", &record("text/markdown"))
        .await?;
    // A second build starts with empty central storage over the same members.
    let second = build(CENTRAL, &[("m", m.clone()), ("q", q.clone())])?;
    let metadata = second.get_metadata(&key("m/a.txt")).await?;
    assert_eq!(metadata.get_media_type(), "text/markdown");
    let restored = metadata
        .metadata_record()
        .map(|r| r.log.iter().any(|entry| entry.message.contains("backup")))
        .unwrap_or(false);
    assert!(restored, "{metadata:?}");
    Ok(())
}

/// AC-9: under central storage a member section may name only its storage.
#[test]
fn central_member_with_handlers_is_refused() {
    let yaml = "metadata: { storage: memory_central }\nstores:\n  - type: memory\n    prefix: m\n    metadata: { storage: per_entry, on_read: file_facts }\n";
    let message = error_of(build(yaml, &[]));
    assert!(message.contains("Mount 'm'"), "{message}");
    assert!(message.contains("central storage"), "{message}");
}

/// AC-5: after `set_metadata`, the file's front matter and `get_metadata` agree. Uses the core
/// stand-in for liquers-lib's `front_matter` / `front_matter_sync`.
#[tokio::test]
async fn front_matter_write_back_through_the_pipeline() -> TestResult {
    let member = memory("m");
    let yaml = "stores:\n  - type: shared\n    prefix: m\n    config: { name: m }\n    metadata: { on_write: defaults/media_type-fill/test_front_matter_sync, on_read: media_type-fill/test_front_matter }\n";
    let store = build(yaml, &[("m", member.clone())])?;
    let k = key("m/note.md");
    store
        .set(
            &k,
            b"---\ntitle: Old\n---\nBody\n",
            &record("text/markdown"),
        )
        .await?;
    let mut metadata = store.get_metadata(&k).await?;
    assert_eq!(metadata.title(), "Old");
    if let Metadata::MetadataRecord(record) = &mut metadata {
        record.title = "New".to_string();
    }
    store.set_metadata(&k, &metadata).await?;
    assert_eq!(
        member.get_data(&k).await?,
        b"---\ntitle: New\n---\nBody\n".to_vec()
    );
    assert_eq!(store.get_metadata(&k).await?.title(), "New");
    assert_eq!(store.get_asset_info(&k).await?.title, "New");
    Ok(())
}
