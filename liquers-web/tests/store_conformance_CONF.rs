//! `C8`–`C10` — the conformance suite under `wasm32`.
//!
//! The reason this project is an `L`: the same rules must run in a browser realm as well as
//! natively, so the suite carries no test attribute of its own and each crate supplies its harness.
//! Here that is `#[wasm_bindgen_test]`.
//!
//! `C8` and `C10` run under **Node**, in the routine loop:
//!
//! ```text
//! cargo test -p liquers-web --target wasm32-unknown-unknown
//! ```
//!
//! `C9` needs a real browser, because `localStorage` does not exist under Node, so it sits behind
//! `browser-tests` with the rest of the WebDriver-dependent files — one such file in the default
//! set would make the whole light loop demand a chromedriver.

#![cfg(target_arch = "wasm32")]

use liquers_core::parse::parse_key;
use liquers_core::query::Key;
use liquers_core::store_conformance::{
    run_all, ConformanceReport, GenericFixture, SafetyLevel, StoreCapabilities,
};
use liquers_web::store::{FetchStore, JsStore};
use wasm_bindgen::prelude::*;
use wasm_bindgen_test::*;

fn nothing() -> StoreCapabilities {
    StoreCapabilities {
        write: false,
        remove: false,
        directories: false,
        derived_directories: false,
        explicit_directories: false,
        remove_directories: false,
        stored_metadata: false,
        enumerate_keys: false,
    }
}

fn report(report: ConformanceReport) {
    // Printed before asserting: a wasm test failure is otherwise a rule id with no context.
    web_sys::console::log_1(&JsValue::from_str(&format!("{report}")));
    if let Err(e) = report.assert_conformant(&[]) {
        panic!("{}", e.message);
    }
}

/// Install a `fetch` on the global object that serves an in-memory corpus.
///
/// `FetchStore` reads `fetch` from [`js_sys::global`] at call time rather than from
/// `web_sys::Window`, which is what makes this possible: no HTTP server, no WebDriver, and the
/// suite runs in the fast Node loop rather than behind `browser-tests`.
fn install_stub_fetch(body: &'static str) {
    let handler = Closure::<dyn Fn(JsValue) -> js_sys::Promise>::new(move |_url: JsValue| {
        let init = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&init, &"status".into(), &JsValue::from_f64(200.0));
        let response = web_sys::Response::new_with_opt_str(Some(body)).expect("response");
        js_sys::Promise::resolve(&JsValue::from(response))
    });
    let global = js_sys::global();
    js_sys::Reflect::set(&global, &"fetch".into(), handler.as_ref().unchecked_ref())
        .expect("install fetch");
    // Deliberately leaked: the closure must outlive this call, and the process is a test runner.
    handler.forget();
}

/// `C8` — `FetchStore`, read-only over a stubbed `fetch`.
///
/// Its configured key set is the source for `KeyRequest::Existing`, which is the only way a
/// read-only store has any subject at all: the fixture cannot create one.
#[wasm_bindgen_test]
async fn c8_fetch_store() {
    install_stub_fetch("conformance body");

    let prefix = parse_key("web").expect("prefix");
    let known = parse_key("web/known.txt").expect("key");
    let store = FetchStore::new(&prefix, "https://example.invalid/", vec![known.clone()])
        .expect("fetch store");

    let mut capabilities = nothing();
    // It answers `is_dir`/`listdir` from the configured key set, so it does have directories —
    // derived ones, though nothing can remove a child to retire them.
    capabilities.directories = true;

    let fixture = GenericFixture::new(
        "FetchStore(stub fetch)",
        Box::new(store),
        prefix,
        capabilities,
        SafetyLevel::ReadOnly,
    )
    .with_existing(known.clone())
    // Its key space is a configured *set*, not a shape, so an invented name is legitimately
    // unsupported: `prefix04` has to be given a key the store actually knows.
    .with_supported(known)
    .with_outside_prefix(parse_key("elsewhere/x.txt").expect("key"));

    report(run_all(&fixture).await);
}

