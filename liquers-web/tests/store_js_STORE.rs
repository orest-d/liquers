//! `STORE01`–`STORE06` for [`JsStore`], plus `STORE09` (read-only refusal) and `STORE11`
//! (configuration-driven routing).
//!
//! Test IDs and names follow `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` §3. The inventory is in
//! `specs/design/liquers-web-store/phase3-examples.md`, test groups 4 and 6.
//!
//! Pure ECMAScript — the fixture stores are plain objects with no DOM — so these run under Node
//! with no WebDriver:
//!
//! ```text
//! cargo test -p liquers-web --target wasm32-unknown-unknown --test store_js_STORE
//! ```

#![cfg(target_arch = "wasm32")]

use liquers_core::error::ErrorType;
use liquers_core::metadata::Metadata;
use liquers_core::parse::parse_key;
use liquers_core::query::Key;
use liquers_core::store::{AsyncStore, AsyncStoreRouter};
use liquers_core::store_config::StoreRouterConfig;
use liquers_web::store::{build_router, build_store, JsStore, WebStoreFactory};
use wasm_bindgen::prelude::*;
use wasm_bindgen_test::*;

fn key(text: &str) -> Key {
    parse_key(text).expect("fixture key must parse")
}

/// Evaluates a JavaScript expression and returns it as an object.
fn object(source: &str) -> js_sys::Object {
    js_sys::eval(source)
        .expect("fixture source must evaluate")
        .dyn_into::<js_sys::Object>()
        .expect("fixture must be an object")
}

/// A complete, writable store in JavaScript, backed by a `Map`.
///
/// Deliberately mixes `async` and plain methods: `isDir` and `isSupported` are synchronous, which
/// the adapter must accept alongside the Promise-returning ones.
const FULL_STORE: &str = r#"
(() => {
  const data = new Map();
  const dirs = () => {
    const d = new Map();
    for (const k of data.keys()) {
      const parts = k.split('/');
      for (let i = 0; i < parts.length; i++) {
        const parent = parts.slice(0, i).join('/');
        if (!d.has(parent)) d.set(parent, new Set());
        d.get(parent).add(parts[i]);
      }
    }
    return d;
  };
  return {
    _data: data,
    async get(key) {
      if (!data.has(key)) return undefined;      // absence is `undefined`, not a throw
      return data.get(key);
    },
    async set(key, bytes, metadata) { data.set(key, { data: bytes, metadata }); },
    async setMetadata(key, metadata) {
      const rec = data.get(key);
      if (rec) rec.metadata = metadata;
    },
    async remove(key) { data.delete(key); },
    async removedir(key) {
      for (const k of [...data.keys()]) {
        if (k === key || k.startsWith(key + '/')) data.delete(k);
      }
    },
    async contains(key) { return data.has(key) || dirs().has(key); },
    isDir(key) { return dirs().has(key); },      // synchronous on purpose
    async listdir(key) { return [...(dirs().get(key) ?? [])]; },
    async makedir(_key) { /* directories are implicit here */ },
    isSupported(_key) { return true; },          // synchronous on purpose
  };
})()
"#;

/// A store with `get` and nothing else: the minimum the protocol allows.
const READ_ONLY_STORE: &str = r#"
(() => {
  const data = new Map([['ro/fixed.txt', {
    data: new Uint8Array([104, 105]),
    metadata: { media_type: 'text/plain' },
  }]]);
  return { async get(key) { return data.get(key); } };
})()
"#;

fn full_store(prefix: &str) -> JsStore {
    JsStore::new(&key(prefix), "full", object(FULL_STORE)).expect("full store adapts")
}

/// STORE01 — data and metadata cross the boundary together.
#[wasm_bindgen_test]
async fn store01_set_get_data_and_metadata() {
    let store = full_store("d");
    let k = key("d/f.txt");
    let mut record = liquers_core::metadata::MetadataRecord::new();
    record.with_media_type("text/plain".to_string());

    store
        .set(&k, b"data", &Metadata::MetadataRecord(record))
        .await
        .expect("set");

    let (data, metadata) = store.get(&k).await.expect("get");
    assert_eq!(data, b"data");
    assert_eq!(metadata.get_media_type(), "text/plain");
}

/// STORE01b — bytes survive the `Uint8Array` round trip, including non-UTF-8.
#[wasm_bindgen_test]
async fn store01_binary_round_trips() {
    let store = full_store("d");
    let k = key("d/blob.bin");
    let bytes: Vec<u8> = vec![0x00, 0xFF, 0xFE, 0x89, 0x50];
    store.set(&k, &bytes, &Metadata::new()).await.expect("set");
    let (back, _) = store.get(&k).await.expect("get");
    assert_eq!(back, bytes);
}

/// STORE02 — an absent key is `KeyNotFound`, and a thrown error is not a panic.
///
/// Two paths, because a page has two ways to signal trouble. Returning `undefined` is the
/// documented way to say "absent"; a thrown `Error` carries no Liquers error type, so it becomes a
/// read error rather than being mistaken for absence.
#[wasm_bindgen_test]
async fn store02_missing_key_error() {
    let store = full_store("d");
    match store.get(&key("d/absent")).await {
        Ok(_) => panic!("an absent key must not resolve"),
        Err(e) => assert_eq!(e.error_type, ErrorType::KeyNotFound, "{}", e.message),
    }

    let thrower = JsStore::new(
        &key("d"),
        "thrower",
        object("({ get(_k) { throw new Error('boom'); } })"),
    )
    .expect("adapts");
    match thrower.get(&key("d/x")).await {
        Ok(_) => panic!("a throwing store must not resolve"),
        Err(e) => {
            assert_eq!(e.error_type, ErrorType::KeyReadError, "{}", e.message);
            assert!(
                e.message.contains("boom"),
                "the thrown text is kept: {}",
                e.message
            );
        }
    }
}

