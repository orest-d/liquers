//! §7 — key shape.
//!
//! A key given to a store is **absolute**: no element may be `.` or `..`. A relative key reaching a
//! store is refused with [`ErrorType::KeyNotAbsolute`], by every method and by the path builders
//! directly. Relative keys are resolved at plan level; a store never resolves them.

use crate::error::ErrorType;
use crate::store_conformance::rules::support::metadata;
use crate::store_conformance::{failed, failed_at, keys_for, Fixture, KeyRequest, RuleOutcome};

/// The relative key this pair of rules probes with, or the outcome that declines them.
async fn relative_key(f: &dyn Fixture) -> Result<crate::query::Key, RuleOutcome> {
    let keys = keys_for(f, KeyRequest::Relative).await?;
    let Some(key) = keys.first().cloned() else {
        return Err(failed("the fixture returned no key for Relative"));
    };
    if !key.is_relative() {
        return Err(failed_at(
            format!(
                "the fixture offered {} as relative, but Key::is_relative disagrees",
                key.encode()
            ),
            vec![key],
        ));
    }
    Ok(key)
}

/// Check one method's refusal, stopping the rule at the first method that accepts the key.
fn refusal(
    method: &str,
    key: &crate::query::Key,
    error_type: Option<ErrorType>,
) -> Option<RuleOutcome> {
    match error_type {
        Some(ErrorType::KeyNotAbsolute) => None,
        Some(other) => Some(failed_at(
            format!(
                "{method} refused the relative key {} with {other:?}, not KeyNotAbsolute",
                key.encode()
            ),
            vec![key.clone()],
        )),
        None => Some(failed_at(
            format!(
                "{method} accepted the relative key {} — a store never resolves one",
                key.encode()
            ),
            vec![key.clone()],
        )),
    }
}

/// `keyshape01` — the **reading** methods refuse a relative key with `KeyNotAbsolute`.
///
/// Safe at `ReadOnly`: nothing here mutates even if the refusal is broken.
pub async fn keyshape01(f: &dyn Fixture) -> RuleOutcome {
    let key = match relative_key(f).await {
        Ok(k) => k,
        Err(outcome) => return outcome,
    };
    let store = f.store();
    let observed = [
        ("get", store.get(&key).await.err().map(|e| e.error_type)),
        ("get_bytes", store.get_bytes(&key).await.err().map(|e| e.error_type)),
        ("get_metadata", store.get_metadata(&key).await.err().map(|e| e.error_type)),
        ("contains", store.contains(&key).await.err().map(|e| e.error_type)),
        ("is_dir", store.is_dir(&key).await.err().map(|e| e.error_type)),
        ("listdir", store.listdir(&key).await.err().map(|e| e.error_type)),
    ];
    for (method, error_type) in observed {
        if let Some(outcome) = refusal(method, &key, error_type) {
            return outcome;
        }
    }
    RuleOutcome::Passed
}

/// `keyshape02` — the **mutating** methods refuse a relative key with `KeyNotAbsolute`.
///
/// **`Scratch`, and that is a safety requirement rather than bookkeeping.** Checking that `set`,
/// `remove` and `removedir` refuse means *calling* them with a traversal key — and on exactly the
/// nonconforming store this rule diagnoses, that key may resolve outside the store's namespace and
/// destroy data that was never this run's. `CreateOnly` forbids removal, so a rule invoking
/// `removedir` cannot live there; `ReadOnly` less so still.
///
/// Each method is probed **in turn, stopping at the first that accepts the key**. An eagerly built
/// list would go on calling `remove` and `removedir` after `set` had already proved the store
/// resolves relative keys — which is the moment to stop, not to continue.
pub async fn keyshape02(f: &dyn Fixture) -> RuleOutcome {
    let key = match relative_key(f).await {
        Ok(k) => k,
        Err(outcome) => return outcome,
    };
    let store = f.store();

    if let Some(outcome) = refusal(
        "set",
        &key,
        store
            .set(&key, b"must not be written", &metadata())
            .await
            .err()
            .map(|e| e.error_type),
    ) {
        return outcome;
    }
    if let Some(outcome) = refusal(
        "set_metadata",
        &key,
        store.set_metadata(&key, &metadata()).await.err().map(|e| e.error_type),
    ) {
        return outcome;
    }
    if let Some(outcome) = refusal(
        "makedir",
        &key,
        store.makedir(&key).await.err().map(|e| e.error_type),
    ) {
        return outcome;
    }
    if let Some(outcome) = refusal(
        "remove",
        &key,
        store.remove(&key).await.err().map(|e| e.error_type),
    ) {
        return outcome;
    }
    if let Some(outcome) = refusal(
        "removedir",
        &key,
        store.removedir(&key).await.err().map(|e| e.error_type),
    ) {
        return outcome;
    }
    RuleOutcome::Passed
}
