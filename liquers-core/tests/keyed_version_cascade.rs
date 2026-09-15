//! Versions for computed keyed assets, and the invalidation that depends on them.
//!
//! The defect these guard (`KEYED-EXPIRY-DOES-NOT-CASCADE-TO-KEYED-DEPENDENTS`) is not visible in
//! a two-asset test: a *direct* dependent has always been invalidated, through the weak-reference
//! route that runs outside the version guard. What never happened was the second hop. So the
//! fixture here is a **three-link chain**, and the assertion on the third link is the regression
//! test — the one on the second is a control that passed before the fix too.
//!
//! Every test awaits `get()` before reading `status()`: `evaluate()` returns an asset that may
//! still be `Processing`, and reading the status first reports a lie and makes `expire()` fail.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use liquers_core::{
    assets::{AssetData, AssetManager, PersistenceStatus},
    context::{Context, EnvRef, Environment, SimpleEnvironment},
    error::Error,
    metadata::{Metadata, Status, Version},
    parse::parse_key,
    query::Key,
    recipes::{DefaultRecipeProvider, Recipe, RecipeList},
    state::State,
    store::{AsyncMemoryStore, AsyncStore},
    value::{Value, ValueInterface},
};
use liquers_macro::register_command;

mod fixtures;
use fixtures::{CountingStore, StoreSnapshot};

type TestEnv = SimpleEnvironment<Value>;

/// `a.txt` <- hello · `b.txt` <- a.txt/world · `c.txt` <- b.txt/world · `n.bin` <- a.txt/count
///
/// `n.bin` is the non-serializable case and needs no new value type: `Value::as_bytes` refuses an
/// integer for the `bin` data format and accepts a string, and the data format is seeded from the
/// key's extension — so the pair differs by one character of a filename.
async fn chain_env(counter: Arc<AtomicUsize>) -> Result<EnvRef<TestEnv>, Box<dyn std::error::Error>>
{
    let store = AsyncMemoryStore::new(&Key::new());
    seed_recipes(&store).await?;
    env_over_store(Box::new(store), counter)
}

/// The commands the chain is built from. Shared by every environment helper here, so a test that
/// rebuilds an environment over the same store cannot accidentally register a *different*
/// implementation version and invalidate everything for the wrong reason.
fn register_chain_commands(
    env: &mut TestEnv,
    counter: Arc<AtomicUsize>,
) -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = TestEnv;

    fn hello() -> Result<Value, Error> {
        Ok(Value::from("Hello"))
    }
    fn world(state: &State<Value>) -> Result<Value, Error> {
        Ok(Value::from(format!("{}, world!", state.try_into_string()?)))
    }
    let cr = &mut env.command_registry;
    register_command!(cr, fn hello() -> result version: 1)?;
    register_command!(cr, fn world(state) -> result version: 2)?;

    // A counting command, so a second evaluation can produce different content.
    env.command_registry.register_command(
        liquers_core::command_metadata::CommandKey::new_name("count"),
        move |_, _, _| Ok(Value::I32(counter.fetch_add(1, Ordering::SeqCst) as i32)),
    )?;
    Ok(())
}

/// Write `recipes.yaml` describing the chain. Takes `&dyn AsyncStore` so the same seeding works
/// for a plain memory store and for a wrapper around one.
async fn seed_recipes(store: &dyn AsyncStore) -> Result<(), Box<dyn std::error::Error>> {
    let mut rl = RecipeList::new();
    rl.add_recipe(Recipe::new(
        "hello/a.txt".to_string(),
        "A".into(),
        "root of the chain".into(),
    )?);
    rl.add_recipe(Recipe::new(
        "-R/a.txt/-/world/b.txt".to_string(),
        "B".into(),
        "depends on a.txt".into(),
    )?);
    rl.add_recipe(Recipe::new(
        "-R/b.txt/-/world/c.txt".to_string(),
        "C".into(),
        "depends on b.txt".into(),
    )?);
    rl.add_recipe(Recipe::new(
        "-R/a.txt/-/count/n.bin".to_string(),
        "N".into(),
        "non-serializable at .bin".into(),
    )?);
    store
        .set(
            &parse_key("recipes.yaml")?,
            serde_yaml::to_string(&rl)?.as_bytes(),
            &Metadata::new(),
        )
        .await?;
    Ok(())
}

