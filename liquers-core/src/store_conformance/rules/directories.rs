//! §2 — directories on a backend that has none, and the data round trip.
//!
//! Most backends are flat: a key set with no directories in it, so `is_dir`, `contains` and
//! `listdir` have to be *derived* — every proper prefix of a stored key is a directory. A store
//! uses whichever of the three sources of truth its backend offers (`stat`, a bounded listing, or
//! [`DirectoryIndex`](crate::store_dir_index::DirectoryIndex)), and these rules check what all
//! three must agree on regardless of which was used.

use crate::metadata::{Metadata, MetadataRecord};
use crate::query::Key;
use crate::store_conformance::{failed, failed_at, keys_for, Fixture, KeyRequest, RuleOutcome};

fn metadata() -> Metadata {
    Metadata::MetadataRecord(MetadataRecord::new())
}

/// Create `key` after checking it is absent, and record it. See `sibling::create`.
async fn create(f: &dyn Fixture, key: &Key, request: KeyRequest) -> Result<(), RuleOutcome> {
    match f.store().contains(key).await {
        Ok(true) => {
            return Err(RuleOutcome::SkippedPrecondition {
                request,
                reason: format!("{} already exists", key.encode()),
            })
        }
        Ok(false) => {}
        Err(e) => return Err(e.into()),
    }
    match f.store().set(key, b"conformance", &metadata()).await {
        Ok(()) => {
            f.record_created(key);
            Ok(())
        }
        Err(e) => Err(e.into()),
    }
}

/// One nested key and the directory above it.
async fn nested(f: &dyn Fixture) -> Result<(Key, Key), RuleOutcome> {
    let keys = keys_for(f, KeyRequest::FreshNested { depth: 1 }).await?;
    let Some(leaf) = keys.first().cloned() else {
        return Err(failed("the fixture returned no key for FreshNested"));
    };
    let parent = leaf.parent();
    if parent.is_empty() {
        return Err(RuleOutcome::SkippedPrecondition {
            request: KeyRequest::FreshNested { depth: 1 },
            reason: "the fixture returned a key with no parent directory".to_owned(),
        });
    }
    Ok((leaf, parent))
}

/// `dir01` — a directory holding children is addressable.
///
/// Issue row 2: on a backend with no directory objects, `listdir` could see a directory that
/// `is_dir` and `contains` then denied. A directory a listing can reach must be addressable.
pub async fn dir01(f: &dyn Fixture) -> RuleOutcome {
    let (leaf, parent) = match nested(f).await {
        Ok(p) => p,
        Err(outcome) => return outcome,
    };
    if let Err(outcome) = create(f, &leaf, KeyRequest::FreshNested { depth: 1 }).await {
        return outcome;
    }

    match f.store().is_dir(&parent).await {
        Ok(true) => {}
        Ok(false) => {
            return failed_at(
                format!(
                    "is_dir({}) is false though it holds {}",
                    parent.encode(),
                    leaf.encode()
                ),
                vec![parent, leaf],
            )
        }
        Err(e) => return e.into(),
    }

    match f.store().contains(&parent).await {
        Ok(true) => RuleOutcome::Passed,
        Ok(false) => failed_at(
            format!(
                "contains({}) is false though is_dir says it is a directory",
                parent.encode()
            ),
            vec![parent, leaf],
        ),
        Err(e) => e.into(),
    }
}

/// `dir02` — `is_dir` on an absent key is `Ok(false)`, never an error.
///
/// Issue row 1. The distinction is load-bearing: a store reporting an S3 403 as `Ok(false)` tells
/// a caller a directory does not exist when the truth is that permission was refused. This rule
/// checks only the *absent* half — a backend failure must still be an error, which no rule can
/// provoke portably.
pub async fn dir02(f: &dyn Fixture) -> RuleOutcome {
    let keys = match keys_for(f, KeyRequest::Fresh).await {
        Ok(k) => k,
        Err(outcome) => return outcome,
    };
    let Some(key) = keys.first() else {
        return failed("the fixture returned no key for Fresh");
    };

    // Nothing is created: the key must be absent for the question to mean anything.
    match f.store().contains(key).await {
        Ok(false) => {}
        Ok(true) => {
            return RuleOutcome::SkippedPrecondition {
                request: KeyRequest::Fresh,
                reason: format!("{} exists, so it cannot test absence", key.encode()),
            }
        }
        Err(e) => return e.into(),
    }

    match f.store().is_dir(key).await {
        Ok(false) => RuleOutcome::Passed,
        Ok(true) => failed_at(
            format!("is_dir({}) is true for a key that does not exist", key.encode()),
            vec![key.clone()],
        ),
        Err(e) => failed_at(
            format!(
                "is_dir({}) returned {:?} for an absent key; absence is Ok(false), not an error",
                key.encode(),
                e.error_type
            ),
            vec![key.clone()],
        ),
    }
}

