//! Rendering on `RecordingHost`: what each tag asks the host, sections, frames, partials and the
//! error modes (Phase 3, Examples 2 and 3).
#![cfg(not(target_arch = "wasm32"))]

mod common;

use common::recording_host::{Recorded, RecordingHost};
use liquers_core::data_node::AnyNode;
use liquers_core::error::Error;
use liquers_template::{
    parse_template, render, ContextStack, ErrorMode, ExpandFormat, FrameKind, RenderOptions,
    TemplateSyntax,
};
use serde_json::json;

fn run(
    template: &str,
    data: serde_json::Value,
    host: &RecordingHost,
    errors: ErrorMode,
) -> Result<String, Error> {
    let t = parse_template(template, TemplateSyntax::Mustache)?;
    let mut stack = ContextStack::new();
    stack.push(AnyNode::from_json(data), FrameKind::Data);
    let options = RenderOptions {
        errors,
        ..RenderOptions::default()
    };
    futures::executor::block_on(render(&t, stack, host, &options))
}

fn recorded(query: &str, overrides: Vec<(String, serde_json::Value)>, input: Option<serde_json::Value>) -> Recorded {
    Recorded {
        query: query.to_string(),
        overrides,
        input,
    }
}

fn example_2_data() -> serde_json::Value {
    json!({"x": "a-b", "tags": ["p", "q"], "row": {"n": 1}})
}

#[test]
fn example_2_requests() {
    let row = Some(json!({"n": 1}));
    let cases: Vec<(&str, Vec<Recorded>)> = vec![
        ("{{$ cmd-{{x}}/q}}", vec![recorded("cmd-a~_b/q", vec![], None)]),
        ("{{$$ cmd-{{x}}/q}}", vec![recorded("cmd-a-b/q", vec![], None)]),
        ("{{>$ to_text}}", vec![recorded("to_text", vec![], None)]),
        ("{{#row}}{{$. to_text}}{{/row}}", vec![recorded("to_text", vec![], row.clone())]),
        ("{{$.row to_text}}", vec![recorded("to_text", vec![], row.clone())]),
        ("{{$$.row to_text}}", vec![recorded("to_text", vec![], row.clone())]),
        ("{{#$.row to_text}}x{{/}}", vec![recorded("to_text", vec![], row.clone())]),
        ("{{>$.row to_text}}", vec![recorded("to_text", vec![], row.clone())]),
        ("{{$ cmd-{{tags}}}}", vec![recorded("cmd", vec![("arg".to_string(), json!(["p", "q"]))], None)]),
    ];
    for (template, expected) in cases {
        let host = RecordingHost::new();
        let out = run(template, example_2_data(), &host, ErrorMode::Strict)
            .unwrap_or_else(|e| panic!("{template}: {e}"));
        assert_eq!(host.requests(), expected, "{template}");
        assert!(out == "ok" || out == "x", "{template} rendered {out:?}");
    }
    let host = RecordingHost::new();
    let err = run("{{$ cmd-{{tags}}/q}}", example_2_data(), &host, ErrorMode::Strict)
        .expect_err("an override reaches only the last action");
    assert!(err.message.contains("only the last action"), "{}", err.message);
    assert_eq!(err.position.column, 9);
    assert!(host.requests().is_empty());
}

#[test]
fn section_over_list_map_bool() {
    let host = RecordingHost::new()
        .answer("list", json!([1, 2]))
        .answer("map", json!({"k": "v"}))
        .answer("yes", json!(true))
        .answer("no", json!(false));
    let out = run(
        "{{#$ list}}({{.}}){{/}} {{#$ map}}{{k}}{{/}} {{#$ yes}}Y{{/}}{{#$ no}}N{{/}}",
        json!({}),
        &host,
        ErrorMode::Strict,
    )
    .expect("render");
    assert_eq!(out, "(1)(2) v Y");
}

#[test]
fn inverted_on_empty_result() {
    let host = RecordingHost::new().answer("empty", json!([])).answer("list", json!([1]));
    let out = run("{{^$ empty}}none{{/}}{{^$ list}}never{{/}}", json!({}), &host, ErrorMode::Strict)
        .expect("render");
    assert_eq!(out, "none");
}

#[test]
fn bare_close_ends_query_section() {
    let host = RecordingHost::new().answer("list", json!([{"t": "a"}, {"t": "b"}]));
    let out = run("{{#$ list}}{{t}}{{/}}|{{#$ list}}{{t}}{{/$}}", json!({}), &host, ErrorMode::Strict)
        .expect("render");
    assert_eq!(out, "ab|ab");
}

#[test]
fn evaluate_has_no_input() {
    let host = RecordingHost::new();
    run("{{$ to_text}}", json!({"a": 1}), &host, ErrorMode::Strict).expect("render");
    assert_eq!(host.requests(), vec![recorded("to_text", vec![], None)]);
}

