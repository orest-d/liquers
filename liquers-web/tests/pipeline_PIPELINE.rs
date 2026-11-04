//! `PIPELINE*` conformance tests: pipelines from JavaScript (`src/pipeline.rs`).
//!
//! Pipelines and declarations are written as JavaScript object literals (`js_sys::eval`), the
//! form a page writes, and go through the exported functions a page calls. The suites share one
//! wasm instance and so one singleton; every test starts from `reset_global` + `init_global`.

#![cfg(target_arch = "wasm32")]

use liquers_web::environment::{init_global, reset_global};
use wasm_bindgen::prelude::*;
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

fn fresh() {
    reset_global();
    init_global().expect("init");
}

fn register(spec: &str) {
    liquers_web::register_commands(js(spec).unchecked_into(), None).expect("register");
}

async fn evaluate(query: &str) -> Result<JsValue, JsValue> {
    JsFuture::from(liquers_web::evaluate(query)).await
}

async fn evaluate_pipeline(pipeline: &JsValue) -> Result<JsValue, JsValue> {
    JsFuture::from(liquers_web::evaluate_pipeline(pipeline.clone())).await
}

fn error_type(error: &JsValue) -> String {
    get(error, "errorType").as_string().unwrap_or_default()
}

fn message(error: &JsValue) -> String {
    get(error, "message").as_string().unwrap_or_default()
}

/// The two commands most tests compose: a source and a transform.
fn register_fixtures(prefix: &str) {
    register(&format!(
        r#"[
          {{ name: "{prefix}_greet", arguments: [{{ name: "who", type: "string", default: "world" }}],
             run: (who) => "hi " + who }},
          {{ name: "{prefix}_shout", state: "text", run: (text) => text.toUpperCase() }},
        ]"#
    ));
}