/// `dir03` — every entry `listdir` calls a directory answers `is_dir == true`.
///
/// The forward half of "`listdir` and `is_dir` must agree".
pub async fn dir03(f: &dyn Fixture) -> RuleOutcome {
    let (leaf, parent) = match nested(f).await {
        Ok(p) => p,
        Err(outcome) => return outcome,
    };
    if let Err(outcome) = create(f, &leaf, KeyRequest::FreshNested { depth: 1 }).await {
        return outcome;
    }

    let grandparent = parent.parent();
    let entries = match f.store().listdir_keys(&grandparent).await {
        Ok(e) => e,
        Err(e) => return e.into(),
    };

    for entry in entries {
        let listed_as_dir = match f.store().is_dir(&entry).await {
            Ok(v) => v,
            Err(e) => return e.into(),
        };
        // `parent` is known to be a directory: it holds `leaf`.
        if entry == parent && !listed_as_dir {
            return failed_at(
                format!(
                    "{} is listed under {} and holds {}, but is_dir says it is not a directory",
                    parent.encode(),
                    grandparent.encode(),
                    leaf.encode()
                ),
                vec![parent, leaf],
            );
        }
    }
    RuleOutcome::Passed
}

/// `dir04` — a directory's metadata is directory-shaped and carries its key.
///
/// `default_metadata` must honour **both** its arguments. A record with `is_dir == false` and no
/// key is a file-shaped answer for a directory, which is exactly what a caller reading the record
/// directly receives — and `get_asset_info` is built on `get_metadata`, so a store that cannot
/// produce directory metadata cannot answer `-R-dir/` queries.
pub async fn dir04(f: &dyn Fixture) -> RuleOutcome {
    let (leaf, parent) = match nested(f).await {
        Ok(p) => p,
        Err(outcome) => return outcome,
    };
    if let Err(outcome) = create(f, &leaf, KeyRequest::FreshNested { depth: 1 }).await {
        return outcome;
    }

    let metadata = match f.store().get_metadata(&parent).await {
        Ok(m) => m,
        Err(e) => return e.into(),
    };

    if !metadata.is_dir() {
        return failed_at(
            format!(
                "metadata for the directory {} has is_dir == false",
                parent.encode()
            ),
            vec![parent],
        );
    }
    match metadata.key() {
        Ok(Some(key)) if key == parent => RuleOutcome::Passed,
        Ok(Some(key)) => failed_at(
            format!(
                "metadata for {} carries the key {}",
                parent.encode(),
                key.encode()
            ),
            vec![parent],
        ),
        Ok(None) => failed_at(
            format!("metadata for {} carries no key", parent.encode()),
            vec![parent],
        ),
        Err(e) => e.into(),
    }
}

/// `dir05` — `contains` falls back to `is_dir`.
///
/// Issue row 3. Provided by the `AsyncStore` default; a store overriding `is_dir` and not
/// `contains` gets the two disagreeing, silently. This is the contract `traitdef01` used to check
/// for the trait defaults alone.
pub async fn dir05(f: &dyn Fixture) -> RuleOutcome {
    let (leaf, parent) = match nested(f).await {
        Ok(p) => p,
        Err(outcome) => return outcome,
    };
    if let Err(outcome) = create(f, &leaf, KeyRequest::FreshNested { depth: 1 }).await {
        return outcome;
    }

    let is_dir = match f.store().is_dir(&parent).await {
        Ok(v) => v,
        Err(e) => return e.into(),
    };
    let contains = match f.store().contains(&parent).await {
        Ok(v) => v,
        Err(e) => return e.into(),
    };

    if is_dir && !contains {
        failed_at(
            format!(
                "is_dir({0}) is true but contains({0}) is false",
                parent.encode()
            ),
            vec![parent],
        )
    } else {
        RuleOutcome::Passed
    }
}

/// `dir06` — the agreement holds in reverse: a key that answers `is_dir` appears in its parent's
/// listing.
///
/// Without this, `dir03` alone is satisfied by a store that lists nothing at all.
pub async fn dir06(f: &dyn Fixture) -> RuleOutcome {
    let (leaf, parent) = match nested(f).await {
        Ok(p) => p,
        Err(outcome) => return outcome,
    };
    if let Err(outcome) = create(f, &leaf, KeyRequest::FreshNested { depth: 1 }).await {
        return outcome;
    }

    match f.store().is_dir(&parent).await {
        Ok(true) => {}
        Ok(false) => return RuleOutcome::Passed, // dir01 owns this disagreement
        Err(e) => return e.into(),
    }

    let grandparent = parent.parent();
    match f.store().listdir_keys(&grandparent).await {
        Ok(entries) if entries.contains(&parent) => RuleOutcome::Passed,
        Ok(entries) => failed_at(
            format!(
                "is_dir({}) is true but it is absent from listdir({}); got {:?}",
                parent.encode(),
                grandparent.encode(),
                entries.iter().map(|k| k.encode()).collect::<Vec<_>>()
            ),
            vec![parent, grandparent],
        ),
        Err(e) => e.into(),
    }
}

