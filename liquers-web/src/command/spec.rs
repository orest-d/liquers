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
//! # Main components
//!
//! - [`JsCommandSpec`] — a parsed declaration: key, metadata, state mode, and the `run` function
//!   (`None` for a pipeline command); [`JsCommandSpec::check`] reports everything that refuses one.
//! - [`StateMode`] and [`IsAsync`] — how `run` receives its state and whether its result is awaited.
//! - [`RESERVED_NAMESPACE`] — `web`, refused to JavaScript.
//!
//! # Where it is used
//!
//! `crate::command::import` reads batches through it; [`crate::command::register_js_command`]
//! registers what it accepts; a rebuild replays declarations through [`JsCommandSpec::parse`].
//!
//! # Related concepts
//!
//! `liquers_core::command_declaration::CommandDeclaration` runs stages 2–5 of the shared
//! declaration pipeline (`specs/reference/COMMAND_DECLARATION.md`); this module is stage 1, the
//! JavaScript-specific part. `crate::pipeline` reads a declaration's `pipeline`.
//!
//! # Examples
//!
//! The declarations above, and every test in `tests/commands_DECLARATION.rs` (Node). The crate
//! does not build natively, so there is no doctest:
//!
//! ```text
//! liquers.registerCommand({
//!   name: "repeat", state: "text", run: (t, n) => t.repeat(n),
//!   arguments: [{ name: "n", type: "int", default: 2 }],
//! });
//! ```
//!
//! Only `name` and `run` are required; everything else has a meaningful default, so the minimal
//! declaration stays one line (`COMMAND09`).
//!
//! A **pipeline command** (`PIPELINE04`) has a `pipeline` instead of `run`: a pipeline in its JSON
//! form (`specs/reference/PIPELINES.md`), whose placeholders are bound to the declared
//! `arguments`. It has no function, so `run`, `state` and `async` are refused beside it, and its
//! arguments are never inferred:
//!
//! ```javascript
//! liquers.registerCommand({
//!   name: "greet_twice",
//!   pipeline: [{ action: "greet", parameters: [{ placeholder: "who" }] }, { action: "twice" }],
//!   arguments: [{ name: "who", default: "world" }],
//! });
//! ```
//!
//! A **template command** (`TEMPLATE02`) has a `template` instead: the link (a query string or a
//! `liquers.Query`) of the template it renders. It is the alias of `template/render` of
//! `specs/guides/TEMPLATE_GUIDE.md` §3 with the link as head parameter, so it has no function and
//! its arguments are `render`'s (`data`, `errors`, `cwd`); `run`, `state`, `async`, `pipeline` and
//! `arguments` are refused beside it:
//!
//! ```javascript
//! liquers.registerCommand({ name: "greeting", template: "-R/templates/greet.md" });
//! ```

