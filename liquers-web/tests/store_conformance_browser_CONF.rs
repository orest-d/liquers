//! `C9` — the conformance suite against `LocalStorageStore`, in a real browser.
//!
//! `localStorage` does not exist under Node, so this file configures `run_in_browser` and is gated
//! on `browser-tests`, like `store_local_STORE.rs`. Keeping it out of `store_conformance_CONF.rs`
//! keeps the routine Node loop free of any WebDriver dependency:
//!
//! ```text
//! CHROMEDRIVER=$(which chromedriver) cargo test -p liquers-web \
//!   --target wasm32-unknown-unknown --features browser-tests --test store_conformance_browser_CONF
//! ```

#![cfg(all(target_arch = "wasm32", feature = "browser-tests"))]

use liquers_core::query::Key;
use liquers_core::store_conformance::{
    run_all, ConformanceReport, GenericFixture, SafetyLevel, StoreCapabilities,
};
use wasm_bindgen::prelude::*;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

fn report(report: ConformanceReport) {
    // Printed before asserting: a wasm test failure is otherwise a rule id with no context.
    web_sys::console::log_1(&JsValue::from_str(&format!("{report}")));
    if let Err(e) = report.assert_conformant(&[]) {
        panic!("{}", e.message);
    }
}

/// `C9` — `LocalStorageStore`.
///
/// `localStorage` does not exist under Node — `web_sys::window()` returns `None` — so this needs a
/// real browser and a chromedriver whose major version matches it:
///
/// ```text
/// CHROMEDRIVER=$(which chromedriver) cargo test -p liquers-web \
///   --target wasm32-unknown-unknown --features browser-tests --test store_conformance_browser_CONF
/// ```
///
/// **`with_run_id` matters here and nowhere else in tree.** `localStorage` persists across tests in
/// one browser session, so a fixture using the default in-process stem passes the first time and
/// meets its own leftovers the second — the failure `store_local_STORE.rs` already documents. The
/// namespace is cleared first for the same reason.
#[wasm_bindgen_test]
async fn c9_local_storage_store() {
    use liquers_web::store::LocalStorageStore;

    const NAMESPACE: &str = "lqconf";
    let storage = web_sys::window()
        .expect("a browser window")
        .local_storage()
        .expect("localStorage is accessible")
        .expect("localStorage is present");
    // Clear the namespace: this store outlives the test that wrote it.
    let mut doomed = Vec::new();
    for i in 0..storage.length().unwrap_or(0) {
        if let Ok(Some(k)) = storage.key(i) {
            if k.starts_with(NAMESPACE) {
                doomed.push(k);
            }
        }
    }
    for k in doomed {
        let _ = storage.remove_item(&k);
    }

    let prefix = Key::new();
    let store = LocalStorageStore::new(&prefix, NAMESPACE, None).expect("local storage store");

    let capabilities = StoreCapabilities {
        write: true,
        remove: true,
        directories: true,
        derived_directories: true,
        explicit_directories: true,
        remove_directories: true,
        stored_metadata: true,
        enumerate_keys: true,
    };

    let fixture = GenericFixture::new(
        "LocalStorageStore",
        Box::new(store),
        prefix,
        capabilities,
        SafetyLevel::Scratch,
    )
    .with_run_id(format!("lqrun{:x}", js_sys::Date::now() as u64));

    report(run_all(&fixture).await);
}
