//! Dependency audit across environments and restarts
//! (`specs/design/dependency-audit-and-expiry-provenance/`, Phase 3 I1).
//!
//! The audits here compare a dependency's *durable* version with what its dependents recorded,
//! in an environment that has just "restarted": a second environment over a replayed snapshot of
//! the first one's store, so its dependency manager holds no versions at all.

mod common;
mod fixtures;

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use liquers_core::{
    assets::{AssetManager, AssetRef, AuditMode, AuditReport},
    command_metadata::CommandKey,
    context::{EnvRef, Environment, SimpleEnvironment},
    environment_builder::{AssetManagerOptions, DependencyAuditPolicy, EnvironmentBuilder, Queued},
    environment_config::EnvironmentConfig,
    error::Error,
    metadata::{DependencyKey, DependencyRecord, ExpiryCause, ExpiryReason, LogEntryKind, Metadata, Status, Version},
    parse::{parse_key, parse_query},
    query::Key,
    recipes::{DefaultRecipeProvider, Recipe, RecipeList},
    state::State,
    store::{AsyncMemoryStore, AsyncStore},
    value::Value,
};

use common::manager_scenarios::{
    provenance_evaluate_chain, provenance_store, provenance_text_metadata,
    register_provenance_commands,
};
use fixtures::StoreSnapshot;

type TestEnv = SimpleEnvironment<Value>;

fn env_over(store: AsyncMemoryStore) -> EnvRef<TestEnv> {
    let mut env = TestEnv::new();
    register_provenance_commands(&mut env.command_registry);
    env.with_async_store(Box::new(store));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    env.to_ref()
}

/// Corner case 5: a reason one environment persisted is what a second environment over the same
/// stored data reads — through the store and through `get_asset_info` — with nothing live in it.
#[tokio::test]
async fn two_envs_share_persisted_reason() -> Result<(), Box<dyn std::error::Error>> {
    let a = parse_key("data/a.txt")?;
    let b = parse_key("data/b.txt")?;
    let report = parse_key("data/report.txt")?;
    let keys: Vec<Key> = vec![parse_key("data/recipes.yaml")?, a.clone(), b.clone(), report.clone()];

    let snapshot = {
        let envref = env_over(provenance_store(true).await?);
        provenance_evaluate_chain(&envref).await?;
        envref.get_asset_manager().expire(&a).await?;
        StoreSnapshot::capture(&envref.get_async_store(), &keys).await?
    };

    let second_store = AsyncMemoryStore::new(&Key::new());
    snapshot.replay_into(&second_store).await?;
    let envref2 = env_over(second_store);
    let am2 = envref2.get_asset_manager();

    let root = DependencyKey::from(&a);
    let expected: [(&Key, ExpiryReason); 3] = [
        (
            &a,
            ExpiryReason::Direct {
                cause: ExpiryCause::Explicit,
            },
        ),
        (
            &b,
            ExpiryReason::Cascaded {
                cause: ExpiryCause::Explicit,
                root: root.clone(),
                via: root.clone(),
            },
        ),
        (
            &report,
            ExpiryReason::Cascaded {
                cause: ExpiryCause::Explicit,
                root,
                via: DependencyKey::from(&b),
            },
        ),
    ];
    for (key, reason) in expected {
        let stored = envref2.get_async_store().get_metadata(key).await?;
        assert_eq!(stored.status(), Status::Expired, "{key}");
        assert_eq!(stored.expiry_reason(), Some(reason.clone()), "{key}");
        let info = am2.get_asset_info(key).await?;
        assert_eq!(info.status, Status::Expired, "{key}");
        assert_eq!(info.expiry_reason, Some(reason), "{key}: AssetInfo projects the reason");
    }
    Ok(())
}

// ======================================================================================
// Audits after a restart (Step 6 of the dependency-audit design).
// ======================================================================================

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Fail fast instead of hanging if a call deadlocks.
async fn within<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(std::time::Duration::from_secs(20), future)
        .await
        .expect("call did not finish within 20 s")
}

/// The commands of the provenance chain, counting every run, so a test can assert that an audit
/// ran nothing. Same names and (default) versions as `register_provenance_commands`, which is what
/// lets the first and the second environment disagree about nothing but the counter.
fn register_counting_commands(env: &mut TestEnv, calls: Arc<AtomicUsize>) {
    register_counting_commands_in(&mut env.command_registry, calls)
}

fn register_counting_commands_in(
    cr: &mut liquers_core::commands::CommandRegistry<TestEnv>,
    calls: Arc<AtomicUsize>,
) {
    let upper_calls = calls.clone();
    cr
        .register_command(
            CommandKey::new_name("upper"),
            move |state: &State<Value>, _args, _ctx| -> Result<Value, Error> {
                upper_calls.fetch_add(1, Ordering::SeqCst);
                Ok(Value::from(state.try_into_string()?.to_uppercase()))
            },
        )
        .expect("register upper");
    cr
        .register_command(
            CommandKey::new_name("make_text"),
            |_state, _args, _ctx| -> Result<Value, Error> { Ok(Value::from("generated")) },
        )
        .expect("register make_text");
    cr
        .register_async_command(CommandKey::new_name("summarize"), move |_state, _args, ctx| {
            let calls = calls.clone();
            Box::pin(async move {
                calls.fetch_add(1, Ordering::SeqCst);
                let b = ctx
                    .get_dependency_state(&parse_query("-R/data/b.txt")?)
                    .await?
                    .try_into_string()?;
                Ok(Value::from(format!("summary of {b}")))
            })
        })
        .expect("register summarize");
}

fn counting_env(store: Box<dyn AsyncStore>, calls: Arc<AtomicUsize>) -> EnvRef<TestEnv> {
    let mut env = TestEnv::new();
    register_counting_commands(&mut env, calls);
    env.with_async_store(store);
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    env.to_ref()
}

fn chain_keys() -> Result<Vec<Key>, Error> {
    ["data/recipes.yaml", "data/a.txt", "data/b.txt", "data/report.txt"]
        .into_iter()
        .map(parse_key)
        .collect()
}

