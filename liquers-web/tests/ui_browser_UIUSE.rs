//! `UIUSE` tests that need a real DOM: `mountUI` in a page.
//!
//! Behind the `browser-tests` feature, like `store_local_STORE.rs`:
//!
//! ```text
//! CHROMEDRIVER=$(which chromedriver) cargo test -p liquers-web --target wasm32-unknown-unknown \
//!   --features browser-tests,debug-handles --test ui_browser_UIUSE
//! ```
//!
//! Each test mounts into a fresh `<div>` and drives the mount the way a page does: through the
//! exported methods, called by name.

#![cfg(all(target_arch = "wasm32", feature = "browser-tests"))]

mod common;
use common::{arg, args, fresh, func, get, obj, register_fixture_commands, set, sleep, with};

use liquers_web::environment::{register_command_on, reset_global};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

fn document() -> web_sys::Document {
    web_sys::window()
        .and_then(|w| w.document())
        .expect("a document")
}

/// A fresh, attached container.
fn container() -> web_sys::Element {
    let doc = document();
    let div = doc.create_element("div").expect("div");
    doc.body()
        .expect("body")
        .append_child(&div)
        .expect("attach");
    div
}

fn options(key: &str, value: JsValue) -> JsValue {
    let o = obj();
    set(&o, key, &value);
    o.into()
}

async fn mount(app: &web_sys::Element, options: JsValue) -> Result<JsValue, JsValue> {
    JsFuture::from(liquers_web::ui::mount_ui(app.clone().into(), Some(options))).await
}

/// Calls `object.method(...args)` as page code would.
fn call(object: &JsValue, method: &str, arguments: &[JsValue]) -> Result<JsValue, JsValue> {
    let f: js_sys::Function = get(object, method).unchecked_into();
    let list = js_sys::Array::new();
    for a in arguments {
        list.push(a);
    }
    f.apply(object, &list)
}

/// Waits (in 50 ms ticks, up to about 3 s) until `done` holds.
async fn until(label: &str, mut done: impl FnMut() -> bool) {
    for _ in 0..60 {
        if done() {
            return;
        }
        sleep(50).await;
    }
    panic!("timed out waiting for {label}");
}

/// A status subscription that records `[handle, status]` pairs in a JavaScript array.
fn recorder() -> (js_sys::Array, js_sys::Function) {
    let events = js_sys::Array::new();
    let push = func(
        "(events) => (e) => events.push([e.handle, e.status, e.error ? e.error.message : null])",
    );
    let callback: js_sys::Function = push
        .call1(&JsValue::NULL, &events)
        .expect("recorder")
        .unchecked_into();
    (events, callback)
}

fn pairs(events: &js_sys::Array) -> Vec<(Option<f64>, String)> {
    events
        .iter()
        .map(|e| {
            let e = js_sys::Array::from(&e);
            (e.get(0).as_f64(), e.get(1).as_string().unwrap_or_default())
        })
        .collect()
}

const CONSOLE: &str = "hello/q/ns-lui/query_console";

/// UIUSE01 — a mount shows a query's value, or a spec.
#[wasm_bindgen_test]
async fn uiuse01_mount_from_query_and_spec() {
    fresh();
    register_fixture_commands();
    let app = container();
    let ui = mount(&app, options("query", JsValue::from_str(CONSOLE)))
        .await
        .expect("mount the console");
    until("the console", || app.inner_html().contains("ui-element-")).await;
    assert_eq!(get(&ui, "running").as_bool(), Some(true));
    call(&ui, "dispose", &[]).expect("dispose");

    let app2 = container();
    let spec = "layout: !vertical null\ninit:\n  - \"hello/ns-lui/add-child\"\n";
    let ui2 = mount(&app2, options("spec", JsValue::from_str(spec)))
        .await
        .expect("mount a spec");
    until("the spec's child", || {
        app2.inner_html().contains("Hello, world!")
    })
    .await;
    call(&ui2, "dispose", &[]).expect("dispose");
    reset_global();
}

