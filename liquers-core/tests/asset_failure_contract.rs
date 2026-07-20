//! WP-2 — Asset terminal outcome contract.
//!
//! A finished asset has exactly one observable terminal `State`. `get()` returns `Ok(state)`
//! for any obtained terminal outcome (value OR error/cancelled); the error surfaces only when a
//! *value* is requested (`value_state()` / value extractors). Failure preserves the metadata
//! audit trail. `Error`/`Cancelled` are re-evaluated on a fresh manager request.

use std::sync::atomic::{AtomicUsize, Ordering};

use liquers_core::{
    assets::AssetManager,
    context::{Environment, SimpleEnvironment},
    error::{Error, ErrorType},
    metadata::{Metadata, Status},
    query::{Query, TryToQuery},
    state::State,
    value::Value,
};

fn q(s: &str) -> Query {
    s.try_to_query().unwrap()
}

fn boom_env() -> SimpleEnvironment<Value> {
    let mut env = SimpleEnvironment::<Value>::new();
    let key = liquers_core::command_metadata::CommandKey::new_name("boom");
    env.command_registry
        .register_command(
            key,
            |_state, _args, ctx| -> Result<Value, Error> {
                ctx.info("step1")?;
                Err(Error::general_error("boom".to_string()))
            },
        )
        .expect("register boom failed");
    env
}

/// I1 — a failing command yields Ok(error_state) from get(), repeatedly, and value extraction
/// errors with the computed message. (Red before: get() returned Err / Ok(none-state).)
#[tokio::test]
async fn test_failed_asset_get_returns_ok_error_state() {
    let query = q("boom");
    let env = boom_env();
    let envref = env.to_ref();
    let asset = envref
        .get_asset_manager()
        .get_asset(&query)
        .await
        .expect("get_asset");

    for _ in 0..3 {
        let state = asset.get().await.expect("get() must be Ok(error_state)");
        assert_eq!(state.status(), Status::Error);
        // Value extraction surfaces the computed error.
        assert!(state.try_into_string().is_err());
        assert!(state.value().is_err());
        let e = state.value_state().expect_err("value_state must be Err");
        assert!(e.message.contains("boom"), "message was: {}", e.message);
    }
}

/// I3 — failure preserves the metadata log/query (fail_asset uses with_error, not from_error).
#[tokio::test]
async fn test_failure_preserves_metadata_log() {
    let query = q("boom");
    let env = boom_env();
    let envref = env.to_ref();
    let asset = envref
        .get_asset_manager()
        .get_asset(&query)
        .await
        .expect("get_asset");
    let _ = asset.get().await.expect("get");

    let metadata = asset.get_metadata().await.expect("metadata");
    if let Metadata::MetadataRecord(mr) = &metadata {
        assert!(
            mr.log.iter().any(|e| e.message.contains("step1")),
            "log entry 'step1' must survive failure; log = {:?}",
            mr.log
        );
        assert!(mr.is_error, "metadata must record the error");
    } else {
        panic!("expected MetadataRecord");
    }
}

/// I7 — an Error asset is re-evaluated on a fresh manager request (cache miss). The counter
/// command fails on the first evaluation and succeeds on the second.
#[tokio::test]
async fn test_error_asset_reevaluated_on_rerequest() {
    static CALLS: AtomicUsize = AtomicUsize::new(0);
    CALLS.store(0, Ordering::SeqCst);

    let mut env = SimpleEnvironment::<Value>::new();
    let key = liquers_core::command_metadata::CommandKey::new_name("counter");
    env.command_registry
        .register_command(key, |_state, _args, _ctx| {
            let n = CALLS.fetch_add(1, Ordering::SeqCst) + 1;
            if n == 1 {
                Err(Error::general_error("transient".to_string()))
            } else {
                Ok(Value::from("ok"))
            }
        })
        .expect("register counter failed");
    let envref = env.to_ref();
    let query = q("counter");

    // First request evaluates and fails.
    let asset1 = envref.get_asset_manager().get_asset(&query).await.unwrap();
    let s1 = asset1.get().await.unwrap();
    assert_eq!(s1.status(), Status::Error);
    assert!(s1.value_state().is_err());

    // A fresh manager request treats the Error as a cache miss and re-evaluates -> success.
    let asset2 = envref.get_asset_manager().get_asset(&query).await.unwrap();
    let s2 = asset2.get().await.unwrap();
    assert_eq!(s2.status(), Status::Ready, "stale Error must be re-evaluated");
    assert_eq!(s2.value_state().unwrap().try_into_string().unwrap(), "ok");
}

/// U2/U3/U4 — value_error / value_state mapping over constructed states (no async needed).
#[test]
fn test_value_state_mapping() {
    // Value-bearing (Ready-like default): value extraction succeeds.
    let ok: State<Value> = State::new().with_data(Value::from("hi"));
    assert!(ok.value_error().is_none());
    assert_eq!(ok.value_state().unwrap().try_into_string().unwrap(), "hi");

    // Error state (via from_error): value extraction returns the stored error.
    let err_state: State<Value> = State::from_error(Error::general_error("kaboom".to_string()));
    let e = err_state.value_error().expect("error state must have a value_error");
    assert!(e.message.contains("kaboom"));
    // error_result mirrors the stored error for an error state.
    assert!(err_state.error_result().is_err());
    assert!(err_state.value_state().is_err());

    // Cancelled state: no stored error, but value extraction yields ErrorType::Cancelled.
    let mut cancelled: State<Value> = State::new();
    cancelled.set_status(Status::Cancelled).unwrap();
    // error_result is Ok for a cancelled state (no stored error) — the subtlety WP-2 fixes.
    assert!(cancelled.error_result().is_ok());
    let ce = cancelled.value_error().expect("cancelled must have a value_error");
    assert_eq!(ce.error_type, ErrorType::Cancelled);
    assert!(cancelled.value_state().is_err());
}

/// U1 — the typed cancellation constructor.
#[test]
fn test_error_cancelled_constructor() {
    let e = Error::cancelled("stopped");
    assert_eq!(e.error_type, ErrorType::Cancelled);
    assert!(e.is_cancelled());
    assert!(!Error::general_error("x".to_string()).is_cancelled());
}
