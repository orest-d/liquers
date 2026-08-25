//! `EVAL*` conformance tests — evaluating a query through the real environment.
//!
//! These use the shared fixture commands, so they exercise the same definitions as every other
//! suite.

#![cfg(target_arch = "wasm32")]

mod common;
use common::{
    decl, decl_with_params, eval_to_js, fresh, get, register_fixture_commands, with, with_timeout,
};

use liquers_web::asset::{get_asset_for, LiquersState};
use liquers_web::environment::{register_command_on, reset_global, shared_env};
use liquers_web::objects::LiquersQuery;
use wasm_bindgen::prelude::*;
use wasm_bindgen_test::*;

/// EVAL01 — evaluating a query returns its converted value.
#[wasm_bindgen_test]
async fn eval01_evaluate_builtin_query() {
    fresh();
    register_fixture_commands();

    assert_eq!(
        eval_to_js("hello")
            .await
            .expect("hello")
            .as_string()
            .as_deref(),
        Some("Hello, world!")
    );
    assert_eq!(
        eval_to_js("number-42").await.expect("number").as_f64(),
        Some(42.0)
    );
    assert_eq!(
        eval_to_js("hello/shout")
            .await
            .expect("shout")
            .as_string()
            .as_deref(),
        Some("HELLO, WORLD!")
    );
    reset_global();
}

/// EVAL02 — a query given as a string and the same query wrapped in a `Query` object agree.
///
/// The point is that the wrapper is not a second parser: `Query::parse(s).encode()` must round-trip
/// to the same query, and evaluating either must produce the same value.
#[wasm_bindgen_test]
async fn eval02_string_and_wrapped_query_agree() {
    fresh();
    register_fixture_commands();

    let text = "hello/repeat-3";
    let wrapped = LiquersQuery::parse(text).expect("parse");
    assert_eq!(
        wrapped.encode(),
        text,
        "the wrapper must round-trip the query"
    );

    let envref = shared_env().expect("env");
    let from_string = eval_to_js(text).await.expect("evaluate the string form");
    let from_object = liquers_web::eval::evaluate_query(envref, wrapped.inner().clone())
        .await
        .expect("evaluate the wrapped form");

    assert_eq!(from_string.as_string(), from_object.as_string());
    assert_eq!(
        from_string.as_string().as_deref(),
        Some("Hello, world!Hello, world!Hello, world!")
    );
    reset_global();
}

/// EVAL03 — metadata and logs are reachable from a resolved asset.
#[wasm_bindgen_test]
async fn eval03_metadata_and_logs_available() {
    fresh();
    register_fixture_commands();

    let envref = shared_env().expect("env");
    let query = liquers_core::parse::parse_query("hello").expect("parse");
    let asset = get_asset_for(envref, query).await.expect("get asset");

    let state = asset.get().await.expect("resolve");
    let wrapped = LiquersState::from_state(state);

    // Status crosses as a lowercase string, and a resolved asset is ready.
    assert_eq!(wrapped.status(), "ready");

    // Metadata is a plain object with the fields a page needs.
    let metadata = wrapped.metadata().expect("metadata");
    assert!(
        metadata.is_object(),
        "metadata must cross as a plain object"
    );
    assert!(
        !get(&metadata, "type_identifier").is_undefined(),
        "metadata must carry the value's type identifier"
    );

    // The log is always an array — absent entries are an empty log, not a missing field.
    let log = wrapped.log().expect("log");
    assert!(
        js_sys::Array::is_array(&log),
        "the log must always be an array"
    );

    // And the value is reachable from the same state.
    assert_eq!(
        wrapped.to_js().expect("value").as_string().as_deref(),
        Some("Hello, world!")
    );
    reset_global();
}

/// EVAL04 — an invalid query maps through the error type, at the layer that rejected it.
#[wasm_bindgen_test]
async fn eval04_invalid_query_maps_through_error() {
    fresh();
    register_fixture_commands();

    // Unparseable: rejected before planning.
    let parse_error = liquers_core::parse::parse_query("hello/~~~broken~~~")
        .err()
        .expect("a malformed query must not parse");
    assert_eq!(
        parse_error.error_type,
        liquers_core::error::ErrorType::ParseError
    );

    // Parses, but names a command that does not exist: rejected at planning.
    let plan_error = eval_to_js("hello/no_such_command")
        .await
        .err()
        .expect("an unknown command must not evaluate");
    assert_eq!(
        plan_error.error_type,
        liquers_core::error::ErrorType::ActionNotRegistered,
        "an unknown command must fail to plan, naming the action: {}",
        plan_error.message
    );

    // Parses and plans, but the command throws: rejected at execution.
    register_command_on(&decl("boom", "throw new Error('bang');")).expect("register");
    let exec_error = eval_to_js("boom")
        .await
        .err()
        .expect("a throwing command must fail");
    assert!(
        exec_error.message.contains("bang"),
        "the JavaScript message must survive: {}",
        exec_error.message
    );
    reset_global();
}

