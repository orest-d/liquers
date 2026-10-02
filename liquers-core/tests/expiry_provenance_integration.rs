//! Expiry provenance end to end: every route into `Expired` records why, in the same metadata
//! write as the status (Part C of `specs/design/dependency-audit-and-expiry-provenance/`, Phase 3
//! I5, implemented in Phase 4 Step 4).
//!
//! The fixture is the chain of `common::manager_scenarios`: `data/a.txt -> data/b.txt ->
//! data/report.txt`, where `report.txt` reads `b.txt` from inside its command so that a cascade
//! from `a.txt` reaches it in a second hop. Every call that takes the manager's key-mutation lock
//! goes through `within`, so a lock-discipline mistake fails in seconds instead of hanging.

mod common;
mod fixtures;

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use liquers_core::{
    assets::{AssetManager, AssetRef, AuditMode},
    context::{EnvRef, Environment, SimpleEnvironment},
    error::Error,
    metadata::{DependencyKey, ExpiryCause, ExpiryReason, LogEntryKind, Metadata, Status, Version},
    parse::parse_key,
    query::Key,
    recipes::DefaultRecipeProvider,
    store::{AsyncMemoryStore, AsyncStore},
    value::Value,
};
use fixtures::StoreSnapshot;

use common::manager_scenarios::{
    provenance_evaluate_chain, provenance_store, provenance_text_metadata,
    register_provenance_commands,
};

type TestEnv = SimpleEnvironment<Value>;
type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Fail fast instead of hanging if a locking call deadlocks.
async fn within<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(std::time::Duration::from_secs(10), future)
        .await
        .expect("call did not finish within 10 s: a lock held reentrantly?")
}

async fn provenance_env(with_a_recipe: bool) -> Result<EnvRef<TestEnv>, Error> {
    let mut env = TestEnv::new();
    register_provenance_commands(&mut env.command_registry);
    env.with_async_store(Box::new(provenance_store(with_a_recipe).await?));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    Ok(env.to_ref())
}

fn key(name: &str) -> Key {
    parse_key(name).expect("key")
}

fn dep(name: &str) -> DependencyKey {
    DependencyKey::from(&key(name))
}

/// The live (cached) asset of an already-evaluated key.
async fn live(envref: &EnvRef<TestEnv>, name: &str) -> Result<AssetRef<TestEnv>, Error> {
    let asset = envref.get_asset_manager().get(&key(name)).await?;
    let _ = asset.get().await?;
    Ok(asset)
}

async fn stored(envref: &EnvRef<TestEnv>, name: &str) -> Result<Metadata, Error> {
    envref.get_async_store().get_metadata(&key(name)).await
}

fn log_of(metadata: &Metadata) -> Vec<(LogEntryKind, String)> {
    match metadata {
        Metadata::MetadataRecord(mr) => mr.log.iter().map(|e| (e.kind.clone(), e.message.clone())).collect(),
        Metadata::LegacyMetadata(_) => Vec::new(),
    }
}

/// `set_binary(a.txt)` with the chain live: the first hop's `via` is the root, the second hop's is
/// its own direct dependency — on the live assets, which is what a reader in this process sees.
#[tokio::test]
async fn via_names_the_direct_dependency_on_a_two_step_cascade() -> TestResult {
    let envref = provenance_env(false).await?;
    let am = envref.get_asset_manager();
    within(am.set_binary(&key("data/a.txt"), b"hello", provenance_text_metadata())).await?;
    provenance_evaluate_chain(&envref).await?;
    let b = live(&envref, "data/b.txt").await?;
    let report = live(&envref, "data/report.txt").await?;

    within(am.set_binary(&key("data/a.txt"), b"changed", provenance_text_metadata())).await?;

    let cause = ExpiryCause::Updated {
        version: Version::from_content(b"changed"),
    };
    assert_eq!(
        b.get_metadata().await?.expiry_reason(),
        Some(ExpiryReason::Cascaded {
            cause: cause.clone(),
            root: dep("data/a.txt"),
            via: dep("data/a.txt"),
        })
    );
    assert_eq!(
        report.get_metadata().await?.expiry_reason(),
        Some(ExpiryReason::Cascaded {
            cause,
            root: dep("data/a.txt"),
            via: dep("data/b.txt"),
        })
    );
    Ok(())
}

/// The stored sidecar's last log entry is the reason's own log entry, written with `Expired`.
#[tokio::test]
async fn log_line_is_persisted_with_the_status() -> TestResult {
    let envref = provenance_env(false).await?;
    let am = envref.get_asset_manager();
    within(am.set_binary(&key("data/a.txt"), b"hello", provenance_text_metadata())).await?;
    provenance_evaluate_chain(&envref).await?;

    within(am.set_binary(&key("data/a.txt"), b"changed", provenance_text_metadata())).await?;

    for name in ["data/b.txt", "data/report.txt"] {
        let metadata = stored(&envref, name).await?;
        assert_eq!(metadata.status(), Status::Expired, "{name}");
        let Some(reason) = metadata.expiry_reason() else {
            panic!("{name} is stored Expired without a reason");
        };
        let expected = reason.log_entry(name);
        let log = log_of(&metadata);
        let Some((kind, message)) = log.last() else {
            panic!("{name} has an empty log");
        };
        assert_eq!(
            (kind, message.as_str()),
            (&expected.kind, expected.message.as_str())
        );
        assert_eq!(*kind, LogEntryKind::Info, "an Updated cascade is the contract working");
    }
    Ok(())
}

