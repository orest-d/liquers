//! §9 — what `keys()` returns.
//!
//! > **`keys()` returns data keys, the directories above them, and the store's own prefix. Every
//! > key it returns starts with that prefix.**
//!
//! The second sentence is the one a router depends on: a store enumerating keys it does not own
//! makes a composed namespace unreadable, because the caller cannot tell which store an answer came
//! from. The cost of the first is that an enumerated key is not necessarily one `get` will succeed
//! on — a directory is enumerated and cannot be read as data.

use crate::store_conformance::rules::support::create;
use crate::store_conformance::{failed, failed_at, keys_for, Fixture, KeyRequest, RuleOutcome};

/// `keys01` — every key `keys()` returns starts with the store's prefix.
///
/// Checked against [`Fixture::expected_prefix`] rather than `key_prefix()`, so a store that
/// under-reports its prefix cannot satisfy this rule by agreeing with itself.
pub async fn keys01(f: &dyn Fixture) -> RuleOutcome {
    let prefix = f.expected_prefix();
    match f.store().keys().await {
        Ok(keys) => {
            for key in &keys {
                if !key.has_key_prefix(&prefix) {
                    return failed_at(
                        format!(
                            "keys() returned {}, which is not under the store's prefix {}",
                            key.encode(),
                            prefix.encode()
                        ),
                        vec![key.clone(), prefix],
                    );
                }
            }
            RuleOutcome::Passed
        }
        Err(e) => e.into(),
    }
}

/// `keys02` — `keys()` returns data keys, the directories above them, and the prefix itself.
///
/// The divergence recorded as `CORE-STORE-KEYS-MEANS-TWO-DIFFERENT-THINGS`: one stored object
/// yields one key or four, depending on which store answers, and a router can return both shapes at
/// once.
pub async fn keys02(f: &dyn Fixture) -> RuleOutcome {
    let requested = match keys_for(f, KeyRequest::FreshNested { depth: 1 }).await {
        Ok(k) => k,
        Err(outcome) => return outcome,
    };
    let Some(leaf) = requested.first().cloned() else {
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

    let keys = match f.store().keys().await {
        Ok(k) => k,
        Err(e) => return e.into(),
    };
    let prefix = f.expected_prefix();

    for (what, expected) in [
        ("the data key", &leaf),
        ("the directory above it", &parent),
        ("the store's own prefix", &prefix),
    ] {
        if !keys.contains(expected) {
            return failed_at(
                format!(
                    "keys() omitted {what} {}; it returned {:?}",
                    expected.encode(),
                    keys.iter().map(|k| k.encode()).collect::<Vec<_>>()
                ),
                vec![expected.clone()],
            );
        }
    }
    RuleOutcome::Passed
}