/// EVAL05 — the context reaches a command, and the payload behaves as documented.
///
/// Scoped to what exists (Phase 3): `Payload = ()` for this environment, so the assertion is that
/// a command runs inside a real context — one created by the asset manager, able to evaluate a
/// dependent query — rather than that a payload arrives. This widens when `UIUSE` introduces a
/// real payload.
#[wasm_bindgen_test]
async fn eval05_payload_and_context_reach_a_command() {
    fresh();
    register_fixture_commands();

    // A command whose result depends on the state produced by an earlier step proves the context
    // carried the pipeline through: `shout` cannot see "Hello, world!" unless `hello` ran first
    // and its state was threaded through the same evaluation.
    assert_eq!(
        eval_to_js("hello/repeat-2/shout")
            .await
            .expect("three-step pipeline")
            .as_string()
            .as_deref(),
        Some("HELLO, WORLD!HELLO, WORLD!")
    );

    // The documented payload behaviour: this environment declares `Payload = ()`, and no command
    // requires one, so evaluation proceeds without a payload being supplied.
    let envref = shared_env().expect("env");
    let _: &() =
        &<liquers_web::WebEnvironment as liquers_core::context::Environment>::Payload::default();
    let query = liquers_core::parse::parse_query("hello").expect("parse");
    assert!(
        get_asset_for(envref, query).await.is_ok(),
        "a query needs no payload when Payload = ()"
    );
    reset_global();
}

/// EVAL06 — cancellation has a defined terminal result.
///
/// **The defined result on this target is "nothing was cancelled."** The wasm environment uses
/// `ImmediateAssetManager` (Phase 1 decision 5), which evaluates during `get_asset`, so by the
/// time a caller holds the handle the asset has already reached a terminal status and there is
/// nothing left to stop.
///
/// This is asserted rather than accommodated. An earlier version of this test matched on both
/// outcomes — "either it was cancelled or it had finished" — which passes whatever the
/// implementation does and would have kept passing if `cancel()` started throwing, hanging, or
/// corrupting the asset. The behaviour is deterministic, so the assertion is too; the day a
/// deferred asset manager makes cancellation real, this test fails and says so.
///
/// See `specs/issues/WEB-CANCELLATION-INERT.md`.
#[wasm_bindgen_test]
async fn eval06_cancellation_has_defined_terminal_result() {
    fresh();
    register_fixture_commands();

    // A command that takes 300 ms, so a *deferred* manager would leave a real window to cancel in.
    // Registered before the environment is shared: sharing it first would make `envref` stale,
    // because registering afterwards rebuilds the environment.
    register_command_on(&with(
        decl(
            "slow",
            "return new Promise(r => setTimeout(() => r('finished'), 300));",
        ),
        "async",
        JsValue::TRUE,
    ))
    .expect("register slow");

    let envref = shared_env().expect("env");
    let query = liquers_core::parse::parse_query("slow").expect("parse");
    let asset = get_asset_for(envref, query).await.expect("get asset");

    // Already terminal on arrival — this is the property that makes cancellation inert.
    assert_eq!(
        liquers_web::asset::status_name(asset.status().await),
        "ready",
        "the immediate asset manager evaluates during get_asset, so the asset is terminal before \
         the caller can act on it"
    );

    asset.cancel().await.expect("cancel must not error");
    assert_eq!(
        liquers_web::asset::status_name(asset.status().await),
        "ready",
        "cancelling a terminal asset must leave it terminal and unchanged"
    );

    // And the value is still there: an inert cancellation must not damage the result.
    let state = with_timeout(2000, "get after cancel", asset.get())
        .await
        .expect("a terminal asset still yields its value after an inert cancel");
    let wrapped = LiquersState::from_state(state);
    assert_eq!(wrapped.status(), "ready");
    assert_eq!(
        wrapped.to_js().expect("value").as_string().as_deref(),
        Some("finished")
    );

    // Cancelling again is a no-op, not an error.
    asset.cancel().await.expect("a second cancel is a no-op");
    reset_global();
}

/// A built-in Rust command consumes a JavaScript command's result.
///
/// Not a prescribed conformance test, but it is *the* justification for Phase 1 decision 2 —
/// structural conversion as the default rather than opaque pass-through. If a JavaScript command's
/// result were carried opaquely, `to_text` would have nothing it could read and the two halves of
/// the command set could not be composed in one query. That is the whole argument, so it gets a
/// test.
#[wasm_bindgen_test]
async fn web_builtin_rust_command_consumes_a_javascript_result() {
    fresh();
    register_fixture_commands();

    assert_eq!(
        eval_to_js("hello/to_text")
            .await
            .expect("a Rust built-in must be able to read a JavaScript result")
            .as_string()
            .as_deref(),
        Some("Hello, world!")
    );

    // And the other direction: a JavaScript command reading what a Rust built-in produced.
    assert_eq!(
        eval_to_js("hello/to_text/shout")
            .await
            .expect("a JavaScript command must be able to read a Rust result")
            .as_string()
            .as_deref(),
        Some("HELLO, WORLD!")
    );
    reset_global();
}