/// An environment over an already-seeded store.
fn env_over_store(
    store: Box<dyn AsyncStore>,
    counter: Arc<AtomicUsize>,
) -> Result<EnvRef<TestEnv>, Box<dyn std::error::Error>> {
    let mut env = TestEnv::new();
    register_chain_commands(&mut env, counter)?;
    env.with_async_store(store);
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    Ok(env.to_ref())
}

/// Not "a version was assigned" — the exact bytes. This is what the single-serialization decision
/// buys, though note it cannot *prove* it: `Value::Text` encodes deterministically, so a
/// two-serialization implementation would pass here too. The design is the control, not this test.
#[tokio::test]
async fn computed_keyed_asset_version_is_the_hash_of_stored_bytes(
) -> Result<(), Box<dyn std::error::Error>> {
    let envref = chain_env(Arc::new(AtomicUsize::new(0))).await?;
    let a = envref.evaluate("-R/a.txt").await?;
    let state = a.get().await?;

    let stored = envref
        .get_async_store()
        .get_bytes(&parse_key("a.txt")?)
        .await?;

    assert_eq!(
        state.metadata.version(),
        Some(Version::from_bytes(&stored)),
        "the version must describe exactly the bytes the store holds"
    );
    Ok(())
}

/// **The regression test.** `b` is a control that passed before the fix; `c` is the defect.
#[tokio::test]
async fn keyed_expiry_cascades_to_keyed_dependents() -> Result<(), Box<dyn std::error::Error>> {
    let envref = chain_env(Arc::new(AtomicUsize::new(0))).await?;

    let c = envref.evaluate("-R/c.txt").await?;
    assert_eq!(c.get().await?.try_into_string()?, "Hello, world!, world!");
    let a = envref.evaluate("-R/a.txt").await?;
    let b = envref.evaluate("-R/b.txt").await?;
    let _ = a.get().await?;
    let _ = b.get().await?;

    a.expire().await?;

    assert_eq!(a.status().await, Status::Expired);
    assert_eq!(
        b.status().await,
        Status::Expired,
        "control: a direct dependent was invalidated before this fix too"
    );
    assert_eq!(
        c.status().await,
        Status::Expired,
        "the regression: invalidation must reach past the first hop"
    );
    Ok(())
}

/// A dependent's record must name the version its dependency actually had, not the unknown
/// captured before that dependency evaluated
/// (`DEPENDENCY-RECORD-VERSION-CAPTURED-BEFORE-DEPENDENCY-EVALUATES`).
#[tokio::test]
async fn dependency_record_carries_the_dependencys_post_evaluation_version(
) -> Result<(), Box<dyn std::error::Error>> {
    let envref = chain_env(Arc::new(AtomicUsize::new(0))).await?;
    let b = envref.evaluate("-R/b.txt").await?;
    let b_state = b.get().await?;
    let a = envref.evaluate("-R/a.txt").await?;
    let a_version = a.get().await?.metadata.version().expect("a has a version");

    let record = b_state
        .metadata
        .get_dependencies()
        .iter()
        .find(|d| d.key.as_str() == "-R/a.txt")
        .expect("b records a dependency on a.txt");

    assert_eq!(
        record.version, a_version,
        "the record must carry the dependency's settled version, not Version(0)"
    );
    Ok(())
}

/// The manager holds real command versions from `start()`, and the record used to carry zero while
/// the graph edge carried the truth (`PLAN-DEPENDENCY-RECORDS-HARDCODE-VERSION-ZERO`).
#[tokio::test]
async fn plan_dependency_record_carries_the_command_version(
) -> Result<(), Box<dyn std::error::Error>> {
    let envref = chain_env(Arc::new(AtomicUsize::new(0))).await?;
    let b = envref.evaluate("-R/b.txt").await?;
    let state = b.get().await?;

    let record = state
        .metadata
        .get_dependencies()
        .iter()
        .find(|d| d.key.as_str() == "ns-dep/command_impl---world")
        .expect("b records a dependency on the world command");

    assert_eq!(
        record.version,
        Version::new(2),
        "the declared `version: 2`, not zero"
    );
    Ok(())
}

