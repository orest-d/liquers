//! `C9` and `C11` — the conformance suite against `LocalStorageStore` (plain, and under a `metadata`
//! section), in a real browser.
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

/// Removes every `localStorage` entry whose name starts with `namespace`.
fn clear_namespace(namespace: &str) {
    let storage = web_sys::window()
        .expect("a browser window")
        .local_storage()
        .expect("localStorage is accessible")
        .expect("localStorage is present");
    let mut doomed = Vec::new();
    for i in 0..storage.length().unwrap_or(0) {
        if let Ok(Some(k)) = storage.key(i) {
            if k.starts_with(namespace) {
                doomed.push(k);
            }
        }
    }
    for k in doomed {
        let _ = storage.remove_item(&k);
    }
}

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

/// `C11` — `LocalStorageStore` under a `metadata` section, `per_entry` and `grouped`, built from
/// configuration the way a page builds it. The counterpart of core's `A3` / `A4` over memory:
/// the pipeline keeps its records as ordinary data objects through the store's data-only methods
/// (WEB-BUILT-IN-STORES-HAVE-NO-DATA-ACCESS).
///
/// ```text
/// CHROMEDRIVER=$(which chromedriver) cargo test -p liquers-web \
///   --target wasm32-unknown-unknown --features browser-tests --test store_conformance_browser_CONF
/// ```
#[wasm_bindgen_test]
async fn c11_local_storage_store_under_metadata_sections() {
    use liquers_core::parse::parse_key;
    use liquers_core::store::SharedStore;
    use liquers_core::store_config::StoreRouterConfig;
    use liquers_web::store::{build_store, WebStoreFactory};

    for storage in ["per_entry", "grouped"] {
        let namespace = format!("lqconf_{storage}");
        clear_namespace(&namespace);
        let yaml = format!(
            "stores:\n  - type: localstorage\n    prefix: ls\n    config: {{ namespace: {namespace} }}\n    metadata: {{ storage: {storage} }}\n"
        );
        let config = StoreRouterConfig::from_yaml(&yaml).expect("config parses");
        let (registry, functions) = liquers_core::store::realm::StoreConfigFunctions::builtin();
        let store = build_store(&config, WebStoreFactory::new(), registry, functions)
            .expect("a localstorage mount takes a metadata section");

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
            &format!("MetadataPipelineStore(LocalStorageStore, {storage})"),
            Box::new(SharedStore(store)),
            Key::new(),
            capabilities,
            SafetyLevel::Scratch,
        )
        .with_key_base(parse_key("ls").expect("prefix"))
        .with_run_id(format!("lqrun{:x}", js_sys::Date::now() as u64));

        report(run_all(&fixture).await);
    }
}
