#![allow(unused_imports)]
#![allow(dead_code)]

use std::fmt::Display;

use crate::error::Error;
use crate::query::{ActionParameter, Query};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A structure holding a description of an identified issue with a command registry
/// Issue can be either a warning or an error (when is_error is true)
/// Command can be identified by realm, name and namespace
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CommandRegistryIssue {
    pub realm: String,
    pub namespace: String,
    pub name: String,
    pub is_error: bool,
    pub message: String,
}

impl CommandRegistryIssue {
    pub fn new(realm: &str, namespace: &str, name: &str, is_error: bool, message: String) -> Self {
        CommandRegistryIssue {
            realm: realm.to_string(),
            namespace: namespace.to_string(),
            name: name.to_string(),
            is_error,
            message: message.to_string(),
        }
    }
    pub fn warning(realm: &str, namespace: &str, name: &str, message: String) -> Self {
        CommandRegistryIssue::new(realm, name, namespace, false, message)
    }
    pub fn error(realm: &str, namespace: &str, name: &str, message: String) -> Self {
        CommandRegistryIssue::new(realm, name, namespace, true, message)
    }
}

/// Single alternative of an enum argument, see EnumArgument
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EnumArgumentAlternative {
    pub alias: String,
    pub value: CommandParameterValue,
}

/// Type of an enum argument, see EnumArgument
/// This is a restricted version of ArgumentType to prevent circular type definition
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum EnumArgumentType {
    #[serde(rename = "string")]
    String,
    #[serde(rename = "int")]
    Integer,
    #[serde(rename = "int_opt")]
    IntegerOption,
    #[serde(rename = "float")]
    Float,
    #[serde(rename = "float_opt")]
    FloatOption,
    #[serde(rename = "bool")]
    Boolean,
    #[serde(rename = "any")]
    Any,
}

impl Default for EnumArgumentType {
    fn default() -> Self {
        EnumArgumentType::String
    }
}
//TODO: add support for value with type_identifier

/// Enum argument type specification
/// EnumArgument specifies string aliases for values via vector of EnumArgumentAlternative.
/// Besides alternatives (values) EnumArgument has name and a value type.
/// If others_allowed is false, then only the values from the vector 'values' are allowed.
/// If others_allowed is true, then any value is allowed, but it must conform to the value_type.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EnumArgument {
    pub name: String,
    pub values: Vec<EnumArgumentAlternative>,
    pub others_allowed: bool,
    pub value_type: EnumArgumentType,
}

impl EnumArgument {
    pub fn new(name: &str) -> Self {
        EnumArgument {
            name: name.to_string(),
            values: Vec::new(),
            others_allowed: false,
            value_type: EnumArgumentType::Any,
        }
    }
    pub fn with_string_value(mut self, alias: &str, value: &str) -> Self {
        self.value_type = EnumArgumentType::String;
        self.values.push(EnumArgumentAlternative {
            alias: alias.to_string(),
            value: CommandParameterValue::Value(Value::String(value.to_string())),
        });
        self
    }
    pub fn with_alternative(mut self, alias: &str) -> Self {
        self.with_string_value(alias, alias)
    }
    pub fn with_int_value(mut self, alias: &str, value: i32) -> Self {
        self.value_type = EnumArgumentType::Integer;
        self.values.push(EnumArgumentAlternative {
            alias: alias.to_string(),
            value: CommandParameterValue::Value(Value::Number(serde_json::Number::from(value))),
        });
        self
    }
    pub fn with_value<T:Into<Value>>(mut self, alias: &str, value: T) -> Self {
        self.values.push(EnumArgumentAlternative {
            alias: alias.to_string(),
            value: CommandParameterValue::from_value(value.into())
        });
        self
    }

    pub fn with_link(mut self, alias: &str, query: Query) -> Self {
        self.values.push(EnumArgumentAlternative {
            alias: alias.to_string(),
            value: CommandParameterValue::Query(query),
        });
        self
    }
    pub fn with_value_type(mut self, value_type: EnumArgumentType) -> Self {
        self.value_type = value_type;
        self
    }
    pub fn with_others_allowed(mut self) -> Self {
        self.others_allowed = true;
        self
    }

    /// Convert alias of an enum alternative
    /// If the name is not found in the alternatives, then DefaultValue::NoDefault is returned
    pub fn expand_alias(&self, alias: &str) -> CommandParameterValue {
        for alternative in &self.values {
            if alternative.alias == alias {
                return alternative.value.clone();
            }
        }
        CommandParameterValue::None
    }
}

//TODO: add support for value with type_identifier
/// Argument type specification
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ArgumentType {
    #[serde(rename = "string")]
    String,
    #[serde(rename = "int")]
    Integer,
    #[serde(rename = "int_opt")]
    IntegerOption,
    #[serde(rename = "float")]
    Float,
    #[serde(rename = "float_opt")]
    FloatOption,
    #[serde(rename = "bool")]
    Boolean,
    Enum(EnumArgument),
    #[serde(rename = "any")]
    Any,
    #[serde(rename = "none")]
    None,
}