/// A keyed asset whose value does not serialize leaves **no durable trace** — which is the premise
/// the durability rule rests on, so it is asserted rather than assumed.
#[tokio::test]
async fn non_serializable_keyed_asset_takes_a_unique_fallback_version(
) -> Result<(), Box<dyn std::error::Error>> {
    let envref = chain_env(Arc::new(AtomicUsize::new(0))).await?;
    let n = envref.evaluate("-R/n.bin").await?;
    let state = n.get().await?;

    let version = state.metadata.version().expect("a fallback version");
    assert!(
        !version.is_unknown(),
        "a fallback that produced Version(0) would be no fallback at all"
    );
    assert_eq!(n.persistence_status().await, PersistenceStatus::NonSerializable);
    assert!(
        !envref
            .get_async_store()
            .contains(&parse_key("n.bin")?)
            .await?,
        "nothing durable: this is why such an asset's dependents expire on restart"
    );
    Ok(())
}

/// The no-op that keeps the commonest path in the system cheap: a query asset is not a
/// dependency-graph node, so it is never serialized for a version nothing reads.
#[tokio::test]
async fn query_asset_has_no_version() -> Result<(), Box<dyn std::error::Error>> {
    let envref = chain_env(Arc::new(AtomicUsize::new(0))).await?;
    let q = envref.evaluate("hello/world").await?;

    assert_eq!(q.get().await?.metadata.version(), None);
    Ok(())
}

// --- The audit seam ---
//
// Nothing in `liquers-core` calls these, by design: the default policy is "never". These tests are
// the only callers, which is the point rather than a shortcoming — a public API with no in-tree
// caller has to be exercised as a user would call it.

/// **The exploratory workflow.** A user deletes an intermediate by hand; the result stays valid
/// and keeps being served, because nothing audits unless asked.
#[tokio::test]
async fn nothing_audits_by_default() -> Result<(), Box<dyn std::error::Error>> {
    let envref = chain_env(Arc::new(AtomicUsize::new(0))).await?;
    let c = envref.evaluate("-R/c.txt").await?;
    let _ = c.get().await?;

    // Delete the intermediate outright — data and metadata.
    envref.get_async_store().remove(&parse_key("a.txt")?).await?;

    let again = envref.evaluate("-R/c.txt").await?;
    let _ = again.get().await?;
    assert_eq!(
        again.status().await,
        Status::Ready,
        "a deleted intermediate must not invalidate a result nobody asked about"
    );
    Ok(())
}

/// `version(key)` reads metadata, never the value — so keeping the stored metadata and deleting the data
/// still verifies clean. An "optimization" that computed a version from the value breaks here.
#[tokio::test]
async fn metadata_kept_data_deleted_still_verifies_clean(
) -> Result<(), Box<dyn std::error::Error>> {
    let envref = chain_env(Arc::new(AtomicUsize::new(0))).await?;
    let c = envref.evaluate("-R/c.txt").await?;
    let _ = c.get().await?;
    let a_key = parse_key("a.txt")?;
    let a_metadata = envref.get_async_store().get_metadata(&a_key).await?;

    // Data gone, stored metadata kept.
    envref.get_async_store().remove(&a_key).await?;
    envref
        .get_async_store()
        .set_metadata(&a_key, &a_metadata)
        .await?;

    let manager = envref.get_asset_manager();
    assert_eq!(
        manager.version(&a_key).await?,
        a_metadata.version(),
        "a version is a fact about metadata, not about the value being present"
    );
    Ok(())
}

/// An audit over a graph with nothing missing reports nothing and expires nothing.
#[tokio::test]
async fn audit_of_a_complete_graph_expires_nothing() -> Result<(), Box<dyn std::error::Error>> {
    let envref = chain_env(Arc::new(AtomicUsize::new(0))).await?;
    let c = envref.evaluate("-R/c.txt").await?;
    let _ = c.get().await?;

    let report = envref
        .get_asset_manager()
        .trigger_dependency_audit_all_registered()
        .await?;

    assert!(
        report.expired.is_empty(),
        "everything is known and consistent: {report:?}"
    );
    assert_eq!(c.status().await, Status::Ready);
    Ok(())
}

