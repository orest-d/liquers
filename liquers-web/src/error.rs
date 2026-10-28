//! The `ERROR` bridge: Liquers `Error` ↔ JavaScript.
//!
//! One structured error class rather than a typed hierarchy. `ErrorType` has 27 variants, and 27
//! JavaScript subclasses would be surface without benefit — the discriminant is carried as a
//! string field instead, which is also what makes it forward-compatible.

use liquers_core::error::{Error, ErrorCategory, ErrorType};
use liquers_core::issue_report::{Issue, IssueReport};
use liquers_core::query::Position;
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
        ErrorType::AssetNotFound => "asset_not_found",
        ErrorType::AssetNotSupported => "asset_not_supported",
        ErrorType::KeyNotSupported => "key_not_supported",
        ErrorType::KeyNotAbsolute => "key_not_absolute",
        ErrorType::KeyReadError => "key_read_error",
        ErrorType::KeyWriteError => "key_write_error",
        ErrorType::UnexpectedError => "unexpected_error",
        ErrorType::ExecutionError => "execution_error",
        ErrorType::DependencyVersionMismatch => "dependency_version_mismatch",
        ErrorType::DependencyCycle => "dependency_cycle",
        ErrorType::StatusConflict => "status_conflict",
        ErrorType::Cancelled => "cancelled",
        ErrorType::ConfigurationError => "configuration_error",
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
        "asset_not_found" => ErrorType::AssetNotFound,
        "asset_not_supported" => ErrorType::AssetNotSupported,
        "key_not_supported" => ErrorType::KeyNotSupported,
        "key_not_absolute" => ErrorType::KeyNotAbsolute,
        "key_read_error" => ErrorType::KeyReadError,
        "key_write_error" => ErrorType::KeyWriteError,
        "unexpected_error" => ErrorType::UnexpectedError,
        "execution_error" => ErrorType::ExecutionError,
        "dependency_version_mismatch" => ErrorType::DependencyVersionMismatch,
        "dependency_cycle" => ErrorType::DependencyCycle,
        "status_conflict" => ErrorType::StatusConflict,
        "cancelled" => ErrorType::Cancelled,
        "configuration_error" => ErrorType::ConfigurationError,
        _ => return None,
    };
    Some(t)
}

/// Stable string names for every [`ErrorCategory`], as `LiquersError.categories` reports them.
pub fn error_category_name(c: ErrorCategory) -> &'static str {
    match c {
        ErrorCategory::Store => "store",
        ErrorCategory::KeyedAsset => "keyed_asset",
        ErrorCategory::NotFound => "not_found",
    }
}

/// A Liquers error, visible to JavaScript.
#[wasm_bindgen(js_name = LiquersError)]
pub struct LiquersError {
    inner: Error,
    js_class: Option<String>,
    js_stack: Option<String>,
    report: Option<JsValue>,
}

#[wasm_bindgen(js_class = LiquersError)]
impl LiquersError {
    /// `new LiquersError(errorType, message, query?)`: a typed error a page can throw back at
    /// Liquers, for example from a `JsStore` delegate. `errorType` is a name as `errorType`
    /// reports it (`"key_not_found"`); an unknown name is a `TypeError`, never a silent `general`.
    /// A key is given as a key query (`"-R/data/a.txt"`) in `query`; a query that does not parse is
    /// a `TypeError`. `key`, `jsClass` and `jsStack` stay null: the page did not throw a JavaScript
    /// exception, and `key` is filled only by Liquers.
    #[wasm_bindgen(constructor)]
    pub fn construct(
        #[wasm_bindgen(js_name = errorType)] error_type: &str,
        message: String,
        query: Option<String>,
    ) -> Result<LiquersError, JsValue> {
        let Some(error_type) = error_type_from_name(error_type) else {
            return Err(js_sys::TypeError::new(&format!(
                "LiquersError: unknown error type '{error_type}'"
            ))
            .into());
        };
        let mut error = Error::from_error(error_type, message);
        if let Some(text) = query {
            let parsed = liquers_core::parse::parse_query(&text).map_err(|e| {
                JsValue::from(js_sys::TypeError::new(&format!(
                    "LiquersError: invalid query '{text}': {}",
                    e.message
                )))
            })?;
            error = error.with_query(&parsed);
        }
        Ok(LiquersError::new(error))
    }

    /// The error type, as a stable string such as `"conversion_error"`.
    #[wasm_bindgen(getter, js_name = errorType)]
    pub fn error_type(&self) -> String {
        error_type_name(self.inner.error_type).to_string()
    }

