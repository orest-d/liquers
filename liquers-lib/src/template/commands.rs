//! The `ns-template` commands: `render` (the template is an argument), `expand` (the template is
//! the state) and `validate` (diagnostics, nothing runs).

use std::sync::Arc;

use liquers_core::context::{Context, Environment};
use liquers_core::data_node::{AnyNode, NodeKind};
use liquers_core::error::Error;
use liquers_core::query::Key;
use liquers_core::state::State;
use liquers_core::value::ValueInterface;
use liquers_template::{
    parse_template, render as render_template, validate_template, ContextStack, ErrorMode,
    FrameKind, RenderOptions, TemplateSyntax,
};

use super::host::ContextHost;
use crate::value::Value;

/// `data` as a frame: a map of named values, or `None` for empty text or none. Anything else is an
/// error, since it would have no names.
fn data_node(data: Value) -> Result<Option<AnyNode>, Error> {
    let node = Value::data_node(Arc::new(data));
    match node.kind() {
        NodeKind::Map => Ok(Some(node)),
        NodeKind::Null => Ok(None),
        NodeKind::Text => match &node {
            AnyNode::Sync(n) if n.scalar().is_some_and(|s| s.to_text().is_empty()) => Ok(None),
            AnyNode::Sync(_) | AnyNode::Async(_) => Err(data_not_a_map(NodeKind::Text)),
        },
        kind @ (NodeKind::Bool
        | NodeKind::Integer
        | NodeKind::Float
        | NodeKind::List
        | NodeKind::Opaque) => Err(data_not_a_map(kind)),
    }
}

fn data_not_a_map(kind: NodeKind) -> Error {
    Error::general_error(format!(
        "template data must be a map of named values, or empty; got {}",
        liquers_template::name::kind_name(kind)
    ))
}

/// Renders `template` over `stack`; HTML escaping is on when the result is named as HTML.
async fn render_with<E: Environment<Value = Value>>(
    template: &str,
    stack: ContextStack<'_>,
    errors: &str,
    host: &ContextHost<E>,
    context: &Context<E>,
) -> Result<Value, Error> {
    let errors = ErrorMode::parse(errors)?;
    let parsed = parse_template(template, TemplateSyntax::Mustache)?;
    let metadata = context.get_metadata().await?;
    let html_escape = matches!(metadata.extension().as_deref(), Some("html" | "htm"));
    let options = RenderOptions {
        html_escape,
        errors,
        ..RenderOptions::default()
    };
    let text = render_template(&parsed, stack, host, &options).await?;
    Ok(Value::from(text))
}

/// Renders the template given as `template`. The state is the bottom frame (and `{{.}}`), `data`
/// a frame of names above it.
pub async fn render<E: Environment<Value = Value>>(
    state: State<Value>,
    template: String,
    data: Value,
    errors: String,
    cwd: Key,
    context: Context<E>,
) -> Result<Value, Error> {
    let data = data_node(data)?;
    let host = ContextHost::new(context.clone(), cwd);
    let mut stack = ContextStack::new();
    if !state.is_none() {
        stack.push(host.node_of(&state), FrameKind::Data);
    }
    if let Some(data) = data {
        stack.push(data, FrameKind::Names);
    }
    render_with(&template, stack, &errors, &host, &context).await
}

/// Renders the state as a template; `data` is its data and `{{.}}`.
pub async fn expand<E: Environment<Value = Value>>(
    state: State<Value>,
    data: Value,
    errors: String,
    cwd: Key,
    context: Context<E>,
) -> Result<Value, Error> {
    let template = state.try_into_string()?;
    let data = data_node(data)?;
    let host = ContextHost::new(context.clone(), cwd);
    let mut stack = ContextStack::new();
    if let Some(data) = data {
        stack.push(data, FrameKind::Data);
    }
    render_with(&template, stack, &errors, &host, &context).await
}

/// The state's diagnostics as a template, a list of `{severity, message, position}`; runs nothing.
pub async fn validate<E: Environment<Value = Value>>(
    state: State<Value>,
    context: Context<E>,
) -> Result<Value, Error> {
    let template = state.try_into_string()?;
    let envref = context.get_envref();
    let diagnostics = validate_template(
        &template,
        TemplateSyntax::Mustache,
        Some(envref.get_command_metadata_registry()),
    );
    let json = serde_json::to_value(&diagnostics)
        .map_err(|e| Error::general_error(format!("cannot write diagnostics: {e}")))?;
    Value::try_from_json_value(&json)
}

/// Registers the `ns-template` commands.
///
/// The caller must define `type CommandEnvironment = ...` in scope before invoking.
#[macro_export]
macro_rules! register_template_commands {
    ($cr:expr) => {{
        use liquers_macro::register_command;
        use $crate::template::commands::*;
        #[allow(unused_imports)]
        use $crate::value::Value;
        #[allow(unused_imports)]
        use liquers_core::query::Key;

        register_command!($cr,
            async fn render(state, template: String, data: Value = "", errors: String = "strict",
                cwd: Key = query "-R-key/.", context) -> result
            namespace: "template"
            label: "Render template"
            doc: "Render the template given as the first argument; the state and `data` are its data"
        )?;

        register_command!($cr,
            async fn expand(state, data: Value = "", errors: String = "strict",
                cwd: Key = query "-R-key/.", context) -> result
            namespace: "template"
            label: "Expand template"
            doc: "Render the state as a template; `data` is its data"
        )?;

        register_command!($cr,
            async fn validate(state, context) -> result
            namespace: "template"
            label: "Validate template"
            doc: "Positioned errors and warnings for the state as a template; runs nothing"
        )?;

        Ok::<(), liquers_core::error::Error>(())
    }};
}