/// A non-keyed query has nothing to audit, and that is an empty report rather than an error.
#[tokio::test]
async fn audit_of_a_non_keyed_query_is_an_empty_report() -> Result<(), Box<dyn std::error::Error>> {
    let envref = chain_env(Arc::new(AtomicUsize::new(0))).await?;
    let query = liquers_core::parse::parse_query("hello/world")?;

    let report = envref
        .get_asset_manager()
        .trigger_dependency_audit(&query)
        .await?;

    assert_eq!(report, liquers_core::assets::AuditReport::default());
    Ok(())
}

// ======================================================================================
// Fast-track: the success baseline, and the dependency-status check that gates it.
// `stale-dependency-status-finalization`, Phase 3 F0-F4.
// ======================================================================================

/// **The baseline, and it did not exist before this design.**
///
/// Searching the suite for `try_fast_track` found exactly one test, and it asserted the function
/// returns `false` (`expiration_integration.rs`, an `Expired` store entry). Nothing asserted it can
/// return `true`. That matters because the dependency check added beside it fails *open* by design:
/// an implementation that failed closed instead would stop fast-tracking entirely, every result
/// would still be correct, and the suite would stay green. The bug would arrive as "everything got
/// slow" rather than as a red test. This test is where that failure lands.
#[tokio::test]
async fn fast_track_succeeds_for_a_ready_stored_asset() -> Result<(), Box<dyn std::error::Error>> {
    let envref = chain_env(Arc::new(AtomicUsize::new(0))).await?;

    // Evaluate b.txt so the store holds it as Ready, with a dependency record on a.txt.
    let b = envref.evaluate("-R/b.txt").await?;
    assert_eq!(b.get().await?.try_into_string()?, "Hello, world!");
    assert_eq!(b.status().await, Status::Ready);

    let key = parse_key("b.txt")?;
    let (_bytes, stored) = envref.get_async_store().get(&key).await?;
    assert_eq!(
        stored.status(),
        Status::Ready,
        "precondition: the store must hold b.txt as Ready for this to be the success path"
    );

    // A fresh AssetData over that entry must load it rather than re-evaluate.
    let mut reloaded =
        AssetData::<TestEnv>::new(9501, key.clone().into(), Some(key.clone()), envref.clone());
    assert!(
        reloaded.try_fast_track().await?,
        "a Ready store entry whose dependencies are all fine must fast-track"
    );
    assert_eq!(
        reloaded
            .poll_state()
            .expect("a fast-tracked asset exposes its state")
            .try_into_string()?,
        "Hello, world!",
        "the fast-tracked asset exposes the stored value, so it was loaded not recomputed"
    );
    Ok(())
}

/// Build a second environment over a replayed snapshot — the restart simulation. The first
/// environment must already be dropped by the caller, so the new manager and dependency manager
/// start genuinely empty.
async fn rehydrated_env(
    snapshot: &StoreSnapshot,
    counter: Arc<AtomicUsize>,
) -> Result<EnvRef<TestEnv>, Box<dyn std::error::Error>> {
    let store = AsyncMemoryStore::new(&Key::new());
    snapshot.replay_into(&store).await?;
    env_over_store(Box::new(store), counter)
}

/// F1 — the restart case, and the reason the writing half is worth anything.
///
/// A fresh process holds `b.txt` as `Ready` with a dependency record on `a.txt`, and the store
/// says `a.txt` is `Expired`. The version check cannot see this: `a.txt` has not been recomputed,
/// so there is no version change to detect, and the dependency manager of a fresh process knows
/// no versions at all. Only reading the dependency's stored status catches it.
#[tokio::test]
async fn fast_track_declines_a_dependency_expired_in_the_store(
) -> Result<(), Box<dyn std::error::Error>> {
    let recipes = parse_key("recipes.yaml")?;
    let a_key = parse_key("a.txt")?;
    let b_key = parse_key("b.txt")?;

    let snapshot = {
        let envref = chain_env(Arc::new(AtomicUsize::new(0))).await?;
        let b = envref.evaluate("-R/b.txt").await?;
        let _ = b.get().await?;
        let a = envref.evaluate("-R/a.txt").await?;
        let _ = a.get().await?;

        // Snapshot b.txt while it is still Ready — expiring a.txt cascades and would rewrite it.
        let store = envref.get_async_store();
        let mut snap = StoreSnapshot::capture(&store, &[recipes.clone(), b_key.clone()]).await?;

        a.expire().await?;
        let a_snap = StoreSnapshot::capture(&store, &[a_key.clone()]).await?;
        snap.absorb(a_snap);
        snap
    };

    let envref2 = rehydrated_env(&snapshot, Arc::new(AtomicUsize::new(0))).await?;
    let store2 = envref2.get_async_store();
    assert_eq!(
        store2.get(&b_key).await?.1.status(),
        Status::Ready,
        "precondition: b.txt must look reusable, or this tests the status gate instead"
    );
    assert_eq!(
        store2.get(&a_key).await?.1.status(),
        Status::Expired,
        "precondition: its dependency must be stale in the store"
    );

    let mut reloaded =
        AssetData::<TestEnv>::new(9502, b_key.clone().into(), Some(b_key.clone()), envref2.clone());
    assert!(
        !reloaded.try_fast_track().await?,
        "a dependency the store reports as Expired must refuse the fast track"
    );
    Ok(())
}

