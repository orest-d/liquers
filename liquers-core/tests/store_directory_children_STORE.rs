//! Directory metadata carries its direct children, one level deep, and a key holding only
//! metadata is listed — on every store (STORE_SEMANTICS §2, §8).

use liquers_core::metadata::{Metadata, MetadataRecord};
use liquers_core::parse::parse_key;
use liquers_core::query::Key;
use liquers_core::store::{AsyncFileStore, AsyncMemoryStore, AsyncStore};

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn temp_root(tag: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    std::env::temp_dir().join(format!("lq-dirchildren-{tag}-{nanos}"))
}

async fn populate(store: &dyn AsyncStore) -> TestResult {
    let empty = Metadata::MetadataRecord(MetadataRecord::new());
    store.set(&parse_key("reports/q1.csv")?, b"a,b\n1,2\n", &empty).await?;
    store.set(&parse_key("reports/2025/summary.txt")?, b"fine", &empty).await?;
    let mut pending = MetadataRecord::new();
    pending.with_title("Q2 figures".to_owned());
    store
        .set_metadata(&parse_key("reports/q2.csv")?, &Metadata::MetadataRecord(pending))
        .await?;
    Ok(())
}

async fn check_listing(store: &dyn AsyncStore) -> TestResult {
    let reports = parse_key("reports")?;

    let mut names = store.listdir(&reports).await?;
    names.sort();
    assert_eq!(names, vec!["2025", "q1.csv", "q2.csv"]); // FAILS at HEAD on AsyncFileStore
    assert!(store.contains(&parse_key("reports/q2.csv")?).await?);

    let Metadata::MetadataRecord(record) = store.get_metadata(&reports).await? else {
        return Err("directory metadata should be a record".into());
    };
    let mut children: Vec<(String, bool)> = record
        .children
        .iter()
        .filter_map(|info| {
            let name = info.key.as_ref()?.filename()?.encode().to_string();
            Some((name, info.is_dir))
        })
        .collect();
    children.sort();
    assert_eq!(
        children,
        vec![("2025".into(), true), ("q1.csv".into(), false), ("q2.csv".into(), false)]
    );
    Ok(())
}

#[tokio::test]
async fn file_store_lists_children_and_metadata_only_keys() -> TestResult {
    let root = temp_root("file");
    tokio::fs::create_dir_all(&root).await?;
    let store = AsyncFileStore::new(root.to_string_lossy().as_ref(), &Key::new());
    populate(&store).await?;
    let outcome = check_listing(&store).await;
    let _ = tokio::fs::remove_dir_all(&root).await;
    outcome
}

#[tokio::test]
async fn memory_store_lists_children_and_metadata_only_keys() -> TestResult {
    let store = AsyncMemoryStore::new(&Key::new());
    populate(&store).await?;
    check_listing(&store).await
}
