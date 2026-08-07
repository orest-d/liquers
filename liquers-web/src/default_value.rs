//! [`JsValueBridge`] for the default value type.
//!
//! This is the **only** module that names a concrete value type. Everything in
//! [`crate::bridge`] stays generic so a downstream crate can substitute its own; this module is
//! what makes `liquers-web`'s own exported surface concrete.

use liquers_core::error::{Error, ErrorType};
use liquers_lib::value::{ExtValue, Value};
use std::sync::Arc;
use wasm_bindgen::JsValue;

use crate::bridge::JsValueBridge;
use crate::value::JsOpaque;

impl JsValueBridge for Value {
    fn from_js_custom(_js: &JsValue) -> Result<Option<Self>, Error> {
        // The default value type has no JavaScript-specific structural cases beyond the standard
        // mapping, so it always falls through.
        Ok(None)
    }

    fn to_js_custom(&self) -> Result<Option<JsValue>, Error> {
        Ok(None)
    }

    fn from_js_opaque(_js: JsValue, opaque: JsOpaque) -> Result<Self, Error> {
        Ok(Value::Extended(ExtValue::Foreign {
            value: Arc::new(opaque),
        }))
    }

    fn as_js_opaque(&self) -> Result<Option<&JsOpaque>, Error> {
        match self {
            Value::Extended(ExtValue::Foreign { value }) => {
                match value.as_any().downcast_ref::<JsOpaque>() {
                    Some(js) => Ok(Some(js)),
                    // The value *is* opaque, but belongs to another language runtime. Naming the
                    // origin is what turns a cross-language mistake into a diagnosable one.
                    None => Err(Error::conversion_error(
                        value.origin(),
                        "a JavaScript value (it belongs to a different language runtime)",
                    )),
                }
            }
            // Not an opaque value at all — not an error, just nothing to recover.
            Value::Base(_) => Ok(None),
            Value::Extended(ExtValue::Image { .. }) => Ok(None),
            #[cfg(feature = "polars")]
            Value::Extended(ExtValue::PolarsDataFrame { .. }) => Ok(None),
            #[cfg(feature = "egui")]
            Value::Extended(ExtValue::UiCommand { .. }) => Ok(None),
            #[cfg(feature = "egui")]
            Value::Extended(ExtValue::Widget { .. }) => Ok(None),
            Value::Extended(ExtValue::UIElement { .. }) => Ok(None),
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