/// F2 — the live-asset branch. The manager is the authority on status, so when it holds the
/// dependency the store is never consulted.
#[tokio::test]
async fn fast_track_declines_a_dependency_expired_in_memory(
) -> Result<(), Box<dyn std::error::Error>> {
    let recipes = parse_key("recipes.yaml")?;
    let a_key = parse_key("a.txt")?;
    let b_key = parse_key("b.txt")?;

    let snapshot = {
        let envref = chain_env(Arc::new(AtomicUsize::new(0))).await?;
        let b = envref.evaluate("-R/b.txt").await?;
        let _ = b.get().await?;
        let store = envref.get_async_store();
        StoreSnapshot::capture(&store, &[recipes.clone(), a_key.clone(), b_key.clone()]).await?
    };

    let envref2 = rehydrated_env(&snapshot, Arc::new(AtomicUsize::new(0))).await?;
    // Bring a.txt into this manager and expire it there. b.txt is not a live asset here and no
    // edge to it has been registered, so nothing cascades — its store entry stays Ready and the
    // only thing wrong is the live dependency.
    let a = envref2.evaluate("-R/a.txt").await?;
    let _ = a.get().await?;
    a.expire().await?;
    assert_eq!(a.status().await, Status::Expired);
    assert_eq!(
        envref2.get_async_store().get(&b_key).await?.1.status(),
        Status::Ready,
        "precondition: b.txt must still look reusable in the store"
    );

    let mut reloaded =
        AssetData::<TestEnv>::new(9503, b_key.clone().into(), Some(b_key.clone()), envref2.clone());
    assert!(
        !reloaded.try_fast_track().await?,
        "a dependency the manager reports as Expired must refuse the fast track"
    );
    Ok(())
}

/// F3 — **the guard whose failure would be invisible.**
///
/// `b.txt` depends on a command-implementation node as well as on `a.txt`, and
/// `Key::try_from` cannot address one. If the check treated "cannot determine" as "expired", it
/// would refuse the fast track for nearly every asset in the system: every result would still be
/// correct, merely recomputed, so no assertion anywhere would fail and the loss would surface as
/// a performance complaint rather than a bug. Fail open on absence, closed only on evidence.
#[tokio::test]
async fn fast_track_proceeds_when_the_dependency_check_is_inconclusive(
) -> Result<(), Box<dyn std::error::Error>> {
    let envref = chain_env(Arc::new(AtomicUsize::new(0))).await?;
    let b = envref.evaluate("-R/b.txt").await?;
    let state = b.get().await?;

    // The premise: at least one recorded dependency is not store-addressable.
    let inconclusive = state
        .metadata
        .get_dependencies()
        .iter()
        .filter(|d| Key::try_from(&d.key).is_err())
        .count();
    assert!(
        inconclusive > 0,
        "premise of this test: b.txt must carry a dependency the store cannot address"
    );

    let b_key = parse_key("b.txt")?;
    let mut reloaded =
        AssetData::<TestEnv>::new(9504, b_key.clone().into(), Some(b_key.clone()), envref.clone());
    assert!(
        reloaded.try_fast_track().await?,
        "a dependency that cannot be determined is not evidence of staleness"
    );
    Ok(())
}

// ======================================================================================
// Reload — `CROSS-PROCESS-RELOAD-IS-UNTESTED` R1-R3, on the re-hydration helper.
// ======================================================================================

