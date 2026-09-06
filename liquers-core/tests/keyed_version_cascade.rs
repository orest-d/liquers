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
    assets::{AssetManager, PersistenceStatus},
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

type TestEnv = SimpleEnvironment<Value>;

/// `a.txt` <- hello · `b.txt` <- a.txt/world · `c.txt` <- b.txt/world · `n.bin` <- a.txt/count
///
/// `n.bin` is the non-serializable case and needs no new value type: `Value::as_bytes` refuses an
/// integer for the `bin` data format and accepts a string, and the data format is seeded from the
/// key's extension — so the pair differs by one character of a filename.
async fn chain_env(counter: Arc<AtomicUsize>) -> Result<EnvRef<TestEnv>, Box<dyn std::error::Error>>
{
    type CommandEnvironment = TestEnv;
    let mut env = TestEnv::new();

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
    let c = counter.clone();
    env.command_registry
        .register_command(
            liquers_core::command_metadata::CommandKey::new_name("count"),
            move |_, _, _| Ok(Value::I32(c.fetch_add(1, Ordering::SeqCst) as i32)),
        )?;

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

    let store = AsyncMemoryStore::new(&Key::new());
    store
        .set(
            &parse_key("recipes.yaml")?,
            serde_yaml::to_string(&rl)?.as_bytes(),
            &Metadata::new(),
        )
        .await?;
    env.with_async_store(Box::new(store));
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

/// `version(key)` reads metadata, never the value — so keeping the sidecar and deleting the data
/// still verifies clean. An "optimization" that computed a version from the value breaks here.
#[tokio::test]
async fn metadata_kept_data_deleted_still_verifies_clean(
) -> Result<(), Box<dyn std::error::Error>> {
    let envref = chain_env(Arc::new(AtomicUsize::new(0))).await?;
    let c = envref.evaluate("-R/c.txt").await?;
    let _ = c.get().await?;
    let a_key = parse_key("a.txt")?;
    let a_metadata = envref.get_async_store().get_metadata(&a_key).await?;

    // Data gone, sidecar kept.
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