/// UIUSE01b — a spec passed as an object, with a button that runs a query.
#[wasm_bindgen_test]
async fn uiuse01b_object_spec_with_menu() {
    fresh();
    register_fixture_commands();
    let yaml = "menu:\n  items:\n  - !button\n    label: Probe\n    action:\n      query: \"hello/ns-lui/add-child\"\nlayout: !vertical null\n";
    let spec = liquers_lib::ui::widgets::ui_spec_element::UISpec::from_yaml(yaml).expect("yaml");
    let json = serde_json::to_string(&spec).expect("json");
    let object = js_sys::JSON::parse(&json).expect("object");
    let app = container();
    let ui = mount(&app, options("spec", object))
        .await
        .expect("mount an object spec");
    until("the button", || {
        app.query_selector("[data-lq-action]")
            .ok()
            .flatten()
            .is_some()
    })
    .await;
    let button: web_sys::HtmlElement = app
        .query_selector("[data-lq-action]")
        .expect("query")
        .expect("button")
        .unchecked_into();
    button.click();
    until("the added child", || {
        app.inner_html().contains("Hello, world!")
    })
    .await;
    call(&ui, "dispose", &[]).expect("dispose");
    reset_global();
}

/// UIUSE03 — a control's action reaches a page command, whose context names the element.
#[wasm_bindgen_test]
async fn uiuse03_action_reaches_command_with_element() {
    fresh();
    let seen = js_sys::Array::new();
    let run = func("(seen) => (context) => { seen.push(context.element); return 'ok'; }")
        .call1(&JsValue::NULL, &seen)
        .expect("run");
    let spec = obj();
    set(&spec, "name", &JsValue::from_str("action_probe"));
    set(&spec, "run", &run);
    let spec = with(
        spec.into(),
        "arguments",
        args(vec![arg("context", None, None)]),
    );
    register_command_on(&spec).expect("register");

    let yaml = "menu:\n  items:\n  - !button\n    label: Probe\n    action:\n      query: \"action_probe\"\nlayout: !vertical null\n";
    let app = container();
    let ui = mount(&app, options("spec", JsValue::from_str(yaml)))
        .await
        .expect("mount");
    until("the button", || {
        app.query_selector("[data-lq-action]")
            .ok()
            .flatten()
            .is_some()
    })
    .await;
    let button: web_sys::HtmlElement = app
        .query_selector("[data-lq-action]")
        .expect("query")
        .expect("button")
        .unchecked_into();
    button.click();
    until("the probe", || seen.length() > 0).await;
    let root = js_sys::Array::from(&call(&ui, "elements", &[]).expect("elements")).get(0);
    assert_eq!(
        seen.get(0).as_f64(),
        get(&root, "handle").as_f64(),
        "context.element is the spec element's handle, as a number"
    );
    call(&ui, "dispose", &[]).expect("dispose");
    reset_global();
}

/// UIUSE04 — status events arrive in order, after the page was updated.
#[wasm_bindgen_test]
async fn uiuse04_status_events_in_order() {
    fresh();
    register_fixture_commands();
    let app = container();
    let ui = mount(&app, JsValue::UNDEFINED).await.expect("mount empty");
    let (events, callback) = recorder();
    call(&ui, "on", &[JsValue::from_str("status"), callback.into()]).expect("on");
    let h = call(&ui, "show", &[JsValue::from_str("hello")])
        .expect("show")
        .as_f64()
        .expect("a number");
    until("ready", || pairs(&events).iter().any(|(_, s)| s == "ready")).await;
    assert_eq!(
        pairs(&events),
        vec![
            (Some(h), "processing".to_string()),
            (Some(h), "ready".to_string())
        ]
    );
    assert!(
        app.inner_html().contains("Hello, world!"),
        "painted before ready"
    );
    call(&ui, "dispose", &[]).expect("dispose");
    reset_global();
}

/// UIUSE04b — a throwing callback does not stop delivery to the others.
#[wasm_bindgen_test]
async fn uiuse04b_throwing_callback_does_not_stop_delivery() {
    fresh();
    register_fixture_commands();
    let app = container();
    let ui = mount(&app, JsValue::UNDEFINED).await.expect("mount");
    let thrower = func("() => { throw new Error('subscriber failure'); }");
    call(&ui, "on", &[JsValue::from_str("status"), thrower.into()]).expect("on thrower");
    let (events, callback) = recorder();
    call(&ui, "on", &[JsValue::from_str("status"), callback.into()]).expect("on recorder");
    call(&ui, "show", &[JsValue::from_str("hello")]).expect("show");
    until("ready", || pairs(&events).iter().any(|(_, s)| s == "ready")).await;
    assert_eq!(get(&ui, "running").as_bool(), Some(true));
    call(&ui, "dispose", &[]).expect("dispose");
    reset_global();
}

