//! `STORE01`–`STORE06` for [`LocalStorageStore`].
//!
//! Test IDs and names follow `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §3. The inventory is in
//! `specs/design/liquers-web-store/phase3-examples.md`, test group 5.
//!
//! **These need a real browser,** and are therefore behind the `browser-tests` feature.
//! `localStorage` does not exist under Node — `web_sys::window()` returns `None` there — so this
//! file carries `run_in_browser`. The feature gate is what keeps the routine Node loop from
//! needing a WebDriver because of one file:
//!
//! ```text
//! CHROMEDRIVER=$(which chromedriver) cargo test -p liquers-web \
//!   --target wasm32-unknown-unknown --features browser-tests --test store_local_STORE
//! ```
//!
//! The chromedriver's major version must match the installed Chromium; a mismatch is refused
//! outright, with the two versions named in the error.
//!
//! Every test uses its own namespace and clears it first. `localStorage` persists across tests in
//! one browser session, so a suite that relies on a fresh profile passes locally and fails the
//! first time it runs twice.

#![cfg(all(target_arch = "wasm32", feature = "browser-tests"))]

use liquers_core::error::ErrorType;
use liquers_core::metadata::{Metadata, MetadataRecord};
use liquers_core::parse::parse_key;
use liquers_core::query::Key;
use liquers_core::store::AsyncStore;
use liquers_web::store::LocalStorageStore;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

/// Removes every entry under `namespace`, so each test starts from a known state.
fn clear(namespace: &str) {
    let storage = web_sys::window()
        .expect("a browser window")
        .local_storage()
        .expect("localStorage is accessible")
        .expect("localStorage is enabled");
    let prefix = format!("{namespace}/");
    let mut doomed = Vec::new();
    let length = storage.length().expect("length");
    for i in 0..length {
        if let Ok(Some(name)) = storage.key(i) {
            if name.starts_with(&prefix) {
                doomed.push(name);
            }
        }
    }
    for name in doomed {
        let _ = storage.remove_item(&name);
    }
}

/// A store on the root prefix in its own namespace.
fn store(namespace: &str) -> LocalStorageStore {
    clear(namespace);
    LocalStorageStore::new(&Key::new(), namespace, None).expect("store opens")
}

fn key(text: &str) -> Key {
    parse_key(text).expect("fixture key must parse")
}

fn text_metadata(media_type: &str) -> Metadata {
    let mut record = MetadataRecord::new();
    record.with_media_type(media_type.to_string());
    Metadata::MetadataRecord(record)
}

/// STORE01 — data and metadata go in together and come back together.
#[wasm_bindgen_test]
async fn store01_set_get_data_and_metadata() {
    let store = store("t_store01");
    let k = key("d/f.txt");

    store
        .set(&k, b"data", &text_metadata("text/plain"))
        .await
        .expect("set");

    let (data, metadata) = store.get(&k).await.expect("get");
    assert_eq!(data, b"data");
    assert_eq!(metadata.get_media_type(), "text/plain");

    // The same answer through the metadata-only path, which is a separate code path.
    let only = store.get_metadata(&k).await.expect("get_metadata");
    assert_eq!(only.get_media_type(), "text/plain");
}

/// STORE01b — binary that is not valid UTF-8 survives byte-identical.
///
/// The base64 path, exercised through the store rather than through the codec directly, so a store
/// that forgot to use the envelope would fail here even though the codec's own tests pass.
#[wasm_bindgen_test]
async fn store01_binary_round_trips_through_the_store() {
    let store = store("t_store01b");
    let k = key("d/logo.png");
    let png: Vec<u8> = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0xFF, 0xFE];

    store.set(&k, &png, &Metadata::new()).await.expect("set");
    let (back, metadata) = store.get(&k).await.expect("get");
    assert_eq!(back, png, "binary must round-trip byte-identical");
    assert_eq!(
        metadata.get_media_type(),
        "image/png",
        "metadata is finalized from the key's extension"
    );
}

/// STORE02 — a missing key is `KeyNotFound`, not a read error.
#[wasm_bindgen_test]
async fn store02_missing_key_error() {
    let store = store("t_store02");
    match store.get(&key("d/absent")).await {
        Ok(_) => panic!("an absent key must not resolve"),
        Err(e) => assert_eq!(e.error_type, ErrorType::KeyNotFound, "{}", e.message),
    }
}

