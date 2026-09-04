#![allow(unused_imports)]
#![allow(dead_code)]

//! Query planning and the executable plan data model.
//!
//! [`Plan`] is an executable plan that the interpreter can execute.
//! A single step in the plan is a [`Step`] that contains an executable instruction.
//!
//! A [`Query`] describes a general syntax, it obtains a specific meaning by building a [`Plan`],
//! which can be executed by an interpreter.  [`PlanBuilder`] performs the synchronous part of planning against a
//! [`CommandMetadataRegistry`]: it resolves commands, converts action parameters, applies
//! defaults, identifies links, and derives initial volatility, payload, and expiration
//! requirements. It does not execute commands or produce an asset.
//!
//! Planning that depends on an environment remains asynchronous and lives in
//! `crate::interpreter`. In particular, `finalize_plan` incorporates dependency volatility and
//! expiration and registers dependency information before `apply_plan` executes the steps.
//! Consequently, the corresponding fields on a newly built plan are preliminary until
//! finalization has completed.
//!
//! Recipe arguments use [`Plan::override_value`] and [`Plan::override_link`]. Both deliberately
//! target only the last action in a plan; they are not general substitutions across all steps.
//!
//! [`Plan::init_steps`] are planning diagnostics copied into metadata. They are distinct from
//! diagnostic variants in [`Plan::steps`], which the interpreter encounters in execution order.

use std::clone;
use std::collections::HashSet;
use std::fmt::Display;
use std::ops::Index;

use itertools::Itertools;
use nom::Err;
use serde_json::Value;

use crate::command_metadata::{
    self, ArgumentInfo, ArgumentType, CommandKey, CommandMetadata, CommandMetadataRegistry,
    CommandParameterValue, EnumArgumentType, PayloadRequirement,
};
use crate::context::{EnvRef, Environment};
use crate::dependencies::{DependencyRelation, PlanDependency};
use crate::error::{Error, ErrorType};
use crate::expiration::Expires;
use crate::metadata::{DependencyKey, Metadata, MetadataRecord, Status};
use crate::parse::parse_key;
use crate::query::{
    ActionParameter, ActionRequest, CwdCursor, Key, Position, Query, QuerySegment, ResourceName,
    ResourceQuerySegment, RELATIVE_WITHOUT_CWD_WARNING,
};
use crate::value::ValueInterface;

fn normalize_namespace(ns: &str) -> &str {
    if ns == "root" {
        ""
    } else {
        ns
    }
}

fn namespaces_for_query(
    query: &Query,
    cmr: &CommandMetadataRegistry,
) -> Result<Vec<String>, Error> {
    let mut namespaces = Vec::new();
    if let Some(ns) = query.last_ns() {
        for x in ns.iter() {
            match x {
                ActionParameter::String(s, _) => namespaces.push(s.to_string()),
                _ => {
                    return Err(Error::not_supported(
                        "Only string parameters are supported in ns".into(),
                    ));
                }
            }
        }
    }
    cmr.default_namespaces.iter().for_each(|x| {
        namespaces.push(x.clone());
    });
    Ok(namespaces)
}

/// Rewrites `query` so every *relative* default link becomes an explicit query link.
///
/// A default link lives in command metadata, not in the query text, so it is invisible to anything
/// that identifies a query — including the asset manager's cache key. That is harmless while the
/// default is absolute, because the metadata reproduces it identically everywhere. A **relative**
/// default such as `-R-key/.` is different: it resolves differently per directory, so a query that
/// leaves it implicit would name one asset for results that legitimately differ.
///
/// Promoting keeps the link relative; freezing resolves it afterwards, like any other operand. The
/// result is built as an AST rather than by concatenating text, per `QUERY-BUILDER-TOOLING`.
fn promote_relative_default_links(
    query: &Query,
    cmr: &CommandMetadataRegistry,
) -> Result<Query, Error> {
    let mut promoted = query.clone();
    for segment in promoted.segments.iter_mut() {
        let QuerySegment::Transform(transform) = segment else {
            continue;
        };
        for action in transform.query.iter_mut() {
            let namespaces = namespaces_for_query(query, cmr)?;
            let realm = query.last_transform_query_name().unwrap_or_default();
            let Some(metadata) = cmr.find_command_in_namespaces(&realm, &namespaces, &action.name)
            else {
                // An unresolvable action is not this function's error to report; plan building
                // reaches the same command and produces a proper diagnostic with a position.
                continue;
            };
            for (index, argument) in metadata.arguments.iter().enumerate() {
                if action.parameters.len() > index {
                    continue; // supplied explicitly; no default in play
                }
                let CommandParameterValue::Query(default) = &argument.default else {
                    continue;
                };
                if !default.has_relative_operand() {
                    continue; // absolute: metadata reproduces it, so leave it implicit
                }
                if action.parameters.len() != index {
                    // An earlier argument was omitted too, so appending would bind the link to
                    // *that* slot and leave this one implicit — the recorded query would mean
                    // something other than the plan it was recorded for. Writing the earlier
                    // defaults out to keep the positions is possible but not always (a
                    // placeholder or an injected argument has nothing to write), so the query is
                    // recorded unpromoted instead. That is merely less self-contained, never
                    // wrong; see PREDECESSOR-CUT-NOT-YET-EQUIVALENT.
                    break;
                }
                action.parameters.push(ActionParameter::Link(
                    default.clone(),
                    action.position.clone(),
                ));
            }
        }
    }
    Ok(promoted)
}

fn append_actions(query: &Query, actions: Vec<ActionRequest>) -> Query {
    let mut q = query.clone();
    match q.segments.last_mut() {
        None => {
            q.segments.push(QuerySegment::Transform(
                crate::query::TransformQuerySegment {
                    header: None,
                    query: actions,
                    filename: None,
                },
            ));
        }
        Some(QuerySegment::Resource(_)) => {
            q.segments.push(QuerySegment::Transform(
                crate::query::TransformQuerySegment {
                    header: None,
                    query: actions,
                    filename: None,
                },
            ));
        }
        Some(QuerySegment::Transform(tqs)) => {
            tqs.query.extend(actions);
            // Appending actions invalidates transform filename.
            tqs.filename = None;
        }
    }
    q
}

/// Append an action request to a query with optional namespace injection.
///
/// The function first tries plain append. If the resolved command namespace does not
/// match `ns` (treating `root` and empty namespace as equivalent), it prepends `ns-<ns>`
/// before the action. Appending to an existing transform removes its filename because the
/// filename no longer terminates the modified transform.
///
/// This returns an error when an existing `ns` instruction contains non-string parameters.
pub fn append_action(
    query: &Query,
    ns: &str,
    action: ActionRequest,
    cmr: &CommandMetadataRegistry,
) -> Result<Query, Error> {
    let plain_query = append_actions(query, vec![action.clone()]);

    let namespaces = namespaces_for_query(query, cmr)?;
    let realm = query.last_transform_query_name().unwrap_or_default();
    let resolved_plain = cmr.find_command_in_namespaces(&realm, &namespaces, &action.name);

    let requested_ns = normalize_namespace(ns);
    if resolved_plain
        .as_ref()
        .is_some_and(|m| normalize_namespace(&m.namespace) == requested_ns)
    {
        return Ok(plain_query);
    }

    if requested_ns.is_empty() {
        return Ok(plain_query);
    }

    let ns_action = ActionRequest::new("ns".to_string())
        .with_parameters(vec![ActionParameter::new_string(requested_ns.to_string())]);
    Ok(append_actions(query, vec![ns_action, action]))
}

/// One operation or diagnostic in an executable [`Plan`].
///
/// Data-producing variants replace the current interpreter value. Context modifiers such as
/// [`Step::Filename`] and [`Step::SetCwd`] change execution context without producing data.
/// [`Step::Info`], [`Step::Warning`], and [`Step::Error`] in `Plan::steps` are executable
/// diagnostics; a `Step::Error` logs an error but is not the same as [`Plan::error`].
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Step {
    /// Load the evaluated state of the asset at a logical key.
    GetAsset(Key),
    /// Load the binary representation of the asset at a logical key.
    GetAssetBinary(Key),
    /// Load metadata for the asset at a logical key.
    GetAssetMetadata(Key),
    /// Load the recipe associated with a keyed asset.
    GetAssetRecipe(Key),
    /// List the evaluated asset directory at a logical key.
    GetAssetDirectory(Key),
    /// Read data directly from the backing store, bypassing asset evaluation.
    GetResource(Key),
    /// Read metadata directly from the backing store.
    GetResourceMetadata(Key),
    /// List a directory directly in the backing store.
    GetResourceDirectory(Key),
    /// Evaluate a nested query and use its resulting value.
    Evaluate(Query),
    /// Use a query itself as the current value instead of evaluating it.
    UseQueryValue(Query),
    /// Invoke a resolved command with resolved parameters.
    Action {
        /// Command realm selected by the query.
        realm: String,
        /// Resolved command namespace.
        ns: String,
        /// Registered command name.
        action_name: String,
        /// Source position of the action for diagnostics.
        position: Position,
        /// Parameter values resolved from metadata, query text, and overrides.
        parameters: ResolvedParameterValues,
    },
    /// Set the output filename in the current execution context.
    Filename(ResourceName),
    /// Emit an informational execution log entry.
    Info(String),
    /// Emit a warning execution log entry.
    Warning(String),
    /// Emit an error-level execution log entry without itself aborting execution.
    Error(String),
    /// Execute a nested, already-built plan.
    Plan(Plan),
    /// Set the logical working key used to resolve relative references.
    SetCwd(Key),
    /// Convert a logical key into the current value without loading that key.
    UseKeyValue(Key),
}

impl Step {
    /// Returns whether this is an executable error diagnostic.
    pub fn is_error(&self) -> bool {
        match self {
            Step::Error(_) => true,
            _ => false,
        }
    }
    /// Returns whether this is an executable warning diagnostic.
    pub fn is_warning(&self) -> bool {
        match self {
            Step::Warning(_) => true,
            _ => false,
        }
    }
    /// Returns whether this step invokes a registered command.
    pub fn is_action(&self) -> bool {
        match self {
            Step::Action { .. } => true,
            _ => false,
        }
    }

    /// Returns whether this step changes context or emits a diagnostic without producing data.
    pub fn is_context_modifier(&self) -> bool {
        match self {
            Step::GetAsset(_key) => false,
            Step::GetAssetBinary(_key) => false,
            Step::GetAssetMetadata(_key) => false,
            Step::GetAssetRecipe(_key) => false,
            Step::GetAssetDirectory(_key) => false,
            Step::GetResource(_key) => false,
            Step::GetResourceMetadata(_key) => false,
            Step::GetResourceDirectory(_key) => false,
            Step::Evaluate(_) => false,
            Step::UseQueryValue(_) => false,
            Step::Action { .. } => false,
            Step::Filename(_resource_name) => true,
            Step::Info(_) => true,
            Step::Warning(_) => true,
            Step::Error(_) => true,
            Step::Plan(_) => false,
            Step::SetCwd(_) => true,
            Step::UseKeyValue(_) => false,
        }
    }
}

fn resource_query_step_matches(resource: &ResourceQuerySegment, step: &Step) -> bool {
    let key = &resource.key;
    let instruction = resource
        .header
        .as_ref()
        .and_then(|header| header.parameters.first())
        .map(|parameter| parameter.value.as_str());

    match instruction {
        None => matches!(step, Step::GetAsset(step_key) if step_key == key),
        Some("b" | "bin" | "binary") => {
            matches!(step, Step::GetAssetBinary(step_key) if step_key == key)
        }
        Some("meta" | "metadata") => {
            matches!(step, Step::GetAssetMetadata(step_key) if step_key == key)
        }
        Some("dir" | "directory") => {
            matches!(step, Step::GetAssetDirectory(step_key) if step_key == key)
        }
        Some("sdir" | "store_directory") => {
            matches!(step, Step::GetResourceDirectory(step_key) if step_key == key)
        }
        Some("r" | "recipe") => {
            matches!(step, Step::GetAssetRecipe(step_key) if step_key == key)
        }
        Some("data" | "value") => {
            matches!(step, Step::GetAsset(step_key) if step_key == key)
        }
        Some("stored" | "stored_binary" | "stored_bin" | "sbin") => {
            matches!(step, Step::GetResource(step_key) if step_key == key)
        }
        Some("stored_meta" | "stored_metadata") => {
            matches!(step, Step::GetResourceMetadata(step_key) if step_key == key)
        }
        Some("cwd") => matches!(step, Step::SetCwd(step_key) if step_key == key),
        Some("key") => matches!(step, Step::UseKeyValue(step_key) if step_key == key),
        Some(_) => false,
    }
}

/// A partially or fully resolved command argument stored in an action [`Step`].
///
/// Values retain their source—command default, query parameter, recipe override, enum mapping,
/// or context injection—so diagnostics and metadata can explain how an argument was obtained.
/// Link variants contain queries that the interpreter resolves before command invocation.
/// [`ParameterValue::MultipleParameters`] represents a variadic argument.
///
/// [`ParameterValue::Placeholder`] is permitted while building a recipe so a named override can
/// fill the value later. [`ParameterValue::None`] is an intermediate unresolved state and should
/// not remain in an executable plan.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ParameterValue {
    /// Default value of the parameter from the command metadata.
    DefaultValue(String, Value),
    /// Default link of the parameter from the command metadata.
    DefaultLink(String, Query),
    /// Resolved value of the parameter from the action request.
    ParameterValue(String, Value, Position),
    /// Resolved link of the parameter from the action request.
    ParameterLink(String, Query, Position),
    /// Value override supplied by a recipe.
    OverrideValue(String, Value),
    /// Link override supplied by a recipe.
    OverrideLink(String, Query),
    /// Unresolved named parameter that a later recipe override is expected to fill.
    Placeholder(String),
    /// Query link selected through an enum alias.
    EnumLink(String, Query, Position),
    /// Resolved elements of a variadic argument, with the argument's name.
    ///
    /// The name is carried here for the same reason every other variant carries one: an argument
    /// slot that cannot report its name cannot be found by [`ResolvedParameterValues::override_value`]
    /// or [`ResolvedParameterValues::override_link`], so a recipe could not override it. It cannot
    /// be derived from the elements, because an empty list has none.
    MultipleParameters(String, Vec<ParameterValue>),
    /// Parameter supplied by the command execution context rather than query text.
    Injected(String),
    /// Intermediate state representing an unresolved parameter.
    None,
}

impl Display for ParameterValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParameterValue::DefaultValue(name, v) => write!(f, "default {name}: {v}"),
            ParameterValue::DefaultLink(name, q) => {
                write!(f, "default link {}: {}", name, q.encode())
            }
            ParameterValue::ParameterValue(name, v, _) => write!(f, "value {name}: {v}"),
            ParameterValue::ParameterLink(name, q, _) => write!(f, "link {}: {}", name, q.encode()),
            ParameterValue::OverrideValue(name, v) => write!(f, "override {name}: {v}"),
            ParameterValue::OverrideLink(name, q) => {
                write!(f, "override link {}: {}", name, q.encode())
            }
            ParameterValue::EnumLink(name, q, _) => write!(f, "enum link {}: {}", name, q.encode()),
            ParameterValue::MultipleParameters(_, v) => {
                write!(
                    f,
                    "multiple:{}",
                    v.iter().map(|x| format!("{}", x)).join(",")
                )
            }
            ParameterValue::Injected(name) => write!(f, "injected {}", name),
            ParameterValue::None => write!(f, "None"),
            ParameterValue::Placeholder(name) => write!(f, "placeholder {name}"),
        }
    }
}

impl ParameterValue {
    fn parse_other_enum_value(
        argument_name: &str,
        raw_value: &str,
        enum_type: &EnumArgumentType,
        pos: &Position,
    ) -> Result<Self, Error> {
        let value = match enum_type {
            EnumArgumentType::String | EnumArgumentType::Any => Value::String(raw_value.to_owned()),
            EnumArgumentType::Integer | EnumArgumentType::IntegerOption => {
                let x = raw_value.parse::<i64>().map_err(|_| {
                    Error::conversion_error_with_message(
                        raw_value.to_owned(),
                        "integer",
                        "Expected integer value for enum fallback",
                    )
                    .with_position(pos)
                })?;
                Value::Number(x.into())
            }
            EnumArgumentType::Float | EnumArgumentType::FloatOption => {
                let x = raw_value.parse::<f64>().map_err(|_| {
                    Error::conversion_error_with_message(
                        raw_value.to_owned(),
                        "float",
                        "Expected float value for enum fallback",
                    )
                    .with_position(pos)
                })?;
                Value::Number(
                    serde_json::Number::from_f64(x).ok_or(
                        Error::conversion_error_with_message(
                            raw_value.to_owned(),
                            "float",
                            "Float value is not representable",
                        )
                        .with_position(pos),
                    )?,
                )
            }
            EnumArgumentType::Boolean => match raw_value.to_lowercase().as_str() {
                "true" | "t" | "yes" | "y" | "1" => Value::Bool(true),
                "false" | "f" | "no" | "n" | "0" => Value::Bool(false),
                _ => {
                    return Err(Error::conversion_error_with_message(
                        raw_value.to_owned(),
                        "boolean",
                        "Expected boolean value for enum fallback",
                    )
                    .with_position(pos))
                }
            },
        };
        Ok(ParameterValue::ParameterValue(
            argument_name.to_owned(),
            value,
            pos.to_owned(),
        ))
    }

    /// Creates the initial resolved representation described by command argument metadata.
    ///
    /// Defaults and injected arguments are materialized immediately. A variadic argument becomes
    /// `MultipleParameters`, including any values supplied by an array default.
    pub fn from_arginfo(arginfo: &ArgumentInfo) -> Self {
        if arginfo.multiple {
            let mut values = Vec::new();
            match &arginfo.default {
                CommandParameterValue::Value(v) => match v {
                    Value::Array(a) => {
                        for x in a {
                            values.push(ParameterValue::DefaultValue(
                                arginfo.name.clone(),
                                x.clone(),
                            ));
                        }
                    }
                    _ => values.push(ParameterValue::DefaultValue(
                        arginfo.name.clone(),
                        v.clone(),
                    )),
                },
                CommandParameterValue::Query(q) => {
                    values.push(ParameterValue::DefaultLink(arginfo.name.clone(), q.clone()))
                }
                CommandParameterValue::None => (),
            }
            ParameterValue::MultipleParameters(arginfo.name.clone(), values)
        } else {
            match &arginfo.default {
                CommandParameterValue::Value(x) => {
                    ParameterValue::DefaultValue(arginfo.name.clone(), x.clone())
                }
                CommandParameterValue::Query(q) => {
                    ParameterValue::DefaultLink(arginfo.name.clone(), q.clone())
                }
                CommandParameterValue::None => {
                    if arginfo.injected {
                        ParameterValue::Injected(arginfo.name.clone())
                    } else {
                        ParameterValue::None
                    }
                }
            }
        }
    }
    /// Converts a command-metadata default into a parameter value with the supplied name.
    pub fn from_command_parameter_value(name: &str, cpv: &CommandParameterValue) -> Self {
        match cpv {
            CommandParameterValue::Value(x) => {
                ParameterValue::DefaultValue(name.to_owned(), x.clone())
            } // TODO: pass name
            CommandParameterValue::Query(q) => {
                ParameterValue::DefaultLink(name.to_owned(), q.clone())
            }
            CommandParameterValue::None => ParameterValue::None,
        }
    }

    /// Converts an unresolved `None` into an `ArgumentMissing` error at `position`.
    ///
    /// Other variants are returned unchanged. The closure is evaluated only when the value is
    /// missing.
    pub fn to_result(self, error: impl Fn() -> String, position: &Position) -> Result<Self, Error> {
        match self {
            ParameterValue::None => {
                Err(Error::new(ErrorType::ArgumentMissing, error()).with_position(position))
            }
            _ => Ok(self),
        }
    }

    /// Parses one textual action parameter according to its command argument metadata.
    ///
    /// Empty strings may select a default or the type's optional fallback. Conversion failures
    /// retain `pos` for source-level diagnostics.
    pub fn from_string(arginfo: &ArgumentInfo, s: &str, pos: &Position) -> Result<Self, Error> {
        match arginfo.argument_type {
            ArgumentType::String => Ok(ParameterValue::ParameterValue(
                arginfo.name.clone(),
                Value::String(s.to_owned()),
                pos.to_owned(),
            )),
            ArgumentType::Integer => {
                if s.is_empty() {
                    return Self::from_arginfo(arginfo).to_result(
                        || format!("Integer argument {} missing", &arginfo.name),
                        pos,
                    );
                }
                let n = s
                    .parse::<i64>()
                    .map_err(|_e| Error::conversion_error_at_position(s, "integer", pos))?;
                Ok(ParameterValue::ParameterValue(
                    arginfo.name.clone(),
                    n.into(),
                    pos.to_owned(),
                ))
            }
            ArgumentType::IntegerOption => {
                if s.is_empty() {
                    let res = Self::from_arginfo(arginfo);
                    if res.is_none() {
                        return Ok(Self::ParameterValue(
                            arginfo.name.clone(),
                            Value::Null,
                            pos.to_owned(),
                        ));
                    } else {
                        return Ok(res);
                    }
                }
                let n = s
                    .parse::<i64>()
                    .map_err(|_e| Error::conversion_error_at_position(s, "integer", pos))?;
                Ok(ParameterValue::ParameterValue(
                    arginfo.name.clone(),
                    n.into(),
                    pos.to_owned(),
                ))
            }
            ArgumentType::Float => {
                if s.is_empty() {
                    return Self::from_arginfo(arginfo)
                        .to_result(|| format!("Float argument {} missing", &arginfo.name), pos);
                }
                let x = s
                    .parse::<f64>()
                    .map_err(|_e| Error::conversion_error_at_position(s, "float", pos))?;
                Ok(ParameterValue::ParameterValue(
                    arginfo.name.clone(),
                    x.into(),
                    pos.to_owned(),
                ))
            }
            ArgumentType::FloatOption => {
                if s.is_empty() {
                    let res = Self::from_arginfo(arginfo);
                    if res.is_none() {
                        return Ok(Self::ParameterValue(
                            arginfo.name.clone(),
                            Value::Null,
                            pos.to_owned(),
                        ));
                    } else {
                        return Ok(res);
                    }
                }
                let x = s
                    .parse::<f64>()
                    .map_err(|_e| Error::conversion_error_at_position(s, "float", pos))?;
                Ok(ParameterValue::ParameterValue(
                    arginfo.name.clone(),
                    x.into(),
                    pos.to_owned(),
                ))
            }
            ArgumentType::Boolean => {
                if s.is_empty() {
                    let res = Self::from_arginfo(arginfo);
                    if res.is_none() {
                        return Ok(Self::ParameterValue(
                            arginfo.name.clone(),
                            Value::Bool(false),
                            pos.to_owned(),
                        ));
                    } else {
                        return Ok(res);
                    }
                }
                match s.to_lowercase().as_str() {
                    "true" | "t" | "yes" | "y" | "1" => Ok(ParameterValue::ParameterValue(
                        arginfo.name.clone(),
                        Value::Bool(true),
                        pos.to_owned(),
                    )),
                    "false" | "f" | "no" | "n" | "0" => Ok(ParameterValue::ParameterValue(
                        arginfo.name.clone(),
                        Value::Bool(false),
                        pos.to_owned(),
                    )),
                    _ => Err(Error::conversion_error_at_position(
                        s.to_owned(),
                        "boolean",
                        pos,
                    )),
                }
            }
            ArgumentType::Enum(ref e) => match e.expand_alias(s) {
                CommandParameterValue::Value(x) => Ok(ParameterValue::ParameterValue(
                    arginfo.name.clone(),
                    x.clone(),
                    pos.to_owned(),
                )),
                CommandParameterValue::Query(q) => Ok(ParameterValue::EnumLink(
                    arginfo.name.clone(),
                    q.clone(),
                    pos.to_owned(),
                )),
                CommandParameterValue::None => {
                    if e.others_allowed {
                        Self::parse_other_enum_value(&arginfo.name, s, &e.value_type, pos)
                    } else {
                        let aliases = e.values.iter().map(|v| v.alias.clone()).join(", ");
                        Err(Error::conversion_error_with_message(
                            s.to_owned(),
                            &e.name,
                            &format!(
                                "Undefined enum value for argument {}. Valid values: {}",
                                arginfo.name, aliases
                            ),
                        )
                        .with_position(pos))
                    }
                }
            },
            ArgumentType::Any => {
                if s.is_empty() {
                    let res = Self::from_arginfo(arginfo);
                    if res.is_none() {
                        Ok(Self::ParameterValue(
                            arginfo.name.clone(),
                            s.into(),
                            pos.to_owned(),
                        ))
                    } else {
                        Ok(res)
                    }
                } else {
                    Ok(ParameterValue::ParameterValue(
                        arginfo.name.clone(),
                        Value::String(s.to_owned()),
                        pos.to_owned(),
                    ))
                }
            }
            ArgumentType::None => Err(Error::not_supported(
                "None not supported as argument type".to_string(),
            )),
            ArgumentType::GlobalEnum(_) => Err(Error::not_supported(
                "GlobalEnum not supported as argument type".to_string(),
            )),
        }
    }

