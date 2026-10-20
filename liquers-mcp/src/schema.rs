//! JSON schemas for tool arguments, derived from command metadata.

use std::collections::BTreeSet;

use liquers_core::command_metadata::{
    ArgumentInfo, ArgumentType, CommandMetadata, CommandParameterValue, EnumArgument,
    EnumArgumentType,
};
use serde_json::{json, Map, Value};

use crate::decl::KeyArgumentDecl;

/// The engine's own `format` argument: it changes the extension of the tool query's filename.
pub const FORMAT_ARGUMENT: &str = "format";

/// The input schema of a tool running `command`.
///
/// The key argument, when declared, is a string property; `hidden` arguments are left out. An
/// argument is required when it has no default, is not optional and is not `multiple`.
pub fn argument_schema(
    command: &CommandMetadata,
    key: Option<&KeyArgumentDecl>,
    hidden: &BTreeSet<String>,
) -> Map<String, Value> {
    let mut properties = Map::new();
    let mut required = Vec::new();
    if let Some(key) = key {
        let description = key
            .description
            .clone()
            .unwrap_or_else(|| "Key relative to the root".to_string());
        properties.insert(
            key.name.clone(),
            json!({"type": "string", "description": description}),
        );
        if key.required {
            required.push(Value::String(key.name.clone()));
        }
    }
    for argument in &command.arguments {
        if hidden.contains(&argument.name) {
            continue;
        }
        let Some(schema) = argument_property(argument) else {
            continue;
        };
        if is_required(argument) {
            required.push(Value::String(argument.name.clone()));
        }
        properties.insert(argument.name.clone(), schema);
    }
    let mut schema = Map::new();
    schema.insert("type".to_string(), json!("object"));
    schema.insert("properties".to_string(), Value::Object(properties));
    if !required.is_empty() {
        schema.insert("required".to_string(), Value::Array(required));
    }
    schema.insert("additionalProperties".to_string(), json!(false));
    schema
}

/// The schema of the engine's `format` argument.
pub fn format_property(default_extension: &str) -> Value {
    json!({
        "type": "string",
        "description": format!(
            "Output format, as a file extension (md, csv, json, yaml, …); default {default_extension}"
        ),
    })
}

fn is_required(argument: &ArgumentInfo) -> bool {
    if argument.multiple || argument.argument_type.is_option() {
        return false;
    }
    match &argument.default {
        CommandParameterValue::Value(_) | CommandParameterValue::Query(_) => false,
        CommandParameterValue::None => true,
    }
}

/// The property schema of one argument, or `None` for an argument of type `None`.
fn argument_property(argument: &ArgumentInfo) -> Option<Value> {
    let mut schema = type_schema(&argument.argument_type)?;
    if argument.multiple {
        schema = json!({"type": "array", "items": schema});
    }
    if let Value::Object(map) = &mut schema {
        if !argument.label.is_empty() && argument.label != argument.name {
            map.insert("title".to_string(), json!(argument.label));
        }
        if !argument.description.is_empty() {
            map.insert("description".to_string(), json!(argument.description));
        }
        if let CommandParameterValue::Value(default) = &argument.default {
            if !argument.argument_type.is_any() && !argument.multiple {
                map.insert("default".to_string(), default.clone());
            }
        }
    }
    Some(schema)
}

fn type_schema(argument_type: &ArgumentType) -> Option<Value> {
    Some(match argument_type {
        ArgumentType::String => json!({"type": "string"}),
        ArgumentType::Integer => json!({"type": "integer"}),
        ArgumentType::IntegerOption => json!({"type": ["integer", "null"]}),
        ArgumentType::Float => json!({"type": "number"}),
        ArgumentType::FloatOption => json!({"type": ["number", "null"]}),
        ArgumentType::Boolean => json!({"type": "boolean"}),
        ArgumentType::BooleanOption => json!({"type": ["boolean", "null"]}),
        ArgumentType::Enum(e) => enum_schema(e),
        // Resolved by the registry before a schema is built; an unresolved one accepts anything.
        ArgumentType::GlobalEnum(_) => json!({}),
        ArgumentType::Any => json!({}),
        ArgumentType::None => return None,
    })
}

fn enum_schema(e: &EnumArgument) -> Value {
    let aliases: Vec<Value> = e.values.iter().map(|v| json!(v.alias)).collect();
    if e.others_allowed {
        json!({"anyOf": [{"enum": aliases}, enum_value_schema(&e.value_type)]})
    } else {
        json!({"enum": aliases})
    }
}

fn enum_value_schema(value_type: &EnumArgumentType) -> Value {
    match value_type {
        EnumArgumentType::String => json!({"type": "string"}),
        EnumArgumentType::Integer => json!({"type": "integer"}),
        EnumArgumentType::IntegerOption => json!({"type": ["integer", "null"]}),
        EnumArgumentType::Float => json!({"type": "number"}),
        EnumArgumentType::FloatOption => json!({"type": ["number", "null"]}),
        EnumArgumentType::Boolean => json!({"type": "boolean"}),
        EnumArgumentType::BooleanOption => json!({"type": ["boolean", "null"]}),
        EnumArgumentType::Any => json!({}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_maps_types_defaults_and_hidden_arguments() {
        let mut command = CommandMetadata::new("list");
        command
            .with_argument(ArgumentInfo::string_argument("text"))
            .with_argument(ArgumentInfo::integer_argument("limit", false).with_default(20))
            .with_argument(ArgumentInfo::string_argument("root"));
        let hidden: BTreeSet<String> = ["root".to_string()].into();
        let key = KeyArgumentDecl {
            name: "path".to_string(),
            description: None,
            required: false,
        };
        let schema = Value::Object(argument_schema(&command, Some(&key), &hidden));
        assert_eq!(schema["properties"]["path"]["type"], "string");
        assert_eq!(schema["properties"]["limit"]["default"], 20);
        assert!(schema["properties"].get("root").is_none());
        assert_eq!(schema["required"], json!(["text"]));
    }
}
