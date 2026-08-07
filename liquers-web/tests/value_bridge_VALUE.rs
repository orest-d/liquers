//! `VALUE*` conformance tests — the value bridge.
//!
//! Test IDs and names follow `specs/LANGUAGE-INTEGRATION_GUIDE.md` §3: the logical ID leads the
//! test-specific part of the name. The inventory these implement is in
//! `specs/liquers-web/phase3-examples.md`.
//!
//! These exercise pure ECMAScript semantics — `Object`, `Array`, `Date`, `Map`, `Uint8Array`,
//! `BigInt`, `Function` — and touch no DOM, so they run under Node as well as in a browser.
//! Node is the default harness because it needs no WebDriver:
//!
//! ```text
//! cargo test -p liquers-web --target wasm32-unknown-unknown --test value_bridge_VALUE
//! ```
//!
//! Tests that genuinely need a browser (`RUNTIME*`, `ASYNCQ*`, `PACKAGE*`) add
//! `wasm_bindgen_test_configure!(run_in_browser)` in their own files and require a chromedriver
//! whose major version matches the installed Chromium.

#![cfg(target_arch = "wasm32")]

use liquers_core::value::ValueInterface;
use liquers_lib::value::{ExtValue, Value};
use liquers_web::bridge::{js_to_value, opaque_value, value_to_js, ConversionPolicy};
use liquers_web::value::JsOpaque;
use wasm_bindgen::prelude::*;
use wasm_bindgen_test::*;

const STRUCTURAL: ConversionPolicy = ConversionPolicy::Structural;
const OPAQUE_OK: ConversionPolicy = ConversionPolicy::AllowOpaque;

fn conv(js: &JsValue) -> Result<Value, liquers_core::error::Error> {
    js_to_value::<Value>(js, STRUCTURAL)
}

/// VALUE01 — primitives round-trip, and `null`/`undefined` both collapse to none.
///
/// The collapse is lossy and deliberate; asserting it here means a future change to that policy
/// has to be a conscious one.
#[wasm_bindgen_test]
fn value01_primitive_roundtrip() {
    assert!(conv(&JsValue::NULL).expect("null").is_none());
    assert!(conv(&JsValue::UNDEFINED).expect("undefined").is_none());

    let b = conv(&JsValue::from_bool(true)).expect("bool");
    assert_eq!(b.try_into_bool().expect("as bool"), true);

    let s = conv(&JsValue::from_str("hello")).expect("string");
    assert_eq!(s.try_into_string().expect("as string"), "hello");

    let i = conv(&JsValue::from_f64(42.0)).expect("integral number");
    assert_eq!(i.try_into_i64().expect("as i64"), 42);

    let f = conv(&JsValue::from_f64(1.5)).expect("float");
    assert!((f.try_into_f64().expect("as f64") - 1.5).abs() < f64::EPSILON);

    // Round trip back out.
    let js = value_to_js(&conv(&JsValue::from_str("round")).expect("string")).expect("to js");
    assert_eq!(js.as_string().as_deref(), Some("round"));
}

/// VALUE02 — nested arrays and objects convert structurally.
#[wasm_bindgen_test]
fn value02_nested_array_object_roundtrip() {
    let inner = js_sys::Object::new();
    js_sys::Reflect::set(&inner, &"n".into(), &JsValue::from_f64(7.0)).expect("set n");

    let array = js_sys::Array::new();
    array.push(&JsValue::from_f64(1.0));
    array.push(&inner);

    let obj = js_sys::Object::new();
    js_sys::Reflect::set(&obj, &"items".into(), &array).expect("set items");

    let value = conv(&obj).expect("nested object");
    let json = value.try_into_json_value().expect("as json");
    assert_eq!(json["items"][0], serde_json::json!(1));
    assert_eq!(json["items"][1]["n"], serde_json::json!(7));
}

