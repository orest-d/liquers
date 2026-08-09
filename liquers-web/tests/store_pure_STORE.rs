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

// ---------------------------------------------------------------------------
// FetchStore pure functions: URL construction and metadata inference (M3).
// ---------------------------------------------------------------------------

use liquers_web::store::fetch::{directory_index, infer_metadata, key_to_url, media_type_of};
use std::collections::BTreeSet;

/// Reads the media type out of a `Metadata`, matching both variants explicitly.
fn media_type(metadata: &liquers_core::metadata::Metadata) -> String {
    match metadata {
        liquers_core::metadata::Metadata::MetadataRecord(record) => record.get_media_type(),
        liquers_core::metadata::Metadata::LegacyMetadata(value) => value
            .get("media_type")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
    }
}

/// url01 — the store's own prefix is stripped, the rest of the key is joined verbatim.
#[wasm_bindgen_test]
fn url01_prefix_is_stripped() {
    let prefix = parse_key("data").expect("prefix");
    let key = parse_key("data/sub/report.json").expect("key");
    let url = key_to_url("https://example.org/reference/", &prefix, &key, STORE).expect("url");
    assert_eq!(url, "https://example.org/reference/sub/report.json");
}

/// url02 — a missing trailing slash on the prefix does not fuse two segments.
#[wasm_bindgen_test]
fn url02_prefix_is_normalized() {
    let prefix = liquers_core::query::Key::new();
    let key = parse_key("a.txt").expect("key");
    let url = key_to_url("https://example.org/data", &prefix, &key, STORE).expect("url");
    assert_eq!(url, "https://example.org/data/a.txt");
}

/// url03 / STORE05 — a key the guard refuses never becomes a URL.
///
/// This is the fetch store's half of STORE05: without it, `..` would resolve against the URL
/// prefix on the *server* side and read outside the published directory.
#[wasm_bindgen_test]
fn url03_escaping_key_is_refused() {
    let prefix = liquers_core::query::Key::new();
    let key = parse_key("a/../../etc/passwd").expect("key");
    match key_to_url("https://example.org/data/", &prefix, &key, STORE) {
        Ok(url) => panic!("an escaping key must not produce a URL, got {url:?}"),
        Err(e) => assert_eq!(e.error_type, ErrorType::KeyNotSupported, "{}", e.message),
    }
}

/// STORE10 — the filename extension beats the response `Content-Type`.
///
/// The inverse of the usual web rule, and deliberate: Liquers dispatches deserialization on
/// `data_format`, which comes from the extension, so a server labelling every file `text/plain`
/// would break every command downstream of a fetched asset.
#[wasm_bindgen_test]
fn store10_extension_wins_over_content_type() {
    let key = parse_key("data/input.csv").expect("key");
    let metadata = infer_metadata(&key, Some("text/plain"), None);
    assert_eq!(media_type(&metadata), "text/csv");
}

/// STORE10 — the response fills in when the extension says nothing.
#[wasm_bindgen_test]
fn store10_content_type_fills_in_unknown_extension() {
    let unknown_ext = parse_key("data/blob.zzz").expect("key");
    let metadata = infer_metadata(&unknown_ext, Some("application/json; charset=utf-8"), None);
    assert_eq!(
        media_type(&metadata),
        "application/json",
        "parameters must be stripped from the header"
    );

    let no_ext = parse_key("data/notes").expect("key");
    let metadata = infer_metadata(&no_ext, Some("text/markdown"), None);
    assert_eq!(media_type(&metadata), "text/markdown");

    // With neither source, the answer is the honest unknown, not an empty string.
    let metadata = infer_metadata(&no_ext, None, None);
    assert_eq!(media_type(&metadata), "application/octet-stream");

    // A header carrying no usable type must not overwrite the extension's answer.
    assert_eq!(media_type_of("  ; charset=utf-8"), None);
}

/// STORE10 — size comes from the response.
#[wasm_bindgen_test]
fn store10_content_length_becomes_file_size() {
    let key = parse_key("data/input.csv").expect("key");
    let metadata = infer_metadata(&key, None, Some(4096));
    assert_eq!(metadata.file_size(), Some(4096));
}

/// STORE03 — the directory index derived from a key set is internally consistent.
///
/// Every ancestor of every key is a directory, and every key's own name appears in its parent's
/// listing — which is what makes `listdir`, `is_dir` and `contains` agree without a network call.
#[wasm_bindgen_test]
fn store03_directory_index_is_consistent() {
    let keys: BTreeSet<_> = ["data/a.txt", "data/b.txt", "data/sub/c.txt"]
        .iter()
        .map(|k| parse_key(k).expect("key"))
        .collect();
    let dirs = directory_index(&keys);

    let root = liquers_core::query::Key::new();
    let root_children: Vec<String> = dirs
        .get(&root)
        .expect("the root must be a directory")
        .iter()
        .cloned()
        .collect();
    assert_eq!(root_children, vec!["data".to_string()]);

    let data = parse_key("data").expect("key");
    let mut children: Vec<String> = dirs.get(&data).expect("data is a directory").iter().cloned().collect();
    children.sort();
    assert_eq!(children, vec!["a.txt", "b.txt", "sub"]);

    let sub = parse_key("data/sub").expect("key");
    assert!(dirs.contains_key(&sub), "an intermediate directory must exist");
    assert!(
        !dirs.contains_key(&parse_key("data/a.txt").expect("key")),
        "a leaf key must not be a directory"
    );
}
