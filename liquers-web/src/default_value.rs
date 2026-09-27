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
            #[cfg(feature = "records")]
            ExtValue::RecordView { value } => {
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

/// Reports whether a serialization refusal came from an opaque foreign value.
///
/// Used by the tests that pin the documented degradation: an opaque value refuses byte
/// serialization with a [`ErrorType::SerializationError`], which the asset layer absorbs.
pub fn is_serialization_refusal(err: &Error) -> bool {
    err.error_type == ErrorType::SerializationError
}