/// STORE03 — listing, `contains` and `is_dir` agree.
#[wasm_bindgen_test]
async fn store03_directory_listing_invariants() {
    let store = full_store("d");
    store
        .set(&key("d/a"), b"1", &Metadata::new())
        .await
        .expect("set a");
    store
        .set(&key("d/b"), b"2", &Metadata::new())
        .await
        .expect("set b");

    let mut names = store.listdir(&key("d")).await.expect("listdir");
    names.sort();
    assert_eq!(names, vec!["a", "b"]);

    for name in &names {
        assert!(
            store
                .contains(&key("d").join(name))
                .await
                .expect("contains"),
            "{name} is listed and must be contained"
        );
    }
    assert!(store.is_dir(&key("d")).await.expect("is_dir"));
    assert!(!store.is_dir(&key("d/a")).await.expect("is_dir"));
}

/// STORE04 — remove and removedir reach through to the page's store.
#[wasm_bindgen_test]
async fn store04_remove_and_removedir() {
    let store = full_store("d");
    let k = key("d/x");
    store.set(&k, b"1", &Metadata::new()).await.expect("set");
    store.remove(&k).await.expect("remove");
    assert!(!store.contains(&k).await.expect("contains"));

    store
        .set(&key("d/y"), b"2", &Metadata::new())
        .await
        .expect("set y");
    store.removedir(&key("d")).await.expect("removedir");
    assert!(!store.contains(&key("d/y")).await.expect("contains y"));
}

/// STORE05 — an escaping key is refused before the page object is ever called.
#[wasm_bindgen_test]
async fn store05_unsupported_key() {
    let store = full_store("d");
    for text in ["../escape", "d/../../etc", "d/./b"] {
        let k = key(text);
        match store.get(&k).await {
            Ok(_) => panic!("{text} must be refused"),
            Err(e) => assert_eq!(e.error_type, ErrorType::KeyNotAbsolute, "{}", e.message),
        }
        assert!(!store.is_supported(&k), "{text} must not be routed here");
    }
}

/// STORE05b — traversal stays refused after `makedir`, and dotted names stay ordinary.
///
/// A `JsStore` has no real directories, so the trap the blueprint describes (a filesystem resolving
/// `..` once the directory exists) cannot occur here. What this pins is that the guard runs before
/// the page object is called, whatever the object has been asked to create.
#[wasm_bindgen_test]
async fn store05b_traversal_after_makedir_is_refused() {
    let page_object = object(FULL_STORE);
    let store = JsStore::new(&key("d"), "full", page_object.clone()).expect("adapts");
    store.makedir(&key("d/a")).await.expect("makedir");

    let escaping = key("d/a/../../etc");
    match store.get(&escaping).await {
        Ok(_) => panic!("traversal after makedir must be refused"),
        Err(e) => assert_eq!(e.error_type, ErrorType::KeyNotAbsolute, "{}", e.message),
    }
    match store.set(&escaping, b"x", &Metadata::new()).await {
        Ok(()) => panic!("traversal after makedir must be refused for writes too"),
        Err(e) => assert_eq!(e.error_type, ErrorType::KeyNotAbsolute, "{}", e.message),
    }
    let data: js_sys::Map = js_sys::Reflect::get(&page_object, &JsValue::from_str("_data"))
        .expect("_data")
        .dyn_into()
        .expect("a Map");
    assert_eq!(
        data.size(),
        0,
        "the refused write must not reach the page object"
    );

    for text in ["d/.hidden", "d/a..b", "d/...", "d/..x", "d/v1.2.3"] {
        assert!(store.is_supported(&key(text)), "{text} is an ordinary name");
    }
}

/// STORE06 — concurrent writes to one key leave the last value, intact.
#[wasm_bindgen_test]
async fn store06_concurrent_update_policy() {
    let store = full_store("d");
    let k = key("d/race");
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
    assert_eq!(data, vec![9u8]);
}

/// STORE09 — a store missing the write half refuses, and the router does not fall through.
///
/// The second assertion is the one that matters. A router that retried the next matching store on
/// failure would make a read-only prefix silently writable *somewhere else*, and the refusal alone
/// would not reveal it.
#[wasm_bindgen_test]
async fn store09_read_only_refuses_and_router_does_not_fall_through() {
    let read_only = JsStore::new(&key("ro"), "ro", object(READ_ONLY_STORE)).expect("adapts");
    let writable = full_store("rw");

    let mut router = AsyncStoreRouter::new();
    router.add_store(Box::new(read_only));
    router.add_store(Box::new(writable));

    // A read through the router reaches the read-only store.
    let (data, _) = router
        .get(&key("ro/fixed.txt"))
        .await
        .expect("read-only get");
    assert_eq!(data, b"hi");

    match router.set(&key("ro/new.txt"), b"x", &Metadata::new()).await {
        Ok(()) => panic!("a read-only store must refuse writes"),
        Err(e) => assert_eq!(e.error_type, ErrorType::KeyNotSupported, "{}", e.message),
    }

    // Nothing leaked into the writable store at the other prefix.
    assert!(
        !router.contains(&key("rw/new.txt")).await.expect("contains"),
        "the refused write must not have landed in another store"
    );
    // And the writable prefix still works, so the refusal was about the store, not the router.
    router
        .set(&key("rw/ok.txt"), b"y", &Metadata::new())
        .await
        .expect("the writable prefix must still accept writes");
}

