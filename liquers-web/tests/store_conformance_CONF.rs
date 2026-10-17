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
//! `C9` needs a real browser, because `localStorage` does not exist under Node, so it lives in
//! `store_conformance_browser_CONF.rs`, which configures `run_in_browser` and sits behind
//! `browser-tests` — one such file in the default set would make the whole light loop demand a
//! chromedriver. It used to be here, gated on the feature but without `run_in_browser`, so it ran
//! under Node and panicked before checking anything (LOCAL-STORAGE-STORE-FAILS-CONFORMANCE-IN-A-BROWSER).

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
                // `null` is the protocol's "absent"; throwing would say the read failed.
                // A metadata-only entry has no data object, so it is absent here too (`sidecar05`).
                if (!entry || entry.data === null) { return null; }
                return { data: entry.data, metadata: entry.metadata };
            },
            getMetadata(key) {
                const entry = data.get(key);
                return entry ? entry.metadata : null;
            },
            set(key, value, metadata) { data.set(key, { data: value, metadata: metadata }); },
            setMetadata(key, metadata) {
                const entry = data.get(key) || { data: null, metadata: {} };
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

    // No allowed failures: the stub uses the `null` sentinel for absence, and `JsStore` gives a
    // directory directory-shaped metadata (WEB-JS-STORE-CANNOT-EXPRESS-KEY-NOT-FOUND,
    // WEB-JS-STORE-HAS-NO-DIRECTORY-METADATA).
    report(run_all(&fixture).await);
}

/// Install a `fetch` serving `files` (URL → text body) with `ETag`, `Last-Modified` and
/// `Content-Length` headers, answering `HEAD` without a body and anything unlisted with 404.
fn install_header_fetch(files: &[(&str, String)]) {
    let map = js_sys::Object::new();
    for (url, body) in files {
        js_sys::Reflect::set(&map, &JsValue::from_str(url), &JsValue::from_str(body))
            .expect("map entry");
    }
    let make = js_sys::Function::new_with_args(
        "files",
        r#"
        return async (url, options) => {
          const body = files[url];
          if (body === undefined) return new Response(null, { status: 404 });
          const headers = {
            'Content-Length': String(new TextEncoder().encode(body).length),
            'ETag': '"v1"',
            'Last-Modified': 'Wed, 21 Oct 2015 07:28:00 GMT',
            'Content-Type': 'text/plain',
          };
          const isHead = options && options.method === 'HEAD';
          return new Response(isHead ? null : body, { status: 200, headers });
        };
        "#,
    );
    let fetch = make.call1(&JsValue::NULL, &map).expect("build fetch");
    js_sys::Reflect::set(&js_sys::global(), &"fetch".into(), &fetch).expect("install fetch");
}

/// `FetchStore`'s data-only access is the read half: bytes, raw names, facts from the response
/// headers, its header-derived record as the stored one — and every data write refused
/// (WEB-BUILT-IN-STORES-HAVE-NO-DATA-ACCESS).
#[wasm_bindgen_test]
async fn fetch_store_data_only_read_half() {
    use liquers_core::error::ErrorType;
    use liquers_core::store::AsyncStore;
    install_header_fetch(&[("https://h.invalid/a.txt", "hello".to_string())]);
    let prefix = parse_key("web").expect("prefix");
    let a = parse_key("web/a.txt").expect("key");
    let sidecar = parse_key("web/a.txt.__metadata__").expect("key");
    let store = FetchStore::new(&prefix, "https://h.invalid/", vec![a.clone(), sidecar.clone()])
        .expect("fetch store");

    let caps = store.data_capabilities();
    assert!(caps.data_access);
    assert!(!caps.atomic_write && !caps.rename && !caps.modified_time && !caps.etag);

    assert_eq!(store.get_data(&a).await.expect("get_data"), b"hello".to_vec());
    let mut names = store.listdir_data(&prefix).await.expect("listdir_data");
    names.sort();
    assert_eq!(names, vec!["a.txt", "a.txt.__metadata__"]);

    let facts = store.data_facts(&a).await.expect("facts").expect("present");
    assert_eq!(facts.size, 5);
    assert_eq!(facts.etag.as_deref(), Some("\"v1\""));
    assert_eq!(
        facts.modified.as_ref().map(|t| t.as_str().to_string()),
        Some("2015-10-21T07:28:00Z".to_string())
    );
    // Listed but not served: absent, not an error.
    assert_eq!(store.data_facts(&sidecar).await.expect("facts"), None);
    // Unlisted: absent, and no request is made for it.
    assert_eq!(store.data_facts(&parse_key("web/other").expect("key")).await.expect("facts"), None);
    assert_eq!(store.data_facts(&prefix).await.expect("facts"), None, "a directory has no facts");

    let stored = store.get_stored_metadata(&a).await.expect("stored").expect("a record");
    assert_eq!(stored.get_media_type(), "text/plain");
    assert_eq!(store.get_stored_metadata(&sidecar).await.expect("stored"), None);

    for result in [
        store.set_data(&a, b"x").await,
        store.remove_data(&a).await,
        store.rename_data(&a, &sidecar).await,
    ] {
        match result {
            Ok(()) => panic!("a data write must be refused"),
            Err(e) => assert_eq!(e.error_type, ErrorType::NotSupported, "{}", e.message),
        }
    }
}

/// An `http` mount takes a `metadata` section: here `grouped`, with the folder's record file
/// served by the host as one more listed key, under `read_only: true`.
#[wasm_bindgen_test]
async fn fetch_store_under_grouped_metadata_section() {
    use liquers_core::metadata::{Metadata, MetadataRecord};
    use liquers_core::store_config::StoreRouterConfig;
    use liquers_web::store::{build_store, WebStoreFactory};

    let mut record = MetadataRecord::new();
    record.title = "From the host".to_string();
    let record_json = Metadata::MetadataRecord(record).to_json().expect("json");
    install_header_fetch(&[
        ("https://h.invalid/a.txt", "hello".to_string()),
        ("https://h.invalid/__metadata__.json", format!("{{\"a.txt\": {record_json}}}")),
    ]);
    let yaml = "stores:\n  - type: http\n    prefix: web\n    config:\n      url_prefix: https://h.invalid/\n      keys: [a.txt, __metadata__.json]\n    read_only: true\n    metadata: { storage: grouped }\n";
    let config = StoreRouterConfig::from_yaml(yaml).expect("config parses");
    let (registry, functions) = liquers_core::store::realm::StoreConfigFunctions::builtin();
    let store = build_store(&config, WebStoreFactory::new(), registry, functions)
        .unwrap_or_else(|e| panic!("an http mount must take a metadata section: {}", e.message));

    let a = parse_key("web/a.txt").expect("key");
    let metadata = store.get_metadata(&a).await.expect("metadata");
    assert_eq!(metadata.title(), "From the host");
    let (data, _) = store.get(&a).await.expect("get");
    assert_eq!(data, b"hello".to_vec());
    // The record file is the storage model's reserved name, so the listing drops it.
    assert_eq!(
        store.listdir(&parse_key("web").expect("key")).await.expect("listdir"),
        vec!["a.txt".to_string()]
    );
}
