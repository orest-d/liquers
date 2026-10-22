//! Environment teardown: dropping the last `EnvRef` drops the environment, its asset manager, its
//! cached assets and the manager's background tasks.
//!
//! Design: `specs/design/environment-manager-reference-cycle/` (Phase 3, `teardown01`-`teardown09`).
//! Before it, the asset manager and every asset held the environment with a strong `Arc`, so no
//! environment was ever dropped.
//!
//! Releases happen when a task finishes (a job's claim, a queue entry), which may be a moment
//! after the value is observable, so the assertions on counts and `Weak`s poll with [`eventually`].

use std::sync::{Arc, Weak};
use std::time::Duration;

use liquers_core::{
    assets::AssetManager,
    command_metadata::CommandKey,
    commands::CommandRegistry,
    context::{EnvRef, Environment, GenericEnvironment},
    environment_builder::{AssetManagerKind, AssetManagerOptions, EnvironmentBuilder, Inline, Queued},
    error::Error,
    metadata::{Metadata, MetadataRecord, Status},
    parse::{parse_key, parse_query},
    query::Key,
    store::{AsyncMemoryStore, AsyncStore},
    value::Value,
};
use tokio::sync::Semaphore;

type TestResult = Result<(), Box<dyn std::error::Error>>;
type Env<K> = GenericEnvironment<Value, (), K>;

/// `hello` returns `"hi"`; `gated-<tag>` waits for a permit of `gate`, then returns `<tag>`;
/// `store_name` waits for a permit, then reads the store through `Context::get_envref`.
fn register<E: Environment<Value = Value>>(cr: &mut CommandRegistry<E>, gate: Arc<Semaphore>) -> Result<(), Error> {
    cr.register_command(CommandKey::new_name("hello"), |_, _, _| Ok(Value::from("hi")))?;
    let g = gate.clone();
    cr.register_async_command(CommandKey::new_name("gated"), move |_, args, _| {
        let gate = g.clone();
        Box::pin(async move {
            gate.acquire().await.map_err(|e| Error::general_error(e.to_string()))?.forget();
            let tag: String = args.get(0, "tag")?;
            Ok(Value::from(tag))
        })
    })?
    .with_argument(liquers_core::command_metadata::ArgumentInfo::string_argument("tag"));
    cr.register_async_command(CommandKey::new_name("store_name"), move |_, _, context| {
        let gate = gate.clone();
        Box::pin(async move {
            gate.acquire().await.map_err(|e| Error::general_error(e.to_string()))?.forget();
            let store = context.get_envref().get_async_store();
            Ok(Value::from(store.store_name()))
        })
    })?;
    Ok(())
}

async fn store_with_x() -> Result<AsyncMemoryStore, Error> {
    let store = AsyncMemoryStore::new(&Key::new());
    let metadata = Metadata::MetadataRecord(
        MetadataRecord::new()
            .with_type_identifier("Text".to_owned())
            .with_status(Status::Source)
            .clone(),
    );
    store.set(&parse_key("data/x.txt")?, b"stored", &metadata).await?;
    // A computed value left by an earlier run: loads `Ready`, so it can be expired.
    let mut ready = MetadataRecord::new();
    ready.with_type_identifier("Text".to_owned()).with_status(Status::Ready);
    store
        .set(&parse_key("data/ready.txt")?, b"computed", &Metadata::MetadataRecord(ready))
        .await?;
    Ok(store)
}

/// An environment of kind `K` with the test commands and `data/x.txt` in its store; `capacity`
/// sets the job queue's (queued kinds only).
async fn build<K: AssetManagerKind>(
    gate: Arc<Semaphore>,
    capacity: Option<usize>,
) -> Result<EnvRef<Env<K>>, Error> {
    let mut options = AssetManagerOptions::default();
    if let Some(capacity) = capacity {
        options = options.with_job_capacity(capacity);
    }
    let mut builder = EnvironmentBuilder::<Value, (), K>::new()
        .with_async_store(Arc::new(store_with_x().await?))
        .with_asset_manager_options(options);
    register(&mut builder.command_registry, gate)?;
    builder.build()
}