fn dep(name: &str) -> DependencyKey {
    DependencyKey::from(&parse_key(name).expect("key"))
}

/// Process one: write `a.txt`, compute `b.txt` and `report.txt`, and return what was persisted
/// together with the version `a.txt` carried.
async fn first_process() -> Result<(StoreSnapshot, Version), Box<dyn std::error::Error>> {
    let envref = counting_env(
        Box::new(provenance_store(false).await?),
        Arc::new(AtomicUsize::new(0)),
    );
    let a = parse_key("data/a.txt")?;
    within(envref.get_asset_manager().set_binary(&a, b"hello", provenance_text_metadata())).await?;
    provenance_evaluate_chain(&envref).await?;
    let store = envref.get_async_store();
    let a_version = store
        .get_metadata(&a)
        .await?
        .version()
        .expect("a.txt is stored with a version");
    Ok((StoreSnapshot::capture(&store, &chain_keys()?).await?, a_version))
}

/// Process two: replay the snapshot into a fresh store, then let `tamper` rewrite the stored
/// versions of dependencies before anything is loaded.
async fn second_process(
    snapshot: &StoreSnapshot,
    changed: &[(&str, Version)],
    calls: Arc<AtomicUsize>,
) -> Result<EnvRef<TestEnv>, Box<dyn std::error::Error>> {
    second_process_with(snapshot, changed, calls, DependencyAuditPolicy::Explicit).await
}

/// [`second_process`] under a given audit policy.
async fn second_process_with(
    snapshot: &StoreSnapshot,
    changed: &[(&str, Version)],
    calls: Arc<AtomicUsize>,
    policy: DependencyAuditPolicy,
) -> Result<EnvRef<TestEnv>, Box<dyn std::error::Error>> {
    let store = AsyncMemoryStore::new(&Key::new());
    snapshot.replay_into(&store).await?;
    for (name, version) in changed {
        let key = parse_key(name)?;
        let mut metadata = store.get_metadata(&key).await?;
        metadata.set_version(Some(*version))?;
        store.set_metadata(&key, &metadata).await?;
    }
    let mut builder = EnvironmentBuilder::<Value, (), Queued>::new()
        .with_asset_manager_options(AssetManagerOptions::default().with_dependency_audit(policy))
        .with_async_store(Arc::new(store))
        .with_recipe_provider(Arc::new(DefaultRecipeProvider));
    register_counting_commands_in(&mut builder.command_registry, calls);
    Ok(builder.build()?)
}

/// Load `name` the way a restarted service would: evaluate it and wait. It must fast-track, which
/// is what registers its recorded edges.
async fn load(envref: &EnvRef<TestEnv>, name: &str) -> Result<AssetRef<TestEnv>, Box<dyn std::error::Error>> {
    let asset = envref.evaluate(&format!("-R/{name}")).await?;
    let _ = asset.get().await?;
    assert_eq!(asset.status().await, Status::Ready, "precondition: {name} is served as stored");
    Ok(asset)
}

fn moved() -> Version {
    Version::new(0xB0_0B)
}

/// The withdrawn test R2 of `AUDIT-CANNOT-EXPIRE-ON-A-FIRST-OBSERVED-VERSION`: the dependency's
/// durable version moved, the fresh process never held a version for it, and the audit must still
/// expire the dependent.
#[tokio::test]
async fn audit_after_restart_expires_dependent() -> TestResult {
    let (snapshot, a_version) = first_process().await?;
    let envref = second_process(&snapshot, &[("data/a.txt", moved())], Arc::new(AtomicUsize::new(0))).await?;
    let b = load(&envref, "data/b.txt").await?;

    let report = within(
        envref
            .get_asset_manager()
            .trigger_dependency_audit(&parse_query("-R/data/b.txt")?),
    )
    .await?;

    assert_eq!(report.checked, vec![dep("data/a.txt")]);
    assert_eq!(report.expired, vec![dep("data/b.txt")]);
    assert_eq!(
        report.findings.len(),
        1,
        "one stale edge: {report:?}"
    );
    assert_eq!(report.findings[0].dependency, dep("data/a.txt"));
    assert_eq!(report.findings[0].dependent, dep("data/b.txt"));
    assert_eq!(report.findings[0].expected, a_version);
    assert_eq!(report.findings[0].found, moved());
    assert_eq!(b.status().await, Status::Expired);
    assert_eq!(
        b.get_metadata().await?.expiry_reason(),
        Some(ExpiryReason::Cascaded {
            cause: ExpiryCause::Audit { found: moved() },
            root: dep("data/a.txt"),
            via: dep("data/a.txt"),
        })
    );
    Ok(())
}

/// An audit reads versions; it never runs a command. Not while loading, not while expiring.
#[tokio::test]
async fn audit_never_evaluates() -> TestResult {
    let (snapshot, _) = first_process().await?;
    let calls = Arc::new(AtomicUsize::new(0));
    let envref = second_process(&snapshot, &[("data/a.txt", moved())], calls.clone()).await?;
    let _b = load(&envref, "data/b.txt").await?;
    let _report = load(&envref, "data/report.txt").await?;
    assert_eq!(calls.load(Ordering::SeqCst), 0, "precondition: loading ran nothing");

    let audit = within(envref.get_asset_manager().trigger_dependency_audit_all_registered()).await?;

    assert!(!audit.expired.is_empty(), "the audit must have acted: {audit:?}");
    assert_eq!(calls.load(Ordering::SeqCst), 0, "an audit must not evaluate anything");
    Ok(())
}