/// STORE11 — a router built from a configuration document routes by prefix, first match wins.
#[wasm_bindgen_test]
async fn store11_configuration_routes_by_prefix() {
    let mut factory = WebStoreFactory::new();
    factory.register_object("first", object(FULL_STORE));
    factory.register_object("second", object(FULL_STORE));

    let config: StoreRouterConfig = StoreRouterConfig::from_yaml(
        r#"
stores:
  - type: js
    prefix: a
    config: { object: first }
  - type: js
    prefix: b
    config: { object: second }
"#,
    )
    .expect("config parses");

    let router = build_router(&config, factory).expect("router builds");

    router
        .set(&key("a/one.txt"), b"1", &Metadata::new())
        .await
        .expect("write to a");
    assert!(router
        .contains(&key("a/one.txt"))
        .await
        .expect("contains a"));
    assert!(
        !router
            .contains(&key("b/one.txt"))
            .await
            .expect("contains b"),
        "the two prefixes must be separate stores"
    );

    // A key matching no configured prefix is absent, not misrouted.
    match router.get(&key("zzz/nope.txt")).await {
        Ok(_) => panic!("an unmatched prefix must not resolve"),
        Err(e) => assert_eq!(e.error_type, ErrorType::KeyNotFound, "{}", e.message),
    }
}

/// STORE11 — overlapping prefixes follow document order, and the overlap is accepted silently.
///
/// The router takes the first entry whose prefix matches, so `data` listed before `data/scratch`
/// shadows it. liquers-web does not warn about the shadowing (see the README's `## Conformance`);
/// this test pins the routing in both orders.
#[wasm_bindgen_test]
async fn store11_overlapping_prefixes_follow_document_order() {
    async fn holder(first: &str, second: &str) -> (bool, bool) {
        let broad = object(FULL_STORE);
        let narrow = object(FULL_STORE);
        let mut factory = WebStoreFactory::new();
        factory.register_object("broad", broad.clone());
        factory.register_object("narrow", narrow.clone());
        let entry = |prefix: &str| {
            let name = if prefix == "data" { "broad" } else { "narrow" };
            format!("  - type: js\n    prefix: {prefix}\n    config: {{ object: {name} }}\n")
        };
        let yaml = format!("stores:\n{}{}", entry(first), entry(second));
        let config = StoreRouterConfig::from_yaml(&yaml).expect("config parses");
        let router = build_router(&config, factory).expect("router builds");
        router
            .set(&key("data/scratch/x"), b"1", &Metadata::new())
            .await
            .expect("write");
        let has = |o: &js_sys::Object| {
            js_sys::Reflect::get(o, &JsValue::from_str("_data"))
                .expect("_data")
                .dyn_into::<js_sys::Map>()
                .expect("a Map")
                .has(&JsValue::from_str("data/scratch/x"))
        };
        (has(&broad), has(&narrow))
    }

    assert_eq!(
        holder("data", "data/scratch").await,
        (true, false),
        "listed first, `data` must shadow `data/scratch`"
    );
    assert_eq!(
        holder("data/scratch", "data").await,
        (false, true),
        "listed first, `data/scratch` must take its own keys"
    );
}

/// C11 — the protocol is bound at construction, so removing a method afterwards changes nothing.
///
/// Resolving methods lazily would turn a page's later mutation into a `TypeError` from inside an
/// unrelated query.
#[wasm_bindgen_test]
async fn c11_methods_are_bound_at_construction() {
    let page_object = object(FULL_STORE);
    let store = JsStore::new(&key("d"), "full", page_object.clone()).expect("adapts");

    store
        .set(&key("d/before.txt"), b"1", &Metadata::new())
        .await
        .expect("set while the method is present");

    // The page deletes `set` after registration.
    js_sys::Reflect::delete_property(&page_object, &JsValue::from_str("set"))
        .expect("delete succeeds");

    store
        .set(&key("d/after.txt"), b"2", &Metadata::new())
        .await
        .expect("the store keeps the function it resolved at construction");
}

/// STORE12a — `null` means absent, and a directory gets directory-shaped metadata.
#[wasm_bindgen_test]
async fn store12a_absence_sentinel_and_directory_metadata() {
    let source = r#"(function () {
        const data = new Map([
            ["d/file.txt", { data: new Uint8Array([102]), metadata: { title: "a file" } }],
        ]);
        const dirs = new Set(["d", "d/subdir"]);
        return {
            get(key) { return data.has(key) ? data.get(key) : null; },
            getMetadata(key) { return data.has(key) ? data.get(key).metadata : null; },
            isDir(key) { return dirs.has(key); },
            listdir(key) { return key === "d" ? ["file.txt", "subdir"] : []; },
        };
    })()"#;
    let store = JsStore::new(&key("d"), "mixed", object(source)).expect("adapts");

    let file = store.get_metadata(&key("d/file.txt")).await.expect("file metadata");
    let Metadata::MetadataRecord(file) = file else { panic!("record expected") };
    assert_eq!(file.title, "a file");

    let dir = store.get_metadata(&key("d")).await.expect("directory metadata");
    let Metadata::MetadataRecord(dir) = dir else { panic!("record expected") };
    assert!(dir.is_dir);
    let mut names: Vec<String> = dir
        .children
        .iter()
        .filter_map(|i| i.key.as_ref().map(|k| k.encode()))
        .collect();
    names.sort();
    assert_eq!(names, vec!["d/file.txt", "d/subdir"]);

    match store.get_metadata(&key("d/absent")).await {
        Err(e) => assert_eq!(e.error_type, ErrorType::KeyNotFound, "{}", e.message),
        Ok(m) => panic!("absent key gave {m:?}"),
    }
    match store.get(&key("d/absent")).await {
        Err(e) => assert_eq!(e.error_type, ErrorType::KeyNotFound, "{}", e.message),
        Ok(_) => panic!("absent key gave data"),
    }
}

