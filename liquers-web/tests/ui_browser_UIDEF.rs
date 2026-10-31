//! `UIDEF` tests that need a real DOM: events on page-defined elements, their `mount` lifecycle,
//! the DOM against server-side rendering, the page renderer, and calls back into the mount.
//!
//! Behind the `browser-tests` feature, like `ui_browser_UIUSE.rs`:
//!
//! ```text
//! CHROMEDRIVER=$(which chromedriver) cargo test -p liquers-web --target wasm32-unknown-unknown \
//!   --features browser-tests,debug-handles --test ui_browser_UIDEF
//! ```

#![cfg(all(target_arch = "wasm32", feature = "browser-tests"))]

mod common;
use common::{arg, args, decl_with_params, fresh, func, get, obj, set, sleep, with};

use liquers_web::environment::{register_command_on, reset_global, shared_env};
use liquers_web::ui_define::{define_element, element, undefine_element};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_browser);

fn document() -> web_sys::Document {
    web_sys::window()
        .and_then(|w| w.document())
        .expect("a document")
}

fn container() -> web_sys::Element {
    let doc = document();
    let div = doc.create_element("div").expect("div");
    doc.body()
        .expect("body")
        .append_child(&div)
        .expect("attach");
    div
}

fn options(query: &str) -> JsValue {
    let o = obj();
    set(&o, "query", &JsValue::from_str(query));
    o.into()
}

async fn mount(app: &web_sys::Element, query: &str) -> JsValue {
    JsFuture::from(liquers_web::ui::mount_ui(
        app.clone().into(),
        Some(options(query)),
    ))
    .await
    .expect("mount")
}

fn call(object: &JsValue, method: &str, arguments: &[JsValue]) -> Result<JsValue, JsValue> {
    let f: js_sys::Function = get(object, method).unchecked_into();
    let list = js_sys::Array::new();
    for a in arguments {
        list.push(a);
    }
    f.apply(object, &list)
}

async fn until(label: &str, mut done: impl FnMut() -> bool) {
    for _ in 0..60 {
        if done() {
            return;
        }
        sleep(50).await;
    }
    panic!("timed out waiting for {label}");
}

fn global(name: &str) -> JsValue {
    get(&js_sys::global(), name)
}

fn set_global(name: &str, value: &JsValue) {
    set(js_sys::global().unchecked_ref(), name, value);
}

fn global_log(name: &str) -> Vec<String> {
    let log = global(name);
    if !js_sys::Array::is_array(&log) {
        return Vec::new();
    }
    js_sys::Array::from(&log)
        .iter()
        .map(|v| v.as_string().unwrap_or_else(|| format!("{v:?}")))
        .collect()
}

/// Registers `<name>-v`, a page command that returns `element(kind, { value: v })`.
fn register_element_command(name: &str, kind: &str) {
    let closure = Closure::<dyn Fn(String, JsValue) -> Result<JsValue, JsValue>>::new(
        |kind: String, props: JsValue| element(kind, Some(props), None).map(JsValue::from),
    );
    set_global("__lqElement", closure.as_ref().unchecked_ref());
    closure.forget();
    register_command_on(&with(
        decl_with_params(
            name,
            &["v"],
            &format!("return globalThis.__lqElement('{kind}', {{ value: v }});"),
        ),
        "arguments",
        args(vec![arg("v", Some("int"), None)]),
    ))
    .expect("register the element command");
}

/// Example 1's gauge: a value and a `+` button whose `inc` event adds one. Each event is logged
/// as `handle:kind` in `globalThis.__events`.
fn define_gauge(kind: &str) {
    set_global("__events", &js_sys::Array::new().into());
    let def = obj();
    set(&def, "type", &JsValue::from_str(kind));
    set(
        &def,
        "render",
        &func(
            "(p) => `<span class=\"v\">${p.value}%</span><button data-lq-event=\"inc\">+</button>`",
        )
        .into(),
    );
    set(
        &def,
        "onEvent",
        &func(
            "(p, e) => { globalThis.__events.push(e.handle + ':' + e.kind); \
             return e.kind === 'inc' ? { ...p, value: p.value + 1 } : p; }",
        )
        .into(),
    );
    define_element(def.into()).expect("define the gauge");
}