/// `ReportOnly` names the stale edges and changes nothing: no status, no reason, no log line, no
/// stored byte.
#[tokio::test]
async fn report_only_audit_changes_nothing() -> TestResult {
    let (snapshot, a_version) = first_process().await?;
    let envref = second_process(&snapshot, &[("data/a.txt", moved())], Arc::new(AtomicUsize::new(0))).await?;
    let b = load(&envref, "data/b.txt").await?;
    let store = envref.get_async_store();
    let b_key = parse_key("data/b.txt")?;
    let stored_before = format!("{:?}", store.get_metadata(&b_key).await?);
    let live_before = format!("{:?}", b.get_metadata().await?);

    let report = within(
        envref
            .get_asset_manager()
            .trigger_dependency_audit_with(&parse_query("-R/data/b.txt")?, AuditMode::ReportOnly),
    )
    .await?;

    assert!(report.expired.is_empty(), "{report:?}");
    assert_eq!(report.findings.len(), 1, "{report:?}");
    assert_eq!(report.findings[0].dependent, dep("data/b.txt"));
    assert_eq!(report.findings[0].expected, a_version);
    assert_eq!(report.findings[0].found, moved());
    assert_eq!(b.status().await, Status::Ready);
    assert_eq!(format!("{:?}", b.get_metadata().await?), live_before);
    assert_eq!(format!("{:?}", store.get_metadata(&b_key).await?), stored_before);

    // The same call over everything the manager knows of is just as inert.
    let all = within(
        envref
            .get_asset_manager()
            .trigger_dependency_audit_all_registered_with(AuditMode::ReportOnly),
    )
    .await?;
    assert!(all.expired.is_empty(), "{all:?}");
    assert_eq!(b.status().await, Status::Ready);
    assert_eq!(format!("{:?}", store.get_metadata(&b_key).await?), stored_before);

    // And it registered nothing: a later `Expire` audit still finds the gap and acts on it.
    let expire = within(
        envref
            .get_asset_manager()
            .trigger_dependency_audit(&parse_query("-R/data/b.txt")?),
    )
    .await?;
    assert_eq!(expire.expired, vec![dep("data/b.txt")], "{expire:?}");
    Ok(())
}

/// The expiry runs the whole cascade, not only the audited key's direct dependents, while the
/// findings stay the direct edges.
#[tokio::test]
async fn audit_expires_transitive_dependents() -> TestResult {
    let (snapshot, _) = first_process().await?;
    let envref = second_process(&snapshot, &[("data/a.txt", moved())], Arc::new(AtomicUsize::new(0))).await?;
    let b = load(&envref, "data/b.txt").await?;
    let report_asset = load(&envref, "data/report.txt").await?;

    let report = within(
        envref
            .get_asset_manager()
            .trigger_dependency_audit(&parse_query("-R/data/b.txt")?),
    )
    .await?;

    assert_eq!(b.status().await, Status::Expired);
    assert_eq!(report_asset.status().await, Status::Expired, "the second hop is reached");
    assert!(report.expired.contains(&dep("data/b.txt")), "{report:?}");
    assert!(report.expired.contains(&dep("data/report.txt")), "{report:?}");
    assert_eq!(report.findings.len(), 1, "findings are the direct edges only: {report:?}");
    assert_eq!(
        report_asset.get_metadata().await?.expiry_reason(),
        Some(ExpiryReason::Cascaded {
            cause: ExpiryCause::Audit { found: moved() },
            root: dep("data/a.txt"),
            via: dep("data/b.txt"),
        })
    );
    Ok(())
}

/// Every stale edge is listed, with what the dependent expected and what the store holds now; an
/// edge that still matches is not.
#[tokio::test]
async fn audit_report_lists_all_findings() -> TestResult {
    // `report.txt` and `c.txt` both read `b.txt`, and `b.txt` reads `a.txt`.
    let mut recipes = RecipeList::new();
    recipes.add_recipe(Recipe::new("-R/data/a.txt/-/upper/b.txt".to_string(), "B".into(), "".into())?);
    recipes.add_recipe(Recipe::new("-R/data/a.txt/-/upper/c.txt".to_string(), "C".into(), "".into())?);
    recipes.add_recipe(Recipe::new("-R/data/a.txt/-/upper/d.txt".to_string(), "D".into(), "".into())?);
    let source = AsyncMemoryStore::new(&Key::new());
    source
        .set(&parse_key("data/recipes.yaml")?, serde_yaml::to_string(&recipes)?.as_bytes(), &Metadata::new())
        .await?;
    let first = counting_env(Box::new(source), Arc::new(AtomicUsize::new(0)));
    let am = first.get_asset_manager();
    within(am.set_binary(&parse_key("data/a.txt")?, b"hello", provenance_text_metadata())).await?;
    for name in ["b", "c", "d"] {
        let asset = am.get(&parse_key(&format!("data/{name}.txt"))?).await?;
        let _ = asset.get().await?;
        common::manager_scenarios::wait_until_stored(&first, &parse_key(&format!("data/{name}.txt"))?, Status::Ready)
            .await?;
    }
    let keys: Vec<Key> = ["data/recipes.yaml", "data/a.txt", "data/b.txt", "data/c.txt", "data/d.txt"]
        .into_iter()
        .map(parse_key)
        .collect::<Result<_, _>>()?;
    let snapshot = StoreSnapshot::capture(&first.get_async_store(), &keys).await?;
    let a_version = first.get_async_store().get_metadata(&parse_key("data/a.txt")?).await?.version();

    let store = AsyncMemoryStore::new(&Key::new());
    snapshot.replay_into(&store).await?;
    let envref = counting_env(Box::new(store), Arc::new(AtomicUsize::new(0)));
    for name in ["data/b.txt", "data/c.txt", "data/d.txt"] {
        load(&envref, name).await?;
    }
    // Only `a.txt` is audited here, so changing its stored version makes every dependent stale.
    envref
        .get_async_store()
        .set_metadata(&parse_key("data/a.txt")?, &{
            let mut m = envref.get_async_store().get_metadata(&parse_key("data/a.txt")?).await?;
            m.set_version(Some(moved()))?;
            m
        })
        .await?;

    let report = within(
        envref
            .get_asset_manager()
            .trigger_dependency_audit_all_registered_with(AuditMode::ReportOnly),
    )
    .await?;

    let mut dependents: Vec<&str> = report.findings.iter().map(|f| f.dependent.as_str()).collect();
    dependents.sort();
    assert_eq!(dependents, vec!["-R/data/b.txt", "-R/data/c.txt", "-R/data/d.txt"], "{report:?}");
    for finding in &report.findings {
        assert_eq!(finding.dependency, dep("data/a.txt"));
        assert_eq!(Some(finding.expected), a_version);
        assert_eq!(finding.found, moved());
    }

    // A fully matching graph yields no findings.
    let quiet = second_process(&snapshot_of_chain().await?, &[], Arc::new(AtomicUsize::new(0))).await?;
    load(&quiet, "data/b.txt").await?;
    let nothing = within(quiet.get_asset_manager().trigger_dependency_audit_all_registered_with(AuditMode::ReportOnly)).await?;
    assert!(nothing.findings.is_empty(), "{nothing:?}");
    Ok(())
}

