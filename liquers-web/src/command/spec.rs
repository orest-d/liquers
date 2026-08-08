//! Parsing a JavaScript command declaration into `CommandMetadata`.
//!
//! The declaration is an object literal:
//!
//! ```javascript
//! liquers.registerCommand({
//!   name: "repeat",                                        // required
//!   run: (text, count) => text.repeat(count),               // required
//!   arguments: [{ name: "count", type: "int", default: 2 }],// optional — inferred if absent
//!   state: "text",                                          // optional
//!   namespace: "", realm: "", doc: "", label: "",           // optional
//!   volatile: false, async: false,                          // optional
//! });
//! ```
//!
//! Only `name` and `run` are required; everything else has a meaningful default, so the minimal
//! declaration stays one line (`COMMAND09`).

use liquers_core::command_metadata::{
    ArgumentInfo, ArgumentType, CommandKey, CommandMetadata, CommandParameterValue,
};
use liquers_core::error::{Error, ErrorType};
use wasm_bindgen::prelude::*;

/// The namespace reserved for platform-dependent commands provided by `liquers-web` itself
/// (`alert`, later DOM access). Registration into it from JavaScript is refused, so that later
/// additions do not have to contend with user code for the name.
pub const RESERVED_NAMESPACE: &str = "web";

/// How the input `State` is presented to the JavaScript callable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateMode {
    /// No state argument — a *source* command, which produces a value rather than transforming one.
    None,
    /// The converted value.
    Value,
    /// The state's text form.
    Text,
    /// The state itself, giving access to metadata.
    State,
}

impl StateMode {
    fn parse(name: &str) -> Result<StateMode, Error> {
        match name {
            "none" => Ok(StateMode::None),
            "value" => Ok(StateMode::Value),
            "text" | "string" => Ok(StateMode::Text),
            "state" => Ok(StateMode::State),
            other => Err(Error::from_error(
                ErrorType::ParameterError,
                format!(
                    "Unknown state mode {other:?}; expected \"none\", \"value\", \"text\" or \"state\""
                ),
            )),
        }
    }

    /// Whether the callable receives a leading state argument.
    pub fn takes_state(self) -> bool {
        match self {
            StateMode::None => false,
            StateMode::Value | StateMode::Text | StateMode::State => true,
        }
    }
}

/// Whether the callable's result is awaited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsAsync {
    /// Declared async; the result is awaited.
    Async,
    /// Declared sync; a returned Promise is an error rather than a silently un-awaited value.
    Sync,
    /// Not declared — decided per call by testing whether the result is thenable. This is the
    /// default because a plain function may still return a Promise.
    Auto,
}

/// A parsed JavaScript command declaration.
pub struct JsCommandSpec {
    pub key: CommandKey,
    pub metadata: CommandMetadata,
    pub state_mode: StateMode,
    pub is_async: IsAsync,
    pub run: js_sys::Function,
    /// Whether the argument list was inferred rather than declared. Reported by
    /// `describeCommand` so inference is never invisible (`COMMAND05`).
    pub arguments_inferred: bool,
}

fn get(obj: &JsValue, key: &str) -> Option<JsValue> {
    js_sys::Reflect::get(obj, &JsValue::from_str(key))
        .ok()
        .filter(|v| !v.is_undefined() && !v.is_null())
}

fn get_string(obj: &JsValue, key: &str) -> Option<String> {
    get(obj, key).and_then(|v| v.as_string())
}

fn get_bool(obj: &JsValue, key: &str) -> Option<bool> {
    get(obj, key).and_then(|v| v.as_bool())
}

