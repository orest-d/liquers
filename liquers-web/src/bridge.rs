//! The generic value bridge: [`JsValueBridge`] plus the conversion functions built on it.
//!
//! # The rule this module exists to enforce
//!
//! **Nothing here may name a concrete value type.** Every item is generic over `V: JsValueBridge`.
//! `wasm-bindgen` cannot export generic types — it needs monomorphic types to generate JS glue —
//! so `liquers-web` exports a concrete surface built on `liquers_lib::value::Value`, while the
//! machinery underneath stays generic. That is what lets a downstream crate supply its own value
//! type and environment (the "Tier 2" path in
//! `specs/liquers-web/phase2-architecture.md`, "Extensibility"): it implements [`JsValueBridge`]
//! for its own type and reuses these functions unchanged.
//!
//! Most third-party JavaScript types need none of this. A value that only passes *through* Liquers
//! from one JavaScript command to another is carried opaquely by
//! [`crate::value::JsOpaque`] — no custom value type, and no O(size) conversion.

use liquers_core::error::{Error, ErrorType};
use liquers_core::value::ValueInterface;
use std::sync::Arc;
use wasm_bindgen::{JsCast, JsValue};

use crate::value::JsOpaque;

/// How aggressively an unrecognised JavaScript value is converted.
///
/// The default is [`ConversionPolicy::Structural`]: a value Liquers does not recognise is an
/// error rather than being silently wrapped, so opacity is always something the caller asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ConversionPolicy {
    /// Convert structurally; an unrecognised object is a `ConversionError`.
    #[default]
    Structural,
    /// Convert structurally where possible, retain anything else opaquely. Selected by the
    /// explicit `opaque()` opt-in, never inferred.
    AllowOpaque,
}

/// The one trait a value type implements to join the JavaScript bridge.
///
/// Implemented here for `liquers_lib::value::Value` (see [`crate::default_value`]); a downstream
/// crate implements it for its own type to reuse every function in this module.
pub trait JsValueBridge: ValueInterface + Sized {
    /// Structural conversion of a JavaScript value this type understands specially.
    ///
    /// `Ok(None)` means "not mine, fall through to the standard mapping" — it is not a failure.
    fn from_js_custom(js: &JsValue) -> Result<Option<Self>, Error>;

    /// Inverse of [`Self::from_js_custom`]. `Ok(None)` falls back to the standard mapping.
    fn to_js_custom(&self) -> Result<Option<JsValue>, Error>;

    /// Wraps an opaque JavaScript value.
    ///
    /// Returning `Err(NotSupported)` is a legitimate implementation: it opts the value type out of
    /// opaque retention entirely, at the cost of failing the `opaque()` entry point.
    fn from_js_opaque(js: JsValue, opaque: JsOpaque) -> Result<Self, Error>;

    /// Recovers a retained JavaScript value.
    ///
    /// `Ok(None)` means this value is not an opaque foreign value at all. `Err` means it *is* one
    /// but belongs to a different language runtime — the error names its origin, which is what
    /// makes a cross-language mistake diagnosable rather than merely failed.
    fn as_js_opaque(&self) -> Result<Option<&JsOpaque>, Error>;
}

/// Maximum depth for structural conversion.
///
/// A cyclic object would otherwise recurse until the wasm stack is exhausted, which aborts the
/// whole module. Depth is bounded and exceeding it is a typed error naming the path, so a cycle is
/// reported rather than silently truncated.
const MAX_DEPTH: usize = 64;

/// Converts a JavaScript value into the Liquers value type.
///
/// Generic over `V` — see the module rule.
pub fn js_to_value<V: JsValueBridge>(js: &JsValue, policy: ConversionPolicy) -> Result<V, Error> {
    js_to_value_at(js, policy, 0, "$")
}