fn node_text(app: &web_sys::Element, handle: f64) -> String {
    app.query_selector(&format!("#ui-element-{handle} .v"))
        .ok()
        .flatten()
        .and_then(|n| n.text_content())
        .unwrap_or_default()
}

fn click(app: &web_sys::Element, selector: &str) {
    let node: web_sys::HtmlElement = app
        .query_selector(selector)
        .expect("selector")
        .expect("a node to click")
        .unchecked_into();
    node.click();
}

/// UIDEF03 — a click on one instance's markup reaches that instance only, with its own handle.
#[wasm_bindgen_test]
async fn uidef03_event_reaches_only_its_instance() {
    fresh();
    define_gauge("gauge03");
    register_element_command("gauge", "gauge03");
    let app = container();
    let ui = mount(&app, "gauge-50").await;
    until("the first gauge", || app.inner_html().contains("50%")).await;
    let first = get(&call(&ui, "elements", &[]).expect("elements"), "0");
    let first = get(&first, "handle").as_f64().expect("a handle");
    let second = call(&ui, "show", &[JsValue::from_str("gauge-7")])
        .expect("show")
        .as_f64()
        .expect("a handle");
    until("the second gauge", || node_text(&app, second) == "7%").await;

    click(&app, &format!("#ui-element-{first} button"));
    until("the first gauge to read 51%", || {
        node_text(&app, first) == "51%"
    })
    .await;
    assert_eq!(
        node_text(&app, second),
        "7%",
        "the other instance is untouched"
    );
    assert_eq!(global_log("__events"), vec![format!("{first}:inc")]);
    assert!(
        app.inner_html().contains("data-lq-delegate=\"gauge03\""),
        "the wrapper names the type"
    );
    call(&ui, "dispose", &[]).expect("dispose");
    undefine_element("gauge03".into());
    reset_global();
}

/// UIDEF04 — the mounted DOM is the server-side rendering of the same state.
#[wasm_bindgen_test]
async fn uidef04_dom_matches_ssr() {
    fresh();
    define_gauge("gauge04");
    register_element_command("gauge", "gauge04");
    let app = container();
    let envref = shared_env().expect("env");
    let ui = liquers_web::ui::mount_ui_with(app.clone().into(), options("gauge-5"), envref, None)
        .await
        .expect("mount");
    until("the gauge", || app.inner_html().contains("5%")).await;
    sleep(100).await;
    let ssr = liquers_lib::ui::web::render_app_ssr(&ui.app_state())
        .await
        .expect("ssr");
    // The browser normalizes markup it parses; compare both through the same parser.
    let template = document().create_element("div").expect("div");
    template.set_inner_html(&ssr);
    assert_eq!(app.inner_html(), template.inner_html());
    ui.dispose();
    undefine_element("gauge04".into());
    reset_global();
}

/// UIDEF06b — `mount` runs once the node is in the page; its cleanup runs when the element is
/// replaced, and the remaining cleanups when the mount is disposed. The functions are then
/// released.
#[cfg(feature = "debug-handles")]
#[wasm_bindgen_test]
async fn uidef06b_mount_cleanup_runs_on_replace_and_remove() {
    use liquers_web::command::adapter::live_handle_count;
    fresh();
    register_element_command("spark", "spark06b");
    // Earlier tests' driver loops release their environments (and those commands) at their next
    // tick; let them finish before counting.
    sleep(100).await;
    let baseline = live_handle_count();
    set_global("__log", &js_sys::Array::new().into());
    let def = obj();
    set(&def, "type", &JsValue::from_str("spark06b"));
    set(
        &def,
        "render",
        &func("(p) => `<canvas data-v=\"${p.value}\" width=\"80\" height=\"20\"></canvas>`").into(),
    );
    set(
        &def,
        "mount",
        &func(
            "(node, p) => { globalThis.__log.push('mount ' + p.value + ' ' + \
             (node.isConnected && node.querySelector('canvas') !== null)); \
             return () => globalThis.__log.push('cleanup ' + p.value); }",
        )
        .into(),
    );
    define_element(def.into()).expect("define");
    let app = container();
    let ui = mount(&app, "spark-1").await;
    until("the first mount", || {
        global_log("__log") == vec!["mount 1 true"]
    })
    .await;
    let first = get(
        &get(&call(&ui, "elements", &[]).expect("elements"), "0"),
        "handle",
    );
    call(&ui, "show", &[JsValue::from_str("spark-2"), first]).expect("replace");
    until("the replacement", || global_log("__log").len() == 3).await;
    assert_eq!(
        global_log("__log"),
        vec!["mount 1 true", "cleanup 1", "mount 2 true"]
    );
    call(&ui, "show", &[JsValue::from_str("spark-3")]).expect("a second element");
    until("the second element", || global_log("__log").len() == 4).await;
    call(&ui, "dispose", &[]).expect("dispose");
    let mut log = global_log("__log");
    log[4..].sort();
    assert_eq!(&log[4..], &["cleanup 2", "cleanup 3"], "{log:?}");
    assert!(undefine_element("spark06b".into()));
    drop(ui);
    assert_eq!(
        live_handle_count(),
        baseline,
        "every page function released"
    );
    reset_global();
}