/// PIPELINE01 — evaluating a pipeline equals evaluating its query, both for the pipeline
/// `queryToPipeline` returns and for one written by hand, as an array or as `{ elements }`.
#[wasm_bindgen_test]
async fn pipeline01_evaluating_a_pipeline_equals_its_query() {
    fresh();
    register_fixtures("p01");
    let query = "p01_greet-bob/p01_shout";

    let expected = evaluate(query).await.expect("evaluate the query");
    assert_eq!(expected.as_string().as_deref(), Some("HI BOB"));

    let converted = liquers_web::query_to_pipeline(query).expect("convert");
    assert!(
        js_sys::Array::is_array(&get(&converted, "elements")),
        "queryToPipeline returns the object form: {}",
        json(&converted)
    );
    let from_query = evaluate_pipeline(&converted).await.expect("evaluate it");
    assert_eq!(from_query, expected);

    let by_hand = js(r#"[{ action: "p01_greet", parameters: ["bob"] }, { action: "p01_shout" }]"#);
    assert_eq!(
        evaluate_pipeline(&by_hand).await.expect("array form"),
        expected
    );

    let named = js(
        r#"{ elements: [{ action: "p01_greet", arguments: { who: "bob" } }, { action: "p01_shout" }] }"#,
    );
    assert_eq!(
        evaluate_pipeline(&named).await.expect("named argument"),
        expected
    );

    // And back: the named argument is placed by the command's metadata.
    assert_eq!(
        liquers_web::pipeline_to_query(named).expect("to query"),
        query
    );

    // A failing pipeline rejects with a structured LiquersError, as `evaluate` does.
    let failing = js(r#"[{ action: "p01_no_such_command" }]"#);
    let error = evaluate_pipeline(&failing)
        .await
        .expect_err("an unknown command");
    assert!(!error_type(&error).is_empty(), "a LiquersError: {error:?}");

    // Query text where a pipeline is expected is refused, not parsed.
    let error = evaluate_pipeline(&JsValue::from_str(query))
        .await
        .expect_err("a string is not a pipeline");
    assert_eq!(error_type(&error), "parameter_error");
    reset_global();
}

/// PIPELINE02 — a structured argument (a map) reaches an `any` argument whole, with its numbers
/// keeping their JSON types.
#[wasm_bindgen_test]
async fn pipeline02_map_argument_reaches_any_argument_whole() {
    fresh();
    register(
        r#"{ name: "p02_describe", arguments: [{ name: "options" }],
             run: (options) => JSON.stringify(options)
               + "|" + Number.isInteger(options.count) + "|" + (typeof options.ratio) }"#,
    );
    let pipeline = js(r#"[{ action: "p02_describe",
              arguments: { options: { value: { count: 3, ratio: 0.5, tags: ["a", "b"], deep: { on: true } } } } }]"#);
    let result = evaluate_pipeline(&pipeline).await.expect("evaluate");
    assert_eq!(
        result.as_string().as_deref(),
        Some(r#"{"count":3,"deep":{"on":true},"ratio":0.5,"tags":["a","b"]}|true|number"#)
    );

    // A live JavaScript object has no JSON form: refused, never carried.
    let with_map = js(
        r#"[{ action: "p02_describe", arguments: { options: { value: new Map([["a", 1]]) } } }]"#,
    );
    let error = evaluate_pipeline(&with_map)
        .await
        .expect_err("a Map is refused");
    assert_eq!(error_type(&error), "conversion_error");
    reset_global();
}

/// PIPELINE03 — a pipeline with no query form raises an error naming the representability rule;
/// a `QueryEditor` refuses the edit that would cause it.
#[wasm_bindgen_test]
async fn pipeline03_non_representable_pipeline_names_the_rule() {
    fresh();
    register(r#"{ name: "p03_take", arguments: [{ name: "options" }], run: (o) => o }"#);
    let pipeline = js(r#"[{ action: "p03_take", arguments: { options: { value: { a: 1 } } } }]"#);

    let error = liquers_web::pipeline_to_query(pipeline.clone()).expect_err("no query form");
    assert_eq!(error_type(&error), "parameter_error");
    let text = message(&error);
    assert!(
        text.contains("not representable as a query ("),
        "the message names the rule: {text}"
    );

    // The same pipeline still evaluates: representability is a property of the query form only.
    let value = evaluate_pipeline(&pipeline).await.expect("evaluates");
    assert_eq!(json(&value), r#"{"a":1}"#);

    let mut editor =
        liquers_web::LiquersQueryEditor::new(Some("p03_take".to_string())).expect("a query editor");
    let before = editor.query().expect("a query");
    let refused = editor.set(0, JsValue::from_str("options"), js(r#"{ a: 1 }"#));
    assert!(refused.is_err(), "a map argument has no query form");
    assert_eq!(editor.query().expect("still a query"), before);
    reset_global();
}

/// PIPELINE04 — a pipeline command registered from JavaScript plans and runs as the long query,
/// with defaults, after a rebuild, and listed before its steps in a batch.
#[wasm_bindgen_test]
async fn pipeline04_pipeline_command_runs_as_the_long_query() {
    fresh();
    // The pipeline command comes first in the batch; its steps are function commands of the same
    // batch, which registration puts in first.
    register(
        r#"[
          { name: "p04_loud_greet",
            pipeline: [{ action: "p04_greet", parameters: [{ placeholder: "who" }] }, { action: "p04_shout" }],
            arguments: [{ name: "who", default: "world" }],
            doc: "Greets, loudly." },
          { name: "p04_greet", arguments: [{ name: "who", type: "string", default: "world" }],
            run: (who) => "hi " + who },
          { name: "p04_shout", state: "text", run: (text) => text.toUpperCase() },
        ]"#,
    );
    let long = evaluate("p04_greet-bob/p04_shout")
        .await
        .expect("long query");
    let short = evaluate("p04_loud_greet-bob")
        .await
        .expect("pipeline command");
    assert_eq!(short, long);
    assert_eq!(short.as_string().as_deref(), Some("HI BOB"));
    assert_eq!(
        evaluate("p04_loud_greet")
            .await
            .expect("default")
            .as_string()
            .as_deref(),
        Some("HI WORLD")
    );

    // The argument's omitted type was completed from the slot its placeholder fills.
    let info = liquers_web::describe_command("p04_loud_greet").expect("describe");
    assert!(
        json(&info).contains("String") || json(&info).contains("string"),
        "the argument takes the step's string type: {}",
        json(&info)
    );

    // Registered after the environment was shared: the rebuild replays every command.
    register(
        r#"{ name: "p04_twice_loud", pipeline: [{ action: "p04_loud_greet", parameters: [{ placeholder: "who" }] },
                                                 { action: "p04_shout" }],
             arguments: [{ name: "who" }] }"#,
    );
    assert_eq!(
        evaluate("p04_twice_loud-ann")
            .await
            .expect("after a rebuild")
            .as_string()
            .as_deref(),
        Some("HI ANN")
    );
    assert_eq!(
        evaluate("p04_loud_greet-bob").await.expect("replayed"),
        long
    );
    reset_global();
}

/// PIPELINE04, the refusals: a pipeline command is checked with its batch before anything is
/// registered, and the report names what is wrong.
#[wasm_bindgen_test]
fn pipeline04_invalid_pipeline_commands_are_refused_with_a_report() {
    fresh();
    let batch = js(r#"[
          { name: "p04b_unknown_step", pipeline: [{ action: "p04b_missing" }] },
          { name: "p04b_undeclared", pipeline: [{ action: "to_text", parameters: [{ placeholder: "x" }] }] },
          { name: "p04b_both", pipeline: [{ action: "to_text" }], run: () => 1 },
        ]"#);
    let report = liquers_web::validate_commands(batch.clone().unchecked_into(), None);
    let text = json(&report);
    assert!(
        text.contains("p04b_missing"),
        "the unknown step is named: {text}"
    );
    assert!(
        text.contains("Placeholder 'x'"),
        "the undeclared placeholder is named: {text}"
    );
    assert!(
        text.contains("takes no `run`"),
        "run beside pipeline is refused: {text}"
    );

    let refused = liquers_web::register_commands(batch.unchecked_into(), None);
    assert!(refused.is_err(), "the batch is refused whole");
    assert!(!liquers_web::environment::has_command("p04b_undeclared"));

    // Imported metadata may define a pipeline command too; it needs no implementation.
    let yaml = r#"
commands:
  - name: p04b_imported
    module: ""
    definition: !Pipeline
      pipeline:
        elements:
          - action: to_text
"#;
    let report = liquers_web::register_commands(JsValue::from_str(yaml).unchecked_into(), None)
        .unwrap_or_else(|e| panic!("imported pipeline command: {}", message(&e)));
    assert_eq!(get(&report.into(), "hasErrors").as_bool(), Some(false));
    assert!(liquers_web::environment::has_command("p04b_imported"));
    reset_global();
}

/// PIPELINE05 — a refused edit leaves the pipeline unchanged.
#[wasm_bindgen_test]
fn pipeline05_refused_edit_leaves_the_pipeline_unchanged() {
    fresh();
    register_fixtures("p05");
    let mut editor = liquers_web::LiquersPipelineEditor::new(None).expect("editor");
    let at = editor.action("p05_greet", None).expect("a known command");
    editor
        .set(at, JsValue::from_str("who"), JsValue::from_str("eve"))
        .expect("a known argument");
    let before = json(&editor.pipeline().expect("pipeline"));

    // Each of these is refused, and none changes anything.
    assert!(editor
        .set(at, JsValue::from_str("nope"), JsValue::from(1))
        .is_err());
    assert!(editor.action("p05_no_such_command", None).is_err());
    // An unknown namespace still searches the default ones, where `p05_shout` is; a namespace
    // that is not an identifier cannot be written at all.
    assert!(editor
        .action("p05_shout", Some(JsValue::from_str("not a namespace")))
        .is_err());
    assert!(editor.push(js(r#"{ nonsense: 1 }"#)).is_err());
    assert!(editor.remove(7).is_err());
    assert_eq!(json(&editor.pipeline().expect("pipeline")), before);
    assert_eq!(editor.length(), 1);

    // The accepted edits build what the query says.
    editor.action("p05_shout", None).expect("append");
    assert_eq!(editor.query().expect("query"), "p05_greet-eve/p05_shout");
    reset_global();
}