fn js_to_value_at<V: JsValueBridge>(
    js: &JsValue,
    policy: ConversionPolicy,
    depth: usize,
    path: &str,
) -> Result<V, Error> {
    if depth > MAX_DEPTH {
        return Err(Error::from_error(
            ErrorType::ConversionError,
            format!(
                "Structural conversion exceeded depth {MAX_DEPTH} at {path} — the value is \
                 probably cyclic. Use the opaque opt-in to retain it by identity instead."
            ),
        ));
    }

    // A value type may claim a JavaScript value before the standard mapping sees it.
    if let Some(v) = V::from_js_custom(js)? {
        return Ok(v);
    }

    // null and undefined both collapse to none. Documented as lossy: JavaScript distinguishes
    // them, Liquers does not.
    if js.is_null() || js.is_undefined() {
        return Ok(V::none());
    }
    if let Some(b) = js.as_bool() {
        return Ok(V::from_bool(b));
    }
    if let Some(n) = js.as_f64() {
        return Ok(number_to_value::<V>(n));
    }
    if let Some(s) = js.as_string() {
        return Ok(V::from_string(s));
    }
    if js.is_bigint() {
        return bigint_to_value::<V>(js, path);
    }
    // Bytes before objects: a Uint8Array is an object, and must never be mistaken for text or
    // degraded into an array of numbers at the top level.
    if let Some(bytes) = as_byte_vec(js) {
        return Ok(V::from_bytes(bytes));
    }
    if js_sys::Array::is_array(js) {
        let json = js_to_json(js, depth, path)?;
        return V::try_from_json_value(&json);
    }
    if is_plain_object(js) {
        let json = js_to_json(js, depth, path)?;
        return V::try_from_json_value(&json);
    }

    // Anything else — a class instance, Map, Set, Date, DOM node, or function — is only carried
    // when the caller explicitly asked for it. Silent opacity is what the opt-in exists to prevent.
    match policy {
        ConversionPolicy::AllowOpaque => opaque_value::<V>(js.clone()),
        ConversionPolicy::Structural => Err(Error::from_error(
            ErrorType::ConversionError,
            format!(
                "Cannot structurally convert the JavaScript value at {path} (type {}). Wrap it \
                 with liquers.opaque(...) to retain it by identity.",
                crate::value::JsOpaque::new(js.clone()).type_tag()
            ),
        )),
    }
}

/// JavaScript has one number type. An integral value inside the exactly-representable range
/// becomes an integer; everything else stays a float, including NaN and the infinities.
fn number_to_value<V: JsValueBridge>(n: f64) -> V {
    const MAX_EXACT: f64 = 9_007_199_254_740_992.0; // 2^53
    if n.fract() == 0.0 && n.abs() <= MAX_EXACT {
        V::from_i64(n as i64)
    } else {
        V::from_f64(n)
    }
}

/// `BigInt` is the lossless integer path. Out of `i64` range is a typed error rather than a silent
/// wraparound — the whole point of using `BigInt` is that the caller cares about exactness.
fn bigint_to_value<V: JsValueBridge>(js: &JsValue, path: &str) -> Result<V, Error> {
    // A BigInt is not a string and `as_string()` yields None for it; it has to be stringified
    // through its own `toString`, in base 10.
    let bigint = js_sys::BigInt::from(js.clone());
    let text = bigint
        .to_string(10)
        .ok()
        .and_then(|s| s.as_string())
        .ok_or_else(|| {
            Error::from_error(
                ErrorType::ConversionError,
                format!("Could not read the BigInt at {path}"),
            )
        })?;
    text.parse::<i64>().map(V::from_i64).map_err(|_| {
        Error::from_error(
            ErrorType::ConversionError,
            format!("BigInt {text} at {path} is outside the range of a 64-bit integer"),
        )
    })
}

/// Recognises `Uint8Array` and `ArrayBuffer`, the two byte carriers.
fn as_byte_vec(js: &JsValue) -> Option<Vec<u8>> {
    if js.is_instance_of::<js_sys::Uint8Array>() {
        return Some(js_sys::Uint8Array::from(js.clone()).to_vec());
    }
    if js.is_instance_of::<js_sys::ArrayBuffer>() {
        let buffer: js_sys::ArrayBuffer = js.clone().unchecked_into();
        return Some(js_sys::Uint8Array::new(&buffer).to_vec());
    }
    None
}

/// A "plain object" is one whose constructor is `Object`, or which has no prototype at all.
/// Everything else is a class instance and is not structurally converted.
fn is_plain_object(js: &JsValue) -> bool {
    if !js.is_object() {
        return false;
    }
    let proto = js_sys::Object::get_prototype_of(js);
    if proto.is_null() || proto.is_undefined() {
        return true; // Object.create(null)
    }
    js_sys::Reflect::get(js, &JsValue::from_str("constructor"))
        .ok()
        .and_then(|c| js_sys::Reflect::get(&c, &JsValue::from_str("name")).ok())
        .and_then(|n| n.as_string())
        .map(|n| n == "Object")
        .unwrap_or(false)
}

