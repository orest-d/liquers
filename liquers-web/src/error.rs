//! The `ERROR` bridge: Liquers `Error` ↔ JavaScript.
//!
//! One structured error class rather than a typed hierarchy. `ErrorType` has 22 variants, and 22
//! JavaScript subclasses would be surface without benefit — the discriminant is carried as a
//! string field instead, which is also what makes it forward-compatible.

use liquers_core::error::{Error, ErrorType};
use wasm_bindgen::prelude::*;

/// Stable string names for every [`ErrorType`].
///
/// Names, never integer discriminants: an ordinal would silently change meaning if a variant were
/// inserted. The match is exhaustive with no `_ =>` arm, so a new variant is a compile error here
/// rather than a value that reaches JavaScript unnamed.
pub fn error_type_name(t: ErrorType) -> &'static str {
    match t {
        ErrorType::ArgumentMissing => "argument_missing",
        ErrorType::ActionNotRegistered => "action_not_registered",
        ErrorType::CommandAlreadyRegistered => "command_already_registered",
        ErrorType::ParseError => "parse_error",
        ErrorType::ParameterError => "parameter_error",
        ErrorType::TooManyParameters => "too_many_parameters",
        ErrorType::ConversionError => "conversion_error",
        ErrorType::SerializationError => "serialization_error",
        ErrorType::General => "general",
        ErrorType::CacheNotSupported => "cache_not_supported",
        ErrorType::UnknownCommand => "unknown_command",
        ErrorType::NotSupported => "not_supported",
        ErrorType::NotAvailable => "not_available",
        ErrorType::KeyNotFound => "key_not_found",
        ErrorType::KeyNotSupported => "key_not_supported",
        ErrorType::KeyReadError => "key_read_error",
        ErrorType::KeyWriteError => "key_write_error",
        ErrorType::UnexpectedError => "unexpected_error",
        ErrorType::ExecutionError => "execution_error",
        ErrorType::DependencyVersionMismatch => "dependency_version_mismatch",
        ErrorType::DependencyCycle => "dependency_cycle",
        ErrorType::Cancelled => "cancelled",
    }
}

/// Parses a name produced by [`error_type_name`].
///
/// An unrecognised name is `None` rather than a silent default — the forward-compatibility policy
/// is that a name from a newer Liquers is reported as unknown, never quietly mapped to `General`.
pub fn error_type_from_name(name: &str) -> Option<ErrorType> {
    let t = match name {
        "argument_missing" => ErrorType::ArgumentMissing,
        "action_not_registered" => ErrorType::ActionNotRegistered,
        "command_already_registered" => ErrorType::CommandAlreadyRegistered,
        "parse_error" => ErrorType::ParseError,
        "parameter_error" => ErrorType::ParameterError,
        "too_many_parameters" => ErrorType::TooManyParameters,
        "conversion_error" => ErrorType::ConversionError,
        "serialization_error" => ErrorType::SerializationError,
        "general" => ErrorType::General,
        "cache_not_supported" => ErrorType::CacheNotSupported,
        "unknown_command" => ErrorType::UnknownCommand,
        "not_supported" => ErrorType::NotSupported,
        "not_available" => ErrorType::NotAvailable,
        "key_not_found" => ErrorType::KeyNotFound,
        "key_not_supported" => ErrorType::KeyNotSupported,
        "key_read_error" => ErrorType::KeyReadError,
        "key_write_error" => ErrorType::KeyWriteError,
        "unexpected_error" => ErrorType::UnexpectedError,
        "execution_error" => ErrorType::ExecutionError,
        "dependency_version_mismatch" => ErrorType::DependencyVersionMismatch,
        "dependency_cycle" => ErrorType::DependencyCycle,
        "cancelled" => ErrorType::Cancelled,
        _ => return None,
    };
    Some(t)
}

/// A Liquers error, visible to JavaScript.
#[wasm_bindgen(js_name = LiquersError)]
pub struct LiquersError {
    inner: Error,
    js_class: Option<String>,
    js_stack: Option<String>,
}

#[wasm_bindgen(js_class = LiquersError)]
impl LiquersError {
    /// The error type, as a stable string such as `"conversion_error"`.
    #[wasm_bindgen(getter, js_name = errorType)]
    pub fn error_type(&self) -> String {
        error_type_name(self.inner.error_type).to_string()
    }

    #[wasm_bindgen(getter)]
    pub fn message(&self) -> String {
        self.inner.message.clone()
    }

    /// The query being evaluated when the error occurred, if any.
    #[wasm_bindgen(getter)]
    pub fn query(&self) -> Option<String> {
        self.inner.query.clone()
    }

    /// The store key involved, if any.
    #[wasm_bindgen(getter)]
    pub fn key(&self) -> Option<String> {
        self.inner.key.clone()
    }

    /// Source position as `{offset, line, column}`, or `null` when unknown.
    #[wasm_bindgen(getter)]
    pub fn position(&self) -> JsValue {
        let p = &self.inner.position;
        if p.is_unknown() {
            return JsValue::NULL;
        }
        let obj = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&obj, &"offset".into(), &JsValue::from_f64(p.offset as f64));
        let _ = js_sys::Reflect::set(&obj, &"line".into(), &JsValue::from_f64(p.line as f64));
        let _ = js_sys::Reflect::set(&obj, &"column".into(), &JsValue::from_f64(p.column as f64));
        obj.into()
    }

    /// The originating JavaScript error's class name, when this error came from a thrown value.
    #[wasm_bindgen(getter, js_name = jsClass)]
    pub fn js_class(&self) -> Option<String> {
        self.js_class.clone()
    }

    /// The originating JavaScript stack, when available.
    #[wasm_bindgen(getter, js_name = jsStack)]
    pub fn js_stack(&self) -> Option<String> {
        self.js_stack.clone()
    }

    #[wasm_bindgen(js_name = toString)]
    pub fn to_string_js(&self) -> String {
        format!("{}: {}", self.error_type(), self.inner.message)
    }
}

