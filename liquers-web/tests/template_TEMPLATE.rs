//! `TEMPLATE*` conformance tests: templates from JavaScript (`src/template.rs`).
//!
//! The tests of `LANGUAGE-INTEGRATION_GUIDE.md` §TEMPLATE, over the store content listed there:
//! `templates/greet.md` = `Hello, {{.}}!`, `templates/count.md` = `{{$ -R/d/count.txt}}`,
//! `templates/page.md` = `{{> ./header.md}}body`, `templates/header.md` = `Header `, and
//! `d/count.txt` = `1`. Content is written through the asset manager, as a Rust caller (and
//! `liquers-lib`'s own template tests) write it, so a dependent result is invalidated.
//!
//! Commands are JavaScript object literals (`js_sys::eval`), the form a page writes. The suites
//! share one wasm instance and so one singleton; every test starts from `reset_global` +
//! `init_global`.

#![cfg(target_arch = "wasm32")]

use liquers_core::assets::AssetManager;
use liquers_core::metadata::MetadataRecord;
use liquers_core::store_config::StoreRouterConfig;
use liquers_web::environment::{
    configure_store_on, init_global, register_store_object_on, reset_global, shared_env,
};
use liquers_web::objects::{LiquersKey, LiquersQuery};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use wasm_bindgen_test::*;

fn js(source: &str) -> JsValue {
    js_sys::eval(&format!("({source})")).expect("evaluate a JavaScript literal")
}

fn get(v: &JsValue, k: &str) -> JsValue {
    js_sys::Reflect::get(v, &JsValue::from_str(k)).expect("get property")
}

fn json(v: &JsValue) -> String {
    js_sys::JSON::stringify(v)
        .expect("stringify")
        .as_string()
        .expect("a string")
}

fn text(v: &JsValue) -> String {
    v.as_string()
        .unwrap_or_else(|| panic!("expected a string, got {}", json(v)))
}

fn message(error: &JsValue) -> String {
    get(error, "message").as_string().unwrap_or_default()
}

fn query(text: &str) -> JsValue {
    LiquersQuery::parse(text).expect("query").into()
}

/// A writable store object holding everything under `templates/` and `d/`.
const STORE: &str = r#"
(() => {
  const data = new Map();
  return {
    async get(key) { return data.get(key); },
    async set(key, bytes, metadata) { data.set(key, { data: bytes, metadata }); },
    async contains(key) { return data.has(key); },
    isDir(key) { return [...data.keys()].some((k) => k.startsWith(key + "/")); },
    isSupported(key) { return true; },
    async keys() { return [...data.keys()]; },
  };
})()
"#;

async fn set(key: &str, content: &str) {
    let envref = shared_env().expect("env");
    let key = liquers_core::parse::parse_key(key).expect("key");
    let mut record = MetadataRecord {
        type_identifier: "Text".to_string(),
        type_name: "text".to_string(),
        data_format: Some("txt".to_string()),
        ..Default::default()
    };
    record.filename = key.filename().map(|name| name.encode().to_string());
    record.status = liquers_core::metadata::Status::Source;
    envref
        .get_asset_manager()
        .set_binary(&key, content.as_bytes(), record)
        .await
        .expect("write the store");
}

/// A fresh environment with the guide's store content.
async fn fresh() {
    reset_global();
    init_global().expect("init");
    let object = js_sys::eval(STORE)
        .expect("store source")
        .dyn_into::<js_sys::Object>()
        .expect("an object");
    register_store_object_on("templatestore", object).expect("register the store object");
    configure_store_on(
        StoreRouterConfig::from_yaml(
            "stores:\n  - type: js\n    prefix: templates\n    config: { object: templatestore }\n  \
             - type: js\n    prefix: d\n    config: { object: templatestore }\n",
        )
        .expect("config parses"),
    )
    .expect("configure");
    set("templates/greet.md", "Hello, {{.}}!").await;
    set("templates/count.md", "{{$ -R/d/count.txt}}").await;
    set("templates/page.md", "{{> ./header.md}}body").await;
    set("templates/header.md", "Header ").await;
    set("d/count.txt", "1").await;
    // The guide's tests start from `hello` and `number-42`; here they are source commands.
    register(
        r#"[{ name: "hello", run: () => "hello" },
            { name: "number", arguments: [{ name: "n", type: "int" }], run: (n) => n }]"#,
    );
}