/// STORE12b — a throwing delegate is a failure, and is never reinterpreted as a directory.
#[wasm_bindgen_test]
async fn store12b_thrown_error_is_not_a_directory() {
    let store = JsStore::new(
        &key("d"),
        "thrower",
        object(r#"({ get(key) { throw new Error("read boom"); }, isDir(key) { return true; } })"#),
    )
    .expect("adapts");
    match store.get_metadata(&key("d/x")).await {
        Err(e) => assert_eq!(e.error_type, ErrorType::KeyReadError, "{}", e.message),
        Ok(m) => panic!("a thrown error became {m:?}"),
    }
}

/// ERROR06 — a delegate that throws a page-constructed `LiquersError` reports its type, not the
/// adapter's `KeyReadError` fallback.
#[wasm_bindgen_test]
async fn store_delegate_throwing_a_constructed_error_keeps_its_type() {
    let error = liquers_web::error::LiquersError::construct(
        "key_not_found",
        "no such key: a.txt".to_string(),
        None,
    )
    .expect("constructs");
    let factory = js_sys::Function::new_with_args("err", "return { get(key) { throw err; } };");
    let delegate = factory
        .call1(&JsValue::NULL, &JsValue::from(error))
        .expect("builds the delegate");
    let store = JsStore::new(&key("d"), "typed", delegate.unchecked_into()).expect("adapts");
    match store.get(&key("d/a.txt")).await {
        Err(e) => {
            assert_eq!(e.error_type, ErrorType::KeyNotFound, "{}", e.message);
            assert_eq!(e.message, "no such key: a.txt");
        }
        Ok(_) => panic!("a thrown error gave data"),
    }
}

/// STORE12c — documented break: `getMetadata` returning `null` now means absent, even when
/// `get` has data. It used to yield an empty record.
#[wasm_bindgen_test]
async fn store12c_get_metadata_null_is_not_found() {
    let store = JsStore::new(
        &key("d"),
        "nullmeta",
        object(r#"({
            get(key) { return key === "d/x.bin" ? { data: new Uint8Array([255]) } : null; },
            getMetadata(key) { return null; },
        })"#),
    )
    .expect("adapts");
    match store.get_metadata(&key("d/x.bin")).await {
        Err(e) => assert_eq!(e.error_type, ErrorType::KeyNotFound, "{}", e.message),
        Ok(m) => panic!("getMetadata → null gave {m:?}"),
    }
}

/// C12 — a `js` entry naming an unregistered object fails at configuration time, saying which.
#[wasm_bindgen_test]
fn c12_unregistered_object_fails_with_its_name() {
    let config = StoreRouterConfig::from_yaml(
        "stores:\n  - type: js\n    prefix: x\n    config: { object: never_registered }\n",
    )
    .expect("config parses");

    match build_router(&config, WebStoreFactory::new()) {
        Ok(_) => panic!("a store naming an unregistered object must not build"),
        Err(e) => assert!(
            e.message.contains("never_registered"),
            "the error must name the missing object, got: {}",
            e.message
        ),
    }
}

/// A store without `get` is refused at construction: it is the one method with no fallback.
#[wasm_bindgen_test]
fn js_store_requires_get() {
    match JsStore::new(
        &key("d"),
        "broken",
        object("({ contains(_k) { return false; } })"),
    ) {
        Ok(_) => panic!("a store with no `get` must not adapt"),
        Err(e) => assert!(e.message.contains("get"), "got: {}", e.message),
    }
}

/// An absent optional method errors rather than taking the trait's permissive default.
///
/// `AsyncStore` defaults `listdir` to `[]` and `contains` to `false`. Inheriting those would make
/// a half-written store indistinguishable from an empty one, and `STORE03` would pass vacuously.
#[wasm_bindgen_test]
async fn absent_optional_methods_error_rather_than_default() {
    let store = JsStore::new(&key("ro"), "ro", object(READ_ONLY_STORE)).expect("adapts");
    let k = key("ro/fixed.txt");

    match store.listdir(&key("ro")).await {
        Ok(names) => panic!("listdir must not silently return {names:?}"),
        Err(e) => assert_eq!(e.error_type, ErrorType::KeyNotSupported, "{}", e.message),
    }
    match store.contains(&k).await {
        Ok(answer) => panic!("contains must not silently return {answer}"),
        Err(e) => assert_eq!(e.error_type, ErrorType::KeyNotSupported, "{}", e.message),
    }
    // But `getMetadata` is derived from `get`, because a store that serves data can always answer.
    let metadata = store
        .get_metadata(&k)
        .await
        .expect("metadata is derived from get");
    assert_eq!(metadata.get_media_type(), "text/plain");
}

// ---------------------------------------------------------------------------
// Environment wiring — the store must survive a rebuild.
// ---------------------------------------------------------------------------

mod common;

use liquers_web::environment::{
    configure_store_on, global_store, init_global, register_command_on, register_store_object_on,
    reset_global,
};

