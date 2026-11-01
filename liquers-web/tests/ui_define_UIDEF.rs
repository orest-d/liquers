//! `UIDEF` tests that need no DOM: `defineElement`, `element()`, their refusals, and the release of
//! the page's functions. Events, `mount` and the renderer are tested in `ui_browser_UIDEF.rs`.

#![cfg(target_arch = "wasm32")]

mod common;
use common::{arg, args, decl_with_params, eval_to_js, fresh, func, get, obj, set, with};

use std::sync::Arc;

use liquers_lib::ui::{DelegatedElement, UIElement};
use liquers_web::environment::{register_command_on, reset_global};
use liquers_web::ui_define::{define_element, element, undefine_element};
use wasm_bindgen::prelude::*;
use wasm_bindgen_test::*;

fn error_type(thrown: &JsValue) -> String {
    get(thrown, "errorType").as_string().unwrap_or_default()
}

fn definition(kind: &str, render: &str) -> js_sys::Object {
    let def = obj();
    set(&def, "type", &JsValue::from_str(kind));
    set(&def, "render", &func(render).into());
    def
}

fn render_html(handle: &JsValue) -> String {
    let render: js_sys::Function = get(handle, "renderHtml").unchecked_into();
    render
        .call0(handle)
        .expect("renderHtml")
        .as_string()
        .unwrap_or_default()
}

fn props(json: &str) -> JsValue {
    js_sys::JSON::parse(json).expect("JSON props")
}

/// Installs `globalThis.__lqElement(type, props)`, so a page command can make an element the way
/// `liquers.element` does.
fn install_element_function() {
    let closure = Closure::<dyn Fn(String, JsValue) -> Result<JsValue, JsValue>>::new(
        |kind: String, props: JsValue| element(kind, Some(props), None).map(JsValue::from),
    );
    let global = js_sys::global();
    js_sys::Reflect::set(
        &global,
        &JsValue::from_str("__lqElement"),
        closure.as_ref().unchecked_ref(),
    )
    .expect("install");
    closure.forget();
}