    /// Consumes the action parameter or parameters belonging to `arginfo`.
    ///
    /// Injected arguments consume no query parameters. Variadic arguments consume the iterator's
    /// remainder. If a required scalar is absent, a placeholder is returned only when
    /// `allow_placeholders` is enabled; otherwise this returns `ArgumentMissing`.
    pub fn pop_value(
        arginfo: &ArgumentInfo,
        param: &mut ActionParameterIterator,
        allow_placeholders: bool,
    ) -> Result<Self, Error> {
        let p = Self::from_arginfo(arginfo);
        if arginfo.injected {
            return Ok(p);
        }

        if arginfo.multiple {
            let mut values = Vec::new();
            for x in &mut *param {
                match x {
                    ActionParameter::String(s, pos) => {
                        let pv = Self::from_string(arginfo, s, pos)?;
                        match pv {
                            ParameterValue::ParameterValue(_, _, _) => values.push(pv),
                            ParameterValue::DefaultValue(_, _) => values.push(pv),
                            ParameterValue::OverrideValue(_, _) => values.push(pv),
                            ParameterValue::DefaultLink(_, _) => values.push(pv),
                            ParameterValue::ParameterLink(_, _, _) => values.push(pv),
                            ParameterValue::OverrideLink(_, _) => values.push(pv),
                            ParameterValue::EnumLink(_, _, _) => values.push(pv),
                            ParameterValue::MultipleParameters(_, _) => {
                                return Err(Error::unexpected_error(
                                    "Multiple parameters not supported inside vector argument"
                                        .to_string(),
                                )
                                .with_position(pos))
                            }
                            ParameterValue::Injected(name) => {
                                return Err(Error::unexpected_error(format!(
                                    "Injected values ({name}) not supported inside vector argument"
                                ))
                                .with_position(pos))
                            }
                            ParameterValue::None => {
                                return Err(Error::unexpected_error(
                                    "None value not supported inside vector argument".to_string(),
                                )
                                .with_position(pos))
                            }
                            ParameterValue::Placeholder(name) => {
                                return Err(Error::general_error(format!(
                                    "Placeholder '{name}' not supported inside vector argument"
                                ))
                                .with_position(pos))
                            }
                        }
                    }
                    ActionParameter::Link(q, pos) => {
                        values.push(ParameterValue::ParameterLink(
                            arginfo.name.clone(),
                            q.clone(),
                            pos.clone(),
                        ));
                    }
                }
            }
            return Ok(ParameterValue::MultipleParameters(
                arginfo.name.clone(),
                values,
            ));
        }

        match param.next() {
            Some(ActionParameter::String(s, pos)) => Self::from_string(arginfo, s, pos),
            Some(ActionParameter::Link(q, pos)) => Ok(ParameterValue::ParameterLink(
                arginfo.name.clone(),
                q.clone(),
                pos.clone(),
            )),
            None => {
                // Try to apply the default value first
                let default = Self::from_arginfo(arginfo);
                if !default.is_none() {
                    Ok(default)
                } else if allow_placeholders {
                    Ok(ParameterValue::Placeholder(arginfo.name.clone()))
                } else {
                    Self::from_arginfo(arginfo).to_result(
                        || format!("Missing argument '{}' (pop_value)", arginfo.name),
                        &param.position,
                    )
                }
            }
        }
    }
    /// Returns whether this value came from command metadata as a default.
    pub fn is_default(&self) -> bool {
        match self {
            ParameterValue::DefaultValue(_, _) => true,
            ParameterValue::DefaultLink(_, _) => true,
            _ => false,
        }
    }
    /// Returns whether this is the unresolved `None` state.
    pub fn is_none(&self) -> bool {
        match self {
            ParameterValue::None => true,
            _ => false,
        }
    }
    /// Returns whether this value contains a query link.
    pub fn is_link(&self) -> bool {
        match self {
            ParameterValue::DefaultLink(_, _) => true,
            ParameterValue::ParameterLink(_, _, _) => true,
            ParameterValue::OverrideLink(_, _) => true,
            ParameterValue::EnumLink(_, _, _) => true,
            _ => false,
        }
    }
    /// Returns whether this parameter is supplied by the execution context.
    pub fn is_injected(&self) -> bool {
        match self {
            ParameterValue::Injected(_) => true,
            _ => false,
        }
    }
    /// Returns whether this represents a variadic argument.
    pub fn is_multiple(&self) -> bool {
        match self {
            ParameterValue::MultipleParameters(_, _) => true,
            _ => false,
        }
    }
    /// Returns the command argument name, or `None` for unnamed aggregate and empty variants.
    pub fn name(&self) -> Option<String> {
        match self {
            ParameterValue::DefaultValue(name, _) => Some(name.clone()),
            ParameterValue::DefaultLink(name, _) => Some(name.clone()),
            ParameterValue::ParameterValue(name, _, _) => Some(name.clone()),
            ParameterValue::ParameterLink(name, _, _) => Some(name.clone()),
            ParameterValue::OverrideValue(name, _) => Some(name.clone()),
            ParameterValue::OverrideLink(name, _) => Some(name.clone()),
            ParameterValue::EnumLink(name, _, _) => Some(name.clone()),
            ParameterValue::Injected(name) => Some(name.clone()),
            ParameterValue::Placeholder(name) => Some(name.clone()),
            ParameterValue::MultipleParameters(name, _) => Some(name.clone()),
            ParameterValue::None => None,
        }
    }
    /// Returns a clone of the contained JSON value, when this is a value variant.
    pub fn value(&self) -> Option<Value> {
        match self {
            ParameterValue::DefaultValue(_, v) => Some(v.clone()),
            ParameterValue::ParameterValue(_, v, _) => Some(v.clone()),
            ParameterValue::OverrideValue(_, v) => Some(v.clone()),
            _ => None,
        }
    }
    /// Returns a clone of the contained query, when this is a link variant.
    pub fn link(&self) -> Option<Query> {
        match self {
            ParameterValue::DefaultLink(_, q) => Some(q.clone()),
            ParameterValue::ParameterLink(_, q, _) => Some(q.clone()),
            ParameterValue::OverrideLink(_, q) => Some(q.clone()),
            ParameterValue::EnumLink(_, q, _) => Some(q.clone()),
            _ => None,
        }
    }
    /// Returns clones of the elements of a variadic argument.
    pub fn multiple(&self) -> Option<Vec<ParameterValue>> {
        match self {
            ParameterValue::MultipleParameters(_, v) => Some(v.clone()),
            _ => None,
        }
    }
    /// Returns the source position retained by query- and enum-derived values.
    ///
    /// Variants without a query-text origin return [`Position::unknown`].
    pub fn position(&self) -> Position {
        match self {
            ParameterValue::ParameterValue(_, _, pos) => pos.clone(),
            ParameterValue::ParameterLink(_, _, pos) => pos.clone(),
            ParameterValue::EnumLink(_, _, pos) => pos.clone(),
            _ => Position::unknown(),
        }
    }
}

/// Number of action parameters a command can consume, from argument slot `skip` onward.
///
/// This is deliberately not `arguments.len()`. Injected arguments are excluded because they are
/// supplied by the execution context and consume no query parameter, and `skip` accounts for
/// alias head parameters, which fill leading slots before the action is consulted. Reporting the
/// raw length would tell the author of an aliased or injected command that their query accepts
/// more parameters than it does.
fn accepted_parameter_count(command_metadata: &CommandMetadata, skip: usize) -> usize {
    command_metadata
        .arguments
        .iter()
        .skip(skip)
        .filter(|a| !a.injected)
        .count()
}

/// Ordered command arguments resolved for an action [`Step`].
///
/// Entries correspond to command metadata order. Injected arguments remain represented by
/// [`ParameterValue::Injected`] markers so the executor can fill them without consuming query
/// parameters.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ResolvedParameterValues(pub Vec<ParameterValue>);

impl ResolvedParameterValues {
    /// Rewrites every link query in these parameters into CWD-absolute form.
    ///
    /// Each link is resolved against a **clone** of `cursor`, so a `-R-cwd` inside a link scopes
    /// only that link and cannot move the enclosing plan's working key. This mirrors the scope rule
    /// `find_dependencies` already applies, and is what
    /// `find_dependencies_child_query_cwd_does_not_leak` pins.
    pub(crate) fn freeze_cwd(&mut self, cursor: &mut CwdCursor) {
        for parameter in self.0.iter_mut() {
            parameter.freeze_cwd(cursor);
        }
    }
}

impl ParameterValue {
    /// Rewrites this parameter's link query, if it has one, into CWD-absolute form.
    ///
    /// See [`ResolvedParameterValues::freeze_cwd`] for the scope rule.
    pub(crate) fn freeze_cwd(&mut self, cursor: &mut CwdCursor) {
        match self {
            ParameterValue::DefaultLink(_, query)
            | ParameterValue::ParameterLink(_, query, _)
            | ParameterValue::OverrideLink(_, query)
            | ParameterValue::EnumLink(_, query, _) => {
                let mut scoped = cursor.clone();
                *query = scoped.resolve_query_scoped(query);
                // The scope protects the working key, not the diagnostics: a link that fell back
                // to logical root still owes the caller its one warning.
                cursor.absorb_diagnostics(&scoped);
            }
            ParameterValue::MultipleParameters(_, values) => {
                for value in values.iter_mut() {
                    value.freeze_cwd(cursor);
                }
            }
            ParameterValue::DefaultValue(_, _)
            | ParameterValue::ParameterValue(_, _, _)
            | ParameterValue::OverrideValue(_, _)
            | ParameterValue::Placeholder(_)
            | ParameterValue::Injected(_)
            | ParameterValue::None => {}
        }
    }
}
impl Default for ResolvedParameterValues {
    fn default() -> Self {
        Self::new()
    }
}

impl ResolvedParameterValues {
    /// Creates an empty parameter list.
    pub fn new() -> Self {
        ResolvedParameterValues(Vec::new())
    }
    /// Resolves an action, optionally prefixing arguments supplied by an alias definition.
    ///
    /// `head_parameters` fill the first command argument slots; remaining slots consume the
    /// action request. Defaults and injection rules come from `command_metadata`. Missing required
    /// arguments become placeholders only when `allow_placeholders` is true.
    pub fn from_action_extended(
        action_request: &ActionRequest,
        command_metadata: &CommandMetadata,
        head_parameters: &[CommandParameterValue],
        allow_placeholders: bool,
    ) -> Result<Self, Error> {
        let mut parameters = ActionParameterIterator::new(action_request);
        let mut values = head_parameters
            .iter()
            .zip(command_metadata.arguments.iter())
            .map(|(x, arginfo)| ParameterValue::from_command_parameter_value(&arginfo.name, x))
            .collect_vec();
        let n = values.len();
        for a in command_metadata.arguments.iter().skip(n) {
            let pv = ParameterValue::pop_value(a, &mut parameters, allow_placeholders)?;
            values.push(pv);
        }

        // Every declared argument has been served. Anything the action still holds is surplus:
        // no argument will consume it, so accepting it silently would discard what was written.
        //
        // The leftover is discovered by asking the iterator rather than by comparing counts.
        // That is what keeps the two exemptions correct without special-casing either: a
        // `multiple` argument has already drained the iterator, and an injected argument never
        // took a parameter from it.
        if let Some(excess) = parameters.next() {
            return Err(Error::too_many_parameters(
                &format!("command '{}'", command_metadata.name),
                accepted_parameter_count(command_metadata, n),
                // `next()` increments before returning, so this is already the 1-based index
                // of `excess` in the written parameter list.
                parameters.parameter_number,
                &excess.encode(),
                &excess.position(),
            ));
        }
        Ok(ResolvedParameterValues(values))
    }
    /// Resolves an action without alias-supplied leading parameters.
    pub fn from_action(
        action_request: &ActionRequest,
        command_metadata: &CommandMetadata,
        allow_placeholders: bool,
    ) -> Result<Self, Error> {
        Self::from_action_extended(action_request, command_metadata, &[], allow_placeholders)
    }

    /// Removes all resolved parameters.
    pub fn clear(&mut self) {
        self.0.clear();
    }
    /// Returns the number of command argument slots.
    pub fn len(&self) -> usize {
        self.0.len()
    }
    /// Returns whether the list contains no command argument slots.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    /// Replaces the named, non-injected argument with a value override.
    ///
    /// Returns `false` when the name is absent or identifies an injected argument.
    pub fn override_value(&mut self, name: &str, value: Value) -> bool {
        for pv in &mut self.0 {
            if let Some(n) = pv.name() {
                if n == name {
                    if pv.is_injected() {
                        // TODO: maybe this could be an error
                        return false;
                    }
                    // A variadic slot stays a parameter list. Replacing it with a scalar
                    // `OverrideValue` would make `CommandArguments::get_multiple` reject it, and
                    // the command declared the argument as a list. An array override supplies one
                    // element per entry, mirroring how `from_arginfo` expands an array default.
                    if pv.is_multiple() {
                        let elements = match &value {
                            Value::Array(entries) => entries
                                .iter()
                                .map(|entry| {
                                    ParameterValue::OverrideValue(n.clone(), entry.clone())
                                })
                                .collect(),
                            _ => vec![ParameterValue::OverrideValue(n.clone(), value.clone())],
                        };
                        *pv = ParameterValue::MultipleParameters(n.clone(), elements);
                        return true;
                    }
                    *pv = ParameterValue::OverrideValue(n.clone(), value.clone());
                    return true;
                }
            }
        }
        false
    }
    /// Replaces the named, non-injected argument with a query-link override.
    ///
    /// Returns `false` when the name is absent or identifies an injected argument.
    pub fn override_link(&mut self, name: &str, query: Query) -> bool {
        for pv in &mut self.0 {
            if let Some(n) = pv.name() {
                if n == name {
                    if pv.is_injected() {
                        // TODO: maybe this could be an error
                        return false;
                    }
                    // As in `override_value`: a variadic slot stays a parameter list, holding one
                    // linked element. The interpreter materialises links inside a list
                    // element-wise, so `get_multiple` sees a value by the time it runs.
                    //
                    // A link whose result is an array yields one element holding that array,
                    // not one element per entry - see LINK-IN-VARIADIC-DOES-NOT-EXPAND.
                    if pv.is_multiple() {
                        *pv = ParameterValue::MultipleParameters(
                            n.clone(),
                            vec![ParameterValue::OverrideLink(n.clone(), query.clone())],
                        );
                        return true;
                    }
                    *pv = ParameterValue::OverrideLink(n.clone(), query.clone());
                    return true;
                }
            }
        }
        false
    }

    /// Iterates over resolved parameters in command metadata order.
    pub fn iter(&self) -> std::slice::Iter<'_, ParameterValue> {
        self.0.iter()
    }

    /// Mutably iterates over resolved parameters in command metadata order.
    pub fn iter_mut(&mut self) -> std::slice::IterMut<'_, ParameterValue> {
        self.0.iter_mut()
    }

    /// Returns the parameter at `index`.
    pub fn get(&self, index: usize) -> Option<&ParameterValue> {
        self.0.get(index)
    }

    /// Returns the mutable parameter at `index`.
    pub fn get_mut(&mut self, index: usize) -> Option<&mut ParameterValue> {
        self.0.get_mut(index)
    }
}

impl IntoIterator for ResolvedParameterValues {
    type Item = ParameterValue;
    type IntoIter = std::vec::IntoIter<ParameterValue>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

/// Cursor over the parameters of one [`ActionRequest`].
///
/// The cursor retains the most recently consumed source position so a missing trailing argument
/// can still be reported near the action that required it.
pub struct ActionParameterIterator<'a> {
    /// Action whose parameters are being consumed.
    pub action_request: &'a ActionRequest,
    /// Zero-based index of the next parameter.
    pub parameter_number: usize,
    /// Position of the last consumed parameter, or the action position initially.
    pub position: Position,
}

impl<'a> ActionParameterIterator<'a> {
    /// Creates a cursor positioned before the first action parameter.
    pub fn new(action_request: &'a ActionRequest) -> Self {
        ActionParameterIterator {
            action_request,
            parameter_number: 0,
            position: action_request.position.clone(),
        }
    }
}

impl<'a> Iterator for ActionParameterIterator<'a> {
    type Item = &'a ActionParameter;
    fn next(&mut self) -> Option<Self::Item> {
        if self.parameter_number < self.action_request.parameters.len() {
            let p = &self.action_request.parameters[self.parameter_number];
            self.parameter_number += 1;
            self.position = p.position();
            Some(p)
        } else {
            None
        }
    }
}

/// Synchronously compiles a [`Query`] into a preliminary [`Plan`].
///
/// The builder resolves commands and parameters against borrowed command metadata. It expands
/// predecessor queries and rejects missing required parameters by default. Recipe construction can
/// enable placeholders and fill them afterward with named overrides.
///
/// The resulting volatility and expiration fields include command and link information available
/// without an environment. Dependency-derived values are incorporated later by interpreter
/// finalization.
pub struct PlanBuilder<'c> {
    query: Query,
    command_registry: &'c CommandMetadataRegistry,
    plan: Plan,
    allow_placeholders: bool,

    /// Track volatility during plan building
    is_volatile: bool,

    /// Track payload requirement during plan building.
    /// Mirrors `is_volatile`; a `Required` outcome also forces volatility, since the
    /// commands that set it are registered with `volatile` already set.
    payload_required: PayloadRequirement,

    /// Track expiration during plan building (minimum of all command expirations)
    expires: Expires,
}

// TODO: support cache
// TODO: support volatile flags
// TODO: support inline flag
//
// `PlanBuilder` also *records* facts it does not act on, for the passes that run after it:
// [`Plan::predecessor`] and [`Plan::predecessor_steps`] describe a boundary it never cuts, and
// [`Plan::volatility_source`] distinguishes volatility a boundary may be cut in front of from
// volatility that forbids one anywhere. The builder always expands; cutting is a separate pass
// over a frozen plan, so that volatility, payload requirement and expiration are computed over
// the whole plan before anything is moved.
impl<'c> PlanBuilder<'c> {
    /// Creates a builder that expands predecessors and rejects placeholders.
    pub fn new(query: Query, command_registry: &'c CommandMetadataRegistry) -> Self {
        PlanBuilder {
            query,
            command_registry,
            plan: Plan::new(),
            allow_placeholders: false,
            is_volatile: false,
            payload_required: PayloadRequirement::None,
            expires: Expires::Never,
        }
    }
    /// Allows missing required arguments to become named placeholders.
    ///
    /// This is primarily used by recipes, which apply named overrides after the initial build.
    pub fn with_placeholders_allowed(mut self) -> Self {
        self.allow_placeholders = true;
        self
    }
    /// Mark plan as volatile and add explanatory Step::Info
    fn mark_volatile(&mut self, reason: &str, scope: VolatilitySource) {
        if !self.is_volatile {
            self.is_volatile = true;
            self.plan.init_info(reason.to_string());
        }
        // Deliberately outside the early-out above. A `Declared` source arriving after the plan
        // is already volatile must still be recorded, or `vol_cmd/v/tail` looks positional and
        // gets a boundary cut out of it.
        self.plan.upgrade_volatility_source(scope);
    }

    /// Helper: check if action command is volatile via CommandMetadata
    fn is_action_volatile(&self, command_key: &CommandKey) -> bool {
        if let Some(metadata) = self.command_registry.get(command_key.clone()) {
            metadata.volatile
        } else {
            false
        }
    }

    /// Mark plan as requiring an evaluation payload and add an explanatory Step::Info.
    ///
    /// Mirrors [`Self::mark_volatile`], including its transition guard: the message is
    /// recorded once, on the first command that causes the requirement, and not at all for
    /// a plan that needs no payload.
    fn mark_payload_required(&mut self, reason: &str) {
        if !self.payload_required.is_required() {
            self.payload_required = PayloadRequirement::Required;
            self.plan.init_info(reason.to_string());
        }
    }

    /// Helper: read the payload requirement of an action command via CommandMetadata
    fn action_payload_requirement(&self, command_key: &CommandKey) -> PayloadRequirement {
        if let Some(metadata) = self.command_registry.get(command_key.clone()) {
            metadata.payload_required
        } else {
            PayloadRequirement::None
        }
    }

    /// Update plan expiration by combining command expiration constraints.
    fn update_expiration(&mut self, command_expires: &Expires) {
        let previous = self.expires.clone();
        self.expires |= command_expires.clone();
        if self.expires != previous {
            self.plan.init_info(format!(
                "Plan expiration updated: '{}' | '{}' -> '{}'",
                previous, command_expires, self.expires
            ));
        }
        if self.expires.is_volatile() {
            self.is_volatile = true;
        }
    }

    /// Look up expiration specification from command metadata
    fn get_action_expiration(&self, command_key: &CommandKey) -> Expires {
        if let Some(metadata) = self.command_registry.get(command_key.clone()) {
            metadata.expires.clone()
        } else {
            Expires::Never
        }
    }

    /// Helper: check if parameters contain links to volatile or payload-requiring queries
    fn check_parameters_for_volatile_links(
        &mut self,
        params: &ResolvedParameterValues,
    ) -> Result<(), Error> {
        for param in &params.0 {
            self.check_parameter_for_volatile_links(param)?;
        }
        Ok(())
    }

    /// Helper: recursively check a single parameter for volatile or payload-requiring links.
    ///
    /// Both properties are read from the same sub-plan, so a link is only built once.
    fn check_parameter_for_volatile_links(&mut self, param: &ParameterValue) -> Result<(), Error> {
        match param {
            ParameterValue::DefaultLink(_, query)
            | ParameterValue::ParameterLink(_, query, _)
            | ParameterValue::OverrideLink(_, query)
            | ParameterValue::EnumLink(_, query, _) => {
                // Build a sub-plan for the linked query and check if it's volatile
                let mut link_pb = PlanBuilder::new(query.clone(), self.command_registry);
                let link_plan = link_pb.build()?;
                if link_plan.is_volatile {
                    self.mark_volatile(
                        &format!(
                            "Volatile due to link parameter to volatile query: {}",
                            query
                        ),
                        // The link is consumed at one action, so everything ahead of it is
                        // still pure; the linked query's own scope governs the linked asset.
                        VolatilitySource::Positional,
                    );
                }
                if link_plan.payload_required.is_required() {
                    self.mark_payload_required(&format!(
                        "Payload required due to link parameter to payload-requiring query: {}",
                        query
                    ));
                }
            }
            ParameterValue::MultipleParameters(_, params) => {
                // Recursively check nested parameters
                for nested_param in params {
                    self.check_parameter_for_volatile_links(nested_param)?;
                }
            }
            _ => {
                // Other parameter types don't affect volatility or payload requirement
            }
        }
        Ok(())
    }

    /// Builds and returns the current preliminary plan.
    ///
    /// Reusing a builder after this call is not intended: processing appends to its internal plan.
    /// The returned plan has not undergone environment-backed dependency finalization and executes
    /// no commands.
    pub fn build(&mut self) -> Result<Plan, Error> {
        let query = self.query.clone();
        self.plan.query = query.clone();
        self.process_query(&query)?;

        // Set is_volatile field from builder state
        self.plan.is_volatile = self.is_volatile;

        // Set payload_required field from builder state
        self.plan.payload_required = self.payload_required;

        // Set expires field from builder state (first-pass estimate)
        self.plan.expires = self.expires.clone();

        self.plan.check_consistent()?;
        Ok(self.plan.clone())
    }

    fn get_namespaces(&self, query: &Query) -> Result<Vec<String>, Error> {
        namespaces_for_query(query, self.command_registry)
    }

    fn get_command_metadata(
        &mut self,
        query: &Query,
        action_request: &ActionRequest,
    ) -> Result<CommandMetadata, Error> {
        let namespaces = self.get_namespaces(query)?;
        let realm = query.last_transform_query_name().unwrap_or("".to_string());

        if let Some(command_metadata) = self.command_registry.find_command_in_namespaces(
            &realm,
            &namespaces,
            &action_request.name,
        ) {
            Ok(command_metadata.resolve_global_enums(self.command_registry)?)
        } else {
            Err(Error::action_not_registered(action_request, &namespaces)
                .with_query(query)
                .with_position(&action_request.position))
        }
    }