/// R1 — nothing audits by default, across a restart. The in-process form of this is
/// `nothing_audits_by_default`; this is the dimension the persisted `DependencyRecord` exists for.
#[tokio::test]
async fn reloaded_dependent_is_served_without_audit() -> Result<(), Box<dyn std::error::Error>> {
    let recipes = parse_key("recipes.yaml")?;
    let a_key = parse_key("a.txt")?;
    let b_key = parse_key("b.txt")?;
    let counter = Arc::new(AtomicUsize::new(0));

    let snapshot = {
        let envref = chain_env(counter.clone()).await?;
        let b = envref.evaluate("-R/b.txt").await?;
        let _ = b.get().await?;
        let store = envref.get_async_store();
        StoreSnapshot::capture(&store, &[recipes.clone(), a_key.clone(), b_key.clone()]).await?
    };

    let envref2 = rehydrated_env(&snapshot, counter.clone()).await?;
    let mut reloaded =
        AssetData::<TestEnv>::new(9505, b_key.clone().into(), Some(b_key.clone()), envref2.clone());
    assert!(
        reloaded.try_fast_track().await?,
        "a process that has never evaluated the dependency must still serve the stored dependent"
    );
    assert_eq!(
        reloaded
            .poll_state()
            .expect("fast-tracked")
            .try_into_string()?,
        "Hello, world!"
    );
    Ok(())
}

/// R3 — **deployment safety, and the claim the issue calls the least-tested in the versions
/// design.** Every dependency record written before computed assets carried versions holds
/// `Version(0)`. If such a record did not match, upgrading would invalidate an entire existing
/// store on first contact.
#[tokio::test]
async fn a_pre_versions_record_with_version_zero_still_matches(
) -> Result<(), Box<dyn std::error::Error>> {
    let recipes = parse_key("recipes.yaml")?;
    let a_key = parse_key("a.txt")?;
    let b_key = parse_key("b.txt")?;

    let mut snapshot = {
        let envref = chain_env(Arc::new(AtomicUsize::new(0))).await?;
        let b = envref.evaluate("-R/b.txt").await?;
        let _ = b.get().await?;
        let store = envref.get_async_store();
        StoreSnapshot::capture(&store, &[recipes.clone(), a_key.clone(), b_key.clone()]).await?
    };
    // Make the store look like one written before the versions work landed.
    snapshot.downgrade_dependency_versions_to_unknown();

    let envref2 = rehydrated_env(&snapshot, Arc::new(AtomicUsize::new(0))).await?;
    // Give the dependency manager a real version for a.txt, so the recorded unknown is compared
    // against something concrete — which is exactly the upgrade situation.
    let a = envref2.evaluate("-R/a.txt").await?;
    let _ = a.get().await?;

    let mut reloaded =
        AssetData::<TestEnv>::new(9506, b_key.clone().into(), Some(b_key.clone()), envref2.clone());
    assert!(
        reloaded.try_fast_track().await?,
        "a pre-versions record carrying Version(0) must still match, or upgrading invalidates \
         every existing store entry on first contact"
    );
    Ok(())
}

