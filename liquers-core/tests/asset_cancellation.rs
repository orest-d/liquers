//! Deterministic asset cancellation outcome and cooperative command cancellation.
//! Design: `specs/design/asset-cancellation-outcome/` (Phase 3, AC-1 to AC-12).
//!
//! The runtime is multi-threaded throughout: a synchronous command blocks its worker thread while
//! it runs, and the test has to be able to request the cancel from another one.

use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc,
};
use std::time::Duration;

use liquers_core::{
    assets::{AssetData, AssetManager, AssetRef},
    command_metadata::CommandKey,
    context::{Context, EnvRef, Environment, SimpleEnvironment},
    error::{Error, ErrorType},
    metadata::{LogEntryKind, Metadata, MetadataRecord, Status},
    parse::{parse_key, parse_query},
    query::Key,
    recipes::{DefaultRecipeProvider, Recipe, RecipeList},
    state::State,
    store::{AsyncMemoryStore, AsyncStore},
    value::Value,
};

type Env = SimpleEnvironment<Value>;

/// Fail fast instead of hanging.
async fn within<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(10), future)
        .await
        .expect("did not finish within 10 s")
}

/// Wait until `asset` has `status`.
async fn wait_for_status(asset: &AssetRef<Env>, status: Status) {
    within(async {
        while asset.status().await != status {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
}

/// Wait until `asset` reached a finished status, and return it.
async fn wait_finished(asset: &AssetRef<Env>) -> Status {
    within(async {
        loop {
            let status = asset.status().await;
            if status.is_finished() {
                return status;
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
}

/// Wait until a command set `flag`: it has started. A status alone does not say so — a queued
/// asset is `Processing` from the moment it is claimed, before its command runs, and a cancel
/// requested in that window correctly skips the command (AC-4).
async fn wait_started(flag: &AtomicBool) {
    within(until(flag)).await
}

/// Sleep in short steps until `flag` is set. Survives a re-run of the command.
async fn until(flag: &AtomicBool) {
    while !flag.load(Ordering::SeqCst) {
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
}

/// An environment over a memory store, with `recipes` (query, title) at the root, after
/// `register` added its commands.
async fn env_with(
    recipes: &[&str],
    register: impl FnOnce(&mut Env),
) -> EnvRef<Env> {
    let mut rl = RecipeList::new();
    for q in recipes {
        rl.add_recipe(Recipe::new(q.to_string(), "Recipe".to_string(), String::new()).unwrap());
    }
    let store = AsyncMemoryStore::new(&Key::new());
    store
        .set(
            &parse_key("recipes.yaml").unwrap(),
            serde_yaml::to_string(&rl).unwrap().as_bytes(),
            &Metadata::new(),
        )
        .await
        .unwrap();
    let mut env: Env = SimpleEnvironment::new();
    register(&mut env);
    env.with_async_store(Box::new(store));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider::new()));
    env.to_ref()
}

/// The error read back from a finished asset.
async fn value_error(asset: &AssetRef<Env>) -> Option<Error> {
    asset.poll_state().await.and_then(|s| s.value_error())
}

/// Whether the store holds a value (not merely metadata) for `key`.
async fn stored_bytes(envref: &EnvRef<Env>, key: &Key) -> Option<Vec<u8>> {
    envref.get_async_store().get_bytes(key).await.ok().filter(|b| !b.is_empty())
}

/// AC-1, AC-5 (sync): a synchronous command sees the cancel, finishes anyway and returns `Ok`;
/// the asset ends `Ready`, its value is stored, and the request is dropped.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ac01_completed_sync_run_wins_over_late_cancel() -> Result<(), Box<dyn std::error::Error>> {
    let saw_before = Arc::new(AtomicBool::new(true));
    let saw = saw_before.clone();
    let started = Arc::new(AtomicBool::new(false));
    let start = started.clone();
    let envref = env_with(&["finish_anyway/out.txt"], move |env| {
        env.command_registry
            .register_command(CommandKey::new_name("finish_anyway"), move |_, _, context: Context<Env>| {
                saw.store(context.is_cancelled(), Ordering::SeqCst);
                start.store(true, Ordering::SeqCst);
                while !context.is_cancelled() {
                    std::thread::sleep(Duration::from_millis(2));
                }
                Ok(Value::from("finished"))
            })
            .unwrap();
    })
    .await;
    let am = envref.get_asset_manager();
    let key = parse_key("out.txt")?;

    let asset = am.get(&key).await?;
    wait_started(&started).await;
    within(asset.cancel()).await?;

    assert_eq!(wait_finished(&asset).await, Status::Ready);
    assert!(!saw_before.load(Ordering::SeqCst), "is_cancelled() is false before the cancel");
    assert_eq!(asset.get().await?.try_into_string()?, "finished");
    assert!(!asset.is_cancelled().await, "a run that finished drops the request");
    within(async {
        while stored_bytes(&envref, &key).await.is_none() {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await;
    assert_eq!(stored_bytes(&envref, &key).await.unwrap(), b"finished");
    Ok(())
}

/// AC-2, AC-10: an async command suspended at an await point is dropped; the asset ends
/// `Cancelled` holding no value, nothing is written, and the cause names its key.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ac02_cancel_suspended_async_command() -> Result<(), Box<dyn std::error::Error>> {
    let started = Arc::new(AtomicBool::new(false));
    let start = started.clone();
    let envref = env_with(&["hang/out.txt"], move |env| {
        env.command_registry
            .register_async_command(CommandKey::new_name("hang"), move |_, _, _| {
                start.store(true, Ordering::SeqCst);
                Box::pin(async move {
                    tokio::time::sleep(Duration::from_secs(60)).await;
                    Ok(Value::from("never"))
                })
            })
            .unwrap();
    })
    .await;
    let am = envref.get_asset_manager();
    let key = parse_key("out.txt")?;

    let asset = am.get(&key).await?;
    wait_started(&started).await;
    within(asset.cancel()).await?;

    assert_eq!(wait_finished(&asset).await, Status::Cancelled);
    let error = value_error(&asset).await.expect("a cancelled asset has a cancellation error");
    assert_eq!(error.error_type, ErrorType::Cancelled);
    assert_eq!(error.query.as_deref(), Some(key.encode().as_str()));
    let metadata = asset.get_metadata().await?;
    assert!(!metadata.is_error().unwrap_or(true), "a cancellation is not a failure");
    assert_eq!(stored_bytes(&envref, &key).await, None, "no value is written");
    Ok(())
}

/// AC-3: cancelling an asset that waits for a dependency cancels it alone; the dependency
/// finishes normally.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ac03_cancel_while_waiting_for_dependency() -> Result<(), Box<dyn std::error::Error>> {
    let release = Arc::new(AtomicBool::new(false));
    let gate = release.clone();
    let envref = env_with(&[], move |env| {
        env.command_registry
            .register_async_command(CommandKey::new_name("gated_dep"), move |_, _, _| {
                let gate = gate.clone();
                Box::pin(async move {
                    until(&gate).await;
                    Ok(Value::from("dependency value"))
                })
            })
            .unwrap();
        env.command_registry
            .register_async_command(CommandKey::new_name("waits_for_dep"), |_, _, context: Context<Env>| {
                Box::pin(async move {
                    let state = context
                        .get_dependency_state(&parse_query("gated_dep").unwrap())
                        .await?;
                    Ok(Value::from(state.try_into_string()?))
                })
            })
            .unwrap();
    })
    .await;
    let am = envref.get_asset_manager();

    let parent = am.get_asset(&parse_query("waits_for_dep")?).await?;
    wait_for_status(&parent, Status::Dependencies).await;
    within(parent.cancel()).await?;
    assert_eq!(wait_finished(&parent).await, Status::Cancelled);

    release.store(true, Ordering::SeqCst);
    let dependency = am.get_asset(&parse_query("gated_dep")?).await?;
    let state = within(dependency.get()).await?;
    assert_eq!(state.try_into_string()?, "dependency value");
    assert_eq!(dependency.status().await, Status::Ready);
    Ok(())
}

/// AC-4: a cancel of a `Submitted` asset ends it `Cancelled` at once, and running it afterwards
/// never invokes the command.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ac04_cancel_before_the_job_starts() -> Result<(), Box<dyn std::error::Error>> {
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = calls.clone();
    let envref = env_with(&[], move |env| {
        env.command_registry
            .register_command(CommandKey::new_name("counted"), move |_, _, _| {
                counter.fetch_add(1, Ordering::SeqCst);
                Ok(Value::from("ran"))
            })
            .unwrap();
    })
    .await;
    let asset =
        AssetData::<Env>::new(4001, parse_query("counted")?.into(), None, envref.clone()).to_ref();
    asset.submitted().await?;
    assert_eq!(asset.status().await, Status::Submitted);

    within(asset.cancel()).await?;
    assert_eq!(asset.status().await, Status::Cancelled);
    asset.run(None).await?;

    assert_eq!(asset.status().await, Status::Cancelled);
    assert_eq!(calls.load(Ordering::SeqCst), 0, "the command is never invoked");
    asset.submitted().await?;
    assert_eq!(asset.status().await, Status::Cancelled, "re-parking does not revive it");
    Ok(())
}

/// AC-5 (async), AC-6: an async command polls `is_cancelled`, then returns the error of
/// `check_cancelled`; the asset ends `Cancelled`, not `Error`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ac05_async_command_checks_cancellation() -> Result<(), Box<dyn std::error::Error>> {
    let first_check = Arc::new(AtomicBool::new(true));
    let first = first_check.clone();
    let started = Arc::new(AtomicBool::new(false));
    let start = started.clone();
    let envref = env_with(&[], move |env| {
        env.command_registry
            .register_async_command(CommandKey::new_name("polite"), move |_, _, context: Context<Env>| {
                let first = first.clone();
                let start = start.clone();
                Box::pin(async move {
                    first.store(context.is_cancelled(), Ordering::SeqCst);
                    start.store(true, Ordering::SeqCst);
                    while !context.is_cancelled() {
                        // A blocking sleep: no await point, so only the check can stop it.
                        std::thread::sleep(Duration::from_millis(2));
                    }
                    context.check_cancelled()?;
                    Ok(Value::from("unreachable"))
                })
            })
            .unwrap();
    })
    .await;
    let asset = envref.get_asset_manager().get_asset(&parse_query("polite")?).await?;
    wait_started(&started).await;
    within(asset.cancel()).await?;

    assert_eq!(wait_finished(&asset).await, Status::Cancelled);
    assert!(!first_check.load(Ordering::SeqCst));
    let error = value_error(&asset).await.expect("cancellation error");
    assert_eq!(error.error_type, ErrorType::Cancelled);
    assert_eq!(error.query.as_deref(), Some("polite"));
    Ok(())
}

/// AC-6: a command's own cancellation error, with no cancel requested, ends the asset
/// `Cancelled`; nothing is stored, and the cause gets the asset's key.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ac06_cancellation_error_ends_cancelled() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&["gives_up/out.txt"], |env| {
        env.command_registry
            .register_command(CommandKey::new_name("gives_up"), |_, _, _| {
                Err(Error::cancelled("gave up".to_string()))
            })
            .unwrap();
    })
    .await;
    let key = parse_key("out.txt")?;
    let asset = envref.get_asset_manager().get(&key).await?;

    assert_eq!(wait_finished(&asset).await, Status::Cancelled);
    let error = value_error(&asset).await.expect("cancellation error");
    assert_eq!(error.error_type, ErrorType::Cancelled);
    assert!(error.message.contains("gave up"), "{}", error.message);
    assert_eq!(error.query.as_deref(), Some(key.encode().as_str()));
    assert_eq!(stored_bytes(&envref, &key).await, None);
    Ok(())
}

/// Record every distinct status `asset` passes through until a moment after it finished.
async fn status_trace(asset: AssetRef<Env>) -> Vec<Status> {
    let mut trace: Vec<Status> = Vec::new();
    let mut settled = 0;
    while settled < 200 {
        let status = asset.status().await;
        if trace.last() != Some(&status) {
            trace.push(status);
        }
        if status.is_finished() {
            settled += 1;
        }
        tokio::task::yield_now().await;
    }
    trace
}

/// AC-7: when cancellation races completion, exactly one terminal status is observed, and the
/// asset never leaves it. The command returns at varying moments around the request.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ac07_one_terminal_status_per_run() -> Result<(), Box<dyn std::error::Error>> {
    for round in 0..10u64 {
        let started = Arc::new(AtomicBool::new(false));
        let release = Arc::new(AtomicBool::new(false));
        let (start, gate) = (started.clone(), release.clone());
        let envref = env_with(&[], move |env| {
            env.command_registry
                .register_command(CommandKey::new_name("short"), move |_, _, _| {
                    start.store(true, Ordering::SeqCst);
                    while !gate.load(Ordering::SeqCst) {
                        std::thread::sleep(Duration::from_micros(200));
                    }
                    Ok(Value::from("short"))
                })
                .unwrap();
        })
        .await;
        let asset = AssetData::<Env>::new(7000 + round, parse_query("short")?.into(), None, envref)
            .to_ref();
        let tracer = tokio::spawn(status_trace(asset.clone()));
        let runner = {
            let asset = asset.clone();
            tokio::spawn(async move { asset.run(None).await })
        };
        wait_started(&started).await;
        let canceller = {
            let asset = asset.clone();
            tokio::spawn(async move { asset.cancel().await })
        };
        // Release the command before, around or after the request lands.
        tokio::time::sleep(Duration::from_micros(300 * (round % 5))).await;
        release.store(true, Ordering::SeqCst);
        within(canceller).await??;
        let _ = within(runner).await?;
        let trace = within(tracer).await?;
        let finished: Vec<_> = trace.iter().filter(|s| s.is_finished()).collect();
        assert_eq!(finished.len(), 1, "round {round}: {trace:?}");
        assert_eq!(finished[0], &Status::Ready, "round {round}: a sync command completes");
    }
    Ok(())
}