fn js_store_config(prefix: &str, object_name: &str) -> StoreRouterConfig {
    StoreRouterConfig::from_yaml(&format!(
        "stores:\n  - type: js\n    prefix: {prefix}\n    config: {{ object: {object_name} }}\n"
    ))
    .expect("config parses")
}

/// The store survives a rebuild triggered by registering a command afterwards.
///
/// **This is the failure mode Phase 4 singled out as the most dangerous in the milestone.**
/// Registering a command after the environment has been shared rebuilds it from scratch, and
/// anything not replayed into the fresh environment is lost silently — a `-R/` query simply stops
/// resolving, with no error at the point the mistake was made. Asserting the store's *name* rather
/// than merely that a store exists is what makes this a real check: a default `NoAsyncStore` is
/// still a store.
#[wasm_bindgen_test]
fn store_survives_a_rebuild() {
    reset_global();
    init_global().expect("init");

    register_store_object_on("mystore", object(FULL_STORE)).expect("register object");
    configure_store_on(js_store_config("d", "mystore")).expect("configure");

    let before = global_store().expect("store before").store_name();
    assert!(
        before.contains("Store router"),
        "the configured store must be the router, got {before:?}"
    );

    // Registering after the environment has been shared forces a rebuild.
    register_command_on(&common::decl("hello", "return 'hi';")).expect("register command");

    let after = global_store().expect("store after rebuild").store_name();
    assert_eq!(
        after, before,
        "the store must survive the rebuild that command registration triggers"
    );

    // And it still routes: the store object registered before the rebuild is still reachable.
    use liquers_core::context::Environment;
    let envref = liquers_web::environment::with_global().expect("global env");
    let store = envref.0.get_async_store();
    assert!(
        store.is_supported(&key("d/x.txt")),
        "the configured prefix must still route after a rebuild"
    );

    reset_global();
}

/// `reset_global` clears the store, so suites cannot leak one into each other.
#[wasm_bindgen_test]
fn reset_clears_the_store() {
    reset_global();
    init_global().expect("init");
    register_store_object_on("mystore", object(FULL_STORE)).expect("register object");
    configure_store_on(js_store_config("d", "mystore")).expect("configure");
    assert!(global_store()
        .expect("store")
        .store_name()
        .contains("Store router"));

    reset_global();
    init_global().expect("re-init");
    let name = global_store().expect("store after reset").store_name();
    assert!(
        !name.contains("Store router"),
        "a reset environment must not keep the previous store, got {name:?}"
    );
    reset_global();
}

/// Registering the object *after* configuring still works: order must not matter.
#[wasm_bindgen_test]
fn object_may_be_registered_after_the_configuration() {
    reset_global();
    init_global().expect("init");

    // Configuring first fails, because the named object does not exist yet...
    assert!(
        configure_store_on(js_store_config("d", "late")).is_err(),
        "a configuration naming an unregistered object must fail loudly"
    );

    // ...and registering the object then re-configuring succeeds.
    register_store_object_on("late", object(FULL_STORE)).expect("register object");
    configure_store_on(js_store_config("d", "late")).expect("configure after registering");
    assert!(global_store()
        .expect("store")
        .store_name()
        .contains("Store router"));

    reset_global();
}

// ---------------------------------------------------------------------------
// Review follow-up (PR #24).
// ---------------------------------------------------------------------------

/// A global `Environment` handle sees a rebuild that happened after it was created.
///
/// The singleton is *replaced*, not mutated. A handle that cached the `EnvRef` it was made with
/// kept evaluating against the environment as it was at `Environment.global()` time, so
/// `configureStore()` followed by `evaluate()` on the **same handle** ran against the store-less
/// predecessor while `store()` returned the configured one — two halves of one object disagreeing.
#[wasm_bindgen_test]
fn global_handle_follows_the_rebuilt_environment() {
    use liquers_web::environment::{with_global, LiquersEnvironment};

    reset_global();
    init_global().expect("init");

    // Taking the handle shares the environment, so the configuration below rebuilds.
    let handle = LiquersEnvironment::global().expect("global handle");
    let before = handle.resolve().expect("resolve before");

    register_store_object_on("mine", object(FULL_STORE)).expect("register object");
    configure_store_on(js_store_config("mem", "mine")).expect("configure");

    let after = handle.resolve().expect("resolve after");
    assert!(
        !std::sync::Arc::ptr_eq(&before.0, &after.0),
        "the configuration must have rebuilt the environment, or this test proves nothing"
    );
    assert!(
        std::sync::Arc::ptr_eq(&after.0, &with_global().expect("global").0),
        "the handle must resolve to the current singleton, not the one it was created from"
    );

    // Both halves of the handle now agree: the store it reports is the configured one.
    assert!(handle
        .store()
        .expect("store")
        .store_name()
        .contains("Store router"));

    reset_global();
}

/// An explicit instance keeps its own environment and is unaffected by the singleton.
#[wasm_bindgen_test]
fn explicit_instance_is_not_the_singleton() {
    use liquers_web::environment::{with_global, LiquersEnvironment};

    reset_global();
    init_global().expect("init");
    let instance = LiquersEnvironment::new().expect("instance");
    let shared = liquers_web::environment::shared_env().expect("shared");
    assert!(!std::sync::Arc::ptr_eq(
        &instance.resolve().expect("resolve").0,
        &shared.0
    ));

    register_store_object_on("mine", object(FULL_STORE)).expect("register object");
    configure_store_on(js_store_config("mem", "mine")).expect("configure");

    assert!(
        !std::sync::Arc::ptr_eq(
            &instance.resolve().expect("resolve").0,
            &with_global().expect("global").0
        ),
        "configuring the singleton must not repoint an explicit instance"
    );
    reset_global();
}

