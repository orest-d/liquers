//! `ACCESSOR*` conformance tests: using existing settable value accessors from JavaScript.
//!
//! Design `specs/design/settable-value-accessor/` (Phase 3 test 7); the IDs are the `ACCESSOR`
//! feature of `LANGUAGE-INTEGRATION_GUIDE.md`, direction 1 (use), and match
//! `liquers-py/tests/test_ACCESSOR_use.py`. Defining a root or a step in JavaScript is out of
//! scope (`ACCESSOR-CANNOT-BE-IMPLEMENTED-IN-PYTHON-OR-JAVASCRIPT`).

#![cfg(target_arch = "wasm32")]

mod common;
use common::{decl, eval_to_js, fresh, get, sleep, with, with_timeout};

use std::sync::Arc;

use liquers_core::assets::AssetManager;
use liquers_core::metadata::{Metadata, MetadataRecord};
use liquers_core::state::State;
use liquers_core::value::ValueInterface;
use liquers_web::default_value::WebValue;
use liquers_core::store_config::StoreRouterConfig;
use liquers_web::environment::{configure_store_on, register_command_on, reset_global, shared_env};
use liquers_web::LiquersAccessor;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use wasm_bindgen_test::*;

const TITLE: &str = "-R-key/d/config.yaml/-/ns-acc/accessor/field-title";
const CONFIG: &str = "title: Dashboard\nrows: 3\n";

/// A fresh environment with a memory store at `d`, holding `d/config.yaml`.
async fn env_with_config() {
    fresh();
    configure_store_on(
        StoreRouterConfig::from_yaml("stores:\n  - type: memory\n    prefix: d\n")
            .expect("config parses"),
    )
    .expect("configure");
    let envref = shared_env().expect("env");
    let value = WebValue::from_string(CONFIG.to_string());
    let mut record = MetadataRecord::new();
    record.data_format = Some(value.default_data_format().to_string());
    let state = State::from_value_and_metadata(value, Arc::new(Metadata::MetadataRecord(record)));
    envref
        .get_asset_manager()
        .set_state(&liquers_core::parse::parse_key("d/config.yaml").expect("key"), state)
        .await
        .expect("seed config.yaml");
}

async fn settle(promise: js_sys::Promise) -> Result<JsValue, JsValue> {
    with_timeout(10_000, "an accessor promise", JsFuture::from(promise)).await
}

async fn accessor(query: &str) -> LiquersAccessor {
    let handle = settle(liquers_web::accessor(query))
        .await
        .unwrap_or_else(|e| panic!("accessor({query}) rejected: {e:?}"));
    handle_of(handle)
}

/// A Rust handle over the accessor an `Accessor` object stands for, read through the value
/// bridge (a wasm-bindgen class cannot be borrowed back from a `JsValue`). It is a new handle, so
/// it starts with no token of its own.
fn handle_of(js: JsValue) -> LiquersAccessor {
    assert!(
        js_sys::Reflect::has(&js, &JsValue::from_str("__liquersAccessorHandle")).unwrap_or(false),
        "not an Accessor handle: {js:?}"
    );
    let value: WebValue =
        liquers_web::bridge::js_to_value(&js, liquers_web::ConversionPolicy::Structural)
            .expect("an Accessor handle crosses back");
    LiquersAccessor::new(
        value.try_into_accessor().expect("an accessor value"),
        shared_env().ok(),
    )
}

async fn text(promise: js_sys::Promise) -> String {
    settle(promise)
        .await
        .expect("resolves")
        .as_string()
        .expect("a string")
}

async fn stored(key: &str) -> String {
    eval_to_js(&format!("-R/{key}"))
        .await
        .expect("evaluates")
        .as_string()
        .expect("text")
}

/// ACCESSOR01: get and set with a token; `set` uses the token this handle last saw.
#[wasm_bindgen_test]
async fn accessor01_get_and_set_with_a_token() {
    env_with_config().await;
    let acc = accessor(TITLE).await;
    assert_eq!(get(&acc.capabilities(), "write").as_bool(), Some(true));
    let snap = settle(acc.snapshot()).await.expect("snapshot");
    let value: js_sys::Function = get(&snap, "value").unchecked_into();
    assert_eq!(
        value.call0(&snap).expect("value()").as_string().as_deref(),
        Some("Dashboard")
    );
    let token = get(&snap, "token").as_string().expect("a token");
    assert!(!token.is_empty());
    let new = text(acc.set_if(JsValue::from_str("Sales"), Some(token.clone()))).await;
    assert_ne!(new, token);
    assert_eq!(
        settle(acc.get()).await.expect("get").as_string().as_deref(),
        Some("Sales")
    );
    assert_eq!(stored("d/config.yaml").await, "title: Sales\nrows: 3\n");
    let third = text(acc.set(JsValue::from_str("Report"))).await;
    assert_ne!(third, new);
    let again = settle(acc.snapshot()).await.expect("snapshot");
    assert_eq!(get(&again, "token").as_string(), Some(third));
    reset_global();
}