/// `dir07` — directory metadata does not populate `children`.
///
/// **Blocked, and deliberately so.** `STORE_SEMANTICS.md` §2 says directory metadata must not carry
/// `children`, and *every* implementation populates it — including the `AsyncStore` trait default
/// the sentence itself points at. A rule no implementation has ever followed is a rule that was
/// never agreed, so this reports `Blocked` rather than failing eight stores or passing vacuously.
///
/// The check below still runs, so the moment the question is settled this becomes a live rule by
/// deleting one branch. See `STORE-SEMANTICS-CHILDREN-RULE-CONTRADICTS-EVERY-STORE`.
pub async fn dir07(f: &dyn Fixture) -> RuleOutcome {
    let (leaf, parent) = match nested(f).await {
        Ok(p) => p,
        Err(outcome) => return outcome,
    };
    if let Err(outcome) = create(f, &leaf, KeyRequest::FreshNested { depth: 1 }).await {
        return outcome;
    }

    // `children` lives on the record, so a store returning legacy metadata cannot populate it and
    // trivially conforms — which is why this reads the record rather than `get_asset_info`.
    match f.store().get_metadata(&parent).await {
        Ok(Metadata::MetadataRecord(record)) if record.children.is_empty() => RuleOutcome::Passed,
        Ok(Metadata::MetadataRecord(record)) => RuleOutcome::Blocked {
            issue: "STORE-SEMANTICS-CHILDREN-RULE-CONTRADICTS-EVERY-STORE".to_owned(),
            detail: format!(
                "metadata for {} carries {} children. The contract forbids this and every \
                 implementation does it, so the contract is what has to be settled first",
                parent.encode(),
                record.children.len()
            ),
        },
        Ok(_) => RuleOutcome::Passed,
        Err(e) => e.into(),
    }
}

/// `data03` — a key that already holds data can be read.
///
/// The positive read path, and until it existed nothing exercised it. `Existing` is documented as
/// the only source of subjects for a read-only store — `FetchStore` seeds one — yet **no rule
/// requested it**, so a read-only store whose read path was completely broken reported conformant
/// on six passing rules that never read anything.
pub async fn data03(f: &dyn Fixture) -> RuleOutcome {
    let keys = match keys_for(f, KeyRequest::Existing).await {
        Ok(k) => k,
        Err(outcome) => return outcome,
    };
    let Some(key) = keys.first().cloned() else {
        return failed("the fixture returned no key for Existing");
    };

    match f.store().contains(&key).await {
        Ok(true) => {}
        Ok(false) => {
            return failed_at(
                format!(
                    "the fixture offered {} as an existing key, but contains says it is absent",
                    key.encode()
                ),
                vec![key],
            )
        }
        Err(e) => return e.into(),
    }

    match f.store().get_bytes(&key).await {
        Ok(_) => RuleOutcome::Passed,
        Err(e) => failed_at(
            format!(
                "get_bytes({}) failed with {:?} for a key the fixture says exists",
                key.encode(),
                e.error_type
            ),
            vec![key],
        ),
    }
}

/// `dir08` — a directory that already exists answers `is_dir`.
///
/// The read-only counterpart of `dir01`, for a store that cannot create anything.
pub async fn dir08(f: &dyn Fixture) -> RuleOutcome {
    let keys = match keys_for(f, KeyRequest::ExistingDirectory).await {
        Ok(k) => k,
        Err(outcome) => return outcome,
    };
    let Some(key) = keys.first().cloned() else {
        return failed("the fixture returned no key for ExistingDirectory");
    };

    match f.store().is_dir(&key).await {
        Ok(true) => RuleOutcome::Passed,
        Ok(false) => failed_at(
            format!(
                "the fixture offered {} as an existing directory, but is_dir is false",
                key.encode()
            ),
            vec![key],
        ),
        Err(e) => e.into(),
    }
}

/// `data01` — `set` then `get` returns the same bytes.
///
/// The floor. A store failing this fails everything else for uninteresting reasons, so it is worth
/// having as its own line in the report rather than as an assumption inside other rules.
pub async fn data01(f: &dyn Fixture) -> RuleOutcome {
    let keys = match keys_for(f, KeyRequest::Fresh).await {
        Ok(k) => k,
        Err(outcome) => return outcome,
    };
    let Some(key) = keys.first().cloned() else {
        return failed("the fixture returned no key for Fresh");
    };

    // Bytes that are not valid UTF-8, so a store that round-trips through a string is caught.
    let payload: Vec<u8> = vec![0x00, 0xff, 0xfe, b'l', b'q', 0x80, 0x7f];

    match f.store().contains(&key).await {
        Ok(false) => {}
        Ok(true) => {
            return RuleOutcome::SkippedPrecondition {
                request: KeyRequest::Fresh,
                reason: format!("{} already exists", key.encode()),
            }
        }
        Err(e) => return e.into(),
    }
    if let Err(e) = f.store().set(&key, &payload, &metadata()).await {
        return e.into();
    }
    f.record_created(&key);

    match f.store().get_bytes(&key).await {
        Ok(bytes) if bytes == payload => RuleOutcome::Passed,
        Ok(bytes) => failed_at(
            format!(
                "set/get did not round-trip: wrote {} bytes, read {} back",
                payload.len(),
                bytes.len()
            ),
            vec![key],
        ),
        Err(e) => failed_at(
            format!("set succeeded but get returned {:?}", e.error_type),
            vec![key],
        ),
    }
}
