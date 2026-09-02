//! §8 — metadata sidecars.
//!
//! A store that keeps metadata beside its data uses the suffix `.__metadata__`: the metadata for
//! `foo` lives at `foo.__metadata__`. That makes one class of key unrepresentable — the *data* path
//! of the key `foo.__metadata__` is byte-identical to the *metadata* path of the key `foo` — and
//! such keys are **refused** rather than silently colliding. A store must not accept a key it
//! cannot address unambiguously.

use crate::metadata::{Metadata, MetadataRecord};
use crate::store_conformance::rules::support::{metadata as blank_metadata, require_absent};
use crate::store_conformance::{failed, failed_at, keys_for, Fixture, KeyRequest, RuleOutcome};

/// `sidecar01` — a key that would collide with another key's metadata path is refused.
///
/// Only meaningful for a store that uses sidecars; one that keeps metadata another way declines the
/// precondition, and the report says so rather than counting a pass.
pub async fn sidecar01(f: &dyn Fixture) -> RuleOutcome {
    let keys = match keys_for(f, KeyRequest::MetadataCollision).await {
        Ok(k) => k,
        Err(outcome) => return outcome,
    };
    let Some(key) = keys.first().cloned() else {
        return failed("the fixture returned no key for MetadataCollision");
    };

    if f.store().is_supported(&key) {
        return failed_at(
            format!(
                "is_supported({}) is true, but its data path collides with another key's metadata \
                 path — a store must not accept a key it cannot address unambiguously",
                key.encode()
            ),
            vec![key],
        );
    }
    RuleOutcome::Passed
}

/// `sidecar03` — the fallible operations actually reject a sidecar-colliding key.
///
/// `sidecar01` only checks `is_supported`, which is a **routing hint**: `AsyncStoreRouter` consults
/// it, but a caller can invoke `get`, `set` or `set_metadata` directly without asking. A
/// sidecar-backed store that reports `false` there and still accepts the key in `set` would pass
/// `sidecar01` while overwriting another key's metadata — the collision the rule claims to prevent.
///
/// `Scratch`, because proving it means calling `set`. If the store wrongly accepts, damage has been
/// done to a key this run did not create — which is why the failure says so, and why this rule can
/// only run against a store the operator has declared expendable.
pub async fn sidecar03(f: &dyn Fixture) -> RuleOutcome {
    let keys = match keys_for(f, KeyRequest::MetadataCollision).await {
        Ok(k) => k,
        Err(outcome) => return outcome,
    };
    let Some(key) = keys.first().cloned() else {
        return failed("the fixture returned no key for MetadataCollision");
    };

    // Reads first: harmless, and they establish whether the refusal is uniform.
    if f.store().get_bytes(&key).await.is_ok() {
        return failed_at(
            format!(
                "{} is refused by is_supported but get_bytes read it — the refusal is not uniform",
                key.encode()
            ),
            vec![key],
        );
    }

    match f.store().set(&key, b"must be refused", &blank_metadata()).await {
        Err(_) => {}
        Ok(()) => {
            f.record_created(&key);
            return failed_at(
                format!(
                    "set({0}) succeeded though is_supported refuses it. Its data path is another                      key's metadata path, so this write has corrupted that key's metadata — a                      store must refuse what it cannot address unambiguously, not merely decline to                      route it",
                    key.encode()
                ),
                vec![key],
            );
        }
    }

    match f.store().set_metadata(&key, &blank_metadata()).await {
        Err(_) => RuleOutcome::Passed,
        Ok(()) => failed_at(
            format!(
                "set_metadata({}) succeeded though is_supported refuses the key",
                key.encode()
            ),
            vec![key],
        ),
    }
}

/// `sidecar02` — metadata written with `set_metadata` reads back.
///
/// Uses a distinguishing field rather than comparing whole records: a store may legitimately add
/// its own derived fields (size, timestamps) on the way out, so equality would fail a correct
/// store.
pub async fn sidecar02(f: &dyn Fixture) -> RuleOutcome {
    let keys = match keys_for(f, KeyRequest::Fresh).await {
        Ok(k) => k,
        Err(outcome) => return outcome,
    };
    let Some(key) = keys.first().cloned() else {
        return failed("the fixture returned no key for Fresh");
    };
    if let Err(outcome) = require_absent(f, &key, KeyRequest::Fresh).await {
        return outcome;
    }

    const TITLE: &str = "conformance sidecar02";
    let mut record = MetadataRecord::new();
    record.with_key(key.clone()).with_title(TITLE.to_owned());
    let written = Metadata::MetadataRecord(record);

    if let Err(e) = f.store().set(&key, b"body", &written).await {
        return e.into();
    }
    f.record_created(&key);
    if let Err(e) = f.store().set_metadata(&key, &written).await {
        return e.into();
    }

    match f.store().get_metadata(&key).await {
        Ok(Metadata::MetadataRecord(record)) if record.title == TITLE => RuleOutcome::Passed,
        Ok(Metadata::MetadataRecord(record)) => failed_at(
            format!(
                "set_metadata wrote title {TITLE:?} for {} but get_metadata returned {:?}",
                key.encode(),
                record.title
            ),
            vec![key],
        ),
        Ok(_) => failed_at(
            format!(
                "get_metadata({}) returned legacy metadata, so what set_metadata wrote cannot be \
                 read back",
                key.encode()
            ),
            vec![key],
        ),
        Err(e) => e.into(),
    }
}