async fn snapshot_of_chain() -> Result<StoreSnapshot, Box<dyn std::error::Error>> {
    Ok(first_process().await?.0)
}

/// A 100-link chain is expired by one audit of its first dependent. The cascade is a walk, not a
/// recursion, so the depth costs nothing.
///
/// The persisted chain is written link by link instead of being computed: evaluating a hundred
/// links records every upstream link on every downstream one, which is a cost this test is not
/// about. Each link here depends on its predecessor alone, so `via` is a different key at every
/// hop.
#[tokio::test]
async fn cascade_over_100_link_chain() -> TestResult {
    const LINKS: usize = 100;
    // Each link's version is the content hash of its bytes, as Liquers writes it: a version that
    // does not fingerprint the stored bytes would be taken for an edit made outside Liquers
    // when the link is loaded (Part G), which is not what this test is about.
    fn link_bytes(i: usize) -> Vec<u8> {
        format!("x{i}").into_bytes()
    }
    fn link_version(i: usize) -> Version {
        Version::from_content(&link_bytes(i))
    }
    let store = AsyncMemoryStore::new(&Key::new());
    for i in 0..=LINKS {
        let mut record = provenance_text_metadata();
        record.status = Status::Ready;
        record.version = Some(link_version(i));
        if i > 0 {
            record.dependencies.push(DependencyRecord::new(
                dep(&format!("data/l{}.txt", i - 1)),
                link_version(i - 1),
            ));
        }
        store
            .set(&parse_key(&format!("data/l{i}.txt"))?, &link_bytes(i), &Metadata::MetadataRecord(record))
            .await?;
    }
    let mut head = store.get_metadata(&parse_key("data/l0.txt")?).await?;
    head.set_version(Some(moved()))?;
    store.set_metadata(&parse_key("data/l0.txt")?, &head).await?;
    let envref = counting_env(Box::new(store), Arc::new(AtomicUsize::new(0)));
    let mut loaded = Vec::new();
    for i in 1..=LINKS {
        loaded.push(load(&envref, &format!("data/l{i}.txt")).await?);
    }

    let report = within(
        envref
            .get_asset_manager()
            .trigger_dependency_audit(&parse_query("-R/data/l1.txt")?),
    )
    .await?;

    assert_eq!(report.expired.len(), LINKS, "every link is expired exactly once");
    for (i, asset) in loaded.iter().enumerate() {
        assert_eq!(asset.status().await, Status::Expired, "link {}", i + 1);
    }
    let last = envref
        .get_async_store()
        .get_metadata(&parse_key(&format!("data/l{LINKS}.txt"))?)
        .await?;
    assert_eq!(
        last.expiry_reason(),
        Some(ExpiryReason::Cascaded {
            cause: ExpiryCause::Audit { found: moved() },
            root: dep("data/l0.txt"),
            via: dep(&format!("data/l{}.txt", LINKS - 1)),
        })
    );
    Ok(())
}

/// Two audits racing over the same graph expire each dependent once between them: one expiry
/// reason, one expiry log line.
#[tokio::test]
async fn concurrent_audits_do_not_double_expire() -> TestResult {
    let (snapshot, _) = first_process().await?;
    let envref = second_process(&snapshot, &[("data/a.txt", moved())], Arc::new(AtomicUsize::new(0))).await?;
    let b = load(&envref, "data/b.txt").await?;
    let am = envref.get_asset_manager();
    let query = parse_query("-R/data/b.txt")?;

    let (first, second) = within(async {
        tokio::join!(
            am.trigger_dependency_audit(&query),
            am.trigger_dependency_audit(&query)
        )
    })
    .await;
    let (first, second) = (first?, second?);

    let times_expired = first.expired.iter().chain(second.expired.iter()).filter(|k| **k == dep("data/b.txt")).count();
    assert_eq!(times_expired, 1, "{first:?} / {second:?}");
    assert_eq!(b.status().await, Status::Expired);
    let expiry_lines = match b.get_metadata().await? {
        Metadata::MetadataRecord(record) => record
            .log
            .iter()
            .filter(|e| e.kind == LogEntryKind::Warning && e.message.contains("expired"))
            .count(),
        Metadata::LegacyMetadata(_) => 0,
    };
    assert_eq!(expiry_lines, 1, "exactly one expiry log line");
    Ok(())
}

/// A store that reads `data/a.txt` fine until told to fail.
struct FlakyStore {
    inner: AsyncMemoryStore,
    failing: Arc<AtomicBool>,
}

#[async_trait]
impl AsyncStore for FlakyStore {
    async fn get(&self, key: &Key) -> Result<(Vec<u8>, Metadata), Error> {
        self.inner.get(key).await
    }

    async fn set_metadata(&self, key: &Key, metadata: &Metadata) -> Result<(), Error> {
        self.inner.set_metadata(key, metadata).await
    }

    async fn get_metadata(&self, key: &Key) -> Result<Metadata, Error> {
        if self.failing.load(Ordering::SeqCst) {
            return Err(Error::key_read_error(key, "FlakyStore", "intentional failure"));
        }
        self.inner.get_metadata(key).await
    }

    async fn set(&self, key: &Key, data: &[u8], metadata: &Metadata) -> Result<(), Error> {
        self.inner.set(key, data, metadata).await
    }

