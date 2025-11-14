#![allow(unused_imports)]
#![allow(dead_code)]

use std::clone;
use std::fmt::Display;
use std::ops::Index;

use itertools::Itertools;
use nom::Err;
use serde_json::Value;

use crate::command_metadata::{
    self, ArgumentInfo, ArgumentType, CommandKey, CommandMetadata, CommandMetadataRegistry,
    CommandParameterValue, EnumArgumentType,
};
use crate::context::EnvRef;
use crate::error::{Error, ErrorType};
use crate::query::{
    ActionParameter, ActionRequest, Key, Position, Query, QuerySegment, ResourceName,
    ResourceQuerySegment,
};
use crate::value::ValueInterface;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Step {
    GetAsset(Key),
    GetAssetBinary(Key),
    GetAssetMetadata(Key),
    GetResource(Key),
    GetResourceMetadata(Key),
    Evaluate(Query),
    Action {
        realm: String,
        ns: String,
        action_name: String,
        position: Position,
        parameters: ResolvedParameterValues,
    },
    Filename(ResourceName),
    Info(String),
    Warning(String),
    Error(String),
    Plan(Plan),
    SetCwd(Key),
    UseKeyValue(Key),
}

impl Step {
    pub fn is_error(&self) -> bool {
        match self {
            Step::Error(_) => true,
            _ => false,
        }
    }
    pub fn is_warning(&self) -> bool {
        match self {
            Step::Warning(_) => true,
            _ => false,
        }
    }
    /// Returns true if this step is an action step,
    pub fn is_action(&self) -> bool {
        match self {
            Step::Action { .. } => true,
            _ => false,
        }
    }

