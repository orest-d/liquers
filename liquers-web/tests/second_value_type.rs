//! Tier-2 extensibility proof: a **second value type** through the same generic surface.
//!
//! Phase 2 promises that a downstream crate can define its own value type and reuse this crate's
//! conversion layer, command adapter and `Promise` bridge unchanged. That promise rests on a hard
//! rule — no module except `default_value.rs` may name `liquers_lib::value::Value` — and a rule
//! nothing exercises is a rule that quietly stops holding.
//!
//! So this file plays the part of a downstream crate. It defines `MatrixExt`, builds
//! `TestValue = CombinedValue<SimpleValue, MatrixExt>`, implements [`JsValueBridge`] for it, and
//! then instantiates **every** generic function in the crate at that type.
//!
//! # Compilation is necessary but not sufficient
//!
//! If the hard rule were violated, this file would fail to compile — which is the point, and is
//! why the instantiation list below is exhaustive rather than representative. But compiling only
//! proves the generic path type-checks. So the reduced conversion suite (`VALUE01`, `VALUE04`,
//! `VALUE09`) runs against `TestValue` as well, to show the generic path *behaves*.

#![cfg(target_arch = "wasm32")]

mod common;

use std::borrow::Cow;
use std::sync::Arc;

use liquers_core::error::{Error, ErrorType};
use liquers_core::value::{DefaultValueSerializer, ValueInterface};
// Imported from `liquers_lib::value` — the path a downstream crate would reach for first. It did
// not work until this test tried it: `value/mod.rs` glob-imported these privately, so they were
// not re-exported at that level and the extension path required reaching into `value::extended`
// and `value::simple`.
use liquers_lib::value::{CombinedValue, SimpleValue, ValueExtension};
use liquers_web::bridge::{
    js_to_value, opaque_value, value_to_js, ConversionPolicy, JsExtensionBridge, JsValueBridge,
};
use liquers_web::value::JsOpaque;
use wasm_bindgen::prelude::*;
use wasm_bindgen_test::*;

// ---------------------------------------------------------------------------
// A downstream value type
// ---------------------------------------------------------------------------

/// A third-party type that Rust commands need to operate on *structurally*, so the opaque path is
/// not enough — the case Tier 2 exists for.
#[derive(Debug, Clone, PartialEq)]
pub struct Matrix {
    pub rows: usize,
    pub cols: usize,
    pub data: Vec<f64>,
}

/// The downstream extension. Carries the third-party type, and — because a `JsValueBridge` impl
/// needs somewhere to put a retained JavaScript value — a foreign variant of its own.
#[derive(Debug, Clone)]
pub enum MatrixExt {
    Matrix(Arc<Matrix>),
    Foreign(Arc<JsOpaque>),
}

impl MatrixExt {
    fn name(&self) -> &'static str {
        match self {
            MatrixExt::Matrix(_) => "matrix",
            MatrixExt::Foreign(_) => "foreign",
        }
    }
}

impl ValueExtension for MatrixExt {
    fn identifier(&self) -> Cow<'static, str> {
        Cow::Borrowed(self.name())
    }

    fn type_name(&self) -> Cow<'static, str> {
        Cow::Borrowed(self.name())
    }

    fn default_extension(&self) -> Cow<'static, str> {
        Cow::Borrowed("json")
    }

    fn default_filename(&self) -> Cow<'static, str> {
        Cow::Borrowed("matrix.json")
    }

    fn default_media_type(&self) -> Cow<'static, str> {
        Cow::Borrowed("application/json")
    }

    fn try_into_json_value(&self) -> Result<serde_json::Value, Error> {
        match self {
            MatrixExt::Matrix(m) => Ok(serde_json::json!({
                "rows": m.rows,
                "cols": m.cols,
                "data": m.data,
            })),
            // An opaque value has no serializable form — the documented degradation.
            MatrixExt::Foreign(_) => Err(Error::conversion_error("foreign", "JSON")),
        }
    }
}

/// `DefaultValueSerializer` is a supertrait of `ValueExtension`, so a downstream extension has to
/// provide it. Refusing both directions is legitimate and is what an in-memory-only type does.
impl DefaultValueSerializer for MatrixExt {
    fn as_bytes(&self, _format: &str) -> Result<Vec<u8>, Error> {
        Err(Error::from_error(
            ErrorType::SerializationError,
            "a matrix is not serialized to bytes in this test".to_string(),
        ))
    }

