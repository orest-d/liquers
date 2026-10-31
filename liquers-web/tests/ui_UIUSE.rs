//! `UIUSE` tests that need no DOM: UI element handles, `mountUI`'s refusals, and the `lui`
//! commands in the builtins. The mount itself is tested in `ui_browser_UIUSE.rs`.

#![cfg(target_arch = "wasm32")]

mod common;
use common::{eval_to_js, fresh, get, obj, set};

use std::sync::Arc;

use liquers_lib::ui::UIElement;
use liquers_lib::value::{ExtValue, Value};
use liquers_web::bridge::{js_to_value, value_to_js, ConversionPolicy};
use liquers_web::environment::reset_global;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use wasm_bindgen_test::*;

fn error_type(thrown: &JsValue) -> String {
    get(thrown, "errorType").as_string().unwrap_or_default()
}

fn element_of(value: &Value) -> Arc<dyn UIElement> {
    match value {
        Value::Extended(ExtValue::UIElement { value }) => value.clone(),
        other => panic!("not a UI element: {other:?}"),
    }
}

/// UIUSE02 — a UI element value reaches JavaScript as a `UIElement` handle.
#[wasm_bindgen_test]
async fn uiuse02_ui_element_value_reaches_js_as_handle() {
    fresh();
    let handle = eval_to_js("hello/q/ns-lui/query_console")
        .await
        .expect("evaluate the console");
    assert_eq!(
        get(&handle, "typeName").as_string().as_deref(),
        Some("QueryConsoleElement")
    );
    let render: js_sys::Function = get(&handle, "renderHtml").unchecked_into();
    let html = render
        .call0(&handle)
        .expect("renderHtml")
        .as_string()
        .unwrap_or_default();
    assert!(html.contains("lq-"), "markup of the console: {html}");
    reset_global();
}

/// UIUSE02b — a handle passed back to Liquers gives Rust the same element `Arc`.
#[wasm_bindgen_test]
async fn uiuse02b_ui_element_handle_crosses_back_as_same_arc() {
    fresh();
    let element: Arc<dyn UIElement> = Arc::new(liquers_lib::ui::Placeholder::new());
    let value = Value::Extended(ExtValue::UIElement {
        value: element.clone(),
    });
    let handle = value_to_js(&value).expect("to JavaScript");
    assert_eq!(
        get(&handle, "typeName").as_string().as_deref(),
        Some(element.type_name())
    );
    let back: Value = js_to_value(&handle, ConversionPolicy::Structural).expect("back");
    assert!(
        Arc::ptr_eq(&element_of(&back), &element),
        "the same Arc comes back"
    );
    // A spread copy is data, not a handle.
    let copy = js_sys::Object::assign(&js_sys::Object::new(), handle.unchecked_ref());
    let copied: Result<Value, _> = js_to_value(&copy.into(), ConversionPolicy::Structural);
    if let Ok(v) = copied {
        assert!(
            !matches!(v, Value::Extended(ExtValue::UIElement { .. })),
            "a plain copy must not cross as the element"
        );
    }
    reset_global();
}

/// UIUSE02c — a freed handle is refused.
#[wasm_bindgen_test]
async fn uiuse02c_freed_ui_element_is_refused() {
    fresh();
    let element: Arc<dyn UIElement> = Arc::new(liquers_lib::ui::Placeholder::new());
    let handle = value_to_js(&Value::Extended(ExtValue::UIElement { value: element }))
        .expect("to JavaScript");
    let free: js_sys::Function = get(&handle, "free").unchecked_into();
    free.call0(&handle).expect("free");
    let back: Result<Value, _> = js_to_value(&handle, ConversionPolicy::Structural);
    assert!(back.is_err(), "a freed handle must not convert");
    reset_global();
}

async fn mount_rejection(options: JsValue) -> JsValue {
    let promise = liquers_web::ui::mount_ui(JsValue::NULL, options);
    match JsFuture::from(promise).await {
        Ok(v) => panic!("mountUI must reject here, resolved with {v:?}"),
        Err(e) => e,
    }
}

/// UIUSE07 — an unparsable query is refused before anything is mounted.
#[wasm_bindgen_test]
async fn uiuse07_mount_rejects_unparsable_query() {
    fresh();
    let options = obj();
    set(&options, "query", &JsValue::from_str("a~"));
    let thrown = mount_rejection(options.into()).await;
    assert_eq!(error_type(&thrown), "parse_error");
    reset_global();
}

/// UIUSE07c — `query` and `spec` together are refused.
#[wasm_bindgen_test]
async fn uiuse07c_mount_refuses_query_and_spec() {
    fresh();
    let options = obj();
    set(&options, "query", &JsValue::from_str("hello"));
    set(
        &options,
        "spec",
        &JsValue::from_str("layout: !vertical null"),
    );
    let thrown = mount_rejection(options.into()).await;
    let message = get(&thrown, "message").as_string().unwrap_or_default();
    assert!(message.contains("not both"), "{message}");
    reset_global();
}

/// UIUSE08 — without a DOM (Node), mounting is refused as not supported.
#[wasm_bindgen_test]
async fn uiuse08_mount_without_dom_is_refused() {
    fresh();
    let options = obj();
    set(&options, "query", &JsValue::from_str("hello"));
    let thrown = mount_rejection(options.into()).await;
    assert_eq!(error_type(&thrown), "not_supported");
    reset_global();
}

/// The `lui` commands are built in: `ns-lui/roots` fails for want of a UI payload, not because
/// the command is unknown.
#[wasm_bindgen_test]
async fn uiuse_lui_commands_are_registered() {
    fresh();
    let envref = liquers_web::environment::shared_env().expect("env");
    let key = liquers_core::command_metadata::CommandKey::new("", "lui", "roots");
    assert!(
        envref.get_command_metadata_registry().get(key).is_some(),
        "lui/roots is registered"
    );
    let err = match eval_to_js("ns-lui/roots").await {
        Err(e) => e,
        Ok(v) => panic!("roots needs a UI payload, got {v:?}"),
    };
    assert!(
        !err.message.to_lowercase().contains("not found")
            && !err.message.to_lowercase().contains("unknown command"),
        "the failure is not an unknown command: {}",
        err.message
    );
    reset_global();
}
