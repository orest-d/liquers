//! §4 — absence is not an error.
//!
//! | Call | On a key that is simply absent |
//! |---|---|
//! | `is_dir`, `contains` | `Ok(false)` |
//! | `get`, `get_bytes`, `get_metadata` | `Err(KeyNotFound)` |
//! | `removedir` | `Ok(())` |
//!
//! The distinction between "not there" and "could not tell" is load-bearing: a store reporting an
//! S3 403 as `Ok(false)` tells a caller a directory does not exist when the truth is that
//! permission was refused. These rules check the *absent* half; no rule can provoke a backend
//! failure portably, so the other half stays a matter for review.

use crate::error::ErrorType;
use crate::store_conformance::rules::support::require_absent;
use crate::store_conformance::{failed, failed_at, keys_for, Fixture, KeyRequest, RuleOutcome};

/// `absence01` — reading an absent key gives `KeyNotFound`, from all three read methods.
///
/// All three, because they are separately implemented in most stores and a store that gets `get`
/// right can still return a general error from `get_metadata`.
pub async fn absence01(f: &dyn Fixture) -> RuleOutcome {
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

    let observed = [
        ("get", f.store().get(&key).await.err().map(|e| e.error_type)),
        (
            "get_bytes",
            f.store().get_bytes(&key).await.err().map(|e| e.error_type),
        ),
        (
            "get_metadata",
            f.store()
                .get_metadata(&key)
                .await
                .err()
                .map(|e| e.error_type),
        ),
    ];

    for (method, error_type) in observed {
        match error_type {
            Some(ErrorType::KeyNotFound) => {}
            Some(other) => {
                return failed_at(
                    format!(
                        "{method}({}) on an absent key gave {other:?}, not KeyNotFound",
                        key.encode()
                    ),
                    vec![key],
                )
            }
            None => {
                return failed_at(
                    format!("{method}({}) succeeded on an absent key", key.encode()),
                    vec![key],
                )
            }
        }
    }
    RuleOutcome::Passed
}

/// `absence02` — `contains` on an absent key is `Ok(false)`, not an error.
pub async fn absence02(f: &dyn Fixture) -> RuleOutcome {
    let keys = match keys_for(f, KeyRequest::Fresh).await {
        Ok(k) => k,
        Err(outcome) => return outcome,
    };
    let Some(key) = keys.first().cloned() else {
        return failed("the fixture returned no key for Fresh");
    };

    match f.store().contains(&key).await {
        Ok(false) => RuleOutcome::Passed,
        Ok(true) => RuleOutcome::SkippedPrecondition {
            request: KeyRequest::Fresh,
            reason: format!("{} exists, so it cannot test absence", key.encode()),
        },
        Err(e) => failed_at(
            format!(
                "contains({}) returned {:?} for an absent key; absence is Ok(false)",
                key.encode(),
                e.error_type
            ),
            vec![key],
        ),
    }
}

/// `absence03` — `removedir` on a directory that does not exist returns `Ok(())`.
///
/// Stated as `Ok(())` rather than "does not claim to have removed one", which would be satisfied by
/// either answer and check nothing. The rule requires
/// [`Capability::RemoveDirectories`](crate::store_conformance::Capability), so a store that refuses
/// directory removal outright is not asked: its `Err(KeyNotSupported)` is a refusal, not a false
/// claim of success, and the postcondition already holds.
pub async fn absence03(f: &dyn Fixture) -> RuleOutcome {
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

    match f.store().removedir(&key).await {
        Ok(()) => RuleOutcome::Passed,
        Err(e) => failed_at(
            format!(
                "removedir({}) on an absent directory gave {:?}; the postcondition already holds, \
                 so this is Ok(())",
                key.encode(),
                e.error_type
            ),
            vec![key],
        ),
    }
}