/// A `mount` that throws is reported, and the element keeps its markup.
#[wasm_bindgen_test]
async fn uidef06c_throwing_mount_keeps_markup() {
    fresh();
    let def = obj();
    set(&def, "type", &JsValue::from_str("bad06c"));
    set(&def, "render", &func("(p) => `<i>${p.value}</i>`").into());
    set(
        &def,
        "mount",
        &func("() => { throw new Error('no'); }").into(),
    );
    define_element(def.into()).expect("define");
    register_element_command("bad", "bad06c");
    let app = container();
    let ui = mount(&app, "bad-4").await;
    until("the markup", || app.inner_html().contains("<i>4</i>")).await;
    sleep(100).await;
    assert!(app.inner_html().contains("<i>4</i>"));
    call(&ui, "dispose", &[]).expect("dispose");
    undefine_element("bad06c".into());
    reset_global();
}

/// The calls a recording renderer received, as `[changes, tree]` pairs.
fn recording_renderer() -> (js_sys::Array, JsValue) {
    let calls = js_sys::Array::new();
    let make = func(
        "(calls) => ({ render(tree, changes) { calls.push([changes, JSON.parse(JSON.stringify(tree))]); }, \
         dispose() { calls.push(['disposed', null]); } })",
    );
    let renderer = make.call1(&JsValue::NULL, &calls).expect("renderer");
    (calls, renderer)
}

fn element_in(tree: &JsValue, handle: f64) -> JsValue {
    get(&get(tree, "elements"), &format!("{handle}"))
}