#[test]
fn names_frame_is_not_dot() {
    let host = RecordingHost::new();
    let t = parse_template("{{.}} {{b}} {{a}} {{$. to_text}}", TemplateSyntax::Mustache).expect("parse");
    let mut stack = ContextStack::new();
    stack.push(AnyNode::from_json(json!({"a": 1})), FrameKind::Data);
    stack.push(AnyNode::from_json(json!({"a": 3, "b": 2})), FrameKind::Names);
    let out = futures::executor::block_on(render(&t, stack, &host, &RenderOptions::default()))
        .expect("render");
    assert_eq!(out, "{\"a\":1} 2 3 ok");
    assert_eq!(host.requests(), vec![recorded("to_text", vec![], Some(json!({"a": 1})))]);
}

#[test]
fn partial_cycle_is_error() {
    let host = RecordingHost::new().partial("self", "x{{>self}}");
    let err = run("{{>self}}", json!({}), &host, ErrorMode::Strict).expect_err("cycle");
    assert!(err.message.contains("includes itself"), "{}", err.message);
}

#[test]
fn partial_depth_capped() {
    let host = RecordingHost::new().partial("deep", "{{#a}}{{>deep}}{{/a}}");
    let err = run("{{>deep}}", json!({"a": {}}), &host, ErrorMode::Strict)
        .expect_err("depth");
    assert!(err.message.contains("deeper than 32"), "{}", err.message);
}

const EXAMPLE_3: &str = "{{items.0.title}} {{items[i].title}} {{items[-1].title}} {{m[\"a b\"]}} [{{items.5.title}}]\n{{items[5].title}} {{$ ns-fixture/fail}}\n{{#$ ns-fixture/fail}}never{{/}}";

fn example_3_data() -> serde_json::Value {
    json!({"items": [{"title": "first"}, {"title": "second"}], "i": 1, "m": {"a b": 7}})
}

fn example_3_host() -> RecordingHost {
    RecordingHost::new().fail("ns-fixture/fail", "failed on purpose")
}

#[test]
fn example_3_modes() {
    let host = example_3_host();
    let err = run(EXAMPLE_3, example_3_data(), &host, ErrorMode::Strict).expect_err("strict fails");
    assert_eq!((err.position.line, err.position.column), (2, 8));
    assert!(host.warnings().is_empty());

    let host = example_3_host();
    let out = run(EXAMPLE_3, example_3_data(), &host, ErrorMode::Expand(ExpandFormat::Text)).expect("expand");
    assert_eq!(
        out,
        "first second second 7 []\n\
         [template error at 2:8: index 5 out of range for a list of 2] [template error at 2:20: failed on purpose]\n\
         [template error at 3:1: failed on purpose]"
    );
    assert_eq!(host.warnings().len(), 3);

    let host = example_3_host();
    let out = run(EXAMPLE_3, example_3_data(), &host, ErrorMode::Expand(ExpandFormat::Html)).expect("html");
    assert!(out.contains("<span class=\"liquers-template-error\" title=\"2:8\">"), "{out}");
    assert_eq!(out.matches("<span").count(), 3);

    let host = example_3_host();
    let out = run(EXAMPLE_3, example_3_data(), &host, ErrorMode::Expand(ExpandFormat::Markdown)).expect("md");
    assert!(out.contains("`[template error at 3:1: failed on purpose]`"), "{out}");

    let host = example_3_host();
    let out = run(EXAMPLE_3, example_3_data(), &host, ErrorMode::Silent).expect("silent");
    assert_eq!(out, "first second second 7 []\n \n");
    assert_eq!(host.warnings().len(), 3);
}

#[test]
fn failing_section_head_is_one_unit() {
    let host = example_3_host();
    let out = run(
        "{{#$ ns-fixture/fail}}{{$ never}}{{/}}",
        json!({}),
        &host,
        ErrorMode::Expand(ExpandFormat::Text),
    )
    .expect("expand");
    assert_eq!(out, "[template error at 1:1: failed on purpose]");
    assert_eq!(host.requests().len(), 1, "the body never runs");
}

#[test]
fn strict_rejects_invalid_before_any_query() {
    let host = example_3_host();
    let err = run("{{$ ns-fixture/fail}} {{a..b}}", json!({}), &host, ErrorMode::Strict)
        .expect_err("invalid");
    assert_eq!(err.position.column, 27, "at the second '.' of a..b");
    assert!(host.requests().is_empty(), "no query ran");
}

#[test]
fn irrecoverable_in_every_mode() {
    for text in ["{{#items}}x", "{{#a}}{{/b}}", "x {{y", "{{=a=}}"] {
        assert!(parse_template(text, TemplateSyntax::Mustache).is_err(), "{text}");
    }
}