/// AC-8: a replacement made while a sync command runs is not overwritten by its late result,
/// in memory or in the store.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ac08_replacement_is_not_overwritten() -> Result<(), Box<dyn std::error::Error>> {
    let release = Arc::new(AtomicBool::new(false));
    let gate = release.clone();
    let started = Arc::new(AtomicBool::new(false));
    let start = started.clone();
    let envref = env_with(&["late/out.txt"], move |env| {
        env.command_registry
            .register_command(CommandKey::new_name("late"), move |_, _, _| {
                start.store(true, Ordering::SeqCst);
                while !gate.load(Ordering::SeqCst) {
                    std::thread::sleep(Duration::from_millis(2));
                }
                Ok(Value::from("late"))
            })
            .unwrap();
    })
    .await;
    let am = envref.get_asset_manager();
    let key = parse_key("out.txt")?;

    let replaced = am.get(&key).await?;
    wait_started(&started).await;
    let metadata = MetadataRecord {
        type_identifier: "Text".to_string(),
        type_name: "text".to_string(),
        data_format: Some("txt".to_string()),
        ..Default::default()
    };
    within(am.set_binary(&key, b"replacement", metadata)).await?;
    assert_eq!(replaced.status().await, Status::Cancelled);

    release.store(true, Ordering::SeqCst);
    // Let the late result arrive (and be discarded).
    tokio::time::sleep(Duration::from_millis(50)).await;

    assert_eq!(replaced.status().await, Status::Cancelled);
    assert_eq!(stored_bytes(&envref, &key).await.unwrap(), b"replacement");
    let current = am.get(&key).await?;
    assert_eq!(current.get().await?.try_into_string()?, "replacement");
    Ok(())
}