/// UIUSE05 — disposing releases the page's callbacks.
#[cfg(feature = "debug-handles")]
#[wasm_bindgen_test]
async fn uiuse05_dispose_releases_callbacks() {
    fresh();
    let before = liquers_web::ui::live_subscription_count();
    let app = container();
    let ui = mount(&app, JsValue::UNDEFINED).await.expect("mount");
    let (_, callback) = recorder();
    let sub = call(
        &ui,
        "on",
        &[JsValue::from_str("status"), callback.clone().into()],
    )
    .expect("on");
    call(&ui, "on", &[JsValue::from_str("status"), callback.into()]).expect("on twice");
    assert_eq!(liquers_web::ui::live_subscription_count(), before + 2);
    call(&sub, "dispose", &[]).expect("sub.dispose");
    assert_eq!(liquers_web::ui::live_subscription_count(), before + 1);
    call(&ui, "dispose", &[]).expect("ui.dispose");
    assert_eq!(liquers_web::ui::live_subscription_count(), before);
    assert_eq!(app.inner_html(), "", "the container is emptied");
    assert_eq!(get(&ui, "running").as_bool(), Some(false));
    reset_global();
}

/// UIUSE06 — a result whose element was replaced meanwhile is ignored.
#[wasm_bindgen_test]
async fn uiuse06_stale_result_is_ignored() {
    fresh();
    register_fixture_commands();
    // `slowa` waits for a promise the test resolves; while it waits, `show` replaces the element.
    let gate = func(
        "() => { let open; const p = new Promise((r) => { open = r; }); return { p, open }; }",
    )
    .call0(&JsValue::NULL)
    .expect("gate");
    let run = func("(gate) => async () => { await gate.p; return 'stale A'; }")
        .call1(&JsValue::NULL, &gate)
        .expect("run");
    let spec = obj();
    set(&spec, "name", &JsValue::from_str("slowa"));
    set(&spec, "run", &run);
    set(&spec, "async", &JsValue::TRUE);
    register_command_on(&spec.into()).expect("register slowa");

    let app = container();
    let ui = mount(&app, JsValue::UNDEFINED).await.expect("mount");
    let h = call(&ui, "show", &[JsValue::from_str("slowa")]).expect("show A");
    sleep(100).await; // A is now awaiting the gate, inside the driver.
    call(&ui, "show", &[JsValue::from_str("hello"), h.clone()]).expect("show B instead");
    let open: js_sys::Function = get(&gate, "open").unchecked_into();
    open.call0(&JsValue::NULL).expect("open the gate");
    until("B", || app.inner_html().contains("Hello, world!")).await;
    sleep(200).await;
    assert!(
        !app.inner_html().contains("stale A"),
        "A's result must not overwrite B: {}",
        app.inner_html()
    );
    call(&ui, "dispose", &[]).expect("dispose");
    reset_global();
}

/// UIUSE07b — a failing element reports an `error` event carrying the error.
#[wasm_bindgen_test]
async fn uiuse07b_failed_element_reports_error_event() {
    fresh();
    let app = container();
    let ui = mount(&app, JsValue::UNDEFINED).await.expect("mount");
    let (events, callback) = recorder();
    call(&ui, "on", &[JsValue::from_str("status"), callback.into()]).expect("on");
    call(&ui, "show", &[JsValue::from_str("nosuchcommand")]).expect("show");
    until("error", || pairs(&events).iter().any(|(_, s)| s == "error")).await;
    let last = js_sys::Array::from(&events.get(events.length() - 1));
    assert!(last.get(2).as_string().is_some(), "the error is carried");
    call(&ui, "dispose", &[]).expect("dispose");
    reset_global();
}