    // TODO: RQS realm should should be supported
    fn process_resource_query(&mut self, rqs: &ResourceQuerySegment) -> Result<(), Error> {
        if let Some(header) = &rqs.header {
            if !header.name.is_empty() {
                self.plan.init_warning(format!(
                    "Resource header name is ignored: '{}'",
                    header.name
                ));
            }
            if let Some(first) = header.parameters.first() {
                // A header takes exactly one instruction. Unlike the header *name* above,
                // which is reserved for a future realm interpretation and so is warned about
                // rather than rejected, nothing will ever consume a second parameter.
                if let Some(excess) = header.parameters.get(1) {
                    return Err(Error::too_many_parameters(
                        "resource header",
                        1,
                        2,
                        &excess.value,
                        &excess.position,
                    ));
                }

                match first.value.as_str() {
                    "b" | "bin" | "binary" => {
                        self.plan.steps.push(Step::GetAssetBinary(rqs.key.clone()));
                    }
                    "meta" | "metadata" => {
                        self.plan
                            .steps
                            .push(Step::GetAssetMetadata(rqs.key.clone()));
                    }
                    "dir" | "directory" => {
                        self.plan
                            .steps
                            .push(Step::GetAssetDirectory(rqs.key.clone()));
                    }
                    "sdir" | "store_directory" => {
                        self.plan
                            .steps
                            .push(Step::GetResourceDirectory(rqs.key.clone()));
                    }
                    "r" | "recipe" => {
                        self.plan.steps.push(Step::GetAssetRecipe(rqs.key.clone()));
                    }
                    "data" | "value" => {
                        self.plan.steps.push(Step::GetAsset(rqs.key.clone()));
                    }
                    "stored" | "stored_binary" | "stored_bin" | "sbin" => {
                        self.plan.steps.push(Step::GetResource(rqs.key.clone()));
                    }
                    "stored_meta" | "stored_metadata" => {
                        self.plan
                            .steps
                            .push(Step::GetResourceMetadata(rqs.key.clone()));
                    }
                    "cwd" => {
                        self.plan.steps.push(Step::SetCwd(rqs.key.clone()));
                    }
                    "key" => {
                        self.plan.steps.push(Step::UseKeyValue(rqs.key.clone()));
                    }
                    _ => {
                        return Err(Error::not_supported(format!(
                            "Unknown resource header instruction '{}'. Valid instructions: \
                             b, bin, binary, meta, metadata, dir, directory, sdir, \
                             store_directory, r, recipe, data, value, stored, stored_binary, \
                             stored_bin, sbin, stored_meta, stored_metadata, cwd, key",
                            first.value
                        ))
                        .with_position(&first.position));
                    }
                }
            } else {
                // A header with no parameters at all is the plain asset request.
                self.plan.steps.push(Step::GetAsset(rqs.key.clone()));
            }
        } else {
            //self.plan.steps.push(Step::GetResource(rqs.key.clone()));
            self.plan.steps.push(Step::GetAsset(rqs.key.clone()));
        }
        Ok(())
    }

    fn process_action(
        &mut self,
        query: &Query,
        action_request: &ActionRequest,
    ) -> Result<(), Error> {
        // Intercept 'v' instruction BEFORE normal action processing
        if action_request.name == "v" {
            // `v` resolves no command metadata, so the arity check in
            // `ResolvedParameterValues::from_action_extended` never sees it. It takes no
            // parameters, so anything written here would be silently discarded.
            if let Some(excess) = action_request.parameters.first() {
                return Err(Error::too_many_parameters(
                    "instruction 'v'",
                    0,
                    1,
                    &excess.encode(),
                    &excess.position(),
                ));
            }
            // `v` is a statement about the whole plan, not about a position in it: it emits no
            // step, and its position carries no information. Nothing here is cacheable.
            self.mark_volatile(
                "Volatile due to instruction 'v'",
                VolatilitySource::Declared,
            );
            return Ok(()); // Don't create Step::Action for 'v'
        }

        let command_metadata = self.get_command_metadata(query, action_request)?;

        // Check if command is volatile
        let command_key = CommandKey::new(
            // TODO: There should be a convinience method to create CommandKey from CommandMetadata
            &command_metadata.realm,
            &command_metadata.namespace,
            &command_metadata.name,
        );
        if self.is_action_volatile(&command_key) {
            self.mark_volatile(
                &format!(
                    "Volatile due to command '{}/{}/{}'",
                    command_metadata.realm, command_metadata.namespace, command_metadata.name
                ),
                VolatilitySource::Positional,
            );
        }

        // Check if command requires an evaluation payload
        if self.action_payload_requirement(&command_key).is_required() {
            self.mark_payload_required(&format!(
                "Payload required due to command '{}/{}/{}'",
                command_metadata.realm, command_metadata.namespace, command_metadata.name
            ));
        }

        // Check if command has expiration specification
        let action_expires = self.get_action_expiration(&command_key);
        if !action_expires.is_never() {
            self.update_expiration(&action_expires);
        }

        match &command_metadata.definition {
            command_metadata::CommandDefinition::Registered => {
                // Resolve parameters first
                let parameters = ResolvedParameterValues::from_action(
                    action_request,
                    &command_metadata,
                    self.allow_placeholders,
                )?;

                // Check parameters for links to volatile queries
                self.check_parameters_for_volatile_links(&parameters)?;

                self.plan.steps.push(Step::Action {
                    realm: command_metadata.realm.clone(),
                    ns: command_metadata.namespace.clone(),
                    action_name: action_request.name.clone(),
                    position: action_request.position.clone(),
                    parameters,
                });
            }
            command_metadata::CommandDefinition::Alias {
                command,
                head_parameters,
            } => {
                let original_key = command_metadata.key();
                self.plan.steps.push(Step::Info(format!(
                    "Alias command {} to {}",
                    original_key, &command
                )));

                // Resolve parameters first
                let parameters = ResolvedParameterValues::from_action_extended(
                    action_request,
                    &command_metadata,
                    head_parameters,
                    self.allow_placeholders,
                )?;

                // Check parameters for links to volatile queries
                self.check_parameters_for_volatile_links(&parameters)?;

                self.plan.steps.push(Step::Action {
                    realm: command.realm.clone(),
                    ns: command.namespace.clone(),
                    action_name: command.name.clone(),
                    position: action_request.position.clone(),
                    parameters,
                });
            }
        }

        Ok(())
    }

    fn strip_q_instruction(&self, query: &Query) -> Query {
        // Get the last segment
        if let Some(QuerySegment::Transform(tqs)) = query.segments.last() {
            // Check if last action in transform segment is "q"
            if tqs.query.last().is_some_and(|a| a.is_q()) {
                // Reconstruct query without the "q" action
                let mut new_tqs = tqs.clone();
                new_tqs.query.pop(); // Remove the "q" action

                let mut new_segments = query.segments[..query.segments.len() - 1].to_vec();
                if !new_tqs.is_empty() {
                    new_segments.push(QuerySegment::Transform(new_tqs));
                }

                return Query {
                    segments: new_segments,
                    absolute: query.absolute,
                    source: query.source.clone(),
                };
            }
        }
        query.clone()
    }

    fn process_query(&mut self, query: &Query) -> Result<(), Error> {
        //eprintln!("process query {}", query);
        if query.is_empty() || query.is_ns() {
            return Ok(());
        }

        // Check if query ends with "q" instruction
        if query.is_q() {
            // Validate that "q" has no arguments
            if let Some(QuerySegment::Transform(tqs)) = query.segments.last() {
                if let Some(q_action) = tqs.query.last() {
                    if let (true, Some(excess)) = (q_action.is_q(), q_action.parameters.first()) {
                        // Already rejected before this design; it only lacked a position.
                        return Err(Error::not_supported(
                            "The 'q' instruction does not accept any arguments".to_string(),
                        )
                        .with_position(&excess.position()));
                    }
                }
            }

            // Check if there's a filename to process separately
            let has_filename = query.filename().is_some();
            let filename = query.filename();

            let query_without_q = self.strip_q_instruction(query);

            // Strip filename from query_without_q if present
            let query_without_q_and_filename = if has_filename {
                query_without_q.without_filename()
            } else {
                query_without_q
            };

            if !query_without_q_and_filename.is_empty() {
                self.plan
                    .steps
                    .push(Step::UseQueryValue(query_without_q_and_filename));
            }

            // Add filename as separate step if present
            if let Some(filename) = filename {
                self.plan.steps.push(Step::Filename(filename));
            }

            return Ok(());
        }

        if let Some(rq) = query.resource_query() {
            //eprintln!("RESOURCE {}", rq);
            self.process_resource_query(&rq)?;
            return Ok(());
        }
        if let Some(transform) = query.transform_query() {
            //eprintln!("TRANSFORM {}", &transform);
            if let Some(action) = transform.action() {
                let mut query = query.clone();
                query.segments = Vec::new();
                self.process_action(&query, &action)?;
                return Ok(());
            }
            if transform.is_filename() {
                //eprintln!("FILENAME {}", &transform);
                self.plan
                    .steps
                    .push(Step::Filename(transform.filename.unwrap().clone()));
                return Ok(());
            }
        }

        let (p, q) = query.predecessor();
        //eprintln!("PREDECESOR: {:?}", &p);
        //eprintln!("REMAINDER:  {:?}", &q);

        // Whether the split produced an action to run after the predecessor, as opposed to a
        // trailing filename, which is a naming instruction rather than a step in the chain.
        let remainder_is_action = match &q {
            Some(QuerySegment::Resource(_)) => true,
            Some(QuerySegment::Transform(tqs)) => tqs.action().is_some(),
            None => false,
        };

        if let Some(p) = p.as_ref() {
            if !p.is_empty() {
                // Check if predecessor ends with "q" instruction
                if p.is_q() {
                    // Strip the "q" instruction and create Step::UseQueryValue
                    let query_without_q = self.strip_q_instruction(p);
                    if !query_without_q.is_empty() {
                        self.plan.steps.push(Step::UseQueryValue(query_without_q));
                    }
                } else {
                    // The builder always expands. Cutting a boundary is a policy decision made
                    // after freezing, when the steps are in execution order and every operand is
                    // absolute; recording the sub-query here is all that pass needs.
                    //
                    // Record only when the remainder is a real action. `Query::predecessor` splits
                    // a trailing *filename* off as the remainder too, and this assignment runs at
                    // every level of the recursion, so recording there would let the outermost
                    // level overwrite the inner one with the whole action chain — cutting would
                    // then swallow the last action, leaving a recipe's overrides nothing to patch.
                    let recorded = promote_relative_default_links(p, self.command_registry)?;
                    self.process_query(p)?;
                    if remainder_is_action {
                        self.plan.predecessor = Some(recorded);
                        self.plan.predecessor_steps = self.plan.steps.len();
                    }
                }
            }
        }
        if let Some(qs) = q {
            match qs {
                QuerySegment::Resource(ref rqs) => {
                    self.process_resource_query(rqs)?;
                    return Ok(());
                }
                QuerySegment::Transform(ref tqs) => {
                    if tqs.is_empty() || tqs.is_ns() || tqs.is_q() {
                        return Ok(());
                    }
                    if let Some(action) = tqs.action() {
                        self.process_action(query, &action)?;
                        return Ok(());
                    }
                    if tqs.is_filename() {
                        self.plan
                            .steps
                            .push(Step::Filename(tqs.filename.as_ref().unwrap().clone()));
                        return Ok(());
                    }
                    return Err(Error::not_supported(format!(
                        "Unexpected query segment '{}'",
                        qs.encode()
                    )));
                }
            }
        }
        Ok(())
    }

    /// Applies a value override to the last action built so far.
    ///
    /// Returns `false` when the last action has no matching overridable parameter.
    pub fn override_value(&mut self, name: &str, value: Value) -> bool {
        self.plan.override_value(name, value)
    }

    /// Applies a query-link override to the last action built so far.
    ///
    /// Returns `false` when the last action has no matching overridable parameter.
    pub fn override_link(&mut self, name: &str, query: Query) -> bool {
        self.plan.override_link(name, query)
    }
}

/// Resolved interpreter operations and planning metadata for one source query.
///
/// A plan is a data model, not an evaluated result. [`PlanBuilder`] creates the synchronous
/// first pass; environment-backed dependency analysis may subsequently update volatility,
/// expiration, diagnostics, and dependencies before the interpreter executes [`Self::steps`].
///
/// Serde derives expose the current internal representation. Except for fields explicitly marked
/// with `serde(default)`, this module does not define a stable cross-version wire format.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Plan {
    /// Query from which this plan was built.
    pub query: Query,

    /// Diagnostics produced during planning and analysis, before execution.
    ///
    /// This should contain only [`Step::Info`], [`Step::Warning`], and [`Step::Error`]. The
    /// interpreter does not execute this list; metadata projection copies it into the asset log.
    #[serde(default)]
    pub init_steps: Vec<Step>,

    /// Ordered operations and executable diagnostics interpreted at runtime.
    pub steps: Vec<Step>,

    /// Whether the plan produces a result that must not be reused as a stable asset.
    ///
    /// Command and link volatility is computed during building; dependency volatility is added
    /// during finalization. This required serialized field intentionally has no Serde default.
    pub is_volatile: bool,

    /// Whether this plan needs an evaluation payload to run.
    ///
    /// It is computed from command metadata and linked subplans. A required payload also implies
    /// volatile evaluation. This field has a Serde default so older plans deserialize as
    /// [`PayloadRequirement::None`].
    #[serde(default)]
    pub payload_required: PayloadRequirement,

    /// Expiration specification inferred from command metadata during plan building.
    /// This is a first-pass estimate; authoritative expiration is computed at finalization.
    #[serde(default)]
    pub expires: Expires,

    /// Error discovered during plan creation/analysis (e.g. cyclic dependency).
    #[serde(default)]
    pub error: Option<Error>,

    /// Dependencies discovered during plan analysis.
    #[serde(default)]
    pub dependencies: Vec<PlanDependency>,

    /// CWD every operand in this plan was resolved against, or `None` while still source-relative.
    ///
    /// Set exactly once by [`Self::freeze_cwd`]. A frozen plan is never re-frozen under a different
    /// CWD; callers rebuild from the source [`Query`] or `Recipe`, which is the contract
    /// `finalize_plan` already documents.
    #[serde(default)]
    pub frozen_cwd: Option<Key>,

    /// Predecessor sub-query the builder descended into, with relative default links promoted to
    /// explicit query links so the query is self-contained.
    ///
    /// `None` when the query has no predecessor. [`Self::cut_predecessor`] turns this into a
    /// [`Step::Evaluate`] boundary.
    #[serde(default)]
    pub predecessor: Option<Query>,

    /// Number of leading [`Self::steps`] emitted for [`Self::predecessor`].
    #[serde(default)]
    pub predecessor_steps: usize,

    /// Where this plan's volatility came from, when it is volatile.
    ///
    /// [`VolatilitySource::Declared`] is a statement about the whole plan and appears in no
    /// candidate boundary's query, so nothing downstream could recover it. It is what
    /// [`Self::cut_predecessor`] consults before looking for a boundary at all.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volatility_source: Option<VolatilitySource>,

    /// Number of leading [`Self::steps`] that were *not* emitted by the builder for
    /// [`Self::query`] — a recipe's CWD prefix.
    ///
    /// [`crate::recipes::Recipe::to_plan`] inserts a [`Step::SetCwd`] at index 0 after building,
    /// which shifts every step the builder emitted. Recording how many such steps there are makes
    /// the prefix a fact rather than something each consumer infers for itself, and lets an index
    /// taken against the query's own steps survive the insert.
    #[serde(default)]
    pub prologue_steps: usize,
}

/// How a plan came to be volatile.
///
/// The distinction decides where an evaluation boundary may be placed, so it is data rather
/// than a diagnostic. A closed set: a new source is a compile error at every match.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VolatilitySource {
    /// A volatile command, or a volatile dependency.
    ///
    /// **Positional**: volatility is a property of that command, so everything ahead of it in
    /// the chain is genuinely pure and a boundary may be cut in front of it.
    Positional,
    /// A whole-plan declaration — the `v` instruction, a recipe's `volatile: true`, or a recipe
    /// expiration that is itself volatile.
    ///
    /// **Not positional**: it carries no position and says that nothing here is cacheable, so
    /// the plan may not be cut at all. A boundary is a cache entry.
    Declared,
}

impl VolatilitySource {
    /// `Declared` outranks `Positional`: a whole-plan declaration is never weakened by a
    /// command-level one arriving later.
    fn is_stronger_than(self, other: VolatilitySource) -> bool {
        match (self, other) {
            (VolatilitySource::Declared, VolatilitySource::Positional) => true,
            (VolatilitySource::Declared, VolatilitySource::Declared) => false,
            (VolatilitySource::Positional, VolatilitySource::Positional) => false,
            (VolatilitySource::Positional, VolatilitySource::Declared) => false,
        }
    }
}

impl Default for Plan {
    fn default() -> Self {
        Self::new()
    }
}

impl Plan {
    /// Creates an empty, nonvolatile plan with no expiration or payload requirement.
    pub fn new() -> Self {
        Plan {
            query: Query::new(),
            init_steps: Vec::new(),
            steps: Vec::new(),
            is_volatile: false,
            payload_required: PayloadRequirement::None,
            expires: Expires::Never,
            error: None,
            dependencies: Vec::new(),
            frozen_cwd: None,
            predecessor: None,
            predecessor_steps: 0,
            volatility_source: None,
            prologue_steps: 0,
        }
    }

    /// Checks the invariants tying the coupled predecessor fields to [`Self::steps`].
    ///
    /// Returns an error rather than panicking: library code must not panic, and every caller
    /// here already has a `Result` to propagate into. Callers without one can wrap this in a
    /// `debug_assert!`.
    ///
    /// Three defects in this area have been of one shape — a plan mutated through a subset of
    /// the coupled fields, leaving an index pointing at the wrong step. This turns the next one
    /// into an error at its source rather than a wrong value two layers away.
    pub(crate) fn check_consistent(&self) -> Result<(), Error> {
        if self.prologue_steps > self.steps.len() {
            return Err(Error::general_error(format!(
                "Plan prologue of {} step(s) exceeds its {} step(s)",
                self.prologue_steps,
                self.steps.len()
            ))
            .with_query(&self.query));
        }
        if self.predecessor.is_some()
            && (self.predecessor_steps < self.prologue_steps
                || self.predecessor_steps > self.steps.len())
        {
            return Err(Error::general_error(format!(
                "Plan predecessor range {} is outside its prologue {} and {} step(s)",
                self.predecessor_steps,
                self.prologue_steps,
                self.steps.len()
            ))
            .with_query(&self.query));
        }
        Ok(())
    }

    /// Records where this plan's volatility came from, keeping the stronger source.
    ///
    /// Called for every contribution rather than only the first, because a
    /// [`VolatilitySource::Declared`] arriving after a [`VolatilitySource::Positional`] one must
    /// still win — otherwise `vol_cmd/v/tail` would be recorded as positional and cut.
    pub(crate) fn upgrade_volatility_source(&mut self, source: VolatilitySource) {
        match self.volatility_source {
            Some(current) if !source.is_stronger_than(current) => {}
            Some(_) | None => self.volatility_source = Some(source),
        }
    }

    /// Resolves every CWD-relative operand in this plan against `entry`, in execution order.
    ///
    /// After this returns, the plan is self-contained: no step, link parameter or nested plan
    /// depends on a working key any more, so dependency analysis, pre-scheduling and execution all
    /// observe the same absolute operands instead of each re-deriving them with its own cursor.
    ///
    /// `entry` is optional because a plan may be finalized with no CWD installed. Resolution then
    /// falls back to logical root, and the returned flag reports whether that fallback was actually
    /// *used* — a plan with no relative operand does not touch it. Callers own the warning, so that
    /// a plan needing no CWD stays silent.
    ///
    /// Idempotent: freezing an already-frozen plan against the same key is a no-op, because
    /// `CwdCursor::resolve_key` returns a non-relative key unchanged. Returns the CWD in effect
    /// after the last step together with that fallback flag.
    ///
    /// Errors when the plan is already frozen against a *different* key, which means a caller
    /// reused a finalized plan under another CWD — the case `finalize_plan` already forbids.
    pub fn freeze_cwd(&mut self, entry: Option<Key>) -> Result<(Key, bool), Error> {
        let requested = entry.clone().unwrap_or_else(Key::new);
        if let Some(frozen) = &self.frozen_cwd {
            if frozen == &requested {
                return Ok((frozen.clone(), false));
            }
            return Err(Error::general_error(format!(
                "Plan is already frozen against CWD '{}' and cannot be re-frozen against '{}'; \
                 rebuild it from its source query or recipe instead",
                frozen.encode(),
                requested.encode()
            ))
            .with_query(&self.query));
        }

        let mut cursor = CwdCursor::new(entry);
        self.freeze_cwd_with(&mut cursor)?;
        let defaulted_to_root = cursor.take_root_fallback();
        self.frozen_cwd = Some(requested);
        Ok((cursor.current().unwrap_or_else(Key::new), defaulted_to_root))
    }

    /// Continues an enclosing freeze walk, sharing the caller's cursor.
    ///
    /// A nested [`Step::Plan`] shares the cursor rather than cloning it, so its final working key
    /// affects later outer steps — the behaviour `find_dependencies_nested_plan_propagates_cwd`
    /// pins. Link parameters clone instead; see [`ResolvedParameterValues::freeze_cwd`].
    pub(crate) fn freeze_cwd_with(&mut self, cursor: &mut CwdCursor) -> Result<(), Error> {
        // Read before any rewriting: the index is derived by matching the source query's resource
        // segments against steps, which only holds while the operands are still source-relative.
        let absolute_resource_step = self.absolute_query_resource_step_index();
        let mut root_cursor = CwdCursor::new(Some(Key::new()));

        // The predecessor is the leading steps, so it resolves from the entry state of this
        // walk — *after* any prologue. A recipe's `SetCwd` prefix is not part of `query`, so the
        // cursor has to be advanced over it first: leaving it out freezes the boundary query one
        // CWD short, and every relative operand inside it silently loses its folder.
        if let Some(predecessor) = &mut self.predecessor {
            let mut scoped = cursor.clone();
            for step in self.steps.iter().take(self.prologue_steps) {
                if let Step::SetCwd(key) = step {
                    scoped.set_cwd_from(key);
                }
            }
            *predecessor = scoped.resolve_query_scoped(predecessor);
        }

        for (index, step) in self.steps.iter_mut().enumerate() {
            let at_absolute_resource = absolute_resource_step == Some(index);
            let step_cursor = if at_absolute_resource {
                &mut root_cursor
            } else {
                &mut *cursor
            };
            match step {
                Step::GetAsset(key)
                | Step::GetAssetBinary(key)
                | Step::GetAssetMetadata(key)
                | Step::GetAssetRecipe(key)
                | Step::GetAssetDirectory(key)
                | Step::GetResource(key)
                | Step::GetResourceMetadata(key)
                | Step::GetResourceDirectory(key)
                | Step::UseKeyValue(key) => {
                    *key = step_cursor.resolve_key(key);
                }
                Step::SetCwd(key) => {
                    // Advances the working key *and* rewrites the operand, so the step remains as
                    // provenance while nothing downstream depends on executing it.
                    let resolved = step_cursor.set_cwd_from(key);
                    if at_absolute_resource {
                        cursor.set_cwd_from(&resolved);
                    }
                    *key = resolved;
                }
                Step::Evaluate(query) | Step::UseQueryValue(query) => {
                    *query = step_cursor.resolve_query_scoped(query);
                }
                Step::Action { parameters, .. } => {
                    parameters.freeze_cwd(step_cursor);
                }
                Step::Plan(nested) => {
                    nested.freeze_cwd_with(step_cursor)?;
                }
                Step::Filename(_) | Step::Info(_) | Step::Warning(_) | Step::Error(_) => {}
            }
        }
        Ok(())
    }