/// AC-8 for `AssetRef::to_override`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ac08_to_override_discards_the_late_result() -> Result<(), Box<dyn std::error::Error>> {
    let release = Arc::new(AtomicBool::new(false));
    let gate = release.clone();
    let started = Arc::new(AtomicBool::new(false));
    let start = started.clone();
    let envref = env_with(&[], move |env| {
        env.command_registry
            .register_command(CommandKey::new_name("late"), move |_, _, _| {
                start.store(true, Ordering::SeqCst);
                while !gate.load(Ordering::SeqCst) {
                    std::thread::sleep(Duration::from_millis(2));
                }
                Ok(Value::from("late"))
            })
            .unwrap();
    })
    .await;
    let asset =
        AssetData::<Env>::new(8001, parse_query("late")?.into(), None, envref).to_ref();
    let runner = {
        let asset = asset.clone();
        tokio::spawn(async move { asset.run(None).await })
    };
    wait_started(&started).await;
    within(asset.to_override()).await?;
    release.store(true, Ordering::SeqCst);
    let _ = within(runner).await?;

    assert_eq!(asset.status().await, Status::Override);
    let state = asset.poll_state().await.expect("an override is readable");
    assert!(state.try_into_string().map(|s| s != "late").unwrap_or(true));
    Ok(())
}