/// No `Asset <number>` anywhere in the log of an asset expired by a cascade; the expiry lines name
/// keys.
#[tokio::test]
async fn log_line_names_keys_not_asset_ids() -> TestResult {
    let envref = provenance_env(false).await?;
    let am = envref.get_asset_manager();
    within(am.set_binary(&key("data/a.txt"), b"hello", provenance_text_metadata())).await?;
    provenance_evaluate_chain(&envref).await?;
    let report = live(&envref, "data/report.txt").await?;

    within(am.set_binary(&key("data/a.txt"), b"changed", provenance_text_metadata())).await?;

    for metadata in [report.get_metadata().await?, stored(&envref, "data/report.txt").await?] {
        let log = log_of(&metadata);
        assert!(log.iter().any(|(_, m)| m
            == "data/report.txt expired: new content of -R/data/a.txt triggered a cascade \
                expiration via direct dependency -R/data/b.txt"));
        for (_, message) in &log {
            let lower = message.to_lowercase();
            let names_an_id = lower.match_indices("asset ").any(|(i, _)| {
                lower[i + "asset ".len()..]
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_digit())
            });
            assert!(!names_an_id, "log line names a runtime asset id: {message}");
        }
    }
    Ok(())
}

/// The integration twin of `log_line_format_per_cause`: an explicit expiry of a computed root.
/// The direct dependent's line omits `via` (it is the root); the second hop's names it.
#[tokio::test]
async fn two_step_cascade_log_names_root_and_via() -> TestResult {
    let envref = provenance_env(true).await?;
    provenance_evaluate_chain(&envref).await?;
    let a = live(&envref, "data/a.txt").await?;
    let b = live(&envref, "data/b.txt").await?;
    let report = live(&envref, "data/report.txt").await?;

    within(a.expire()).await?;

    let lines = |metadata: &Metadata| -> Vec<String> {
        log_of(metadata).into_iter().map(|(_, m)| m).collect()
    };
    assert!(lines(&a.get_metadata().await?)
        .contains(&"data/a.txt expired: expiration was requested explicitly".to_string()));
    assert!(lines(&b.get_metadata().await?).contains(
        &"data/b.txt expired: explicit expiration of -R/data/a.txt triggered a cascade expiration"
            .to_string()
    ));
    assert!(lines(&report.get_metadata().await?).contains(
        &"data/report.txt expired: explicit expiration of -R/data/a.txt triggered a cascade \
          expiration via direct dependency -R/data/b.txt"
            .to_string()
    ));
    Ok(())
}

/// `remove` of a `Source` cascades with `Removed`, at `Info`, and the source is gone.
#[tokio::test]
async fn removing_a_source_cascades_with_removed() -> TestResult {
    let envref = provenance_env(false).await?;
    let am = envref.get_asset_manager();
    within(am.set_binary(&key("data/a.txt"), b"hello", provenance_text_metadata())).await?;
    provenance_evaluate_chain(&envref).await?;

    within(am.remove(&key("data/a.txt"))).await?;

    assert!(!envref.get_async_store().contains(&key("data/a.txt")).await?);
    let b = stored(&envref, "data/b.txt").await?;
    assert_eq!(
        b.expiry_reason(),
        Some(ExpiryReason::Cascaded {
            cause: ExpiryCause::Removed,
            root: dep("data/a.txt"),
            via: dep("data/a.txt"),
        })
    );
    let report = stored(&envref, "data/report.txt").await?;
    assert_eq!(
        report.expiry_reason(),
        Some(ExpiryReason::Cascaded {
            cause: ExpiryCause::Removed,
            root: dep("data/a.txt"),
            via: dep("data/b.txt"),
        })
    );
    let Some((kind, _)) = log_of(&report).last().cloned() else {
        panic!("empty log");
    };
    assert_eq!(kind, LogEntryKind::Info);
    Ok(())
}

/// `set_binary` of a dependency cascades with `Updated { version: from_content(new bytes) }`;
/// storing the same bytes again expires nothing.
#[tokio::test]
async fn set_binary_of_a_dependency_cascades_with_updated() -> TestResult {
    let envref = provenance_env(false).await?;
    let am = envref.get_asset_manager();
    within(am.set_binary(&key("data/a.txt"), b"v1", provenance_text_metadata())).await?;
    provenance_evaluate_chain(&envref).await?;

    within(am.set_binary(&key("data/a.txt"), b"v3", provenance_text_metadata())).await?;

    assert_eq!(
        stored(&envref, "data/b.txt").await?.expiry_reason(),
        Some(ExpiryReason::Cascaded {
            cause: ExpiryCause::Updated {
                version: Version::from_content(b"v3"),
            },
            root: dep("data/a.txt"),
            via: dep("data/a.txt"),
        })
    );

    // Recompute on the new content, then write the same bytes: no version change, no cascade.
    provenance_evaluate_chain(&envref).await?;
    within(am.set_binary(&key("data/a.txt"), b"v3", provenance_text_metadata())).await?;
    assert_eq!(stored(&envref, "data/b.txt").await?.status(), Status::Ready);
    assert_eq!(stored(&envref, "data/report.txt").await?.status(), Status::Ready);
    Ok(())
}

