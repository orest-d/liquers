//! The JavaScript side of the value bridge.
//!
//! [`JsOpaque`] is `liquers-web`'s implementation of [`ForeignValue`] — a JavaScript value retained
//! by identity rather than copied. It lives here rather than in `liquers-lib` so that `liquers-lib`
//! never names `JsValue`, which is what lets Starlark and Python implement the same trait elsewhere
//! without any of them meeting.
//!
//! See `specs/design/liquers-web/phase1-high-level-design.md` decision 2 for the semantics: structural
//! conversion is the default, opacity is an explicit opt-in, and identity is *not* promised.

use std::borrow::Cow;
use std::sync::Arc;

use liquers_core::error::{Error, ErrorType};
use liquers_lib::value::foreign::ForeignValue;
use wasm_bindgen::JsValue;

/// The `origin` tag reported by every value this crate wraps.
pub const ORIGIN_JAVASCRIPT: &str = "javascript";

/// The type identifier of a retained JavaScript value.
///
/// A **constant** because the identifier is needed in two places that cannot see each other: the
/// instance method [`ForeignValue::identifier`], which the value path calls through
/// `Arc<dyn ForeignValue>`, and [`js_value_type_info`], which registers the type before any value
/// exists. If the two ever disagreed, a `JsOpaque` would report an identifier the registry does
/// not contain — the exact failure registration exists to prevent, but caused by a typo rather
/// than by a structural gap and therefore much harder to find. The type system cannot enforce the
/// agreement: `ForeignValue` must stay object-safe, and a default body is type-checked with
/// `Self: ?Sized`, so it cannot call an associated function. A constant and
/// `the_constant_and_the_instance_agree` are what close it.
///
/// `js.Value`, not a bare `js`: a bare name asserts that Liquers owns the concept and is reserved
/// for `liquers-core` and `liquers-lib`. See `specs/reference/VALUE_TYPE_SYSTEM.md`.
pub const JS_VALUE_TYPE_IDENTIFIER: &str = "js.Value";

/// The registry entry for a retained JavaScript value — the single construction site.
///
/// Registered by `environment::new_environment`, which every rebuild path funnels through, so the
/// registration needs no retention and cannot drift from what a value reports.
pub fn js_value_type_info() -> liquers_core::type_system::TypeInfo {
    liquers_core::type_system::TypeInfo::new(JS_VALUE_TYPE_IDENTIFIER)
        .with_type_name("JsValue")
        .with_defaults("json", "json", "application/json", "value.json")
    // Deliberately no `with_data_formats`: `JsOpaque::as_bytes` refuses, so the type has no byte
    // form. The write path exempts a formatless type from the format check exactly as it exempts
    // a UI element; declaring a format here would instead let `set_binary` accept bytes that can
    // never be materialized.
}

/// An owned handle to a JavaScript value, retained by identity.
///
/// Cloning is a refcount bump on the `wasm-bindgen` heap table and dropping releases the slot, so
/// lifetime is automatic: no registry, no ambient thread-local, no hand-rolled refcounting. The
/// cost of that is real and deliberate — while such a value sits in an asset, it pins whatever it
/// references (a DOM subtree, a large `ArrayBuffer`) for the lifetime of that asset. The explicit
/// opt-in is what keeps that from happening by accident.
///
/// Identity is *not* a guarantee. Liquers promises that the same query evaluates to the same value
/// when it is neither volatile nor expired; that is a statement about query determinism, not about
/// preserving a JavaScript object graph. `===` will often hold in practice, because a cached asset
/// hands back the identical object — treat that as incidental.
#[derive(Clone)]
pub struct JsOpaque {
    value: JsValue,
    /// `constructor.name` captured at wrap time, or `"object"`. Used for `type_name`, so that
    /// metadata and error messages identify *which* JavaScript object is involved.
    type_tag: Arc<str>,
}

impl JsOpaque {
    /// Wraps a JavaScript value, capturing its constructor name for diagnostics.
    pub fn new(value: JsValue) -> Self {
        let type_tag = constructor_name(&value);
        JsOpaque { value, type_tag }
    }

    /// The retained JavaScript value.
    pub fn value(&self) -> &JsValue {
        &self.value
    }

    /// The captured constructor name.
    pub fn type_tag(&self) -> &str {
        &self.type_tag
    }
}

/// Deliberately does not delegate to `JsValue`'s `Debug`, which can call into JavaScript and is
/// unsuitable inside an error path.
impl core::fmt::Debug for JsOpaque {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Js({})", self.type_tag)
    }
}

