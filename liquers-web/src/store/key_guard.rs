//! Key shapes the browser stores refuse.
//!
//! Two different wrongs, kept apart because a caller can act on the difference:
//!
//! - **A relative key** — any element that is `.` or `..`. A store requires an absolute key, and
//!   here a `..` element would let a key escape a URL prefix or a `localStorage` namespace. The
//!   rule lives in [`liquers_core::query::Key::as_absolute`] and the refusal is
//!   [`liquers_core::error::ErrorType::KeyNotAbsolute`]; see the `liquers_core::store` module
//!   documentation for why refusal rather than normalization.
//! - **An empty element** — malformed rather than relative, and refused with
//!   [`liquers_core::error::Error::key_not_supported`]. The store name is informative there, which
//!   is why it keeps the store-scoped error.
//!
//! This module used to carry its own copy of the relative-key check while
//! `specs/issues/STORE-FILESTORE-PATH-TRAVERSAL.md` was open. That is fixed, the rule is shared,
//! and only the empty-element case remains local.

use liquers_core::error::Error;
use liquers_core::query::Key;

/// Returns an error when `key` has an element that could escape the store's namespace.
///
/// Called from every `is_supported` **and** from each fallible method, because `is_supported`
/// gates *routing*, not direct calls: a store used without the router would otherwise skip the
/// check.
pub fn check_key(key: &Key, store_name: &str) -> Result<(), Error> {
    key.as_absolute()?;
    for segment in key.iter() {
        if segment.encode().is_empty() {
            return Err(Error::key_not_supported(key, store_name));
        }
    }
    Ok(())
}