/// AC-9, AC-10, AC-11: a cancelled dependency cascades through every dependent that does not
/// handle it; each ends `Cancelled` naming the root cause, and logs one cascade warning.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ac09_cascade_names_the_root_cause() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[], |env| {
        env.command_registry
            .register_async_command(CommandKey::new_name("root_dep"), |_, _, _| {
                Box::pin(async move {
                    tokio::time::sleep(Duration::from_secs(60)).await;
                    Ok(Value::from("never"))
                })
            })
            .unwrap();
        for (name, inner) in [("middle", "root_dep"), ("top", "middle")] {
            env.command_registry
                .register_async_command(CommandKey::new_name(name), move |_, _, context: Context<Env>| {
                    Box::pin(async move {
                        let state = context
                            .get_dependency_state(&parse_query(inner).unwrap())
                            .await?;
                        Ok(Value::from(state.try_into_string()?))
                    })
                })
                .unwrap();
        }
    })
    .await;
    let am = envref.get_asset_manager();

    let top = am.get_asset(&parse_query("top")?).await?;
    let root = am.get_asset(&parse_query("root_dep")?).await?;
    wait_for_status(&root, Status::Processing).await;
    let middle = am.get_asset(&parse_query("middle")?).await?;
    wait_for_status(&middle, Status::Dependencies).await;
    within(root.cancel()).await?;

    for asset in [&root, &middle, &top] {
        assert_eq!(wait_finished(asset).await, Status::Cancelled);
        let error = value_error(asset).await.expect("cancellation error");
        assert_eq!(error.error_type, ErrorType::Cancelled);
        assert_eq!(error.query.as_deref(), Some("root_dep"), "root cause at every depth");
    }
    for (asset, dependency) in [(&middle, "root_dep"), (&top, "middle")] {
        let MetadataRecordLog(log) = log_of(asset).await;
        let warnings: Vec<_> = log
            .iter()
            .filter(|(kind, message)| {
                *kind == LogEntryKind::Warning && message.contains("was cancelled; root cause")
            })
            .collect();
        assert_eq!(warnings.len(), 1, "{log:?}");
        assert!(warnings[0].1.contains(dependency), "{:?}", warnings[0]);
        assert!(warnings[0].1.contains("root_dep"), "{:?}", warnings[0]);
    }
    let root_log = log_of(&root).await.0;
    assert!(
        !root_log.iter().any(|(kind, _)| *kind == LogEntryKind::Warning),
        "the asset cancelled directly logs no cascade warning: {root_log:?}"
    );
    Ok(())
}

