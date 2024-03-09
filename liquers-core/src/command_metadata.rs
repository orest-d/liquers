#![allow(unused_imports)]
#![allow(dead_code)]

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

// TODO: consider link as value for enum argument
/// Single alternative of an enum argument, see EnumArgument
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EnumArgumentAlternative {
    pub name: String,
    pub value: Value,
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
            value_type: EnumArgumentType::String,
        }
    }
    pub fn with_value(&mut self, name: &str, value: Value) -> &mut Self {
        self.values.push(EnumArgumentAlternative {
            name: name.to_string(),
            value,
        });
        self
    }
    pub fn with_value_type(&mut self, value_type: EnumArgumentType) -> &mut Self {
        self.value_type = value_type;
        self
    }
    pub fn with_others_allowed(&mut self) -> &mut Self {
        self.others_allowed = true;
        self
    }
    pub fn name_to_value(&self, name: String) -> Option<Value> {
        for alternative in &self.values {
            if alternative.name == name {
                return Some(alternative.value.clone());
            }
        }
        if self.others_allowed {
            return Some(Value::String(name));
        }
        None
    }
}
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

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum DefaultValue {
    Value(Value),
    Query(Query),
    NoDefault,
}

impl DefaultValue {
    fn new() -> Self {
        DefaultValue::NoDefault
    }
    fn null() -> Self {
        DefaultValue::Value(Value::Null)
    }
    fn is_null(&self) -> bool {
        match self {
            DefaultValue::Value(value) => value.is_null(),
            _ => false,
        }
    }
    fn from_value(value: Value) -> Self {
        DefaultValue::Value(value)
    }
    fn from_query(query: Query) -> Self {
        DefaultValue::Query(query)
    }
    fn from_string(value: &str) -> Self {
        DefaultValue::Value(Value::String(value.to_string()))
    }
    fn from_integer(value: i64) -> Self {
        DefaultValue::Value(Value::Number(serde_json::Number::from(value)))
    }
    fn from_float(value: f64) -> Self {
        DefaultValue::Value(Value::Number(serde_json::Number::from_f64(value).unwrap()))
    }
}

impl Default for DefaultValue {
    fn default() -> Self {
        DefaultValue::NoDefault
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
pub struct ArgumentInfo {
    pub name: String,
    pub label: String,
    pub default: DefaultValue,
    pub argument_type: ArgumentType,
    pub multiple: bool,
    pub gui_info: ArgumentGUIInfo,
}

impl ArgumentInfo {
    pub fn any_argument(name: &str) -> Self {
        ArgumentInfo {
            name: name.to_string(),
            label: name.replace("_", " ").to_string(),
            default: DefaultValue::NoDefault,
            argument_type: ArgumentType::Any,
            multiple: false,
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
            default: DefaultValue::NoDefault,
            argument_type: ArgumentType::Any,
            multiple: false,
            gui_info: ArgumentGUIInfo::TextField(40),
        }
    }
    pub fn string_argument(name: &str) -> Self {
        ArgumentInfo {
            name: name.to_string(),
            label: name.replace("_", " ").to_string(),
            default: DefaultValue::NoDefault,
            argument_type: ArgumentType::String,
            multiple: false,
            gui_info: ArgumentGUIInfo::TextField(40),
        }
    }
    pub fn integer_argument(name: &str, option: bool) -> Self {
        ArgumentInfo {
            name: name.to_string(),
            label: name.replace("_", " ").to_string(),
            default: if option {
                DefaultValue::null()
            } else {
                DefaultValue::NoDefault
            },
            argument_type: if option {
                ArgumentType::IntegerOption
            } else {
                ArgumentType::Integer
            },
            multiple: false,
            gui_info: ArgumentGUIInfo::IntegerField,
        }
    }
    pub fn float_argument(name: &str, option: bool) -> Self {
        ArgumentInfo {
            name: name.to_string(),
            label: name.replace("_", " ").to_string(),
            default: if option {
                DefaultValue::null()
            } else {
                DefaultValue::NoDefault
            },
            argument_type: if option {
                ArgumentType::FloatOption
            } else {
                ArgumentType::Float
            },
            multiple: false,
            gui_info: ArgumentGUIInfo::FloatField,
        }
    }
    pub fn boolean_argument(name: &str) -> Self {
        ArgumentInfo {
            name: name.to_string(),
            label: name.replace("_", " ").to_string(),
            default: DefaultValue::NoDefault,
            argument_type: ArgumentType::Boolean,
            multiple: false,
            gui_info: ArgumentGUIInfo::Checkbox,
        }
    }
    pub fn with_default_none(&mut self) -> &mut Self {
        self.default = DefaultValue::null();
        self
    }
    pub fn with_default(&mut self, value: &str) -> &mut Self {
        self.default = DefaultValue::from_string(value);
        self
    }
    pub fn true_by_default(&mut self) -> &mut Self {
        self.default = DefaultValue::from_value(Value::Bool(true));
        self
    }
    pub fn false_by_default(&mut self) -> &mut Self {
        self.default = DefaultValue::from_value(Value::Bool(false));
        self
    }

    pub fn with_label(&mut self, label: &str) -> &mut Self {
        self.label = label.to_string();
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

impl From<&CommandMetadata> for CommandKey {
    fn from(command: &CommandMetadata) -> Self {
        CommandKey::new(&command.realm, &command.namespace, command.name.as_str())
    }
}

impl From<&CommandKey> for String {
    fn from(key: &CommandKey) -> Self {
        format!("-p-cmd-{}-{}-{}", key.realm, key.namespace, key.name)
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
    pub module: String,
    pub doc: String,
    pub state_argument: ArgumentInfo,
    pub arguments: Vec<ArgumentInfo>,
    pub cache:bool,
    pub volatile:bool,
}

impl CommandMetadata {
    pub fn new(name: &str) -> Self {
        CommandMetadata {
            realm: "".to_string(),
            namespace: "root".to_string(),
            name: name.to_string(),
            module: "".to_string(),
            doc: "".to_string(),
            state_argument: ArgumentInfo::any_argument("state"),
            arguments: Vec::new(),
            cache:true,
            volatile:false,
        }
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
        self.state_argument = state_argument;
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
}

impl CommandMetadataRegistry {
    pub fn new() -> Self {
        CommandMetadataRegistry {
            commands: Vec::new(),
        }
    }
    pub fn add_command(&mut self, command: &CommandMetadata) -> &mut Self {
        self.commands.push(command.to_owned());
        self
    }

    pub fn get_mut<K>(&mut self, key:K) -> Option<&mut CommandMetadata>
    where K:Into<CommandKey>
    {
        let key:CommandKey = key.into();
        for command in &mut self.commands {
            if command.realm == key.realm && command.namespace == key.namespace && command.name == key.name {
                return Some(command);
            }
        }
        None
    }

    pub fn get<K>(&self, key:K) -> Option<&CommandMetadata>
    where K:Into<CommandKey>
    {
        let key = key.into();
        for command in &self.commands {
            if command.realm == key.realm && command.namespace == key.namespace && command.name == key.name {
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
