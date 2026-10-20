//! [`JsValueBridge`] for the default value type.
//!
//! This is the **only** module that names a concrete value type. Everything in
//! [`crate::bridge`] stays generic so a downstream crate can substitute its own; this module is
//! what makes `liquers-web`'s own exported surface concrete.

use liquers_core::error::{Error, ErrorType};
use liquers_lib::value::ExtValue;
use std::sync::Arc;
use wasm_bindgen::JsValue;

use crate::bridge::JsExtensionBridge;
use crate::value::JsOpaque;

/// The value type this crate's exported surface is built on.
///
/// `liquers_lib::value::Value` is `CombinedValue<SimpleValue, ExtValue>`, selected by linking
/// `liquers-lib` with `default-features = false, features = ["webui"]` — `polars` and `egui` are
/// not wasm targets. Named here so the choice has one place to be stated, asserted and changed
/// (`PACKAGE05`).
pub type WebValue = liquers_lib::value::Value;

// Implemented on the *extension*, not on `Value`. `bridge`'s blanket impl carries it up to
// `CombinedValue<SimpleValue, ExtValue>`, and this is the same path a downstream crate takes —
// so the documented Tier-2 route is the one this crate itself uses, rather than a second-class
// alternative to it.
// This crate's `records` feature must agree with liquers-lib's, because the matches below gate their
// record arms on it. Cargo can turn `liquers-lib/records` on through another crate in the same
// build while leaving this one off; the match then fails as non-exhaustive, and this assertion is
// what says why.
const _: () = assert!(
    liquers_lib::RECORDS_ENABLED == cfg!(feature = "records"),
    "liquers-lib's `records` feature is enabled in this build but liquers-web's is not (or the \
     reverse): enable `liquers-web/records` too"
);

impl JsExtensionBridge for ExtValue {
    fn from_js_custom(js: &JsValue) -> Result<Option<Self>, Error> {
        // A record handle the page received crosses back as the value it was made from — the same
        // `Arc`, with its schema, roles and origin — rather than through a JSON round trip.
        // Everything else falls through to the standard mapping.
        #[cfg(feature = "records")]
        if let Some(value) = record_handle_value(js)? {
            return Ok(Some(value));
        }
        #[cfg(not(feature = "records"))]
        let _ = js;
        Ok(None)
    }

    fn to_js_custom(&self) -> Result<Option<JsValue>, Error> {
        match self {
            ExtValue::Image { .. } => Ok(None),
            ExtValue::Foreign { .. } => Ok(None),
            ExtValue::UIElement { .. } => Ok(None),
            #[cfg(feature = "polars")]
            ExtValue::PolarsDataFrame { .. } => Ok(None),
            #[cfg(feature = "egui")]
            ExtValue::UiCommand { .. } => Ok(None),
            #[cfg(feature = "egui")]
            ExtValue::Widget { .. } => Ok(None),
            // The wasm route: a view crossing into JavaScript is materialized first (Phase 2's
            // "The wasm route: a dedicated safe mechanism, not Arrow") so the handle's `Arc`
            // keeps buffers the descriptor points into alive for as long as JS holds it.
            //
            // `RECORDS10`: a single-cell view reaches JavaScript as its own scalar, as
            // `liquers-lib`'s scalar path reads it; only a larger view crosses as a batch.
            #[cfg(feature = "records")]
            ExtValue::RecordView { value } => {
                if is_single_cell(value.as_ref()) {
                    return single_cell_to_js(self, value.as_ref()).map(Some);
                }
                let batch = value.materialize()?;
                Ok(Some(
                    crate::records::LiquersRecordBatch::from_view(value.clone(), batch).into(),
                ))
            }
            // A source crosses as a handle, never materialized: its chunks are read only when the
            // page traverses it.
            #[cfg(feature = "records")]
            ExtValue::RecordSource { value } => Ok(Some(
                crate::records::LiquersRecordSource::from(value.clone()).into(),
            )),
        }
    }

    fn from_js_opaque(_js: JsValue, opaque: JsOpaque) -> Result<Self, Error> {
        Ok(ExtValue::Foreign {
            value: Arc::new(opaque),
        })
    }

    fn as_js_opaque(&self) -> Result<Option<&JsOpaque>, Error> {
        match self {
            ExtValue::Foreign { value } => match value.as_any().downcast_ref::<JsOpaque>() {
                Some(js) => Ok(Some(js)),
                // The value *is* opaque, but belongs to another language runtime. Naming the
                // origin is what turns a cross-language mistake into a diagnosable one.
                None => Err(Error::conversion_error(
                    value.origin(),
                    "a JavaScript value (it belongs to a different language runtime)",
                )),
            },
            // Not an opaque value at all — not an error, just nothing to recover.
            ExtValue::Image { .. } => Ok(None),
            #[cfg(feature = "polars")]
            ExtValue::PolarsDataFrame { .. } => Ok(None),
            #[cfg(feature = "egui")]
            ExtValue::UiCommand { .. } => Ok(None),
            #[cfg(feature = "egui")]
            ExtValue::Widget { .. } => Ok(None),
            ExtValue::UIElement { .. } => Ok(None),
            // Neither is an opaque foreign value — `to_js_custom` is what converts these.
            #[cfg(feature = "records")]
            ExtValue::RecordView { .. } => Ok(None),
            #[cfg(feature = "records")]
            ExtValue::RecordSource { .. } => Ok(None),
        }
    }
}