    async fn contains(&self, key: &Key) -> Result<bool, Error> {
        if self.failing.load(Ordering::SeqCst) {
            return Err(Error::key_read_error(key, "FlakyStore", "intentional failure"));
        }
        self.inner.contains(key).await
    }

    async fn remove(&self, key: &Key) -> Result<(), Error> {
        self.inner.remove(key).await
    }
}

/// A store that cannot answer is an error, not a missing version: the audit must not expire
/// anything on a transient failure.
#[tokio::test]
async fn audit_store_error_propagates() -> TestResult {
    let (snapshot, _) = first_process().await?;
    let inner = AsyncMemoryStore::new(&Key::new());
    snapshot.replay_into(&inner).await?;
    let failing = Arc::new(AtomicBool::new(false));
    let envref = counting_env(
        Box::new(FlakyStore { inner, failing: failing.clone() }),
        Arc::new(AtomicUsize::new(0)),
    );
    let b = load(&envref, "data/b.txt").await?;

    failing.store(true, Ordering::SeqCst);
    let result = within(
        envref
            .get_asset_manager()
            .trigger_dependency_audit(&parse_query("-R/data/b.txt")?),
    )
    .await;
    failing.store(false, Ordering::SeqCst);

    assert!(result.is_err(), "a failing store is an error, not an empty report: {result:?}");
    assert_eq!(b.status().await, Status::Ready, "nothing is expired on a store error");
    let report: AuditReport = within(
        envref
            .get_asset_manager()
            .trigger_dependency_audit(&parse_query("-R/data/b.txt")?),
    )
    .await?;
    assert!(report.expired.is_empty(), "the stored version matches again: {report:?}");
    Ok(())
}

/// Clarification 4 of Revision 2: an audit expires along the edges loaded in this process. A
/// dependent that is loaded afterwards is not missed, because the audit left the current version
/// in the map and the existing check on load refuses the stale copy.
#[tokio::test]
async fn unloaded_dependent_is_refused_on_later_load() -> TestResult {
    let mut recipes = RecipeList::new();
    recipes.add_recipe(Recipe::new("-R/data/a.txt/-/upper/b.txt".to_string(), "B".into(), "".into())?);
    recipes.add_recipe(Recipe::new("-R/data/a.txt/-/upper/c.txt".to_string(), "C".into(), "".into())?);
    let source = AsyncMemoryStore::new(&Key::new());
    source
        .set(&parse_key("data/recipes.yaml")?, serde_yaml::to_string(&recipes)?.as_bytes(), &Metadata::new())
        .await?;
    let first = counting_env(Box::new(source), Arc::new(AtomicUsize::new(0)));
    let am = first.get_asset_manager();
    within(am.set_binary(&parse_key("data/a.txt")?, b"hello", provenance_text_metadata())).await?;
    for name in ["b", "c"] {
        let key = parse_key(&format!("data/{name}.txt"))?;
        let asset = am.get(&key).await?;
        let _ = asset.get().await?;
        common::manager_scenarios::wait_until_stored(&first, &key, Status::Ready).await?;
    }
    let keys: Vec<Key> = ["data/recipes.yaml", "data/a.txt", "data/b.txt", "data/c.txt"]
        .into_iter()
        .map(parse_key)
        .collect::<Result<_, _>>()?;
    let snapshot = StoreSnapshot::capture(&first.get_async_store(), &keys).await?;

    let calls = Arc::new(AtomicUsize::new(0));
    let envref = second_process(&snapshot, &[("data/a.txt", moved())], calls.clone()).await?;
    let b = load(&envref, "data/b.txt").await?;
    within(
        envref
            .get_asset_manager()
            .trigger_dependency_audit(&parse_query("-R/data/b.txt")?),
    )
    .await?;
    assert_eq!(b.status().await, Status::Expired, "precondition: the loaded dependent is expired");
    assert_eq!(calls.load(Ordering::SeqCst), 0);

    // `c.txt` was not loaded when the audit ran. Loading it now must not serve the stale copy.
    let c = envref.evaluate("-R/data/c.txt").await?;
    let _ = c.get().await?;

    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "the stale copy was refused and recomputed, not served"
    );
    assert_eq!(c.status().await, Status::Ready);
    Ok(())
}

// ======================================================================================
// The `on_load` policy (Step 7 of the dependency-audit design).
// ======================================================================================

/// Example 1: after a restart in which a batch job moved the dependency, a strict service refuses
/// the stale result and recomputes it, while a lax one serves it until an audit says otherwise.
#[tokio::test]
async fn strict_service_after_restart() -> TestResult {
    let (snapshot, _) = first_process().await?;
    let changed = [("data/a.txt", moved())];

    let strict_calls = Arc::new(AtomicUsize::new(0));
    let strict =
        second_process_with(&snapshot, &changed, strict_calls.clone(), DependencyAuditPolicy::OnLoad).await?;
    let b = within(strict.evaluate("-R/data/b.txt")).await?;
    assert_eq!(within(b.get()).await?.try_into_string()?, "HELLO");
    assert_eq!(strict_calls.load(Ordering::SeqCst), 1, "the stale copy was refused and recomputed");

    let lax_calls = Arc::new(AtomicUsize::new(0));
    let lax =
        second_process_with(&snapshot, &changed, lax_calls.clone(), DependencyAuditPolicy::Explicit).await?;
    let served = load(&lax, "data/b.txt").await?;
    assert_eq!(lax_calls.load(Ordering::SeqCst), 0, "loading is as before: served as stored");
    let report = within(
        lax.get_asset_manager()
            .trigger_dependency_audit_with(&parse_query("-R/data/b.txt")?, AuditMode::ReportOnly),
    )
    .await?;
    assert!(report.expired.is_empty(), "ReportOnly never expires: {report:?}");
    assert_eq!(report.findings.len(), 1);
    assert_eq!(report.findings[0].dependency, dep("data/a.txt"));
    assert_eq!(report.findings[0].dependent, dep("data/b.txt"));
    assert_eq!(report.findings[0].found, moved());
    assert_eq!(served.status().await, Status::Ready);
    let wet = within(
        lax.get_asset_manager()
            .trigger_dependency_audit(&parse_query("-R/data/b.txt")?),
    )
    .await?;
    assert_eq!(wet.expired, vec![dep("data/b.txt")]);
    // The operator can read why: the audited dependency is the root, and b depends on it directly.
    assert_eq!(served.status().await, Status::Expired);
    assert_eq!(
        served.get_metadata().await?.expiry_reason(),
        Some(ExpiryReason::Cascaded {
            cause: ExpiryCause::Audit { found: moved() },
            root: dep("data/a.txt"),
            via: dep("data/a.txt"),
        })
    );
    Ok(())
}

