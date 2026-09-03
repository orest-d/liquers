//! §1 — the sibling rule.
//!
//! > **No operation on a key may read, list, or delete anything under a different key.**
//!
//! A key whose name is a *prefix* of another key's name is a different key: `data` and `database`
//! are unrelated, as are `sub` and `subway`. This is the rule a store breaks when it addresses its
//! backend by string prefix rather than by path, and breaking it is how `removedir("data")`
//! destroyed `database/` through `DELETE /api/store/removedir/{*key}`.
//!
//! Every rule here works from [`KeyRequest::FreshPrefixPair`], because the pair *is* the subject.
//! A store whose key space cannot produce one — numeric row IDs, say — declines, and the report
//! says so rather than reporting a pass.

use crate::metadata::{Metadata, MetadataRecord};
use crate::query::Key;
use crate::store_conformance::{
    failed, failed_at, keys_for, Fixture, KeyRequest, RuleOutcome,
};

/// Bytes distinctive enough that a truncation is not mistaken for a correct read.
const KEEP: &[u8] = b"this key belongs to the sibling that must survive";

fn metadata() -> Metadata {
    Metadata::MetadataRecord(MetadataRecord::new())
}

/// Create `key` after checking it is absent, and record it.
///
/// The check is what upholds [`SafetyLevel::Scratch`](crate::store_conformance::SafetyLevel):
/// rules never touch a key that was already there. The record is what makes cleanup and the
/// residue report possible — an unrecorded key leaks silently.
async fn create(f: &dyn Fixture, key: &Key, data: &[u8]) -> Result<(), RuleOutcome> {
    match f.store().contains(key).await {
        Ok(true) => {
            return Err(RuleOutcome::SkippedPrecondition {
                request: KeyRequest::FreshPrefixPair,
                reason: format!("{} already exists; the fixture must supply fresh keys", key.encode()),
            })
        }
        Ok(false) => {}
        Err(e) => return Err(e.into()),
    }
    match f.store().set(key, data, &metadata()).await {
        Ok(()) => {
            f.record_created(key);
            Ok(())
        }
        Err(e) => Err(e.into()),
    }
}

/// The two names, shorter first, or a decline.
async fn pair(f: &dyn Fixture) -> Result<(Key, Key), RuleOutcome> {
    let keys = keys_for(f, KeyRequest::FreshPrefixPair).await?;
    if keys.len() < 2 {
        return Err(failed(
            "the fixture returned fewer than two keys for FreshPrefixPair",
        ));
    }
    let (a, b) = (keys[0].clone(), keys[1].clone());
    // Order by encoded length so `a` is the prefix and `b` the longer name, whichever way the
    // fixture returned them.
    if a.encode().len() <= b.encode().len() {
        Ok((a, b))
    } else {
        Ok((b, a))
    }
}

/// `sibling01` — `removedir` on a directory does not touch a sibling whose name extends it.
///
/// The P0. `removedir("sub")` on a store that deletes by string prefix takes `subway/` with it,
/// and the caller is told it succeeded.
///
/// The assertion reads the surviving key's **bytes** rather than merely asking whether it exists:
/// a backend that truncated instead of unlinking would pass an existence check.
pub async fn sibling01(f: &dyn Fixture) -> RuleOutcome {
    let (short, long) = match pair(f).await {
        Ok(p) => p,
        Err(outcome) => return outcome,
    };
    let doomed = short.join("doomed.txt");
    let survivor = long.join("survivor.txt");

    if let Err(outcome) = create(f, &doomed, b"this one is removed").await {
        return outcome;
    }
    if let Err(outcome) = create(f, &survivor, KEEP).await {
        return outcome;
    }

    if let Err(e) = f.store().removedir(&short).await {
        return e.into();
    }

    match f.store().get_bytes(&survivor).await {
        Ok(bytes) if bytes == KEEP => RuleOutcome::Passed,
        Ok(_) => failed_at(
            format!(
                "removedir({}) left {} present but altered",
                short.encode(),
                survivor.encode()
            ),
            vec![short, survivor],
        ),
        Err(e) => failed_at(
            format!(
                "removedir({}) destroyed the sibling {} ({:?})",
                short.encode(),
                survivor.encode(),
                e.error_type
            ),
            vec![short, survivor],
        ),
    }
}

