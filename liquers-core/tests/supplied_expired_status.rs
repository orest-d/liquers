//! A value supplied already `Expired` to `set_state` / `set_binary` is logged as expired.
//!
//! Every other route into `Expired` goes through `AssetManager::record_expiry`, which logs
//! "<subject> expired: <reason>". This route bypasses it, so the write itself logs the warning
//! "Asset expired" (the moment Liquers learns of the expiry) followed by an info entry saying the
//! diagnostics are recorded after the fact. The structured `expiry_reason` is kept as supplied.
//! Designs: `specs/design/supplied-expired-status-reason/`, `immediate-set-state-status-match/`.

use std::sync::Arc;

use liquers_core::{
    assets::AssetManager,
    context::{EnvRef, Environment, ImmediateEnvironment, SimpleEnvironment},
    error::Error,
    metadata::{
        DependencyKey, ExpiryCause, ExpiryReason, LogEntry, LogEntryKind, Metadata, MetadataRecord,
        Status,
    },
    parse::parse_key,
    query::Key,
    state::State,
    store::AsyncMemoryStore,
    value::Value,
};

fn simple() -> EnvRef<SimpleEnvironment<Value>> {
    let mut env = SimpleEnvironment::<Value>::new();
    env.with_async_store(Box::new(AsyncMemoryStore::new(&Key::new())));
    env.to_ref()
}

fn immediate() -> EnvRef<ImmediateEnvironment<Value>> {
    let mut env = ImmediateEnvironment::<Value>::new();
    env.with_async_store(Box::new(AsyncMemoryStore::new(&Key::new())));
    env.to_ref()
}

fn text_record(status: Status, reason: Option<ExpiryReason>) -> MetadataRecord {
    let mut record = MetadataRecord::new();
    record.with_type_identifier("Text".to_owned());
    record.with_type_name("text".to_owned());
    record.data_format = Some("txt".to_owned());
    record.with_status(status);
    record.expiry_reason = reason;
    record.log.push(LogEntry::info("supplied entry".to_string()));
    record
}

/// The log of the stored metadata for `key`, and its `expiry_reason`.
async fn stored<E: Environment>(
    envref: &EnvRef<E>,
    key: &Key,
) -> Result<(Vec<LogEntry>, Option<ExpiryReason>, Status), Error> {
    match envref.get_async_store().get_metadata(key).await? {
        Metadata::MetadataRecord(record) => {
            Ok((record.log.clone(), record.expiry_reason.clone(), record.status))
        }
        Metadata::LegacyMetadata(_) => Err(Error::general_error("legacy metadata".to_string())),
    }
}

/// The last two log entries are the expiry warning and the after-the-fact info.
fn assert_expiry_logged(log: &[LogEntry], route: &str, detail: &str) {
    assert!(log.len() >= 3, "{log:?}");
    assert_eq!(log[0].message, "supplied entry", "supplied entries are kept: {log:?}");
    let warning = &log[log.len() - 2];
    let info = &log[log.len() - 1];
    assert_eq!(warning.kind, LogEntryKind::Warning, "{log:?}");
    assert_eq!(warning.message, "Asset expired");
    assert_eq!(info.kind, LogEntryKind::Info, "{log:?}");
    assert!(info.message.contains("after the fact"), "{}", info.message);
    assert!(info.message.contains(route), "{}", info.message);
    assert!(info.message.contains(detail), "{}", info.message);
}

async fn scenario_state<E: Environment<Value = Value>>(envref: EnvRef<E>) -> Result<(), Error> {
    let key = parse_key("data/x.txt")?;
    let state = State::from_value_and_metadata(
        Value::from("x"),
        Arc::new(Metadata::MetadataRecord(text_record(Status::Expired, None))),
    );
    envref.get_asset_manager().set_state(&key, state).await?;
    let (log, reason, status) = stored(&envref, &key).await?;
    assert_eq!(status, Status::Expired);
    assert_eq!(reason, None, "the structured reason stays as supplied");
    assert_expiry_logged(&log, "set_state", "unknown");
    Ok(())
}

async fn scenario_binary<E: Environment<Value = Value>>(envref: EnvRef<E>) -> Result<(), Error> {
    let key = parse_key("data/y.txt")?;
    envref
        .get_asset_manager()
        .set_binary(&key, b"y", text_record(Status::Expired, None))
        .await?;
    let (log, reason, status) = stored(&envref, &key).await?;
    assert_eq!(status, Status::Expired);
    assert_eq!(reason, None);
    assert_expiry_logged(&log, "set_binary", "unknown");
    Ok(())
}

async fn scenario_reason_kept<E: Environment<Value = Value>>(
    envref: EnvRef<E>,
) -> Result<(), Error> {
    let key = parse_key("data/z.txt")?;
    let root = DependencyKey::from(&parse_key("data/root.txt")?);
    let reason = ExpiryReason::Cascaded {
        cause: ExpiryCause::Explicit,
        root: root.clone(),
        via: root,
    };
    envref
        .get_asset_manager()
        .set_binary(&key, b"z", text_record(Status::Expired, Some(reason.clone())))
        .await?;
    let (log, stored_reason, _) = stored(&envref, &key).await?;
    assert_eq!(stored_reason, Some(reason));
    assert_expiry_logged(&log, "set_binary", "supplied reason");
    assert!(log[log.len() - 1].message.contains("data/root.txt"), "{log:?}");
    Ok(())
}

async fn scenario_ready_unaffected<E: Environment<Value = Value>>(
    envref: EnvRef<E>,
) -> Result<(), Error> {
    let key = parse_key("data/r.txt")?;
    envref
        .get_asset_manager()
        .set_binary(&key, b"r", text_record(Status::Ready, None))
        .await?;
    let (log, _, status) = stored(&envref, &key).await?;
    assert_eq!(status, Status::Source);
    assert!(
        log.iter()
            .all(|entry| entry.message != "Asset expired" && !entry.message.contains("after the fact")),
        "{log:?}"
    );
    Ok(())
}

#[tokio::test]
async fn supplied_expired_state_logs_asset_expired() -> Result<(), Error> {
    scenario_state(simple()).await?;
    scenario_state(immediate()).await
}

#[tokio::test]
async fn supplied_expired_binary_logs_asset_expired() -> Result<(), Error> {
    scenario_binary(simple()).await?;
    scenario_binary(immediate()).await
}

#[tokio::test]
async fn supplied_expiry_reason_is_kept_and_logged() -> Result<(), Error> {
    scenario_reason_kept(simple()).await?;
    scenario_reason_kept(immediate()).await
}

#[tokio::test]
async fn supplied_ready_adds_no_expiry_log() -> Result<(), Error> {
    scenario_ready_unaffected(simple()).await?;
    scenario_ready_unaffected(immediate()).await
}
