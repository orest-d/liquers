//! §7 — key shape.
//!
//! A key given to a store is **absolute**: no element may be `.` or `..`. A relative key reaching a
//! store is refused with [`ErrorType::KeyNotAbsolute`], by every method and by the path builders
//! directly. Relative keys are resolved at plan level; a store never resolves them.

use crate::error::ErrorType;
use crate::store_conformance::rules::support::metadata;
use crate::store_conformance::{failed, failed_at, keys_for, Fixture, KeyRequest, RuleOutcome};

/// `keyshape01` — every fallible key-taking method refuses a relative key with `KeyNotAbsolute`.
///
/// **Runs at `CreateOnly`, not `ReadOnly`, and that is not a formality.** Checking the refusal
/// means *calling* `set`, `remove` and `removedir` with the key — so on a store whose refusal is
/// broken, this rule mutates. A level advertised as safe against somebody's data must not contain
/// a rule that writes when the store misbehaves.
///
/// The traversal key is the exploitable shape: an *interior* `..` needs no CWD, so nothing
/// normalizes it before the store sees it.
pub async fn keyshape01(f: &dyn Fixture) -> RuleOutcome {
    let keys = match keys_for(f, KeyRequest::Relative).await {
        Ok(k) => k,
        Err(outcome) => return outcome,
    };
    let Some(key) = keys.first().cloned() else {
        return failed("the fixture returned no key for Relative");
    };
    if !key.is_relative() {
        return failed_at(
            format!(
                "the fixture offered {} as relative, but Key::is_relative disagrees",
                key.encode()
            ),
            vec![key],
        );
    }

    let store = f.store();
    let observed = [
        ("get", store.get(&key).await.err().map(|e| e.error_type)),
        (
            "get_bytes",
            store.get_bytes(&key).await.err().map(|e| e.error_type),
        ),
        (
            "get_metadata",
            store.get_metadata(&key).await.err().map(|e| e.error_type),
        ),
        (
            "set",
            store
                .set(&key, b"must not be written", &metadata())
                .await
                .err()
                .map(|e| e.error_type),
        ),
        (
            "set_metadata",
            store
                .set_metadata(&key, &metadata())
                .await
                .err()
                .map(|e| e.error_type),
        ),
        ("remove", store.remove(&key).await.err().map(|e| e.error_type)),
        (
            "removedir",
            store.removedir(&key).await.err().map(|e| e.error_type),
        ),
        (
            "contains",
            store.contains(&key).await.err().map(|e| e.error_type),
        ),
        ("is_dir", store.is_dir(&key).await.err().map(|e| e.error_type)),
        (
            "makedir",
            store.makedir(&key).await.err().map(|e| e.error_type),
        ),
        (
            "listdir",
            store.listdir(&key).await.err().map(|e| e.error_type),
        ),
    ];

    for (method, error_type) in observed {
        match error_type {
            Some(ErrorType::KeyNotAbsolute) => {}
            Some(other) => {
                return failed_at(
                    format!(
                        "{method} refused the relative key {} with {other:?}, not KeyNotAbsolute",
                        key.encode()
                    ),
                    vec![key],
                )
            }
            None => {
                return failed_at(
                    format!(
                        "{method} accepted the relative key {} — a store never resolves one",
                        key.encode()
                    ),
                    vec![key],
                )
            }
        }
    }
    RuleOutcome::Passed
}
