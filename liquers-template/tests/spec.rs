//! The Mustache specification (AC-1): every case of the six required modules, on `MapHost`.
#![cfg(not(target_arch = "wasm32"))]

use liquers_core::data_node::AnyNode;
use liquers_core::parse::parse_key;
use liquers_template::{
    parse_template, render, ContextStack, FrameKind, MapHost, RenderOptions, TemplateSyntax,
};

fn run(module: &str) {
    let path = format!("{}/tests/spec/{module}.json", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(&path).expect("spec file");
    let spec: serde_json::Value = serde_json::from_str(&text).expect("spec json");
    let mut failures = Vec::new();
    let tests = spec["tests"].as_array().expect("tests");
    for test in tests {
        let name = test["name"].as_str().unwrap_or("?");
        let template = test["template"].as_str().expect("template");
        let expected = test["expected"].as_str().expect("expected");
        let mut host = MapHost::new();
        if let Some(partials) = test["partials"].as_object() {
            for (key, text) in partials {
                host = host.with_partial(parse_key(key).expect("partial key"), text.as_str().unwrap_or(""));
            }
        }
        let options = RenderOptions {
            html_escape: true,
            ..RenderOptions::default()
        };
        let result = parse_template(template, TemplateSyntax::Mustache).and_then(|t| {
            let mut stack = ContextStack::new();
            stack.push(AnyNode::from_json(test["data"].clone()), FrameKind::Data);
            futures::executor::block_on(render(&t, stack, &host, &options))
        });
        match result {
            Ok(out) if out == expected => {}
            Ok(out) => failures.push(format!("{name}: expected {expected:?}, got {out:?}")),
            Err(e) => failures.push(format!("{name}: error {e}")),
        }
    }
    assert!(failures.is_empty(), "{module}: {} of {} failed:\n{}", failures.len(), tests.len(), failures.join("\n"));
}

#[test]
fn spec_interpolation() {
    run("interpolation");
}

#[test]
fn spec_sections() {
    run("sections");
}

#[test]
fn spec_inverted() {
    run("inverted");
}

#[test]
fn spec_comments() {
    run("comments");
}

#[test]
fn spec_partials() {
    run("partials");
}

#[test]
fn spec_delimiters() {
    run("delimiters");
}