/// UIDEF01 — a defined type renders its markup inside the delegated wrapper.
#[wasm_bindgen_test]
fn uidef01_element_handle_renders_defined_markup() {
    define_element(definition("gauge01", "(p) => `<span class=\"v\">${p.value}%</span>`").into())
        .expect("define");
    let handle: JsValue = element("gauge01".into(), Some(props(r#"{"value":50}"#)), None)
        .expect("element")
        .into();
    assert_eq!(
        get(&handle, "typeName").as_string().as_deref(),
        Some("DelegatedElement")
    );
    assert_eq!(get(&handle, "kind").as_string().as_deref(), Some("gauge01"));
    assert_eq!(
        get(&handle, "title").as_string().as_deref(),
        Some("gauge01")
    );
    let html = render_html(&handle);
    assert!(html.contains("<span class=\"v\">50%</span>"), "{html}");
    assert!(html.contains("data-lq-delegate=\"gauge01\""), "{html}");
    assert!(html.contains("lq-delegated"), "{html}");
    assert!(undefine_element("gauge01".into()));
    assert!(!undefine_element("gauge01".into()), "already undefined");
}

/// UIDEF01b — a throwing render, or one returning a non-string, is contained in the element.
#[wasm_bindgen_test]
fn uidef01b_render_failure_is_contained() {
    define_element(definition("throws01b", "() => { throw new Error('<bad>'); }").into())
        .expect("define");
    let handle: JsValue = element("throws01b".into(), None, None)
        .expect("element")
        .into();
    let html = render_html(&handle);
    assert!(html.contains("lq-error"), "{html}");
    assert!(html.contains("&lt;bad&gt;"), "the error is escaped: {html}");
    assert!(!html.contains("<bad>"), "{html}");

    define_element(definition("number01b", "() => 42").into()).expect("define");
    let handle: JsValue = element("number01b".into(), None, None)
        .expect("element")
        .into();
    let html = render_html(&handle);
    assert!(
        html.contains("lq-error") && html.contains("not a string"),
        "{html}"
    );
    undefine_element("throws01b".into());
    undefine_element("number01b".into());
}

/// UIDEF01c — defining a type again changes how its existing instances render.
#[wasm_bindgen_test]
fn uidef01c_redefine_rerenders() {
    define_element(definition("gauge01c", "(p) => `old ${p.value}`").into()).expect("define");
    let handle: JsValue = element("gauge01c".into(), Some(props(r#"{"value":1}"#)), None)
        .expect("element")
        .into();
    assert!(render_html(&handle).contains("old 1"));
    define_element(definition("gauge01c", "(p) => `new ${p.value}`").into()).expect("redefine");
    let html = render_html(&handle);
    assert!(html.contains("new 1") && !html.contains("old"), "{html}");
    undefine_element("gauge01c".into());
}

/// UIDEF01d — `defineElement` names what is missing or wrong.
#[wasm_bindgen_test]
fn uidef01d_define_refuses_bad_definitions() {
    let no_type = obj();
    set(&no_type, "render", &func("() => ''").into());
    let e = define_element(no_type.into()).expect_err("no type");
    assert!(
        get(&e, "message")
            .as_string()
            .unwrap_or_default()
            .contains("type"),
        "{e:?}"
    );

    let no_render = obj();
    set(&no_render, "type", &JsValue::from_str("x01d"));
    let e = define_element(no_render.into()).expect_err("no render");
    assert!(
        get(&e, "message")
            .as_string()
            .unwrap_or_default()
            .contains("render"),
        "{e:?}"
    );

    let bad_event = definition("x01d", "() => ''");
    set(&bad_event, "onEvent", &JsValue::from_f64(3.0));
    let e = define_element(bad_event.into()).expect_err("onEvent not a function");
    assert!(
        get(&e, "message")
            .as_string()
            .unwrap_or_default()
            .contains("onEvent"),
        "{e:?}"
    );
    assert!(!undefine_element("x01d".into()), "nothing was defined");
}

/// UIDEF02 — props round-trip: a page command makes an element, a second one reads its kind and
/// props back from the handle it receives.
#[wasm_bindgen_test]
async fn uidef02_props_roundtrip_through_a_command() {
    fresh();
    install_element_function();
    register_command_on(&with(
        decl_with_params(
            "gauge",
            &["v"],
            "return globalThis.__lqElement('gauge02', { value: v, label: 'cpu', tags: ['a', 1, null] });",
        ),
        "arguments",
        args(vec![arg("v", Some("int"), None)]),
    ))
    .expect("register gauge");
    register_command_on(&with(
        decl_with_params(
            "describe",
            &["e"],
            "return e.kind + ':' + JSON.stringify(e.props);",
        ),
        "state",
        JsValue::from_str("value"),
    ))
    .expect("register describe");
    let text = eval_to_js("gauge-3/describe")
        .await
        .expect("evaluate")
        .as_string()
        .unwrap_or_default();
    // Props are JSON data; object keys come back sorted, not in the page's order.
    assert_eq!(
        text,
        r#"gauge02:{"label":"cpu","tags":["a",1,null],"value":3}"#
    );
    let handle = eval_to_js("gauge-3").await.expect("the element");
    assert_eq!(get(&handle, "kind").as_string().as_deref(), Some("gauge02"));
    reset_global();
}

/// UIDEF02b — props that are not JSON are refused with a `ConversionError` naming what was found.
#[wasm_bindgen_test]
fn uidef02b_non_json_props_refused() {
    let cases: Vec<(JsValue, &str)> = vec![
        (js_sys::eval("({ f: () => 1 })").expect("eval"), "function"),
        (
            js_sys::eval("({ when: new Date(0) })").expect("eval"),
            "Date",
        ),
        (js_sys::eval("({ n: 10n })").expect("eval"), "bigint"),
        (
            js_sys::eval("(() => { const a = {}; a.self = a; return a; })()").expect("eval"),
            "cycle",
        ),
        (js_sys::eval("({ x: NaN })").expect("eval"), "NaN"),
    ];
    for (value, needle) in cases {
        let thrown = match element("gauge02b".into(), Some(value), None) {
            Ok(_) => panic!("props with a {needle} must be refused"),
            Err(e) => e,
        };
        assert_eq!(error_type(&thrown), "conversion_error", "{needle}");
        let message = get(&thrown, "message").as_string().unwrap_or_default();
        assert!(message.contains(needle), "{needle}: {message}");
    }
    // A shared, acyclic object is data, and `undefined` fields are left out.
    let shared =
        js_sys::eval("(() => { const s = { k: 1 }; return { a: s, b: s, u: undefined }; })()")
            .expect("eval");
    let handle: JsValue = element("gauge02b".into(), Some(shared), None)
        .expect("a shared object is fine")
        .into();
    let props = get(&handle, "props");
    assert_eq!(
        js_sys::JSON::stringify(&props)
            .ok()
            .and_then(|s| s.as_string())
            .as_deref(),
        Some(r#"{"a":{"k":1},"b":{"k":1}}"#)
    );
}

/// UIDEF05 — an element of a type not yet defined (a restored tree) renders a placeholder, and
/// its markup once the type is defined.
#[wasm_bindgen_test]
fn uidef05_restored_element_renders_after_define() {
    let saved = serde_json::to_string(
        &(Box::new(DelegatedElement::new(
            "gauge05",
            serde_json::json!({ "value": 9 }),
        )) as Box<dyn UIElement>),
    )
    .expect("serialize");
    let restored: Box<dyn UIElement> = serde_json::from_str(&saved).expect("restore");
    let restored: Arc<dyn UIElement> = Arc::from(restored);
    let handle: JsValue = liquers_web::ui::LiquersUIElement::new(restored).into();
    let html = render_html(&handle);
    assert!(
        html.contains("lq-undefined") && html.contains("gauge05"),
        "{html}"
    );
    define_element(definition("gauge05", "(p) => `<b>${p.value}</b>`").into()).expect("define");
    let html = render_html(&handle);
    assert!(
        html.contains("<b>9</b>") && !html.contains("lq-undefined"),
        "{html}"
    );
    undefine_element("gauge05".into());
}

/// UIDEF06 — undefining a type releases its functions.
#[cfg(feature = "debug-handles")]
#[wasm_bindgen_test]
fn uidef06_dispose_releases_callbacks() {
    use liquers_web::command::adapter::live_handle_count;
    let baseline = live_handle_count();
    let def = definition("gauge06", "() => ''");
    set(&def, "onEvent", &func("(p) => p").into());
    set(&def, "mount", &func("() => {}").into());
    define_element(def.into()).expect("define");
    assert_eq!(
        live_handle_count(),
        baseline + 3,
        "render, onEvent and mount"
    );
    define_element(definition("gauge06", "() => 'again'").into()).expect("redefine");
    assert_eq!(
        live_handle_count(),
        baseline + 1,
        "redefining releases the replaced functions"
    );
    assert!(undefine_element("gauge06".into()));
    assert_eq!(live_handle_count(), baseline);
}

/// A page-supplied title is the element's title; `element`'s own title wins over it.
#[wasm_bindgen_test]
fn uidef_title_from_definition_or_element() {
    let def = definition("titled", "() => ''");
    set(&def, "title", &JsValue::from_str("Gauge"));
    define_element(def.into()).expect("define");
    let a: JsValue = element("titled".into(), None, None)
        .expect("element")
        .into();
    assert_eq!(get(&a, "title").as_string().as_deref(), Some("Gauge"));
    let b: JsValue = element("titled".into(), None, Some("CPU".into()))
        .expect("element")
        .into();
    assert_eq!(get(&b, "title").as_string().as_deref(), Some("CPU"));
    // Other elements have no kind or props.
    let other: JsValue =
        liquers_web::ui::LiquersUIElement::new(Arc::new(liquers_lib::ui::Placeholder::new()))
            .into();
    assert!(get(&other, "kind").is_undefined());
    assert!(get(&other, "props").is_undefined());
    undefine_element("titled".into());
}