    fn deserialize_from_bytes(_b: &[u8], _type_identifier: &str, _fmt: &str) -> Result<Self, Error> {
        Err(Error::from_error(
            ErrorType::SerializationError,
            "a matrix is not deserialized from bytes in this test".to_string(),
        ))
    }
}

/// The downstream value type. `CombinedValue` is what makes this two lines rather than an
/// implementation of `ValueInterface` from scratch.
pub type TestValue = CombinedValue<SimpleValue, MatrixExt>;

/// The bridge is implemented on **`MatrixExt`, not on `TestValue`** — and that is the finding this
/// file exists to produce.
///
/// Phase 2's Example 4 sketched `impl JsValueBridge for MyValue`, where `MyValue` is a
/// `CombinedValue`. That does not compile from a downstream crate: both the trait and
/// `CombinedValue` are foreign there, and `CombinedValue` is not `#[fundamental]`, so instantiating
/// it with a local type does not make the self type local (E0117). `MatrixExt` *is* local, so a
/// foreign trait on it is always allowed, and `bridge`'s blanket impl carries it up to `TestValue`.
impl JsExtensionBridge for MatrixExt {
    /// Recognizes a `{ rows, cols, data }` object as a `Matrix`, before the standard structural
    /// mapping would turn it into a plain JSON object. This is the hook that makes Tier 2 worth
    /// having: a downstream type keeps its identity across the boundary.
    fn from_js_custom(js: &JsValue) -> Result<Option<Self>, Error> {
        if !js.is_object() || js_sys::Array::is_array(js) {
            return Ok(None);
        }
        let get = |k: &str| js_sys::Reflect::get(js, &JsValue::from_str(k)).ok();
        let (rows, cols, data) = match (get("rows"), get("cols"), get("data")) {
            (Some(r), Some(c), Some(d))
                if !r.is_undefined() && !c.is_undefined() && js_sys::Array::is_array(&d) =>
            {
                (r, c, d)
            }
            _ => return Ok(None),
        };

        let rows = rows.as_f64().ok_or_else(|| {
            Error::from_error(ErrorType::ConversionError, "matrix rows must be a number".to_string())
        })? as usize;
        let cols = cols.as_f64().ok_or_else(|| {
            Error::from_error(ErrorType::ConversionError, "matrix cols must be a number".to_string())
        })? as usize;

        let array = js_sys::Array::from(&data);
        let mut values = Vec::with_capacity(array.length() as usize);
        for item in array.iter() {
            values.push(item.as_f64().ok_or_else(|| {
                Error::from_error(
                    ErrorType::ConversionError,
                    "matrix data must contain only numbers".to_string(),
                )
            })?);
        }
        if values.len() != rows * cols {
            return Err(Error::from_error(
                ErrorType::ConversionError,
                format!("matrix is {rows}x{cols} but carries {} values", values.len()),
            ));
        }

        Ok(Some(MatrixExt::Matrix(Arc::new(Matrix {
            rows,
            cols,
            data: values,
        }))))
    }

    fn to_js_custom(&self) -> Result<Option<JsValue>, Error> {
        match self {
            MatrixExt::Matrix(m) => {
                let obj = js_sys::Object::new();
                let set = |k: &str, v: &JsValue| {
                    js_sys::Reflect::set(&obj, &JsValue::from_str(k), v).map_err(|_| {
                        Error::from_error(
                            ErrorType::ConversionError,
                            format!("could not set {k} on the matrix"),
                        )
                    })
                };
                set("rows", &JsValue::from_f64(m.rows as f64))?;
                set("cols", &JsValue::from_f64(m.cols as f64))?;
                let data = js_sys::Array::new();
                for v in &m.data {
                    data.push(&JsValue::from_f64(*v));
                }
                set("data", &data)?;
                Ok(Some(obj.into()))
            }
            MatrixExt::Foreign(_) => Ok(None),
        }
    }

    fn from_js_opaque(_js: JsValue, opaque: JsOpaque) -> Result<Self, Error> {
        Ok(MatrixExt::Foreign(Arc::new(opaque)))
    }

    fn as_js_opaque(&self) -> Result<Option<&JsOpaque>, Error> {
        match self {
            MatrixExt::Foreign(o) => Ok(Some(o.as_ref())),
            MatrixExt::Matrix(_) => Ok(None),
        }
    }
}

/// The downstream environment, generic in exactly the way Phase 2 requires.
pub type TestEnvironment = liquers_lib::environment::DefaultEnvironment<TestValue, ()>;

// ---------------------------------------------------------------------------
// Instantiation: every generic function, at the second value type
// ---------------------------------------------------------------------------