#[tokio::test]
async fn on_load_refuses_stale_fast_track() -> TestResult {
    let (snapshot, _) = first_process().await?;
    let calls = Arc::new(AtomicUsize::new(0));
    let envref = second_process_with(
        &snapshot,
        &[("data/a.txt", moved())],
        calls.clone(),
        DependencyAuditPolicy::OnLoad,
    )
    .await?;
    let b = within(envref.evaluate("-R/data/b.txt")).await?;
    let _ = within(b.get()).await?;
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(b.status().await, Status::Ready);
    Ok(())
}

/// A dependency that left no durable version cannot be shown to be what the dependent used, so
/// `on_load` refuses (a current 0 against a concrete recorded version is not "compatible").
#[tokio::test]
async fn on_load_refuses_when_dependency_has_no_version() -> TestResult {
    let (snapshot, _) = first_process().await?;
    let calls = Arc::new(AtomicUsize::new(0));
    let envref = second_process_with(
        &snapshot,
        &[("data/a.txt", Version::unknown())],
        calls.clone(),
        DependencyAuditPolicy::OnLoad,
    )
    .await?;
    let b = within(envref.evaluate("-R/data/b.txt")).await?;
    let _ = within(b.get()).await?;
    assert_eq!(calls.load(Ordering::SeqCst), 1, "refused and recomputed");
    Ok(())
}

/// Pitfall 2: under `explicit`, deleting an intermediate by hand does not make its dependents
/// recompute on load.
#[tokio::test]
async fn explicit_policy_serves_when_intermediate_deleted() -> TestResult {
    let (snapshot, _) = first_process().await?;
    let calls = Arc::new(AtomicUsize::new(0));
    let envref =
        second_process_with(&snapshot, &[], calls.clone(), DependencyAuditPolicy::Explicit).await?;
    envref.get_async_store().remove(&parse_key("data/a.txt")?).await?;
    let _ = load(&envref, "data/b.txt").await?;
    assert_eq!(calls.load(Ordering::SeqCst), 0, "served as stored");
    Ok(())
}

#[tokio::test]
async fn audit_policy_from_config_yaml() -> TestResult {
    let config = EnvironmentConfig::from_yaml("assets:\n  dependency_audit: on_load\n")?;
    let envref = EnvironmentBuilder::<Value, (), Queued>::new()
        .with_asset_manager_options(config.assets)
        .build()?;
    assert_eq!(
        envref.get_asset_manager().dependency_audit_policy(),
        DependencyAuditPolicy::OnLoad
    );
    let default = EnvironmentBuilder::<Value, (), Queued>::new().build()?;
    assert_eq!(
        default.get_asset_manager().dependency_audit_policy(),
        DependencyAuditPolicy::Explicit
    );
    assert_eq!(
        default.get_asset_manager().version_verification(),
        liquers_core::environment_builder::VersionVerification::OnRead
    );
    Ok(())
}

// ======================================================================================
// Folder-listing dependencies (Step 8 of the dependency-audit design, Part D).
// ======================================================================================

/// A store with the provenance recipes in `data/` and, in `home`, one recipe
/// `index.txt = -R-dir/data/-/index_files`. With `home == "data"` the index lives inside the
/// folder it lists.
async fn index_store(home: &str, with_a_recipe: bool) -> Result<AsyncMemoryStore, Error> {
    let mut rl = RecipeList::new();
    rl.add_recipe(Recipe::new(
        "-R-dir/data/-/index_files/index.txt".to_string(),
        "Index".into(),
        "lists data".into(),
    )?);
    let yaml = serde_yaml::to_string(&rl)
        .map_err(|e| Error::general_error(format!("recipes.yaml: {e}")))?;
    let store = if home == "data" {
        // The provenance store already owns `data/recipes.yaml`; merge the index recipe into it.
        let store = provenance_store(with_a_recipe).await?;
        let key = parse_key("data/recipes.yaml")?;
        let (bytes, _) = store.get(&key).await?;
        let mut existing: RecipeList = serde_yaml::from_slice(&bytes)
            .map_err(|e| Error::general_error(format!("recipes.yaml: {e}")))?;
        for recipe in rl.recipes {
            existing.add_recipe(recipe);
        }
        let merged = serde_yaml::to_string(&existing)
            .map_err(|e| Error::general_error(format!("recipes.yaml: {e}")))?;
        store.set(&key, merged.as_bytes(), &Metadata::new()).await?;
        store
    } else {
        let store = provenance_store(with_a_recipe).await?;
        store
            .set(&parse_key(&format!("{home}/recipes.yaml"))?, yaml.as_bytes(), &Metadata::new())
            .await?;
        store
    };
    Ok(store)
}

fn index_env(store: Box<dyn AsyncStore>, calls: Arc<AtomicUsize>) -> EnvRef<TestEnv> {
    let mut env = TestEnv::new();
    register_provenance_commands(&mut env.command_registry);
    common::manager_scenarios::register_index_files(&mut env.command_registry, calls);
    env.with_async_store(store);
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    env.to_ref()
}

fn index_key(home: &str) -> Result<Key, Error> {
    parse_key(&format!("{home}/index.txt"))
}