/// ACCESSOR02: project and describe; a query that is not an accessor is read-only.
#[wasm_bindgen_test]
async fn accessor02_project_and_describe() {
    env_with_config().await;
    let root = accessor("-R-key/d/config.yaml/-/ns-acc/accessor").await;
    let title = handle_of(settle(root.project("field-title")).await.expect("project"));
    assert_eq!(title.address(), TITLE);
    assert_eq!(
        settle(title.get()).await.expect("get").as_string().as_deref(),
        Some("Dashboard")
    );
    let part = settle(title.describe()).await.expect("describe");
    assert_eq!(get(&part, "typeIdentifier").as_string().as_deref(), Some("Text"));
    assert_eq!(get(&part, "readOnly").as_bool(), Some(false));
    let plain = accessor("-R/d/config.yaml").await;
    assert_eq!(get(&plain.capabilities(), "write").as_bool(), Some(false));
    let part = settle(plain.describe()).await.expect("describe");
    assert_eq!(get(&part, "readOnly").as_bool(), Some(true));
    reset_global();
}

/// ACCESSOR03: events reach a subscriber, from a task of its own; `dispose` stops them.
#[wasm_bindgen_test]
async fn accessor03_events_reach_subscribers() {
    env_with_config().await;
    let acc = accessor(TITLE).await;
    let token = get(&settle(acc.snapshot()).await.expect("snapshot"), "token").as_string();
    let seen = js_sys::Array::new();
    let push = js_sys::Function::new_with_args("e", "this.push(e.event);").bind0(&seen);
    let subscription = acc.subscribe(push);
    let new = text(acc.set_if(JsValue::from_str("Sales"), token)).await;
    for _ in 0..100 {
        if seen.includes(&JsValue::from_str("changed"), 0) {
            break;
        }
        sleep(20).await;
    }
    assert!(
        seen.includes(&JsValue::from_str("changed"), 0),
        "no changed event; saw {seen:?}"
    );
    subscription.dispose();
    let count = seen.length();
    text(acc.set_if(JsValue::from_str("Again"), Some(new))).await;
    sleep(100).await;
    assert_eq!(seen.length(), count, "a disposed subscription still calls back");
    reset_global();
}

/// ACCESSOR04: a conflict rejects with `status_conflict` carrying the current token.
#[wasm_bindgen_test]
async fn accessor04_conflict_is_status_conflict() {
    env_with_config().await;
    let acc = accessor(TITLE).await;
    let old = get(&settle(acc.snapshot()).await.expect("snapshot"), "token").as_string();
    text(acc.set_if(JsValue::from_str("Sales"), old.clone())).await;
    let rejection = settle(acc.set_if(JsValue::from_str("Stale"), old))
        .await
        .expect_err("a stale token conflicts");
    assert_eq!(
        get(&rejection, "errorType").as_string().as_deref(),
        Some("status_conflict")
    );
    let info = settle(acc.info()).await.expect("info");
    assert_eq!(get(&rejection, "token").as_string(), get(&info, "token").as_string());
    assert!(get(&rejection, "token").is_string());
    assert_eq!(stored("d/config.yaml").await, "title: Sales\nrows: 3\n");
    let rejection = settle(acc.set_if(JsValue::from_str("New"), None))
        .await
        .expect_err("creating a value that exists conflicts");
    assert_eq!(
        get(&rejection, "errorType").as_string().as_deref(),
        Some("status_conflict")
    );
    // A conflict from elsewhere has no token.
    let other = liquers_web::LiquersError::construct("status_conflict", "busy".into(), None)
        .expect("construct");
    assert_eq!(other.token(), None);
    reset_global();
}