/// A failed reconfiguration leaves the working store retained and installed.
///
/// The retained configuration is what every later rebuild replays, so getting this wrong is
/// delayed rather than immediate: the failure is visible only when something *else* triggers a
/// rebuild, and then the store either vanishes or the rebuild keeps failing.
#[wasm_bindgen_test]
fn failed_reconfiguration_keeps_the_previous_store() {
    reset_global();
    init_global().expect("init");
    register_store_object_on("good", object(FULL_STORE)).expect("register object");
    configure_store_on(js_store_config("mem", "good")).expect("configure");

    // An invalid replacement: it names an object nobody registered.
    let invalid = configure_store_on(js_store_config("mem", "never_registered"));
    assert!(
        invalid.is_err(),
        "a configuration naming an unknown object must fail"
    );

    // Still installed…
    assert!(global_store()
        .expect("store")
        .store_name()
        .contains("Store router"));

    // …and still *retained*, which only a later rebuild can show. Registering a command is the
    // ordinary way a rebuild happens.
    register_command_on(&common::decl("hello", "return 'hi';")).expect("register command");
    assert!(
        global_store()
            .expect("store after rebuild")
            .store_name()
            .contains("Store router"),
        "the working configuration must survive a rebuild that follows a failed replacement"
    );

    reset_global();
}

// ---------------------------------------------------------------------------
// STORE14–STORE17: store adapters from configuration (`specs/design/store-adapters/`).
// ---------------------------------------------------------------------------

/// A store offering data-only access: plain bytes in a `Map`, a fixed backend time.
const DATA_STORE: &str = r#"
(() => {
  const data = new Map();
  const children = (key) => {
    const prefix = key === '' ? '' : key + '/';
    const names = new Set();
    for (const k of data.keys()) {
      if (k.startsWith(prefix)) names.add(k.slice(prefix.length).split('/')[0]);
    }
    return [...names];
  };
  let gets = 0;
  return {
    _data: data,
    _gets() { return gets; },
    async get(key) {
      gets += 1;
      if (!data.has(key)) return undefined;
      return { data: data.get(key), metadata: {} };
    },
    async getData(key) { return data.get(key) ?? null; },
    async setData(key, bytes) { data.set(key, bytes); },
    async removeData(key) { data.delete(key); },
    async listdirData(key) { return children(key); },
    async dataFacts(key) {
      const bytes = data.get(key);
      if (bytes === undefined) return null;
      return { size: bytes.length, modified: '2026-10-15T12:00:00.123Z', etag: 'e' + bytes.length };
    },
    isDir(key) { return !data.has(key) && children(key).length > 0; },
    async contains(key) { return data.has(key) || children(key).length > 0; },
    async listdir(key) { return children(key); },
  };
})()
"#;

fn data_store_factory(name: &str, source: &str) -> WebStoreFactory {
    let mut factory = WebStoreFactory::new();
    factory.register_object(name, object(source));
    factory
}

fn built(yaml: &str, factory: WebStoreFactory) -> Result<std::sync::Arc<dyn AsyncStore>, liquers_core::error::Error> {
    let config = StoreRouterConfig::from_yaml(yaml).expect("config parses");
    let (registry, functions) = liquers_core::store::realm::StoreConfigFunctions::builtin();
    build_store(&config, factory, registry, functions)
}

/// STORE14 — `read_only` on a `js` entry refuses every write before it reaches the object.
///
/// The page object here is fully writable, so a refusal can only come from the mount: an object
/// with no write methods would refuse on its own and prove nothing about `read_only`.
#[wasm_bindgen_test]
async fn store14_read_only_js_store_refuses_writes() {
    let page_object = object(FULL_STORE);
    let mut factory = WebStoreFactory::new();
    factory.register_object("rw", page_object.clone());
    let store = built(
        "stores:\n  - type: js\n    prefix: ro\n    config: { object: rw }\n    read_only: true\n",
        factory,
    )
    .expect("store builds");

    let k = key("ro/new.txt");
    let refused = |what: &str, result: Result<(), liquers_core::error::Error>| match result {
        Ok(()) => panic!("a read-only mount must refuse {what}"),
        Err(e) => assert_eq!(
            e.error_type,
            ErrorType::NotSupported,
            "{what}: {}",
            e.message
        ),
    };
    refused("set", store.set(&k, b"x", &Metadata::new()).await);
    refused(
        "set_metadata",
        store.set_metadata(&k, &Metadata::new()).await,
    );
    refused("remove", store.remove(&k).await);
    refused("removedir", store.removedir(&key("ro/dir")).await);
    refused("makedir", store.makedir(&key("ro/dir")).await);

    let data: js_sys::Map = js_sys::Reflect::get(&page_object, &JsValue::from_str("_data"))
        .expect("_data")
        .dyn_into()
        .expect("a Map");
    assert_eq!(data.size(), 0, "no write may reach the page object");

    assert!(store.is_read_only());
    // Through the JavaScript export, so a renamed or dropped `js_name` fails here.
    let handle = JsValue::from(liquers_web::store::LiquersStore::new(store.clone()));
    let is_read_only: js_sys::Function =
        js_sys::Reflect::get(&handle, &JsValue::from_str("isReadOnly"))
            .expect("isReadOnly")
            .dyn_into()
            .expect("`Store.isReadOnly` is a function");
    assert_eq!(
        is_read_only.call0(&handle).expect("isReadOnly()"),
        JsValue::TRUE,
        "`Store.isReadOnly()` must report the mount's policy to JavaScript"
    );
}

