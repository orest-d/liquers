//! §3 — derived and explicit directories are different things.
//!
//! A directory **derived** from its children retires when the last child is removed. A directory
//! **created** by `makedir` has no children and persists until `removedir`. A derived index alone
//! cannot express the second, which is why `AsyncMemoryStore::makedir` once recorded nothing at all
//! and reported success — a P0.
//!
//! Retirement is **not** universal, and the capability model says so: a real filesystem directory
//! is an object in its own right and survives its last file. Only a store whose directories are
//! derived declares [`Capability::DerivedDirectories`](crate::store_conformance::Capability).

use crate::query::Key;
use crate::store_conformance::rules::support::{create, require_absent};
use crate::store_conformance::{failed, failed_at, keys_for, Fixture, KeyRequest, RuleOutcome};

/// `explicit01` — `makedir` creates a directory that exists, is empty, and persists.
///
/// The P0 this catches (`CORE-ASYNC-MEMORY-STORE-MAKEDIR-DOES-NOTHING`) was a `makedir` that
/// recorded nothing and returned `Ok(())`. Asking only whether `makedir` succeeded would not have
/// caught it; asking whether the directory is *there afterwards* does.
pub async fn explicit01(f: &dyn Fixture) -> RuleOutcome {
    let keys = match keys_for(f, KeyRequest::Fresh).await {
        Ok(k) => k,
        Err(outcome) => return outcome,
    };
    let Some(dir) = keys.first().cloned() else {
        return failed("the fixture returned no key for Fresh");
    };
    if let Err(outcome) = require_absent(f, &dir, KeyRequest::Fresh).await {
        return outcome;
    }

    if let Err(e) = f.store().makedir(&dir).await {
        return e.into();
    }
    f.record_created(&dir);

    match f.store().is_dir(&dir).await {
        Ok(true) => {}
        Ok(false) => {
            return failed_at(
                format!(
                    "makedir({0}) returned Ok but is_dir({0}) is false — it recorded nothing",
                    dir.encode()
                ),
                vec![dir],
            )
        }
        Err(e) => return e.into(),
    }

    match f.store().listdir(&dir).await {
        Ok(entries) if entries.is_empty() => RuleOutcome::Passed,
        Ok(entries) => failed_at(
            format!(
                "a freshly created directory {} lists {entries:?}",
                dir.encode()
            ),
            vec![dir],
        ),
        Err(e) => e.into(),
    }
}

/// `explicit02` — a *derived* directory retires when its last child goes.
///
/// Gated on [`Capability::DerivedDirectories`](crate::store_conformance::Capability), because this
/// is false of a real filesystem: `AsyncFileStore::remove` unlinks the file and leaves the
/// directory, and `is_dir` stats the path. Requiring it of every store would fail a correct one.
pub async fn explicit02(f: &dyn Fixture) -> RuleOutcome {
    let keys = match keys_for(f, KeyRequest::FreshNested { depth: 1 }).await {
        Ok(k) => k,
        Err(outcome) => return outcome,
    };
    let Some(leaf) = keys.first().cloned() else {
        return failed("the fixture returned no key for FreshNested");
    };
    let parent = leaf.parent();
    if parent.is_empty() {
        return RuleOutcome::SkippedPrecondition {
            request: KeyRequest::FreshNested { depth: 1 },
            reason: "the fixture returned a key with no parent directory".to_owned(),
        };
    }

    if let Err(outcome) = create(f, &leaf, KeyRequest::FreshNested { depth: 1 }).await {
        return outcome;
    }
    match f.store().is_dir(&parent).await {
        Ok(true) => {}
        Ok(false) => return RuleOutcome::Passed, // `dir01` owns this disagreement
        Err(e) => return e.into(),
    }

    if let Err(e) = f.store().remove(&leaf).await {
        return e.into();
    }

    match f.store().is_dir(&parent).await {
        Ok(false) => RuleOutcome::Passed,
        Ok(true) => failed_at(
            format!(
                "{} still reports as a directory after its only child {} was removed",
                parent.encode(),
                leaf.encode()
            ),
            vec![parent, leaf],
        ),
        Err(e) => e.into(),
    }
}

/// `explicit03` — a recursive `removedir` takes explicit descendants with it.
///
/// Forgetting one marker leaves a `makedir` descendant reporting as a directory after the removal
/// that was supposed to contain it succeeded — a directory with no parent and no children, which
/// nothing will ever clean up.
pub async fn explicit03(f: &dyn Fixture) -> RuleOutcome {
    let keys = match keys_for(f, KeyRequest::FreshNested { depth: 1 }).await {
        Ok(k) => k,
        Err(outcome) => return outcome,
    };
    let Some(leaf) = keys.first().cloned() else {
        return failed("the fixture returned no key for FreshNested");
    };
    let inner = leaf.parent();
    let outer = inner.parent();
    if inner.is_empty() || outer.is_empty() {
        return RuleOutcome::SkippedPrecondition {
            request: KeyRequest::FreshNested { depth: 1 },
            reason: "two levels of directory are needed and the fixture supplied fewer".to_owned(),
        };
    }

    if let Err(e) = f.store().makedir(&inner).await {
        return e.into();
    }
    f.record_created(&inner);

    if let Err(e) = f.store().removedir(&outer).await {
        return e.into();
    }

    match f.store().is_dir(&inner).await {
        Ok(false) => RuleOutcome::Passed,
        Ok(true) => failed_at(
            format!(
                "removedir({}) succeeded but the explicit directory {} beneath it survives",
                outer.encode(),
                inner.encode()
            ),
            vec![outer, inner],
        ),
        Err(e) => e.into(),
    }
}