/// Readers polling `get_metadata` while a cascade runs never see `Expired` without its reason:
/// status and reason are written under one `data` write lock.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn reader_never_sees_expired_without_reason() -> TestResult {
    let envref = provenance_env(false).await?;
    let am = envref.get_asset_manager();
    within(am.set_binary(&key("data/a.txt"), b"hello", provenance_text_metadata())).await?;
    provenance_evaluate_chain(&envref).await?;
    let watched = vec![
        live(&envref, "data/b.txt").await?,
        live(&envref, "data/report.txt").await?,
    ];

    let stop = Arc::new(AtomicBool::new(false));
    let torn = Arc::new(AtomicUsize::new(0));
    let seen_expired = Arc::new(AtomicUsize::new(0));
    let mut readers = Vec::new();
    for asset in watched.iter().cloned() {
        let (stop, torn, seen_expired) = (stop.clone(), torn.clone(), seen_expired.clone());
        readers.push(tokio::spawn(async move {
            let mut counted = false;
            while !stop.load(Ordering::SeqCst) {
                if let Ok(metadata) = asset.get_metadata().await {
                    if metadata.status() == Status::Expired {
                        if metadata.expiry_reason().is_none() {
                            torn.fetch_add(1, Ordering::SeqCst);
                        }
                        if !counted {
                            counted = true;
                            seen_expired.fetch_add(1, Ordering::SeqCst);
                        }
                    }
                }
                tokio::task::yield_now().await;
            }
        }));
    }

    within(am.set_binary(&key("data/a.txt"), b"changed", provenance_text_metadata())).await?;
    // Let every reader observe the final state at least once.
    for _ in 0..500 {
        if seen_expired.load(Ordering::SeqCst) == watched.len() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    stop.store(true, Ordering::SeqCst);
    for reader in readers {
        reader.await?;
    }

    assert_eq!(seen_expired.load(Ordering::SeqCst), watched.len());
    assert_eq!(torn.load(Ordering::SeqCst), 0, "a reader saw Expired without its reason");
    Ok(())
}

/// The audit expires the *dependents* of the key it audited; the audited key itself is the
/// evidence, not the casualty. After an audit that expires `b.txt` and `report.txt`, `a.txt`'s
/// status and reason are untouched — in the store and in the live asset.
#[tokio::test]
async fn audit_never_expires_the_root() -> TestResult {
    let snapshot = {
        let envref = provenance_env(false).await?;
        let am = envref.get_asset_manager();
        within(am.set_binary(&key("data/a.txt"), b"hello", provenance_text_metadata())).await?;
        provenance_evaluate_chain(&envref).await?;
        let keys: Vec<Key> = ["data/recipes.yaml", "data/a.txt", "data/b.txt", "data/report.txt"]
            .into_iter()
            .map(key)
            .collect();
        StoreSnapshot::capture(&envref.get_async_store(), &keys).await?
    };
    let store = AsyncMemoryStore::new(&Key::new());
    snapshot.replay_into(&store).await?;
    let mut a_metadata = store.get_metadata(&key("data/a.txt")).await?;
    a_metadata.set_version(Some(Version::new(0xB0_0B)))?;
    store.set_metadata(&key("data/a.txt"), &a_metadata).await?;
    let status_before = a_metadata.status();
    let mut env = TestEnv::new();
    register_provenance_commands(&mut env.command_registry);
    env.with_async_store(Box::new(store));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    let envref = env.to_ref();
    // `a.txt` stays unloaded: loading it would register its moved version and make `b.txt` refuse
    // to load, leaving the audit nothing to expire.
    let b = live(&envref, "data/b.txt").await?;
    let report_asset = live(&envref, "data/report.txt").await?;

    let report = within(envref.get_asset_manager().trigger_dependency_audit_with(
        &liquers_core::parse::parse_query("-R/data/b.txt")?,
        AuditMode::Expire,
    ))
    .await?;

    assert!(
        report.expired.contains(&dep("data/b.txt")) || b.status().await == Status::Expired,
        "precondition: the audit must have expired a dependent: {report:?}"
    );
    assert_eq!(b.status().await, Status::Expired);
    assert_eq!(report_asset.status().await, Status::Expired);
    assert!(!report.expired.contains(&dep("data/a.txt")), "{report:?}");
    let a = live(&envref, "data/a.txt").await?;
    assert_eq!(a.status().await, status_before, "the audited key keeps its status");
    assert_eq!(a.get_metadata().await?.expiry_reason(), None, "and has no expiry reason");
    let stored = stored(&envref, "data/a.txt").await?;
    assert_eq!(stored.status(), status_before);
    assert_eq!(stored.expiry_reason(), None);
    Ok(())
}