/// Evaluate the index and wait until it is stored `Ready`; returns its text.
async fn evaluate_index(envref: &EnvRef<TestEnv>, home: &str) -> Result<String, Box<dyn std::error::Error>> {
    let key = index_key(home)?;
    let asset = within(envref.get_asset_manager().get(&key)).await?;
    let text = within(asset.get()).await?.try_into_string()?;
    within(common::manager_scenarios::wait_until_stored(envref, &key, Status::Ready)).await?;
    Ok(text)
}

async fn put(envref: &EnvRef<TestEnv>, name: &str, content: &[u8]) -> TestResult {
    within(envref.get_asset_manager().set_binary(
        &parse_key(name)?,
        content,
        provenance_text_metadata(),
    ))
    .await?;
    Ok(())
}

async fn stored_status(envref: &EnvRef<TestEnv>, key: &Key) -> Result<Status, Error> {
    Ok(envref.get_async_store().get_metadata(key).await?.status())
}

/// Example 2: a new file in a listed folder moves the listing version and expires the index.
#[tokio::test]
async fn adding_a_file_expires_the_index() -> TestResult {
    let calls = Arc::new(AtomicUsize::new(0));
    let envref = index_env(Box::new(index_store("idx", false).await?), calls.clone());
    put(&envref, "data/a.txt", b"hello").await?;
    let before = evaluate_index(&envref, "idx").await?;
    assert!(before.contains("data/a.txt"), "{before}");
    assert!(!before.contains("data/new.txt"), "{before}");
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    put(&envref, "data/new.txt", b"fresh").await?;

    let index = index_key("idx")?;
    let metadata = envref.get_async_store().get_metadata(&index).await?;
    assert_eq!(metadata.status(), Status::Expired);
    let dir = DependencyKey::from_dir_key(&parse_key("data")?);
    let Some(ExpiryReason::Cascaded { cause, root, via }) = metadata.expiry_reason() else {
        panic!("expected a cascaded reason, got {:?}", metadata.expiry_reason());
    };
    assert_eq!(root, dir);
    assert_eq!(via, dir);
    let ExpiryCause::Updated { version } = cause else {
        panic!("expected an Updated cause, got {cause:?}");
    };
    assert!(!version.is_unknown());

    let after = evaluate_index(&envref, "idx").await?;
    assert!(after.contains("data/new.txt"), "{after}");
    assert_eq!(calls.load(Ordering::SeqCst), 2, "recomputed once");
    Ok(())
}

/// Pitfall 8: a folder gains a file while nothing is running. After a restart nothing holds the
/// listing version, the index is served as stored, and an audit finds the gap.
#[tokio::test]
async fn listing_gap_resolved_by_audit_after_restart() -> TestResult {
    let first = index_env(
        Box::new(index_store("idx", false).await?),
        Arc::new(AtomicUsize::new(0)),
    );
    put(&first, "data/a.txt", b"hello").await?;
    evaluate_index(&first, "idx").await?;
    let keys = [
        parse_key("data/recipes.yaml")?,
        parse_key("idx/recipes.yaml")?,
        parse_key("data/a.txt")?,
        index_key("idx")?,
    ];
    let snapshot = StoreSnapshot::capture(&first.get_async_store(), &keys).await?;

    let store = AsyncMemoryStore::new(&Key::new());
    snapshot.replay_into(&store).await?;
    // The folder changes behind Liquers' back.
    store
        .set(
            &parse_key("data/outside.txt")?,
            b"x",
            &Metadata::MetadataRecord(provenance_text_metadata().into()),
        )
        .await?;
    let calls = Arc::new(AtomicUsize::new(0));
    let second = index_env(Box::new(store), calls.clone());
    let index = within(second.evaluate("-R/idx/index.txt")).await?;
    let _ = within(index.get()).await?;
    assert_eq!(index.status().await, Status::Ready, "served as stored: nothing knows the gap");
    assert_eq!(calls.load(Ordering::SeqCst), 0);

    let report = within(
        second
            .get_asset_manager()
            .trigger_dependency_audit(&parse_query("-R/idx/index.txt")?),
    )
    .await?;

    let dir = DependencyKey::from_dir_key(&parse_key("data")?);
    assert_eq!(report.checked, vec![dir.clone()], "{report:?}");
    assert_eq!(report.expired, vec![dep("idx/index.txt")], "{report:?}");
    assert_eq!(index.status().await, Status::Expired);
    Ok(())
}

/// The plan carries `-R-dir/data` with no version. The edge is added anyway, and the step then
/// upgrades the recorded dependency to the listing's version: one record, concrete.
#[tokio::test]
async fn plan_dependency_without_version_gets_unknown_edge_then_upgrade() -> TestResult {
    let envref = index_env(
        Box::new(index_store("idx", false).await?),
        Arc::new(AtomicUsize::new(0)),
    );
    put(&envref, "data/a.txt", b"hello").await?;
    evaluate_index(&envref, "idx").await?;

    let metadata = envref.get_async_store().get_metadata(&index_key("idx")?).await?;
    let dir = DependencyKey::from_dir_key(&parse_key("data")?);
    let records: Vec<&DependencyRecord> = metadata
        .get_dependencies()
        .iter()
        .filter(|record| record.key == dir)
        .collect();
    assert_eq!(records.len(), 1, "one record for the listing: {:?}", metadata.get_dependencies());
    assert!(!records[0].version.is_unknown(), "upgraded to the listing version: {:?}", metadata.get_dependencies());

    // Concrete, and the one the audit sees: nothing is stale.
    let report = within(
        envref
            .get_asset_manager()
            .trigger_dependency_audit(&parse_query("-R/idx/index.txt")?),
    )
    .await?;
    assert!(report.expired.is_empty(), "{report:?}");
    Ok(())
}

/// Corner case 2: rewriting a member with other bytes leaves the membership alone.
#[tokio::test]
async fn content_change_does_not_move_listing_version() -> TestResult {
    let calls = Arc::new(AtomicUsize::new(0));
    let envref = index_env(Box::new(index_store("idx", false).await?), calls.clone());
    put(&envref, "data/a.txt", b"hello").await?;
    evaluate_index(&envref, "idx").await?;

    put(&envref, "data/a.txt", b"entirely different").await?;

    assert_eq!(stored_status(&envref, &index_key("idx")?).await?, Status::Ready);
    let _ = evaluate_index(&envref, "idx").await?;
    assert_eq!(calls.load(Ordering::SeqCst), 1, "served from the store");
    Ok(())
}

