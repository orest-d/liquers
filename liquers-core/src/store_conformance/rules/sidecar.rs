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

/// `sidecar04` — a key holding only metadata is listed by its parent.
///
/// A sidecar implies its data key (§8), so `set_metadata` on a key with no data must leave that
/// key enumerable: `contains` answers it and its parent's `listdir` names it. The file stores used
/// to drop the sidecar from listings, which made such a key invisible while `contains` and
/// `get_metadata` still answered it — and made the answer depend on which backend a key landed in.
///
/// A store may instead **refuse** metadata for a key with no data, with `KeyNotFound`; that is a
/// consistent answer and passes. What fails is accepting the write and then hiding the key.
///
/// Needs `Directories` as well as `StoredMetadata`: "listed by its parent" means nothing for a store
/// that has no listing, such as the bare trait defaults, whose `contains` consults only `is_dir`.
pub async fn sidecar04(f: &dyn Fixture) -> RuleOutcome {
    let request = KeyRequest::FreshNested { depth: 1 };
    let keys = match keys_for(f, request.clone()).await {
        Ok(k) => k,
        Err(outcome) => return outcome,
    };
    let Some(key) = keys.first().cloned() else {
        return failed("the fixture returned no key for FreshNested");
    };
    let parent = key.parent();
    if let Err(outcome) = require_absent(f, &key, request).await {
        return outcome;
    }

    let mut record = MetadataRecord::new();
    record.with_key(key.clone()).with_title("conformance sidecar04".to_owned());
    match f.store().set_metadata(&key, &Metadata::MetadataRecord(record)).await {
        Ok(()) => f.record_created(&key),
        Err(e) if e.error_type == crate::error::ErrorType::KeyNotFound => {
            return RuleOutcome::Passed
        }
        Err(e) => return e.into(),
    }

    match f.store().contains(&key).await {
        Ok(true) => {}
        Ok(false) => {
            return failed_at(
                format!(
                    "set_metadata({}) succeeded on a key with no data, but contains is false",
                    key.encode()
                ),
                vec![key],
            )
        }
        Err(e) => return e.into(),
    }
    match f.store().listdir_keys(&parent).await {
        Ok(listed) if listed.contains(&key) => RuleOutcome::Passed,
        Ok(listed) => failed_at(
            format!(
                "set_metadata({}) succeeded on a key with no data, but listdir({}) omits it; got {:?}",
                key.encode(),
                parent.encode(),
                listed.iter().map(|k| k.encode()).collect::<Vec<_>>()
            ),
            vec![key, parent],
        ),
        Err(e) => e.into(),
    }
}

/// `sidecar05` — a key holding only metadata has no data object.
///
/// `set_metadata` on a key with no data must not invent one: `get` and `get_bytes` answer
/// `KeyNotFound`, while a key written with `set(k, b"", m)` holds an *empty* data object that
/// reads back as `[]`. The memory store used to answer a metadata-only key with empty bytes, which
/// made "no data object" indistinguishable from "the value is the empty byte string" and served a
/// metadata-only `Text` entry as `""`.
///
/// A store that refuses metadata for a key with no data (`KeyNotFound`) still runs the empty-object
/// half, since that half does not depend on the refusal.
pub async fn sidecar05(f: &dyn Fixture) -> RuleOutcome {
    let request = KeyRequest::FreshSiblings { count: 2 };
    let keys = match keys_for(f, request.clone()).await {
        Ok(k) => k,
        Err(outcome) => return outcome,
    };
    let (Some(metadata_only), Some(empty)) = (keys.first().cloned(), keys.get(1).cloned()) else {
        return failed("the fixture returned fewer than two keys for FreshSiblings { count: 2 }");
    };
    for key in [&metadata_only, &empty] {
        if let Err(outcome) = require_absent(f, key, request.clone()).await {
            return outcome;
        }
    }

    let mut record = MetadataRecord::new();
    record.with_key(metadata_only.clone()).with_title("conformance sidecar05".to_owned());
    let accepted = match f
        .store()
        .set_metadata(&metadata_only, &Metadata::MetadataRecord(record))
        .await
    {
        Ok(()) => {
            f.record_created(&metadata_only);
            true
        }
        Err(e) if e.error_type == crate::error::ErrorType::KeyNotFound => false,
        Err(e) => return e.into(),
    };
    if accepted {
        match f.store().get_bytes(&metadata_only).await {
            Err(e) if e.error_type == crate::error::ErrorType::KeyNotFound => {}
            Err(e) => return e.into(),
            Ok(bytes) => {
                return failed_at(
                    format!(
                        "get_bytes({}) on a metadata-only key returned {} byte(s) instead of \
                         KeyNotFound",
                        metadata_only.encode(),
                        bytes.len()
                    ),
                    vec![metadata_only],
                )
            }
        }
        match f.store().get(&metadata_only).await {
            Err(e) if e.error_type == crate::error::ErrorType::KeyNotFound => {}
            Err(e) => return e.into(),
            Ok((bytes, _)) => {
                return failed_at(
                    format!(
                        "get({}) on a metadata-only key returned {} byte(s) instead of KeyNotFound",
                        metadata_only.encode(),
                        bytes.len()
                    ),
                    vec![metadata_only],
                )
            }
        }
    }

    if let Err(e) = f.store().set(&empty, b"", &blank_metadata()).await {
        return e.into();
    }
    f.record_created(&empty);
    match f.store().get_bytes(&empty).await {
        Ok(bytes) if bytes.is_empty() => RuleOutcome::Passed,
        Ok(bytes) => failed_at(
            format!(
                "set({}, b\"\") stored an empty data object but get_bytes returned {} byte(s)",
                empty.encode(),
                bytes.len()
            ),
            vec![empty],
        ),
        Err(e) => e.into(),
    }
}
