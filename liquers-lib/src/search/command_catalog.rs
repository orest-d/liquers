//! `ns-search/commands`: the command registry as records.

use std::sync::Arc;

use liquers_core::command_metadata::{
    ArgumentInfo, ArgumentType, CommandKey, CommandMetadata, CommandParameterValue,
};
use liquers_core::context::{Context, Environment};
use liquers_core::error::Error;

use crate::records::{
    FieldRole, FieldSchema, FieldType, FieldValue, KeyRole, RecordBatchMut, RecordSchema,
    RecordView, RecordViewMut,
};
use crate::value::{ExtValueInterface, Value};

/// The query fragment that calls a command: `ns-<namespace>/<name>`, or the bare name for the
/// empty or `root` namespace. The realm is not shown (decision D5).
pub fn command_query(command: &CommandMetadata) -> String {
    match command.namespace.as_str() {
        "" | "root" => command.name.clone(),
        namespace => format!("ns-{namespace}/{}", command.name),
    }
}

fn type_name(argument_type: &ArgumentType) -> String {
    match argument_type {
        ArgumentType::String => "String".to_string(),
        ArgumentType::Integer => "Integer".to_string(),
        ArgumentType::IntegerOption => "Integer?".to_string(),
        ArgumentType::Float => "Float".to_string(),
        ArgumentType::FloatOption => "Float?".to_string(),
        ArgumentType::Boolean => "Boolean".to_string(),
        ArgumentType::Enum(e) => {
            let aliases: Vec<&str> = e.values.iter().map(|v| v.alias.as_str()).collect();
            format!("Enum({})", aliases.join("|"))
        }
        ArgumentType::GlobalEnum(name) => format!("Enum({name})"),
        ArgumentType::Any => "Any".to_string(),
        ArgumentType::None => "None".to_string(),
    }
}

fn default_text(default: &CommandParameterValue) -> Option<String> {
    match default {
        CommandParameterValue::Value(value) => {
            Some(serde_json::to_string(value).unwrap_or_else(|_| format!("{value:?}")))
        }
        CommandParameterValue::Query(query) => Some(format!("query \"{}\"", query.encode())),
        CommandParameterValue::None => None,
    }
}

fn argument_text(argument: &ArgumentInfo) -> String {
    let mut out = format!("{}: {}", argument.name, type_name(&argument.argument_type));
    if argument.multiple {
        out.push_str("...");
    } else if let Some(default) = default_text(&argument.default) {
        out.push_str(" = ");
        out.push_str(&default);
    }
    out
}

/// The command's signature as a call: `ns-search/search(query: String = "", rank: Boolean = …)`.
pub fn signature(command: &CommandMetadata) -> String {
    let arguments: Vec<String> = command
        .arguments
        .iter()
        .filter(|argument| !argument.injected)
        .map(argument_text)
        .collect();
    format!("{}({})", command_query(command), arguments.join(", "))
}

fn schema() -> Result<Arc<RecordSchema>, Error> {
    let name_role = || FieldRole::text().and_stored();
    Ok(Arc::new(RecordSchema::new(vec![
        FieldSchema::new("id", FieldType::Text).with_key(KeyRole::Id),
        FieldSchema::new("query", FieldType::Text).with_role(FieldRole::keyword()),
        FieldSchema::new("realm", FieldType::Text).with_role(name_role()),
        FieldSchema::new("namespace", FieldType::Text).with_role(name_role()),
        FieldSchema::new("name", FieldType::Text).with_role(name_role()),
        FieldSchema::new("module", FieldType::Text).with_role(name_role()),
        FieldSchema::new("title", FieldType::Text).with_role(FieldRole::text().and_stored()),
        FieldSchema::new("description", FieldType::Text).with_role(FieldRole::text().and_stored()),
        FieldSchema::new("doc", FieldType::Text).with_role(FieldRole::text().and_stored()),
        FieldSchema::new("volatile", FieldType::Bool).with_role(FieldRole::keyword()),
        FieldSchema::new("output_filename", FieldType::Text).with_role(FieldRole::keyword()),
    ])?))
}

fn text(value: &str) -> FieldValue {
    if value.is_empty() { FieldValue::Null } else { FieldValue::Text(Arc::from(value)) }
}

/// `ns-search/commands`. `namespace` selects one namespace (`rec` or `ns-rec`); empty means all.
pub async fn commands<E: Environment<Value = Value>>(
    namespace: String,
    context: Context<E>,
) -> Result<Value, Error> {
    let wanted = namespace.strip_prefix("ns-").unwrap_or(&namespace).to_string();
    let envref = context.get_envref();
    let registry = envref.get_command_metadata_registry();
    let mut selected: Vec<&CommandMetadata> = registry
        .commands
        .iter()
        .filter(|command| wanted.is_empty() || command.namespace == wanted)
        .collect();
    selected.sort_by(|a, b| (&a.namespace, &a.name, &a.realm).cmp(&(&b.namespace, &b.name, &b.realm)));
    let mut batch = RecordBatchMut::with_capacity(schema()?, selected.len());
    for command in selected {
        batch.append_row(&[
            FieldValue::Text(Arc::from(CommandKey::from(command).to_string().as_str())),
            FieldValue::Text(Arc::from(command_query(command).as_str())),
            text(&command.realm),
            text(&command.namespace),
            text(&command.name),
            text(&command.module),
            text(&command.label),
            FieldValue::Text(Arc::from(signature(command).as_str())),
            text(&command.doc),
            FieldValue::Bool(command.volatile),
            text(&command.filename),
        ])?;
    }
    let view: Arc<dyn RecordView> = Arc::new(batch.freeze()?);
    Ok(Value::from_record_view(view))
}