/// UIDEF07 — a page renderer receives the whole tree first, then what changed; `dispatchEvent`
/// reaches an element.
#[wasm_bindgen_test]
async fn uidef07_renderer_receives_tree_and_changes() {
    fresh();
    define_gauge("gauge07");
    register_element_command("gauge", "gauge07");
    let (calls, renderer) = recording_renderer();
    let ui = JsFuture::from(liquers_web::ui::mount_renderer(
        renderer,
        Some(options("gauge-50")),
    ))
    .await
    .expect("mountRenderer");

    let last_tree =
        |calls: &js_sys::Array| js_sys::Array::from(&calls.get(calls.length() - 1)).get(1);
    until("the gauge in the tree", || {
        calls.length() > 0
            && get(&element_in(&last_tree(&calls), 1.0), "kind")
                .as_string()
                .as_deref()
                == Some("gauge07")
    })
    .await;
    let first = js_sys::Array::from(&calls.get(0));
    assert_eq!(
        first.get(0).as_string().as_deref(),
        Some("all"),
        "the first call"
    );
    let tree = last_tree(&calls);
    let roots: Vec<f64> = js_sys::Array::from(&get(&tree, "roots"))
        .iter()
        .filter_map(|v| v.as_f64())
        .collect();
    assert_eq!(roots, vec![1.0]);
    let gauge = element_in(&tree, 1.0);
    assert_eq!(
        get(&gauge, "typeName").as_string().as_deref(),
        Some("DelegatedElement")
    );
    assert!(get(&gauge, "html")
        .as_string()
        .unwrap_or_default()
        .contains("50%"));
    assert_eq!(get(&get(&gauge, "props"), "value").as_f64(), Some(50.0));

    let before = calls.length();
    let second = call(&ui, "show", &[JsValue::from_str("gauge-7")])
        .expect("show")
        .as_f64()
        .expect("a handle");
    until("an inserted record", || {
        (before..calls.length()).any(|i| {
            let changes = js_sys::Array::from(&calls.get(i)).get(0);
            js_sys::Array::is_array(&changes)
                && js_sys::Array::from(&changes)
                    .iter()
                    .any(|c| get(&c, "inserted").as_f64() == Some(second))
        })
    })
    .await;

    call(
        &ui,
        "dispatchEvent",
        &[JsValue::from_f64(1.0), JsValue::from_str("inc")],
    )
    .expect("dispatchEvent");
    until("the gauge at 51", || {
        get(&get(&element_in(&last_tree(&calls), 1.0), "props"), "value").as_f64() == Some(51.0)
    })
    .await;
    assert_eq!(global_log("__events"), vec!["1:inc".to_string()]);
    assert!(
        call(
            &ui,
            "dispatchEvent",
            &[JsValue::from_f64(1.0), JsValue::from_str("")]
        )
        .is_err(),
        "an empty kind is refused"
    );

    call(&ui, "dispose", &[]).expect("dispose");
    let last = js_sys::Array::from(&calls.get(calls.length() - 1));
    assert_eq!(last.get(0).as_string().as_deref(), Some("disposed"));
    undefine_element("gauge07".into());
    reset_global();
}

/// `mountRenderer` without a `render` function is refused.
#[wasm_bindgen_test]
async fn uidef07b_renderer_without_render_is_refused() {
    fresh();
    let result = JsFuture::from(liquers_web::ui::mount_renderer(obj().into(), None)).await;
    let thrown = result.expect_err("refused");
    assert!(get(&thrown, "message")
        .as_string()
        .unwrap_or_default()
        .contains("render"));
    reset_global();
}

/// UIDEF08 — `show` from inside `render` is refused (the UI lock is held); `submit` is a message
/// and works.
#[wasm_bindgen_test]
async fn uidef08_reentry_from_render_is_deferred_or_refused() {
    fresh();
    set_global("__ui", &JsValue::UNDEFINED);
    set_global("__r", &js_sys::Array::new().into());
    let def = obj();
    set(&def, "type", &JsValue::from_str("reenter08"));
    set(
        &def,
        "render",
        &func(
            "(p) => { const ui = globalThis.__ui; if (ui && globalThis.__r.length === 0) { \
             try { ui.show('hello'); globalThis.__r.push('shown'); } \
             catch (e) { globalThis.__r.push('show: ' + e.message); } \
             try { ui.submit('hello'); globalThis.__r.push('submitted'); } \
             catch (e) { globalThis.__r.push('submit: ' + e.message); } } \
             return `<b>${p.value}</b>`; }",
        )
        .into(),
    );
    define_element(def.into()).expect("define");
    register_element_command("re", "reenter08");
    register_command_on(&decl_with_params("hello", &[], "return 'hi';")).expect("hello");
    let app = container();
    let ui = mount(&app, "re-1").await;
    set_global("__ui", &ui);
    // Redefining re-renders the element, now with the mount in reach.
    until("the element", || app.inner_html().contains("<b>1</b>")).await;
    crate_invalidate(&ui);
    until("the re-entrant render", || global_log("__r").len() == 2).await;
    let r = global_log("__r");
    assert!(r[0].starts_with("show: ") && r[0].contains("busy"), "{r:?}");
    assert_eq!(r[1], "submitted");
    assert_eq!(get(&ui, "running").as_bool(), Some(true));
    call(&ui, "dispose", &[]).expect("dispose");
    set_global("__ui", &JsValue::UNDEFINED);
    undefine_element("reenter08".into());
    reset_global();
}