fn register(spec: &str) {
    if let Err(e) = liquers_web::register_commands(js(spec).unchecked_into(), None) {
        panic!("register {spec}: {}", message(&e));
    }
}

/// The value, or a panic naming `what` and the error's message.
fn ok(result: Result<JsValue, JsValue>, what: &str) -> JsValue {
    result.unwrap_or_else(|e| panic!("{what}: {} {}", message(&e), json(&e)))
}

async fn evaluate(query: &str) -> Result<JsValue, JsValue> {
    JsFuture::from(liquers_web::evaluate(query)).await
}

async fn evaluate_text(query: &str) -> String {
    text(&evaluate(query).await.unwrap_or_else(|e| {
        panic!("{query} failed: {}", message(&e))
    }))
}

async fn expand(template: &str, data: Option<&str>, options: Option<&str>) -> Result<JsValue, JsValue> {
    JsFuture::from(liquers_web::expand(
        JsValue::from_str(template),
        data.map(js),
        options.map(js),
    ))
    .await
}

/// TEMPLATE01 — the `ns-template` commands are built in.
#[wasm_bindgen_test]
async fn template01_template_commands_are_built_in() {
    fresh().await;
    assert_eq!(
        evaluate_text("hello/ns-template/render-~X~-R/templates/greet.md~E").await,
        "Hello, hello!"
    );
    let diagnostics = ok(
        evaluate("-R/templates/greet.md/-/ns-template/validate").await,
        "validate",
    );
    assert_eq!(json(&diagnostics), "[]");
}