/// STORE03 — listing, `contains` and `is_dir` agree with each other.
#[wasm_bindgen_test]
async fn store03_directory_listing_invariants() {
    let store = store("t_store03");
    store.set(&key("d/a"), b"1", &Metadata::new()).await.expect("set a");
    store.set(&key("d/b"), b"2", &Metadata::new()).await.expect("set b");
    store
        .set(&key("d/sub/c"), b"3", &Metadata::new())
        .await
        .expect("set c");

    let mut names = store.listdir(&key("d")).await.expect("listdir");
    names.sort();
    assert_eq!(names, vec!["a", "b", "sub"]);

    for name in &names {
        let child = key("d").join(name);
        assert!(
            store.contains(&child).await.expect("contains"),
            "{child} is listed and must therefore be contained"
        );
    }

    assert!(store.is_dir(&key("d")).await.expect("is_dir"));
    assert!(store.is_dir(&key("d/sub")).await.expect("is_dir"));
    assert!(!store.is_dir(&key("d/a")).await.expect("is_dir"));
}

/// STORE04 — `remove` and `removedir` take things away, including recursively.
#[wasm_bindgen_test]
async fn store04_remove_and_removedir() {
    let store = store("t_store04");
    let k = key("d/x");
    store.set(&k, b"1", &Metadata::new()).await.expect("set");
    store.remove(&k).await.expect("remove");
    assert!(!store.contains(&k).await.expect("contains"));

    store.set(&key("d/y"), b"2", &Metadata::new()).await.expect("set y");
    store.removedir(&key("d")).await.expect("removedir");
    assert!(!store.contains(&key("d")).await.expect("contains d"));
    assert!(
        !store.contains(&key("d/y")).await.expect("contains d/y"),
        "removedir must take the directory's contents with it"
    );
}

/// STORE05 — a key that could escape the namespace is refused.
#[wasm_bindgen_test]
async fn store05_unsupported_key() {
    let store = store("t_store05");
    for text in ["../escape", "a/../../etc"] {
        let k = key(text);
        match store.get(&k).await {
            Ok(_) => panic!("{text} must be refused"),
            Err(e) => assert_eq!(e.error_type, ErrorType::KeyNotSupported, "{}", e.message),
        }
        match store.set(&k, b"x", &Metadata::new()).await {
            Ok(()) => panic!("{text} must be refused for writes too"),
            Err(e) => assert_eq!(e.error_type, ErrorType::KeyNotSupported, "{}", e.message),
        }
        assert!(!store.is_supported(&k), "{text} must not be routed here");
    }
}

/// STORE06 — concurrent writes: the last one wins, intact.
///
/// Not "one of them won". `LocalStorageStore` never awaits — the Web Storage API is synchronous —
/// so there is no interleaving point and the outcome is determinate. Asserting the *specific* last
/// value, and that it is one byte rather than a concatenation, makes this a tripwire for the day
/// somebody makes a storage call asynchronous.
#[wasm_bindgen_test]
async fn store06_concurrent_update_policy() {
    let store = store("t_store06");
    let k = key("d/race");

    // Owned payloads, so the futures do not borrow temporaries; `join_all` then drives all ten
    // concurrently rather than one after another, which is what the contract is about.
    let payloads: Vec<Vec<u8>> = (0u8..10).map(|i| vec![i]).collect();
    let metadata = Metadata::new();
    let writes: Vec<_> = payloads
        .iter()
        .map(|payload| store.set(&k, payload, &metadata))
        .collect();
    for result in futures::future::join_all(writes).await {
        result.expect("set");
    }

    let (data, _) = store.get(&k).await.expect("get");
    assert_eq!(data.len(), 1, "the value must not be torn");
    assert_eq!(data, vec![9u8], "the last write must be the one that survives");
}

/// STORE04b / C8 — `makedir` creates a directory that has no contents, and it persists.
///
/// The reason empty-directory markers exist. A second store opened over the same namespace stands
/// in for a page reload: it rebuilds its index purely by scanning storage.
#[wasm_bindgen_test]
async fn store04_makedir_survives_reopen() {
    let namespace = "t_makedir";
    let store = store(namespace);
    let d = key("empty/dir");
    store.makedir(&d).await.expect("makedir");
    assert!(store.is_dir(&d).await.expect("is_dir"));

    let reopened = LocalStorageStore::new(&Key::new(), namespace, None).expect("reopen");
    assert!(
        reopened.is_dir(&d).await.expect("is_dir after reopen"),
        "an empty directory must survive a reload"
    );
    assert!(
        reopened.listdir(&d).await.expect("listdir").is_empty(),
        "it is still empty"
    );
}