/// A JavaScript object implementing **every** method `JsStore` forwards.
///
/// A partial stub would report capability gaps belonging to the stub rather than to `JsStore`,
/// which is the trap in testing a delegating store: the report looks like findings and is nothing
/// of the sort. The first draft here *was* partial — no `listdir`, and `get` returning the raw
/// value rather than `{data, metadata}` — and produced five failures and two errors that said
/// nothing about `JsStore` at all.
///
/// The protocol is the one documented at the top of `liquers-web/src/store/js_store.rs`: keys
/// cross as strings, data as `Uint8Array`, metadata as a plain object.
///
/// Note `removedir`, which deletes by `key + "/"` rather than by `key`. Deleting by bare prefix is
/// the defect `sibling01` exists for, and it would be as easy to write here as it was in the
/// OpenDAL store.
fn stub_js_object() -> js_sys::Object {
    let source = r#"(function () {
        const data = new Map();
        const dirs = new Set();
        function children(key) {
            const p = key === "" ? "" : key + "/";
            const out = new Set();
            const consider = (k) => {
                if (k.startsWith(p) && k.length > p.length) {
                    const rest = k.slice(p.length);
                    const i = rest.indexOf("/");
                    out.add(i === -1 ? rest : rest.slice(0, i));
                }
            };
            for (const k of data.keys()) consider(k);
            for (const d of dirs) consider(d);
            return Array.from(out);
        }
        return {
            get(key) {
                const entry = data.get(key);
                if (!entry) { throw new Error("not found: " + key); }
                return { data: entry.data, metadata: entry.metadata };
            },
            set(key, value, metadata) { data.set(key, { data: value, metadata: metadata }); },
            setMetadata(key, metadata) {
                const entry = data.get(key) || { data: new Uint8Array(), metadata: {} };
                entry.metadata = metadata;
                data.set(key, entry);
            },
            remove(key) { data.delete(key); },
            removedir(key) {
                const p = key === "" ? "" : key + "/";
                for (const k of Array.from(data.keys())) { if (k.startsWith(p)) data.delete(k); }
                for (const d of Array.from(dirs)) { if (d === key || d.startsWith(p)) dirs.delete(d); }
            },
            makedir(key) { dirs.add(key); },
            contains(key) { return data.has(key) || dirs.has(key) || children(key).length > 0; },
            isDir(key) { return dirs.has(key) || children(key).length > 0; },
            listdir(key) { return children(key); },
        };
    })()"#;
    js_sys::eval(source)
        .expect("stub store object")
        .dyn_into::<js_sys::Object>()
        .expect("object")
}

/// `C10` — `JsStore` over a stub object.
#[wasm_bindgen_test]
async fn c10_js_store() {
    let prefix = Key::new();
    let store = JsStore::new(&prefix, "stub", stub_js_object()).expect("js store");

    let capabilities = StoreCapabilities {
        write: true,
        remove: true,
        directories: true,
        // The stub derives directories from its keys, so one retires with its last child.
        derived_directories: true,
        explicit_directories: true,
        remove_directories: true,
        stored_metadata: true,
        enumerate_keys: true,
    };

    let fixture = GenericFixture::new(
        "JsStore(stub object)",
        Box::new(store),
        prefix,
        capabilities,
        SafetyLevel::Scratch,
    );

    let result = run_all(&fixture).await;
    web_sys::console::log_1(&JsValue::from_str(&format!("{result}")));
    if let Err(e) = result.assert_conformant(&[
        // The JS protocol has no way to say "not found": a delegate signals every failure by
        // throwing, and `JsStore` maps a thrown error to `KeyReadError`. §4 makes absence and
        // failure different answers, so these two cannot pass until the protocol can express it.
        // WEB-JS-STORE-CANNOT-EXPRESS-KEY-NOT-FOUND.
        liquers_core::store_conformance::AllowedFailure {
            rule: "absence01",
            issue: "WEB-JS-STORE-CANNOT-EXPRESS-KEY-NOT-FOUND",
        },
        liquers_core::store_conformance::AllowedFailure {
            rule: "remove03",
            issue: "WEB-JS-STORE-CANNOT-EXPRESS-KEY-NOT-FOUND",
        },
        // `get_metadata` on a directory key falls through to `get`, which throws because a
        // directory has no data. §2 requires `default_metadata(key, true)` instead.
        // WEB-JS-STORE-HAS-NO-DIRECTORY-METADATA.
        liquers_core::store_conformance::AllowedFailure {
            rule: "dir04",
            issue: "WEB-JS-STORE-HAS-NO-DIRECTORY-METADATA",
        },
        liquers_core::store_conformance::AllowedFailure {
            rule: "dir07",
            issue: "WEB-JS-STORE-HAS-NO-DIRECTORY-METADATA",
        },
    ]) {
        panic!("{}", e.message);
    }
}

/// `C9` — `LocalStorageStore`, behind `browser-tests`.
///
/// `localStorage` does not exist under Node — `web_sys::window()` returns `None` — so this needs a
/// real browser and a chromedriver whose major version matches it:
///
/// ```text
/// CHROMEDRIVER=$(which chromedriver) cargo test -p liquers-web \
///   --target wasm32-unknown-unknown --features browser-tests --test store_conformance_CONF
/// ```
///
/// It is gated off by default because one `run_in_browser` file in the default set would make the
/// whole light Node loop demand a WebDriver.
///
/// **`with_run_id` matters here and nowhere else in tree.** `localStorage` persists across tests in
/// one browser session, so a fixture using the default in-process stem passes the first time and
/// meets its own leftovers the second — the failure `store_local_STORE.rs` already documents. The
/// namespace is cleared first for the same reason.
#[cfg(feature = "browser-tests")]
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