/// Polls `condition` until it holds or about five seconds pass, yielding to the runtime between
/// tries so finishing tasks can release what they hold.
async fn eventually(mut condition: impl FnMut() -> bool) -> bool {
    for _ in 0..500 {
        if condition() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    condition()
}

fn alive_tasks() -> usize {
    tokio::runtime::Handle::current().metrics().num_alive_tasks()
}

// ---------------------------------------------------------------------------
// AC-1
// ---------------------------------------------------------------------------

async fn fresh_has_one_owner<K: AssetManagerKind>() -> TestResult {
    let envref = Env::<K>::new().to_ref();
    assert_eq!(Arc::strong_count(&envref.0), 1, "to_ref: the caller is the only owner");
    let envref = build::<K>(Arc::new(Semaphore::new(0)), None).await?;
    assert_eq!(Arc::strong_count(&envref.0), 1, "build: the caller is the only owner");
    Ok(())
}

#[tokio::test]
async fn teardown01_fresh_environment_has_one_strong_reference() -> TestResult {
    fresh_has_one_owner::<Queued>().await?;
    fresh_has_one_owner::<Inline>().await
}

// ---------------------------------------------------------------------------
// AC-2
// ---------------------------------------------------------------------------

async fn cached_does_not_own<K: AssetManagerKind>() -> TestResult {
    let envref = build::<K>(Arc::new(Semaphore::new(0)), None).await?;
    let query_asset = envref.evaluate("hello").await?;
    assert_eq!(query_asset.get().await?.try_into_string()?, "hi");
    let key_asset = envref.get_asset_manager().get(&parse_key("data/x.txt")?).await?;
    assert_eq!(key_asset.get().await?.try_into_string()?, "stored");
    drop(query_asset);
    drop(key_asset);
    assert!(
        eventually(|| Arc::strong_count(&envref.0) == 1).await,
        "cached assets must not own the environment (count {})",
        Arc::strong_count(&envref.0)
    );
    // Still cached: the query is answered from the manager's map.
    assert!(envref.get_asset_manager().lookup_query_asset(&parse_query("hello")?).is_some());
    Ok(())
}

#[tokio::test]
async fn teardown02_cached_query_and_key_do_not_own_environment() -> TestResult {
    cached_does_not_own::<Queued>().await?;
    cached_does_not_own::<Inline>().await
}

// ---------------------------------------------------------------------------
// AC-3
// ---------------------------------------------------------------------------

async fn last_envref_drops_all<K: AssetManagerKind>() -> TestResult {
    let envref = build::<K>(Arc::new(Semaphore::new(0)), None).await?;
    let state = envref.evaluate("hello").await?.get().await?;
    let value: Weak<Value> = Arc::downgrade(state.data_unchecked());
    drop(state);
    // A real `interpreter::evaluate` too, which goes through a temporary asset.
    let evaluated = liquers_core::interpreter::evaluate(envref.clone(), "hello", None).await?;
    assert_eq!(evaluated.try_into_string()?, "hi");
    let env: Weak<Env<K>> = Arc::downgrade(&envref.0);
    drop(envref);
    assert!(eventually(|| env.upgrade().is_none()).await, "the environment leaked");
    assert!(value.upgrade().is_none(), "the cached value outlived its environment");
    Ok(())
}

#[tokio::test]
async fn teardown03_last_envref_drops_environment_and_values() -> TestResult {
    last_envref_drops_all::<Queued>().await?;
    last_envref_drops_all::<Inline>().await
}

// ---------------------------------------------------------------------------
// AC-4
// ---------------------------------------------------------------------------

async fn repeated_retains_nothing<K: AssetManagerKind>() -> TestResult {
    let mut weaks: Vec<Weak<Env<K>>> = Vec::new();
    for _ in 0..100 {
        let envref = build::<K>(Arc::new(Semaphore::new(0)), None).await?;
        assert_eq!(envref.evaluate("hello").await?.get().await?.try_into_string()?, "hi");
        weaks.push(Arc::downgrade(&envref.0));
    }
    assert!(
        eventually(|| weaks.iter().all(|w| w.upgrade().is_none())).await,
        "{} of 100 environments are still alive",
        weaks.iter().filter(|w| w.upgrade().is_some()).count()
    );
    Ok(())
}

#[tokio::test]
async fn teardown04_repeated_build_and_drop_retains_nothing() -> TestResult {
    repeated_retains_nothing::<Queued>().await?;
    repeated_retains_nothing::<Inline>().await
}

// ---------------------------------------------------------------------------
// AC-5
// ---------------------------------------------------------------------------

/// Queued: capacity 1, so the second job is parked in the queue when the caller lets go.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn teardown05_queued_asset_survives_dropped_envref() -> TestResult {
    let gate = Arc::new(Semaphore::new(0));
    let envref = build::<Queued>(gate.clone(), Some(1)).await?;
    let first = envref.evaluate("gated-a").await?;
    let second = envref.evaluate("gated-b").await?;
    let env = Arc::downgrade(&envref.0);
    drop(envref);
    assert!(env.upgrade().is_some(), "work in flight keeps its environment");
    gate.add_permits(2);
    let a = tokio::time::timeout(Duration::from_secs(5), first.get()).await??;
    let b = tokio::time::timeout(Duration::from_secs(5), second.get()).await??;
    assert_eq!((a.try_into_string()?, b.try_into_string()?), ("a".into(), "b".into()));
    drop((first, second));
    assert!(eventually(|| env.upgrade().is_none()).await, "dropped once the work finished");
    Ok(())
}