/// ACCESSOR05: a memory accessor is a variable shared by every handle of it.
#[wasm_bindgen_test]
async fn accessor05_memory_accessor_is_shared() {
    env_with_config().await;
    let counter = liquers_web::memory_accessor(Some(JsValue::from_f64(1.0)), Some("counter".into()))
        .expect("memoryAccessor");
    assert_eq!(settle(counter.get()).await.expect("get").as_f64(), Some(1.0));
    assert_eq!(counter.address(), "counter");
    let global = js_sys::global();
    let handle: JsValue = liquers_web::memory_accessor(None, None).expect("memoryAccessor").into();
    // The page keeps a handle of `counter` for the command to return.
    let counter_js: JsValue = counter.into();
    js_sys::Reflect::set(&global, &JsValue::from_str("__accCounter"), &counter_js).expect("set");
    register_command_on(&decl("shared", "return globalThis.__accCounter;")).expect("shared");
    let other = handle_of(eval_to_js("shared").await.expect("shared"));
    text(other.set(JsValue::from_f64(2.0))).await;
    let counter = handle_of(counter_js);
    assert_eq!(settle(counter.get()).await.expect("get").as_f64(), Some(2.0));
    // An empty variable: get fails, a create sets it.
    let empty = handle_of(handle);
    settle(empty.get()).await.expect_err("an empty variable has no value");
    text(empty.set_if(JsValue::from_str("first"), None)).await;
    assert_eq!(
        settle(empty.get()).await.expect("get").as_string().as_deref(),
        Some("first")
    );
    let _ = js_sys::Reflect::delete_property(global.unchecked_ref::<js_sys::Object>(), &JsValue::from_str("__accCounter"));
    reset_global();
}

/// ACCESSOR06: an accessor crosses into a command and back as the same Rust accessor; each
/// handle keeps its own token.
#[wasm_bindgen_test]
async fn accessor06_accessor_crosses_back_unchanged() {
    env_with_config().await;
    let acc_js = settle(liquers_web::accessor(TITLE)).await.expect("accessor");
    let global = js_sys::global();
    js_sys::Reflect::set(&global, &JsValue::from_str("__accGive"), &acc_js).expect("set");
    register_command_on(&decl("give", "return globalThis.__accGive;")).expect("give");
    register_command_on(&with(
        common::decl_with_params(
            "address_of",
            &["acc"],
            "globalThis.__accCrossed = acc; return acc.address;",
        ),
        "state",
        JsValue::from_str("value"),
    ))
    .expect("address_of");
    assert_eq!(
        eval_to_js("give/address_of").await.expect("evaluates").as_string().as_deref(),
        Some(TITLE)
    );
    let acc = handle_of(acc_js);
    let crossed = handle_of(get(&global, "__accCrossed"));
    let first = get(&settle(acc.snapshot()).await.expect("snapshot"), "token").as_string();
    let crossed_token =
        get(&settle(crossed.snapshot()).await.expect("snapshot"), "token").as_string();
    text(crossed.set_if(JsValue::from_str("Sales"), crossed_token)).await;
    assert_eq!(
        settle(acc.get()).await.expect("get").as_string().as_deref(),
        Some("Sales")
    );
    let rejection = settle(acc.set_if(JsValue::from_str("Stale"), first))
        .await
        .expect_err("the first holder's token is stale");
    assert_eq!(
        get(&rejection, "errorType").as_string().as_deref(),
        Some("status_conflict")
    );
    for name in ["__accGive", "__accCrossed"] {
        let _ = js_sys::Reflect::delete_property(global.unchecked_ref::<js_sys::Object>(), &JsValue::from_str(name));
    }
    reset_global();
}

/// ACCESSOR07: info and log; a missing key's status is `None`, with no token.
#[wasm_bindgen_test]
async fn accessor07_info_and_log() {
    env_with_config().await;
    let acc = accessor(TITLE).await;
    let snap = settle(acc.snapshot()).await.expect("snapshot");
    let info = settle(acc.info()).await.expect("info");
    assert_eq!(get(&info, "token").as_string(), get(&snap, "token").as_string());
    assert_eq!(get(&info, "status").as_string().as_deref(), Some("source"));
    let log_len = get(&info, "logLen").as_f64().expect("logLen");
    let log = settle(acc.log(None)).await.expect("log");
    assert!(js_sys::Array::is_array(&log));
    let since = settle(acc.log(Some(log_len as u32))).await.expect("log");
    assert_eq!(since.unchecked_into::<js_sys::Array>().length(), 0);
    let missing = accessor("-R-key/d/none.txt/-/ns-acc/store").await;
    let info = settle(missing.info()).await.expect("info");
    assert_eq!(get(&info, "status").as_string().as_deref(), Some("none"));
    assert!(get(&info, "token").is_null());
    reset_global();
}
