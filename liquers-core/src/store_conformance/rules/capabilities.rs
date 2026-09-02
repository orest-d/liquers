//! Refuting rules — the ones that run when a capability is declared **absent**.
//!
//! Capability gating alone makes `false` an exit rather than a claim. Without these, a fully
//! writable store could declare every capability `false`, skip every write, removal and enumeration
//! check, and satisfy `assert_conformant` — and the store least likely to be *given* a capability is
//! the one whose implementation of it is broken. That is how a `makedir` recording nothing would
//! escape `explicit01`.
//!
//! Each rule here asserts the store really does refuse what it says it cannot do. They need no
//! mutation of their own: a store that refuses returns an error, and one that does not has already
//! contradicted its declaration by the time we look.

use crate::store_conformance::rules::support::metadata;
use crate::store_conformance::{failed, failed_at, keys_for, Fixture, KeyRequest, RuleOutcome};

/// Ask for a fresh key, or hand back the outcome that declines the rule.
macro_rules! fresh {
    ($f:expr) => {
        match keys_for($f, KeyRequest::Fresh).await {
            Ok(keys) => match keys.first().cloned() {
                Some(key) => key,
                None => return failed("the fixture returned no key for Fresh"),
            },
            Err(outcome) => return outcome,
        }
    };
}

/// `nowrite01` — a store declaring no `Write` refuses `set`.
pub async fn nowrite01(f: &dyn Fixture) -> RuleOutcome {
    let key = fresh!(f);
    match f.store().set(&key, b"must be refused", &metadata()).await {
        Err(_) => RuleOutcome::Passed,
        Ok(()) => {
            f.record_created(&key);
            failed_at(
                format!(
                    "the fixture declares no Write capability, but set({}) succeeded — a \
                     declaration is a claim, not a way to skip the write rules",
                    key.encode()
                ),
                vec![key],
            )
        }
    }
}

/// `noremove01` — a store declaring no `Remove` refuses `remove`.
pub async fn noremove01(f: &dyn Fixture) -> RuleOutcome {
    let key = fresh!(f);
    match f.store().remove(&key).await {
        Err(_) => RuleOutcome::Passed,
        Ok(()) => failed_at(
            format!(
                "the fixture declares no Remove capability, but remove({}) succeeded",
                key.encode()
            ),
            vec![key],
        ),
    }
}

/// `nodir01` — a store declaring no `Directories` answers `is_dir` false, or refuses.
///
/// A store with no directory structure may legitimately answer `Ok(false)` or refuse; what it may
/// not do is claim a directory exists while declaring it has none.
pub async fn nodir01(f: &dyn Fixture) -> RuleOutcome {
    let key = fresh!(f);
    match f.store().is_dir(&key).await {
        Ok(false) | Err(_) => RuleOutcome::Passed,
        Ok(true) => failed_at(
            format!(
                "the fixture declares no Directories capability, but is_dir({}) is true",
                key.encode()
            ),
            vec![key],
        ),
    }
}

/// `nomakedir01` — a store declaring no `ExplicitDirectories` refuses `makedir`.
///
/// The one that matters most: a `makedir` recording nothing and returning `Ok(())` was a P0
/// (`CORE-ASYNC-MEMORY-STORE-MAKEDIR-DOES-NOTHING`), and declaring the capability `false` is
/// exactly how such a store would avoid `explicit01`.
pub async fn nomakedir01(f: &dyn Fixture) -> RuleOutcome {
    let key = fresh!(f);
    match f.store().makedir(&key).await {
        Err(_) => RuleOutcome::Passed,
        Ok(()) => {
            f.record_created(&key);
            failed_at(
                format!(
                    "the fixture declares no ExplicitDirectories capability, but makedir({}) \
                     returned Ok — either it creates directories, or it is silently doing nothing",
                    key.encode()
                ),
                vec![key],
            )
        }
    }
}

/// `noremovedir01` — a store declaring no `RemoveDirectories` refuses `removedir`.
pub async fn noremovedir01(f: &dyn Fixture) -> RuleOutcome {
    let key = fresh!(f);
    match f.store().removedir(&key).await {
        Err(_) => RuleOutcome::Passed,
        Ok(()) => failed_at(
            format!(
                "the fixture declares no RemoveDirectories capability, but removedir({}) \
                 returned Ok",
                key.encode()
            ),
            vec![key],
        ),
    }
}

/// `nokeys01` — a store declaring no `EnumerateKeys` does not enumerate.
///
/// Refusing is the clearest answer; an empty listing is accepted, since a store with nothing to
/// enumerate cannot be distinguished from one that will not. What fails is returning content while
/// declaring it cannot.
pub async fn nokeys01(f: &dyn Fixture) -> RuleOutcome {
    match f.store().keys().await {
        Err(_) => RuleOutcome::Passed,
        // A store may still answer with its own prefix and nothing else — the `AsyncStore` default
        // appends it unconditionally — and that is not enumeration. Only *contents* contradict the
        // declaration.
        Ok(keys) if keys.len() <= 1 => RuleOutcome::Passed,
        Ok(keys) => failed(format!(
            "the fixture declares no EnumerateKeys capability, but keys() returned {} keys",
            keys.len()
        )),
    }
}