    /// Returns true is this step just modifies the context,
    /// i.e. logging operation, changing cwd or filename
    /// and does not produce any data.
    pub fn is_context_modifier(&self) -> bool {
        match self {
            Step::GetAsset(_key) => false,
            Step::GetAssetBinary(_key) => false,
            Step::GetAssetMetadata(_key) => false,
            Step::GetResource(_key) => false,
            Step::GetResourceMetadata(_key) => false,
            Step::Evaluate(_) => false,
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


/// Parameter value contains the partially or fully resolved value of a single command parameter.
/// Parameter values are passed to the command executor when the command is executed.
/// There are four variants of parameter value:
/// - JSON value - directly containing the parameter value. To keep things simple, it is represented as a serde_json::Value.
/// - Link - a query that will be executed to get the parameter value. The query is resolved before the command is executed.
/// - Injected - the parameter value is injected by the environment.
/// - Multiple parameters - a vector of parameter values. This is used for vector arguments - i.e. when ArgumentInfo::multiple is set.
/// - None - the parameter value is not set. This is a temporary state. Plan should never contain a parameter with None value.
///
/// Parameter can be obtained from several sources:
/// - Resolved from the action request. This is the most common case.
/// - Default value from the command metadata (ArgumentInfo). This is used when ActionRequest does not have enough parameters
/// or (depending on a type) when ActionParameter is an empty string.
/// - From a link passed as an ActionParameter.
/// - From a value or link that is an enum value for the ActionParameter.
/// - Injected from an environment. Whether a parameter is injected is determined by its type and is set in the ArgumentInfo.
/// Supported scalar values (string, integer, optional integer, float, optional float, boolean, enum and any) are never injected,
/// all other types are always injected. Note that any is a Environment::Value type.
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
    /// Override value of the parameter, e.g. from a recipe
    OverrideValue(String, Value),
    /// Override link of the parameter, e.g. from a recipe
    OverrideLink(String, Query),
    /// Parameter placeholder - when neither the default nor the resolved value is set, but override is expected.
    Placeholder(String),
    /// Enum link - when parameter enum value maps to a link
    EnumLink(String, Query, Position),
    /// Multiple parameters - used for vector arguments
    MultipleParameters(Vec<ParameterValue>),
    /// Injected parameter value
    Injected(String),
    /// Parameter value is not set
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
            ParameterValue::MultipleParameters(v) => {
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
            ParameterValue::MultipleParameters(values)
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

    pub fn to_result(self, error: impl Fn() -> String, position: &Position) -> Result<Self, Error> {
        match self {
            ParameterValue::None => {
                Err(Error::new(ErrorType::ArgumentMissing, error()).with_position(position))
            }
            _ => Ok(self),
        }
    }

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
                        Ok(ParameterValue::ParameterValue(
                            arginfo.name.clone(),
                            Value::String(s.to_owned()),
                            pos.to_owned(),
                        ))
                    } else {
                        Err(Error::conversion_error_with_message(
                            s.to_owned(),
                            &e.name,
                            &format!("Undefined enum {} in argument {}", e.name, arginfo.name),
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
                            ParameterValue::MultipleParameters(_) => {
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
            return Ok(ParameterValue::MultipleParameters(values));
        }

        match param.next() {
            Some(ActionParameter::String(s, pos)) => Self::from_string(arginfo, s, pos),
            Some(ActionParameter::Link(q, pos)) => Ok(ParameterValue::ParameterLink(
                arginfo.name.clone(),
                q.clone(),
                pos.clone(),
            )),
            None => {
                if allow_placeholders {
                    Ok(ParameterValue::Placeholder(arginfo.name.clone()))
                } else {
                    Self::from_arginfo(arginfo).to_result(
                        || format!("Missing argument '{}'", arginfo.name),
                        &param.position,
                    )
                }
            }
        }
    }
    pub fn is_default(&self) -> bool {
        match self {
            ParameterValue::DefaultValue(_, _) => true,
            ParameterValue::DefaultLink(_, _) => true,
            _ => false,
        }
    }
    pub fn is_none(&self) -> bool {
        match self {
            ParameterValue::None => true,
            _ => false,
        }
    }
    pub fn is_link(&self) -> bool {
        match self {
            ParameterValue::DefaultLink(_, _) => true,
            ParameterValue::ParameterLink(_, _, _) => true,
            ParameterValue::OverrideLink(_, _) => true,
            ParameterValue::EnumLink(_, _, _) => true,
            _ => false,
        }
    }
    pub fn is_injected(&self) -> bool {
        match self {
            ParameterValue::Injected(_) => true,
            _ => false,
        }
    }
    pub fn is_multiple(&self) -> bool {
        match self {
            ParameterValue::MultipleParameters(_) => true,
            _ => false,
        }
    }
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
            _ => None,
        }
    }
    pub fn value(&self) -> Option<Value> {
        match self {
            ParameterValue::DefaultValue(_, v) => Some(v.clone()),
            ParameterValue::ParameterValue(_, v, _) => Some(v.clone()),
            ParameterValue::OverrideValue(_, v) => Some(v.clone()),
            _ => None,
        }
    }
    pub fn link(&self) -> Option<Query> {
        match self {
            ParameterValue::DefaultLink(_, q) => Some(q.clone()),
            ParameterValue::ParameterLink(_, q, _) => Some(q.clone()),
            ParameterValue::OverrideLink(_, q) => Some(q.clone()),
            ParameterValue::EnumLink(_, q, _) => Some(q.clone()),
            _ => None,
        }
    }
    pub fn multiple(&self) -> Option<Vec<ParameterValue>> {
        match self {
            ParameterValue::MultipleParameters(v) => Some(v.clone()),
            _ => None,
        }
    }
    pub fn position(&self) -> Position {
        match self {
            ParameterValue::ParameterValue(_, _, pos) => pos.clone(),
            ParameterValue::ParameterLink(_, _, pos) => pos.clone(),
            ParameterValue::EnumLink(_, _, pos) => pos.clone(),
            _ => Position::unknown(),
        }
    }
}

/// ResolvedParameterValues contains the resolved values of all command parameters.
/// It is used in a Plan to define (resolved) parameters of an action.
/// Injected parameter values are (of course) not included in ResolvedParameterValues,
/// but they are marked with an Injected parameter value placeholder.
/// ResolvedParameterValues is created from an ActionRequest and CommandMetadata.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ResolvedParameterValues(pub Vec<ParameterValue>);
impl Default for ResolvedParameterValues {
    fn default() -> Self {
        Self::new()
    }
}

impl ResolvedParameterValues {
    pub fn new() -> Self {
        ResolvedParameterValues(Vec::new())
    }
    /// Create ResolvedParameterValues from an ActionRequest.
    /// The command metadata is used to determine the default values of the parameters.
    /// Additional source of parameters are the head_parameters,
    /// filling the parameter slots at the beginning of the parameter list.
    /// The head_parameters are used for an alias command.
    /// If allow_placeholders is true, the missing parameters are replaced with a placeholder,
    /// otherwise an error is returned. This is used when parameters are expected to be overriden e.g. for
    /// - recipes with parameters defined inside the recipe
    /// - calling a query as a service
    /// - passing user-defined arguments into a query in general.
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
        Ok(ResolvedParameterValues(values))
    }
    pub fn from_action(
        action_request: &ActionRequest,
        command_metadata: &CommandMetadata,
        allow_placeholders: bool,
    ) -> Result<Self, Error> {
        Self::from_action_extended(action_request, command_metadata, &[], allow_placeholders)
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn override_value(&mut self, name: &str, value: Value) -> bool {
        for pv in &mut self.0 {
            if let Some(n) = pv.name() {
                if n == name {
                    if pv.is_injected() {
                        // TODO: maybe this could be an error
                        return false;
                    }
                    *pv = ParameterValue::OverrideValue(n.clone(), value.clone());
                    return true;
                }
            }
        }
        false
    }
    pub fn override_link(&mut self, name: &str, query: Query) -> bool {
        for pv in &mut self.0 {
            if let Some(n) = pv.name() {
                if n == name {
                    if pv.is_injected() {
                        // TODO: maybe this could be an error
                        return false;
                    }
                    *pv = ParameterValue::OverrideLink(n.clone(), query.clone());
                    return true;
                }
            }
        }
        false
    }


    pub fn iter(&self) -> std::slice::Iter<'_, ParameterValue> {
        self.0.iter()
    }

    pub fn iter_mut(&mut self) -> std::slice::IterMut<'_, ParameterValue> {
        self.0.iter_mut()
    }

