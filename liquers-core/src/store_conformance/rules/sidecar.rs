//! §8 — metadata sidecars.
//!
//! A store that keeps metadata beside its data uses the suffix `.__metadata__`: the metadata for
//! `foo` lives at `foo.__metadata__`. That makes one class of key unrepresentable — the *data* path
//! of the key `foo.__metadata__` is byte-identical to the *metadata* path of the key `foo` — and
//! such keys are **refused** rather than silently colliding. A store must not accept a key it
//! cannot address unambiguously.

use crate::metadata::{Metadata, MetadataRecord};
use crate::store_conformance::rules::support::require_absent;
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