/// UIUSE08b — a callback may dispose the mount; no later callback runs.
#[wasm_bindgen_test]
async fn uiuse08b_dispose_from_inside_a_callback() {
    fresh();
    register_fixture_commands();
    let app = container();
    let ui = mount(&app, JsValue::UNDEFINED).await.expect("mount");
    let count = js_sys::Array::new();
    let cb = func("(ui, count) => (e) => { count.push(e.status); ui.dispose(); }")
        .call2(&JsValue::NULL, &ui, &count)
        .expect("cb");
    call(&ui, "on", &[JsValue::from_str("status"), cb]).expect("on");
    call(&ui, "show", &[JsValue::from_str("hello")]).expect("show");
    until("the first event", || count.length() > 0).await;
    sleep(200).await;
    assert_eq!(count.length(), 1, "no callback after dispose");
    assert_eq!(get(&ui, "running").as_bool(), Some(false));
    reset_global();
}

/// UIUSE08c — calls on a disposed mount are refused.
#[wasm_bindgen_test]
async fn uiuse08c_calls_after_dispose_reject() {
    fresh();
    let app = container();
    let ui = mount(&app, JsValue::UNDEFINED).await.expect("mount");
    call(&ui, "dispose", &[]).expect("dispose");
    call(&ui, "dispose", &[]).expect("dispose is idempotent");
    let thrown = call(&ui, "show", &[JsValue::from_str("hello")]).expect_err("show refused");
    let message = get(&thrown, "message").as_string().unwrap_or_default();
    assert!(message.contains("disposed"), "{message}");
    reset_global();
}

/// UIUSE08d — nothing is painted into the container after `dispose()`, even when a query was
/// awaiting at the time.
#[wasm_bindgen_test]
async fn uiuse08d_dispose_while_a_query_awaits() {
    fresh();
    let run = func("async () => { await new Promise((r) => setTimeout(r, 200)); return 'late'; }");
    let spec = obj();
    set(&spec, "name", &JsValue::from_str("latecomer"));
    set(&spec, "run", &run.into());
    set(&spec, "async", &JsValue::TRUE);
    register_command_on(&spec.into()).expect("register");
    let app = container();
    let ui = mount(&app, JsValue::UNDEFINED).await.expect("mount");
    call(&ui, "show", &[JsValue::from_str("latecomer")]).expect("show");
    sleep(60).await;
    call(&ui, "dispose", &[]).expect("dispose");
    sleep(400).await;
    assert_eq!(app.inner_html(), "", "nothing painted after dispose");
    reset_global();
}

/// UIUSE09 — a mount follows a rebuilt environment: a command registered after mounting works.
#[wasm_bindgen_test]
async fn uiuse09_mount_follows_rebuild() {
    fresh();
    register_fixture_commands();
    let app = container();
    let ui = mount(&app, options("query", JsValue::from_str("hello")))
        .await
        .expect("mount");
    until("hello", || app.inner_html().contains("Hello, world!")).await;
    register_command_on(&common::decl("late_command", "return 'registered late';"))
        .expect("register after mounting");
    call(&ui, "show", &[JsValue::from_str("late_command")]).expect("show");
    until("the late command", || {
        app.inner_html().contains("registered late")
    })
    .await;
    call(&ui, "dispose", &[]).expect("dispose");
    reset_global();
}

/// UIUSE10 — `show` adds a root and replaces an element; handles are numbers.
#[wasm_bindgen_test]
async fn uiuse10_show_adds_and_replaces() {
    fresh();
    register_fixture_commands();
    let app = container();
    let ui = mount(&app, JsValue::UNDEFINED).await.expect("mount");
    let h = call(&ui, "show", &[JsValue::from_str("hello")]).expect("show");
    assert_eq!(h.js_typeof().as_string().as_deref(), Some("number"));
    until("hello", || app.inner_html().contains("Hello, world!")).await;
    let same = call(&ui, "show", &[JsValue::from_str("hello/shout"), h.clone()]).expect("instead");
    assert_eq!(same.as_f64(), h.as_f64(), "replacing keeps the handle");
    until("shouted", || app.inner_html().contains("HELLO, WORLD!")).await;
    let elements = js_sys::Array::from(&call(&ui, "elements", &[]).expect("elements"));
    assert_eq!(elements.length(), 1);
    assert_eq!(
        get(&elements.get(0), "status").as_string().as_deref(),
        Some("ready")
    );
    let missing = call(
        &ui,
        "show",
        &[JsValue::from_str("hello"), JsValue::from_f64(9999.0)],
    );
    assert!(missing.is_err(), "an unknown handle is refused");
    call(&ui, "dispose", &[]).expect("dispose");
    reset_global();
}
