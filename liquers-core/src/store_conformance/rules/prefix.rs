//! §6 — keys, prefixes and routing.
//!
//! A store is constructed with a `prefix: Key`. `AsyncStoreRouter::is_dir` and `listdir` select on
//! `key_prefix()` **alone** — unlike `find_store`, which also consults `is_supported` — so a store
//! that under-reports its prefix answers for keys belonging to stores listed after it.
//!
//! `is_supported` is a *separate* question from the prefix test the router already performs: it is
//! what makes layering (`with_overlay`, `with_fallback`) work, and a store answering "yes" to every
//! absolute key cannot participate in a layering correctly.

use crate::store_conformance::{failed, failed_at, keys_for, Fixture, KeyRequest, RuleOutcome};

/// `prefix01` — `key_prefix()` reports the prefix the store was configured with.
///
/// The comparison is against [`Fixture::expected_prefix`], **not** against `key_prefix()` itself.
/// Without independent ground truth this rule could only compare the method under test with
/// itself, and a store returning `Key::new()` — which is the divergence this rule exists for —
/// would pass it.
pub async fn prefix01(f: &dyn Fixture) -> RuleOutcome {
    let expected = f.expected_prefix();
    let reported = f.store().key_prefix();
    if reported == expected {
        RuleOutcome::Passed
    } else {
        failed_at(
            format!(
                "key_prefix() reports {:?} but the store was configured with {:?}",
                reported.encode(),
                expected.encode()
            ),
            vec![reported, expected],
        )
    }
}

/// `prefix02` — `is_supported` is false for a key outside this store's prefix.
pub async fn prefix02(f: &dyn Fixture) -> RuleOutcome {
    let keys = match keys_for(f, KeyRequest::OutsidePrefix).await {
        Ok(k) => k,
        Err(outcome) => return outcome,
    };
    let Some(key) = keys.first().cloned() else {
        return failed("the fixture returned no key for OutsidePrefix");
    };
    if f.store().is_supported(&key) {
        failed_at(
            format!(
                "is_supported({}) is true though the key is outside the prefix {}",
                key.encode(),
                f.expected_prefix().encode()
            ),
            vec![key],
        )
    } else {
        RuleOutcome::Passed
    }
}

/// `prefix03` — `is_supported` is false for a key whose *shape* this store cannot address.
///
/// Distinct from `prefix02`: one is about the prefix, the other about the key's form. A single
/// request serving both would let a fixture answer for one and leave the other unchecked.
pub async fn prefix03(f: &dyn Fixture) -> RuleOutcome {
    let keys = match keys_for(f, KeyRequest::UnsupportedShape).await {
        Ok(k) => k,
        Err(outcome) => return outcome,
    };
    let Some(key) = keys.first().cloned() else {
        return failed("the fixture returned no key for UnsupportedShape");
    };
    if f.store().is_supported(&key) {
        failed_at(
            format!(
                "is_supported({}) is true though the store cannot address that key shape",
                key.encode()
            ),
            vec![key],
        )
    } else {
        RuleOutcome::Passed
    }
}

/// `prefix04` — `is_supported` is **true** for a key inside the prefix that the store can address.
///
/// The positive half, and the one that was missing. `prefix02` and `prefix03` both assert *false*,
/// and the trait default returns `false` unconditionally — so without this rule a store that
/// refuses everything passes both and looks conformant while being unusable in a layering.
pub async fn prefix04(f: &dyn Fixture) -> RuleOutcome {
    let keys = match keys_for(f, KeyRequest::Supported).await {
        Ok(k) => k,
        Err(outcome) => return outcome,
    };
    let Some(key) = keys.first().cloned() else {
        return failed("the fixture returned no key for Supported");
    };
    if f.store().is_supported(&key) {
        RuleOutcome::Passed
    } else {
        failed_at(
            format!(
                "is_supported({}) is false for a key the store is expected to accept; a store that \
                 refuses everything cannot take part in a layering",
                key.encode()
            ),
            vec![key],
        )
    }
}