    /// Cuts a predecessor boundary at the last candidate prefix that can be **cached**.
    ///
    /// A boundary is a cache entry: the prefix becomes an asset in its own right, so it can be
    /// shared between consumers, expire on its own schedule and be scheduled alongside its
    /// siblings. Two things make a candidate uncacheable, and the walk steps back past both:
    ///
    /// - its plan **requires a payload** — a payload is deliberately not part of a cache key,
    ///   so a value computed from one must never end up behind a boundary;
    /// - its plan is **volatile** — a boundary that is recomputed every time buys none of the
    ///   three things a boundary exists for, and costs an extra asset and an extra hop.
    ///
    /// Whole-plan volatility ([`VolatilitySource::Declared`] — the `v` instruction, a recipe's
    /// `volatile: true`) is checked first and declines outright: it says nothing here is
    /// cacheable, and it appears in no candidate's query, so the walk could not see it.
    ///
    /// Requires a frozen plan — cutting an unfrozen one would produce a CWD-dependent boundary
    /// query, which is the defect freezing exists to remove. Each candidate found by stepping
    /// back is built fresh from source and so is frozen here in turn, against the working key
    /// the plan's own steps begin under.
    ///
    /// Volatility, payload requirement, expiration and dependencies are deliberately **not**
    /// recomputed: they were computed over the fully expanded plan, which is why the cut happens
    /// here rather than during building. Every level passed over, and the decline, appends a
    /// planning [`Step::Info`], so a declined cut is distinguishable from a plan that had no
    /// predecessor.
    ///
    /// Returns `false` when no boundary was cut.
    pub fn cut_predecessor(&mut self, cmr: &CommandMetadataRegistry) -> Result<bool, Error> {
        if self.frozen_cwd.is_none() {
            return Err(Error::general_error(
                "Plan must be frozen before its predecessor can be cut".to_string(),
            )
            .with_query(&self.query));
        }
        if self.volatility_source == Some(VolatilitySource::Declared) {
            self.init_info(
                "Predecessor boundary not cut: the plan is declared volatile, so none of it \
                 may be cached"
                    .to_string(),
            );
            return Ok(false);
        }
        let Some(recorded) = self.predecessor.clone() else {
            return Ok(false);
        };
        // `>=` rather than `>`: equality leaves an empty tail, which is the whole plan replaced
        // by a boundary that recomputes it. Unreachable while `v` declines above, and pinned
        // anyway because a positional `v` would reopen it.
        if self.predecessor_steps == 0 || self.predecessor_steps >= self.steps.len() {
            return Ok(false);
        }

        // The working key the query's own steps begin under. Built once; each candidate is
        // frozen against a fresh clone, because freezing advances a cursor.
        let mut base = CwdCursor::new(self.frozen_cwd.clone());
        for step in self.steps.iter().take(self.prologue_steps) {
            if let Step::SetCwd(key) = step {
                base.set_cwd_from(key);
            }
        }

        // Walk back while the candidate cannot be cached. `boundary` and `cut_at` are owned
        // rather than borrowed from `self`, because `init_info` needs `&mut self`.
        let mut boundary = recorded;
        let mut cut_at = self.predecessor_steps;
        loop {
            let mut candidate = PlanBuilder::new(boundary.clone(), cmr)
                .with_placeholders_allowed()
                .build()?;
            if candidate.steps.len() != cut_at.saturating_sub(self.prologue_steps) {
                // A recorded range that no longer matches what the query builds would split in
                // the wrong place; decline rather than risk running an action twice.
                return Ok(false);
            }
            let reason = if candidate.payload_required.is_required() {
                "requires an evaluation payload"
            } else if candidate.is_volatile {
                "is volatile"
            } else {
                break;
            };
            self.init_info(format!(
                "Predecessor boundary expanded at '{}': it {}",
                boundary.encode(),
                reason
            ));
            // Built fresh from source, so its own predecessor is still CWD-relative.
            let mut scoped = base.clone();
            candidate.freeze_cwd_with(&mut scoped)?;
            let Some(inner) = candidate.predecessor.clone() else {
                return Ok(false);
            };
            cut_at = self.prologue_steps + candidate.predecessor_steps;
            if cut_at == 0 {
                return Ok(false);
            }
            boundary = inner;
        }

        let tail = self.steps.split_off(cut_at);
        let mut head: Vec<Step> = self
            .steps
            .drain(..)
            .filter(|step| matches!(step, Step::SetCwd(_)))
            .collect();
        head.push(Step::Evaluate(boundary));
        self.predecessor_steps = head.len();
        self.prologue_steps = self.prologue_steps.min(head.len());
        head.extend(tail);
        self.steps = head;
        self.check_consistent()?;
        Ok(true)
    }

    /// Locates the first executable step produced by an absolute query's own resource segments.
    ///
    /// Recipe conversion may prepend a `SetCwd` that is not part of `query`. Matching all source
    /// resource segments from the end keeps that prefix distinct even when it is identical to a
    /// query-authored `cwd` instruction. The returned index is runtime provenance only: callers
    /// resolve a consumed copy and leave this raw, serializable plan unchanged.
    pub(crate) fn absolute_query_resource_step_index(&self) -> Option<usize> {
        if !self.query.absolute {
            return None;
        }
        let resources: Vec<&ResourceQuerySegment> = self
            .query
            .segments
            .iter()
            .filter_map(|segment| match segment {
                QuerySegment::Resource(resource) => Some(resource),
                QuerySegment::Transform(_) => None,
            })
            .collect();
        if resources.is_empty() {
            return None;
        }

        let mut upper_bound = self.steps.len();
        let mut first_resource_step = None;
        for resource in resources.iter().rev() {
            let index = (0..upper_bound)
                .rev()
                .find(|index| resource_query_step_matches(resource, &self.steps[*index]))?;
            first_resource_step = Some(index);
            upper_bound = index;
        }
        first_resource_step
    }
    /// Returns whether there are no executable steps.
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }
    /// Appends an executable informational diagnostic.
    pub fn info(&mut self, message: String) {
        self.steps.push(Step::Info(message));
    }
    /// Appends an executable warning diagnostic.
    pub fn warning(&mut self, message: String) {
        self.steps.push(Step::Warning(message));
    }
    /// Appends an executable error-level diagnostic.
    ///
    /// This does not populate [`Self::error`] and does not by itself abort interpretation.
    pub fn error(&mut self, message: String) {
        self.steps.push(Step::Error(message));
    }
    /// Returns whether structured or diagnostic errors are present.
    pub fn has_error(&self) -> bool {
        self.error.is_some()
            || self.init_steps.iter().any(|x| x.is_error())
            || self.steps.iter().any(|x| x.is_error())
    }
    /// Returns whether planning or executable warning diagnostics are present.
    pub fn has_warning(&self) -> bool {
        self.init_steps.iter().any(|x| x.is_warning()) || self.steps.iter().any(|x| x.is_warning())
    }
    /// Returns the number of executable steps.
    pub fn len(&self) -> usize {
        self.steps.len()
    }

    /// Sets the preliminary or finalized volatility flag.
    pub fn set_volatile(&mut self, is_volatile: bool) {
        self.is_volatile = is_volatile;
    }

    /// Appends an informational planning diagnostic.
    pub fn init_info(&mut self, message: String) {
        self.init_steps.push(Step::Info(message));
    }

    /// Appends a warning planning diagnostic.
    pub fn init_warning(&mut self, message: String) {
        self.init_steps.push(Step::Warning(message));
    }

    /// Appends an error-level planning diagnostic without setting a structured error.
    pub fn init_error(&mut self, message: String) {
        self.init_steps.push(Step::Error(message));
    }

    /// Records the first structured planning error and always adds it to planning diagnostics.
    pub fn set_error(&mut self, error: Error) {
        if self.error.is_none() {
            self.error = Some(error.clone());
        }
        self.init_error(error.to_string());
    }

    /// Creates submitted asset metadata from plan fields, excluding executable steps.
    ///
    /// Planning diagnostics are copied to the metadata log. Plan dependencies are not copied:
    /// [`PlanDependency`] is analysis data, distinct from runtime dependency records.
    pub fn to_metadata_record(&self) -> MetadataRecord {
        let mut mr = MetadataRecord::new();
        mr.status = Status::Submitted;
        mr.query = self.query.clone();
        mr.is_volatile = self.is_volatile;
        mr.payload_required = self.payload_required;
        mr.expires = self.expires.clone();
        if let Some(error) = &self.error {
            mr.with_error(error.clone());
        }
        for step in &self.init_steps {
            match step {
                Step::Info(msg) => {
                    mr.info(msg);
                }
                Step::Warning(msg) => {
                    mr.warning(msg);
                }
                Step::Error(msg) => {
                    mr.error(msg);
                }
                Step::GetAsset(_)
                | Step::GetAssetBinary(_)
                | Step::GetAssetMetadata(_)
                | Step::GetAssetRecipe(_)
                | Step::GetAssetDirectory(_)
                | Step::GetResource(_)
                | Step::GetResourceMetadata(_)
                | Step::GetResourceDirectory(_)
                | Step::Evaluate(_)
                | Step::UseQueryValue(_)
                | Step::Action { .. }
                | Step::Filename(_)
                | Step::Plan(_)
                | Step::SetCwd(_)
                | Step::UseKeyValue(_) => {}
            }
        }
        mr
    }

    /// Updates an existing metadata record from plan fields.
    ///
    /// Planning diagnostics are appended to the existing log rather than replacing it. Plan
    /// dependencies and executable steps are not copied.
    pub fn update_metadata_record(&self, mr: &mut MetadataRecord) {
        mr.query = self.query.clone();
        mr.is_volatile = self.is_volatile;
        mr.payload_required = self.payload_required;
        mr.expires = self.expires.clone();
        if let Some(error) = &self.error {
            mr.with_error(error.clone());
        }
        for step in &self.init_steps {
            match step {
                Step::Info(msg) => {
                    mr.info(msg);
                }
                Step::Warning(msg) => {
                    mr.warning(msg);
                }
                Step::Error(msg) => {
                    mr.error(msg);
                }
                Step::GetAsset(_)
                | Step::GetAssetBinary(_)
                | Step::GetAssetMetadata(_)
                | Step::GetAssetRecipe(_)
                | Step::GetAssetDirectory(_)
                | Step::GetResource(_)
                | Step::GetResourceMetadata(_)
                | Step::GetResourceDirectory(_)
                | Step::Evaluate(_)
                | Step::UseQueryValue(_)
                | Step::Action { .. }
                | Step::Filename(_)
                | Step::Plan(_)
                | Step::SetCwd(_)
                | Step::UseKeyValue(_) => {}
            }
        }
    }

    /// Find index of the last action in the plan
    fn last_action_index(&self) -> Option<usize> {
        for (i, s) in self.steps.iter().enumerate().rev() {
            if let Step::Action { .. } = s {
                return Some(i);
            }
        }
        None
    }

    /// Overrides the named parameter on the last action step with a JSON value.
    ///
    /// Returns `false` if the plan has no action or the last action has no matching overridable
    /// parameter. Earlier actions are deliberately not searched.
    pub fn override_value(&mut self, name: &str, value: Value) -> bool {
        if let Some(i) = self.last_action_index() {
            if let Step::Action { parameters, .. } = &mut self.steps[i] {
                return parameters.override_value(name, value);
            }
        }
        false
    }

    /// Overrides the named parameter on the last action step with a query link.
    ///
    /// Returns `false` if the plan has no action or the last action has no matching overridable
    /// parameter. Earlier actions are deliberately not searched.
    pub fn override_link(&mut self, name: &str, query: Query) -> bool {
        if let Some(i) = self.last_action_index() {
            if let Step::Action { parameters, .. } = &mut self.steps[i] {
                return parameters.override_link(name, query);
            }
        }
        false
    }

    /// Finds a split point whose suffix contains at most one action plus context modifiers.
    pub fn split_index(&self) -> usize {
        for i in (0..self.steps.len()).rev() {
            if self[i].is_action() {
                if i == 0 {
                    return 0;
                } else {
                    for ii in (0..=i - 1).rev() {
                        if !self[ii].is_context_modifier() {
                            return ii + 1;
                        }
                    }
                    return 0;
                }
            }
            if self[i].is_context_modifier() {
                continue;
            }
            return i + 1;
        }
        0
    }

    /// Splits the plan into a state-producing prefix and a final-action suffix.
    ///
    /// Both halves retain query-level analysis fields and planning diagnostics. The first plan's
    /// result is intended to become the state argument of the second plan. The suffix contains at
    /// most one action plus context modifiers.
    pub fn split(&self) -> (Plan, Plan) {
        if self.is_empty() {
            return (Plan::new(), Plan::new());
        }

        let split_index = self.split_index();
        if split_index == 0 {
            return (Plan::new(), self.clone());
        }
        // Built by cloning and replacing only what differs, rather than by copying a field
        // list: a field added to `Plan` later is then carried by construction, and one that
        // must *not* be carried has to be cleared deliberately, where a reviewer sees it.
        let mut first_plan = self.clone();
        first_plan.steps = self.steps[..split_index].to_vec();
        first_plan.prologue_steps = self.prologue_steps.min(first_plan.steps.len());
        // A boundary is a property of a whole plan, and a half is a fragment. The first half is
        // in fact exactly the predecessor's steps, so carrying the recorded predecessor would
        // give it `predecessor_steps == steps.len()` — a cut replacing every step with a
        // boundary that recomputes the same thing. Its real predecessor is one level deeper,
        // and `split` has no registry to build it.
        first_plan.predecessor = None;
        first_plan.predecessor_steps = 0;

        let mut second_plan = self.clone();
        second_plan.steps = self.steps[split_index..].to_vec();
        second_plan.prologue_steps = 0;
        second_plan.predecessor = None;
        second_plan.predecessor_steps = 0;

        // `frozen_cwd` and `volatility_source` are facts about the operands and the plan's
        // volatility, true of each half independently, so both are carried by the clone.
        (first_plan, second_plan)
    }
}

impl Index<usize> for Plan {
    type Output = Step;
    fn index(&self, index: usize) -> &Self::Output {
        &self.steps[index]
    }
}

// Plan-to-metadata helpers on Metadata (kept here to avoid metadata.rs → plan.rs import)
impl Metadata {
    /// Create a `Metadata::MetadataRecord` from a `Plan`.
    pub fn from_plan(plan: &Plan) -> Self {
        Metadata::MetadataRecord(plan.to_metadata_record())
    }

    /// Update an existing `Metadata` from a `Plan`.
    /// For `LegacyMetadata`, replaces with a fresh record derived from the plan.
    pub fn update_from_plan(&mut self, plan: &Plan) {
        match self {
            Metadata::MetadataRecord(mr) => plan.update_metadata_record(mr),
            Metadata::LegacyMetadata(_) => {
                *self = Metadata::from_plan(plan);
            }
        }
    }
}

// DependencyKey, Version, DependencyRecord are defined in crate::metadata.
// DependencyRelation, PlanDependency are defined in crate::dependencies.

/// Helper function: Find all asset dependencies of a plan (direct and indirect)
/// Returns Error with specific key if circular dependency detected
///
/// # Dependency Semantics
///
/// - **UseKeyValue**: Does NOT create dependency. Creates a value with the key,
///   but does not fetch the resource. No attempt to get the resource is made.
///
/// - **GetAssetRecipe**: Does NOT create circular dependency risk. Asset recipe
///   is associated with the key, but it's a separate resource. In a dependency
///   tree it is a leaf. Recipe does not have further dependencies.
///
/// - **GetResource**: Ambiguous. Fetches data directly from the store, bypassing
///   dependency controls. Treated as no dependency for now, but flagged here.
///   Using the store rather than assets bypasses the asset dependency system.
///
/// - **SetCwd**: Does NOT create dependency on its own, but impacts relative links.
///   Requires complex evaluation: when a key/query is examined for circular dependency,
///   must find valid Cwd (previous SetCwd step in the plan), expand the query to
///   absolute form, and then assess the expanded query/key.
///
fn collect_parameter_dependencies(
    parameter: &ParameterValue,
    cursor: &mut CwdCursor,
    dependencies: &mut HashSet<PlanDependency>,
) {
    match parameter {
        ParameterValue::DefaultLink(name, query) => {
            let resolved = cursor.resolve_query_scoped(query);
            dependencies.insert(PlanDependency::new(
                DependencyKey::from(&resolved),
                DependencyRelation::DefaultLink(name.clone()),
            ));
        }
        ParameterValue::ParameterLink(name, query, _) => {
            let resolved = cursor.resolve_query_scoped(query);
            dependencies.insert(PlanDependency::new(
                DependencyKey::from(&resolved),
                DependencyRelation::ParameterLink(name.clone()),
            ));
        }
        ParameterValue::OverrideLink(name, query) => {
            let resolved = cursor.resolve_query_scoped(query);
            dependencies.insert(PlanDependency::new(
                DependencyKey::from(&resolved),
                DependencyRelation::OverrideLink(name.clone()),
            ));
        }
        ParameterValue::EnumLink(name, query, _) => {
            let resolved = cursor.resolve_query_scoped(query);
            dependencies.insert(PlanDependency::new(
                DependencyKey::from(&resolved),
                DependencyRelation::EnumLink(name.clone()),
            ));
        }
        ParameterValue::MultipleParameters(_, values) => {
            for value in values {
                collect_parameter_dependencies(value, cursor, dependencies);
            }
        }
        ParameterValue::DefaultValue(_, _)
        | ParameterValue::ParameterValue(_, _, _)
        | ParameterValue::OverrideValue(_, _)
        | ParameterValue::Placeholder(_)
        | ParameterValue::Injected(_)
        | ParameterValue::None => {}
    }
}

/// # Parameters
/// - `cursor`: Ordered current working key for resolving dependency operands
pub(crate) fn find_dependencies<'a, E: Environment>(
    envref: EnvRef<E>,
    plan: &'a Plan,
    stack: &'a mut Vec<Key>,
    cursor: &'a mut CwdCursor,
) -> crate::maybe_send::BoxFuture<'a, Result<Vec<PlanDependency>, Error>> {
    Box::pin(async move {
        let mut dependencies = HashSet::new();
        let absolute_resource_step = plan.absolute_query_resource_step_index();

        for (step_index, step) in plan.steps.iter().enumerate() {
            match step {
                Step::GetAsset(key) | Step::GetAssetBinary(key) | Step::GetAssetMetadata(key) => {
                    let resolved_key = if absolute_resource_step == Some(step_index) {
                        key.to_absolute(&Key::new())
                    } else {
                        cursor.resolve_key(key)
                    };

                    // Check for circular dependency
                    if stack.contains(&resolved_key) {
                        return Err(Error::general_error(format!(
                            "Circular dependency detected: key {:?} appears in dependency chain",
                            resolved_key
                        ))
                        .with_key(&resolved_key));
                    }

                    // Add direct dependency
                    dependencies.insert(PlanDependency::new(
                        DependencyKey::from(&resolved_key),
                        DependencyRelation::StateArgument,
                    ));

                    stack.push(resolved_key.clone());
                    if let Ok(Some(recipe)) = envref
                        .get_recipe_provider()
                        .recipe_opt(&resolved_key, envref.clone())
                        .await
                    {
                        dependencies.insert(PlanDependency::new(
                            DependencyKey::from_recipe_key(&resolved_key),
                            DependencyRelation::Recipe,
                        ));
                        let cmr = envref.get_command_metadata_registry();
                        let recipe_plan = recipe.to_plan_for_key(cmr, &resolved_key)?;
                        let mut recipe_cursor = cursor.clone();
                        let nested_dependencies = find_dependencies(
                            envref.clone(),
                            &recipe_plan,
                            stack,
                            &mut recipe_cursor,
                        )
                        .await?;
                        dependencies.extend(nested_dependencies);
                    }
                    stack.pop();
                }
                Step::GetAssetDirectory(key) => {
                    let resolved_key = if absolute_resource_step == Some(step_index) {
                        key.to_absolute(&Key::new())
                    } else {
                        cursor.resolve_key(key)
                    };

                    if stack.contains(&resolved_key) {
                        return Err(Error::general_error(format!(
                            "Circular dependency detected: key {:?} appears in dependency chain",
                            resolved_key
                        ))
                        .with_key(&resolved_key));
                    }

                    dependencies.insert(PlanDependency::new(
                        DependencyKey::from_dir_key(&resolved_key),
                        DependencyRelation::StateArgument,
                    ));
                }
                Step::GetAssetRecipe(key) => {
                    let resolved_key = if absolute_resource_step == Some(step_index) {
                        key.to_absolute(&Key::new())
                    } else {
                        cursor.resolve_key(key)
                    };

                    if stack.contains(&resolved_key) {
                        return Err(Error::general_error(format!(
                            "Circular dependency detected: key {:?} appears in dependency chain",
                            resolved_key
                        ))
                        .with_key(&resolved_key));
                    }

                    dependencies.insert(PlanDependency::new(
                        DependencyKey::from_recipe_key(&resolved_key),
                        DependencyRelation::Recipe,
                    ));
                }
                Step::SetCwd(key) => {
                    if absolute_resource_step == Some(step_index) {
                        let resolved = key.to_absolute(&Key::new());
                        cursor.set_cwd_from(&resolved);
                    } else {
                        cursor.set_cwd_from(key);
                    }
                }
                Step::Evaluate(query) => {
                    let resolved_query = cursor.resolve_query_scoped(query);
                    let cmr = envref.get_command_metadata_registry();
                    let eval_plan = PlanBuilder::new(resolved_query, cmr).build()?;
                    let mut child_cursor = cursor.clone();
                    let child_dependencies =
                        find_dependencies(envref.clone(), &eval_plan, stack, &mut child_cursor)
                            .await?;
                    for dependency in child_dependencies {
                        if Key::try_from(&dependency.key).is_ok() {
                            dependencies.insert(PlanDependency::new(
                                dependency.key,
                                DependencyRelation::StateArgument,
                            ));
                        } else {
                            dependencies.insert(dependency);
                        }
                    }
                }
                Step::Plan(nested_plan) => {
                    dependencies.extend(
                        find_dependencies(envref.clone(), nested_plan, stack, cursor).await?,
                    );
                }
                Step::Action {
                    realm,
                    ns,
                    action_name,
                    parameters,
                    ..
                } => {
                    // Add command metadata and implementation dependencies
                    let ck = CommandKey::new(realm, ns, action_name);
                    dependencies.insert(PlanDependency::new(
                        DependencyKey::for_command_metadata(&ck),
                        DependencyRelation::CommandMetadata,
                    ));
                    dependencies.insert(PlanDependency::new(
                        DependencyKey::for_command_implementation(&ck),
                        DependencyRelation::CommandImplementation,
                    ));

                    for parameter in &parameters.0 {
                        collect_parameter_dependencies(parameter, cursor, &mut dependencies);
                    }
                }
                Step::GetResource(_)
                | Step::GetResourceMetadata(_)
                | Step::GetResourceDirectory(_)
                | Step::UseKeyValue(_)
                | Step::UseQueryValue(_)
                | Step::Filename(_)
                | Step::Info(_)
                | Step::Warning(_)
                | Step::Error(_) => {}
            }
        }

        let mut dependencies: Vec<PlanDependency> = dependencies.into_iter().collect();
        dependencies.sort_by(|a, b| {
            a.key
                .as_str()
                .cmp(b.key.as_str())
                .then_with(|| format!("{:?}", a.relation).cmp(&format!("{:?}", b.relation)))
        });
        Ok(dependencies)
    })
}

fn dependency_check_error(plan: &mut Plan, error: &Error) {
    plan.set_error(error.clone());
}

/// Check if plan has volatile dependencies (Phase 2 check)
/// Returns true if any dependency recipe is volatile
pub(crate) async fn has_volatile_dependencies<E: Environment>(
    envref: EnvRef<E>,
    plan: &mut Plan,
    initial_cwd: Option<Key>,
) -> Result<bool, Error> {
    let mut stack = Vec::new();
    let mut cursor = CwdCursor::new(initial_cwd);
    let dependencies = match find_dependencies(envref.clone(), plan, &mut stack, &mut cursor).await
    {
        Ok(dependencies) => dependencies,
        Err(error) => {
            dependency_check_error(plan, &error);
            return Err(error);
        }
    };
    plan.dependencies = dependencies.clone();
    for dependency in &dependencies {
        plan.init_info(format!(
            "Dependency detected: {} ({:?})",
            dependency.key, dependency.relation
        ));
    }
    if cursor.take_root_fallback() {
        plan.init_warning(RELATIVE_WITHOUT_CWD_WARNING.to_owned());
    }

    if plan.is_volatile {
        return Ok(true);
    }

    // Check each dependency key for volatility
    for dependency in dependencies {
        let Ok(key) = Key::try_from(&dependency.key) else {
            continue;
        };
        if let Ok(Some(recipe)) = envref
            .get_recipe_provider()
            .recipe_opt(&key, envref.clone())
            .await
        {
            if recipe.volatile {
                plan.is_volatile = true;
                plan.init_info(format!(
                    "Volatile due to dependency on volatile key: {:?}",
                    key
                ));
                return Ok(true);
            }
        }
    }

    Ok(false)
}

