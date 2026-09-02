//! §5 — removal.
//!
//! > **`removedir` is specified by its postcondition: if it returns `Ok(())`, the directory does
//! > not exist afterwards.** Failing to remove it is an error. What is forbidden is claiming
//! > success without the effect.
//!
//! Recursion follows from that rather than being stipulated beside it — a directory derived from
//! its children exists while any child remains, so a removal that left one and reported `Ok(())`
//! would break the postcondition. `remove02` checks it directly anyway, because a store with real
//! directory objects can leave a child behind without the parent retiring, and the postcondition
//! alone would not catch that.
//!
//! `removedir` is **not atomic** on any backend, and no rule here pretends otherwise.

use crate::error::ErrorType;
use crate::store_conformance::rules::support::{create, create_with, metadata};
use crate::store_conformance::{failed, failed_at, keys_for, Fixture, KeyRequest, RuleOutcome};

/// `remove01` — after `removedir` returns `Ok`, the directory does not exist.
///
/// The postcondition itself. A store whose `removedir` is a no-op returning `Ok(())` fails here and
/// nowhere else.
pub async fn remove01(f: &dyn Fixture) -> RuleOutcome {
    let keys = match keys_for(f, KeyRequest::FreshNested { depth: 1 }).await {
        Ok(k) => k,
        Err(outcome) => return outcome,
    };
    let Some(leaf) = keys.first().cloned() else {
        return failed("the fixture returned no key for FreshNested");
    };
    let dir = leaf.parent();
    if dir.is_empty() {
        return RuleOutcome::SkippedPrecondition {
            request: KeyRequest::FreshNested { depth: 1 },
            reason: "the fixture returned a key with no parent directory".to_owned(),
        };
    }

    if let Err(outcome) = create(f, &leaf, KeyRequest::FreshNested { depth: 1 }).await {
        return outcome;
    }

    if let Err(e) = f.store().removedir(&dir).await {
        // A refusal is not a broken postcondition; the store simply did not do it.
        return failed_at(
            format!(
                "removedir({}) returned {:?} though the store declares it removes directories",
                dir.encode(),
                e.error_type
            ),
            vec![dir],
        );
    }

    match f.store().is_dir(&dir).await {
        Ok(false) => RuleOutcome::Passed,
        Ok(true) => failed_at(
            format!(
                "removedir({0}) returned Ok but is_dir({0}) is still true",
                dir.encode()
            ),
            vec![dir],
        ),
        Err(e) => e.into(),
    }
}

/// `remove02` — `removedir` is recursive: no child survives it.
///
/// Checked directly rather than left to follow from `remove01`, because a store with real directory
/// objects can unlink the directory while leaving a child addressable, and the postcondition on the
/// directory alone would then pass.
pub async fn remove02(f: &dyn Fixture) -> RuleOutcome {
    let keys = match keys_for(f, KeyRequest::FreshNested { depth: 1 }).await {
        Ok(k) => k,
        Err(outcome) => return outcome,
    };
    let Some(leaf) = keys.first().cloned() else {
        return failed("the fixture returned no key for FreshNested");
    };
    let dir = leaf.parent();
    if dir.is_empty() {
        return RuleOutcome::SkippedPrecondition {
            request: KeyRequest::FreshNested { depth: 1 },
            reason: "the fixture returned a key with no parent directory".to_owned(),
        };
    }
    let sibling = dir.join("second.txt");

    if let Err(outcome) = create(f, &leaf, KeyRequest::FreshNested { depth: 1 }).await {
        return outcome;
    }
    if let Err(outcome) = create(f, &sibling, KeyRequest::FreshNested { depth: 1 }).await {
        return outcome;
    }

    if let Err(e) = f.store().removedir(&dir).await {
        return e.into();
    }

    for child in [&leaf, &sibling] {
        match f.store().contains(child).await {
            Ok(false) => {}
            Ok(true) => {
                return failed_at(
                    format!(
                        "removedir({}) left the child {} behind — it is not recursive",
                        dir.encode(),
                        child.encode()
                    ),
                    vec![dir, child.clone()],
                )
            }
            Err(e) => return e.into(),
        }
    }
    RuleOutcome::Passed
}

/// `remove03` — `remove` deletes data and metadata together.
///
/// A store keeping metadata in a sidecar can unlink the data and leave the sidecar, which then
/// reads back as a key with metadata and no content — the shape that makes a listing report a file
/// that cannot be read.
pub async fn remove03(f: &dyn Fixture) -> RuleOutcome {
    let keys = match keys_for(f, KeyRequest::Fresh).await {
        Ok(k) => k,
        Err(outcome) => return outcome,
    };
    let Some(key) = keys.first().cloned() else {
        return failed("the fixture returned no key for Fresh");
    };

    if let Err(outcome) = create(f, &key, KeyRequest::Fresh).await {
        return outcome;
    }
    if let Err(e) = f.store().remove(&key).await {
        return e.into();
    }

    match f.store().contains(&key).await {
        Ok(false) => {}
        Ok(true) => {
            return failed_at(
                format!("remove({0}) returned Ok but contains({0}) is still true", key.encode()),
                vec![key],
            )
        }
        Err(e) => return e.into(),
    }

    match f.store().get_metadata(&key).await {
        Err(e) if e.error_type == ErrorType::KeyNotFound => RuleOutcome::Passed,
        Err(e) => failed_at(
            format!(
                "after remove({}), get_metadata gave {:?} rather than KeyNotFound",
                key.encode(),
                e.error_type
            ),
            vec![key],
        ),
        Ok(_) => failed_at(
            format!(
                "remove({}) deleted the data but its metadata is still readable",
                key.encode()
            ),
            vec![key],
        ),
    }
}

/// `data02` — writing a key that already exists replaces its content.
///
/// The one rule that deliberately writes twice to the same key, which is why it needs
/// [`SafetyLevel::Scratch`](crate::store_conformance::SafetyLevel): the second write overwrites,
/// and the level exists to say that is only ever done to a key this run created.
pub async fn data02(f: &dyn Fixture) -> RuleOutcome {
    let keys = match keys_for(f, KeyRequest::Fresh).await {
        Ok(k) => k,
        Err(outcome) => return outcome,
    };
    let Some(key) = keys.first().cloned() else {
        return failed("the fixture returned no key for Fresh");
    };

    if let Err(outcome) = create_with(f, &key, b"first", KeyRequest::Fresh).await {
        return outcome;
    }
    // The key is now this run's own, so overwriting it is permitted.
    if let Err(e) = f.store().set(&key, b"second", &metadata()).await {
        return e.into();
    }

    match f.store().get_bytes(&key).await {
        Ok(bytes) if bytes == b"second" => RuleOutcome::Passed,
        Ok(bytes) if bytes == b"first" => failed_at(
            format!("writing {} a second time did not replace its content", key.encode()),
            vec![key],
        ),
        Ok(bytes) => failed_at(
            format!(
                "after two writes {} holds {} bytes, matching neither",
                key.encode(),
                bytes.len()
            ),
            vec![key],
        ),
        Err(e) => failed_at(
            format!("after two writes, get gave {:?}", e.error_type),
            vec![key],
        ),
    }
}
