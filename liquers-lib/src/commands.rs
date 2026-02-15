use std::collections::BTreeMap;
use std::fmt::Write;

use liquers_core::{
    command_metadata::{ArgumentType, CommandDefinition, CommandParameterValue},
    context::{Context, Environment},
    error::Error,
    state::State,
    value::ValueInterface,
};
use liquers_macro::register_command;

use crate::{environment::{CommandRegistryAccess, DefaultEnvironment}, value::{Value, simple::SimpleValue}};

/// Generic command trying to convert any value to text representation.
pub fn to_text<E:Environment>(state: &State<E::Value>, _context:Context<E>) -> Result<E::Value, Error> {
    Ok(E::Value::from_string(state.try_into_string()?))
}

/// Generic command trying to extract metadata from the state.
pub fn to_metadata<E:Environment>(state: &State<E::Value>, _context:Context<E>) -> Result<E::Value, Error> {
    if let Some(metadata) = state.metadata.metadata_record() {
        Ok(E::Value::from_metadata(metadata))
    }
    else{
        Err(Error::general_error("Legacy metadata not supported in to_metadata command".to_string()))
    }
}

/// Generic command trying to extract metadata from the state.
pub fn to_assetinfo<E:Environment>(state: &State<E::Value>, _context:Context<E>) -> Result<E::Value, Error> {
    if let Some(metadata) = state.metadata.metadata_record() {
        Ok(E::Value::from_asset_info(vec![metadata.get_asset_info()]))
    }
    else{
        Err(Error::general_error("Legacy metadata not supported in to_assetinfo command".to_string()))
    }
}


pub fn from_yaml<E:Environment<Value = Value>>(state: &State<E::Value>, context:Context<E>) -> Result<E::Value, Error>
{
    let x = &*(state.data);
    match x {
        Value::Base(SimpleValue::Text { value }) => {
            context.info("Parsing yaml string");
            let v: SimpleValue = serde_yaml::from_str(&value)
                .map_err(|e| Error::general_error(format!("Error parsing yaml string: {e}")))?;
            Ok(Value::new_base(v))
        }
        Value::Base(SimpleValue::Bytes{value: b}) => {
            context.info("Parsing yaml bytes");
            let v: SimpleValue = serde_yaml::from_slice(b)
                .map_err(|e| Error::general_error(format!("Error parsing yaml bytes: {e}")))?;
            Ok(Value::new_base(v))
        }
        _ => {
            context.info("Keeping original value unchanged");
            Ok(x.clone())
        }
    }
}

fn argument_type_name(at: &ArgumentType) -> &str {
    match at {
        ArgumentType::String => "String",
        ArgumentType::Integer => "Integer",
        ArgumentType::Boolean => "Boolean",
        ArgumentType::Float => "Float",
        ArgumentType::IntegerOption => "Integer?",
        ArgumentType::FloatOption => "Float?",
        ArgumentType::Enum(e) => &e.name,
        ArgumentType::Any => "Any",
        ArgumentType::None => "None",
        ArgumentType::GlobalEnum(s) => s.as_str(),
    }
}

fn default_value_display(v: &CommandParameterValue) -> String {
    match v {
        CommandParameterValue::Value(val) => {
            serde_json::to_string(val).unwrap_or_else(|_| format!("{:?}", val))
        }
        CommandParameterValue::Query(q) => format!("`{}`", q.encode()),
        CommandParameterValue::None => "\u{2014}".to_string(), // em-dash
    }
}