/// `sibling02` — `listdir` on a directory reports nothing belonging to a name-extending sibling.
pub async fn sibling02(f: &dyn Fixture) -> RuleOutcome {
    let (short, long) = match pair(f).await {
        Ok(p) => p,
        Err(outcome) => return outcome,
    };
    let inside = short.join("inside.txt");
    let outside = long.join("outside.txt");

    if let Err(outcome) = create(f, &inside, b"inside").await {
        return outcome;
    }
    if let Err(outcome) = create(f, &outside, b"outside").await {
        return outcome;
    }

    match f.store().listdir(&short).await {
        Ok(entries) => {
            if !entries.iter().any(|e| e == "inside.txt") {
                return failed_at(
                    format!(
                        "listdir({}) omitted its own entry inside.txt; got {entries:?}",
                        short.encode()
                    ),
                    vec![short, inside],
                );
            }
            if entries.iter().any(|e| e == "outside.txt") {
                return failed_at(
                    format!(
                        "listdir({}) reported outside.txt, which belongs to {}",
                        short.encode(),
                        long.encode()
                    ),
                    vec![short, long, outside],
                );
            }
            RuleOutcome::Passed
        }
        Err(e) => e.into(),
    }
}

/// `sibling03` — `remove` on a data key does not touch a key whose name extends it.
///
/// The same rule as `sibling01` one level down: `remove("data")` must leave `database/x` alone.
pub async fn sibling03(f: &dyn Fixture) -> RuleOutcome {
    let (short, long) = match pair(f).await {
        Ok(p) => p,
        Err(outcome) => return outcome,
    };
    let survivor = long.join("survivor.txt");

    if let Err(outcome) = create(f, &short, b"removed directly").await {
        return outcome;
    }
    if let Err(outcome) = create(f, &survivor, KEEP).await {
        return outcome;
    }

    if let Err(e) = f.store().remove(&short).await {
        return e.into();
    }

    match f.store().get_bytes(&survivor).await {
        Ok(bytes) if bytes == KEEP => RuleOutcome::Passed,
        Ok(_) => failed_at(
            format!("remove({}) altered {}", short.encode(), survivor.encode()),
            vec![short, survivor],
        ),
        Err(e) => failed_at(
            format!(
                "remove({}) destroyed {} ({:?})",
                short.encode(),
                survivor.encode(),
                e.error_type
            ),
            vec![short, survivor],
        ),
    }
}

/// `sibling04` — a sibling's children do not make a name-extending key look like a directory.
///
/// The sharpest of the family, and the cheapest to get wrong: with only `subway/x` stored, a store
/// that answers `is_dir` by string prefix says `sub` is a directory. Nothing is removed here, so
/// this runs one level lower than `sibling01`.
pub async fn sibling04(f: &dyn Fixture) -> RuleOutcome {
    let (short, long) = match pair(f).await {
        Ok(p) => p,
        Err(outcome) => return outcome,
    };
    let only_child = long.join("only.txt");

    if let Err(outcome) = create(f, &only_child, b"the only stored key").await {
        return outcome;
    }

    match f.store().is_dir(&short).await {
        Ok(false) => {}
        Ok(true) => {
            return failed_at(
                format!(
                    "is_dir({}) is true, but only {} is stored — the store is matching a string prefix",
                    short.encode(),
                    only_child.encode()
                ),
                vec![short, only_child],
            )
        }
        Err(e) => return e.into(),
    }

    match f.store().contains(&short).await {
        Ok(false) => RuleOutcome::Passed,
        Ok(true) => failed_at(
            format!(
                "contains({}) is true, but only {} is stored",
                short.encode(),
                only_child.encode()
            ),
            vec![short, only_child],
        ),
        Err(e) => e.into(),
    }
}

/// `sibling05` — a key the store refuses as data is not addressable as a *directory* either.
///
/// `STORE_SEMANTICS.md` §1: "The directory form is subject to the same key refusals as the data and
/// metadata forms." A store with one place that produces its directory form satisfies this for
/// free; one that spreads the rule across call sites is where a refused key slips back in as a
/// directory. Reads only.
pub async fn sibling05(f: &dyn Fixture) -> RuleOutcome {
    let keys = match keys_for(f, KeyRequest::UnsupportedShape).await {
        Ok(k) => k,
        Err(outcome) => return outcome,
    };
    let Some(key) = keys.first() else {
        return failed("the fixture returned no key for UnsupportedShape");
    };

    if f.store().is_supported(key) {
        return failed_at(
            format!(
                "the fixture offered {} as unsupported, but is_supported says otherwise",
                key.encode()
            ),
            vec![key.clone()],
        );
    }

    match f.store().is_dir(key).await {
        Ok(false) => RuleOutcome::Passed,
        Ok(true) => failed_at(
            format!(
                "{} is refused as data but is_dir reports it as a directory",
                key.encode()
            ),
            vec![key.clone()],
        ),
        // An error is a legitimate answer here: refusing a key the store cannot address is not the
        // same as claiming it is a directory. Only `Ok(true)` breaks the contract.
        Err(_) => RuleOutcome::Passed,
    }
}