impl LiquersError {
    pub fn new(inner: Error) -> Self {
        LiquersError {
            inner,
            js_class: None,
            js_stack: None,
        }
    }

    /// Builds an error from a thrown JavaScript value, keeping the originating class and stack in
    /// the structured `jsClass`/`jsStack` fields as well as in the message.
    ///
    /// `ERROR03` requires the class and stack to survive the round trip; putting them only in the
    /// message text would make that test pass while leaving the fields permanently null, which is
    /// the kind of hollow conformance the guide warns about.
    pub fn from_thrown(thrown: JsValue, fallback: ErrorType) -> Self {
        let (class, stack) = js_error_context(&thrown);
        LiquersError {
            inner: js_error_to_liquers(thrown, fallback),
            js_class: class,
            js_stack: stack,
        }
    }

    pub fn inner(&self) -> &Error {
        &self.inner
    }
}

/// Extracts the class name and stack from a thrown JavaScript value, when it is an `Error`.
pub fn js_error_context(thrown: &JsValue) -> (Option<String>, Option<String>) {
    if !thrown.is_instance_of::<js_sys::Error>() {
        return (None, None);
    }
    let e: js_sys::Error = thrown.clone().unchecked_into();
    let stack = js_sys::Reflect::get(thrown, &JsValue::from_str("stack"))
        .ok()
        .and_then(|s| s.as_string());
    (e.name().as_string(), stack)
}

/// Converts a Liquers error into a value suitable for rejecting a `Promise` or throwing.
///
/// Always a structured `LiquersError`, never a bare string: `ASYNCQ02` requires that a rejection
/// carries the error's fields, and a string discards every one of them.
pub fn liquers_error_to_js(err: Error) -> JsValue {
    LiquersError::new(err).into()
}

/// Converts a thrown JavaScript value into a Liquers error.
///
/// `fallback` is the error type used when the throw carries no better information — normally
/// `ExecutionError` for a failure inside a command, and `ConversionError` at a conversion boundary.
/// The distinction matters: the guide requires that a conversion failure not be reported as an
/// execution failure.
///
/// Class name and stack are preserved when the thrown value is an `Error`. A non-`Error` throw
/// (`throw 42`, `throw undefined`) is coerced to a string rather than being lost.
pub fn js_error_to_liquers(thrown: JsValue, fallback: ErrorType) -> Error {
    // A LiquersError thrown back at us round-trips without degradation.
    if let Some(le) = js_value_as_liquers_error(&thrown) {
        return le;
    }

    let (message, class, stack) = if thrown.is_instance_of::<js_sys::Error>() {
        let e: js_sys::Error = thrown.clone().unchecked_into();
        (
            e.message().as_string().unwrap_or_default(),
            e.name().as_string(),
            // `js_sys::Error` exposes no `stack` accessor; it is a non-standard (though
            // universally implemented) property, so it has to be read reflectively.
            js_sys::Reflect::get(&thrown, &JsValue::from_str("stack"))
                .ok()
                .and_then(|s| s.as_string()),
        )
    } else if thrown.is_undefined() {
        // `throw undefined` has no string form worth reporting; say what happened instead.
        ("undefined was thrown".to_string(), None, None)
    } else {
        // Stringify whatever was thrown. `JsString::from(JsValue)` is an *unchecked cast*, not a
        // coercion, so it yields nothing useful for a number or a boolean — the scalar cases are
        // handled explicitly and anything else falls back to JsValue's own debug representation.
        let text = thrown
            .as_string()
            .or_else(|| thrown.as_f64().map(|n| n.to_string()))
            .or_else(|| thrown.as_bool().map(|b| b.to_string()))
            .unwrap_or_else(|| format!("{thrown:?}"));
        (format!("a non-Error value was thrown: {text}"), None, None)
    };

    let detail = match (&class, &stack) {
        (Some(c), _) => format!("{c}: {message}"),
        (None, _) => message,
    };
    let mut err = Error::from_error(fallback, detail);
    if let Some(s) = stack {
        // V8 and SpiderMonkey both begin `stack` with the "Name: message" line that `detail`
        // already is, so appending it verbatim printed the first line twice.
        let frames = s
            .strip_prefix(err.message.as_str())
            .unwrap_or(&s)
            .trim_start_matches('\n');
        if !frames.is_empty() {
            err.message = format!("{}\n{}", err.message, frames);
        }
    }
    err
}

/// Recovers a `LiquersError` that JavaScript threw back at us, so an error crossing the boundary
/// twice does not accumulate wrapping.
fn js_value_as_liquers_error(thrown: &JsValue) -> Option<Error> {
    let name = js_sys::Reflect::get(thrown, &JsValue::from_str("errorType"))
        .ok()?
        .as_string()?;
    let error_type = error_type_from_name(&name)?;
    let message = js_sys::Reflect::get(thrown, &JsValue::from_str("message"))
        .ok()
        .and_then(|m| m.as_string())
        .unwrap_or_default();
    Some(Error::from_error(error_type, message))
}