impl JsCommandSpec {
    /// Parses a declaration object.
    pub fn parse(spec: &JsValue) -> Result<JsCommandSpec, Error> {
        if !spec.is_object() {
            return Err(Error::from_error(
                ErrorType::ParameterError,
                "registerCommand expects an object, for example { name, run }".to_string(),
            ));
        }

        let name = get_string(spec, "name").ok_or_else(|| {
            Error::from_error(
                ErrorType::ParameterError,
                "A command declaration must have a string `name`".to_string(),
            )
        })?;
        if name.is_empty() {
            return Err(Error::from_error(
                ErrorType::ParameterError,
                "A command `name` must not be empty".to_string(),
            ));
        }

        let run = get(spec, "run")
            .filter(|v| v.is_function())
            .ok_or_else(|| {
                Error::from_error(
                    ErrorType::ParameterError,
                    format!("Command {name:?} must have a `run` function"),
                )
            })?;
        let run: js_sys::Function = run.unchecked_into();

        let namespace = get_string(spec, "namespace").unwrap_or_default();
        if namespace == RESERVED_NAMESPACE {
            return Err(Error::from_error(
                ErrorType::ParameterError,
                format!(
                    "The {RESERVED_NAMESPACE:?} namespace is reserved for platform commands \
                     provided by liquers-web and cannot be registered into from JavaScript"
                ),
            ));
        }
        let realm = get_string(spec, "realm").unwrap_or_default();

        let state_mode = match get_string(spec, "state") {
            Some(s) => StateMode::parse(&s)?,
            // Default: a command with no declared state is a source command. Inference below can
            // still not change this — the state mode is never guessed from the function.
            None => StateMode::None,
        };

        let is_async = match get_bool(spec, "async") {
            Some(true) => IsAsync::Async,
            Some(false) => IsAsync::Sync,
            None => IsAsync::Auto,
        };

        let key = CommandKey::new(&realm, &namespace, &name);
        let mut metadata = CommandMetadata::from_key(key.clone());
        metadata.label = get_string(spec, "label").unwrap_or_else(|| name.clone());
        metadata.doc = get_string(spec, "doc").unwrap_or_default();
        metadata.module = "javascript".to_string();
        // Declared and previously **ignored**, which left every JavaScript command at the
        // `CommandMetadata` default of `false`: a command explicitly opting out of caching was
        // treated as cacheable, and the planner could reuse a stale result. Mirrors what
        // `register_command!` does for a Rust command — it sets `volatile` and leaves `cache`
        // alone.
        metadata.volatile = get_bool(spec, "volatile").unwrap_or(false);

        let (arguments, arguments_inferred) = match get(spec, "arguments") {
            Some(declared) => (parse_arguments(&declared, &name)?, false),
            None => (infer_arguments(&run, state_mode, &name)?, true),
        };
        for a in arguments {
            metadata.arguments.push(a);
        }

        Ok(JsCommandSpec {
            key,
            metadata,
            state_mode,
            is_async,
            run,
            arguments_inferred,
        })
    }
}

/// Parses an explicit `arguments` array. This is the reliable path and always wins.
fn parse_arguments(declared: &JsValue, command: &str) -> Result<Vec<ArgumentInfo>, Error> {
    if !js_sys::Array::is_array(declared) {
        return Err(Error::from_error(
            ErrorType::ParameterError,
            format!("Command {command:?}: `arguments` must be an array"),
        ));
    }
    let array = js_sys::Array::from(declared);
    let mut out = Vec::with_capacity(array.length() as usize);
    for (i, item) in array.iter().enumerate() {
        let name = get_string(&item, "name").ok_or_else(|| {
            Error::from_error(
                ErrorType::ParameterError,
                format!("Command {command:?}: argument {i} must have a string `name`"),
            )
        })?;
        let mut info = ArgumentInfo::any_argument(&name);
        if let Some(t) = get_string(&item, "type") {
            info.argument_type = parse_argument_type(&t, command, &name)?;
        }
        if let Some(default) = get(&item, "default") {
            // Recorded as a JSON default so the planner can resolve it without re-entering
            // JavaScript. A default that is not JSON-representable is refused rather than
            // silently dropped.
            let json = js_default_to_json(&default).ok_or_else(|| {
                Error::from_error(
                    ErrorType::ParameterError,
                    format!(
                        "Command {command:?}, argument {name:?}: the default value must be a \
                         string, number, boolean or null"
                    ),
                )
            })?;
            info.default = CommandParameterValue::Value(json);
        }
        out.push(info);
    }
    Ok(out)
}

fn parse_argument_type(name: &str, command: &str, arg: &str) -> Result<ArgumentType, Error> {
    match name {
        "string" | "str" | "text" => Ok(ArgumentType::String),
        "int" | "integer" => Ok(ArgumentType::Integer),
        "float" | "number" => Ok(ArgumentType::Float),
        "bool" | "boolean" => Ok(ArgumentType::Boolean),
        "any" => Ok(ArgumentType::Any),
        other => Err(Error::from_error(
            ErrorType::ParameterError,
            format!(
                "Command {command:?}, argument {arg:?}: unknown type {other:?}; expected \
                 \"string\", \"int\", \"float\", \"bool\" or \"any\""
            ),
        )),
    }
}