/// TEMPLATE02 — a template command declared from JavaScript renders, and its metadata shows an
/// alias of `template/render`; it is also accepted as a `Query`, and refused beside `run`.
#[wasm_bindgen_test]
async fn template02_declared_template_command_renders() {
    fresh().await;
    register(r#"{ name: "greeting", template: "-R/templates/greet.md", label: "Greeting" }"#);
    assert_eq!(evaluate_text("hello/greeting").await, "Hello, hello!");
    let described = liquers_web::describe_command("greeting").expect("describe");
    let described: JsValue = described.into();
    let definition = json(&get(&described, "definition"));
    assert!(
        definition.contains("Alias") && definition.contains("render") && definition.contains("template"),
        "the definition is an alias of template/render: {definition}"
    );
    let names: Vec<String> = js_sys::Array::from(&get(&described, "arguments"))
        .iter()
        .map(|a| get(&a, "name").as_string().unwrap_or_default())
        .collect();
    assert_eq!(names, ["data", "errors", "cwd"]);

    // A `Query` names the template as well, and `data` reaches the template's names.
    let spec = js(r#"{ name: "named_greeting" }"#);
    js_sys::Reflect::set(&spec, &"template".into(), &query("-R/templates/greet.md")).expect("set");
    liquers_web::register_commands(spec.unchecked_into(), None).expect("register a Query template");
    assert_eq!(evaluate_text("hello/named_greeting").await, "Hello, hello!");

    let refused = liquers_web::register_commands(
        js(r#"{ name: "both", template: "-R/templates/greet.md", run: () => "x" }"#).unchecked_into(),
        None,
    );
    let error = refused.err().expect("`run` beside `template` is refused");
    assert!(message(&error).contains("`run`"), "{}", message(&error));
}

/// TEMPLATE03 — editing a template command's template invalidates its result.
#[wasm_bindgen_test]
async fn template03_editing_the_template_invalidates_results() {
    fresh().await;
    register(r#"{ name: "greeting", template: "-R/templates/greet.md" }"#);
    assert_eq!(evaluate_text("hello/greeting").await, "Hello, hello!");
    set("templates/greet.md", "Hi, {{.}}.").await;
    assert_eq!(evaluate_text("hello/greeting").await, "Hi, hello.");
}

/// TEMPLATE04 — top-level `expand` over a JavaScript object, with a query tag.
#[wasm_bindgen_test]
async fn template04_top_level_expand_over_an_object() {
    fresh().await;
    let out = ok(expand("{{name}} has {{$ number-42}}", Some(r#"{ name: "Ann" }"#), None)
        .await, "expand");
    assert_eq!(text(&out), "Ann has 42");

    // The same call on an Environment handle.
    let env = liquers_web::environment::LiquersEnvironment::global().expect("global");
    let out = ok(
        JsFuture::from(env.expand(JsValue::from_str("{{name}}!"), Some(js(r#"{ name: "Bo" }"#)), None))
            .await,
        "Environment.expand",
    );
    assert_eq!(text(&out), "Bo!");
}

/// TEMPLATE05 — programmatic `render` and `expand` give the text of the equivalent command query.
#[wasm_bindgen_test]
async fn template05_programmatic_call_matches_the_command() {
    fresh().await;
    let by_query = evaluate_text("hello/ns-template/render-~X~-R/templates/greet.md~E").await;
    let rendered = ok(JsFuture::from(liquers_web::render(
        query("-R/templates/greet.md"),
        Some(JsValue::from_str("hello")),
        None,
        None,
    ))
    .await, "render");
    assert_eq!(text(&rendered), by_query);

    let template = evaluate_text("-R/templates/greet.md").await;
    let expanded = ok(expand(&template, None, None).await, "expand");
    assert_eq!(
        text(&expanded),
        evaluate_text("-R/templates/greet.md/-/ns-template/expand").await
    );
}

/// TEMPLATE06 — a render inside a JavaScript command makes the template's queries dependencies of
/// the command's result.
#[wasm_bindgen_test]
async fn template06_in_command_render_records_dependencies() {
    fresh().await;
    js_sys::Reflect::set(&js_sys::global(), &"__template06".into(), &query("-R/templates/count.md"))
        .expect("set global");
    register(r#"{ name: "report", async: true,
                  run: async (context) => await context.render(globalThis.__template06) }"#);
    assert_eq!(evaluate_text("report/out.txt").await, "1");
    set("d/count.txt", "2").await;
    assert_eq!(evaluate_text("report/out.txt").await, "2");
}

/// TEMPLATE07 — `strict` throws a `LiquersError` with the template position; a recovering mode
/// returns the text with the warnings reachable.
#[wasm_bindgen_test]
async fn template07_strict_throws_with_position_recovering_mode_warns() {
    fresh().await;
    let error = expand("ok\n{{$ missing_command}}", None, None)
        .await
        .err()
        .expect("strict throws");
    let position = get(&error, "position");
    assert_eq!(
        (get(&position, "line").as_f64(), get(&position, "column").as_f64()),
        (Some(2.0), Some(1.0)),
        "{}",
        json(&error)
    );

    let result = ok(expand(
        "ok\n{{$ missing_command}}",
        None,
        Some(r#"{ errors: "silent", full: true }"#),
    )
    .await, "silent recovers");
    assert_eq!(text(&get(&result, "text")), "ok\n");
    assert_eq!(js_sys::Array::from(&get(&result, "warnings")).length(), 1, "{}", json(&result));

    // Inside a command there is no `full`: the warnings are in the command's log.
    register(r#"{ name: "quiet", async: true,
                  run: async (context) => await context.expand("a{{$ missing_command}}", null,
                                                                { errors: "silent" }) }"#);
    assert_eq!(evaluate_text("quiet").await, "a");
    register(r#"{ name: "full_in_command", async: true,
                  run: async (context) => await context.expand("a", null, { full: true }) }"#);
    let error = evaluate("full_in_command").await.err().expect("`full` is refused in a command");
    assert!(message(&error).contains("full"), "{}", message(&error));
}

/// TEMPLATE08 — HTML escaping follows the explicit flag.
#[wasm_bindgen_test]
async fn template08_html_escaping_follows_the_flag() {
    fresh().await;
    let plain = ok(expand("{{v}}", Some(r#"{ v: "<b>" }"#), None).await, "plain");
    assert_eq!(text(&plain), "<b>");
    let escaped = ok(expand("{{v}}", Some(r#"{ v: "<b>" }"#), Some("{ html: true }"))
        .await, "escaped");
    assert_eq!(text(&escaped), "&lt;b&gt;");
}

/// TEMPLATE09 — `validate` returns positioned diagnostics as plain objects and runs nothing.
#[wasm_bindgen_test]
async fn template09_validate_runs_nothing() {
    fresh().await;
    js_sys::eval("globalThis.__template09_runs = 0").expect("counter");
    register(r#"{ name: "tick", run: () => { globalThis.__template09_runs += 1; return "t"; } }"#);
    let diagnostics = ok(JsFuture::from(liquers_web::validate_template(JsValue::from_str(
        "{{$ tick}} {{$ no_such_command}}",
    )))
    .await, "validate");
    let diagnostics = js_sys::Array::from(&diagnostics);
    assert_eq!(js_sys::eval("globalThis.__template09_runs").expect("runs").as_f64(), Some(0.0));
    assert_eq!(diagnostics.length(), 1, "{}", json(&diagnostics));
    let first = diagnostics.get(0);
    assert_eq!(text(&get(&first, "severity")), "Error");
    assert!(!text(&get(&first, "message")).is_empty());
    assert!(get(&get(&first, "position"), "line").as_f64().unwrap_or(0.0) >= 1.0);
}

/// TEMPLATE10 — a template that runs a JavaScript command which itself renders completes.
#[wasm_bindgen_test]
async fn template10_nested_render_does_not_deadlock() {
    fresh().await;
    register(r#"[
      { name: "inner", async: true,
        run: async (context) => await context.expand("{{x}}", { x: "in" }) },
      { name: "outer", async: true,
        run: async (context) => await context.expand("[{{$ inner}}]", {}) },
    ]"#);
    assert_eq!(evaluate_text("outer").await, "[in]");
}

/// TEMPLATE11 — a relative partial resolves against the `cwd` passed in: inside a command, the
/// command's own `-R-key/.` argument; at the top level, a `Key` or a key string.
#[wasm_bindgen_test]
async fn template11_relative_partial_resolves_against_cwd() {
    fresh().await;
    js_sys::Reflect::set(&js_sys::global(), &"__template11".into(), &query("-R/templates/page.md"))
        .expect("set global");
    register(r#"{ name: "page", async: true,
                  arguments: [{ name: "cwd", type: "any", default: { pipeline: ["-R-key/."] } },
                              { name: "context" }],
                  run: async (cwd, context) => await context.render(globalThis.__template11, null,
                                                                    null, { cwd }) }"#);
    assert_eq!(evaluate_text("-R-cwd/templates/-/page").await, "Header body");

    let key: JsValue = LiquersKey::parse("templates").expect("key").into();
    let options = js_sys::Object::new();
    js_sys::Reflect::set(&options, &"cwd".into(), &key).expect("set cwd");
    let rendered = ok(JsFuture::from(liquers_web::render(
        query("-R/templates/page.md"),
        None,
        None,
        Some(options.into()),
    ))
    .await, "render with a Key cwd");
    assert_eq!(text(&rendered), "Header body");
    let rendered = ok(JsFuture::from(liquers_web::render(
        query("-R/templates/page.md"),
        None,
        None,
        Some(js(r#"{ cwd: "templates" }"#)),
    ))
    .await, "render with a string cwd");
    assert_eq!(text(&rendered), "Header body");
}