impl ArgumentType {
    pub fn is_option(&self) -> bool {
        match self {
            ArgumentType::IntegerOption => true,
            ArgumentType::FloatOption => true,
            _ => false,
        }
    }
}

impl Default for ArgumentType {
    fn default() -> Self {
        ArgumentType::Any
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ArgumentGUIInfo {
    TextField(usize),
    TextArea(usize, usize),
    IntegerField,
    FloatField,
    Checkbox,
    EnumSelector,
    None,
}

impl Default for ArgumentGUIInfo {
    fn default() -> Self {
        ArgumentGUIInfo::TextField(20)
    }
}

// TODO: maybe Template?
/// CommandParameterValue represents a value of a command parameter.
/// This is used to represent a default value of an argument
/// defined in the CommandMetadata.
/// In Plan building phase, the CommandParameterValue is used to fill the default values
/// where needed when creating the ResolvedParameterValues for an Action.
/// CommandParameterValue can be a JSON Value, a Query or None.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum CommandParameterValue {
    Value(Value),
    Query(Query),
    None,
}

impl CommandParameterValue {
    fn new() -> Self {
        CommandParameterValue::None
    }
    fn null() -> Self {
        CommandParameterValue::Value(Value::Null)
    }
    fn is_null(&self) -> bool {
        match self {
            CommandParameterValue::Value(value) => value.is_null(),
            _ => false,
        }
    }
    fn from_value(value: Value) -> Self {
        CommandParameterValue::Value(value)
    }
    fn from_query(query: Query) -> Self {
        CommandParameterValue::Query(query)
    }
    fn from_string(value: &str) -> Self {
        CommandParameterValue::Value(Value::String(value.to_string()))
    }
    fn from_integer(value: i64) -> Self {
        CommandParameterValue::Value(Value::Number(serde_json::Number::from(value)))
    }
    fn from_float(value: f64) -> Self {
        CommandParameterValue::Value(Value::Number(serde_json::Number::from_f64(value).unwrap()))
    }
}

impl Default for CommandParameterValue {
    fn default() -> Self {
        CommandParameterValue::None
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ArgumentInfo {
    pub name: String,
    pub label: String,
    pub default: CommandParameterValue,
    pub argument_type: ArgumentType,
    pub multiple: bool,
    pub injected: bool,
    pub gui_info: ArgumentGUIInfo,
}

impl ArgumentInfo {
    pub fn any_argument(name: &str) -> Self {
        ArgumentInfo {
            name: name.to_string(),
            label: name.replace("_", " ").to_string(),
            default: CommandParameterValue::None,
            argument_type: ArgumentType::Any,
            multiple: false,
            injected: false,
            gui_info: ArgumentGUIInfo::TextField(40),
        }
    }
    fn check(&self, _realm: &str, _namespace: &str, _name: &str) -> Vec<CommandRegistryIssue> {
        let issues = Vec::new();
        issues
    }

    pub fn argument(name: &str) -> Self {
        ArgumentInfo {
            name: name.to_string(),
            label: name.replace("_", " ").to_string(),
            default: CommandParameterValue::None,
            argument_type: ArgumentType::Any,
            multiple: false,
            injected: false,
            gui_info: ArgumentGUIInfo::TextField(40),
        }
    }
    pub fn string_argument(name: &str) -> Self {
        ArgumentInfo {
            name: name.to_string(),
            label: name.replace("_", " ").to_string(),
            default: CommandParameterValue::None,
            argument_type: ArgumentType::String,
            multiple: false,
            injected: false,
            gui_info: ArgumentGUIInfo::TextField(40),
        }
    }
    pub fn integer_argument(name: &str, option: bool) -> Self {
        ArgumentInfo {
            name: name.to_string(),
            label: name.replace("_", " ").to_string(),
            default: if option {
                CommandParameterValue::null()
            } else {
                CommandParameterValue::None
            },
            argument_type: if option {
                ArgumentType::IntegerOption
            } else {
                ArgumentType::Integer
            },
            multiple: false,
            injected: false,
            gui_info: ArgumentGUIInfo::IntegerField,
        }
    }
    pub fn float_argument(name: &str, option: bool) -> Self {
        ArgumentInfo {
            name: name.to_string(),
            label: name.replace("_", " ").to_string(),
            default: if option {
                CommandParameterValue::null()
            } else {
                CommandParameterValue::None
            },
            argument_type: if option {
                ArgumentType::FloatOption
            } else {
                ArgumentType::Float
            },
            multiple: false,
            injected: false,
            gui_info: ArgumentGUIInfo::FloatField,
        }
    }
    pub fn boolean_argument(name: &str) -> Self {
        ArgumentInfo {
            name: name.to_string(),
            label: name.replace("_", " ").to_string(),
            default: CommandParameterValue::None,
            argument_type: ArgumentType::Boolean,
            multiple: false,
            injected: false,
            gui_info: ArgumentGUIInfo::Checkbox,
        }
    }
    pub fn with_default_none(mut self) -> Self {
        self.default = CommandParameterValue::null();
        self
    }
    pub fn with_type(mut self, argtype:ArgumentType) -> Self {
        self.argument_type = argtype;
        self
    }
    pub fn with_default<T:Into<Value>>(mut self, value: T) -> Self {
        self.default = CommandParameterValue::from_value(value.into());
        self
    }
    pub fn true_by_default(mut self) -> Self {
        self=self.with_type(ArgumentType::Boolean);
        self.default = CommandParameterValue::from_value(Value::Bool(true));
        self
    }
    pub fn false_by_default(mut self) -> Self {
        self=self.with_type(ArgumentType::Boolean);
        self.default = CommandParameterValue::from_value(Value::Bool(false));
        self
    }

    pub fn with_label(mut self, label: &str) -> Self {
        self.label = label.to_string();
        self
    }
    pub fn set_injected(mut self) -> Self {
        self.injected = true;
        self.gui_info = ArgumentGUIInfo::None;
        self
    }
    pub fn set_multiple(mut self) -> Self {
        self.multiple = true;
        self
    }
}

const DEFAULT_REALM: &str = "main";
const DEFAULT_NAMESPACE: &str = "root";

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct CommandKey {
    pub realm: String,
    pub namespace: String,
    pub name: String,
    // TODO: specialization by type and type group
    // TODO: other possibilities? (e.g. version)
}

impl CommandKey {
    pub fn new(realm: &str, namespace: &str, name: &str) -> Self {
        let realm = if realm == DEFAULT_REALM { "" } else { realm };
        let namespace = if namespace == DEFAULT_NAMESPACE {
            ""
        } else {
            namespace
        };
        CommandKey {
            realm: realm.to_owned(),
            namespace: namespace.to_owned(),
            name: name.to_owned(),
        }
    }
    pub fn new_name(name: &str) -> Self {
        CommandKey {
            realm: "".to_owned(),
            namespace: "".to_owned(),
            name: name.to_owned(),
        }
    }
}
/*
impl From<&CommandKey> for CommandKey {
    fn from(key: &CommandKey) -> Self {
        key.clone()
    }
}
*/

impl Display for CommandKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        //TODO: not sure yet what this should be
        write!(f, "{}-{}-{}", self.realm, self.namespace, self.name)
    }
}

impl From<&CommandMetadata> for CommandKey {
    fn from(command: &CommandMetadata) -> Self {
        CommandKey::new(&command.realm, &command.namespace, command.name.as_str())
    }
}

impl From<&CommandKey> for String {
    fn from(key: &CommandKey) -> Self {
        //TODO: not sure yet what this should be
        format!("{}-{}-{}", key.realm, key.namespace, key.name)
    }
}

impl From<&CommandKey> for CommandMetadata {
    fn from(key: &CommandKey) -> Self {
        let mut cm = CommandMetadata::new(key.name.as_str());
        cm.with_realm(key.realm.as_str())
            .with_namespace(key.namespace.as_str());
        cm
    }
}

impl From<&str> for CommandKey {
    fn from(name: &str) -> Self {
        CommandKey {
            realm: "".to_string(),
            namespace: "".to_string(),
            name: name.to_string(),
        }
    }
}

// TODO: continue here
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum CommandDefinition {
    Registered,
    Alias{
        command: CommandKey,
        head_parameters: Vec<CommandParameterValue>,
    },    
}

impl Default for CommandDefinition {
    fn default() -> Self {
        CommandDefinition::Registered
    }
}

// TODO: support input type
// TODO: support output type
/// CommandMetadata describes a command.
/// It contains documentation and information about the command arguments,
/// which is used to fill default values and type-check/validate the arguments
/// during the [crate::plan::Plan] building phase.
/// It does not specify how to execute the command though, this is the role of a CommandExecutor.
///
/// # Example
/// ```
/// use liquers_core::command_metadata::*;
///
/// let mut command = CommandMetadata::new("test");
/// command.with_doc("This is a test command")
///    .with_argument(ArgumentInfo::string_argument("arg1"));
/// ```
#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct CommandMetadata {
    pub realm: String,
    pub namespace: String,
    pub name: String,
    //TODO: improve module - rust, python or jvm module ?
    pub module: String,
    pub doc: String,
    //TODO: state argument should be optional
    pub state_argument: Option<ArgumentInfo>,
    pub arguments: Vec<ArgumentInfo>,
    pub cache: bool,
    pub volatile: bool,
    pub definition: CommandDefinition,
}