struct MetadataRecordLog(Vec<(LogEntryKind, String)>);

async fn log_of(asset: &AssetRef<Env>) -> MetadataRecordLog {
    let metadata = asset.get_metadata().await.unwrap();
    let record = metadata.metadata_record().expect("a metadata record");
    MetadataRecordLog(record.log.iter().map(|e| (e.kind.clone(), e.message.clone())).collect())
}

/// AC-9: a dependent whose command handles the cancellation error finishes normally.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ac09_handled_cascade_finishes_ready() -> Result<(), Box<dyn std::error::Error>> {
    let envref = env_with(&[], |env| {
        env.command_registry
            .register_async_command(CommandKey::new_name("root_dep"), |_, _, _| {
                Box::pin(async move {
                    tokio::time::sleep(Duration::from_secs(60)).await;
                    Ok(Value::from("never"))
                })
            })
            .unwrap();
        env.command_registry
            .register_async_command(CommandKey::new_name("tolerant"), |_, _, context: Context<Env>| {
                Box::pin(async move {
                    match context.get_dependency_state(&parse_query("root_dep").unwrap()).await {
                        Ok(state) => Ok(Value::from(state.try_into_string()?)),
                        Err(e) if e.is_cancelled() => Ok(Value::from("fallback")),
                        Err(e) => Err(e),
                    }
                })
            })
            .unwrap();
    })
    .await;
    let am = envref.get_asset_manager();
    let tolerant = am.get_asset(&parse_query("tolerant")?).await?;
    wait_for_status(&tolerant, Status::Dependencies).await;
    let root = am.get_asset(&parse_query("root_dep")?).await?;
    within(root.cancel()).await?;

    assert_eq!(wait_finished(&tolerant).await, Status::Ready);
    assert_eq!(tolerant.get().await?.try_into_string()?, "fallback");
    Ok(())
}