/// The value a record handle stands for, when `js` is one; `None` for anything else.
///
/// A handle is recognized by its `__liquersRecordHandle` property (a getter on the class), checked
/// with `in` so an ordinary object is never asked for it. A freed handle's getter throws, and a
/// handle whose id the registry no longer holds has been freed too; both are refused, naming the
/// handle's class.
#[cfg(feature = "records")]
fn record_handle_value(js: &JsValue) -> Result<Option<ExtValue>, Error> {
    use crate::records::RecordHandle;
    let marker = JsValue::from_str("__liquersRecordHandle");
    if !is_record_handle(js, &marker) {
        return Ok(None);
    }
    let freed = || {
        let class = js_sys::Reflect::get(js, &JsValue::from_str("constructor"))
            .and_then(|c| js_sys::Reflect::get(&c, &JsValue::from_str("name")))
            .ok()
            .and_then(|name| name.as_string())
            .unwrap_or_else(|| "record".to_string());
        Error::conversion_error(format!("a freed {class} handle"), "a live record handle")
    };
    let id = js_sys::Reflect::get(js, &marker)
        .ok()
        .and_then(|id| id.as_f64())
        .ok_or_else(freed)?;
    match crate::records::lookup_handle(id as u32) {
        Some(RecordHandle::View(value)) => Ok(Some(ExtValue::RecordView { value })),
        Some(RecordHandle::Source(value)) => Ok(Some(ExtValue::RecordSource { value })),
        None => Err(freed()),
    }
}

/// Whether `js` is an instance of a record handle class, recognised by where the marker lives: a
/// getter on the object's prototype, as wasm-bindgen generates it. Data cannot pass — JSON, a
/// spread copy or a structured clone carries the marker, if at all, as an own data property —
/// so an object from outside the page is never taken for a handle.
#[cfg(feature = "records")]
fn is_record_handle(js: &JsValue, marker: &JsValue) -> bool {
    use wasm_bindgen::JsCast;
    if !js.is_object() {
        return false;
    }
    let object: &js_sys::Object = js.unchecked_ref();
    if js_sys::Object::has_own(object, marker) {
        return false;
    }
    let prototype = js_sys::Object::get_prototype_of(js);
    if prototype.is_null() || prototype.is_undefined() {
        return false;
    }
    let descriptor = js_sys::Object::get_own_property_descriptor(&prototype, marker);
    descriptor.is_object()
        && js_sys::Reflect::get(&descriptor, &JsValue::from_str("get"))
            .map(|getter| getter.is_function())
            .unwrap_or(false)
}

/// Whether a view reads as one cell — the same shape `liquers-lib`'s private
/// `record_view_is_single_cell` accepts: one row, and one payload field, or no payload field and an
/// `Id` field or a sole field.
#[cfg(feature = "records")]
fn is_single_cell(view: &dyn liquers_lib::records::RecordView) -> bool {
    let schema = view.schema();
    let payload = schema.payload_fields().len();
    view.len() == 1
        && (payload == 1
            || (payload == 0 && (schema.id_field().is_some() || schema.fields.len() == 1)))
}

/// The JavaScript scalar of a single-cell view. A null cell is `null` and a binary cell a
/// `Uint8Array`, as the standard mapping gives a bare value; every other cell takes the view's own
/// JSON scalar, so numbers, text, booleans, dates and timestamps read as `liquers-lib` reads them.
#[cfg(feature = "records")]
fn single_cell_to_js(
    ext: &ExtValue,
    view: &dyn liquers_lib::records::RecordView,
) -> Result<JsValue, Error> {
    use liquers_lib::records::FieldValue;
    use liquers_lib::value::ValueExtension;
    match view.single_cell()? {
        FieldValue::Null => Ok(JsValue::NULL),
        FieldValue::Bytes(bytes) => Ok(js_sys::Uint8Array::from(bytes.as_ref()).into()),
        FieldValue::Bool(_)
        | FieldValue::Int(_)
        | FieldValue::UInt(_)
        | FieldValue::Float(_)
        | FieldValue::Text(_)
        | FieldValue::Date(_)
        | FieldValue::Timestamp(_)
        | FieldValue::Vector(_) => {
            let json = ext.try_into_json_value()?;
            crate::bridge::serialize_to_js(&json).map_err(|e| {
                Error::from_error(
                    ErrorType::ConversionError,
                    format!("Could not convert a Liquers value to JavaScript: {e}"),
                )
            })
        }
    }
}

/// Reports whether a serialization refusal came from an opaque foreign value.
///
/// Used by the tests that pin the documented degradation: an opaque value refuses byte
/// serialization with a [`ErrorType::SerializationError`], which the asset layer absorbs.
pub fn is_serialization_refusal(err: &Error) -> bool {
    err.error_type == ErrorType::SerializationError
}
