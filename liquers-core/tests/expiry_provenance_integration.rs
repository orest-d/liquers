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
use common::minimal_manager::MinimalEnv;

use common::manager_scenarios::{
    provenance_evaluate_chain, provenance_store, provenance_text_metadata, register_gate_command,
    register_provenance_commands, stale_dependency_store, wait_until_stored, StaleGate,
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

// ---------------------------------------------------------------------------------------------
// Every route into `Expired`, driven from outside the crate (Step 10)
// ---------------------------------------------------------------------------------------------

/// One row per [`ExpiryCause`]: the way into `Expired` that produces it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Route {
    /// A recipe with `expires: "in 1 sec"`, expired by the queued monitor.
    Deadline,
    /// `AssetRef::expire` on a computed root.
    Explicit,
    /// A command that waits (`Context::wait_for_dependency`) for a dependency that expired after
    /// the command submitted it.
    StaleDependency,
    /// `trigger_dependency_audit_with` in a restarted environment whose root moved.
    Audit,
    /// `set_binary` of a dependency with new content.
    Updated,
    /// `remove` of a `Source`.
    Removed,
    /// A `Source` whose bytes were rewritten outside Liquers, found when it is read.
    UpdatedInStore,
}

impl Route {
    const ALL: [Route; 7] = [
        Route::Deadline,
        Route::Explicit,
        Route::StaleDependency,
        Route::Audit,
        Route::Updated,
        Route::Removed,
        Route::UpdatedInStore,
    ];
}

/// The route that produces `cause`. Exhaustive: a new `ExpiryCause` is a compile error here until
/// it has a row.
fn route_of(cause: &ExpiryCause) -> Route {
    match cause {
        ExpiryCause::Deadline { .. } => Route::Deadline,
        ExpiryCause::Explicit => Route::Explicit,
        ExpiryCause::StaleDependency { .. } => Route::StaleDependency,
        ExpiryCause::Audit { .. } => Route::Audit,
        ExpiryCause::Updated { .. } => Route::Updated,
        ExpiryCause::Removed => Route::Removed,
        ExpiryCause::UpdatedInStore { .. } => Route::UpdatedInStore,
    }
}

