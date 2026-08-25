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
        // `js.Value`, not a bare `js`: a bare name asserts that Liquers owns the concept, and a
        // JavaScript value is somebody else's type. See `specs/reference/VALUE_TYPE_SYSTEM.md`.
        "js.Value".into()
    }

    fn type_name(&self) -> Cow<'static, str> {
        Cow::Owned(self.type_tag.to_string())
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
