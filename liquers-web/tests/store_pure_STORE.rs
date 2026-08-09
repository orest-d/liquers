//! Pure-function store tests: the key guard, and (from M3) URL construction and metadata
//! inference.
//!
//! Test IDs and names follow `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §3. The inventory is in
//! `specs/design/liquers-web-store/phase3-examples.md`, test groups 3 and 4.
//!
//! No DOM and no browser API, so these run under Node with no WebDriver:
//!
//! ```text
//! cargo test -p liquers-web --target wasm32-unknown-unknown --test store_pure_STORE
//! ```

#![cfg(target_arch = "wasm32")]

use liquers_core::error::ErrorType;
use liquers_core::parse::parse_key;
use liquers_web::store::check_key;
use wasm_bindgen_test::*;

const STORE: &str = "test store";

/// Asserts that a key shape is refused as unsupported, not merely that something went wrong.
fn assert_refused(key_text: &str) {
    let key = parse_key(key_text)
        .unwrap_or_else(|e| panic!("{key_text:?} must parse before it can be refused: {}", e.message));
    match check_key(&key, STORE) {
        Ok(()) => panic!("{key_text:?} must be refused by the key guard"),
        Err(e) => assert_eq!(
            e.error_type,
            ErrorType::KeyNotSupported,
            "{key_text:?}: wrong error type ({})",
            e.message
        ),
    }
}

/// keyguard01 / STORE05 — a leading `..` is refused.
///
/// `..` is a valid `ResourceName`, so this key parses and plans; refusing it is the store's job.
#[wasm_bindgen_test]
fn keyguard01_parent_segment_refused() {
    assert_refused("../escape");
}

/// keyguard02 — an *interior* `..` is refused.
///
/// This matters more than `keyguard01`: a guard that inspects only the first segment passes that
/// test and still lets `a/../../etc` escape.
#[wasm_bindgen_test]
fn keyguard02_interior_parent_refused() {
    assert_refused("a/../b");
    assert_refused("a/../../etc");
}

/// keyguard03 — `.` is refused too.
///
/// Harmless as a path, but as an *address* it is a second spelling of the parent directory, and
/// two spellings of one address would alias two assets.
#[wasm_bindgen_test]
fn keyguard03_current_segment_refused() {
    assert_refused("a/./b");
}

/// keyguard04 — ordinary keys are accepted.
///
/// The negative half. Without it, `check_key` could refuse everything and the three tests above
/// would all still pass.
#[wasm_bindgen_test]
fn keyguard04_ordinary_keys_accepted() {
    for text in ["a", "a/b.txt", "a.b/c", "data/sub/report.json", "notes"] {
        let key = parse_key(text).unwrap_or_else(|e| panic!("{text:?}: {}", e.message));
        assert!(
            check_key(&key, STORE).is_ok(),
            "{text:?} must be accepted by the key guard"
        );
    }
}

/// keyguard05 — the empty key is accepted.
///
/// It is the store root, which `AsyncStore::keys` uses via `key_prefix()`, so refusing it would
/// break listing at the top level.
#[wasm_bindgen_test]
fn keyguard05_empty_key_accepted() {
    let root = liquers_core::query::Key::new();
    assert!(check_key(&root, STORE).is_ok(), "the root key must be accepted");
}
