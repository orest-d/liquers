//! The crate on wasm32, under Node (Phase 4 Step 12): the Mustache spec suite and a hygienic query
//! placeholder, with the spec files embedded since a wasm test has no file system.
#![cfg(target_arch = "wasm32")]

use liquers_core::command_metadata::CommandMetadataRegistry;
use liquers_core::data_node::AnyNode;
use liquers_core::parse::parse_key;
use liquers_template::{
    parse_query_template, parse_template, render, ContextStack, FrameKind, MapHost, RenderOptions,
    TemplateSyntax,
};
use wasm_bindgen_test::*;

const SPECS: [(&str, &str); 6] = [
    ("interpolation", include_str!("spec/interpolation.json")),
    ("sections", include_str!("spec/sections.json")),
    ("inverted", include_str!("spec/inverted.json")),
    ("comments", include_str!("spec/comments.json")),
    ("partials", include_str!("spec/partials.json")),
    ("delimiters", include_str!("spec/delimiters.json")),
];

#[wasm_bindgen_test]
async fn node_mustache_spec() {
    let mut failures = Vec::new();
    for (module, text) in SPECS {
        let spec: serde_json::Value = serde_json::from_str(text).expect("spec json");
        for test in spec["tests"].as_array().expect("tests") {
            let name = test["name"].as_str().unwrap_or("?");
            let mut host = MapHost::new();
            if let Some(partials) = test["partials"].as_object() {
                for (key, text) in partials {
                    host = host.with_partial(parse_key(key).expect("key"), text.as_str().unwrap_or(""));
                }
            }
            let options = RenderOptions {
                html_escape: true,
                ..RenderOptions::default()
            };
            let template = parse_template(test["template"].as_str().expect("template"), TemplateSyntax::Mustache)
                .expect("template parses");
            let mut stack = ContextStack::new();
            stack.push(AnyNode::from_json(test["data"].clone()), FrameKind::Data);
            match render(&template, stack, &host, &options).await {
                Ok(out) if out == test["expected"].as_str().expect("expected") => {}
                Ok(out) => failures.push(format!("{module}/{name}: got {out:?}")),
                Err(e) => failures.push(format!("{module}/{name}: error {e}")),
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[wasm_bindgen_test]
async fn node_hygienic_placeholder() {
    let template = parse_query_template("cmd-{{x}}/q").expect("query template");
    let mut stack = ContextStack::new();
    stack.push(AnyNode::from_json(serde_json::json!({"x": "a-b"})), FrameKind::Data);
    let instantiated = template
        .instantiate(&stack, &CommandMetadataRegistry::new())
        .await
        .expect("instantiates");
    assert_eq!(instantiated.query.encode(), "cmd-a~_b/q");
}