/// Forces a full re-render of `ui` by showing nothing new: redefining a type invalidates every
/// live mount.
fn crate_invalidate(_ui: &JsValue) {
    let def = obj();
    set(&def, "type", &JsValue::from_str("__invalidate"));
    set(&def, "render", &func("() => ''").into());
    define_element(def.into()).expect("define");
    undefine_element("__invalidate".into());
}

/// UIDEF08b — `onEvent` may call back into the mount (the lock is free), but not replace its own
/// element.
#[wasm_bindgen_test]
async fn uidef08b_on_event_may_call_back() {
    fresh();
    set_global("__r", &js_sys::Array::new().into());
    let def = obj();
    set(&def, "type", &JsValue::from_str("cb08b"));
    set(
        &def,
        "render",
        &func(
            "(p) => `<span class=\"v\">${p.value}</span><button data-lq-event=\"go\">go</button>`",
        )
        .into(),
    );
    set(
        &def,
        "onEvent",
        &func(
            "(p, e) => { const ui = globalThis.__ui; \
             try { globalThis.__r.push('other ' + typeof ui.show('cb-9')); } \
             catch (err) { globalThis.__r.push('other failed: ' + err.message); } \
             try { ui.show('cb-8', e.handle); globalThis.__r.push('own shown'); } \
             catch (err) { globalThis.__r.push('own: ' + err.message); } \
             return { value: p.value + 1 }; }",
        )
        .into(),
    );
    define_element(def.into()).expect("define");
    register_element_command("cb", "cb08b");
    let app = container();
    let ui = mount(&app, "cb-1").await;
    set_global("__ui", &ui);
    until("the element", || node_text(&app, 1.0) == "1").await;
    click(&app, "#ui-element-1 button");
    until("the update", || node_text(&app, 1.0) == "2").await;
    let r = global_log("__r");
    assert_eq!(r[0], "other number", "{r:?}");
    assert!(
        r[1].starts_with("own: ") && r[1].contains("being updated"),
        "{r:?}"
    );
    until("the other element", || app.inner_html().contains(">9<")).await;
    call(&ui, "dispose", &[]).expect("dispose");
    set_global("__ui", &JsValue::UNDEFINED);
    undefine_element("cb08b".into());
    reset_global();
}

/// A type defined from inside a render (the UI lock held) still re-renders its elements, on the
/// next tick.
#[wasm_bindgen_test]
async fn uidef01e_define_from_inside_render_rerenders() {
    fresh();
    let closure = Closure::<dyn Fn(JsValue) -> Result<(), JsValue>>::new(define_element);
    set_global("__lqDefine", closure.as_ref().unchecked_ref());
    closure.forget();
    set_global("__redefine", &JsValue::FALSE);
    let b = obj();
    set(&b, "type", &JsValue::from_str("b01e"));
    set(&b, "render", &func("() => '<u>B1</u>'").into());
    define_element(b.into()).expect("define b");
    let a = obj();
    set(&a, "type", &JsValue::from_str("a01e"));
    set(
        &a,
        "render",
        &func(
            "() => { if (globalThis.__redefine) { globalThis.__redefine = false; \
             globalThis.__lqDefine({ type: 'b01e', render: () => '<u>B2</u>' }); } \
             return '<i>A</i>'; }",
        )
        .into(),
    );
    define_element(a.into()).expect("define a");
    register_element_command("b", "b01e");
    register_element_command("a", "a01e");
    let app = container();
    let ui = mount(&app, "b-1").await;
    call(&ui, "show", &[JsValue::from_str("a-1")]).expect("show a");
    until("both elements", || {
        let html = app.inner_html();
        html.contains("<u>B1</u>") && html.contains("<i>A</i>")
    })
    .await;
    // b renders before a; a's render then redefines b while the lock is held.
    set_global("__redefine", &JsValue::TRUE);
    crate_invalidate(&ui);
    until("b redrawn with its new definition", || {
        app.inner_html().contains("<u>B2</u>")
    })
    .await;
    call(&ui, "dispose", &[]).expect("dispose");
    undefine_element("a01e".into());
    undefine_element("b01e".into());
    reset_global();
}