/// The level Phase 2 assigns to `route`'s cause: the contract working as designed is `Info`, a
/// stored assumption found false or a departure from the contract is `Warning`.
fn expected_level(route: Route) -> LogEntryKind {
    match route {
        Route::Deadline | Route::Explicit | Route::Updated | Route::Removed => LogEntryKind::Info,
        Route::StaleDependency | Route::Audit | Route::UpdatedInStore => LogEntryKind::Warning,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scope {
    /// The asset is the root of the cause.
    Direct,
    /// The cause reached the asset through `root` and `via`.
    Cascaded,
}

/// An asset the route expired, as the store holds it.
struct Observed {
    name: &'static str,
    scope: Scope,
    metadata: Metadata,
}

async fn observe(
    envref: &EnvRef<TestEnv>,
    name: &'static str,
    scope: Scope,
) -> Result<Observed, Box<dyn std::error::Error>> {
    within(wait_until_stored(envref, &key(name), Status::Expired)).await?;
    Ok(Observed {
        name,
        scope,
        metadata: stored(envref, name).await?,
    })
}

/// Process one of a restart: `a.txt` written, `b.txt` and `report.txt` computed and persisted.
async fn persisted_chain(a: &[u8]) -> Result<StoreSnapshot, Box<dyn std::error::Error>> {
    // The recipes file is re-seeded through `set_binary` so that it carries a content-hash
    // version, which a store sweep then finds verified.
    let seed = provenance_store(false).await?;
    let recipes = seed.get_bytes(&key("data/recipes.yaml")).await?;
    let mut env = TestEnv::new();
    register_provenance_commands(&mut env.command_registry);
    env.with_async_store(Box::new(AsyncMemoryStore::new(&Key::new())));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    let envref = env.to_ref();
    let am = envref.get_asset_manager();
    within(am.set_binary(&key("data/recipes.yaml"), &recipes, provenance_text_metadata())).await?;
    within(am.set_binary(&key("data/a.txt"), a, provenance_text_metadata())).await?;
    within(provenance_evaluate_chain(&envref)).await?;
    let keys: Vec<Key> = ["data/recipes.yaml", "data/a.txt", "data/b.txt", "data/report.txt"]
        .into_iter()
        .map(key)
        .collect();
    Ok(StoreSnapshot::capture(&envref.get_async_store(), &keys).await?)
}

/// A fresh store holding `snapshot`, with `edit` applied to it first: what a restarted process
/// finds. Only the bytes are rewritten, keeping the sidecar, as a text editor would.
async fn replayed_store(
    snapshot: &StoreSnapshot,
    edit: Option<(&str, &[u8])>,
) -> Result<AsyncMemoryStore, Error> {
    let store = AsyncMemoryStore::new(&Key::new());
    snapshot.replay_into(&store).await?;
    if let Some((name, bytes)) = edit {
        let metadata = store.get_metadata(&key(name)).await?;
        store.set(&key(name), bytes, &metadata).await?;
    }
    Ok(store)
}

async fn provenance_env_over(store: AsyncMemoryStore) -> Result<EnvRef<TestEnv>, Error> {
    let mut env = TestEnv::new();
    register_provenance_commands(&mut env.command_registry);
    env.with_async_store(Box::new(store));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    Ok(env.to_ref())
}

/// Drive `route` on the `a.txt -> b.txt -> report.txt` fixture and return the assets it expired,
/// each read from the store's metadata.
async fn drive(route: Route) -> Result<Vec<Observed>, Box<dyn std::error::Error>> {
    match route {
        Route::Deadline => {
            use liquers_core::expiration::Expires;
            use liquers_core::recipes::{Recipe, RecipeList};
            let mut rl = RecipeList::new();
            let mut root = Recipe::new("make_text/a.txt".to_string(), "A".into(), "expires".into())?;
            root.expires = Expires::InDuration(std::time::Duration::from_secs(1));
            rl.add_recipe(root);
            rl.add_recipe(Recipe::new(
                "-R/data/a.txt/-/upper/b.txt".to_string(),
                "B".into(),
                "depends on a.txt".into(),
            )?);
            rl.add_recipe(Recipe::new(
                "summarize/report.txt".to_string(),
                "Report".into(),
                "depends on b.txt".into(),
            )?);
            let yaml = serde_yaml::to_string(&rl)
                .map_err(|e| Error::general_error(format!("recipes.yaml: {e}")))?;
            let store = AsyncMemoryStore::new(&Key::new());
            store
                .set(&key("data/recipes.yaml"), yaml.as_bytes(), &Metadata::new())
                .await?;
            let envref = provenance_env_over(store).await?;
            within(provenance_evaluate_chain(&envref)).await?;
            Ok(vec![
                observe(&envref, "data/a.txt", Scope::Direct).await?,
                observe(&envref, "data/b.txt", Scope::Cascaded).await?,
                observe(&envref, "data/report.txt", Scope::Cascaded).await?,
            ])
        }
        Route::Explicit => {
            let envref = provenance_env(true).await?;
            within(provenance_evaluate_chain(&envref)).await?;
            let a = live(&envref, "data/a.txt").await?;
            within(wait_until_stored(&envref, &key("data/a.txt"), Status::Ready)).await?;
            within(a.expire()).await?;
            Ok(vec![
                observe(&envref, "data/a.txt", Scope::Direct).await?,
                observe(&envref, "data/b.txt", Scope::Cascaded).await?,
                observe(&envref, "data/report.txt", Scope::Cascaded).await?,
            ])
        }
        Route::StaleDependency => {
            let gate = StaleGate::new();
            let mut env = TestEnv::new();
            register_provenance_commands(&mut env.command_registry);
            register_gate_command(&mut env.command_registry, gate.clone());
            env.with_async_store(Box::new(stale_dependency_store().await?));
            env.with_recipe_provider(Box::new(DefaultRecipeProvider));
            let envref = env.to_ref();
            let am = envref.get_asset_manager();
            let evaluation = {
                let (envref, gated) = (envref.clone(), key("data/gated.txt"));
                tokio::spawn(async move {
                    let asset = envref.get_asset_manager().get(&gated).await?;
                    let _ = asset.get().await;
                    Ok::<_, Error>(())
                })
            };
            within(gate.entered.notified()).await;
            let dependency = within(am.get(&key("data/a.txt"))).await?;
            within(dependency.expire()).await?;
            gate.release.notify_one();
            within(evaluation).await??;
            Ok(vec![observe(&envref, "data/gated.txt", Scope::Direct).await?])
        }
        Route::Audit => {
            let snapshot = persisted_chain(b"hello").await?;
            let store = replayed_store(&snapshot, None).await?;
            let mut a_metadata = store.get_metadata(&key("data/a.txt")).await?;
            a_metadata.set_version(Some(Version::new(0xB0_0B)))?;
            store.set_metadata(&key("data/a.txt"), &a_metadata).await?;
            let envref = provenance_env_over(store).await?;
            // `a.txt` stays unloaded: loading it would register its moved version and make
            // `b.txt` refuse to load, leaving the audit nothing to expire.
            let _b = live(&envref, "data/b.txt").await?;
            let _report = live(&envref, "data/report.txt").await?;
            within(envref.get_asset_manager().trigger_dependency_audit_with(
                &liquers_core::parse::parse_query("-R/data/b.txt")?,
                AuditMode::Expire,
            ))
            .await?;
            Ok(vec![
                observe(&envref, "data/b.txt", Scope::Cascaded).await?,
                observe(&envref, "data/report.txt", Scope::Cascaded).await?,
            ])
        }
        Route::Updated => {
            let envref = provenance_env(false).await?;
            let am = envref.get_asset_manager();
            within(am.set_binary(&key("data/a.txt"), b"hello", provenance_text_metadata())).await?;
            provenance_evaluate_chain(&envref).await?;
            within(am.set_binary(&key("data/a.txt"), b"changed", provenance_text_metadata())).await?;
            Ok(vec![
                observe(&envref, "data/b.txt", Scope::Cascaded).await?,
                observe(&envref, "data/report.txt", Scope::Cascaded).await?,
            ])
        }
        Route::Removed => {
            let envref = provenance_env(false).await?;
            let am = envref.get_asset_manager();
            within(am.set_binary(&key("data/a.txt"), b"hello", provenance_text_metadata())).await?;
            provenance_evaluate_chain(&envref).await?;
            within(am.remove(&key("data/a.txt"))).await?;
            Ok(vec![
                observe(&envref, "data/b.txt", Scope::Cascaded).await?,
                observe(&envref, "data/report.txt", Scope::Cascaded).await?,
            ])
        }
        Route::UpdatedInStore => {
            let snapshot = persisted_chain(b"hello").await?;
            let store =
                replayed_store(&snapshot, Some(("data/a.txt", b"hello, edited by hand"))).await?;
            let envref = provenance_env_over(store).await?;
            // Serve `b.txt` as stored (loading the edge a -> b at the old version), then read the
            // edited source: its new content hash is found different from the recorded one.
            let _ = live(&envref, "data/b.txt").await?;
            let _ = live(&envref, "data/report.txt").await?;
            let _ = live(&envref, "data/a.txt").await?;
            Ok(vec![
                observe(&envref, "data/b.txt", Scope::Cascaded).await?,
                observe(&envref, "data/report.txt", Scope::Cascaded).await?,
            ])
        }
    }
}

/// One scenario per route: whatever the route expired is stored `Expired` with a reason of that
/// route's cause, at the scope the route has — `Deadline`, `Explicit` and `StaleDependency` are
/// `Direct` on the asset they happened to and `Cascaded` on its dependents; the other four occur
/// only as `Cascaded`, on dependents of a root that is not itself expired.
#[tokio::test]
async fn every_route_persists_its_reason() -> TestResult {
    let mut seen = Vec::new();
    for route in Route::ALL {
        let observed = drive(route).await?;
        assert!(!observed.is_empty(), "{route:?} expired nothing");
        for Observed { name, scope, metadata } in observed {
            assert_eq!(metadata.status(), Status::Expired, "{route:?}: {name}");
            let Some(reason) = metadata.expiry_reason() else {
                panic!("{route:?}: {name} is stored Expired without a reason");
            };
            match (scope, &reason) {
                (Scope::Direct, ExpiryReason::Direct { cause }) => {
                    assert_eq!(route_of(cause), route, "{route:?}: {name}: {reason:?}");
                    if let ExpiryCause::StaleDependency { dependency } = cause {
                        assert_eq!(*dependency, dep("data/a.txt"), "{route:?}: {name}");
                    }
                }
                (Scope::Cascaded, ExpiryReason::Cascaded { cause, root, .. }) => {
                    assert_eq!(route_of(cause), route, "{route:?}: {name}: {reason:?}");
                    assert_eq!(*root, dep("data/a.txt"), "{route:?}: {name}");
                }
                (Scope::Direct, ExpiryReason::Cascaded { .. })
                | (Scope::Cascaded, ExpiryReason::Direct { .. }) => {
                    panic!("{route:?}: {name}: expected scope {scope:?}, stored {reason:?}");
                }
            }
            seen.push(route_of(reason.cause()));
        }
    }
    for route in Route::ALL {
        assert!(seen.contains(&route), "no stored reason carried the {route:?} cause");
    }
    Ok(())
}

/// Every cause writes its log line, in the same write as the status: the stored log holds exactly
/// one line that is the reason's own `log_entry`, at the level Phase 2 gives the cause.
#[tokio::test]
async fn every_cause_writes_a_log_line() -> TestResult {
    for route in Route::ALL {
        for Observed { name, metadata, .. } in drive(route).await? {
            let Some(reason) = metadata.expiry_reason() else {
                panic!("{route:?}: {name} is stored Expired without a reason");
            };
            let expected = reason.log_entry(name);
            assert_eq!(expected.kind, expected_level(route), "{route:?}: the level of the cause");
            let matching: Vec<_> = log_of(&metadata)
                .into_iter()
                .filter(|(_, message)| message.contains(" expired: "))
                .collect();
            assert_eq!(
                matching,
                vec![(expected.kind.clone(), expected.message.clone())],
                "{route:?}: {name}: exactly one expiry line, the reason's own"
            );
            assert!(
                !expected.message.to_lowercase().contains("asset "),
                "{route:?}: {name}: the line names a key, not a runtime asset: {}",
                expected.message
            );
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Phase 3 I5 (Part C): `record_expiry` is the single writer, whatever the route.
// ---------------------------------------------------------------------------------------------

/// `data/a.txt` with three dependents, each `upper` over `a.txt`: `held` (live, a handle kept),
/// `finished` (live, evaluated and let go) and `stored_only` (live asset unmapped, graph edge and
/// stored copy kept).
async fn recording_store() -> Result<AsyncMemoryStore, Error> {
    use liquers_core::recipes::{Recipe, RecipeList};
    let mut recipes = RecipeList::new();
    for name in ["held", "finished", "stored_only"] {
        recipes.add_recipe(Recipe::new(
            format!("-R/data/a.txt/-/upper/{name}.txt"),
            name.into(),
            "depends on a.txt".into(),
        )?);
    }
    let yaml = serde_yaml::to_string(&recipes)
        .map_err(|e| Error::general_error(format!("recipes.yaml: {e}")))?;
    let store = AsyncMemoryStore::new(&Key::new());
    store
        .set(&key("data/recipes.yaml"), yaml.as_bytes(), &Metadata::new())
        .await?;
    Ok(store)
}

/// A manager that overrides `record_expiry` and counts its calls sees exactly one call per
/// expired asset: three dependents, none for the root, and the stored-only copy included.
#[tokio::test]
async fn record_expiry_is_called_for_every_expired_asset() -> TestResult {
    let mut env = MinimalEnv::new();
    register_provenance_commands(&mut env.command_registry);
    env.with_async_store(Box::new(recording_store().await?));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    let envref = env.to_ref();
    let mgr = envref.get_asset_manager();

    let a = key("data/a.txt");
    within(mgr.set_binary(&a, b"hello", provenance_text_metadata())).await?;
    let held = within(mgr.get(&key("data/held.txt"))).await?;
    assert_eq!(held.get().await?.try_into_string()?, "HELLO");
    {
        let finished = within(mgr.get(&key("data/finished.txt"))).await?;
        assert_eq!(finished.get().await?.try_into_string()?, "HELLO");
    }
    let stored_only = key("data/stored_only.txt");
    let asset = within(mgr.get(&stored_only)).await?;
    assert_eq!(asset.get().await?.try_into_string()?, "HELLO");
    drop(asset);
    for name in ["held", "finished", "stored_only"] {
        wait_until_stored(&envref, &key(&format!("data/{name}.txt")), Status::Ready).await?;
    }
    mgr.remove_key_asset(&stored_only).await;
    assert!(mgr.lookup_key_asset(&stored_only).is_none(), "precondition: no live asset");
    let before = mgr.recorded_expiries().len();

    within(mgr.set_binary(&a, b"changed", provenance_text_metadata())).await?;

    let calls = mgr.recorded_expiries()[before..].to_vec();
    let mut subjects: Vec<&str> = calls.iter().map(|(subject, _)| subject.as_str()).collect();
    subjects.sort();
    assert_eq!(
        subjects,
        ["data/finished.txt", "data/held.txt", "data/stored_only.txt"],
        "one call per expired asset, none for the root: {calls:?}"
    );
    let root = dep("data/a.txt");
    let expected = ExpiryReason::Cascaded {
        cause: ExpiryCause::Updated {
            version: Version::from_content(b"changed"),
        },
        root: root.clone(),
        via: root,
    };
    for (subject, reason) in &calls {
        assert_eq!(reason, &expected, "{subject}");
    }
    assert_eq!(held.status().await, Status::Expired);
    assert_eq!(held.get_metadata().await?.expiry_reason(), Some(expected.clone()));

    // The stored-only copy was reached too: its sidecar carries the reason and the log line the
    // method wrote.
    let sidecar = envref.get_async_store().get_metadata(&stored_only).await?;
    assert_eq!(sidecar.status(), Status::Expired);
    assert_eq!(sidecar.expiry_reason(), Some(expected.clone()));
    let line = expected.log_entry("data/stored_only.txt");
    let matching = log_of(&sidecar)
        .into_iter()
        .filter(|(kind, message)| *kind == line.kind && *message == line.message)
        .count();
    assert_eq!(matching, 1, "exactly one expiry line in the sidecar");
    // The root is not an expired asset.
    assert_eq!(envref.get_async_store().get_metadata(&a).await?.status(), Status::Source);
    Ok(())
}