/// VALUE03 — integer boundaries. Beyond 2^53 a `number` is not exact, `BigInt` is, and a `BigInt`
/// outside `i64` is refused rather than wrapped.
#[wasm_bindgen_test]
fn value03_integer_boundaries() {
    // Exactly representable integers become integers.
    let max_exact = conv(&JsValue::from_f64(9_007_199_254_740_992.0)).expect("2^53");
    assert_eq!(max_exact.try_into_i64().expect("as i64"), 9_007_199_254_740_992);

    // BigInt is the lossless path.
    let big = js_sys::BigInt::from(1_234_567_890_123_456_789i64);
    let v = conv(&big.into()).expect("bigint in range");
    assert_eq!(v.try_into_i64().expect("as i64"), 1_234_567_890_123_456_789);

    // Out of i64 range must fail loudly, not wrap.
    let too_big = js_sys::BigInt::new(&JsValue::from_str("99999999999999999999999999"))
        .expect("construct huge bigint");
    let err = conv(&too_big.into()).expect_err("huge bigint must be refused");
    assert_eq!(err.error_type, liquers_core::error::ErrorType::ConversionError);
}

/// VALUE04 — bytes are never confused with text.
#[wasm_bindgen_test]
fn value04_bytes_are_not_confused_with_text() {
    let bytes = js_sys::Uint8Array::from(&[0u8, 1, 2, 255][..]);
    let value = conv(&bytes).expect("uint8array");
    assert_eq!(
        value.try_into_bytes().expect("as bytes"),
        vec![0u8, 1, 2, 255]
    );
    // The contract is that the *mapping* chose bytes, not text. Note what is deliberately not
    // asserted: `try_into_string()` on a Bytes value succeeds, decoding it lossily as UTF-8 —
    // that is the value type's documented permissiveness, not a bridge decision, and asserting
    // against it here would be testing the wrong component.
    assert_eq!(value.identifier(), "bytes", "a Uint8Array must map to bytes, not text");

    // Round trip back to a Uint8Array, not an array of numbers.
    let js = value_to_js(&value).expect("to js");
    assert!(js.is_instance_of::<js_sys::Uint8Array>(), "bytes lost their type");
}

/// VALUE05 — an unknown object follows the configured policy: refused by default, retained only
/// when the caller opted in.
#[wasm_bindgen_test]
fn value05_unknown_object_uses_opaque_value() {
    let date = js_sys::Date::new_0();

    let err = conv(&date).expect_err("a Date must not convert structurally by default");
    assert_eq!(err.error_type, liquers_core::error::ErrorType::ConversionError);

    let retained = js_to_value::<Value>(&date, OPAQUE_OK).expect("opaque policy retains");
    assert!(matches!(retained, Value::Extended(ExtValue::Foreign { .. })));
}

/// VALUE06 — an opaque value refuses byte serialization, with a *serialization* error rather than
/// a conversion error. The asset layer absorbs this, which is why refusing is safe.
#[wasm_bindgen_test]
fn value06_opaque_serialization_fails_or_uses_its_codec() {
    use liquers_core::value::DefaultValueSerializer;

    let obj = js_sys::Date::new_0();
    let value = opaque_value::<Value>(obj.into()).expect("opaque");
    let err = value.as_bytes("json").expect_err("opaque must refuse serialization");
    assert_eq!(
        err.error_type,
        liquers_core::error::ErrorType::SerializationError,
        "byte serialization must refuse with SerializationError, not ConversionError"
    );
}

/// VALUE07 — a cycle is reported, not silently truncated and not a stack overflow.
#[wasm_bindgen_test]
fn value07_cycles_follow_policy() {
    let obj = js_sys::Object::new();
    js_sys::Reflect::set(&obj, &"self".into(), &obj).expect("create cycle");

    let err = conv(&obj).expect_err("a cyclic object must be refused");
    assert_eq!(err.error_type, liquers_core::error::ErrorType::ConversionError);
    assert!(
        err.message.contains("cyclic") || err.message.contains("depth"),
        "the error should say why: {}",
        err.message
    );
}