/// Converts an array or plain object into `serde_json::Value`.
///
/// Compound values go through JSON because [`ValueInterface`] offers no generic array/object
/// constructor — `try_from_json_value` is the only shape every value type supports. **Documented
/// limitation:** a `Uint8Array` *nested* inside an array or object degrades to an array of numbers,
/// because JSON has no byte type. Top-level bytes are preserved exactly.
fn js_to_json(js: &JsValue, depth: usize, path: &str) -> Result<serde_json::Value, Error> {
    if depth > MAX_DEPTH {
        return Err(Error::from_error(
            ErrorType::ConversionError,
            format!(
                "Structural conversion exceeded depth {MAX_DEPTH} at {path} — the value is \
                 probably cyclic. Use the opaque opt-in to retain it by identity instead."
            ),
        ));
    }
    if js.is_null() || js.is_undefined() {
        return Ok(serde_json::Value::Null);
    }
    if let Some(b) = js.as_bool() {
        return Ok(serde_json::Value::Bool(b));
    }
    if let Some(n) = js.as_f64() {
        // Integral values become JSON integers. Emitting them as floats would make a nested `1`
        // arrive as `1.0`, which is a different value once it reaches the value type.
        const MAX_EXACT: f64 = 9_007_199_254_740_992.0; // 2^53
        if n.fract() == 0.0 && n.abs() <= MAX_EXACT {
            return Ok(serde_json::Value::Number((n as i64).into()));
        }
        return Ok(serde_json::Number::from_f64(n)
            .map(serde_json::Value::Number)
            // NaN and the infinities have no JSON form; a string keeps them visible rather
            // than silently becoming null.
            .unwrap_or_else(|| serde_json::Value::String(n.to_string())));
    }
    if let Some(s) = js.as_string() {
        return Ok(serde_json::Value::String(s));
    }
    if js_sys::Array::is_array(js) {
        let array = js_sys::Array::from(js);
        let mut out = Vec::with_capacity(array.length() as usize);
        for (i, item) in array.iter().enumerate() {
            out.push(js_to_json(&item, depth + 1, &format!("{path}[{i}]"))?);
        }
        return Ok(serde_json::Value::Array(out));
    }
    if is_plain_object(js) {
        let obj: &js_sys::Object = js.unchecked_ref();
        let mut map = serde_json::Map::new();
        for entry in js_sys::Object::entries(obj).iter() {
            let pair = js_sys::Array::from(&entry);
            let key = pair.get(0).as_string().ok_or_else(|| {
                Error::from_error(
                    ErrorType::ConversionError,
                    format!("Non-string object key at {path}"),
                )
            })?;
            let value = js_to_json(&pair.get(1), depth + 1, &format!("{path}.{key}"))?;
            map.insert(key, value);
        }
        return Ok(serde_json::Value::Object(map));
    }
    Err(Error::from_error(
        ErrorType::ConversionError,
        format!("Cannot structurally convert the JavaScript value at {path}"),
    ))
}

/// Converts a Liquers value into a JavaScript value.
///
/// An opaque value is handed back as the original object; a value belonging to a *different*
/// language runtime is a typed error naming that runtime.
pub fn value_to_js<V: JsValueBridge>(value: &V) -> Result<JsValue, Error> {
    if let Some(opaque) = value.as_js_opaque()? {
        return Ok(opaque.value().clone());
    }
    if let Some(js) = value.to_js_custom()? {
        return Ok(js);
    }
    if value.is_none() {
        return Ok(JsValue::NULL);
    }
    // Bytes before JSON: `try_into_json_value` would render them as an array of numbers, losing
    // the distinction VALUE04 exists to preserve.
    //
    // Gate on the identifier, NOT on `try_into_bytes()` succeeding: that conversion is
    // deliberately permissive and also accepts text, returning its UTF-8 encoding — so keying off
    // it turned every string into a Uint8Array.
    if value.identifier() == "bytes" {
        let bytes = value.try_into_bytes()?;
        return Ok(js_sys::Uint8Array::from(bytes.as_slice()).into());
    }
    let json = value.try_into_json_value()?;
    json_to_js(&json)
}

fn json_to_js(json: &serde_json::Value) -> Result<JsValue, Error> {
    serde_wasm_bindgen::to_value(json).map_err(|e| {
        Error::from_error(
            ErrorType::ConversionError,
            format!("Could not convert a Liquers value to JavaScript: {e}"),
        )
    })
}

/// Wraps a JavaScript value opaquely — the explicit opt-in behind `liquers.opaque(x)`.
pub fn opaque_value<V: JsValueBridge>(js: JsValue) -> Result<V, Error> {
    let opaque = JsOpaque::new(js.clone());
    V::from_js_opaque(js, opaque)
}

/// Downcasts a foreign value to this crate's JavaScript wrapper.
///
/// `Err` when the value belongs to another language runtime, naming its origin — the cross-language
/// rejection path that `POLYGLOT03`/`POLYGLOT04` require.
pub fn downcast_js(value: &Arc<dyn liquers_lib::value::foreign::ForeignValue>) -> Result<&JsOpaque, Error> {
    value.as_any().downcast_ref::<JsOpaque>().ok_or_else(|| {
        Error::conversion_error(
            value.origin(),
            "a JavaScript value (it belongs to a different language runtime)",
        )
    })
}