impl CommandMetadata {
    pub fn new(name: &str) -> Self {
        CommandMetadata {
            realm: "".to_string(),
            namespace: "root".to_string(),
            name: name.to_string(),
            module: "".to_string(),
            doc: "".to_string(),
            state_argument: Some(ArgumentInfo::any_argument("state")),
            arguments: Vec::new(),
            cache: true,
            volatile: false,
            definition: CommandDefinition::Registered,
        }
    }
    pub fn from_key(key: CommandKey) -> Self {
        CommandMetadata {
            realm: key.realm,
            namespace: key.namespace,
            name: key.name,
            module: "".to_string(),
            doc: "".to_string(),
            state_argument: Some(ArgumentInfo::any_argument("state")),
            arguments: Vec::new(),
            cache: true,
            volatile: false,
            definition: CommandDefinition::Registered,
        }
    }
    pub fn key(&self) -> CommandKey {
        CommandKey::new(&self.realm, &self.namespace, &self.name)
    }
    pub fn check(&self) -> Vec<CommandRegistryIssue> {
        let mut issues = Vec::new();
        if self.name == "" {
            issues.push(CommandRegistryIssue::error(
                &self.realm,
                &self.namespace,
                &self.name,
                "Command name is empty".to_string(),
            ));
        }
        if self.name == "ns" {
            issues.push(CommandRegistryIssue::error(
                &self.realm,
                &self.namespace,
                &self.name,
                "Command name 'ns' is reserved".to_string(),
            ));
        }
        for a in self.arguments.iter() {
            issues.append(&mut a.check(&self.realm, &self.namespace, &self.name));
        }
        issues
    }