impl ForeignValue for JsOpaque {
    fn origin(&self) -> &'static str {
        ORIGIN_JAVASCRIPT
    }

    fn as_any(&self) -> &dyn core::any::Any {
        self
    }

    fn identifier(&self) -> Cow<'static, str> {
        JS_VALUE_TYPE_IDENTIFIER.into()
    }

    fn type_name(&self) -> Cow<'static, str> {
        Cow::Owned(self.type_tag.to_string())
    }

    /// Refines the registered description with *this* instance's constructor name.
    ///
    /// The registered entry says `JsValue`; an instance says `Uint8Array`, `Date`, or whatever
    /// the object's constructor is called. That divergence is the type-axis/`type_name` split
    /// working as designed — `type_name` is informational and is never dispatched on, and the
    /// write path requires only that it be non-empty.
    fn type_info(&self) -> liquers_core::type_system::TypeInfo {
        js_value_type_info().with_type_name(self.type_name())
    }

    fn default_extension(&self) -> Cow<'static, str> {
        "json".into()
    }

    fn default_filename(&self) -> Cow<'static, str> {
        "value.json".into()
    }

    fn default_media_type(&self) -> Cow<'static, str> {
        "application/json".into()
    }

    /// Refuses. A JavaScript `String(obj)` coercion is not a faithful text conversion, and
    /// producing one silently would hide that the value never had a text form.
    fn try_into_string(&self) -> Result<String, Error> {
        Err(Error::conversion_error(self.type_tag.as_ref(), "string"))
    }

    /// Refuses by default. Structural degradation at this boundary is opt-in, because a class
    /// instance quietly coming back as a plain object — only after a cache eviction — is a bad
    /// debugging experience.
    fn try_into_json_value(&self) -> Result<serde_json::Value, Error> {
        Err(Error::conversion_error(self.type_tag.as_ref(), "JSON"))
    }

    /// Refuses, which the asset layer already tolerates: it falls back to a time-based version and
    /// to metadata-only persistence, so an opaque value degrades instead of breaking evaluation.
    /// The cost is that such assets look freshly changed to dependency tracking.
    fn as_bytes(&self, format: &str) -> Result<Vec<u8>, Error> {
        Err(Error::from_error(
            ErrorType::SerializationError,
            format!(
                "Serialization to {} not supported by JavaScript value {}",
                format, self.type_tag
            ),
        ))
    }
}

/// Reads `value.constructor.name`, falling back to `"object"`.
///
/// Best-effort by design: a null-prototype object, a `Proxy` or a minified class all legitimately
/// yield something unhelpful, and none of those is an error — the tag is for diagnostics, never for
/// dispatch.
fn constructor_name(value: &JsValue) -> Arc<str> {
    let ctor = js_sys::Reflect::get(value, &JsValue::from_str("constructor"));
    let name = ctor
        .ok()
        .and_then(|c| js_sys::Reflect::get(&c, &JsValue::from_str("name")).ok())
        .and_then(|n| n.as_string())
        .filter(|n| !n.is_empty());
    match name {
        Some(n) => Arc::from(n.as_str()),
        None => Arc::from("object"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::*;

    /// `fvt5.1` — the constant, the registered description and an instance all agree.
    ///
    /// **This is the guarantee chosen in place of compile-time enforcement.** The identifier is
    /// needed statically (to register the type before any value exists) and per-instance (through
    /// `Arc<dyn ForeignValue>`), and Rust cannot tie the two together: `ForeignValue` must stay
    /// object-safe, so `type_info` takes `&self`, and a default body is type-checked with
    /// `Self: ?Sized` and cannot call an associated function. A shared constant plus this test are
    /// what keep them honest. See `specs/design/foreign-value-type-registration/`.
    #[wasm_bindgen_test]
    fn the_constant_and_the_instance_agree() {
        let opaque = JsOpaque::new(JsValue::from_str("x"));

        assert_eq!(js_value_type_info().type_identifier, JS_VALUE_TYPE_IDENTIFIER);
        assert_eq!(opaque.identifier(), JS_VALUE_TYPE_IDENTIFIER);
        assert_eq!(opaque.type_info().type_identifier, JS_VALUE_TYPE_IDENTIFIER);
    }

    /// The registered entry declares no data formats, because `as_bytes` refuses.
    ///
    /// Declaring one would let `set_binary` accept bytes for a `js.Value` asset that could never
    /// be materialized, moving the failure from write time to read time.
    #[wasm_bindgen_test]
    fn the_registered_type_has_no_byte_form() {
        assert!(
            js_value_type_info().supported_data_formats.is_empty(),
            "a retained JavaScript value has no byte form"
        );
        assert!(JsOpaque::new(JsValue::from_str("x"))
            .as_bytes("json")
            .is_err());
    }

    /// `type_name` is per instance while the identifier is not — the type-axis split.
    #[wasm_bindgen_test]
    fn the_instance_refines_only_the_type_name() {
        let bytes = js_sys::Uint8Array::new_with_length(1);
        let opaque = JsOpaque::new(bytes.into());

        assert_eq!(opaque.type_info().type_identifier, JS_VALUE_TYPE_IDENTIFIER);
        assert_eq!(
            opaque.type_info().type_name, "Uint8Array",
            "the constructor name reaches the description"
        );
        assert_eq!(
            js_value_type_info().type_name,
            "JsValue",
            "while the registered entry keeps the generic name"
        );
    }
}
