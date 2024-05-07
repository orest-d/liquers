#![allow(unused_imports)]
#![allow(dead_code)]

use std::clone;
use std::fmt::Display;

use itertools::Itertools;
use nom::Err;
use serde_json::Value;

use crate::command_metadata::{
    self, ArgumentInfo, ArgumentType, CommandMetadata, CommandMetadataRegistry,
    CommandParameterValue, EnumArgumentType,
};
use crate::error::{Error, ErrorType};
use crate::query::{
    ActionParameter, ActionRequest, Key, Position, Query, QuerySegment, ResourceName,
    ResourceQuerySegment,
};
use crate::value::ValueInterface;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Step {
    GetResource(Key),
    GetResourceMetadata(Key),
    GetNamedResource(Key),
    GetNamedResourceMetadata(Key),
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
}

/// Parameter value contains the partially or fully resolved value of a single command parameter.
/// Parameter values are passed to the command executor when the command is executed.
/// There are four variants of parameter value:
/// - JSON value - directly containing the parameter value. To keep things simple, it is represented as a serde_json::Value.
/// - Link - a query that will be executed to get the parameter value. The query is resolved before the command is executed.
/// - Injected - the parameter value is injected by the environment.
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
    DefaultValue(Value),
    DefaultLink(Query),
    ParameterValue(Value, Position),
    ParameterLink(Query, Position),
    EnumLink(Query, Position),
    Injected,
    None,
}

impl Display for ParameterValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParameterValue::DefaultValue(v) => write!(f, "default:{}", v),
            ParameterValue::DefaultLink(q) => write!(f, "default link:{}", q.encode()),
            ParameterValue::ParameterValue(v, _) => write!(f, "value:{}", v),
            ParameterValue::ParameterLink(q, _) => write!(f, "link:{}", q.encode()),
            ParameterValue::EnumLink(q, _) => write!(f, "enum link:{}", q.encode()),
            ParameterValue::Injected => write!(f, "injected value"),
            ParameterValue::None => write!(f, "None"),
        }
    }
}

