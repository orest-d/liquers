//! Step 8 spike: does `serde_wasm_bindgen` convert a JavaScript declaration object into the
//! `serde_json::Value` the declaration pipeline expects?
//!
//! Phase 2 of `specs/design/command-declaration/` records this as its largest unverified claim,
//! and the answer selects the code path `JsCommandSpec::parse` uses:
//!
//! * **pass** — deserialize the run-less copy straight into `serde_json::Value`;
//! * **fail** — `js_sys::JSON::stringify` then `serde_json::from_str`, at the cost that a
//!   non-JSON default becomes "absent" rather than an error.
//!
//! The test exists to answer that question before any code depends on the answer. It stays
//! afterwards as a regression guard on the conversion.

#![cfg(target_arch = "wasm32")]

mod common;
use common::{arg, args, obj, set};

use liquers_core::command_declaration::CommandDeclaration;
use serde_json::json;
use wasm_bindgen::prelude::*;
use wasm_bindgen_test::*;

/// A declaration object shaped like the ones `registerCommand` actually receives: nested
/// `arguments`, a bare `default: 2`, a tagged `{Value: 2}`, and a `registration` block.
fn declaration_object() -> js_sys::Object {
    let spec = obj();
    set(&spec, "name", &JsValue::from_str("repeat"));
    set(&spec, "label", &JsValue::from_str("Repeat text"));
    set(&spec, "volatile", &JsValue::from_bool(false));
    // The tagged default form the exporter writes, alongside the bare forms JavaScript writes.
    let tagged = obj();
    set(&tagged, "Value", &JsValue::from_f64(7.0));
    let tagged_arg = obj();
    set(&tagged_arg, "name", &JsValue::from_str("times"));
    set(&tagged_arg, "type", &JsValue::from_str("int"));
    set(&tagged_arg, "default", &tagged.into());

    set(
        &spec,
        "arguments",
        &args(vec![
            arg("count", Some("int"), Some(JsValue::from_f64(2.0))),
            arg("sep", Some("string"), Some(JsValue::from_str("-"))),
            tagged_arg.into(),
        ]),
    );

    let javascript = obj();
    set(&javascript, "state", &JsValue::from_str("text"));
    let registration = obj();
    set(&registration, "javascript", &javascript.into());
    set(&spec, "registration", &registration.into());
    spec
}

/// DECL01 — the conversion itself. This is the spike's question.
#[wasm_bindgen_test]
fn decl01_a_javascript_declaration_converts_to_serde_json_value() {
    let spec = declaration_object();
    let converted: serde_json::Value = serde_wasm_bindgen::from_value(spec.into())
        .expect("serde_wasm_bindgen converts a declaration object to serde_json::Value");

    assert_eq!(converted["name"], json!("repeat"));
    assert_eq!(converted["label"], json!("Repeat text"));
    assert_eq!(converted["volatile"], json!(false));
    assert_eq!(
        converted["registration"]["javascript"]["state"],
        json!("text")
    );

    let arguments = converted["arguments"]
        .as_array()
        .expect("`arguments` survives as an array");
    assert_eq!(arguments.len(), 3);
    assert_eq!(arguments[0]["name"], json!("count"));
    assert_eq!(arguments[0]["type"], json!("int"));
    // Integers survive as integers. `js_default_to_json` narrowed a whole f64 to i64 by hand
    // (`spec.rs`, before the rewrite); serde_wasm_bindgen does the same, so a numeric default
    // keeps its representation and no command's `metadata_version` moves. This assertion is the
    // whole reason the spike ran before the rewrite it would have invalidated.
    assert_eq!(
        arguments[0]["default"],
        json!(2),
        "a bare numeric default stays an integer"
    );
    assert_eq!(arguments[1]["default"], json!("-"), "a bare string default");
    assert_eq!(
        arguments[2]["default"],
        json!({ "Value": 7 }),
        "the tagged form survives as a nested object"
    );
}

/// DECL02 — the converted value drives the pipeline end to end, which is what step 9 will do.
#[wasm_bindgen_test]
fn decl02_the_converted_declaration_builds_metadata() {
    let converted: serde_json::Value =
        serde_wasm_bindgen::from_value(declaration_object().into()).expect("converts");

    let metadata = CommandDeclaration::from_document(converted)
        .finish()
        .expect("the converted declaration builds");

    assert_eq!(metadata.name, "repeat");
    assert_eq!(metadata.label, "Repeat text");
    assert_eq!(metadata.arguments.len(), 3);
    assert_eq!(
        metadata.arguments[0].argument_type,
        liquers_core::command_metadata::ArgumentType::Integer,
        "`type` reached `argument_type` through the conversion"
    );
    assert_eq!(
        serde_json::to_value(&metadata)
            .expect("serializes")
            .get("registration"),
        None,
        "a registration hint stays on the declaration"
    );
}

/// DECL03 — a numeric default arrives as a number, whichever representation it takes.
///
/// Written when it looked as though serde's conversion might hand back `2.0` where the old
/// hand-written `js_default_to_json` narrowed a whole f64 to an i64. DECL01 settles that it does
/// not — the narrowing is reproduced — so this is now a broad guard rather than the sharp question
/// it was written as.
#[wasm_bindgen_test]
fn decl03_numeric_defaults_arrive_as_numbers() {
    let spec = obj();
    set(&spec, "name", &JsValue::from_str("f"));
    set(
        &spec,
        "arguments",
        &args(vec![arg(
            "count",
            Some("int"),
            Some(JsValue::from_f64(2.0)),
        )]),
    );
    let converted: serde_json::Value =
        serde_wasm_bindgen::from_value(spec.into()).expect("converts");

    let default = &converted["arguments"][0]["default"];
    assert!(
        default.is_f64() || default.is_i64(),
        "a numeric default arrives as a number: {default}"
    );
}
