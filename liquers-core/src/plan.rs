#![allow(unused_imports)]
#![allow(dead_code)]

use std::fmt::Display;

use itertools::Itertools;
use nom::Err;
use serde_json::Value;

use crate::command_metadata::{
    self, ArgumentInfo, ArgumentType, CommandMetadata, CommandMetadataRegistry, DefaultValue,
    EnumArgumentType,
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
        parameters: ResolvedParameters,
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

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Parameter {
    pub value: Value,
    pub position: Position,
    pub default: bool,
}

impl Parameter {}
impl Display for Parameter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({})", self.value, self.position)
    }
}
impl Default for Parameter {
    fn default() -> Self {
        Parameter {
            value: Value::Null,
            position: Position::unknown(),
            default: false,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ResolvedParameters {
    pub parameters: Vec<Parameter>,
    pub links: Vec<(usize, Query)>,
}

impl ResolvedParameters {
    pub fn new() -> Self {
        ResolvedParameters {
            parameters: Vec::new(),
            links: Vec::new(),
        }
    }
    pub fn clear(&mut self) {
        self.parameters.clear();
        self.links.clear();
    }
}

pub struct PlanBuilder<'c> {
    query: Query,
    command_registry: &'c CommandMetadataRegistry,
    resolved_parameters: ResolvedParameters,
    parameter_number: usize,
    arginfo_number: usize,
    plan: Plan,
}

// TODO: support cache and volatile flags
impl<'c> PlanBuilder<'c> {
    pub fn new(query: Query, command_registry: &'c CommandMetadataRegistry) -> Self {
        PlanBuilder {
            query,
            command_registry,
            resolved_parameters: ResolvedParameters::new(),
            parameter_number: 0,
            arginfo_number: 0,
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
        println!("action:     {}",action_request.encode());
        println!("namespaces: {:?}",&namespaces);
        println!("realm:      {}",&realm);

        if let Some(command_metadata) = self.command_registry.find_command_in_namespaces(
            &realm,
            &namespaces,
            &action_request.name,
        ) {
            println!("Command found");
            Ok(command_metadata.clone())
        } else {
            println!("Command not found");
            Err(Error::action_not_registered(action_request, &namespaces))
        }
    }

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
        self.get_parameters(&command_metadata, action_request)?;
        self.plan.steps.push(Step::Action {
            realm: command_metadata.realm.clone(),
            ns: command_metadata.namespace.clone(),
            action_name: action_request.name.clone(),
            position: action_request.position.clone(),
            parameters: self.resolved_parameters.clone(),
        });
        Ok(())
    }

    fn process_query(&mut self, query: &Query) -> Result<(), Error> {
        println!("process query {}",query);
        if query.is_empty() || query.is_ns() {
            println!("empty or ns");
            return Ok(());
        }
        if let Some(rq) = query.resource_query() {
            println!("RESOURCE {}",rq);
            self.process_resource_query(&rq)?;
            return Ok(());
        }
        if let Some(transform) = query.transform_query() {
            println!("TRANSFORM {}",&transform);
            if let Some(action) = transform.action() {
                println!("ACTION {}",&action);
                let mut query = query.clone();
                query.segments = Vec::new();
                self.process_action(&query, &action)?;
                return Ok(());
            }
            if transform.is_filename() {
                println!("FILENAME {}",&transform);
                self.plan
                    .steps
                    .push(Step::Filename(transform.filename.unwrap().clone()));
                return Ok(());
            }
            println!("Longer transform query");
        }

        let (p, q) = query.predecessor();
        println!("PREDECESOR: {:?}",&p);
        println!("REMAINDER:  {:?}",&q);

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

    // TODO: this is mixing action parameters with defaults from command metadata - faulty logic
    /// Get value from an action parameter, handle links and defaults
    fn pop_action_parameter(
        &mut self,
        arginfo: &ArgumentInfo,
        action_request: &ActionRequest,
    ) -> Result<(Option<Value>, bool), Error> {
        match action_request.parameters.get(self.parameter_number) {
            Some(ActionParameter::String(v, _)) => {
                self.parameter_number += 1;
                Ok((Some(Value::String(v.to_owned())), false))
            }
            Some(ActionParameter::Link(q, _)) => {
                self.resolved_parameters
                    .links
                    .push((self.resolved_parameters.parameters.len(), q.clone()));
                self.parameter_number += 1;
                Ok((None, false))
            }
            None => match &arginfo.default {
                DefaultValue::Value(v) => Ok((Some(v.clone()), true)),
                DefaultValue::Query(q) => {
                    self.resolved_parameters
                        .links
                        .push((self.resolved_parameters.parameters.len(), q.clone()));
                    Ok((None, true))
                }
                DefaultValue::NoDefault => Err(Error::missing_argument(
                    self.arginfo_number,
                    &arginfo.name,
                    &action_request.position,
                )),
            },
        }
    }

    /// Pop single command parameter value
    /// Note that this is different from action parameter.
    /// A command parameter can represent to several action parameters
    /// or it can be filled with default value from command metadata.
    fn pop_value(
        &mut self,
        arginfo: &ArgumentInfo,
        action_request: &ActionRequest,
    ) -> Result<Value, Error> {
        match (
            &arginfo.argument_type,
            self.pop_action_parameter(arginfo, action_request)?,
        ) {
            (_, (None, is_default)) => Ok(Value::Null),
            (ArgumentType::String, (Some(x), is_default)) => Ok(x),
            (ArgumentType::Integer, (Some(x), is_default)) => Ok(x),
            (ArgumentType::IntegerOption, (Some(x), is_default)) => Ok(x),
            (ArgumentType::Float, (Some(x), is_default)) => Ok(x),
            (ArgumentType::FloatOption, (Some(x), is_default)) => Ok(x),
            (ArgumentType::Boolean, (Some(x), is_default)) => Ok(x),
            (ArgumentType::Enum(e), (Some(x), is_default)) => {
                if let Some(xx) = e.name_to_value(x.to_string()) {
                    Ok(xx)
                } else {
                    Err(Error::conversion_error(x, &e.name))
                }
            }
            (ArgumentType::Any, (Some(x), is_default)) => Ok(x),
            (ArgumentType::None, (Some(_), _)) => Err(Error::not_supported(format!(
                "None not supported as argument type"
            ))),
        }
    }
    fn get_parameters(
        &mut self,
        command_metadata: &CommandMetadata,
        action_request: &ActionRequest,
    ) -> Result<(), Error> {
        self.arginfo_number = 0;
        self.parameter_number = 0;
        self.resolved_parameters = ResolvedParameters::new();
        for (i, a) in command_metadata.arguments.iter().enumerate() {
            self.arginfo_number = i;
            let value = self.pop_value(a, action_request)?;
            self.resolved_parameters.parameters.push(Parameter {
                value: value,
                position: Position::unknown(), //TODO: Get the position from ActionParameter
                //action_request.parameters[self.parameter_number].position(),
                default: false, //TODO: set default properly
            });
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
}