    pub fn get(&self, index: usize) -> Option<&ParameterValue> {
        self.0.get(index)
    }

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

pub struct ActionParameterIterator<'a> {
    pub action_request: &'a ActionRequest,
    pub parameter_number: usize,
    pub position: Position,
}

impl<'a> ActionParameterIterator<'a> {
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

pub struct PlanBuilder<'c> {
    query: Query,
    command_registry: &'c CommandMetadataRegistry,
    plan: Plan,
    allow_placeholders: bool,
    expand_predecessors: bool,
}

// TODO: support cache
// TODO: support volatile flags
// TODO: support inline flag
impl<'c> PlanBuilder<'c> {
    pub fn new(query: Query, command_registry: &'c CommandMetadataRegistry) -> Self {
        PlanBuilder {
            query,
            command_registry,
            plan: Plan::new(),
            allow_placeholders: false,
            expand_predecessors: true, // TODO: expand_predecessors should be false by default
        }
    }
    pub fn with_placeholders_allowed(mut self) -> Self {
        self.allow_placeholders = true;
        self
    }
    pub fn expand_predecessors(mut self) -> Self {
        self.expand_predecessors = true;
        self
    }
    pub fn disable_expand_predecessors(mut self) -> Self {
        self.expand_predecessors = false;
        self
    }

    pub fn build(&mut self) -> Result<Plan, Error> {
        let query = self.query.clone();
        self.plan.query = query.clone();
        self.process_query(&query)?;
        Ok(self.plan.clone())
    }