fn js_default_to_json(v: &JsValue) -> Option<serde_json::Value> {
    if v.is_null() {
        return Some(serde_json::Value::Null);
    }
    if let Some(b) = v.as_bool() {
        return Some(serde_json::Value::Bool(b));
    }
    if let Some(s) = v.as_string() {
        return Some(serde_json::Value::String(s));
    }
    if let Some(n) = v.as_f64() {
        if n.fract() == 0.0 && n.abs() <= 9_007_199_254_740_992.0 {
            return Some(serde_json::Value::Number((n as i64).into()));
        }
        return serde_json::Number::from_f64(n).map(serde_json::Value::Number);
    }
    None
}

/// Infers argument names from the function, over the subset where the parse is provably exact.
///
/// See `specs/liquers-web/phase2-architecture.md`. The rule: every parameter must be a plain
/// identifier, and the token count must equal `Function.length`. Anything else — a default, a rest
/// parameter, destructuring, a bound or native function — is **refused** with a specific error
/// rather than mangled into metadata, because the regex silently produces garbage for those.
///
/// The one case that cannot be detected is minification, which yields correct arity and wrong
/// names. Since Liquers binds arguments positionally, that degrades labels rather than behaviour.
fn infer_arguments(
    run: &js_sys::Function,
    state_mode: StateMode,
    command: &str,
) -> Result<Vec<ArgumentInfo>, Error> {
    let source = String::from(run.to_string());
    let params = parameter_list(&source).ok_or_else(|| {
        Error::from_error(
            ErrorType::ParameterError,
            format!(
                "Command {command:?}: could not read the parameter list of `run` (a bound or \
                 native function?). Declare `arguments` explicitly."
            ),
        )
    })?;

    let trimmed = params.trim();
    let tokens: Vec<&str> = if trimmed.is_empty() {
        Vec::new()
    } else {
        trimmed.split(',').map(|t| t.trim()).collect()
    };

    for token in &tokens {
        if !is_plain_identifier(token) {
            return Err(Error::from_error(
                ErrorType::ParameterError,
                format!(
                    "Command {command:?}: cannot infer arguments because the parameter {token:?} \
                     is not a plain identifier (a default, rest or destructured parameter). \
                     Declare `arguments` explicitly."
                ),
            ));
        }
    }

    // `Function.length` counts parameters before the first default or rest parameter. Since those
    // are already refused above, an exact match is expected — a mismatch means the parse disagrees
    // with the one reliable signal, so refuse rather than trust it.
    let declared_len = run.length() as usize;
    if tokens.len() != declared_len {
        return Err(Error::from_error(
            ErrorType::ParameterError,
            format!(
                "Command {command:?}: inferred {} parameter(s) but Function.length is \
                 {declared_len}. Declare `arguments` explicitly.",
                tokens.len()
            ),
        ));
    }

    let skip = if state_mode.takes_state() { 1 } else { 0 };
    if tokens.len() < skip {
        return Err(Error::from_error(
            ErrorType::ParameterError,
            format!(
                "Command {command:?}: declares a state argument but `run` takes no parameters"
            ),
        ));
    }

    Ok(tokens[skip..]
        .iter()
        .map(|name| ArgumentInfo::any_argument(name))
        .collect())
}

/// Extracts the text between the first `(` and its matching `)`, with comments stripped.
fn parameter_list(source: &str) -> Option<String> {
    let stripped = strip_comments(source);
    let open = stripped.find('(')?;
    let close = stripped[open..].find(')')? + open;
    Some(stripped[open + 1..close].to_string())
}

/// Removes `//` and `/* */` comments, which are legal inside a parameter list.
fn strip_comments(source: &str) -> String {
    let bytes: Vec<char> = source.chars().collect();
    let mut out = String::with_capacity(source.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == '/' && i + 1 < bytes.len() && bytes[i + 1] == '/' {
            while i < bytes.len() && bytes[i] != '\n' {
                i += 1;
            }
        } else if bytes[i] == '/' && i + 1 < bytes.len() && bytes[i + 1] == '*' {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == '*' && bytes[i + 1] == '/') {
                i += 1;
            }
            i += 2;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    out
}

/// Whether a token is a plain JavaScript identifier — the subset over which the regex parse is
/// exact.
fn is_plain_identifier(token: &str) -> bool {
    let mut chars = token.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' || c == '$' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}

use wasm_bindgen::JsCast;