/// C3 — a write that would exceed the quota is refused, and changes *nothing*.
///
/// The quota is 4000 rather than a token value because metadata dominates the budget for small
/// payloads: a `MetadataRecord` serializes to several hundred bytes whatever the data length. That
/// is honest accounting — those bytes really are in `localStorage` — and the first draft of this
/// test used 200, which could not hold a single entry.
///
/// The strict `used_bytes` equality is the assertion that matters. `set` writes two entries, and
/// budgeting them one at a time would let the metadata entry land and the data entry be refused,
/// leaving an orphan that occupies quota for a key with no value. `contains` would still report
/// false, so only the byte accounting catches it.
#[wasm_bindgen_test]
async fn quota_refuses_write_and_leaves_store_unchanged() {
    let namespace = "t_quota";
    clear(namespace);
    let store = LocalStorageStore::new(&Key::new(), namespace, Some(4000)).expect("store opens");

    let small = key("d/small.txt");
    store.set(&small, b"ok", &Metadata::new()).await.expect("small write fits");
    let used_before = store.used_bytes();

    let big = key("d/big.bin");
    match store.set(&big, &vec![b'x'; 5000], &Metadata::new()).await {
        Ok(()) => panic!("a write past the quota must be refused"),
        Err(e) => assert_eq!(e.error_type, ErrorType::KeyWriteError, "{}", e.message),
    }

    assert!(
        !store.contains(&big).await.expect("contains"),
        "a refused write must leave nothing behind"
    );
    let (data, _) = store.get(&small).await.expect("the earlier value is intact");
    assert_eq!(data, b"ok");
    assert_eq!(
        store.used_bytes(),
        used_before,
        "a refused write must not consume a single byte — no orphan metadata entry"
    );

    // And the refusal is not permanent: something that fits still writes.
    store
        .set(&key("d/other.txt"), b"fine", &Metadata::new())
        .await
        .expect("a write that fits must still succeed after a refusal");
}

/// An unlimited store (the default) accepts a payload a quota would have refused.
///
/// The negative half of the quota test: without it, a store that refused every write would still
/// pass the test above.
#[wasm_bindgen_test]
async fn quota_absent_means_unlimited() {
    let store = store("t_noquota");
    let k = key("d/big.bin");
    store
        .set(&k, &vec![b'x'; 5000], &Metadata::new())
        .await
        .expect("with no quota the write must succeed");
    let (data, _) = store.get(&k).await.expect("get");
    assert_eq!(data.len(), 5000);
}

/// A write the *browser* refuses part-way through leaves the previous value intact.
///
/// This is the failure the configured budget cannot catch. `set` writes two entries, metadata
/// first; if the data write then throws `QuotaExceededError`, without a rollback the store is left
/// holding the **old data beside the new metadata** — a value silently described by metadata for a
/// different one. `contains` still says true, `used_bytes` still looks plausible, and only reading
/// both halves reveals it.
///
/// No configured quota here, deliberately: the point is the browser's own limit. The payload is
/// far past any implementation's few-megabyte cap, so the refusal is deterministic.
#[wasm_bindgen_test]
async fn browser_quota_failure_rolls_back_the_whole_write() {
    let store = store("t_rollback");
    let k = key("d/doc.txt");

    store
        .set(&k, b"original", &Metadata::new())
        .await
        .expect("the first write fits");
    let used_before = store.used_bytes();

    // ~40 MB of text: past every browser's localStorage limit.
    let oversized = vec![b'x'; 40 * 1024 * 1024];
    match store.set(&k, &oversized, &Metadata::new()).await {
        Ok(()) => panic!("a 40 MB value must not fit in localStorage"),
        Err(e) => assert_eq!(e.error_type, ErrorType::KeyWriteError, "{}", e.message),
    }

    // Both halves are the originals. `file_size` is the discriminator, deliberately: `set`
    // finalizes metadata from the key, so `media_type` is re-derived from the `.txt` extension and
    // reads `text/plain` for *either* version — an assertion on it would pass whatever happened.
    // The recorded size is the field that actually differs between the two writes.
    let (data, metadata) = store.get(&k).await.expect("the original value survives");
    assert_eq!(data, b"original", "the data must not have been replaced");
    assert_eq!(
        metadata.file_size(),
        Some(8),
        "the metadata must describe the *stored* value — this is the half written first, so \
         without a rollback it describes the 40 MB one that was never written"
    );
    assert_eq!(
        store.used_bytes(),
        used_before,
        "a rolled-back write must not leave bytes behind"
    );
}