    fn get_namespaces(&self, query: &Query) -> Result<Vec<String>, Error> {
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
        self.command_registry
            .default_namespaces
            .iter()
            .for_each(|x| {
                namespaces.push(x.clone());
            });

        // TODO: check if the namespaces are registered in command registry
        Ok(namespaces)
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
                self.plan.steps.push(Step::Warning(format!(
                    "Resource header name is ignored: '{}'",
                    header.name
                )));
            }
            if header.parameters.is_empty() {
                self.plan.steps.push(Step::GetAsset(rqs.key.clone()));
            } else {
                if header.parameters.len() > 1 {
                    self.plan.steps.push(Step::Warning(format!(
                        "Resource header has too many parameters: {}, extra parameters are ignored",
                        header.parameters.len()
                    )));
                }

                match header.parameters.first().unwrap().value.as_str() {
                    "b" | "bin" | "binary" => {
                        self.plan.steps.push(Step::GetAssetBinary(rqs.key.clone()));
                    }
                    "meta" | "metadata" => {
                        self.plan
                            .steps
                            .push(Step::GetAssetMetadata(rqs.key.clone()));
                    }
                    "data" | "value" => {
                        self.plan.steps.push(Step::GetAsset(rqs.key.clone()));
                    }
                    "stored" | "stored_binary" | "stored_bin" => {
                        self.plan.steps.push(Step::GetResource(rqs.key.clone()));
                    }
                    "stored_meta" => {
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
                        return Err(Error::not_supported(
                            "Resource header parameters must be string or link".to_string(),
                        ));
                    }
                }
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
        let command_metadata = self.get_command_metadata(query, action_request)?;

        match &command_metadata.definition {
            command_metadata::CommandDefinition::Registered => {
                self.plan.steps.push(Step::Action {
                    realm: command_metadata.realm.clone(),
                    ns: command_metadata.namespace.clone(),
                    action_name: action_request.name.clone(),
                    position: action_request.position.clone(),
                    parameters: ResolvedParameterValues::from_action(
                        action_request,
                        &command_metadata,
                        self.allow_placeholders,
                    )?,
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
                self.plan.steps.push(Step::Action {
                    realm: command.realm.clone(),
                    ns: command.namespace.clone(),
                    action_name: command.name.clone(),
                    position: action_request.position.clone(),
                    parameters: ResolvedParameterValues::from_action_extended(
                        action_request,
                        &command_metadata,
                        head_parameters,
                        self.allow_placeholders,
                    )?,
                });
            }
        }

        Ok(())
    }

    fn process_query(&mut self, query: &Query) -> Result<(), Error> {
        //println!("process query {}", query);
        if query.is_empty() || query.is_ns() {
            println!("empty or ns");
            return Ok(());
        }
        if let Some(rq) = query.resource_query() {
            //println!("RESOURCE {}", rq);
            self.process_resource_query(&rq)?;
            return Ok(());
        }
        if let Some(transform) = query.transform_query() {
            //println!("TRANSFORM {}", &transform);
            if let Some(action) = transform.action() {
                println!("ACTION {}", &action);
                let mut query = query.clone();
                query.segments = Vec::new();
                self.process_action(&query, &action)?;
                return Ok(());
            }
            if transform.is_filename() {
                //println!("FILENAME {}", &transform);
                self.plan
                    .steps
                    .push(Step::Filename(transform.filename.unwrap().clone()));
                return Ok(());
            }
            println!("Longer transform query");
        }

        let (p, q) = query.predecessor();
        //println!("PREDECESOR: {:?}", &p);
        //println!("REMAINDER:  {:?}", &q);

        if let Some(p) = p.as_ref() {
            if !p.is_empty() {
                if self.expand_predecessors {
                    self.process_query(p)?;
                } else {
                    self.plan.steps.push(Step::Evaluate(p.clone()));
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
                    if tqs.is_empty() || tqs.is_ns() {
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

    pub fn override_value(&mut self, name: &str, value: Value) -> bool {
        self.plan.override_value(name, value)
    }

    pub fn override_link(&mut self, name: &str, query: Query) -> bool {
        self.plan.override_link(name, query)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Plan {
    pub query: Query,
    pub steps: Vec<Step>,
}

impl Default for Plan {
    fn default() -> Self {
        Self::new()
    }
}

impl Plan {
    pub fn new() -> Self {
        Plan {
            query: Query::new(),
            steps: Vec::new(),
        }
    }
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }
    pub fn info(&mut self, message: String) {
        self.steps.push(Step::Info(message));
    }
    pub fn warning(&mut self, message: String) {
        self.steps.push(Step::Warning(message));
    }
    pub fn error(&mut self, message: String) {
        self.steps.push(Step::Error(message));
    }
    pub fn has_error(&self) -> bool {
        self.steps.iter().any(|x| x.is_error())
    }
    pub fn has_warning(&self) -> bool {
        self.steps.iter().any(|x| x.is_warning())
    }
    pub fn len(&self) -> usize {
        self.steps.len()
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

    pub fn override_value(&mut self, name: &str, value: Value) -> bool {
        if let Some(i) = self.last_action_index() {
            if let Step::Action { parameters, .. } = &mut self.steps[i] {
                return parameters.override_value(name, value);
            }
        }
        false
    }

    pub fn override_link(&mut self, name: &str, query: Query) -> bool {
        if let Some(i) = self.last_action_index() {
            if let Step::Action { parameters, .. } = &mut self.steps[i] {
                return parameters.override_link(name, query);
            }
        }
        false
    }

    /// Find the index to split the plan.
    /// Plan after the split index should contain only context modifiers and at most one action step.
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

    /// Split the plan into two plans.
    /// First plan is being the state argument dependency for the second plan.
    /// The second plan should contain at most one action step.
    pub fn split(&self) -> (Plan, Plan) {
        if self.is_empty() {
            return (Plan::new(), Plan::new());
        }

        let split_index = self.split_index();
        if split_index == 0 {
            return (Plan::new(), self.clone());
        }
        let mut first_plan = Plan::new();
        first_plan.query = self.query.clone();
        first_plan.steps = self.steps[..split_index].to_vec();
        let mut second_plan = Plan::new();
        second_plan.query = self.query.clone();
        second_plan.steps = self.steps[split_index..].to_vec();
        (first_plan, second_plan)
    }

}

impl Index<usize> for Plan {
    type Output = Step;
    fn index(&self, index: usize) -> &Self::Output {
        &self.steps[index]
    }
}

#[cfg(test)]
mod tests {
    use crate::command_metadata::*;
    use crate::parse::parse_query;
    use crate::query::TryToQuery;
    use serde_yaml;

    use super::*;

    #[test]
    fn first_test() {
        let mut cr = command_metadata::CommandMetadataRegistry::new();
        cr.add_command(CommandMetadata::new("a").with_argument(ArgumentInfo::any_argument("a")));
        let plan = PlanBuilder::new(parse_query("a-1").unwrap(), &cr)
            .build()
            .unwrap();
        println!("plan: {:?}", plan);
        print!("");
        println!("plan.yaml:\n{}", serde_yaml::to_string(&plan).unwrap());
        print!("");
        println!(
            "command_registry.yaml:\n{}",
            serde_yaml::to_string(&cr).unwrap()
        );
        print!("");
        println!("plan.json:\n{}", serde_json::to_string(&plan).unwrap());
        print!("");
        println!(
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
        println!("plan: {:?}", plan);
        print!("");
        println!("plan.yaml:\n{}", serde_yaml::to_string(&plan).unwrap());
        println!("plan.json:\n{}", serde_json::to_string(&plan).unwrap());
        print!("");
        println!(
            "command_registry.yaml:\n{}",
            serde_yaml::to_string(&cr).unwrap()
        );
        print!("");
        println!("plan.json:\n{}", serde_json::to_string(&plan).unwrap());
        print!("");
        println!(
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
        println!("plan.yaml:\n{}", serde_yaml::to_string(&plan).unwrap());
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
            steps: vec![
                Step::Info("info".to_string()),
                Step::Warning("warn".to_string()),
                Step::Error("err".to_string()),
            ],
        };
        assert_eq!(plan.split_index(), 0);
        let (p1, p2) = plan.split();
        assert!(p1.is_empty());
        assert_eq!(p2.len(), 3);

        // Plan with one action at the start
        let plan = Plan {
            query: Default::default(),
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
        };
        assert_eq!(plan.split_index(), 0);
        let (p1, p2) = plan.split();
        assert!(p1.is_empty());
        assert_eq!(p2.len(), 2);

        // Plan with context modifiers before and after an action
        let plan = Plan {
            query: Default::default(),
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
        };
        assert_eq!(plan.split_index(), 0);
        let (p1, p2) = plan.split();
        assert!(p1.is_empty());
        assert_eq!(p2.len(), 5);

        // Plan with a non-context-modifier before the action
        let plan = Plan {
            query: Default::default(),
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
        };
        assert_eq!(plan.split_index(), 1);
        let (p1, p2) = plan.split();
        assert_eq!(p1.len(), 1);
        assert_eq!(p2.len(), 3);
        assert!(p2[0].is_context_modifier());
        assert!(p2[1].is_action());
        assert!(p2[2].is_context_modifier());

        // Plan with two actions
        println!("### Testing plan with two actions");
        let plan = Plan {
            query: Default::default(),
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
            steps: vec![Step::Evaluate(Default::default())],
        };
        assert_eq!(plan.split_index(), 1);
        let (p1, p2) = plan.split();
        assert_eq!(p1.len(), 1);
        assert_eq!(p2.len(), 0);
    }
}