    pub fn with_state_argument(&mut self, state_argument: ArgumentInfo) -> &mut Self {
        self.state_argument = Some(state_argument);
        self
    }
    pub fn no_state_argument(&mut self) -> &mut Self {
        self.state_argument = None;
        self
    }

    pub fn with_argument(&mut self, argument: ArgumentInfo) -> &mut Self {
        self.arguments.push(argument);
        self
    }

    pub fn with_doc(&mut self, doc: &str) -> &mut Self {
        self.doc = doc.to_string();
        self
    }
    pub fn with_realm(&mut self, realm: &str) -> &mut Self {
        self.realm = realm.to_string();
        self
    }
    pub fn with_namespace(&mut self, namespace: &str) -> &mut Self {
        self.namespace = namespace.to_string();
        self
    }
    pub fn with_name(&mut self, name: &str) -> &mut Self {
        self.name = name.to_string();
        self
    }
    pub fn with_module(&mut self, module: &str) -> &mut Self {
        self.module = module.to_string();
        self
    }
}

// TODO: Refactor CommandMetadataRegistry to use realm/ns hierarchy and CommandKey
// TODO: support global enums
/// Command registry is a structure holding description (metadata) of all commands available in the system
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CommandMetadataRegistry {
    pub commands: Vec<CommandMetadata>,
    pub default_namespaces: Vec<String>,
}

impl CommandMetadataRegistry {
    pub fn new() -> Self {
        CommandMetadataRegistry {
            commands: Vec::new(),
            default_namespaces: vec!["".to_string(), "root".to_string()],
        }
    }

    pub fn add_command(&mut self, command: &CommandMetadata) -> &mut Self {
        self.commands.push(command.to_owned());
        self
    }

    pub fn get_mut<K>(&mut self, key: K) -> Option<&mut CommandMetadata>
    where
        K: Into<CommandKey>,
    {
        let key: CommandKey = key.into();
        for command in &mut self.commands {
            if command.realm == key.realm
                && command.namespace == key.namespace
                && command.name == key.name
            {
                return Some(command);
            }
        }
        None
    }

    pub fn get<K>(&self, key: K) -> Option<&CommandMetadata>
    where
        K: Into<CommandKey>,
    {
        let key = key.into();
        for command in &self.commands {
            if command.realm == key.realm
                && command.namespace == key.namespace
                && command.name == key.name
            {
                return Some(command);
            }
        }
        None
    }

    pub fn find_command(
        &self,
        realm: &str,
        namespace: &str,
        name: &str,
    ) -> Option<CommandMetadata> {
        for command in &self.commands {
            if command.realm == realm && command.namespace == namespace && command.name == name {
                return Some(command.clone());
            }
        }
        None
    }
    //TODO: implement command specialization by type
    pub fn find_command_in_namespaces(
        &self,
        realm: &str,
        namespaces: &Vec<String>,
        name: &str,
    ) -> Option<CommandMetadata> {
        for namespace in namespaces {
            if let Some(command) = self.find_command(realm, namespace, name) {
                return Some(command);
            }
        }
        None
    }
}