use liquers_core::command_declaration::CommandDeclaration;
use liquers_core::command_metadata::{ArgumentInfo, CommandKey, CommandMetadata};
use liquers_core::error::{Error, ErrorType};
use liquers_core::issue_report::{Issue, IssueReport, IssueSeverity};
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
    /// The implementation; `None` for a pipeline or template command, whose behaviour is
    /// `metadata.definition`.
    pub run: Option<js_sys::Function>,
    /// Whether the argument list was inferred rather than declared. Reported by
    /// `describeCommand` so inference is never invisible (`COMMAND05`).
    pub arguments_inferred: bool,
    /// Where `run` receives the execution context: an index into the parameters it is called
    /// with, the state (when the command takes one) included. `None` when it takes no context.
    pub context_position: Option<usize>,
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
        // `pipeline` and `template` are read by `check_inner` itself; the shared declaration
        // format has no field for either.
        if matches!(
            key.as_string().as_deref(),
            Some("run") | Some("pipeline") | Some("template")
        ) {
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

/// An error entry about the command `name` (with its `realm` and `namespace`, when known).
fn command_issue(realm: &str, namespace: &str, name: &str, message: String) -> Issue {
    Issue::command_registry(IssueSeverity::Error, realm, namespace, name, message)
}

/// A report holding one error.
fn one_issue(issue: Issue) -> IssueReport {
    let mut report = IssueReport::default();
    report.extend([issue]);
    report
}

impl JsCommandSpec {
    /// Whether this is a definition command (a pipeline or template command): registered as
    /// metadata only, with no function.
    pub fn is_definition(&self) -> bool {
        self.run.is_none()
    }

    /// Parses a declaration object, refusing it with one compact error.
    ///
    /// The error's message is [`Self::check`]'s report summarised by
    /// [`IssueReport::short_summary`].
    pub fn parse(spec: &JsValue) -> Result<JsCommandSpec, Error> {
        match Self::check(spec) {
            (Some(parsed), _) => Ok(parsed),
            (None, report) => Err(Error::from_error(
                ErrorType::ParameterError,
                report
                    .short_summary("Command metadata")
                    .unwrap_or_else(|| "The command declaration was refused".to_string()),
            )),
        }
    }

    /// Parses a declaration object, or reports everything that refuses it.
    ///
    /// Stage 1 of the declaration pipeline (`specs/reference/COMMAND_DECLARATION.md`) is
    /// JavaScript-specific and stays here — resolving `run`, and inferring arguments from the
    /// function source. Stages 2-5 are `liquers-core`'s, and their findings arrive as
    /// `CommandDeclaration::check`'s report. A JavaScript-specific refusal (not an object, no
    /// `name`, no `run`, the reserved namespace, an argument list that cannot be inferred) is one
    /// error entry; it names its command when the name is known.
    ///
    /// The report is returned only for a refused declaration. An accepted one is checked again,
    /// with the rest of its batch, by `CommandMetadataRegistry::check`, so its warnings would
    /// otherwise be reported twice.
    pub fn check(spec: &JsValue) -> (Option<JsCommandSpec>, IssueReport) {
        match Self::check_inner(spec) {
            Ok(parsed) => (Some(parsed), IssueReport::default()),
            Err(report) => (None, report),
        }
    }

    fn check_inner(spec: &JsValue) -> Result<JsCommandSpec, IssueReport> {
        if !spec.is_object() {
            return Err(one_issue(Issue::generic(
                IssueSeverity::Error,
                "registerCommand expects an object, for example { name, run }",
            )));
        }

        // `name` is checked before serde sees the document, so these two messages survive
        // verbatim rather than becoming serde's "missing field `name`".
        let name = reflect_get(spec, "name")
            .and_then(|v| v.as_string())
            .ok_or_else(|| {
                one_issue(Issue::generic(
                    IssueSeverity::Error,
                    "A command declaration must have a string `name`",
                ))
            })?;
        if name.is_empty() {
            return Err(one_issue(Issue::generic(
                IssueSeverity::Error,
                "A command `name` must not be empty",
            )));
        }
        // Read for the report's command identity only; the document below is authoritative.
        let text_field = |key: &str| {
            reflect_get(spec, key)
                .and_then(|v| v.as_string())
                .unwrap_or_default()
        };
        let (realm, namespace) = (text_field("realm"), text_field("namespace"));
        let refuse = |message: String| one_issue(command_issue(&realm, &namespace, &name, message));

        let template = match reflect_get(spec, "template") {
            Some(link) => {
                for field in ["run", "state", "async", "pipeline", "arguments"] {
                    if reflect_get(spec, field).is_some() {
                        return Err(refuse(format!(
                            "Command {name:?} has a `template`, so it takes no `{field}`: a \
                             template command renders its template, with `render`'s arguments"
                        )));
                    }
                }
                Some(
                    template_link(&link)
                        .map_err(|e| refuse(format!("Command {name:?}: {}", e.message)))?,
                )
            }
            None => None,
        };
        let pipeline = match reflect_get(spec, "pipeline") {
            Some(body) => {
                for field in ["run", "state", "async"] {
                    if reflect_get(spec, field).is_some() {
                        return Err(refuse(format!(
                            "Command {name:?} has a `pipeline`, so it takes no `{field}`: a \
                             pipeline command has no function"
                        )));
                    }
                }
                Some(
                    crate::pipeline::pipeline_from_js(&body)
                        .map_err(|e| refuse(format!("Command {name:?}: {}", e.message)))?,
                )
            }
            None => None,
        };
        let run: Option<js_sys::Function> = match (&pipeline, &template) {
            (Some(_), _) | (_, Some(_)) => None,
            (None, None) => Some(
                reflect_get(spec, "run")
                    .filter(|v| v.is_function())
                    .ok_or_else(|| {
                        refuse(format!(
                            "Command {name:?} must have a `run` function, a `pipeline` or a \
                             `template`"
                        ))
                    })?
                    .unchecked_into(),
            ),
        };

        let declaration_object = without_run(spec).map_err(|e| refuse(e.message.clone()))?;
        let mut document: serde_json::Value =
            serde_wasm_bindgen::from_value(declaration_object.into())
                .map_err(|e| refuse(format!("Command {name:?}: {e}")))?;

        let namespace = document
            .get("namespace")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if namespace == RESERVED_NAMESPACE {
            return Err(refuse(reserved_namespace_message()));
        }
        let realm = document
            .get("realm")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        // JavaScript declares its state mode explicitly rather than by naming the first argument,
        // so the core state-delivery convention must not run. The `context` one does, unless the
        // page turned it off (`conventions: { context: false }`).
        let state_mode = match document.get("state").and_then(|v| v.as_str()) {
            Some(s) => StateMode::parse(s).map_err(|e| refuse(e.message.clone()))?,
            // A command with no declared state is a source command. Inference below cannot change
            // this — the state mode is never guessed from the function.
            None => StateMode::None,
        };
        let is_async = match document.get("async").and_then(|v| v.as_bool()) {
            Some(true) => IsAsync::Async,
            Some(false) => IsAsync::Sync,
            None => IsAsync::Auto,
        };

        let arguments_inferred = run.is_some() && document.get("arguments").is_none();
        let context_convention = context_convention_enabled(&document);
        prepare_javascript_document(&mut document, &name, context_convention);

        let mut declaration = CommandDeclaration::from_document(document);
        declaration
            .apply_conventions()
            .map_err(|e| refuse(e.message.clone()))?;
        declaration.fill_defaults();
        let report = declaration.check();
        if report.has_errors() {
            return Err(report);
        }
        // `registration.context` counts the declared arguments only; `run` receives the state
        // first when it takes one.
        let state_offset = if state_mode.takes_state() { 1 } else { 0 };
        let mut context_position = declaration
            .registration()
            .get("context")
            .and_then(|v| v.as_u64())
            .map(|p| p as usize + state_offset);
        let mut metadata = declaration.build().map_err(|e| refuse(e.message.clone()))?;
        metadata.module = "javascript".to_string();

        if let Some(pipeline) = pipeline {
            // As `CommandRegistry::register_pipeline`: a body whose first element sets the state is
            // a source command. The context is a function's concern; a pipeline's steps get their
            // own.
            if pipeline.first_sets_state() {
                metadata.state_argument = None;
            }
            metadata.definition =
                liquers_core::command_metadata::CommandDefinition::Pipeline { pipeline };
            context_position = None;
        }
        if let Some(link) = template {
            // The fields `CommandRegistry::register_alias` copies from the target are completed
            // against the registry when the batch is checked and registered
            // (`adapter::complete_definition`).
            metadata.arguments = template_command_arguments();
            metadata.definition = liquers_core::command_metadata::CommandDefinition::Alias {
                command: template_render_key(),
                head_parameters: vec![
                    liquers_core::command_metadata::CommandParameterValue::Query(link),
                ],
            };
            context_position = None;
        }

        if let (true, Some(run)) = (arguments_inferred, run.as_ref()) {
            let inferred =
                infer_arguments(run, state_mode, &name).map_err(|e| refuse(e.message.clone()))?;
            for (position, argument) in inferred.into_iter().enumerate() {
                if context_convention && argument.name == "context" && context_position.is_none() {
                    // An inferred parameter's index already counts the state.
                    context_position = Some(position + state_offset);
                    continue;
                }
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
            context_position,
        })
    }

    /// Accepts imported metadata whose definition is a pipeline or an alias: it needs no
    /// implementation.
    pub fn from_definition_metadata(mut metadata: CommandMetadata) -> Result<JsCommandSpec, Issue> {
        if metadata.namespace == RESERVED_NAMESPACE {
            return Err(command_issue(
                &metadata.realm,
                &metadata.namespace,
                &metadata.name,
                reserved_namespace_message(),
            ));
        }
        metadata.module = "javascript".to_string();
        Ok(JsCommandSpec {
            key: metadata.key(),
            metadata,
            state_mode: StateMode::None,
            is_async: IsAsync::Auto,
            run: None,
            arguments_inferred: false,
            context_position: None,
        })
    }

    /// Binds imported command metadata to its JavaScript implementation.
    ///
    /// `implementation` is a function, or `{ run, state?, async? }` read with the declaration
    /// fields' names and defaults: no `state` is a source command, no `async` decides per call.
    /// The metadata is taken as it is — the caller validates it with the rest of its batch — apart
    /// from `module`, which is set to `"javascript"` as for a declaration.
    pub fn from_metadata(
        mut metadata: CommandMetadata,
        implementation: &JsValue,
    ) -> Result<JsCommandSpec, Issue> {
        let refuse = |message: String| {
            command_issue(
                &metadata.realm,
                &metadata.namespace,
                &metadata.name,
                message,
            )
        };
        if metadata.namespace == RESERVED_NAMESPACE {
            return Err(refuse(reserved_namespace_message()));
        }
        let (run, state, is_async) = if implementation.is_function() {
            (implementation.clone(), None, None)
        } else if implementation.is_object() {
            (
                reflect_get(implementation, "run").unwrap_or(JsValue::UNDEFINED),
                reflect_get(implementation, "state").and_then(|v| v.as_string()),
                reflect_get(implementation, "async").and_then(|v| v.as_bool()),
            )
        } else {
            (JsValue::UNDEFINED, None, None)
        };
        if !run.is_function() {
            return Err(refuse(format!(
                "Command {:?}: its implementation must be a function or {{ run, state?, async? }}",
                metadata.name
            )));
        }
        let state_mode = match state {
            Some(name) => StateMode::parse(&name).map_err(|e| refuse(e.message.clone()))?,
            None => StateMode::None,
        };
        let is_async = match is_async {
            Some(true) => IsAsync::Async,
            Some(false) => IsAsync::Sync,
            None => IsAsync::Auto,
        };
        metadata.module = "javascript".to_string();
        Ok(JsCommandSpec {
            key: metadata.key(),
            metadata,
            state_mode,
            is_async,
            run: Some(run.unchecked_into()),
            arguments_inferred: false,
            // Imported metadata carries no `registration` record, and its argument list is what
            // the planner binds, so the context is never inserted into an imported command.
            context_position: None,
        })
    }
}

/// The command every template command is an alias of.
pub fn template_render_key() -> CommandKey {
    CommandKey::new("", "template", "render")
}

/// `template/render`'s arguments after its head (`template`), as `TEMPLATE_GUIDE.md` §3 lists
/// them. `cwd` is `any`, because a `Key` argument is; `render` converts it.
fn template_command_arguments() -> Vec<ArgumentInfo> {
    let cwd = ArgumentInfo::any_argument("cwd");
    let cwd = match liquers_core::parse::parse_query("-R-key/.") {
        Ok(query) => cwd.with_default_link(query),
        // The literal parses; were it ever not to, `cwd` would default to the root.
        Err(_) => cwd,
    };
    vec![
        ArgumentInfo::any_argument("data").with_default(""),
        ArgumentInfo::string_argument("errors").with_default("strict"),
        cwd,
    ]
}

/// A template command's `template`: a query string, or a `liquers.Query`.
fn template_link(link: &JsValue) -> Result<liquers_core::query::Query, Error> {
    let text = if let Some(text) = link.as_string() {
        text
    } else if crate::objects::is_query_wrapper(link) {
        crate::objects::encode_query_wrapper(link)?
    } else {
        return Err(Error::from_error(
            ErrorType::ParameterError,
            "`template` must be the template's link: a query string or a liquers.Query".to_string(),
        ));
    };
    liquers_core::parse::parse_query(&text)
}

fn reserved_namespace_message() -> String {
    format!(
        "The {RESERVED_NAMESPACE:?} namespace is reserved for platform commands provided by \
         liquers-web and cannot be registered into from JavaScript"
    )
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
/// * **the state convention is off**, because JavaScript declares its state mode explicitly. The
///   `context` convention stays on unless the page wrote `conventions: { context: false }` (or
///   `conventions: false`).
///
/// See open question 2 of `specs/design/command-declaration/phase2-architecture.md`: unifying the
/// two label rules is defensible but is a deliberate behaviour change, not something to slip in.
fn prepare_javascript_document(document: &mut serde_json::Value, name: &str, context: bool) {
    let map = match document.as_object_mut() {
        Some(map) => map,
        None => return,
    };
    map.insert(
        "conventions".to_string(),
        serde_json::json!({ "state": false, "context": context }),
    );

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

/// Whether the page left the `context` convention on. Only an explicit `false` turns it off, as in
/// core's reading of `conventions`.
fn context_convention_enabled(document: &serde_json::Value) -> bool {
    match document.get("conventions") {
        Some(serde_json::Value::Bool(false)) => false,
        Some(serde_json::Value::Object(map)) => {
            !matches!(map.get("context"), Some(serde_json::Value::Bool(false)))
        }
        Some(_) | None => true,
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