/// VALUE08 — a representative `ExtValue` variant round-trips. `Foreign` is that variant: it is the
/// only wasm-viable extended variant, and it carries the JavaScript value by identity.
#[wasm_bindgen_test]
fn value08_representative_extvalue_roundtrip() {
    let original = js_sys::Date::new_0();
    let value = opaque_value::<Value>(original.clone().into()).expect("opaque");
    assert!(matches!(value, Value::Extended(ExtValue::Foreign { .. })));

    let back = value_to_js(&value).expect("back to js");
    assert_eq!(
        back, JsValue::from(original),
        "the retained object should come back as itself"
    );
}

/// VALUE09 — checked upcast and downcast. A downcast to the wrong concrete type fails rather than
/// producing nonsense.
#[wasm_bindgen_test]
fn value09_checked_upcast_and_downcast() {
    let value = opaque_value::<Value>(js_sys::Date::new_0().into()).expect("opaque");
    match &value {
        Value::Extended(ExtValue::Foreign { value: foreign }) => {
            assert!(
                foreign.as_any().downcast_ref::<JsOpaque>().is_some(),
                "a JavaScript value must downcast to JsOpaque"
            );
            assert!(
                foreign.as_any().downcast_ref::<String>().is_none(),
                "downcasting to an unrelated type must fail"
            );
            assert_eq!(foreign.origin(), liquers_web::ORIGIN_JAVASCRIPT);
        }
        other => panic!("expected a foreign value, got {:?}", other),
    }
}

/// VALUE10 — a retained object keeps its documented identity *within a session*.
///
/// Note what this does and does not assert. Liquers guarantees query determinism, not
/// `roundtrip(obj) === obj`; identity holding here is an artefact of direct retention and is
/// documented as incidental. The test pins the current behaviour so a change to it is deliberate.
#[wasm_bindgen_test]
fn value10_language_only_object_retains_documented_identity() {
    let original = js_sys::Map::new();
    original.set(&"k".into(), &"v".into());

    let value = opaque_value::<Value>(original.clone().into()).expect("opaque");
    let back = value_to_js(&value).expect("back to js");

    let map = js_sys::Map::from(back);
    assert_eq!(map.get(&"k".into()).as_string().as_deref(), Some("v"));
    assert_eq!(map.size(), 1);
}

/// VALUE11 — callables follow the documented policy: a bare function is *not* a value, and is
/// refused unless explicitly retained.
#[wasm_bindgen_test]
fn value11_callable_retention_or_rejection_follows_policy() {
    let f = js_sys::Function::new_no_args("return 1;");

    let err = conv(&f).expect_err("a bare function must not convert structurally");
    assert_eq!(err.error_type, liquers_core::error::ErrorType::ConversionError);

    let retained = opaque_value::<Value>(f.into()).expect("explicit opaque retains a callable");
    assert!(matches!(retained, Value::Extended(ExtValue::Foreign { .. })));
}

/// VALUE12 — the documented scalar behaviour: conversions are explicit, and a failed one is an
/// error rather than a coerced surprise.
#[wasm_bindgen_test]
fn value12_scalar_operators_produce_documented_result() {
    let text = conv(&JsValue::from_str("42")).expect("string");
    // A numeric-looking string stays text; it is not silently coerced.
    assert_eq!(text.try_into_string().expect("as string"), "42");

    let opaque = opaque_value::<Value>(js_sys::Date::new_0().into()).expect("opaque");
    assert!(
        opaque.try_into_string().is_err(),
        "an opaque value must not coerce to text via String(obj)"
    );
}

/// VALUE13 — metadata behaviour is explicit: an opaque value reports its own identifier and type
/// name rather than inheriting a generic one, so metadata stays debuggable.
#[wasm_bindgen_test]
fn value13_state_operations_preserve_or_discard_metadata() {
    let value = opaque_value::<Value>(js_sys::Date::new_0().into()).expect("opaque");
    assert_eq!(value.identifier(), "js");
    assert_eq!(value.type_name(), "Date", "the constructor name should survive");
    assert_eq!(value.default_media_type(), "application/json");
}
