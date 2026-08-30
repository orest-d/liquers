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

use liquers_core::command_declaration::CommandDeclaration;
use liquers_core::command_metadata::{ArgumentInfo, CommandKey, CommandMetadata};
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

fn reflect_get(obj: &JsValue, key: &str) -> Option<JsValue> {
    js_sys::Reflect::get(obj, &JsValue::from_str(key))
        .ok()
        .filter(|v| !v.is_undefined() && !v.is_null())
}

/// A copy of the declaration without `run`, and without any property explicitly set to `null` or
/// `undefined`.
///
/// Dropping the empty properties is not tidiness, it preserves behaviour. The hand-written parser
/// this replaces read every field through a helper that filtered both
/// (`fn get`, before this rewrite), so `{ name: "f", label: null }` meant "no label" and fell back
/// to the default. Handing that `null` to serde instead would fail — `null` is not a `String` — and
/// `arguments: null` would read as a declared-but-empty list rather than as "infer them".
fn without_run(spec: &JsValue) -> Result<js_sys::Object, Error> {
    let source = js_sys::Object::from(spec.clone());
    let copy = js_sys::Object::new();
    for key in js_sys::Object::keys(&source).iter() {
        if key.as_string().as_deref() == Some("run") {
            continue;
        }
        let value = js_sys::Reflect::get(&source, &key).map_err(|_| {
            Error::from_error(
                ErrorType::ParameterError,
                "a command declaration property could not be read".to_string(),
            )
        })?;
        if value.is_null() || value.is_undefined() {
            continue;
        }
        js_sys::Reflect::set(&copy, &key, &value).map_err(|_| {
            Error::from_error(
                ErrorType::ParameterError,
                "a command declaration property could not be copied".to_string(),
            )
        })?;
    }
    Ok(copy)
}

impl JsCommandSpec {
    /// Parses a declaration object.
    ///
    /// Stage 1 of the declaration pipeline (`specs/reference/COMMAND_DECLARATION.md`) is
    /// JavaScript-specific and stays here — resolving `run`, and inferring arguments from the
    /// function source. Stages 2-5 are `liquers-core`'s.
    pub fn parse(spec: &JsValue) -> Result<JsCommandSpec, Error> {
        if !spec.is_object() {
            return Err(Error::from_error(
                ErrorType::ParameterError,
                "registerCommand expects an object, for example { name, run }".to_string(),
            ));
        }

        // `name` is checked before serde sees the document, so these two messages survive
        // verbatim rather than becoming serde's "missing field `name`".
        let name = reflect_get(spec, "name")
            .and_then(|v| v.as_string())
            .ok_or_else(|| {
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

        let run = reflect_get(spec, "run")
            .filter(|v| v.is_function())
            .ok_or_else(|| {
                Error::from_error(
                    ErrorType::ParameterError,
                    format!("Command {name:?} must have a `run` function"),
                )
            })?;
        let run: js_sys::Function = run.unchecked_into();

        let declaration_object = without_run(spec)?;
        let mut document: serde_json::Value =
            serde_wasm_bindgen::from_value(declaration_object.into()).map_err(|e| {
                Error::from_error(ErrorType::ParameterError, format!("Command {name:?}: {e}"))
            })?;

        let namespace = document
            .get("namespace")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if namespace == RESERVED_NAMESPACE {
            return Err(Error::from_error(
                ErrorType::ParameterError,
                format!(
                    "The {RESERVED_NAMESPACE:?} namespace is reserved for platform commands \
                     provided by liquers-web and cannot be registered into from JavaScript"
                ),
            ));
        }
        let realm = document
            .get("realm")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        // JavaScript declares its state mode explicitly rather than by naming the first argument,
        // so the core state-delivery convention must not run — nor the `context` one, since a
        // JavaScript command cannot reach the context at all
        // (JS-COMMAND-CANNOT-ACCESS-CONTEXT).
        let state_mode = match document.get("state").and_then(|v| v.as_str()) {
            Some(s) => StateMode::parse(s)?,
            // A command with no declared state is a source command. Inference below cannot change
            // this — the state mode is never guessed from the function.
            None => StateMode::None,
        };
        let is_async = match document.get("async").and_then(|v| v.as_bool()) {
            Some(true) => IsAsync::Async,
            Some(false) => IsAsync::Sync,
            None => IsAsync::Auto,
        };

        let arguments_inferred = document.get("arguments").is_none();
        prepare_javascript_document(&mut document, &name);

        let mut metadata = CommandDeclaration::from_document(document)
            .finish()
            .map_err(|e| Error::from_error(ErrorType::ParameterError, format!("{e}")))?;
        metadata.module = "javascript".to_string();

        if arguments_inferred {
            for argument in infer_arguments(&run, state_mode, &name)? {
                metadata.arguments.push(argument);
            }
        }

        let key = CommandKey::new(&realm, &namespace, &name);
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

/// Fills in what JavaScript's own rules supply, before the shared pipeline derives anything.
///
/// Every value written here is one where `liquers-web`'s rule differs from the shared default, so
/// letting the shared one apply would change existing commands' `metadata_version` and re-expire
/// their dependent assets:
///
/// * **the command label** is the name **verbatim**, where the shared rule would derive
///   `Foo bar` from `foo_bar`;
/// * **an argument label** is `name.replace('_', " ")`, matching `ArgumentInfo::any_argument`,
///   where the shared rule would capitalise it;
/// * **`state_argument`** is always present, which is what `CommandMetadata::from_key` gave every
///   JavaScript command before and what the conformance suite's metadata assertions expect;
/// * **conventions are off**, because JavaScript declares its state mode explicitly.
///
/// See open question 2 of `specs/design/command-declaration/phase2-architecture.md`: unifying the
/// two label rules is defensible but is a deliberate behaviour change, not something to slip in.
fn prepare_javascript_document(document: &mut serde_json::Value, name: &str) {
    let map = match document.as_object_mut() {
        Some(map) => map,
        None => return,
    };
    map.insert("conventions".to_string(), serde_json::Value::Bool(false));

    let label_missing = map
        .get("label")
        .map(|v| v.as_str().unwrap_or("").is_empty())
        .unwrap_or(true);
    if label_missing {
        map.insert(
            "label".to_string(),
            serde_json::Value::from(name.to_string()),
        );
    }

    if map.get("state_argument").is_none() {
        if let Ok(state_argument) = serde_json::to_value(ArgumentInfo::any_argument("state")) {
            map.insert("state_argument".to_string(), state_argument);
        }
    }

    if let Some(serde_json::Value::Array(arguments)) = map.get_mut("arguments") {
        for argument in arguments.iter_mut() {
            let argument_name = argument
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let argument_map = match argument.as_object_mut() {
                Some(argument_map) => argument_map,
                None => continue,
            };
            let missing = argument_map
                .get("label")
                .map(|v| v.as_str().unwrap_or("").is_empty())
                .unwrap_or(true);
            if missing && !argument_name.is_empty() {
                argument_map.insert(
                    "label".to_string(),
                    serde_json::Value::from(argument_name.replace('_', " ")),
                );
            }
        }
    }
}

/// Infers argument names from the function, over the subset where the parse is provably exact.
///
/// See `specs/design/liquers-web/phase2-architecture.md`. The rule: every parameter must be a plain
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
            format!("Command {command:?}: declares a state argument but `run` takes no parameters"),
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
