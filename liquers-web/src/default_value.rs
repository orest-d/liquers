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
impl JsExtensionBridge for ExtValue {
    fn from_js_custom(_js: &JsValue) -> Result<Option<Self>, Error> {
        // The default value type has no JavaScript-specific structural cases beyond the standard
        // mapping, so it always falls through.
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
                Ok(Some(crate::records::LiquersRecordBatch::from(batch).into()))
            }
            // A source has no JavaScript-specific representation yet — it falls through to the
            // standard mapping, which refuses it (a source serializes only as its manifest, and
            // that byte form is not what structural conversion produces).
            #[cfg(feature = "records")]
            ExtValue::RecordSource { .. } => Ok(None),
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