/// STORE14 — reads still pass through a read-only mount.
#[wasm_bindgen_test]
async fn store14_read_only_js_store_still_reads() {
    let store = built(
        "stores:\n  - type: js\n    prefix: ro\n    config: { object: ro }\n    read_only: true\n",
        data_store_factory("ro", READ_ONLY_STORE),
    )
    .expect("store builds");
    let (data, _) = store
        .get(&key("ro/fixed.txt"))
        .await
        .expect("reads pass through");
    assert_eq!(data, b"hi".to_vec());
}

/// STORE15 — a `metadata` section on an object without the data methods is refused at build.
#[wasm_bindgen_test]
fn store15_metadata_without_data_methods_refused() {
    let result = built(
        "stores:\n  - type: js\n    prefix: d\n    config: { object: full }\n    metadata: { storage: grouped }\n",
        data_store_factory("full", FULL_STORE),
    );
    match result {
        Ok(_) => panic!("a store without data-only access must be refused"),
        Err(e) => {
            assert!(e.message.contains("Mount 'd'"), "{}", e.message);
            assert!(e.message.contains("data-only access"), "{}", e.message);
        }
    }
}

/// STORE16 — under `per_entry` and `grouped` storage the records are data objects the page keeps,
/// the default read chain applies, and the object's own `get` is never called.
#[wasm_bindgen_test]
async fn store16_js_store_under_grouped_storage_round_trips() {
    for storage in ["per_entry", "grouped"] {
        store16_round_trip(storage).await;
    }
}

async fn store16_round_trip(storage: &str) {
    let source = object(DATA_STORE);
    let mut factory = WebStoreFactory::new();
    factory.register_object("notes", source.clone());
    let store = built(
        &format!(
            "stores:\n  - type: js\n    prefix: n\n    config: {{ object: notes }}\n    metadata: {{ storage: {storage} }}\n"
        ),
        factory,
    )
    .expect("store builds");
    let mut record = liquers_core::metadata::MetadataRecord::new();
    record.title = "First".to_string();
    store
        .set(&key("n/a.md"), b"# A", &Metadata::MetadataRecord(record))
        .await
        .expect("set");
    store.set(&key("n/b.txt"), b"bee", &Metadata::new()).await.expect("set");
    let a = store.get_metadata(&key("n/a.md")).await.expect("metadata");
    assert_eq!(a.title(), "First");
    assert_eq!(a.get_media_type(), "text/markdown");
    let mut names = store.listdir(&key("n")).await.expect("listdir");
    names.sort();
    assert_eq!(names, vec!["a.md".to_string(), "b.txt".to_string()]);
    // The default `on_read` chain fills the facts.
    assert_eq!(store.get_asset_info(&key("n/b.txt")).await.expect("info").file_size, Some(3));
    // The records are data objects of the page's store.
    let data = js_sys::Reflect::get(&source, &JsValue::from_str("_data")).expect("_data");
    let map: js_sys::Map = data.dyn_into().expect("a Map");
    let (present, absent) = if storage == "grouped" {
        ("n/__metadata__.json", "n/a.md.__metadata__")
    } else {
        ("n/a.md.__metadata__", "n/__metadata__.json")
    };
    assert!(map.has(&JsValue::from_str(present)), "{storage}: {present}");
    assert!(!map.has(&JsValue::from_str(absent)), "{storage}: {absent}");
    let gets = js_sys::Reflect::get(&source, &JsValue::from_str("_gets"))
        .expect("_gets")
        .dyn_into::<js_sys::Function>()
        .expect("a function")
        .call0(&source)
        .expect("call");
    assert_eq!(gets.as_f64(), Some(0.0), "{storage}: the object's get must not be called");
}

/// STORE17 — `created` comes from the page's `dataFacts` time, in the canonical form.
#[wasm_bindgen_test]
async fn store17_created_round_trips_from_js_time() {
    let store = built(
        "stores:\n  - type: js\n    prefix: n\n    config: { object: notes }\n    metadata: { storage: per_entry }\n",
        data_store_factory("notes", DATA_STORE),
    )
    .expect("store builds");
    store.set(&key("n/a.txt"), b"abc", &Metadata::new()).await.expect("set");
    let metadata = store.get_metadata(&key("n/a.txt")).await.expect("metadata");
    let record = metadata.metadata_record().expect("a record");
    assert_eq!(
        record.created.as_ref().map(|t| t.as_str().to_string()),
        Some("2026-10-15T12:00:00.123Z".to_string())
    );
    assert_eq!(record.etag.as_deref(), Some("e3"));
    let info = store.get_asset_info(&key("n/a.txt")).await.expect("info");
    assert_eq!(info.created, record.created);
}

/// `DATA_STORE` with `dataFacts` reporting the given `modified` text and `precision` name.
fn data_store_with_facts(modified: &str, precision: &str) -> String {
    let original = "modified: '2026-10-15T12:00:00.123Z'";
    assert!(
        DATA_STORE.contains(original),
        "the fixture's facts line moved"
    );
    DATA_STORE.replace(
        original,
        &format!("modified: '{modified}', precision: '{precision}'"),
    )
}

