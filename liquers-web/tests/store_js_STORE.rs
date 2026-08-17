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
use liquers_store::config::StoreRouterConfig;
use liquers_web::store::{build_router, JsStore, WebStoreFactory};
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