/// Generate markdown documentation of registered commands.
///
/// - Both `namespace` and `command_name` empty: document all commands.
/// - `namespace` non-empty, `command_name` empty: all commands in that namespace.
/// - `command_name` non-empty: only the matching command (in `namespace`, or any if empty).
pub fn commands_doc<E: Environment>(
    _state: &State<E::Value>,
    namespace: String,
    command_name: String,
    context: Context<E>,
) -> Result<E::Value, Error> {
    let envref = context.get_envref();
    let registry = envref.get_command_metadata_registry();

    // Group commands by namespace, applying filters
    let mut by_namespace: BTreeMap<String, Vec<&liquers_core::command_metadata::CommandMetadata>> =
        BTreeMap::new();
    for cmd in &registry.commands {
        let ns = if cmd.namespace.is_empty() {
            "root".to_string()
        } else {
            cmd.namespace.clone()
        };

        // Filter by namespace
        if !namespace.is_empty() && ns != namespace {
            continue;
        }
        // Filter by command name
        if !command_name.is_empty() && cmd.name != command_name {
            continue;
        }

        by_namespace.entry(ns).or_default().push(cmd);
    }

    let mut md = String::new();
    let _ = writeln!(md, "# Commands\n");

    for (ns, commands) in &by_namespace {
        let _ = writeln!(md, "## Namespace: `{}`\n", ns);

        for cmd in commands {
            let _ = writeln!(md, "### `{}`\n", cmd.name);

            if !cmd.label.is_empty() {
                let _ = writeln!(md, "*{}*\n", cmd.label);
            }

            if !cmd.doc.is_empty() {
                let _ = writeln!(md, "> {}\n", cmd.doc);
            }

            match &cmd.definition {
                CommandDefinition::Registered => {}
                CommandDefinition::Alias {
                    command,
                    head_parameters: _,
                } => {
                    let alias_ns = if command.namespace.is_empty() {
                        "root"
                    } else {
                        &command.namespace
                    };
                    let _ = writeln!(md, "Alias for `{}/{}`\n", alias_ns, command.name);
                }
            }

            // Collect non-injected arguments
            let visible_args: Vec<_> =
                cmd.arguments.iter().filter(|a| !a.injected).collect();

            if !visible_args.is_empty() {
                let _ = writeln!(md, "| Label | Argument | Multiplicity | Type | Default |");
                let _ = writeln!(md, "|-------|----------|--------------|------|---------|");
                for arg in &visible_args {
                    let multiplicity = if arg.multiple { "multiple" } else { "single" };
                    let label = if arg.label.is_empty() {
                        "\u{2014}"
                    } else {
                        &arg.label
                    };
                    let _ = writeln!(
                        md,
                        "| {} | `{}` | {} | {} | {} |",
                        label,
                        arg.name,
                        multiplicity,
                        argument_type_name(&arg.argument_type),
                        default_value_display(&arg.default),
                    );
                }
                let _ = writeln!(md);
            }

            let _ = writeln!(md, "---\n");
        }
    }

    Ok(E::Value::from_string(md))
}

/// Register core commands via macro.
///
/// The caller must define `type CommandEnvironment = ...` in scope before invoking.
#[macro_export]
macro_rules! register_core_commands {
    ($cr:expr) => {{
        use liquers_macro::register_command;
        use $crate::commands::*;

        register_command!($cr,
            fn to_text(state, context) -> result
            label: "To text"
            doc: "Convert input state to string"
            filename: "text.txt"
        )?;
        register_command!($cr, fn to_metadata(state, context) -> result
            label: "To metadata"
            doc: "Extract metadata from input state"
            filename: "metadata.json"
        )?;
        register_command!($cr,
            fn commands_doc(state, namespace: String = "", command_name: String = "", context) -> result
            label: "Commands documentation"
            doc: "Generate markdown documentation of registered commands"
            filename: "commands.md"
        )?;
        Ok::<(), liquers_core::error::Error>(())
    }};
}

/// Backward-compatible wrapper calling the `register_core_commands!` macro.
pub fn register_commands(mut env:DefaultEnvironment<Value>) -> Result<DefaultEnvironment<Value>, Error> {
    let cr = env.get_mut_command_registry();
    type CommandEnvironment = DefaultEnvironment<Value>;
    register_core_commands!(cr)?;
    Ok(env)
}

/// Master registration macro including all command domains and lui commands.
///
/// The caller must define `type CommandEnvironment = ...` in scope before invoking.
/// Since this includes `register_lui_commands!`, the environment's `Payload` must
/// implement `UIPayload`.
#[macro_export]
macro_rules! register_all_commands {
    ($cr:expr) => {{
        $crate::register_core_commands!($cr)?;
        $crate::register_egui_commands!($cr)?;
        $crate::register_image_commands!($cr)?;
        $crate::register_polars_commands!($cr)?;
        $crate::register_lui_commands!($cr)?;
        Ok::<(), liquers_core::error::Error>(())
    }};
}

/// Backward-compatible function registering all commands except lui (no payload required).
pub fn register_all_commands_fn(mut env:DefaultEnvironment<Value>) -> Result<DefaultEnvironment<Value>, Error> {
    env = register_commands(env)?;
    env = crate::egui::commands::register_commands(env)?;
    #[cfg(feature = "image-support")]
    {
        env = crate::image::commands::register_commands(env)?;
    }
    Ok(env)
}