    /// The categories of the error type, such as `["store", "not_found"]` for `"key_not_found"`;
    /// empty for a type in none. JavaScript has no typed `catch`, so this is how a page handles a
    /// whole group at once: `e.categories.includes("not_found")`.
    #[wasm_bindgen(getter)]
    pub fn categories(&self) -> Vec<String> {
        self.inner
            .error_type
            .categories()
            .iter()
            .map(|c| error_category_name(*c).to_string())
            .collect()
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

    /// The complete validation report behind a refused `registerCommands` or `registerCommand`,
    /// as `validateCommands` returns it; `undefined` on every other error. The message is its
    /// `summary`.
    #[wasm_bindgen(getter, unchecked_return_type = "LiquersIssueReport | undefined")]
    pub fn report(&self) -> JsValue {
        self.report.clone().unwrap_or(JsValue::UNDEFINED)
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
            report: None,
        }
    }

    /// Attaches the validation report a refused command batch carries.
    pub fn with_report(mut self, report: &IssueReport) -> Self {
        self.report = Some(issue_report_to_js(report));
        self
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
            report: None,
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
    let mut error = Error::from_error(error_type, message);
    // A field is copied only when it has the type its getter returns: look-alike objects are
    // accepted, and an ill-typed field on one is dropped rather than coerced or refused.
    let string_field = |name: &str| {
        js_sys::Reflect::get(thrown, &JsValue::from_str(name))
            .ok()
            .and_then(|v| v.as_string())
    };
    error.query = string_field("query");
    error.key = string_field("key");
    if let Some(position) = js_sys::Reflect::get(thrown, &JsValue::from_str("position"))
        .ok()
        .and_then(|p| position_from_js(&p))
    {
        error.position = position;
    }
    Some(error)
}

/// Reads a `{offset, line, column}` object, as the `position` getter returns it.
///
/// `None` unless `value` is an object whose three fields are finite, non-negative numbers, and
/// for a position that is unknown, so the caller keeps its default.
fn position_from_js(value: &JsValue) -> Option<Position> {
    if !value.is_object() {
        return None;
    }
    let number = |name: &str| {
        js_sys::Reflect::get(value, &JsValue::from_str(name))
            .ok()?
            .as_f64()
            .filter(|n| n.is_finite() && *n >= 0.0)
    };
    let position = Position::new(
        number("offset")? as usize,
        number("line")? as u32,
        number("column")? as usize,
    );
    if position.is_unknown() {
        return None;
    }
    Some(position)
}

/// A validation report as JavaScript sees it (`LiquersIssueReport` in the `.d.ts`).
///
/// `{ issues, errorCount, warningCount, hasErrors, summary }`, each issue
/// `{ severity, message, command }`. A flat, declared shape rather than serde's form of
/// [`Issue`], which would put a Rust enum name in the page's API and change it whenever core adds
/// a variant. `severity` is [`liquers_core::issue_report::IssueSeverity`]'s lowercase name;
/// `command` is `{ realm, namespace, name }`, or `null` when no command is known. `summary` is the
/// message a refusal throws, or `null` when there is no error.
pub fn issue_report_to_js(report: &IssueReport) -> JsValue {
    // Built directly rather than through `serialize_to_js`: serde-wasm-bindgen writes a JSON
    // `null` as `undefined`, and the declared shape promises `null`.
    let set = |target: &js_sys::Object, key: &str, value: &JsValue| {
        let _ = js_sys::Reflect::set(target, &JsValue::from_str(key), value);
    };
    let issues = js_sys::Array::new();
    for issue in report.issues() {
        let entry = js_sys::Object::new();
        set(
            &entry,
            "severity",
            &JsValue::from_str(&issue.severity().to_string()),
        );
        set(&entry, "message", &JsValue::from_str(issue.message()));
        let command = match issue {
            Issue::Generic { .. } => JsValue::NULL,
            Issue::CommandRegistry {
                realm,
                namespace,
                name,
                ..
            } => {
                let command = js_sys::Object::new();
                set(&command, "realm", &JsValue::from_str(realm));
                set(&command, "namespace", &JsValue::from_str(namespace));
                set(&command, "name", &JsValue::from_str(name));
                command.into()
            }
        };
        set(&entry, "command", &command);
        issues.push(&entry);
    }
    let document = js_sys::Object::new();
    set(&document, "issues", &issues);
    set(
        &document,
        "errorCount",
        &JsValue::from_f64(report.error_count() as f64),
    );
    set(
        &document,
        "warningCount",
        &JsValue::from_f64(report.warning_count() as f64),
    );
    set(
        &document,
        "hasErrors",
        &JsValue::from_bool(report.has_errors()),
    );
    let summary = report
        .short_summary("Command metadata")
        .map(|summary| JsValue::from_str(&summary))
        .unwrap_or(JsValue::NULL);
    set(&document, "summary", &summary);
    document.into()
}