/// Check if any dependencies have expiration specified in their recipes.
/// Updates the plan's expires field by combining expirations across known dependencies.
/// This is a first-pass estimate at plan-build time. The authoritative computation
/// happens at asset finalization when all dependencies are evaluated and known.
pub(crate) async fn has_expirable_dependencies<E: Environment>(
    envref: EnvRef<E>,
    plan: &mut Plan,
) -> Result<(), Error> {
    let mut visited_recipe_keys = HashSet::new();
    has_expirable_dependencies_impl(envref, plan, &mut visited_recipe_keys).await
}

fn has_expirable_dependencies_impl<'a, E: Environment>(
    envref: EnvRef<E>,
    plan: &'a mut Plan,
    visited_recipe_keys: &'a mut HashSet<Key>,
) -> crate::maybe_send::BoxFuture<'a, Result<(), Error>> {
    Box::pin(async move {
        let dependencies = plan.dependencies.clone();
        let mut changed = false;

        for dependency in dependencies {
            let key = dependency.key.key()?.or(dependency.key.recipe_key()?);
            let Some(key) = key else {
                continue;
            };
            if !visited_recipe_keys.insert(key.clone()) {
                continue;
            }

            if let Ok(Some(recipe)) = envref
                .get_recipe_provider()
                .recipe_opt(&key, envref.clone())
                .await
            {
                let mut dependency_expires = recipe.expires.clone();

                if !recipe.query.is_empty() {
                    let cmr = envref.get_command_metadata_registry();
                    let mut recipe_plan = recipe.to_plan_for_key(cmr, &key)?;
                    has_volatile_dependencies(envref.clone(), &mut recipe_plan, None).await?;
                    has_expirable_dependencies_impl(
                        envref.clone(),
                        &mut recipe_plan,
                        visited_recipe_keys,
                    )
                    .await?;
                    dependency_expires |= recipe_plan.expires.clone();
                }

                let previous = plan.expires.clone();
                plan.expires |= dependency_expires.clone();
                if plan.expires != previous {
                    plan.init_info(format!(
                        "Expiration combined with asset dependency {:?}: '{}' | '{}' -> '{}'",
                        key, previous, dependency_expires, plan.expires
                    ));
                    changed = true;
                }
            }
        }

        if changed && plan.expires.is_volatile() {
            if !plan.is_volatile {
                plan.is_volatile = true;
                plan.init_info(
                    "Volatile: dependency combination includes Immediately expiration".to_string(),
                );
            }
        }

        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    use crate::command_metadata::*;
    use crate::context::{EnvRef, Environment, ImmediateEnvironment};
    use crate::parse::parse_query;
    use crate::query::{QuerySource, TryToQuery};
    use crate::recipes::{AsyncRecipeProvider, Recipe};
    use async_trait::async_trait;
    use serde_yaml;

    use super::*;

    #[derive(Clone)]
    struct CountingRecipeProvider {
        recipes: Arc<HashMap<Key, Recipe>>,
        recipe_opt_calls: Arc<AtomicUsize>,
    }

    impl CountingRecipeProvider {
        fn new(recipes: impl IntoIterator<Item = (Key, Recipe)>) -> Self {
            Self {
                recipes: Arc::new(recipes.into_iter().collect()),
                recipe_opt_calls: Arc::new(AtomicUsize::new(0)),
            }
        }

        fn recipe_opt_calls(&self) -> usize {
            self.recipe_opt_calls.load(Ordering::SeqCst)
        }
    }

    #[cfg_attr(not(target_arch = "wasm32"), async_trait)]
    #[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
    impl<E: Environment> AsyncRecipeProvider<E> for CountingRecipeProvider {
        async fn has_recipes(&self, _key: &Key, _envref: EnvRef<E>) -> Result<bool, Error> {
            Ok(false)
        }

        async fn assets_with_recipes(
            &self,
            _key: &Key,
            _envref: EnvRef<E>,
        ) -> Result<Vec<ResourceName>, Error> {
            Ok(Vec::new())
        }

        async fn recipe_plan(&self, key: &Key, envref: EnvRef<E>) -> Result<Plan, Error> {
            let recipe = self
                .recipes
                .get(key)
                .cloned()
                .ok_or_else(|| Error::key_not_found(key))?;
            recipe.to_plan_for_key(envref.get_command_metadata_registry(), key)
        }

        async fn recipe(&self, key: &Key, _envref: EnvRef<E>) -> Result<Recipe, Error> {
            self.recipes
                .get(key)
                .cloned()
                .ok_or_else(|| Error::key_not_found(key))
        }

        async fn recipe_opt(&self, key: &Key, _envref: EnvRef<E>) -> Result<Option<Recipe>, Error> {
            self.recipe_opt_calls.fetch_add(1, Ordering::SeqCst);
            Ok(self.recipes.get(key).cloned())
        }
    }

    fn plan_dependencies_contain(
        dependencies: &[PlanDependency],
        key: &str,
        relation: &DependencyRelation,
    ) -> bool {
        dependencies
            .iter()
            .any(|dependency| dependency.key.as_str() == key && &dependency.relation == relation)
    }

    #[test]
    fn first_test() {
        let mut cr = command_metadata::CommandMetadataRegistry::new();
        cr.add_command(CommandMetadata::new("a").with_argument(ArgumentInfo::any_argument("a")));
        let plan = PlanBuilder::new(parse_query("a-1").unwrap(), &cr)
            .build()
            .unwrap();
        eprintln!("plan: {:?}", plan);
        print!("");
        eprintln!("plan.yaml:\n{}", serde_yaml::to_string(&plan).unwrap());
        print!("");
        eprintln!(
            "command_registry.yaml:\n{}",
            serde_yaml::to_string(&cr).unwrap()
        );
        print!("");
        eprintln!("plan.json:\n{}", serde_json::to_string(&plan).unwrap());
        print!("");
        eprintln!(
            "command_registry.json:\n{}",
            serde_json::to_string(&cr).unwrap()
        );
        print!("");
    }

    #[test]
    fn first_override() {
        let mut cr = command_metadata::CommandMetadataRegistry::new();
        cr.add_command(CommandMetadata::new("a").with_argument(ArgumentInfo::any_argument("b")));
        let mut plan = PlanBuilder::new(parse_query("a-1").unwrap(), &cr)
            .build()
            .unwrap();
        assert!(plan.override_value("b", Value::String("test".to_string())));
        assert!(!plan.override_value("c", Value::String("test".to_string())));
        eprintln!("plan: {:?}", plan);
        print!("");
        eprintln!("plan.yaml:\n{}", serde_yaml::to_string(&plan).unwrap());
        eprintln!("plan.json:\n{}", serde_json::to_string(&plan).unwrap());
        print!("");
        eprintln!(
            "command_registry.yaml:\n{}",
            serde_yaml::to_string(&cr).unwrap()
        );
        print!("");
        eprintln!("plan.json:\n{}", serde_json::to_string(&plan).unwrap());
        print!("");
        eprintln!(
            "command_registry.json:\n{}",
            serde_json::to_string(&cr).unwrap()
        );
        print!("");
    }

    #[test]
    fn handle_allow_placeholders() {
        let mut cr = command_metadata::CommandMetadataRegistry::new();
        cr.add_command(CommandMetadata::new("a").with_argument(ArgumentInfo::any_argument("b")));
        assert!(PlanBuilder::new(parse_query("a-1").unwrap(), &cr)
            .build()
            .is_ok());
        assert!(PlanBuilder::new(parse_query("a").unwrap(), &cr)
            .build()
            .is_err());
        assert!(PlanBuilder::new(parse_query("a").unwrap(), &cr)
            .with_placeholders_allowed()
            .build()
            .is_ok());
        let plan = PlanBuilder::new(parse_query("a").unwrap(), &cr)
            .with_placeholders_allowed()
            .build()
            .unwrap();
        eprintln!("plan.yaml:\n{}", serde_yaml::to_string(&plan).unwrap());
        assert!(plan.len() == 1);
        if let Step::Action {
            action_name,
            parameters,
            ..
        } = &plan[0]
        {
            assert!(action_name == "a");
            assert!(parameters.0.len() == 1);
            if let ParameterValue::Placeholder(name) = &parameters.0[0] {
                assert!(name == "b");
            } else {
                assert!(false);
            }
        } else {
            assert!(false);
        }
    }

    #[test]
    fn test_string_parameter_value() {
        let arginfo = ArgumentInfo::string_argument("test").with_default("default");
        let pv = ParameterValue::from_arginfo(&arginfo);
        assert_eq!(pv.value(), Some(Value::String("default".to_string())));
        let pv = ParameterValue::from_string(&arginfo, "testarg", &Position::unknown()).unwrap();
        assert_eq!(pv.value(), Some(Value::String("testarg".to_string())));
        let pv = ParameterValue::from_string(&arginfo, "", &Position::unknown()).unwrap();
        assert_eq!(pv.value(), Some(Value::String("".to_string())));
    }

    #[test]
    fn test_enum_parameter_fallback_integer_value() {
        let enum_arg = EnumArgument::new("quality")
            .with_int_value("low", 1)
            .with_int_value("high", 3)
            .with_others_allowed()
            .with_value_type(EnumArgumentType::Integer);
        let arginfo = ArgumentInfo::argument("quality")
            .with_type(ArgumentType::Enum(enum_arg))
            .with_default(2);
        let pv = ParameterValue::from_string(&arginfo, "42", &Position::unknown()).unwrap();
        assert_eq!(pv.value(), Some(Value::Number(42.into())));
    }

    #[test]
    fn test_enum_parameter_fallback_integer_value_error() {
        let enum_arg = EnumArgument::new("quality")
            .with_int_value("low", 1)
            .with_int_value("high", 3)
            .with_others_allowed()
            .with_value_type(EnumArgumentType::Integer);
        let arginfo = ArgumentInfo::argument("quality").with_type(ArgumentType::Enum(enum_arg));
        let error = ParameterValue::from_string(&arginfo, "x", &Position::unknown()).unwrap_err();
        assert!(
            error
                .message
                .contains("Expected integer value for enum fallback"),
            "{}",
            error.message
        );
    }

    #[test]
    fn test_enum_parameter_undefined_alias_error_lists_values() {
        let enum_arg = EnumArgument::new("mode")
            .with_alternative("nearest")
            .with_alternative("lanczos3");
        let arginfo = ArgumentInfo::argument("mode").with_type(ArgumentType::Enum(enum_arg));
        let error =
            ParameterValue::from_string(&arginfo, "unknown", &Position::unknown()).unwrap_err();
        assert!(
            error.message.contains("Valid values: nearest, lanczos3")
                || error.message.contains("Valid values: lanczos3, nearest"),
            "{}",
            error.message
        );
    }

    #[test]
    fn test_append_action_without_namespace_injection() {
        let mut cmr = CommandMetadataRegistry::new();
        let mut cmd = CommandMetadata::new("next");
        cmd.namespace = "".to_string();
        cmr.add_command(&cmd);

        let query = parse_query("-R-bin/data.txt").unwrap();
        let action = ActionRequest::new("next".to_string());
        let appended = append_action(&query, "root", action, &cmr).unwrap();
        assert_eq!(appended.encode(), "-R-bin/data.txt/next");
    }

    #[test]
    fn test_append_action_with_namespace_injection() {
        let mut cmr = CommandMetadataRegistry::new();
        let mut cmd = CommandMetadata::new("next");
        cmd.namespace = "lui".to_string();
        cmr.add_command(&cmd);

        let query = parse_query("-R-bin/data.txt").unwrap();
        let action = ActionRequest::new("next".to_string());
        let appended = append_action(&query, "lui", action, &cmr).unwrap();
        assert_eq!(appended.encode(), "-R-bin/data.txt/ns-lui/next");
    }

    #[test]
    fn test_append_action_clears_transform_filename() {
        let mut cmr = CommandMetadataRegistry::new();
        let mut cmd = CommandMetadata::new("next");
        cmd.namespace = "".to_string();
        cmr.add_command(&cmd);

        let query = Query {
            segments: vec![QuerySegment::Transform(
                crate::query::TransformQuerySegment {
                    header: None,
                    query: vec![ActionRequest::new("base".to_string())],
                    filename: Some(ResourceName::new("data.txt".to_string())),
                },
            )],
            absolute: false,
            source: crate::query::QuerySource::Unspecified,
        };

        let appended =
            append_action(&query, "root", ActionRequest::new("next".to_string()), &cmr).unwrap();
        if let Some(QuerySegment::Transform(tqs)) = appended.segments.last() {
            assert!(tqs.filename.is_none());
            assert_eq!(tqs.query.len(), 2);
            assert_eq!(tqs.query[0].name, "base");
            assert_eq!(tqs.query[1].name, "next");
        } else {
            panic!("Expected transform query segment");
        }
    }
    #[test]
    fn test_pop_parameter_value() -> Result<(), Error> {
        let arginfo = ArgumentInfo::string_argument("test").with_default("default");
        let action = parse_query("hello-testarg-123")?.action().unwrap();
        let mut param = ActionParameterIterator::new(&action);

        let pv = ParameterValue::pop_value(&arginfo, &mut param, false)?;
        assert_eq!(pv.value(), Some(Value::String("testarg".to_string())));

        let arginfo = ArgumentInfo::integer_argument("intarg", false);
        let pv = ParameterValue::pop_value(&arginfo, &mut param, false)?;
        assert_eq!(pv.value(), Some(Value::Number(123.into())));

        let arginfo = ArgumentInfo::integer_argument("intarg2", true);
        let pv = ParameterValue::pop_value(&arginfo, &mut param, false)?;
        assert_eq!(pv.value(), Some(Value::Null));

        let mut param = ActionParameterIterator::new(&action);
        let arginfo = ArgumentInfo::string_argument("test").set_multiple();
        let pv = ParameterValue::pop_value(&arginfo, &mut param, false)?;
        let pv = pv.multiple().unwrap();
        assert_eq!(pv.len(), 2);
        assert_eq!(pv[0].value(), Some(Value::String("testarg".to_string())));
        assert_eq!(pv[1].value(), Some(Value::String("123".to_string())));

        Ok(())
    }
    #[test]
    fn test_resolved_parameter_values() {
        let mut cm = CommandMetadata::new("testcommand");
        cm.with_argument(
            ArgumentInfo::string_argument("arg1")
                .with_default("zzz")
                .to_owned(),
        );
        cm.with_argument(
            ArgumentInfo::integer_argument("arg2", false)
                .with_default(123)
                .to_owned(),
        );
        let action = "testcommand-xxx-234"
            .try_to_query()
            .unwrap()
            .action()
            .unwrap();
        let rp = ResolvedParameterValues::from_action(&action, &cm, false).unwrap();
        assert_eq!(rp.0.len(), 2);
        assert_eq!(rp.0[0].value(), Some(Value::String("xxx".to_string())));
        assert_eq!(rp.0[1].value(), Some(Value::Number(234.into())));
        dbg!(rp);
        let action = "testcommand-yyy".try_to_query().unwrap().action().unwrap();
        let rp = ResolvedParameterValues::from_action(&action, &cm, false).unwrap();
        assert_eq!(rp.0.len(), 2);
        assert_eq!(rp.0[0].value(), Some(Value::String("yyy".to_string())));
        assert_eq!(rp.0[1].value(), Some(Value::Number(123.into())));
        dbg!(rp);
        let action = "testcommand".try_to_query().unwrap().action().unwrap();
        let rp = ResolvedParameterValues::from_action(&action, &cm, false).unwrap();
        assert_eq!(rp.0.len(), 2);
        assert_eq!(rp.0[0].value(), Some(Value::String("zzz".to_string())));
        assert_eq!(rp.0[1].value(), Some(Value::Number(123.into())));
        dbg!(rp);
    }

    #[test]
    fn test_plan_split_index() {
        use crate::plan::ResolvedParameterValues;
        use crate::plan::{Plan, Step};
        use crate::query::{Key, Position, ResourceName};

        // Plan with no actions: should return 0
        let plan = Plan {
            query: Default::default(),
            payload_required: PayloadRequirement::None,
            init_steps: vec![],
            steps: vec![
                Step::Info("info".to_string()),
                Step::Warning("warn".to_string()),
                Step::Error("err".to_string()),
            ],
            is_volatile: false,
            expires: crate::expiration::Expires::Never,
            error: None,
            dependencies: vec![],
            ..Plan::new()
        };
        assert_eq!(plan.split_index(), 0);
        let (p1, p2) = plan.split();
        assert!(p1.is_empty());
        assert_eq!(p2.len(), 3);

        // Plan with one action at the start
        let plan = Plan {
            query: Default::default(),
            payload_required: PayloadRequirement::None,
            init_steps: vec![],
            steps: vec![
                Step::Action {
                    realm: "r".to_string(),
                    ns: "n".to_string(),
                    action_name: "a".to_string(),
                    position: Position::unknown(),
                    parameters: ResolvedParameterValues::new(),
                },
                Step::Info("info".to_string()),
            ],
            is_volatile: false,
            expires: crate::expiration::Expires::Never,
            error: None,
            dependencies: vec![],
            ..Plan::new()
        };
        assert_eq!(plan.split_index(), 0);
        let (p1, p2) = plan.split();
        assert!(p1.is_empty());
        assert_eq!(p2.len(), 2);

        // Plan with context modifiers before and after an action
        let plan = Plan {
            query: Default::default(),
            payload_required: PayloadRequirement::None,
            init_steps: vec![],
            steps: vec![
                Step::Info("info".to_string()),
                Step::SetCwd(Key::new()),
                Step::Action {
                    realm: "r".to_string(),
                    ns: "n".to_string(),
                    action_name: "a".to_string(),
                    position: Position::unknown(),
                    parameters: ResolvedParameterValues::new(),
                },
                Step::Warning("warn".to_string()),
                Step::Filename(ResourceName::new("file.txt".to_string())),
            ],
            is_volatile: false,
            expires: crate::expiration::Expires::Never,
            error: None,
            dependencies: vec![],
            ..Plan::new()
        };
        assert_eq!(plan.split_index(), 0);
        let (p1, p2) = plan.split();
        assert!(p1.is_empty());
        assert_eq!(p2.len(), 5);

        // Plan with a non-context-modifier before the action
        let plan = Plan {
            query: Default::default(),
            payload_required: PayloadRequirement::None,
            init_steps: vec![],
            steps: vec![
                Step::GetAsset(Key::new()),
                Step::Info("info1".to_string()),
                Step::Action {
                    realm: "r".to_string(),
                    ns: "n".to_string(),
                    action_name: "a".to_string(),
                    position: Position::unknown(),
                    parameters: ResolvedParameterValues::new(),
                },
                Step::Info("info2".to_string()),
            ],
            is_volatile: false,
            expires: crate::expiration::Expires::Never,
            error: None,
            dependencies: vec![],
            ..Plan::new()
        };
        assert_eq!(plan.split_index(), 1);
        let (p1, p2) = plan.split();
        assert_eq!(p1.len(), 1);
        assert_eq!(p2.len(), 3);
        assert!(p2[0].is_context_modifier());
        assert!(p2[1].is_action());
        assert!(p2[2].is_context_modifier());

        // Plan with two actions
        eprintln!("### Testing plan with two actions");
        let plan = Plan {
            query: Default::default(),
            payload_required: PayloadRequirement::None,
            init_steps: vec![],
            steps: vec![
                Step::GetAsset(Key::new()),
                Step::Action {
                    realm: "r".to_string(),
                    ns: "n".to_string(),
                    action_name: "a1".to_string(),
                    position: Position::unknown(),
                    parameters: ResolvedParameterValues::new(),
                },
                Step::Action {
                    realm: "r".to_string(),
                    ns: "n".to_string(),
                    action_name: "a2".to_string(),
                    position: Position::unknown(),
                    parameters: ResolvedParameterValues::new(),
                },
                Step::Info("info".to_string()),
            ],
            is_volatile: false,
            expires: crate::expiration::Expires::Never,
            error: None,
            dependencies: vec![],
            ..Plan::new()
        };
        assert_eq!(plan.split_index(), 2);
        let (p1, p2) = plan.split();
        assert_eq!(p1.len(), 2);
        assert_eq!(p2.len(), 2);
        assert!(p1[1].is_action());
        assert!(p2[0].is_action());
        assert!(p2[1].is_context_modifier());

        let plan = Plan {
            query: Default::default(),
            payload_required: PayloadRequirement::None,
            init_steps: vec![],
            steps: vec![Step::Evaluate(Default::default())],
            is_volatile: false,
            expires: crate::expiration::Expires::Never,
            error: None,
            dependencies: vec![],
            ..Plan::new()
        };
        assert_eq!(plan.split_index(), 1);
        let (p1, p2) = plan.split();
        assert_eq!(p1.len(), 1);
        assert_eq!(p2.len(), 0);
    }

    #[test]
    fn test_q_instruction_plan() {
        let mut cr = CommandMetadataRegistry::new();
        cr.add_command(
            CommandMetadata::new("command1").with_argument(ArgumentInfo::any_argument("arg")),
        );
        cr.add_command(
            CommandMetadata::new("command2").with_argument(ArgumentInfo::any_argument("arg")),
        );

        // Parse query: -R/data/test.csv/-/command1-arg/q/command2-arg
        let query = parse_query("-R/data/test.csv/-/command1-arg/q/command2-arg").unwrap();
        let plan = PlanBuilder::new(query, &cr).build().unwrap();

        // Verify plan has 2 steps
        assert_eq!(plan.len(), 2);

        // Step 1 should be Step::UseQueryValue
        if let Step::UseQueryValue(q) = &plan[0] {
            assert_eq!(q.encode(), "-R/data/test.csv/-/command1-arg");
        } else {
            panic!("Expected Step::UseQueryValue, got {:?}", plan[0]);
        }

        // Step 2 should be Step::Action(command2-arg)
        if let Step::Action { action_name, .. } = &plan[1] {
            assert_eq!(action_name, "command2");
        } else {
            panic!("Expected Step::Action, got {:?}", plan[1]);
        }
    }

    #[test]
    fn test_q_instruction_at_end() {
        let mut cr = CommandMetadataRegistry::new();
        cr.add_command(
            CommandMetadata::new("command1").with_argument(ArgumentInfo::any_argument("arg")),
        );

        // Parse query: command1-arg/q (q at the end)
        let query = parse_query("command1-arg/q").unwrap();
        let plan = PlanBuilder::new(query, &cr).build().unwrap();

        // Should have 1 step: UseQueryValue
        assert_eq!(plan.len(), 1);

        // Step should be Step::UseQueryValue
        if let Step::UseQueryValue(q) = &plan[0] {
            assert_eq!(q.encode(), "command1-arg");
        } else {
            panic!("Expected Step::UseQueryValue, got {:?}", plan[0]);
        }
    }

    #[test]
    fn test_q_instruction_with_filename() {
        let mut cr = CommandMetadataRegistry::new();
        cr.add_command(
            CommandMetadata::new("command1").with_argument(ArgumentInfo::any_argument("arg")),
        );

        // Parse query: command1-arg/q/result.txt
        let query = parse_query("command1-arg/q/result.txt").unwrap();
        let plan = PlanBuilder::new(query, &cr).build().unwrap();

        // Should have 2 steps: UseQueryValue and Filename
        assert_eq!(plan.len(), 2);

        // Step 1 should be Step::UseQueryValue
        if let Step::UseQueryValue(q) = &plan[0] {
            assert_eq!(q.encode(), "command1-arg");
        } else {
            panic!("Expected Step::UseQueryValue, got {:?}", plan[0]);
        }

        // Step 2 should be Step::Filename
        if let Step::Filename(name) = &plan[1] {
            assert_eq!(name.name, "result.txt");
        } else {
            panic!("Expected Step::Filename, got {:?}", plan[1]);
        }
    }

    #[test]
    fn test_q_instruction_with_arguments_error() {
        let mut cr = CommandMetadataRegistry::new();
        cr.add_command(
            CommandMetadata::new("command1").with_argument(ArgumentInfo::any_argument("arg")),
        );

        // Parse query: command1-arg/q-invalid (q with argument - should error)
        let query = parse_query("command1-arg/q-invalid").unwrap();
        let result = PlanBuilder::new(query, &cr).build();

        // Should return an error
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.message.contains("does not accept any arguments"));
    }

    #[test]
    fn test_plan_builder_mark_volatile() {
        let cr = CommandMetadataRegistry::new();
        let query = parse_query("").unwrap();
        let mut builder = PlanBuilder::new(query, &cr);

        // Initially not volatile
        assert!(!builder.is_volatile);
        assert_eq!(builder.plan.init_steps.len(), 0);

        // Mark as volatile
        builder.mark_volatile("Test reason", VolatilitySource::Positional);

        // Should be volatile now
        assert!(builder.is_volatile);
        assert_eq!(builder.plan.init_steps.len(), 1);

        // Verify Step::Info was added
        match &builder.plan.init_steps[0] {
            Step::Info(msg) => assert_eq!(msg, "Test reason"),
            _ => panic!("Expected Step::Info"),
        }

        // Calling again should not add another Step::Info (idempotency)
        builder.mark_volatile("Another reason", VolatilitySource::Positional);
        assert_eq!(builder.plan.init_steps.len(), 1);
    }

    #[test]
    fn test_is_action_volatile() {
        let mut cr = CommandMetadataRegistry::new();

        // Add volatile command
        let mut volatile_cmd = CommandMetadata::new("volatile_cmd");
        volatile_cmd.volatile = true;
        cr.add_command(&volatile_cmd);

        // Add non-volatile command
        let normal_cmd = CommandMetadata::new("normal_cmd");
        cr.add_command(&normal_cmd);

        let query = parse_query("").unwrap();
        let builder = PlanBuilder::new(query, &cr);

        // Test volatile command
        // Note: CommandMetadata stores namespace="root", so match it directly
        let volatile_key = CommandKey::new("", "root", "volatile_cmd");
        assert!(builder.is_action_volatile(&volatile_key));

        // Test non-volatile command
        let normal_key = CommandKey::new("", "root", "normal_cmd");
        assert!(!builder.is_action_volatile(&normal_key));

        // Test unknown command
        let unknown_key = CommandKey::new("", "root", "unknown_cmd");
        assert!(!builder.is_action_volatile(&unknown_key));
    }

    #[test]
    fn test_plan_builder_sets_is_volatile() {
        let cr = CommandMetadataRegistry::new();
        let query = parse_query("").unwrap();

        // Test default case: is_volatile = false
        let mut builder1 = PlanBuilder::new(query.clone(), &cr);
        let plan1 = builder1.build().unwrap();
        assert!(!plan1.is_volatile);

        // Test when marked as volatile
        let mut builder2 = PlanBuilder::new(query, &cr);
        builder2.mark_volatile("Test volatility", VolatilitySource::Positional);
        let plan2 = builder2.build().unwrap();
        assert!(plan2.is_volatile);
    }

    #[test]
    fn test_v_instruction_marks_volatile() {
        let cr = CommandMetadataRegistry::new();
        // Query with 'v' instruction
        let query = parse_query("v").unwrap();
        let mut builder = PlanBuilder::new(query, &cr);
        let plan = builder.build().unwrap();

        // Plan should be marked as volatile
        assert!(plan.is_volatile);

        // Should have Step::Info explaining why
        assert!(plan.init_steps.iter().any(|step| {
            matches!(step, Step::Info(msg) if msg.contains("Volatile due to instruction 'v'"))
        }));
    }

    #[test]
    fn test_v_instruction_no_action_step() {
        let cr = CommandMetadataRegistry::new();
        // Query with 'v' instruction
        let query = parse_query("v").unwrap();
        let mut builder = PlanBuilder::new(query, &cr);
        let plan = builder.build().unwrap();

        // Should NOT have a Step::Action for 'v'
        assert!(!plan.steps.iter().any(|step| {
            matches!(step, Step::Action { action_name, .. } if action_name == "v")
        }));
    }

    #[test]
    fn test_volatile_command_marks_volatile() {
        let mut cr = CommandMetadataRegistry::new();

        // Add a volatile command
        let mut volatile_cmd = CommandMetadata::new("volatile_test");
        volatile_cmd.volatile = true;
        cr.add_command(&volatile_cmd);

        // Query using the volatile command
        let query = parse_query("volatile_test").unwrap();
        let mut builder = PlanBuilder::new(query, &cr);
        let plan = builder.build().unwrap();

        // Plan should be marked as volatile
        assert!(plan.is_volatile);

        // Should have Step::Info explaining why
        assert!(plan.init_steps.iter().any(|step| {
            matches!(step, Step::Info(msg) if msg.contains("Volatile due to command"))
        }));
    }

    #[test]
    fn test_link_parameter_volatile() {
        let cr = CommandMetadataRegistry::new();

        // Create a PlanBuilder with an empty query
        let query = parse_query("").unwrap();
        let mut builder = PlanBuilder::new(query, &cr);

        // Create ResolvedParameterValues with a link to a volatile query
        let volatile_query = parse_query("v").unwrap();
        let params = ResolvedParameterValues(vec![ParameterValue::ParameterLink(
            "test_param".to_string(),
            volatile_query,
            Position::unknown(),
        )]);

        // Check parameters for volatile links
        builder
            .check_parameters_for_volatile_links(&params)
            .unwrap();

        // Builder should now be marked as volatile
        assert!(builder.is_volatile);

        // Build the plan
        let plan = builder.build().unwrap();

        // Plan should be marked as volatile
        assert!(plan.is_volatile);

        // Should have Step::Info explaining why
        assert!(plan.init_steps.iter().any(|step| {
            matches!(step, Step::Info(msg) if msg.contains("Volatile due to link parameter"))
        }));
    }

    #[test]
    fn test_plan_is_volatile_field() {
        let cr = CommandMetadataRegistry::new();
        let query = parse_query("").unwrap();
        let mut builder = PlanBuilder::new(query, &cr);

        // Initially not volatile
        assert!(!builder.is_volatile);

        // Mark as volatile
        builder.mark_volatile("test reason", VolatilitySource::Positional);
        assert!(builder.is_volatile);

        // Build the plan
        let plan = builder.build().unwrap();
        assert!(plan.is_volatile);
    }

    #[test]
    fn test_plan_volatile_serialization() {
        let cr = CommandMetadataRegistry::new();
        let query = parse_query("v").unwrap();
        let mut builder = PlanBuilder::new(query, &cr);
        let plan = builder.build().unwrap();

        // Plan should be volatile due to 'v' instruction
        assert!(plan.is_volatile);

        // Serialize and deserialize
        let json = serde_json::to_string(&plan).unwrap();
        let deserialized: Plan = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.is_volatile, true);
    }

    #[test]
    fn test_v_instruction_edge_case_with_q() {
        let cr = CommandMetadataRegistry::new();

        // Test 1: "v" alone should be volatile
        let query1 = parse_query("v").unwrap();
        let mut pb1 = PlanBuilder::new(query1, &cr);
        let plan1 = pb1.build().unwrap();
        assert!(plan1.is_volatile, "v should be volatile");

        // Test 2: "v/q" should NOT be volatile
        // "v/q" evaluates to Query("v"), which is a non-volatile query value
        let query2 = parse_query("v/q").unwrap();
        let mut pb2 = PlanBuilder::new(query2, &cr);
        let plan2 = pb2.build().unwrap();
        assert!(
            !plan2.is_volatile,
            "v/q should NOT be volatile (evaluates to Query value)"
        );
    }

    #[test]
    fn test_plan_expires_default() {
        let cr = CommandMetadataRegistry::new();
        let query = parse_query("-R/data/test.csv").unwrap();
        let mut pb = PlanBuilder::new(query, &cr);
        let plan = pb.build().unwrap();
        assert_eq!(plan.expires, crate::expiration::Expires::Never);
    }

    #[test]
    fn test_plan_expires_from_command() {
        let mut cr = CommandMetadataRegistry::new();
        let mut cm = CommandMetadata::new("expiring_cmd");
        cm.with_argument(ArgumentInfo::any_argument("arg"));
        cm.expires = "in 5 min".parse().unwrap();
        cr.add_command(&cm);

        let query = parse_query("expiring_cmd-hello").unwrap();
        let mut pb = PlanBuilder::new(query, &cr);
        let plan = pb.build().unwrap();
        assert_eq!(
            plan.expires,
            crate::expiration::Expires::InDuration(std::time::Duration::from_secs(300))
        );
    }

    #[test]
    fn test_plan_expires_min_of_commands() {
        let mut cr = CommandMetadataRegistry::new();

        let mut cm1 = CommandMetadata::new("cmd1");
        cm1.with_argument(ArgumentInfo::any_argument("arg"));
        cm1.expires = "in 10 min".parse().unwrap();
        cr.add_command(&cm1);

        let mut cm2 = CommandMetadata::new("cmd2");
        cm2.with_argument(ArgumentInfo::any_argument("arg"));
        cm2.expires = "in 5 min".parse().unwrap();
        cr.add_command(&cm2);

        let query = parse_query("cmd1-a/cmd2-b").unwrap();
        let mut pb = PlanBuilder::new(query, &cr);
        let plan = pb.build().unwrap();
        // Should be the minimum: 5 min
        assert_eq!(
            plan.expires,
            crate::expiration::Expires::InDuration(std::time::Duration::from_secs(300))
        );
    }

    #[test]
    fn test_plan_expires_combines_different_command_expirations() {
        let mut cr = CommandMetadataRegistry::new();

        let mut cm1 = CommandMetadata::new("cmd1");
        cm1.with_argument(ArgumentInfo::any_argument("arg"));
        cm1.expires = "in 10 min".parse().unwrap();
        cr.add_command(&cm1);

        let mut cm2 = CommandMetadata::new("cmd2");
        cm2.with_argument(ArgumentInfo::any_argument("arg"));
        cm2.expires = "end of day".parse().unwrap();
        cr.add_command(&cm2);

        let query = parse_query("cmd1-a/cmd2-b").unwrap();
        let mut pb = PlanBuilder::new(query, &cr);
        let plan = pb.build().unwrap();
        assert_eq!(
            plan.expires,
            crate::expiration::Expires::InDuration(std::time::Duration::from_secs(600))
                | crate::expiration::Expires::EndOfDay { tz_offset: None }
        );
    }

    #[test]
    fn test_plan_expires_serialization() {
        let cr = CommandMetadataRegistry::new();
        let query = parse_query("-R/data/test.csv").unwrap();
        let mut pb = PlanBuilder::new(query, &cr);
        let mut plan = pb.build().unwrap();
        plan.expires = "in 1 hours".parse().unwrap();
        let json = serde_json::to_string(&plan).unwrap();
        let plan2: Plan = serde_json::from_str(&json).unwrap();
        assert_eq!(plan2.expires, plan.expires);
    }

    // ---- Payload requirement: PlanBuilder local detection (U3) ----

    #[test]
    fn test_action_payload_requirement() {
        let mut cr = CommandMetadataRegistry::new();

        let mut payload_cmd = CommandMetadata::new("payload_cmd");
        payload_cmd.payload_required = PayloadRequirement::Required;
        cr.add_command(&payload_cmd);

        let normal_cmd = CommandMetadata::new("normal_cmd");
        cr.add_command(&normal_cmd);

        let query = parse_query("").unwrap();
        let builder = PlanBuilder::new(query, &cr);

        assert_eq!(
            builder.action_payload_requirement(&CommandKey::new("", "root", "payload_cmd")),
            PayloadRequirement::Required
        );
        assert_eq!(
            builder.action_payload_requirement(&CommandKey::new("", "root", "normal_cmd")),
            PayloadRequirement::None
        );
        // Unknown command must not claim a payload requirement.
        assert_eq!(
            builder.action_payload_requirement(&CommandKey::new("", "root", "unknown_cmd")),
            PayloadRequirement::None
        );
    }

    #[test]
    fn test_plan_payload_required_from_command() -> Result<(), Box<dyn std::error::Error>> {
        let mut cr = CommandMetadataRegistry::new();
        let mut payload_cmd = CommandMetadata::new("payload_cmd");
        payload_cmd.payload_required = PayloadRequirement::Required;
        // register_command! sets volatile alongside payload_required; mirror that here.
        payload_cmd.volatile = true;
        cr.add_command(&payload_cmd);

        let plan = PlanBuilder::new(parse_query("payload_cmd")?, &cr).build()?;
        assert_eq!(plan.payload_required, PayloadRequirement::Required);
        // A payload requirement always travels with volatility.
        assert!(plan.is_volatile);
        Ok(())
    }

    #[test]
    fn test_plan_payload_none_by_default() -> Result<(), Box<dyn std::error::Error>> {
        let mut cr = CommandMetadataRegistry::new();
        cr.add_command(&CommandMetadata::new("normal_cmd"));

        let plan = PlanBuilder::new(parse_query("normal_cmd")?, &cr).build()?;
        assert_eq!(plan.payload_required, PayloadRequirement::None);
        Ok(())
    }

    // ---- init_steps reasoning (U7) ----

    #[test]
    fn test_mark_payload_required_is_recorded_once() {
        let cr = CommandMetadataRegistry::new();
        let query = parse_query("").unwrap();
        let mut builder = PlanBuilder::new(query, &cr);

        assert_eq!(builder.payload_required, PayloadRequirement::None);
        assert_eq!(builder.plan.init_steps.len(), 0);

        builder.mark_payload_required("Test reason");
        assert_eq!(builder.payload_required, PayloadRequirement::Required);
        assert_eq!(builder.plan.init_steps.len(), 1);
        match &builder.plan.init_steps[0] {
            Step::Info(msg) => assert_eq!(msg, "Test reason"),
            _ => panic!("Expected Step::Info"),
        }

        // Transition guard: a second cause must not add a second message.
        builder.mark_payload_required("Another reason");
        assert_eq!(builder.plan.init_steps.len(), 1);
    }

    #[test]
    fn test_no_info_when_no_payload_command() -> Result<(), Box<dyn std::error::Error>> {
        let mut cr = CommandMetadataRegistry::new();
        cr.add_command(&CommandMetadata::new("normal_cmd"));

        let plan = PlanBuilder::new(parse_query("normal_cmd")?, &cr).build()?;
        assert!(
            !plan
                .init_steps
                .iter()
                .any(|s| matches!(s, Step::Info(m) if m.contains("Payload required"))),
            "no payload message expected, got: {:?}",
            plan.init_steps
        );
        Ok(())
    }

    #[test]
    fn test_info_names_the_triggering_command() -> Result<(), Box<dyn std::error::Error>> {
        let mut cr = CommandMetadataRegistry::new();
        let mut payload_cmd = CommandMetadata::new("payload_cmd");
        payload_cmd.payload_required = PayloadRequirement::Required;
        cr.add_command(&payload_cmd);

        let plan = PlanBuilder::new(parse_query("payload_cmd")?, &cr).build()?;
        assert!(
            plan.init_steps.iter().any(|s| matches!(s, Step::Info(m)
                    if m.contains("Payload required") && m.contains("payload_cmd"))),
            "expected a message naming the command, got: {:?}",
            plan.init_steps
        );
        Ok(())
    }

    #[test]
    fn test_info_added_once_for_two_payload_commands() -> Result<(), Box<dyn std::error::Error>> {
        let mut cr = CommandMetadataRegistry::new();
        for name in ["payload_a", "payload_b"] {
            let mut cmd = CommandMetadata::new(name);
            cmd.payload_required = PayloadRequirement::Required;
            cmd.state_argument = Some(ArgumentInfo::any_argument("state"));
            cr.add_command(&cmd);
        }

        let plan = PlanBuilder::new(parse_query("payload_a/payload_b")?, &cr).build()?;
        let count = plan
            .init_steps
            .iter()
            .filter(|s| matches!(s, Step::Info(m) if m.contains("Payload required")))
            .count();
        assert_eq!(
            count, 1,
            "expected exactly one message, got: {:?}",
            plan.init_steps
        );
        Ok(())
    }

    // ---- Plan splitting (U4) ----

    #[test]
    fn test_plan_split_preserves_payload_required() -> Result<(), Box<dyn std::error::Error>> {
        // Regression guard: splitting copies is_volatile to both halves, and must copy
        // payload_required too. Missing this is silent until a split plan is evaluated.
        let mut cr = CommandMetadataRegistry::new();
        let mut payload_cmd = CommandMetadata::new("payload_cmd");
        payload_cmd.payload_required = PayloadRequirement::Required;
        payload_cmd.volatile = true;
        cr.add_command(&payload_cmd);
        cr.add_command(&CommandMetadata::new("second_cmd"));

        let plan = PlanBuilder::new(parse_query("payload_cmd/second_cmd")?, &cr).build()?;
        assert_eq!(plan.payload_required, PayloadRequirement::Required);
        assert!(
            plan.split_index() > 0,
            "test needs a plan that actually splits, got steps: {:?}",
            plan.steps
        );

        let (first, second) = plan.split();
        assert_eq!(first.payload_required, PayloadRequirement::Required);
        assert_eq!(second.payload_required, PayloadRequirement::Required);
        // The volatile counterpart must keep working alongside it.
        assert!(first.is_volatile);
        assert!(second.is_volatile);
        Ok(())
    }

    // ---- Serialization (U2, Plan half) ----

    #[test]
    fn test_plan_without_payload_field_deserializes() -> Result<(), Box<dyn std::error::Error>> {
        // Plans serialized before the field existed must still load. This is the reason
        // Plan::payload_required has serde(default) while is_volatile deliberately does not.
        let plan = Plan::new();
        let mut value = serde_json::to_value(&plan)?;
        if let serde_json::Value::Object(ref mut o) = value {
            o.remove("payload_required");
        }
        let back: Plan = serde_json::from_value(value)?;
        assert_eq!(back.payload_required, PayloadRequirement::None);
        Ok(())
    }

    #[test]
    fn test_plan_payload_required_round_trips() -> Result<(), Box<dyn std::error::Error>> {
        let mut plan = Plan::new();
        plan.payload_required = PayloadRequirement::Required;
        let back: Plan = serde_json::from_str(&serde_json::to_string(&plan)?)?;
        assert_eq!(back.payload_required, PayloadRequirement::Required);
        Ok(())
    }

    // ---- Action-parameter links reached from query TEXT (query-link-parser) ----
    //
    // ActionParameter::Link and this whole planner path already worked for
    // programmatically built links; what was impossible was reaching any of it from
    // query text. These pin the newly-reachable path.

    fn link_registry() -> command_metadata::CommandMetadataRegistry {
        let mut cr = command_metadata::CommandMetadataRegistry::new();
        cr.add_command(
            CommandMetadata::new("greet").with_argument(ArgumentInfo::any_argument("who")),
        );
        cr.add_command(&CommandMetadata::new("world"));
        cr
    }

    fn action_parameters(plan: &Plan) -> ResolvedParameterValues {
        for step in &plan.steps {
            if let Step::Action { parameters, .. } = step {
                return parameters.clone();
            }
        }
        panic!("plan has no action step: {plan:?}");
    }

    #[test]
    fn d1_plan_textual_link_is_parameter_link() -> Result<(), Error> {
        let cr = link_registry();
        let plan = PlanBuilder::new(parse_query("greet-~X~world~E")?, &cr).build()?;
        match &action_parameters(&plan).0[0] {
            ParameterValue::ParameterLink(name, query, _) => {
                assert_eq!(name, "who");
                assert_eq!(query.encode(), "world");
            }
            other => panic!("expected a ParameterLink, got {other:?}"),
        }
        Ok(())
    }

    #[test]
    fn d2_plan_textual_and_programmatic_links_agree() -> Result<(), Error> {
        let cr = link_registry();

        // From query text.
        let from_text = PlanBuilder::new(parse_query("greet-~X~world~E")?, &cr).build()?;

        // The same query built programmatically, the only route available before this fix.
        let mut built = parse_query("greet-placeholder")?;
        match &mut built.segments[0] {
            QuerySegment::Transform(tqs) => {
                tqs.query[0].parameters[0] = ActionParameter::new_link(parse_query("world")?);
            }
            QuerySegment::Resource(_) => panic!("expected a transform segment"),
        }
        let from_built = PlanBuilder::new(built, &cr).build()?;

        let (a, b) = (
            action_parameters(&from_text),
            action_parameters(&from_built),
        );
        match (&a.0[0], &b.0[0]) {
            (
                ParameterValue::ParameterLink(n1, q1, _),
                ParameterValue::ParameterLink(n2, q2, _),
            ) => {
                assert_eq!(n1, n2);
                assert_eq!(q1.encode(), q2.encode());
            }
            (x, y) => panic!("expected two ParameterLinks, got {x:?} and {y:?}"),
        }
        Ok(())
    }

    #[test]
    fn d3_plan_link_position_propagates() -> Result<(), Error> {
        // The plan must carry the link's real position, not Position::unknown(), so
        // downstream diagnostics can point back into the query text.
        let cr = link_registry();
        let plan = PlanBuilder::new(parse_query("greet-~X~world~E")?, &cr).build()?;
        match &action_parameters(&plan).0[0] {
            ParameterValue::ParameterLink(_, _, position) => {
                assert!(
                    !position.is_unknown(),
                    "position must survive plan building"
                );
                assert_eq!(position.offset, "greet-".len());
            }
            other => panic!("expected a ParameterLink, got {other:?}"),
        }
        Ok(())
    }

    #[tokio::test]
    async fn find_dependencies_resolves_all_link_variants_with_ordered_cwd() -> Result<(), Error> {
        let envref = ImmediateEnvironment::<crate::value::Value>::new().to_ref();
        let mut plan = Plan::new();
        plan.steps = vec![
            Step::SetCwd(parse_key("a/b")?),
            Step::SetCwd(parse_key("../c")?),
            Step::Action {
                realm: String::new(),
                ns: String::new(),
                action_name: "links".to_owned(),
                position: Position::unknown(),
                parameters: ResolvedParameterValues(vec![
                    ParameterValue::DefaultLink(
                        "default".to_owned(),
                        parse_query("-R/./default.txt")?,
                    ),
                    ParameterValue::ParameterLink(
                        "parameter".to_owned(),
                        parse_query("-R/./parameter.txt")?,
                        Position::unknown(),
                    ),
                    ParameterValue::OverrideLink(
                        "override".to_owned(),
                        parse_query("-R/./override.txt")?,
                    ),
                    ParameterValue::EnumLink(
                        "enum".to_owned(),
                        parse_query("-R/./enum.txt")?,
                        Position::unknown(),
                    ),
                    ParameterValue::MultipleParameters(
                        "multiple".to_owned(),
                        vec![ParameterValue::OverrideLink(
                            "multiple".to_owned(),
                            parse_query("-R/./multiple.txt")?,
                        )],
                    ),
                ]),
            },
        ];

        let mut stack = Vec::new();
        let mut cursor = CwdCursor::default();
        let dependencies = find_dependencies(envref, &plan, &mut stack, &mut cursor).await?;

        for (filename, relation) in [
            (
                "default.txt",
                DependencyRelation::DefaultLink("default".to_owned()),
            ),
            (
                "parameter.txt",
                DependencyRelation::ParameterLink("parameter".to_owned()),
            ),
            (
                "override.txt",
                DependencyRelation::OverrideLink("override".to_owned()),
            ),
            ("enum.txt", DependencyRelation::EnumLink("enum".to_owned())),
            (
                "multiple.txt",
                DependencyRelation::OverrideLink("multiple".to_owned()),
            ),
        ] {
            assert!(plan_dependencies_contain(
                &dependencies,
                &format!("-R/a/c/{filename}"),
                &relation,
            ));
        }
        assert_eq!(cursor.current(), Some(parse_key("a/c")?));
        Ok(())
    }

    #[tokio::test]
    async fn deep_multiple_parameters_and_long_key_preserve_order_and_provenance(
    ) -> Result<(), Error> {
        const DEPTH: usize = 32;
        const KEY_PARTS: usize = 128;

        let raw_key = (0..KEY_PARTS)
            .map(|index| format!("part{index}"))
            .collect::<Vec<_>>()
            .join("/");
        let mut linked_query = parse_query(&format!("-R/./{raw_key}"))?;
        linked_query.source = QuerySource::Other("deep multiple leaf".to_owned());
        let expected_link_position = linked_query.position();

        let mut nested = ParameterValue::OverrideLink("leaf".to_owned(), linked_query);
        for depth in 0..DEPTH {
            nested = ParameterValue::MultipleParameters(
                "multiple".to_owned(),
                vec![
                    ParameterValue::DefaultValue(
                        format!("before-{depth}"),
                        serde_json::json!(depth),
                    ),
                    nested,
                    ParameterValue::OverrideValue(
                        format!("after-{depth}"),
                        serde_json::json!(depth),
                    ),
                ],
            );
        }

        let action_position = Position::new(500, 1, 501);
        let mut plan = Plan::new();
        plan.steps = vec![
            Step::SetCwd(parse_key("base")?),
            Step::Action {
                realm: String::new(),
                ns: String::new(),
                action_name: "deep".to_owned(),
                position: action_position.clone(),
                parameters: ResolvedParameterValues(vec![nested]),
            },
        ];

        let envref = ImmediateEnvironment::<crate::value::Value>::new().to_ref();
        let mut stack = Vec::new();
        let mut cursor = CwdCursor::default();
        let dependencies = find_dependencies(envref, &plan, &mut stack, &mut cursor).await?;

        assert!(plan_dependencies_contain(
            &dependencies,
            &format!("-R/base/{raw_key}"),
            &DependencyRelation::OverrideLink("leaf".to_owned()),
        ));
        assert_eq!(
            dependencies
                .iter()
                .filter(|dependency| matches!(
                    dependency.relation,
                    DependencyRelation::OverrideLink(ref name) if name == "leaf"
                ))
                .count(),
            1
        );
        assert_eq!(cursor.current(), Some(parse_key("base")?));

        let Step::Action {
            position,
            parameters,
            ..
        } = &plan.steps[1]
        else {
            panic!("expected action step");
        };
        assert_eq!(position, &action_position);
        let mut current = &parameters.0[0];
        for depth in (0..DEPTH).rev() {
            let ParameterValue::MultipleParameters(_, values) = current else {
                panic!("expected nesting level {depth}");
            };
            assert_eq!(values.len(), 3);
            assert!(matches!(
                &values[0],
                ParameterValue::DefaultValue(name, value)
                    if name == &format!("before-{depth}") && value == &serde_json::json!(depth)
            ));
            assert!(matches!(
                &values[2],
                ParameterValue::OverrideValue(name, value)
                    if name == &format!("after-{depth}") && value == &serde_json::json!(depth)
            ));
            current = &values[1];
        }
        let ParameterValue::OverrideLink(name, query) = current else {
            panic!("expected deeply nested override link");
        };
        assert_eq!(name, "leaf");
        assert_eq!(query.encode(), format!("-R/./{raw_key}"));
        assert_eq!(
            query.source,
            QuerySource::Other("deep multiple leaf".to_owned())
        );
        assert_eq!(query.position(), expected_link_position);
        Ok(())
    }

    #[tokio::test]
    async fn find_dependencies_child_query_cwd_does_not_leak() -> Result<(), Error> {
        let envref = ImmediateEnvironment::<crate::value::Value>::new().to_ref();
        let mut plan = Plan::new();
        plan.steps = vec![
            Step::SetCwd(parse_key("a/b")?),
            Step::Evaluate(parse_query("-R-cwd/../child/-R/./inside.txt")?),
            Step::GetAsset(parse_key("./outside.txt")?),
        ];

        let mut stack = Vec::new();
        let mut cursor = CwdCursor::default();
        let dependencies = find_dependencies(envref, &plan, &mut stack, &mut cursor).await?;

        assert!(plan_dependencies_contain(
            &dependencies,
            "-R/a/child/inside.txt",
            &DependencyRelation::StateArgument,
        ));
        assert!(plan_dependencies_contain(
            &dependencies,
            "-R/a/b/outside.txt",
            &DependencyRelation::StateArgument,
        ));
        assert!(!dependencies
            .iter()
            .any(|dependency| dependency.key.as_str() == "-R/a/child/outside.txt"));
        assert_eq!(cursor.current(), Some(parse_key("a/b")?));
        Ok(())
    }

    #[tokio::test]
    async fn find_dependencies_nested_plan_propagates_cwd() -> Result<(), Error> {
        let envref = ImmediateEnvironment::<crate::value::Value>::new().to_ref();
        let mut nested = Plan::new();
        nested.steps = vec![
            Step::SetCwd(parse_key("../c")?),
            Step::GetAsset(parse_key("./inside.txt")?),
        ];
        let mut plan = Plan::new();
        plan.steps = vec![
            Step::SetCwd(parse_key("a/b")?),
            Step::Plan(nested),
            Step::GetAsset(parse_key("./outside.txt")?),
        ];

        let mut stack = Vec::new();
        let mut cursor = CwdCursor::default();
        let dependencies = find_dependencies(envref, &plan, &mut stack, &mut cursor).await?;

        for key in ["-R/a/c/inside.txt", "-R/a/c/outside.txt"] {
            assert!(plan_dependencies_contain(
                &dependencies,
                key,
                &DependencyRelation::StateArgument,
            ));
        }
        assert_eq!(cursor.current(), Some(parse_key("a/c")?));
        Ok(())
    }

    #[tokio::test]
    async fn find_dependencies_respects_nested_recipe_cwd() -> Result<(), Error> {
        let output_key = parse_key("recipe/folder/result.txt")?;
        let mut recipe = Recipe::new(
            "use/result.txt".to_owned(),
            "Recipe CWD".to_owned(),
            String::new(),
        )?
        .with_link("input".to_owned(), "-R/./source.txt".to_owned());
        recipe.cwd = Some("recipe/folder".to_owned());
        let provider = CountingRecipeProvider::new([(output_key.clone(), recipe)]);
        let mut env = ImmediateEnvironment::<crate::value::Value>::new();
        env.command_registry.command_metadata_registry.add_command(
            CommandMetadata::new("use").with_argument(ArgumentInfo::any_argument("input")),
        );
        env.with_recipe_provider(Box::new(provider));
        let envref = env.to_ref();
        let mut plan = Plan::new();
        plan.steps.push(Step::GetAsset(output_key));

        let mut stack = Vec::new();
        let mut cursor = CwdCursor::default();
        let dependencies = find_dependencies(envref, &plan, &mut stack, &mut cursor).await?;

        assert!(plan_dependencies_contain(
            &dependencies,
            "-R/recipe/folder/source.txt",
            &DependencyRelation::OverrideLink("input".to_owned()),
        ));
        Ok(())
    }

    #[tokio::test]
    async fn find_dependencies_non_dependency_value_steps_do_not_warn_early() -> Result<(), Error> {
        let envref = ImmediateEnvironment::<crate::value::Value>::new().to_ref();
        let mut plan = Plan::new();
        plan.steps = vec![
            Step::UseKeyValue(parse_key("./key.txt")?),
            Step::UseQueryValue(parse_query("-R/./query.txt")?),
            Step::GetResource(parse_key("./stored.txt")?),
            Step::GetResourceMetadata(parse_key("./metadata.txt")?),
            Step::GetResourceDirectory(parse_key("./directory")?),
        ];

        has_volatile_dependencies(envref, &mut plan, None).await?;

        assert!(plan.dependencies.is_empty());
        assert!(!plan.init_steps.iter().any(
            |step| matches!(step, Step::Warning(message) if message == RELATIVE_WITHOUT_CWD_WARNING)
        ));
        Ok(())
    }

    #[tokio::test]
    async fn volatility_populates_dependencies_once_and_expiration_reuses_them() -> Result<(), Error>
    {
        let dependency_key = parse_key("inputs/source.txt")?;
        let mut recipe = Recipe::default();
        recipe.expires = Expires::InDuration(std::time::Duration::from_secs(30));
        let provider = CountingRecipeProvider::new([(dependency_key.clone(), recipe)]);
        let mut env = ImmediateEnvironment::<crate::value::Value>::new();
        env.with_recipe_provider(Box::new(provider.clone()));
        let envref = env.to_ref();
        let mut plan = Plan::new();
        plan.steps.push(Step::GetAsset(dependency_key));

        has_volatile_dependencies(envref.clone(), &mut plan, None).await?;
        let dependency_info_count = plan
            .init_steps
            .iter()
            .filter(|step| matches!(step, Step::Info(message) if message.starts_with("Dependency detected:")))
            .count();
        assert_eq!(provider.recipe_opt_calls(), 2);
        assert_eq!(dependency_info_count, plan.dependencies.len());

        has_expirable_dependencies(envref, &mut plan).await?;

        assert_eq!(provider.recipe_opt_calls(), 3);
        assert_eq!(
            plan.init_steps
                .iter()
                .filter(|step| matches!(step, Step::Info(message) if message.starts_with("Dependency detected:")))
                .count(),
            dependency_info_count,
        );
        assert_eq!(
            plan.expires,
            Expires::InDuration(std::time::Duration::from_secs(30))
        );
        Ok(())
    }

    #[tokio::test]
    async fn expiration_nested_recipe_uses_keyed_recipe_plan() -> Result<(), Error> {
        let output_key = parse_key("recipe/folder/result.txt")?;
        let source_key = parse_key("recipe/folder/source.txt")?;
        let mut output_recipe = Recipe::new(
            "use/result.txt".to_owned(),
            "Keyed override".to_owned(),
            String::new(),
        )?
        .with_link("input".to_owned(), "-R/./source.txt".to_owned());
        output_recipe.cwd = Some("recipe/folder".to_owned());
        let mut source_recipe = Recipe::default();
        source_recipe.expires = Expires::InDuration(std::time::Duration::from_secs(45));
        let provider = CountingRecipeProvider::new([
            (output_key.clone(), output_recipe),
            (source_key, source_recipe),
        ]);
        let mut env = ImmediateEnvironment::<crate::value::Value>::new();
        env.command_registry.command_metadata_registry.add_command(
            CommandMetadata::new("use").with_argument(ArgumentInfo::any_argument("input")),
        );
        env.with_recipe_provider(Box::new(provider));
        let envref = env.to_ref();
        let mut plan = Plan::new();
        plan.steps.push(Step::GetAsset(output_key));

        has_volatile_dependencies(envref.clone(), &mut plan, None).await?;
        has_expirable_dependencies(envref, &mut plan).await?;

        assert!(plan_dependencies_contain(
            &plan.dependencies,
            "-R/recipe/folder/source.txt",
            &DependencyRelation::OverrideLink("input".to_owned()),
        ));
        assert_eq!(
            plan.expires,
            Expires::InDuration(std::time::Duration::from_secs(45))
        );
        Ok(())
    }

    #[tokio::test]
    async fn d4_link_becomes_parameter_link_dependency() -> Result<(), Error> {
        // find_dependencies is pub(crate), so this lives here rather than in
        // tests/action_parameter_link.rs alongside the other end-to-end checks.
        use crate::context::{Environment, SimpleEnvironment};
        use crate::dependencies::DependencyRelation;

        // `Value` in this module is serde_json::Value; the environment needs ours.
        let env = SimpleEnvironment::<crate::value::Value>::new();
        let envref = env.to_ref();

        let cr = link_registry();
        let plan = PlanBuilder::new(parse_query("greet-~X~world~E")?, &cr).build()?;

        let mut stack = Vec::new();
        let mut cursor = CwdCursor::default();
        let dependencies = find_dependencies(envref, &plan, &mut stack, &mut cursor).await?;

        let links: Vec<_> = dependencies
            .iter()
            .filter(|d| matches!(d.relation, DependencyRelation::ParameterLink(_)))
            .collect();
        assert_eq!(
            links.len(),
            1,
            "the embedded query must appear as a ParameterLink dependency: {dependencies:?}"
        );
        Ok(())
    }

    // ---------------------------------------------------------------------------------
    // Excess action parameters (design: excess-action-parameters-error)
    // ---------------------------------------------------------------------------------

    /// Builds a plan for `q` against a registry holding the given commands.
    fn plan_for(q: &str, cr: &command_metadata::CommandMetadataRegistry) -> Result<Plan, Error> {
        PlanBuilder::new(parse_query(q)?, cr).build()
    }

    fn action_of(q: &str) -> ActionRequest {
        parse_query(q)
            .expect("query must parse")
            .action()
            .expect("query must be a single action")
    }

    /// E1 - the primary case: one parameter more than the command declares.
    #[test]
    fn excess_action_parameter_is_rejected() -> Result<(), Error> {
        let cm = CommandMetadata::new("to_text"); // declares no arguments
        let action = action_of("to_text-extra");

        let err = ResolvedParameterValues::from_action(&action, &cm, false)
            .expect_err("an excess parameter must not build a plan");

        assert_eq!(err.error_type, ErrorType::TooManyParameters);
        // The position is the point of the design: it is what an editor highlights.
        assert_eq!(err.position, action.parameters[0].position());
        assert!(err.message.contains("to_text"), "message: {}", err.message);
        assert!(err.message.contains("extra"), "message: {}", err.message);
        Ok(())
    }

    /// E2 - the exemptions. The rule is only correct if it stays silent in all of these.
    #[test]
    fn arity_boundaries_still_build() -> Result<(), Error> {
        let mut cm = CommandMetadata::new("cmd");
        cm.with_argument(ArgumentInfo::string_argument("a"));
        cm.with_argument(ArgumentInfo::string_argument("b").with_default("bee"));

        // Exactly saturated - the boundary itself.
        assert!(ResolvedParameterValues::from_action(&action_of("cmd-x-y"), &cm, false).is_ok());

        // Under-supplied - `b` falls back to its default. Unchanged behaviour.
        assert!(ResolvedParameterValues::from_action(&action_of("cmd-x"), &cm, false).is_ok());

        // No parameters at all, against a command that declares none. (Omitting a *required*
        // argument is still ArgumentMissing, which this design does not touch.)
        let empty = CommandMetadata::new("empty");
        assert!(ResolvedParameterValues::from_action(&action_of("empty"), &empty, false).is_ok());

        // Variadic - `multiple` drains the iterator, so nothing is ever left over. This must
        // hold WITHOUT a special case in the check; an implementation that compares
        // parameters.len() against arguments.len() fails here and nowhere else.
        let mut vcm = CommandMetadata::new("vcmd");
        vcm.with_argument(ArgumentInfo::string_argument("items").set_multiple());
        assert!(
            ResolvedParameterValues::from_action(&action_of("vcmd-a-b-c-d"), &vcm, false).is_ok()
        );

        Ok(())
    }

    /// A recipe override must be able to reach a variadic argument.
    ///
    /// `MultipleParameters` is one argument slot like any other; a recipe writing
    /// `arguments: {columns: [...]}` must find it by name. Reported by Codex review on PR #38.
    /// See specs/design/variadic-arguments-declaration/.
    #[test]
    fn recipe_override_reaches_a_variadic_argument() -> Result<(), Error> {
        let mut cm = CommandMetadata::new("select_columns");
        cm.with_argument(ArgumentInfo::string_argument("columns").set_multiple());

        let mut values =
            ResolvedParameterValues::from_action(&action_of("select_columns-a-b"), &cm, false)?;
        assert_eq!(values.0.len(), 1, "one argument slot");
        assert_eq!(
            values.0[0].name().as_deref(),
            Some("columns"),
            "a variadic slot must report its argument name, or no override can find it"
        );

        // An array override expands into one element per entry, mirroring how `from_arginfo`
        // expands an array default.
        assert!(
            values.override_value("columns", Value::Array(vec!["x".into(), "y".into()])),
            "override_value must locate the variadic slot"
        );
        let elements = values.0[0]
            .multiple()
            .expect("an applied override must stay a parameter list");
        assert_eq!(elements.len(), 2);
        assert_eq!(elements[0].value(), Some(Value::String("x".to_string())));
        assert_eq!(elements[1].value(), Some(Value::String("y".to_string())));

        // A scalar override is a one-element list.
        let mut values =
            ResolvedParameterValues::from_action(&action_of("select_columns-a-b"), &cm, false)?;
        assert!(values.override_value("columns", "solo".into()));
        let elements = values.0[0].multiple().expect("still a parameter list");
        assert_eq!(elements.len(), 1);
        assert_eq!(elements[0].value(), Some(Value::String("solo".to_string())));

        // A link override likewise stays a parameter list, so the interpreter materialises it
        // element-wise and `get_multiple` accepts the result.
        let mut values =
            ResolvedParameterValues::from_action(&action_of("select_columns-a-b"), &cm, false)?;
        assert!(values.override_link("columns", parse_query("-R/config/cols.json")?));
        let elements = values.0[0].multiple().expect("still a parameter list");
        assert_eq!(elements.len(), 1);
        assert!(elements[0].link().is_some());
        Ok(())
    }

    /// T2 - the *first* surplus parameter is reported, not the last and not a count.
    #[test]
    fn excess_reports_first_surplus_only() -> Result<(), Error> {
        let mut cm = CommandMetadata::new("cmd");
        cm.with_argument(ArgumentInfo::string_argument("a"));
        let action = action_of("cmd-a-b-c-d");

        let err = ResolvedParameterValues::from_action(&action, &cm, false)
            .expect_err("surplus must not build");

        // `b` is parameter #2 and the first one no argument can consume.
        assert_eq!(err.position, action.parameters[1].position());
        assert!(err.message.contains("#2"), "message: {}", err.message);
        assert!(err.message.contains("'b'"), "message: {}", err.message);
        Ok(())
    }

    /// T2 variant - an empty parameter is still a parameter (`parse.rs` documents `action-`
    /// as one empty string parameter), so it is still surplus.
    #[test]
    fn empty_excess_parameter_is_still_excess() -> Result<(), Error> {
        let cm = CommandMetadata::new("cmd");
        let err = ResolvedParameterValues::from_action(&action_of("cmd-"), &cm, false)
            .expect_err("an empty surplus parameter must not build");
        assert_eq!(err.error_type, ErrorType::TooManyParameters);
        Ok(())
    }

    /// T3 - a link in excess position is reported through `encode()`, not a debug form.
    #[test]
    fn excess_link_parameter_encodes() -> Result<(), Error> {
        let cm = CommandMetadata::new("cmd");
        let err = ResolvedParameterValues::from_action(&action_of("cmd-~X~hello~E"), &cm, false)
            .expect_err("surplus link must not build");

        assert!(
            err.message.contains("~X~hello~E"),
            "message: {}",
            err.message
        );
        Ok(())
    }

    /// T4 - injected arguments consume no query parameter, so they are not counted as accepted.
    #[test]
    fn accepted_count_excludes_injected() -> Result<(), Error> {
        let mut cm = CommandMetadata::new("cmd");
        cm.with_argument(ArgumentInfo::string_argument("a"));
        cm.with_argument(ArgumentInfo::string_argument("ctx").set_injected());

        // Two declared arguments, but only one can be supplied by the query.
        let err = ResolvedParameterValues::from_action(&action_of("cmd-x-y"), &cm, false)
            .expect_err("the injected argument must not absorb a parameter");

        assert!(
            err.message.contains("accepts 1"),
            "message: {}",
            err.message
        );
        Ok(())
    }

    /// T5 - alias head parameters fill leading slots, so they too are excluded from `accepted`.
    #[test]
    fn accepted_count_excludes_head_parameters() -> Result<(), Error> {
        let mut cm = CommandMetadata::new("cmd");
        cm.with_argument(ArgumentInfo::string_argument("head"));
        cm.with_argument(ArgumentInfo::string_argument("tail"));

        let head = vec![CommandParameterValue::Value(Value::String("h".to_string()))];
        let err =
            ResolvedParameterValues::from_action_extended(&action_of("cmd-x-y"), &cm, &head, false)
                .expect_err("only one slot is left for the action to fill");

        assert!(
            err.message.contains("accepts 1"),
            "message: {}",
            err.message
        );
        Ok(())
    }

    /// T6 - decision 1: placeholders concern missing arguments, never surplus ones.
    #[test]
    fn excess_errors_under_allow_placeholders() -> Result<(), Error> {
        let cm = CommandMetadata::new("cmd");
        let err = ResolvedParameterValues::from_action(&action_of("cmd-extra"), &cm, true)
            .expect_err("the recipe path is equally strict");
        assert_eq!(err.error_type, ErrorType::TooManyParameters);
        Ok(())
    }

    /// E3 - the resource header has two ignored inputs and they are treated differently.
    /// Surplus parameters can never be consumed, so they error. The header *name* is reserved
    /// for a future realm interpretation (see the TODO above `process_resource_query`), so it
    /// keeps warning - rejecting it would refuse queries a later version accepts.
    #[test]
    fn header_surplus_errors_but_ignored_name_only_warns() -> Result<(), Error> {
        let cr = command_metadata::CommandMetadataRegistry::new();

        let err = plan_for("-R-meta-extra/data/x.txt", &cr)
            .expect_err("surplus header parameters must not build");
        assert_eq!(err.error_type, ErrorType::TooManyParameters);
        assert!(!err.position.is_unknown(), "the excess must be positioned");

        let plan = plan_for("-Rname/data/x.txt", &cr)?;
        assert!(plan.has_warning(), "an ignored header name still warns");
        assert!(!plan.has_error(), "an ignored header name is not an error");

        // One parameter is the normal case and stays unaffected.
        assert!(plan_for("-R-meta/data/x.txt", &cr).is_ok());
        Ok(())
    }

    /// T7 - decision 5: the fallback arm described a parse-shape failure and carried no
    /// position. It now names the instruction and points at it.
    #[test]
    fn unknown_header_instruction_is_positioned() -> Result<(), Error> {
        let cr = command_metadata::CommandMetadataRegistry::new();
        let err = plan_for("-R-nosuch/data/x.txt", &cr)
            .expect_err("an unknown instruction must not build");

        assert!(err.message.contains("nosuch"), "message: {}", err.message);
        assert!(
            !err.message.contains("must be string or link"),
            "the old, misleading message must be gone: {}",
            err.message
        );
        assert!(
            !err.position.is_unknown(),
            "the instruction must be positioned"
        );
        Ok(())
    }

    /// The special instructions bypass command-metadata resolution, so the arity check in
    /// `from_action_extended` never sees them. Each needs its own rule, and they are not the
    /// same rule:
    ///
    /// - `v` takes no parameters -> surplus is an error (it was silently dropped before);
    /// - `q` takes no parameters -> already rejected, now positioned;
    /// - `ns` is legitimately variadic - every parameter names a namespace - so it must keep
    ///   accepting any number of them.
    #[test]
    fn special_instructions_enforce_their_own_arity() -> Result<(), Error> {
        let mut cr = command_metadata::CommandMetadataRegistry::new();
        cr.add_command(&CommandMetadata::new("a"));

        // `v` alone is fine.
        assert!(plan_for("a/v", &cr).is_ok());

        let err = plan_for("a/v-extra", &cr).expect_err("'v' takes no parameters");
        assert_eq!(err.error_type, ErrorType::TooManyParameters);
        assert!(
            err.message.contains("instruction 'v'"),
            "message: {}",
            err.message
        );
        assert!(!err.position.is_unknown(), "the surplus must be positioned");

        // `q` was already rejected; it now carries a position too.
        let err = plan_for("a/q-extra", &cr).expect_err("'q' takes no parameters");
        assert!(err.message.contains("does not accept any arguments"));
        assert!(!err.position.is_unknown(), "the surplus must be positioned");

        // `ns` is variadic by design: each parameter is a namespace name. Making the special
        // instructions uniformly strict would break this.
        let mut nscr = command_metadata::CommandMetadataRegistry::new();
        let mut cm = CommandMetadata::new("b");
        cm.namespace = "two".to_string();
        nscr.add_command(&cm);
        assert!(
            plan_for("ns-one-two/b", &nscr).is_ok(),
            "'ns' must keep accepting several namespaces"
        );
        Ok(())
    }

    /// T8 - the check reaches a full query through PlanBuilder, not only the helper.
    #[test]
    fn plan_builder_rejects_excess_end_to_end() -> Result<(), Error> {
        let mut cr = command_metadata::CommandMetadataRegistry::new();
        cr.add_command(CommandMetadata::new("a").with_argument(ArgumentInfo::any_argument("x")));

        assert!(
            plan_for("a-1", &cr).is_ok(),
            "the saturated query still builds"
        );

        let err = plan_for("a-1-2", &cr).expect_err("the over-supplied query must not build");
        assert_eq!(err.error_type, ErrorType::TooManyParameters);
        assert!(!err.position.is_unknown());
        Ok(())
    }

    // --- freeze -------------------------------------------------------------------------------

    /// Every key-bearing step is resolved. Written as an exhaustive `match` so that adding a
    /// `Step` variant fails to compile here rather than silently going unfrozen.
    #[test]
    fn freeze_resolves_every_keyed_step() -> Result<(), Error> {
        use crate::parse::parse_key;
        let relative = parse_key("./x")?;
        let mut plan = Plan::new();
        plan.steps = vec![
            Step::GetAsset(relative.clone()),
            Step::GetAssetBinary(relative.clone()),
            Step::GetAssetMetadata(relative.clone()),
            Step::GetAssetRecipe(relative.clone()),
            Step::GetAssetDirectory(relative.clone()),
            Step::GetResource(relative.clone()),
            Step::GetResourceMetadata(relative.clone()),
            Step::GetResourceDirectory(relative.clone()),
            Step::UseKeyValue(relative.clone()),
        ];
        plan.freeze_cwd(Some(parse_key("a/b")?))?;

        for step in plan.steps.iter() {
            let key = match step {
                Step::GetAsset(key)
                | Step::GetAssetBinary(key)
                | Step::GetAssetMetadata(key)
                | Step::GetAssetRecipe(key)
                | Step::GetAssetDirectory(key)
                | Step::GetResource(key)
                | Step::GetResourceMetadata(key)
                | Step::GetResourceDirectory(key)
                | Step::UseKeyValue(key)
                | Step::SetCwd(key) => key,
                Step::Evaluate(_)
                | Step::UseQueryValue(_)
                | Step::Action { .. }
                | Step::Plan(_)
                | Step::Filename(_)
                | Step::Info(_)
                | Step::Warning(_)
                | Step::Error(_) => panic!("unexpected step {step:?}"),
            };
            assert_eq!(key.encode(), "a/b/x", "unfrozen operand in {step:?}");
        }
        Ok(())
    }

    /// `SetCwd` takes effect in order, so a later operand sees the most recent working key.
    #[test]
    fn freeze_applies_setcwd_in_order() -> Result<(), Error> {
        use crate::parse::parse_key;
        let mut plan = Plan::new();
        plan.steps = vec![
            Step::SetCwd(parse_key("a/b")?),
            Step::GetAsset(parse_key("./first")?),
            Step::SetCwd(parse_key("../c")?),
            Step::GetAsset(parse_key("./second")?),
        ];
        plan.freeze_cwd(Some(Key::new()))?;

        assert!(matches!(&plan.steps[1], Step::GetAsset(k) if k.encode() == "a/b/first"));
        assert!(matches!(&plan.steps[2], Step::SetCwd(k) if k.encode() == "a/c"));
        assert!(matches!(&plan.steps[3], Step::GetAsset(k) if k.encode() == "a/c/second"));
        Ok(())
    }

    /// A link parameter is its own scope: an inner `-R-cwd` must not move the enclosing plan.
    #[test]
    fn freeze_scopes_link_parameters() -> Result<(), Error> {
        use crate::parse::parse_key;
        let mut plan = Plan::new();
        plan.steps = vec![
            Step::SetCwd(parse_key("a/b")?),
            Step::Action {
                realm: String::new(),
                ns: String::new(),
                action_name: "act".to_owned(),
                position: Position::unknown(),
                parameters: ResolvedParameterValues(vec![ParameterValue::ParameterLink(
                    "value".to_owned(),
                    parse_query("-R-cwd/./child/-R/./inside.txt")?,
                    Position::unknown(),
                )]),
            },
            Step::GetAsset(parse_key("./outside.txt")?),
        ];
        plan.freeze_cwd(Some(Key::new()))?;

        let Step::Action { parameters, .. } = &plan.steps[1] else {
            panic!("expected an action");
        };
        let Some(ParameterValue::ParameterLink(_, link, _)) = parameters.0.first() else {
            panic!("expected a link parameter");
        };
        assert_eq!(link.encode(), "-R-cwd/a/b/child/-R/a/b/child/inside.txt");
        assert!(
            matches!(&plan.steps[2], Step::GetAsset(k) if k.encode() == "a/b/outside.txt"),
            "the link's own cwd must not leak into the enclosing plan"
        );
        Ok(())
    }

    /// A nested plan shares the cursor, so its final working key reaches later outer steps.
    #[test]
    fn freeze_shares_cursor_with_nested_plan() -> Result<(), Error> {
        use crate::parse::parse_key;
        let mut nested = Plan::new();
        nested.steps = vec![Step::SetCwd(parse_key("../c")?)];
        let mut plan = Plan::new();
        plan.steps = vec![
            Step::SetCwd(parse_key("a/b")?),
            Step::Plan(nested),
            Step::GetAsset(parse_key("./after.txt")?),
        ];
        plan.freeze_cwd(Some(Key::new()))?;

        assert!(matches!(&plan.steps[2], Step::GetAsset(k) if k.encode() == "a/c/after.txt"));
        Ok(())
    }

    /// The new fields are `serde(default)`, so a plan serialized before freezing existed still
    /// deserializes — and reads as "not frozen" rather than as frozen against the root.
    #[test]
    fn frozen_plan_serde_defaults_on_legacy() -> Result<(), Error> {
        let legacy = r#"{"query":{"segments":[],"absolute":false,"source":"Unspecified"},
            "steps":[],"is_volatile":false}"#;
        let plan: Plan = serde_json::from_str(legacy)
            .map_err(|e| Error::general_error(format!("legacy plan must deserialize: {e}")))?;
        assert!(plan.frozen_cwd.is_none());
        assert!(plan.predecessor.is_none());
        assert_eq!(plan.predecessor_steps, 0);
        Ok(())
    }

    /// A relative default link is promoted into **its own** argument slot.
    ///
    /// Appending to the action AST only lands in the right place when every earlier slot is
    /// already written. With an omitted argument in front, an appended link would bind to that
    /// earlier argument instead, so the recorded query would mean something different from the
    /// plan it was recorded for.
    #[test]
    fn promotion_does_not_shift_into_an_earlier_argument_slot() -> Result<(), Error> {
        let mut cmr = CommandMetadataRegistry::new();
        let mut prefix = ArgumentInfo::string_argument("prefix");
        prefix.default = CommandParameterValue::Value(serde_json::json!("x"));
        let mut dir = ArgumentInfo::any_argument("dir");
        dir.default = CommandParameterValue::Query(parse_query("-R-key/.")?);
        cmr.add_command(&CommandMetadata::new("seed"));
        cmr.add_command(
            CommandMetadata::new("two_args")
                .with_argument(prefix)
                .with_argument(dir),
        );

        let plan = PlanBuilder::new(parse_query("seed/two_args")?, &cmr).build()?;
        let Some(recorded) = &plan.predecessor else {
            return Ok(()); // nothing recorded is acceptable; a wrong recording is not
        };
        let Some(QuerySegment::Transform(transform)) = recorded.segments.last() else {
            return Ok(());
        };
        for action in transform.query.iter() {
            if action.name != "two_args" {
                continue;
            }
            if let Some(first) = action.parameters.first() {
                assert!(
                    !matches!(first, ActionParameter::Link(_, _)),
                    "the key link landed in the `prefix` slot: {}",
                    recorded.encode()
                );
            }
        }
        Ok(())
    }

    // --- prologue and freezing (predecessor-cut-equivalence step 1) ------------------------

    fn prologue_registry() -> CommandMetadataRegistry {
        let mut cmr = CommandMetadataRegistry::new();
        for name in ["identity", "tail"] {
            cmr.add_command(&CommandMetadata::new(name));
        }
        cmr
    }

    /// The recorded predecessor is frozen against the CWD its own steps start under, not the
    /// entry CWD. `Recipe::to_plan` prepends a `SetCwd` the builder never emitted; the step count
    /// is compensated, and before `prologue_steps` the cursor was not — so the boundary query,
    /// which is the only thing a cut carries, froze one CWD short.
    ///
    /// Fails without the prologue walk in `freeze_cwd_with`, with no cut involved: the defect is
    /// in freezing.
    #[test]
    fn freeze_resolves_predecessor_after_the_recipe_prologue(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let cmr = prologue_registry();
        let mut recipe = Recipe::new(
            "-R/./input.txt/-/identity/tail/out.txt".to_owned(),
            String::new(),
            String::new(),
        )?;
        recipe.cwd = Some("a/c".to_owned());
        let mut plan = recipe.to_plan(&cmr)?;

        assert_eq!(plan.prologue_steps, 1, "the recipe CWD prefix is one step");
        plan.freeze_cwd(None)?;

        let recorded = plan
            .predecessor
            .as_ref()
            .expect("a predecessor was recorded");
        assert_eq!(
            recorded.encode(),
            "-R/a/c/input.txt/-/identity",
            "the boundary query must carry the recipe CWD, not logical root"
        );
        Ok(())
    }

    /// Without a prologue the predecessor resolves from the entry cursor, unchanged.
    #[test]
    fn freeze_leaves_predecessor_alone_without_a_prologue() -> Result<(), Box<dyn std::error::Error>>
    {
        let cmr = prologue_registry();
        let recipe = Recipe::new(
            "-R/./input.txt/-/identity/tail".to_owned(),
            String::new(),
            String::new(),
        )?;
        let mut plan = recipe.to_plan(&cmr)?;
        assert_eq!(plan.prologue_steps, 0);

        plan.freeze_cwd(Some(crate::parse::parse_key("proj/x")?))?;
        assert_eq!(
            plan.predecessor.as_ref().map(|q| q.encode()).as_deref(),
            Some("-R/proj/x/input.txt/-/identity")
        );
        Ok(())
    }

    /// A plan serialized before `prologue_steps` existed loads at the pre-change value.
    #[test]
    fn prologue_steps_defaults_on_a_legacy_plan() -> Result<(), Box<dyn std::error::Error>> {
        let plan = Plan::new();
        let mut json = serde_json::to_value(&plan)?;
        if let Some(object) = json.as_object_mut() {
            object.remove("prologue_steps");
        }
        let back: Plan = serde_json::from_value(json)?;
        assert_eq!(back.prologue_steps, 0);
        Ok(())
    }

    // --- volatility scope (predecessor-cut-equivalence step 2) -----------------------------

    fn scope_registry() -> CommandMetadataRegistry {
        let mut cmr = CommandMetadataRegistry::new();
        for name in ["prefix", "tail", "render"] {
            cmr.add_command(&CommandMetadata::new(name));
        }
        let mut vol = CommandMetadata::new("vol_cmd");
        vol.volatile = true;
        cmr.add_command(&vol);
        cmr
    }

    fn source_of(query: &str, cmr: &CommandMetadataRegistry) -> Option<VolatilitySource> {
        PlanBuilder::new(parse_query(query).unwrap(), cmr)
            .build()
            .unwrap()
            .volatility_source
    }

    /// A volatile command is positional: everything ahead of it is pure, so a boundary may be
    /// cut in front of it.
    #[test]
    fn a_volatile_command_is_positional() {
        let cmr = scope_registry();
        assert_eq!(source_of("prefix/tail", &cmr), None);
        assert_eq!(
            source_of("prefix/vol_cmd/tail", &cmr),
            Some(VolatilitySource::Positional)
        );
    }

    /// `v` is a statement about the whole plan, wherever it sits.
    #[test]
    fn the_v_instruction_is_declared() {
        let cmr = scope_registry();
        for query in ["v/prefix/tail", "prefix/v/tail", "prefix/tail/v"] {
            assert_eq!(
                source_of(query, &cmr),
                Some(VolatilitySource::Declared),
                "`v` declares whole-plan volatility in {query}"
            );
        }
    }

    /// The trap: `mark_volatile` records its *reason* only when the plan is not already
    /// volatile. If the scope upgrade sat inside that early-out, a `v` following a volatile
    /// command would be swallowed, the plan would look positional, and a boundary would be cut
    /// out of a plan that declares nothing is cacheable.
    #[test]
    fn a_declared_source_survives_an_earlier_positional_one() {
        let cmr = scope_registry();
        assert_eq!(
            source_of("vol_cmd/v/tail", &cmr),
            Some(VolatilitySource::Declared),
            "`Declared` must outrank a `Positional` source recorded before it"
        );
        // ...and the reverse order must not weaken it either.
        assert_eq!(
            source_of("v/vol_cmd/tail", &cmr),
            Some(VolatilitySource::Declared)
        );
    }

    /// The flags the placement walk reads are cumulative per prefix and monotone, which is what
    /// lets the outermost cacheable prefix be identified at all.
    #[test]
    fn volatility_is_monotone_along_the_chain() {
        let cmr = scope_registry();
        assert!(
            !PlanBuilder::new(parse_query("prefix").unwrap(), &cmr)
                .build()
                .unwrap()
                .is_volatile
        );
        for query in [
            "prefix/vol_cmd",
            "prefix/vol_cmd/tail",
            "prefix/vol_cmd/tail/render",
        ] {
            assert!(
                PlanBuilder::new(parse_query(query).unwrap(), &cmr)
                    .build()
                    .unwrap()
                    .is_volatile,
                "{query} is volatile once vol_cmd is in it"
            );
        }
    }

    /// A plan serialized before the field existed loads with no recorded source.
    #[test]
    fn volatility_source_defaults_on_a_legacy_plan() -> Result<(), Box<dyn std::error::Error>> {
        let plan = Plan::new();
        let mut json = serde_json::to_value(&plan)?;
        if let Some(object) = json.as_object_mut() {
            object.remove("volatility_source");
        }
        let back: Plan = serde_json::from_value(json)?;
        assert_eq!(back.volatility_source, None);
        Ok(())
    }

    // --- the recipe fold (predecessor-cut-equivalence step 3) -------------------------------

    fn fold_registry() -> CommandMetadataRegistry {
        let mut cmr = CommandMetadataRegistry::new();
        for name in ["prefix", "tail"] {
            cmr.add_command(&CommandMetadata::new(name));
        }
        cmr
    }

    fn folded_plan(volatile: bool, expires: &str) -> Result<Plan, Box<dyn std::error::Error>> {
        let cmr = fold_registry();
        let mut recipe = Recipe::new(
            "prefix/tail/out.txt".to_owned(),
            String::new(),
            String::new(),
        )?;
        recipe.volatile = volatile;
        if !expires.is_empty() {
            recipe.expires = expires.parse()?;
        }
        Ok(recipe.to_plan(&cmr)?)
    }

    /// `Recipe::volatile` reaches the plan, and does so as a whole-plan declaration.
    ///
    /// Before this, `to_plan` read neither of its own declarations: a `volatile: true` recipe
    /// produced `is_volatile == false`, so a recipe preview under-reported it and no consumer
    /// could ask the plan whether it was volatile.
    #[test]
    fn to_plan_folds_recipe_volatility() -> Result<(), Box<dyn std::error::Error>> {
        let plan = folded_plan(true, "")?;
        assert!(plan.is_volatile, "a volatile recipe makes a volatile plan");
        assert_eq!(plan.volatility_source, Some(VolatilitySource::Declared));

        let plain = folded_plan(false, "")?;
        assert!(!plain.is_volatile);
        assert_eq!(plain.volatility_source, None);
        Ok(())
    }

    /// `Recipe::expires` is combined into the plan, as its own documentation promised.
    #[test]
    fn to_plan_combines_recipe_expiration() -> Result<(), Box<dyn std::error::Error>> {
        let plan = folded_plan(false, "in 5 minutes")?;
        assert!(
            !plan.expires.is_never(),
            "a finite recipe expiration reaches the plan: {:?}",
            plan.expires
        );
        Ok(())
    }

    /// A *finite* expiration bounds how long the result stays valid; it says nothing about the
    /// purity of the computation, so it must not make the plan uncuttable. Only an expiration
    /// that is itself volatile does.
    #[test]
    fn finite_expiration_does_not_declare_volatility() -> Result<(), Box<dyn std::error::Error>> {
        let finite = folded_plan(false, "in 5 minutes")?;
        assert!(!finite.is_volatile, "a finite expiration is not volatility");
        assert_eq!(finite.volatility_source, None);

        let immediate = folded_plan(false, "immediately")?;
        assert!(immediate.is_volatile);
        assert_eq!(
            immediate.volatility_source,
            Some(VolatilitySource::Declared),
            "an Immediately expiration is volatile, and whole-plan"
        );
        Ok(())
    }

    // --- split and consistency (predecessor-cut-equivalence step 4) -------------------------

    /// Both halves keep the facts that are true of each independently, and neither claims a
    /// predecessor boundary — a fragment has none.
    #[test]
    fn split_carries_frozen_cwd_and_clears_predecessor() -> Result<(), Box<dyn std::error::Error>> {
        let cmr = fold_registry();
        let mut recipe = Recipe::new(
            "prefix/tail/out.txt".to_owned(),
            String::new(),
            String::new(),
        )?;
        recipe.cwd = Some("a/c".to_owned());
        recipe.volatile = true;
        let mut plan = recipe.to_plan(&cmr)?;
        plan.freeze_cwd(None)?;
        assert!(plan.frozen_cwd.is_some() && plan.predecessor.is_some());

        let (first, second) = plan.split();
        for (label, half) in [("first", &first), ("second", &second)] {
            assert!(half.frozen_cwd.is_some(), "{label} half stays frozen");
            assert_eq!(
                half.volatility_source,
                Some(VolatilitySource::Declared),
                "{label} half keeps the plan's volatility source"
            );
            assert!(
                half.predecessor.is_none(),
                "{label} half claims no boundary"
            );
            assert_eq!(half.predecessor_steps, 0);
            half.check_consistent()?;
        }
        assert_eq!(first.prologue_steps, 1, "the prefix is in the first half");
        assert_eq!(second.prologue_steps, 0);
        Ok(())
    }

    /// The split point and the recorded predecessor range coincide on every shape tried — which
    /// is *why* carrying `predecessor` into the first half would be wrong, and is pinned here
    /// rather than relied on silently.
    #[test]
    fn split_index_equals_predecessor_steps() -> Result<(), Box<dyn std::error::Error>> {
        let cmr = fold_registry();
        for (query, cwd) in [
            ("prefix/tail", None),
            ("prefix/tail/out.txt", None),
            ("prefix/tail", Some("a/c")),
            ("-R/./x.txt/-/prefix/tail", Some("a/c")),
        ] {
            let mut recipe = Recipe::new(query.to_owned(), String::new(), String::new())?;
            recipe.cwd = cwd.map(|c| c.to_owned());
            let plan = recipe.to_plan(&cmr)?;
            if plan.predecessor.is_none() {
                continue;
            }
            assert_eq!(
                plan.split_index(),
                plan.predecessor_steps,
                "split point and predecessor range diverged for {query}"
            );
        }
        Ok(())
    }

    /// A stale range is an error at its source, not a panic and not a wrong cut.
    #[test]
    fn check_consistent_rejects_a_stale_range() -> Result<(), Box<dyn std::error::Error>> {
        let cmr = fold_registry();
        let recipe = Recipe::new("prefix/tail".to_owned(), String::new(), String::new())?;
        let mut plan = recipe.to_plan(&cmr)?;
        plan.check_consistent()?;

        plan.predecessor_steps = plan.steps.len() + 1;
        let error = plan
            .check_consistent()
            .expect_err("a range past the last step is inconsistent");
        assert!(
            error.message.contains("predecessor range"),
            "{}",
            error.message
        );

        let mut prologue = recipe.to_plan(&cmr)?;
        prologue.prologue_steps = prologue.steps.len() + 1;
        let error = prologue
            .check_consistent()
            .expect_err("a prologue longer than the plan is inconsistent");
        assert!(error.message.contains("prologue"), "{}", error.message);
        Ok(())
    }

    // --- the placement walk (predecessor-cut-equivalence step 6) ----------------------------

    fn walk_registry() -> CommandMetadataRegistry {
        let mut cmr = CommandMetadataRegistry::new();
        for name in ["fetch", "expensive", "render", "tail"] {
            cmr.add_command(&CommandMetadata::new(name));
        }
        let mut pay = CommandMetadata::new("personalize");
        pay.payload_required = PayloadRequirement::Required;
        pay.volatile = true; // register_command! sets both together; mirror it
        cmr.add_command(&pay);
        let mut vol = CommandMetadata::new("vol_step");
        vol.volatile = true;
        cmr.add_command(&vol);
        cmr
    }

    fn cut_of(query: &str, cmr: &CommandMetadataRegistry) -> Result<Plan, Error> {
        let mut plan = PlanBuilder::new(parse_query(query)?, cmr).build()?;
        plan.freeze_cwd(None)?;
        plan.cut_predecessor(cmr)?;
        Ok(plan)
    }

    fn boundary_of(plan: &Plan) -> Option<String> {
        plan.steps.iter().find_map(|step| match step {
            Step::Evaluate(query) => Some(query.encode()),
            _other => None,
        })
    }

    /// The base case: nothing is in the way, so the boundary is the whole recorded predecessor.
    #[test]
    fn the_walk_cuts_the_outermost_candidate() -> Result<(), Box<dyn std::error::Error>> {
        let cmr = walk_registry();
        let plan = cut_of("fetch/expensive/render", &cmr)?;
        assert_eq!(boundary_of(&plan).as_deref(), Some("fetch/expensive"));
        Ok(())
    }

    /// A payload-requiring candidate cannot be a cache entry, so the walk steps back past it
    /// and says so.
    #[test]
    fn the_walk_steps_back_past_a_payload() -> Result<(), Box<dyn std::error::Error>> {
        let cmr = walk_registry();
        let plan = cut_of("fetch/personalize/render", &cmr)?;
        assert_eq!(
            boundary_of(&plan).as_deref(),
            Some("fetch"),
            "the boundary lands in front of the payload, not across it"
        );
        assert!(
            plan.init_steps
                .iter()
                .any(|step| matches!(step, Step::Info(m)
                if m.contains("fetch/personalize") && m.contains("payload"))),
            "the reason is recorded: {:?}",
            plan.init_steps
        );
        Ok(())
    }

    /// Volatility is the same predicate on the same candidate plan.
    #[test]
    fn the_walk_steps_back_past_a_volatile_candidate() -> Result<(), Box<dyn std::error::Error>> {
        let cmr = walk_registry();
        let plan = cut_of("fetch/vol_step/render", &cmr)?;
        assert_eq!(boundary_of(&plan).as_deref(), Some("fetch"));
        Ok(())
    }

    /// When the obstacle reaches the head, no boundary at any position is safe.
    #[test]
    fn no_cut_when_the_obstacle_reaches_the_head() -> Result<(), Box<dyn std::error::Error>> {
        let cmr = walk_registry();
        let plan = cut_of("personalize/fetch/render", &cmr)?;
        assert_eq!(boundary_of(&plan), None, "nothing cacheable to cut");
        Ok(())
    }

    /// Whole-plan volatility declines before the walk starts: it appears in no candidate query,
    /// so the walk could not see it, and it says nothing here is cacheable.
    #[test]
    fn declared_volatility_declines_before_the_walk() -> Result<(), Box<dyn std::error::Error>> {
        let cmr = walk_registry();
        for query in ["fetch/v/expensive/render", "fetch/expensive/render/v"] {
            let plan = cut_of(query, &cmr)?;
            assert_eq!(boundary_of(&plan), None, "no boundary in {query}");
            assert!(
                plan.init_steps
                    .iter()
                    .any(|step| matches!(step, Step::Info(m)
                    if m.contains("declared volatile"))),
                "the decline is recorded for {query}"
            );
        }
        Ok(())
    }

    /// A trailing filename is not an action, so the candidate that would swallow the last real
    /// action is never chosen: the parent keeps an action to carry a recipe's overrides.
    #[test]
    fn a_filename_candidate_is_not_chosen() -> Result<(), Box<dyn std::error::Error>> {
        let cmr = walk_registry();
        let plan = cut_of("fetch/expensive/render/out.txt", &cmr)?;
        assert_eq!(boundary_of(&plan).as_deref(), Some("fetch/expensive"));
        assert!(matches!(plan.steps.last(), Some(Step::Filename(_))));
        assert_eq!(
            plan.steps
                .iter()
                .filter(|step| matches!(step, Step::Action { .. }))
                .count(),
            1,
            "the last action stays in the parent: {:?}",
            plan.steps
        );
        Ok(())
    }

    /// A boundary covering every step leaves the parent empty — the whole plan replaced by
    /// something that recomputes it. Declined, and pinned independently of the `Declared` rule
    /// because a positional `v` would reopen the case.
    #[test]
    fn a_whole_plan_cut_is_declined() -> Result<(), Box<dyn std::error::Error>> {
        let cmr = walk_registry();
        let mut plan = PlanBuilder::new(parse_query("fetch/tail")?, &cmr).build()?;
        plan.freeze_cwd(None)?;
        // Force the degenerate shape the `>=` guard exists for.
        plan.predecessor_steps = plan.steps.len();
        assert!(!plan.cut_predecessor(&cmr)?, "an empty tail is not a cut");
        assert_eq!(boundary_of(&plan), None);
        Ok(())
    }

    /// A recorded range that no longer matches what the query builds would split in the wrong
    /// place, so the cut declines rather than risking a duplicated action.
    #[test]
    fn a_stale_range_declines_rather_than_mis_splitting() -> Result<(), Box<dyn std::error::Error>>
    {
        let cmr = walk_registry();
        let mut plan = PlanBuilder::new(parse_query("fetch/expensive/render")?, &cmr).build()?;
        plan.freeze_cwd(None)?;
        plan.predecessor_steps = 1; // claims `fetch/expensive` is one step; it is two
        assert!(!plan.cut_predecessor(&cmr)?);
        assert_eq!(boundary_of(&plan), None);
        Ok(())
    }
}