/// Names every generic entry point at `TestValue`.
///
/// Never called — its purpose is to fail *compilation* if any of these stops being generic. That
/// is the regression guard for Phase 2's hard rule; a runtime test could not catch the same thing,
/// because a function that has quietly become concrete simply would not be reachable from here.
#[allow(dead_code, unused_must_use)]
fn instantiate_every_generic_entry_point() {
    // bridge — conversion in both directions, plus the opaque opt-in.
    let _: fn(&JsValue, ConversionPolicy) -> Result<TestValue, Error> = js_to_value::<TestValue>;
    let _: fn(&TestValue) -> Result<JsValue, Error> = value_to_js::<TestValue>;
    let _: fn(JsValue) -> Result<TestValue, Error> = opaque_value::<TestValue>;

    // command — registration and invocation.
    let _: fn(
        &mut liquers_core::commands::CommandRegistry<TestEnvironment>,
        liquers_web::command::JsCommandSpec,
    ) -> Result<(), Error> = liquers_web::command::register_js_command::<TestEnvironment>;

    // eval — the Promise bridge and the query evaluator.
    let _: fn(
        liquers_core::context::EnvRef<TestEnvironment>,
        liquers_core::query::Query,
    ) -> js_sys::Promise = liquers_web::eval::evaluate_to_promise::<TestEnvironment>;

    // asset — resolving a query to an asset without waiting.
    fn takes_asset_getter<E: liquers_core::context::Environment>(
        _: fn(
            liquers_core::context::EnvRef<E>,
            liquers_core::query::Query,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<liquers_core::assets::AssetRef<E>, Error>>>,
        >,
    ) {
    }
    let _ = takes_asset_getter::<TestEnvironment>;

    // `evaluate_query` and `call_js_command` are `async fn`, so they are named through a call in a
    // never-executed async block rather than as function pointers.
    let _ = async {
        let envref: liquers_core::context::EnvRef<TestEnvironment> = unreachable!();
        let query: liquers_core::query::Query = unreachable!();
        liquers_web::eval::evaluate_query::<TestEnvironment>(envref.clone(), query.clone()).await;
        liquers_web::asset::get_asset_for::<TestEnvironment>(envref, query).await;
    };
}

// ---------------------------------------------------------------------------
// Reduced conversion suite — the generic path must *behave*, not merely compile
// ---------------------------------------------------------------------------

fn conv(js: &JsValue) -> Result<TestValue, Error> {
    js_to_value::<TestValue>(js, ConversionPolicy::Structural)
}

/// VALUE01 (reduced) — primitives round-trip through the second value type.
#[wasm_bindgen_test]
fn value01_primitive_roundtrip_second_value_type() {
    let cases: Vec<JsValue> = vec![
        JsValue::from_str("text"),
        JsValue::from_f64(42.0),
        JsValue::from_f64(1.5),
        JsValue::from_bool(true),
        JsValue::NULL,
    ];
    for js in cases {
        let value = conv(&js).expect("convert into TestValue");
        let back = value_to_js(&value).expect("convert back");
        assert_eq!(
            js_sys::JSON::stringify(&back).ok().and_then(|s| s.as_string()),
            js_sys::JSON::stringify(&js).ok().and_then(|s| s.as_string()),
            "round trip changed the value"
        );
    }
}

/// VALUE04 (reduced) — bytes are not confused with text through the second value type.
#[wasm_bindgen_test]
fn value04_bytes_are_not_confused_with_text_second_value_type() {
    let bytes = js_sys::Uint8Array::from(&[0u8, 1, 2, 255][..]);
    let value = conv(&bytes.into()).expect("convert bytes");
    assert_eq!(value.identifier(), "bytes", "bytes must not become text");

    let back = value_to_js(&value).expect("convert back");
    assert!(
        back.is_instance_of::<js_sys::Uint8Array>(),
        "bytes must return as a Uint8Array, not an array of numbers"
    );
    let back = js_sys::Uint8Array::new(&back);
    assert_eq!(back.to_vec(), vec![0u8, 1, 2, 255]);

    // And a string is not bytes.
    let text = conv(&JsValue::from_str("hello")).expect("convert text");
    assert_ne!(text.identifier(), "bytes");
}

/// VALUE09 (reduced) — the checked upcast and downcast of the downstream extension.
#[wasm_bindgen_test]
fn value09_checked_upcast_and_downcast_second_value_type() {
    // The custom hook recognizes a matrix-shaped object and keeps its identity.
    let obj = js_sys::Object::new();
    js_sys::Reflect::set(&obj, &"rows".into(), &JsValue::from_f64(2.0)).expect("rows");
    js_sys::Reflect::set(&obj, &"cols".into(), &JsValue::from_f64(2.0)).expect("cols");
    let data = js_sys::Array::new();
    for v in [1.0, 2.0, 3.0, 4.0] {
        data.push(&JsValue::from_f64(v));
    }
    js_sys::Reflect::set(&obj, &"data".into(), &data).expect("data");

    let value = conv(&obj.into()).expect("convert a matrix");
    assert_eq!(
        value.identifier(),
        "matrix",
        "the custom hook must claim the value before the structural mapping does"
    );

    // Downcast: the concrete type is recoverable.
    let matrix = match &value {
        CombinedValue::Extended(MatrixExt::Matrix(m)) => m.clone(),
        other => panic!("expected a matrix, got {other:?}"),
    };
    assert_eq!(matrix.rows, 2);
    assert_eq!(matrix.data, vec![1.0, 2.0, 3.0, 4.0]);

    // And it survives the return trip structurally, which is what Tier 1 could not do.
    let back = value_to_js(&value).expect("convert back");
    assert_eq!(
        js_sys::Reflect::get(&back, &"rows".into()).ok().and_then(|v| v.as_f64()),
        Some(2.0)
    );

    // A checked downcast of the *wrong* type fails rather than producing nonsense.
    let text = conv(&JsValue::from_str("not a matrix")).expect("convert text");
    assert!(
        !matches!(text, CombinedValue::Extended(MatrixExt::Matrix(_))),
        "a string must not downcast to a matrix"
    );

    // A malformed matrix is refused, not silently truncated.
    let bad = js_sys::Object::new();
    js_sys::Reflect::set(&bad, &"rows".into(), &JsValue::from_f64(3.0)).expect("rows");
    js_sys::Reflect::set(&bad, &"cols".into(), &JsValue::from_f64(3.0)).expect("cols");
    js_sys::Reflect::set(&bad, &"data".into(), &js_sys::Array::new()).expect("data");
    let err = conv(&bad.into()).expect_err("a 3x3 matrix with no data must be refused");
    assert_eq!(err.error_type, ErrorType::ConversionError);
}

/// The opaque path works at the second value type too — Tier 1 and Tier 2 are not exclusive.
#[wasm_bindgen_test]
fn web_opaque_roundtrip_second_value_type() {
    let original = js_sys::Date::new_0();
    let value = opaque_value::<TestValue>(original.clone().into()).expect("retain opaquely");
    assert!(
        matches!(value, CombinedValue::Extended(MatrixExt::Foreign(_))),
        "the downstream type decides where an opaque value lives"
    );

    let back = value_to_js(&value).expect("convert back");
    assert!(
        back.loose_eq(&original),
        "an opaque value returns the original object by identity"
    );
}

/// A JavaScript command registered against the second value type executes end to end.
///
/// The strongest form of the Tier-2 claim: not only do the generic functions compile at
/// `TestValue`, the whole pipeline — spec parsing, registration, planning, invocation, conversion
/// in both directions — runs on a value type this crate has never heard of.
#[wasm_bindgen_test]
async fn web_second_value_type_evaluates_a_javascript_command() {
    use liquers_core::context::Environment;

    let mut env = TestEnvironment::new();
    let spec = js_sys::Object::new();
    js_sys::Reflect::set(&spec, &"name".into(), &"make_matrix".into()).expect("name");
    js_sys::Reflect::set(
        &spec,
        &"run".into(),
        &js_sys::Function::new_no_args("return { rows: 1, cols: 2, data: [7, 8] };"),
    )
    .expect("run");

    let parsed = liquers_web::command::JsCommandSpec::parse(&spec.into()).expect("parse the spec");
    liquers_web::command::register_js_command(&mut env.command_registry, parsed)
        .expect("register against the downstream environment");

    let envref = env.to_ref();
    let query = liquers_core::parse::parse_query("make_matrix").expect("parse the query");
    let result = liquers_web::eval::evaluate_query(envref, query)
        .await
        .expect("evaluate on the downstream environment");

    // The command's plain object came back as a *matrix*: the downstream hook claimed it on the
    // way in, and rendered it on the way out.
    assert_eq!(
        js_sys::Reflect::get(&result, &"rows".into()).ok().and_then(|v| v.as_f64()),
        Some(1.0)
    );
    assert_eq!(
        js_sys::Reflect::get(&result, &"data".into())
            .ok()
            .map(|d| js_sys::Array::from(&d).length()),
        Some(2)
    );
}
