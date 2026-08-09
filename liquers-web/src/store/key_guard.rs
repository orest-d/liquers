//! Key shapes the browser stores refuse.
//!
//! `..` is a valid [`liquers_core::query::ResourceName`] — the parser accepts `.` as a name
//! character — so `parse_key("../escape")` succeeds and plans as an ordinary `GetAsset`. Refusing
//! it is therefore the *store's* job, and both browser stores need it for the same reason: a `..`
//! segment would let a key escape a URL prefix or a `localStorage` namespace.
//!
//! **Refusal, not normalization.** A key is an address, not a path. Silently rewriting `a/../b` to
//! `b` would make two distinct addresses alias one asset, which is worse than rejecting a key
//! nobody meant to write. [`liquers_core::query::Key::to_absolute`] does resolve `..`, but it runs
//! during relative resolution, before a store sees the key; by the time a key reaches a store it
//! is already absolute and no longer normalized by anything.
//!
//! The same hole exists in `AsyncFileStore` and is exploitable there — see
//! `specs/issues/STORE-FILESTORE-PATH-TRAVERSAL.md`, which proposes hoisting a shared version of
//! this check into `liquers_core::store` so every backend gets it. This module is the browser's
//! copy until that lands.

use liquers_core::error::Error;
use liquers_core::query::Key;

/// Returns an error when `key` has a segment that could escape the store's namespace.
///
/// Refused: `..`, `.`, and empty segments. **Every** segment is inspected, not only the first — a
/// guard that checks only the leading segment accepts `a/../../etc`, which is the shape an
/// attacker would actually write.
pub fn check_key(key: &Key, store_name: &str) -> Result<(), Error> {
    for segment in key.iter() {
        let name = segment.encode();
        if name.is_empty() || name == "." || name == ".." {
            return Err(Error::key_not_supported(key, store_name));
        }
    }
    Ok(())
}