/// R2 — the audit half of the restart story: a dependent reloaded into a fresh process **is**
/// expired by an explicit audit when its dependency can no longer be shown to reconstruct.
///
/// This is the counterpart of `reloaded_dependent_is_served_without_audit`, and the pair is the
/// point. Fast-track serves the stored dependent because a fresh dependency manager knows no
/// versions and therefore cannot contradict the record. The recorded version is not thereby
/// useless — it is the evidence an audit resolves against, and this is where that evidence is
/// spent.
///
/// It is also the **only** test in the suite in which an audit expires anything. Every other one
/// asserts an empty report, so without this the whole expire path of `audit_gaps` was unexercised.
///
/// The dependency is removed rather than rewritten. Removing it is the case the audit answers
/// today; a dependency whose stored version has *moved* is not, because `register_version` treats
/// a first observation as "no change" — see `AUDIT-CANNOT-EXPIRE-ON-A-FIRST-OBSERVED-VERSION`,
/// filed from this test.
#[tokio::test]
async fn explicit_audit_expires_a_reloaded_dependent_whose_dependency_vanished(
) -> Result<(), Box<dyn std::error::Error>> {
    let recipes = parse_key("recipes.yaml")?;
    let a_key = parse_key("a.txt")?;
    let b_key = parse_key("b.txt")?;

    let snapshot = {
        let envref = chain_env(Arc::new(AtomicUsize::new(0))).await?;
        let b = envref.evaluate("-R/b.txt").await?;
        let _ = b.get().await?;
        let store = envref.get_async_store();
        StoreSnapshot::capture(&store, &[recipes.clone(), a_key.clone(), b_key.clone()]).await?
    };

    let envref2 = rehydrated_env(&snapshot, Arc::new(AtomicUsize::new(0))).await?;

    // Reload the dependent first. It fast-tracks — the store says Ready and this manager holds no
    // version to compare against, which is precisely R1's situation — and reloading is what
    // registers the recorded edge the audit will later resolve.
    let b = envref2.evaluate("-R/b.txt").await?;
    let _ = b.get().await?;
    assert_eq!(
        b.status().await,
        Status::Ready,
        "precondition: the reloaded dependent must be served, or the audit has nothing to expire"
    );

    // The dependency leaves no trace: data and metadata both gone, so it has no durable version.
    envref2.get_async_store().remove(&a_key).await?;
    assert!(
        !envref2.get_async_store().contains(&a_key).await?,
        "precondition: the dependency must be genuinely absent"
    );

    let report = envref2
        .get_asset_manager()
        .trigger_dependency_audit(&liquers_core::parse::parse_query("-R/b.txt")?)
        .await?;

    assert!(
        report
            .expired
            .contains(&liquers_core::metadata::DependencyKey::from(&b_key)),
        "the audit must name the dependent it expired: {report:?}"
    );
    assert_eq!(
        b.status().await,
        Status::Expired,
        "an asset that left no trace cannot be shown to reconstruct identically, so a dependent \
         audited against it is expired"
    );
    Ok(())
}

/// `chain_env`, but over a store that counts `get_metadata` calls, so a test can assert that a
/// code path performed **no** store read. The handle is a clone sharing the counter and the inner
/// store with the one the environment owns.
async fn counting_chain_env(
    counter: Arc<AtomicUsize>,
) -> Result<(EnvRef<TestEnv>, CountingStore), Box<dyn std::error::Error>> {
    let store = CountingStore::new(AsyncMemoryStore::new(&Key::new()));
    seed_recipes(&store).await?;
    let handle = store.clone();
    Ok((env_over_store(Box::new(store), counter)?, handle))
}

/// F4 — the live-asset branch is not merely *correct*, it is *taken*.
///
/// F2 shows the check declines a dependency the manager reports as `Expired`, but a check that
/// consulted the store first and the manager second would pass F2 as well. The manager is the
/// authority on status; the store read exists only for a dependency the manager has never heard
/// of. So with every addressable dependency live in memory, the check must read no metadata at
/// all.
///
/// The assertion is on a **delta**, not on an absolute count: building the environment reads the
/// recipe list, and pinning that number would make this test fail whenever recipe loading changes,
/// for a reason having nothing to do with what it guards.
#[tokio::test]
async fn fast_track_reads_no_metadata_when_dependencies_are_live(
) -> Result<(), Box<dyn std::error::Error>> {
    let (envref, store) = counting_chain_env(Arc::new(AtomicUsize::new(0))).await?;

    let b = envref.evaluate("-R/b.txt").await?;
    let _ = b.get().await?;
    // Hold the dependency live in the manager for the duration of the check.
    let a = envref.evaluate("-R/a.txt").await?;
    let _ = a.get().await?;
    assert_eq!(a.status().await, Status::Ready);
    assert!(
        envref
            .get_asset_manager()
            .lookup_key_asset(&parse_key("a.txt")?)
            .is_some(),
        "premise of this test: the dependency must be registered in the manager"
    );

    let b_key = parse_key("b.txt")?;
    let before = store.reads();
    let mut reloaded =
        AssetData::<TestEnv>::new(9507, b_key.clone().into(), Some(b_key.clone()), envref.clone());
    assert!(
        reloaded.try_fast_track().await?,
        "with every dependency live and Ready the fast track must succeed"
    );
    assert_eq!(
        store.reads() - before,
        0,
        "the manager answers for a live dependency; the store read is the fallback, not the path"
    );
    Ok(())
}
