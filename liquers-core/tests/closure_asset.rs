//! A closure run as an ad-hoc asset (`specs/design/ad-hoc-closure-asset/`, AC-1 to AC-7).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use liquers_core::{
    assets::AssetManager,
    context::{EnvRef, Environment, SimpleEnvironment},
    error::Error,
    metadata::{Metadata, MetadataRecord, Status},
    parse::{parse_key, parse_query},
    query::Key,
    store::{AsyncMemoryStore, AsyncStore},
    value::{Value, ValueInterface},
};

type TestEnv = SimpleEnvironment<Value>;
type TestResult = Result<(), Box<dyn std::error::Error>>;

/// An environment whose store holds `a.txt` = `"A"`.
async fn environment() -> Result<EnvRef<TestEnv>, Error> {
    let store = AsyncMemoryStore::new(&Key::new());
    let mut record = MetadataRecord::new();
    record.status = Status::Ready;
    record.with_type_identifier("Text".to_owned());
    record.data_format = Some("txt".to_owned());
    store
        .set(&parse_key("a.txt")?, b"A", &Metadata::MetadataRecord(record))
        .await?;
    let mut env = TestEnv::new();
    env.with_async_store(Box::new(store));
    Ok(env.to_ref())
}

/// Sets `flag` when dropped: proves a body's future was dropped.
struct DropFlag(Arc<AtomicBool>);
impl Drop for DropFlag {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

#[tokio::test]
async fn closure01_runs_value_and_result() -> TestResult {
    let envref = environment().await?;
    let manager = envref.get_asset_manager();
    let keys_before = envref.get_async_store().keys().await?;
    let closure = manager.closure_asset("t")?;
    let asset = closure.asset().clone();
    let other = manager.closure_asset("t")?;
    assert_ne!(asset.id(), other.asset().id());
    let result = closure
        .run(|_context| async move { Ok((Value::from("x"), 7)) }, None)
        .await?;
    assert_eq!(result, Some(7));
    assert_eq!(asset.status().await, Status::Ready);
    assert_eq!(asset.get().await?.try_into_string()?, "x");
    assert_eq!(asset.key().await, None);
    // AC-7: nothing reached the store.
    assert_eq!(envref.get_async_store().keys().await?, keys_before);
    Ok(())
}

#[tokio::test]
async fn closure02_context_logs_and_records_dependencies() -> TestResult {
    let envref = environment().await?;
    let closure = envref.get_asset_manager().closure_asset("deps")?;
    let asset = closure.asset().clone();
    let read = closure
        .run(
            |context| async move {
                context.info("hello")?;
                let state = context.get_dependency_state(&parse_query("-R/a.txt")?).await?;
                let text = state.try_into_string()?;
                Ok((Value::from(text.clone()), text))
            },
            None,
        )
        .await?;
    assert_eq!(read.as_deref(), Some("A"));
    let metadata = asset.get_metadata().await?;
    let record = metadata.metadata_record().ok_or("no metadata record")?;
    assert!(record.log.iter().any(|entry| entry.message == "hello"), "{:?}", record.log);
    let names: Vec<String> = record
        .dependencies
        .iter()
        .map(|d| format!("{:?}", d.key))
        .collect();
    assert!(names.iter().any(|n| n.contains("a.txt")), "{names:?}");
    Ok(())
}

#[tokio::test]
async fn closure03_cancel_while_running() -> TestResult {
    let envref = environment().await?;
    let closure = envref.get_asset_manager().closure_asset("forever")?;
    let asset = closure.asset().clone();
    let dropped = Arc::new(AtomicBool::new(false));
    let started = Arc::new(tokio::sync::Notify::new());
    let (flag, started_in) = (dropped.clone(), started.clone());
    let run = tokio::spawn(async move {
        closure
            .run(
                move |_context| async move {
                    let _guard = DropFlag(flag);
                    started_in.notify_one();
                    tokio::sync::Notify::new().notified().await; // never notified
                    Ok((Value::none(), ()))
                },
                None,
            )
            .await
    });
    started.notified().await;
    asset.cancel().await?;
    let result = tokio::time::timeout(Duration::from_secs(5), run).await??;
    let error = result.expect_err("a cancelled run is an error");
    assert!(error.is_cancelled(), "{error}");
    assert_eq!(asset.status().await, Status::Cancelled);
    assert!(dropped.load(Ordering::SeqCst));
    Ok(())
}

#[tokio::test]
async fn closure04_cancel_before_run() -> TestResult {
    let envref = environment().await?;
    let closure = envref.get_asset_manager().closure_asset("never")?;
    let asset = closure.asset().clone();
    assert_eq!(asset.status().await, Status::Submitted);
    asset.cancel().await?;
    let called = Arc::new(AtomicBool::new(false));
    let flag = called.clone();
    let result = closure
        .run(
            move |_context| async move {
                flag.store(true, Ordering::SeqCst);
                Ok((Value::none(), ()))
            },
            None,
        )
        .await?;
    assert_eq!(result, None);
    assert!(!called.load(Ordering::SeqCst));
    assert_eq!(asset.status().await, Status::Cancelled);
    Ok(())
}

#[tokio::test]
async fn closure05_error_and_panic_end_in_error() -> TestResult {
    let envref = environment().await?;
    let manager = envref.get_asset_manager();

    let failing = manager.closure_asset("fails")?;
    let failed = failing.asset().clone();
    let error = failing
        .run(
            |_context| async move {
                Err::<(Value, ()), _>(Error::general_error("body failed".to_string()))
            },
            None,
        )
        .await
        .expect_err("the body's error");
    assert!(error.to_string().contains("body failed"), "{error}");
    assert_eq!(failed.status().await, Status::Error);

    let panicking = manager.closure_asset("panics")?;
    let panicked = panicking.asset().clone();
    let error = panicking
        .run(
            |_context| async move {
                if true {
                    panic!("the body panicked");
                }
                Ok((Value::none(), ()))
            },
            None,
        )
        .await
        .expect_err("the panic as an error");
    assert!(error.to_string().contains("the body panicked"), "{error}");
    assert_eq!(panicked.status().await, Status::Error);
    Ok(())
}

#[tokio::test]
async fn closure06_dropped_inline_run_is_cancelled() -> TestResult {
    let envref = environment().await?;
    let closure = envref.get_asset_manager().closure_asset("dropped")?;
    let asset = closure.asset().clone();
    {
        let run = closure.run_inline(
            |_context| async move {
                std::future::pending::<()>().await;
                Ok((Value::none(), ()))
            },
            None,
        );
        futures::pin_mut!(run);
        assert!(futures::poll!(run.as_mut()).is_pending());
        assert_eq!(asset.status().await, Status::Processing);
    } // the run is dropped here
    // Never repaired to its placeholder recipe, and a waiter is released.
    let state = tokio::time::timeout(Duration::from_secs(5), asset.get()).await??;
    assert_eq!(asset.status().await, Status::Cancelled);
    assert!(state.is_error().unwrap_or(true) || state.metadata.status() == Status::Cancelled);
    Ok(())
}

/// A caller that drops its last `EnvRef` while the closure runs still gets the closure's result:
/// the run keeps the environment alive until it has completed.
#[tokio::test]
async fn closure07_environment_dropped_mid_run() -> TestResult {
    let envref = environment().await?;
    let closure = envref.get_asset_manager().closure_asset("pinned")?;
    let asset = closure.asset().clone();
    let (started_tx, started_rx) = tokio::sync::oneshot::channel::<()>();
    let (release_tx, release_rx) = tokio::sync::oneshot::channel::<()>();
    let run = tokio::spawn(closure.run(
        move |_context| async move {
            let _ = started_tx.send(());
            let _ = release_rx.await;
            Ok((Value::from("x"), 7))
        },
        None,
    ));
    started_rx.await?;
    drop(envref);
    let _ = release_tx.send(());
    let result = tokio::time::timeout(Duration::from_secs(5), run).await???;
    assert_eq!(result, Some(7));
    assert_eq!(asset.status().await, Status::Ready);
    Ok(())
}