/// Evaluation of a command that returns a structured object, so `EVAL01`'s conversion is not only
/// tested on scalars. Not a prescribed conformance test.
#[wasm_bindgen_test]
async fn web_evaluate_structured_result() {
    fresh();
    register_command_on(&with(
        decl_with_params("record", &[], "return { a: 1, b: ['x', true] };"),
        "doc",
        JsValue::from_str("A structured result."),
    ))
    .expect("register");

    let result = eval_to_js("record").await.expect("evaluate");
    assert_eq!(get(&result, "a").as_f64(), Some(1.0));
    let b = get(&result, "b");
    assert!(js_sys::Array::is_array(&b));
    let b = js_sys::Array::from(&b);
    assert_eq!(b.get(0).as_string().as_deref(), Some("x"));
    assert_eq!(b.get(1).as_bool(), Some(true));
    reset_global();
}

/// EVAL07 — a keyed (`-R/`) query evaluates.
///
/// **This is the regression guard for `CORE-IMMEDIATE-MANAGER-KEYED-RECURSION`.** Keyed
/// evaluation used to recurse until the wasm stack was exhausted: the plan's resource step
/// resolves `-R/d/f.txt` through `AssetManager::get`, which runs an unfinished asset inline,
/// and `evaluate_recipe` then asked `get` who owned the very key it was evaluating. The wasm
/// instance died and the caller's `Promise` never settled — a hang whose only trace was a
/// `pageerror`, which is why the browser suite could not catch it either.
///
/// The ownership question is now a non-evaluating map read
/// (`specs/design/keyed-recipe-ownership/`). This runs in the Node loop, so it is the cheapest
/// guard on the real target.
///
/// The key has both a stored value and a recipe that would produce a different value. Returning
/// the stored value proves `ImmediateAssetManager` fast-tracks before recipe evaluation rather
/// than running the conflicting recipe.
#[wasm_bindgen_test]
async fn eval07_keyed_query_evaluates() {
    use liquers_core::metadata::{Metadata, MetadataRecord};
    use liquers_store::config::StoreRouterConfig;
    use liquers_web::environment::{configure_store_on, register_store_object_on};
    use wasm_bindgen::JsCast;

    // A minimal writable JS store, local to this test. `store_js_STORE.rs` has a fuller one,
    // but duplicating five lines keeps the two suites independent — this one only has to hold
    // a single file.
    const STORE: &str = r#"
(() => {
  const data = new Map();
  return {
    async get(key) { return data.get(key); },
    async set(key, bytes, metadata) { data.set(key, { data: bytes, metadata }); },
    async contains(key) { return data.has(key); },
    isDir(key) { return false; },
    isSupported(key) { return true; },
    async keys() { return [...data.keys()]; },
  };
})()
"#;
    let store_object = js_sys::eval(STORE)
        .expect("fixture source must evaluate")
        .dyn_into::<js_sys::Object>()
        .expect("fixture must be an object");

    fresh();
    register_fixture_commands();

    register_store_object_on("evalstore", store_object).expect("register store object");
    configure_store_on(
        StoreRouterConfig::from_yaml(
            "stores:\n  - type: js\n    prefix: d\n    config: { object: evalstore }\n",
        )
        .expect("config parses"),
    )
    .expect("configure");

    let envref = shared_env().expect("env");
    let mut record = MetadataRecord::new();
    record.with_media_type("text/plain".to_string());
    envref
        .get_async_store()
        .set(
            &liquers_core::parse::parse_key("d/recipes.yaml").expect("key"),
            b"recipes:\n  - query: hello/greeting.txt\n",
            &Metadata::MetadataRecord(record),
        )
        .await
        .expect("seed the store");

    let key = liquers_core::parse::parse_key("d/greeting.txt").expect("key");
    let mut stored_record = MetadataRecord::new();
    stored_record.with_key(key.clone());
    stored_record.with_type_identifier("Text".to_owned());
    stored_record.with_status(liquers_core::metadata::Status::Source);
    envref
        .get_async_store()
        .set(
            &key,
            b"from store",
            &Metadata::MetadataRecord(stored_record),
        )
        .await
        .expect("seed stored result");

    let text = eval_to_js("-R/d/greeting.txt")
        .await
        .expect("keyed evaluation must resolve, not exhaust the stack");
    assert_eq!(
        text.as_string().as_deref(),
        Some("from store"),
        "the eligible stored value must win over the conflicting recipe"
    );

    reset_global();
}