/// Corner case 3: dropping the bytes of a computed asset keeps its recipe, so its name stays in
/// the listing and the version does not move.
#[tokio::test]
async fn deleting_bytes_keeps_the_listing_version() -> TestResult {
    let calls = Arc::new(AtomicUsize::new(0));
    let envref = index_env(Box::new(index_store("idx", true).await?), calls.clone());
    let a = parse_key("data/a.txt")?;
    let value = within(envref.get_asset_manager().get(&a)).await?;
    let _ = within(value.get()).await?;
    common::manager_scenarios::wait_until_stored(&envref, &a, Status::Ready).await?;
    evaluate_index(&envref, "idx").await?;

    within(envref.get_asset_manager().remove(&a)).await?;

    assert_eq!(stored_status(&envref, &index_key("idx")?).await?, Status::Ready);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    Ok(())
}

/// Corner case 5: an index that lives in the folder it lists is part of that listing from the
/// start (its recipe puts the name there), so storing it moves nothing.
#[tokio::test]
async fn index_inside_listed_folder_does_not_self_expire() -> TestResult {
    let calls = Arc::new(AtomicUsize::new(0));
    let envref = index_env(Box::new(index_store("data", false).await?), calls.clone());
    let text = evaluate_index(&envref, "data").await?;
    assert!(text.contains("data/index.txt"), "the index is in its own listing: {text}");

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    assert_eq!(stored_status(&envref, &index_key("data")?).await?, Status::Ready);
    let again = evaluate_index(&envref, "data").await?;
    assert_eq!(again, text);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    Ok(())
}

/// Many writers into one folder: the refreshes may interleave, and the folder's true membership
/// is what the next evaluation sees and records.
#[tokio::test]
async fn concurrent_writes_settle_on_true_membership() -> TestResult {
    let calls = Arc::new(AtomicUsize::new(0));
    let envref = index_env(Box::new(index_store("idx", false).await?), calls.clone());
    put(&envref, "data/a.txt", b"hello").await?;
    evaluate_index(&envref, "idx").await?;

    let mut writers = Vec::new();
    for n in 0..8 {
        let envref = envref.clone();
        writers.push(tokio::spawn(async move {
            let name = format!("data/w{n}.txt");
            let key = parse_key(&name).map_err(|e| e.to_string())?;
            envref
                .get_asset_manager()
                .set_binary(&key, b"w", provenance_text_metadata())
                .await
                .map_err(|e| e.to_string())
        }));
    }
    for writer in writers {
        within(writer).await??;
    }

    assert_eq!(stored_status(&envref, &index_key("idx")?).await?, Status::Expired);
    let text = evaluate_index(&envref, "idx").await?;
    for n in 0..8 {
        assert!(text.contains(&format!("data/w{n}.txt")), "w{n} missing: {text}");
    }
    let report = within(
        envref
            .get_asset_manager()
            .trigger_dependency_audit(&parse_query("-R/idx/index.txt")?),
    )
    .await?;
    assert!(report.expired.is_empty(), "the index recorded the settled listing: {report:?}");
    assert_eq!(stored_status(&envref, &index_key("idx")?).await?, Status::Ready);
    Ok(())
}

/// A store whose `listdir` can be switched off.
struct ListdirFailingStore {
    inner: AsyncMemoryStore,
    failing: Arc<AtomicBool>,
}

#[async_trait]
impl AsyncStore for ListdirFailingStore {
    async fn get(&self, key: &Key) -> Result<(Vec<u8>, Metadata), Error> {
        self.inner.get(key).await
    }

    async fn set_metadata(&self, key: &Key, metadata: &Metadata) -> Result<(), Error> {
        self.inner.set_metadata(key, metadata).await
    }

    async fn get_metadata(&self, key: &Key) -> Result<Metadata, Error> {
        self.inner.get_metadata(key).await
    }

    async fn set(&self, key: &Key, data: &[u8], metadata: &Metadata) -> Result<(), Error> {
        self.inner.set(key, data, metadata).await
    }

    async fn contains(&self, key: &Key) -> Result<bool, Error> {
        self.inner.contains(key).await
    }

    async fn remove(&self, key: &Key) -> Result<(), Error> {
        self.inner.remove(key).await
    }

    async fn is_dir(&self, key: &Key) -> Result<bool, Error> {
        self.inner.is_dir(key).await
    }

    async fn listdir(&self, key: &Key) -> Result<Vec<String>, Error> {
        if self.failing.load(Ordering::SeqCst) {
            return Err(Error::key_read_error(key, "ListdirFailingStore", "intentional failure"));
        }
        self.inner.listdir(key).await
    }
}

/// The write is the user's act; a listing that cannot be refreshed afterwards is reported on
/// stderr and the write stands.
#[tokio::test]
async fn listdir_error_after_write_is_logged_not_fatal() -> TestResult {
    let failing = Arc::new(AtomicBool::new(false));
    let envref = index_env(
        Box::new(ListdirFailingStore {
            inner: index_store("idx", false).await?,
            failing: failing.clone(),
        }),
        Arc::new(AtomicUsize::new(0)),
    );
    put(&envref, "data/a.txt", b"hello").await?;
    evaluate_index(&envref, "idx").await?;

    failing.store(true, Ordering::SeqCst);
    let written = parse_key("data/late.txt")?;
    let result = within(envref.get_asset_manager().set_binary(
        &written,
        b"late",
        provenance_text_metadata(),
    ))
    .await;
    failing.store(false, Ordering::SeqCst);

    result?;
    assert!(envref.get_async_store().contains(&written).await?, "the write stands");
    assert_eq!(
        stored_status(&envref, &index_key("idx")?).await?,
        Status::Ready,
        "the refresh failed, so nothing was moved or expired"
    );
    Ok(())
}
