//! Helpers every rule group needs, so the discipline is written once rather than per rule.
//!
//! The two constraints these encode are the ones violated by habit: **check before mutating** (so
//! a rule at [`SafetyLevel::Scratch`](crate::store_conformance::SafetyLevel) never touches a key
//! that was already there), and **record every key created** (so cleanup and the residue report
//! can see it). A rule that open-codes `set` without both is the failure `H6` and `H7` exist to
//! catch.

use crate::metadata::{Metadata, MetadataRecord};
use crate::query::Key;
use crate::store_conformance::{Fixture, KeyRequest, RuleOutcome};

/// Metadata for a key a rule writes. Rules do not test metadata content except where they say so.
pub fn metadata() -> Metadata {
    Metadata::MetadataRecord(MetadataRecord::new())
}

/// Confirm `key` is absent, or produce the outcome that declines the rule.
pub async fn require_absent(
    f: &dyn Fixture,
    key: &Key,
    request: KeyRequest,
) -> Result<(), RuleOutcome> {
    match f.store().contains(key).await {
        Ok(false) => Ok(()),
        Ok(true) => Err(RuleOutcome::SkippedPrecondition {
            request,
            reason: format!(
                "{} already exists; a rule may only touch keys it created",
                key.encode()
            ),
        }),
        Err(e) => Err(e.into()),
    }
}

/// Create `key` with `data`, after checking it is absent, recording it on success.
pub async fn create_with(
    f: &dyn Fixture,
    key: &Key,
    data: &[u8],
    request: KeyRequest,
) -> Result<(), RuleOutcome> {
    require_absent(f, key, request).await?;
    match f.store().set(key, data, &metadata()).await {
        Ok(()) => {
            f.record_created(key);
            Ok(())
        }
        Err(e) => Err(e.into()),
    }
}

/// Create `key` with filler content.
pub async fn create(f: &dyn Fixture, key: &Key, request: KeyRequest) -> Result<(), RuleOutcome> {
    create_with(f, key, b"conformance", request).await
}