/// STORE17 — an explicit `precision` from `dataFacts` is applied to the backend time.
#[wasm_bindgen_test]
async fn store17_explicit_precision_is_applied() {
    let source = data_store_with_facts("2026-10-15T12:00:00.123456Z", "millis");
    let store = built(
        "stores:\n  - type: js\n    prefix: n\n    config: { object: notes }\n    metadata: { storage: per_entry }\n",
        data_store_factory("notes", &source),
    )
    .expect("store builds");
    store
        .set(&key("n/a.txt"), b"abc", &Metadata::new())
        .await
        .expect("set");
    let metadata = store.get_metadata(&key("n/a.txt")).await.expect("metadata");
    let record = metadata.metadata_record().expect("a record");
    assert_eq!(
        record.created.as_ref().map(|t| t.as_str().to_string()),
        Some("2026-10-15T12:00:00.123Z".to_string()),
        "the time must be cut to the declared millisecond precision"
    );
}

/// STORE17 — an unknown `precision` name is refused, naming it, rather than ignored.
#[wasm_bindgen_test]
async fn store17_unknown_precision_is_refused() {
    let source = data_store_with_facts("2026-10-15T12:00:00Z", "hours");
    let store = built(
        "stores:\n  - type: js\n    prefix: n\n    config: { object: notes }\n    metadata: { storage: per_entry }\n",
        data_store_factory("notes", &source),
    )
    .expect("store builds");
    // The facts are read when `set` stamps `created` or when metadata is read back; whichever
    // reads them first must fail.
    let error = match store.set(&key("n/a.txt"), b"abc", &Metadata::new()).await {
        Err(e) => e,
        Ok(()) => match store.get_metadata(&key("n/a.txt")).await {
            Err(e) => e,
            Ok(m) => panic!("an unknown precision was accepted: {m:?}"),
        },
    };
    assert!(error.message.contains("'hours'"), "{}", error.message);
}

// ---------------------------------------------------------------------------
// STORE07 — a page store's data evaluates end to end (`js-store-page-metadata-status`).
// ---------------------------------------------------------------------------

/// A read-only page store serving one file, with the given `metadata` expression for it.
///
/// Implements only `get` and `contains`, as the e2e test's store does.
fn page_store(metadata: &str) -> js_sys::Object {
    object(&format!(
        r#"({{
  async get(key) {{
    if (key !== 'mem/greeting.txt') return undefined;
    return {{ data: new TextEncoder().encode('from the page'), metadata: {metadata} }};
  }},
  async contains(key) {{ return key === 'mem/greeting.txt'; }},
}})"#
    ))
}

/// Evaluates `query` against a fresh global environment whose `mem` prefix is `store`.
async fn evaluate_over(store: js_sys::Object, query: &str) -> Result<JsValue, liquers_core::error::Error> {
    common::fresh();
    register_store_object_on("page", store).expect("register store object");
    configure_store_on(js_store_config("mem", "page")).expect("configure");
    let result = common::eval_to_js(query).await;
    reset_global();
    result
}

/// AC-1: metadata without a `status` no longer makes the asset layer refuse the bytes.
#[wasm_bindgen_test]
async fn store07_js_store_with_partial_metadata_evaluates() {
    let text = evaluate_over(
        page_store("{ media_type: 'text/plain' }"),
        "-R/mem/greeting.txt/-/to_text",
    )
    .await
    .expect("a page store's resource must evaluate");
    assert_eq!(text.as_string().as_deref(), Some("from the page"));
}

/// AC-2: no metadata at all evaluates the same way, and the record is seeded from the key.
#[wasm_bindgen_test]
async fn store07_js_store_without_metadata_evaluates() {
    let text = evaluate_over(page_store("undefined"), "-R/mem/greeting.txt/-/to_text")
        .await
        .expect("a page store's resource must evaluate");
    assert_eq!(text.as_string().as_deref(), Some("from the page"));

    let store = JsStore::new(&key("mem"), "page", page_store("{ media_type: 'text/plain' }"))
        .expect("construct");
    let (_, metadata) = store.get(&key("mem/greeting.txt")).await.expect("get");
    let Metadata::MetadataRecord(record) = metadata else {
        panic!("a partial object must become a record");
    };
    assert_eq!(record.key, Some(key("mem/greeting.txt")), "the key is seeded");
    assert_eq!(record.filename.as_deref(), Some("greeting.txt"), "so is the filename");
    assert_eq!(record.data_format.as_deref(), Some("txt"), "and the format follows it");
}

/// AC-3: a status the page declares is kept, so a stored `Error` is still not reused.
#[wasm_bindgen_test]
async fn store07_js_store_declared_error_status_is_kept() {
    let result = evaluate_over(
        page_store("{ media_type: 'text/plain', status: 'Error' }"),
        "-R/mem/greeting.txt/-/to_text",
    )
    .await;
    assert!(
        result.as_ref().map(|v| v.as_string()).ok().flatten().as_deref() != Some("from the page"),
        "a declared Error must not be served as a value: {result:?}"
    );
}

/// AC-4: bytes a page writes through the store with empty metadata evaluate afterwards.
#[wasm_bindgen_test]
async fn store07_js_store_page_write_evaluates() {
    common::fresh();
    register_store_object_on("full", object(FULL_STORE)).expect("register store object");
    configure_store_on(js_store_config("mem", "full")).expect("configure");

    let written = global_store().expect("store").set(
        "mem/note.txt",
        b"written by the page",
        js_sys::Object::new().into(),
    );
    wasm_bindgen_futures::JsFuture::from(written)
        .await
        .expect("the write must succeed");

    let text = common::eval_to_js("-R/mem/note.txt/-/to_text")
        .await
        .expect("a page write must evaluate");
    assert_eq!(text.as_string().as_deref(), Some("written by the page"));
    reset_global();
}