/// AC-12: a cancelled keyed asset is described as `Cancelled` with its cause until requested
/// again; the next `get` re-evaluates it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ac12_cancelled_keyed_asset_info_then_reevaluated() -> Result<(), Box<dyn std::error::Error>>
{
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = calls.clone();
    let started = Arc::new(AtomicBool::new(false));
    let start = started.clone();
    let envref = env_with(&["first_hangs/out.txt"], move |env| {
        env.command_registry
            .register_async_command(CommandKey::new_name("first_hangs"), move |_, _, _| {
                let call = counter.fetch_add(1, Ordering::SeqCst);
                start.store(true, Ordering::SeqCst);
                Box::pin(async move {
                    if call == 0 {
                        tokio::time::sleep(Duration::from_secs(60)).await;
                    }
                    Ok(Value::from("second time"))
                })
            })
            .unwrap();
    })
    .await;
    let am = envref.get_asset_manager();
    let key = parse_key("out.txt")?;

    let first = am.get(&key).await?;
    wait_started(&started).await;
    within(first.cancel()).await?;
    wait_for_status(&first, Status::Cancelled).await;

    let info = am.get_asset_info(&key).await?;
    assert_eq!(info.status, Status::Cancelled);
    let cause = info.error_data.expect("the cause is reported");
    assert_eq!(cause.error_type, ErrorType::Cancelled);
    assert_eq!(cause.query.as_deref(), Some(key.encode().as_str()));

    let second = am.get(&key).await?;
    assert_ne!(second.id(), first.id(), "a cancelled entry is a cache miss");
    assert_eq!(within(second.get()).await?.try_into_string()?, "second time");
    Ok(())
}

/// A state built from a cancelled record returns the recorded cause from `value_error`.
#[test]
fn cancelled_state_returns_recorded_cause() {
    let mut record = MetadataRecord::new();
    record.with_cancellation(Error::cancelled("Asset x was cancelled".to_string()).with_query(
        &parse_query("x").unwrap(),
    ));
    let state = State::<Value>::new().with_metadata(Metadata::MetadataRecord(record));
    let error = state.value_error().expect("cancelled");
    assert_eq!(error.error_type, ErrorType::Cancelled);
    assert_eq!(error.query.as_deref(), Some("x"));
    assert!(!state.metadata.is_error().unwrap_or(true));
}