impl ParameterValue {
    pub fn from_arginfo(arginfo: &ArgumentInfo) -> Self {
        match &arginfo.default {
            CommandParameterValue::Value(x) => ParameterValue::DefaultValue(x.clone()),
            CommandParameterValue::Query(q) => ParameterValue::DefaultLink(q.clone()),
            CommandParameterValue::None => {
                if arginfo.injected {
                    ParameterValue::Injected
                } else {
                    ParameterValue::None
                }
            }
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
                    .map_err(|e| Error::conversion_error_at_position(s, "integer", pos))?;
                Ok(ParameterValue::ParameterValue(n.into(), pos.to_owned()))
            }
            ArgumentType::IntegerOption => {
                if s.is_empty() {
                    let res = Self::from_arginfo(arginfo);
                    if res.is_none() {
                        return Ok(Self::ParameterValue(Value::Null, pos.to_owned()));
                    } else {
                        return Ok(res);
                    }
                }
                let n = s
                    .parse::<i64>()
                    .map_err(|e| Error::conversion_error_at_position(s, "integer", pos))?;
                Ok(ParameterValue::ParameterValue(n.into(), pos.to_owned()))
            }
            ArgumentType::Float => {
                if s.is_empty() {
                    return Self::from_arginfo(arginfo)
                        .to_result(|| format!("Float argument {} missing", &arginfo.name), pos);
                }
                let x = s
                    .parse::<f64>()
                    .map_err(|e| Error::conversion_error_at_position(s, "float", pos))?;
                Ok(ParameterValue::ParameterValue(x.into(), pos.to_owned()))
            }
            ArgumentType::FloatOption => {
                if s.is_empty() {
                    let res = Self::from_arginfo(arginfo);
                    if res.is_none() {
                        return Ok(Self::ParameterValue(Value::Null, pos.to_owned()));
                    } else {
                        return Ok(res);
                    }
                }
                let x = s
                    .parse::<f64>()
                    .map_err(|e| Error::conversion_error_at_position(s, "float", pos))?;
                Ok(ParameterValue::ParameterValue(x.into(), pos.to_owned()))
            }
            ArgumentType::Boolean => {
                if s.is_empty() {
                    let res = Self::from_arginfo(arginfo);
                    if res.is_none() {
                        return Ok(Self::ParameterValue(Value::Bool(false), pos.to_owned()));
                    } else {
                        return Ok(res);
                    }
                }
                match s.to_lowercase().as_str() {
                    "true" | "t" | "yes" | "y" | "1" => Ok(ParameterValue::ParameterValue(
                        Value::Bool(true),
                        pos.to_owned(),
                    )),
                    "false" | "f" | "no" | "n" | "0" => Ok(ParameterValue::ParameterValue(
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
                CommandParameterValue::Value(x) => {
                    Ok(ParameterValue::ParameterValue(x.clone(), pos.to_owned()))
                }
                CommandParameterValue::Query(q) => {
                    Ok(ParameterValue::EnumLink(q.clone(), pos.to_owned()))
                }
                CommandParameterValue::None => {
                    if e.others_allowed {
                        Ok(ParameterValue::ParameterValue(
                            Value::String(s.to_owned()),
                            pos.to_owned(),
                        ))
                    } else {
                        Err(Error::conversion_error_with_message(
                            s.to_owned(),
                            &e.name,
                            &format!("Undefined enum {}", e.name),
                        )
                        .with_position(&pos))
                    }
                }
            },
            ArgumentType::Any => {
                if s.is_empty() {
                    let res = Self::from_arginfo(arginfo);
                    if res.is_none() {
                        return Ok(Self::ParameterValue(s.into(), pos.to_owned()));
                    } else {
                        return Ok(res);
                    }
                } else {
                    Ok(ParameterValue::ParameterValue(
                        Value::String(s.to_owned()),
                        pos.to_owned(),
                    ))
                }
            }
            ArgumentType::None => Err(Error::not_supported(
                "None not supported as argument type".to_string(),
            )),
        }
    }

    pub fn pop_value(
        arginfo: &ArgumentInfo,
        param: &mut ActionParameterIterator,
    ) -> Result<Self, Error> {
        let p = Self::from_arginfo(arginfo);
        if arginfo.injected {
            return Ok(p);
        }
        if arginfo.multiple {
            let mut values = Vec::new();
            let values_pos = param.position.clone();
            for x in &mut *param {
                match x {
                    ActionParameter::String(s, pos) => {
                        let pv = Self::from_string(arginfo, s, pos)?;
                        if let Some(v) = pv.value() {
                            values.push(v);
                        } else {
                            return Err(Error::not_supported(format!(
                                "Only values are supported inside vector argument, found {}.",
                                &pv
                            ))
                            .with_position(pos));
                        }
                    }
                    ActionParameter::Link(q, pos) => {
                        return Err(Error::not_supported(
                            "Link not supported in vector argument".to_string(),
                        )
                        .with_position(&param.position));
                    }
                }
            }
            return Ok(ParameterValue::ParameterValue(
                Value::Array(values),
                values_pos,
            ));
        }
        match param.next() {
            Some(ActionParameter::String(s, pos)) => Self::from_string(arginfo, s, pos),
            Some(ActionParameter::Link(q, pos)) => {
                Ok(ParameterValue::ParameterLink(q.clone(), pos.clone()))
            }
            None => Self::from_arginfo(arginfo).to_result(
                || format!("Missing argument '{}'", arginfo.name),
                &param.position,
            ),
        }
    }
    pub fn is_default(&self) -> bool {
        match self {
            ParameterValue::DefaultValue(_) => true,
            ParameterValue::DefaultLink(_) => true,
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
            ParameterValue::DefaultLink(_) => true,
            ParameterValue::ParameterLink(_, _) => true,
            ParameterValue::EnumLink(_, _) => true,
            _ => false,
        }
    }
    pub fn is_injected(&self) -> bool {
        match self {
            ParameterValue::Injected => true,
            _ => false,
        }
    }
    pub fn value(&self) -> Option<Value> {
        match self {
            ParameterValue::DefaultValue(v) => Some(v.clone()),
            ParameterValue::ParameterValue(v, _) => Some(v.clone()),
            _ => None,
        }
    }
    pub fn link(&self) -> Option<Query> {
        match self {
            ParameterValue::DefaultLink(q) => Some(q.clone()),
            ParameterValue::ParameterLink(q, _) => Some(q.clone()),
            ParameterValue::EnumLink(q, _) => Some(q.clone()),
            _ => None,
        }
    }
    pub fn position(&self) -> Position {
        match self {
            ParameterValue::ParameterValue(_, pos) => pos.clone(),
            ParameterValue::ParameterLink(_, pos) => pos.clone(),
            ParameterValue::EnumLink(_, pos) => pos.clone(),
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
impl ResolvedParameterValues {
    pub fn new() -> Self {
        ResolvedParameterValues(Vec::new())
    }
    pub fn from_action(
        action_request: &ActionRequest,
        command_metadata: &CommandMetadata,
    ) -> Result<Self, Error> {
        let mut parameters = ActionParameterIterator::new(action_request);
        let mut values = Vec::new();
        for a in command_metadata.arguments.iter() {
            let pv = ParameterValue::pop_value(a, &mut parameters)?;
            values.push(pv);
        }
        Ok(ResolvedParameterValues(values))
    }
    pub fn clear(&mut self) {
        self.0.clear();
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
}

pub struct PlanBuilder<'c> {
    query: Query,
    command_registry: &'c CommandMetadataRegistry,
    plan: Plan,
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

// TODO: support cache and volatile flags
impl<'c> PlanBuilder<'c> {
    pub fn new(query: Query, command_registry: &'c CommandMetadataRegistry) -> Self {
        PlanBuilder {
            query,
            command_registry,
            plan: Plan::new(),
        }
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
        // TODO: get default namespaces from command registry
        namespaces.push("".to_string());
        namespaces.push("root".to_string());

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
            Ok(command_metadata.clone())
        } else {
            Err(Error::action_not_registered(action_request, &namespaces))
        }
    }

    // TODO: RQS realm should should be supported
    fn process_resource_query(&mut self, rqs: &ResourceQuerySegment) -> Result<(), Error> {
        self.plan.steps.push(Step::GetResource(rqs.key.clone()));
        Ok(())
    }

    fn process_action(
        &mut self,
        query: &Query,
        action_request: &ActionRequest,
    ) -> Result<(), Error> {
        let command_metadata = self.get_command_metadata(query, action_request)?;

        self.plan.steps.push(Step::Action {
            realm: command_metadata.realm.clone(),
            ns: command_metadata.namespace.clone(),
            action_name: action_request.name.clone(),
            position: action_request.position.clone(),
            parameters: ResolvedParameterValues::from_action(action_request, &command_metadata)?,
        });

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
                self.process_query(p)?;
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
                        self.process_action(&query, &action)?;
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
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Plan {
    pub query: Query,
    pub steps: Vec<Step>,
}

impl Plan {
    pub fn new() -> Self {
        Plan {
            query: Query::new(),
            steps: Vec::new(),
        }
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
    fn test_string_parameter_value() {
        let mut arginfo = ArgumentInfo::string_argument("test").with_default("default");
        let pv = ParameterValue::from_arginfo(&arginfo);
        assert_eq!(pv.value(), Some(Value::String("default".to_string())));
        let pv = ParameterValue::from_string(&arginfo, "testarg", &Position::unknown()).unwrap();
        assert_eq!(pv.value(), Some(Value::String("testarg".to_string())));
        let pv = ParameterValue::from_string(&arginfo, "", &Position::unknown()).unwrap();
        assert_eq!(pv.value(), Some(Value::String("".to_string())));
    }
    #[test]
    fn test_pop_parameter_value() -> Result<(), Error> {
        let mut arginfo = ArgumentInfo::string_argument("test").with_default("default");
        let action = parse_query("hello-testarg-123")?.action().unwrap();
        let mut param = ActionParameterIterator::new(&action);

        let pv = ParameterValue::pop_value(&arginfo, &mut param)?;
        assert_eq!(pv.value(), Some(Value::String("testarg".to_string())));

        let arginfo = ArgumentInfo::integer_argument("intarg", false);
        let pv = ParameterValue::pop_value(&arginfo, &mut param)?;
        assert_eq!(pv.value(), Some(Value::Number(123.into())));

        let arginfo = ArgumentInfo::integer_argument("intarg2", true);
        let pv = ParameterValue::pop_value(&arginfo, &mut param)?;
        assert_eq!(pv.value(), Some(Value::Null));

        let mut param = ActionParameterIterator::new(&action);
        let mut arginfo = ArgumentInfo::string_argument("test").set_multiple();
        let pv = ParameterValue::pop_value(&arginfo, &mut param)?;
        assert_eq!(
            pv.value(),
            Some(Value::Array(vec![
                Value::String("testarg".to_string()),
                Value::String("123".to_string())
            ]))
        );

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
        let rp = ResolvedParameterValues::from_action(&action, &cm).unwrap();
        assert_eq!(rp.0.len(), 2);
        assert_eq!(rp.0[0].value(), Some(Value::String("xxx".to_string())));
        assert_eq!(rp.0[1].value(), Some(Value::Number(234.into())));
        dbg!(rp);
        let action = "testcommand-yyy".try_to_query().unwrap().action().unwrap();
        let rp = ResolvedParameterValues::from_action(&action, &cm).unwrap();
        assert_eq!(rp.0.len(), 2);
        assert_eq!(rp.0[0].value(), Some(Value::String("yyy".to_string())));
        assert_eq!(rp.0[1].value(), Some(Value::Number(123.into())));
        dbg!(rp);
        let action = "testcommand".try_to_query().unwrap().action().unwrap();
        let rp = ResolvedParameterValues::from_action(&action, &cm).unwrap();
        assert_eq!(rp.0.len(), 2);
        assert_eq!(rp.0[0].value(), Some(Value::String("zzz".to_string())));
        assert_eq!(rp.0[1].value(), Some(Value::Number(123.into())));
        dbg!(rp);
    }
}