/// Inline: evaluation completes before `evaluate` returns; the asset is still readable after.
#[tokio::test]
async fn teardown05_inline_asset_survives_dropped_envref() -> TestResult {
    let gate = Arc::new(Semaphore::new(1));
    let envref = build::<Inline>(gate, None).await?;
    let asset = envref.evaluate("gated-a").await?;
    let env = Arc::downgrade(&envref.0);
    drop(envref);
    assert_eq!(asset.get().await?.try_into_string()?, "a");
    drop(asset);
    assert!(eventually(|| env.upgrade().is_none()).await);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn teardown06_context_envref_usable_in_command() -> TestResult {
    let gate = Arc::new(Semaphore::new(0));
    let envref = build::<Queued>(gate.clone(), None).await?;
    let expected = envref.get_async_store().store_name();
    let asset = envref.evaluate("store_name").await?;
    drop(envref);
    // The command runs only now, after the caller's last `EnvRef` is gone.
    gate.add_permits(1);
    let state = tokio::time::timeout(Duration::from_secs(5), asset.get()).await??;
    assert_eq!(state.try_into_string()?, expected);

    let envref = build::<Inline>(Arc::new(Semaphore::new(1)), None).await?;
    let asset = envref.evaluate("store_name").await?;
    drop(envref);
    assert_eq!(asset.get().await?.try_into_string()?, expected);
    Ok(())
}

// ---------------------------------------------------------------------------
// AC-6, AC-7 (queued only: the inline manager spawns nothing)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn teardown07_queued_manager_tasks_exit() -> TestResult {
    let baseline = alive_tasks();
    let envref = build::<Queued>(Arc::new(Semaphore::new(0)), None).await?;
    assert_eq!(envref.evaluate("hello").await?.get().await?.try_into_string()?, "hi");
    assert!(alive_tasks() > baseline, "the queue and monitor tasks run");
    drop(envref);
    assert!(
        eventually(|| alive_tasks() == baseline).await,
        "{} tasks left behind",
        alive_tasks() - baseline
    );
    Ok(())
}

#[tokio::test]
async fn teardown08_temporary_asset_loop_exits() -> TestResult {
    let envref = build::<Queued>(Arc::new(Semaphore::new(0)), None).await?;
    // Let the manager's own tasks start, so the baseline counts them.
    tokio::task::yield_now().await;
    let baseline = alive_tasks();
    for _ in 0..10 {
        let state = liquers_core::interpreter::evaluate(envref.clone(), "hello", None).await?;
        assert_eq!(state.try_into_string()?, "hi");
    }
    assert!(
        eventually(|| alive_tasks() == baseline).await,
        "{} service loops left behind",
        alive_tasks() - baseline
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// AC-8
// ---------------------------------------------------------------------------

async fn manager_after_teardown<K: AssetManagerKind>() -> TestResult {
    let envref = build::<K>(Arc::new(Semaphore::new(0)), None).await?;
    let manager = envref.get_asset_manager();
    let env = Arc::downgrade(&envref.0);
    drop(envref);
    assert!(eventually(|| env.upgrade().is_none()).await, "the manager must not own the environment");
    let result = manager.get_asset(&parse_query("hello")?).await;
    let error = result.err().ok_or("get_asset after teardown must fail")?;
    assert!(error.message.contains("environment"), "unexpected error: {error}");
    assert!(manager.try_get_envref().is_err());
    Ok(())
}

#[tokio::test]
async fn teardown09_manager_after_teardown_returns_error() -> TestResult {
    manager_after_teardown::<Queued>().await?;
    manager_after_teardown::<Inline>().await
}

/// The source example of `TEMPORARY-ASSET-NEVER-RUN-IS-KEPT-ALIVE-BY-ITS-SERVICE-LOOP`: a temporary
/// asset that is never run is freed with its last reference, and its loop ends (AC-7).
#[tokio::test]
async fn teardown10_never_run_temporary_asset_is_freed() -> TestResult {
    let envref = build::<Queued>(Arc::new(Semaphore::new(0)), None).await?;
    tokio::task::yield_now().await;
    let baseline = alive_tasks();
    let asset = liquers_core::assets::AssetRef::new_temporary(envref.clone());
    let weak = asset.downgrade();
    drop(asset);
    assert!(weak.upgrade().is_none(), "the service loop must not own the asset");
    assert!(eventually(|| alive_tasks() == baseline).await, "the service loop must end");
    Ok(())
}

/// A keyed asset retained past teardown: `expire` reports the dropped environment rather than
/// panicking while it collects what to persist (AC-8; PR #176 review).
async fn expire_after_teardown<K: AssetManagerKind>() -> TestResult {
    let envref = build::<K>(Arc::new(Semaphore::new(0)), None).await?;
    let asset = envref.get_asset_manager().get(&parse_key("data/ready.txt")?).await?;
    assert_eq!(asset.get().await?.try_into_string()?, "computed");
    assert_eq!(asset.status().await, Status::Ready);
    let env = Arc::downgrade(&envref.0);
    drop(envref);
    assert!(eventually(|| env.upgrade().is_none()).await, "the asset must not own the environment");
    let error = asset.expire().await.err().ok_or("expire after teardown must fail")?;
    assert!(error.message.contains("environment"), "unexpected error: {error}");
    Ok(())
}

#[tokio::test]
async fn teardown11_expire_after_teardown_returns_error() -> TestResult {
    expire_after_teardown::<Queued>().await?;
    expire_after_teardown::<Inline>().await
}
