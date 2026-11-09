//! The `ns-template` commands: `render` (the template is an argument), `expand` (the template is
//! the state) and `validate` (diagnostics, nothing runs). `render` and `expand` build a
//! [`RenderRequest`] from their arguments and call [`render_in_context`], the function a language
//! integration also calls.
//!
//! # Main components
//!
//! - [`register_template_commands!`](crate::register_template_commands): registers the three
//!   commands in namespace `template`.
//! - [`render`], [`expand`], [`validate`]: the command functions.
//!
//! # Where it is used
//!
//! - Queries such as `-R/page.md/-/ns-template/expand` and template commands, aliases of
//!   `template/render` with the template link as head (`specs/guides/TEMPLATE_GUIDE.md` §3).
//! - `liquers-lib/tests/template_commands.rs`, end to end.
//! - The command registries of the servers and bindings built with the default features.
//!
//! # Related concepts
//!
//! - [`top_level`](super::top_level): the render these commands call, also callable at the top
//!   level of a program, with frames given directly.
//! - [`ContextHost`](super::ContextHost): evaluates a template's queries and partials.
//!
//! # Examples
//!
//! ```
//! # use liquers_core::{context::{Context, Environment}, error::Error, parse::parse_query};
//! # use liquers_core::command_metadata::{ArgumentType, CommandDefinition, CommandParameterValue};
//! # use liquers_core::value::ValueInterface;
//! # use liquers_lib::{environment::{CommandRegistryAccess, DefaultEnvironment}, value::Value};
//! # use liquers_macro::register_command;
//! # type CommandEnvironment = DefaultEnvironment<Value>;
//! # fn page() -> Result<Value, Error> { Ok(Value::from("{{x}} and {{$ answer}}")) }
//! # fn answer() -> Result<Value, Error> { Ok(Value::from(42)) }
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut env = CommandEnvironment::new();
//! let cr = env.get_mut_command_registry();
//! liquers_lib::register_template_commands!(cr)?;
//! register_command!(cr, fn page() -> result)?;
//! register_command!(cr, fn answer() -> result)?;
//! let envref = env.to_ref();
//! let query = parse_query("page/ns-template/expand")?;
//! let text = envref.evaluate(&query).await?.get().await?.try_into_string()?;
//! assert_eq!(text, " and 42"); // `x` is not defined: an empty name renders nothing
//! # Ok(()) }
//! ```

use std::sync::Arc;

use liquers_core::context::{Context, Environment};
use liquers_core::data_node::{AnyNode, NodeKind};
use liquers_core::error::Error;
use liquers_core::query::Key;
use liquers_core::state::State;
use liquers_core::value::ValueInterface;
use liquers_template::{validate_template, ErrorMode, FrameKind, TemplateSyntax};

use super::top_level::{render_in_context, FrameData, RenderRequest, TemplateSource};
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

/// HTML escaping is on when the command's result is named as HTML.
async fn html_escape<E: Environment<Value = Value>>(context: &Context<E>) -> Result<bool, Error> {
    let metadata = context.get_metadata().await?;
    Ok(matches!(metadata.extension().as_deref(), Some("html" | "htm")))
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
    let errors = ErrorMode::parse(&errors)?;
    let mut frames = Vec::new();
    if !state.is_none() {
        frames.push((FrameData::State(state), FrameKind::Data));
    }
    if let Some(data) = data {
        frames.push((FrameData::Node(data), FrameKind::Names));
    }
    let request = RenderRequest {
        template: TemplateSource::Text(template),
        frames,
        errors,
        cwd,
        html_escape: html_escape(&context).await?,
    };
    Ok(Value::from(render_in_context(context, request).await?.text))
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
    let errors = ErrorMode::parse(&errors)?;
    let frames = data
        .map(|data| vec![(FrameData::Node(data), FrameKind::Data)])
        .unwrap_or_default();
    let request = RenderRequest {
        template: TemplateSource::Text(template),
        frames,
        errors,
        cwd,
        html_escape: html_escape(&context).await?,
    };
    Ok(Value::from(render_in_context(context, request).await?.text))
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
