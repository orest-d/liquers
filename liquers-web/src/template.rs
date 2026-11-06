//! `TEMPLATE` — rendering Liquers templates from JavaScript.
//!
//! A template is query-aware Mustache (`specs/reference/TEMPLATE_LANGUAGE.md`): `{{name}}` reads
//! data, `{{$ q}}` evaluates a query, `{{> key}}` includes a partial from the store. Three ways in,
//! all over one implementation, [`render_in`], so a template gives the same text however it is
//! called:
//!
//! 1. The `ns-template` commands (`render`, `expand`, `validate`), registered in every web
//!    environment by `crate::builtins` (`TEMPLATE01`).
//! 2. **Template commands**: a declaration with `template` in place of `run` is an alias of
//!    `template/render` with the template link as its head parameter (`crate::command::spec`,
//!    `TEMPLATE02`, `TEMPLATE03`).
//! 3. **Programmatic calls** with one signature on two receivers: the environment
//!    (`liquers.render` / `expand` / `validateTemplate`, `Environment.render` / `expand` /
//!    `validate`) and a command's `Context` (`context.render` / `expand` / `validate`).
//!
//! # Main components
//!
//! - [`TemplateSource`] — the template: a string is template **text**; a `Query` (from
//!   `liquers.Query.parse`) is a **link** whose value is the template, read as a dependency.
//! - [`Frames`] — the data: `render(template, state, data)` puts the state at the bottom (and `.`)
//!   and `data` above it as names; `expand(template, data)` makes `data` the only frame. The same
//!   layout as the commands, so a programmatic call matches the equivalent query (`TEMPLATE05`).
//! - [`render_in`] — renders inside a command's `Context` through `ContextHost`, so every query
//!   tag and partial becomes a dependency of that asset and recovered errors go to its log.
//! - [`render_top_level`] — the same inside a temporary asset of the environment, returning the
//!   recovered errors beside the text (`TEMPLATE04`, `TEMPLATE07`). They are collected by
//!   `CollectingHost`, a `ContextHost` wrapper, because the temporary asset's log is not drained
//!   outside a run.
//! - [`validate`] — positioned diagnostics; runs nothing (`TEMPLATE09`).
//!
//! # Where it is used
//!
//! The module functions in `lib.rs`, the `Environment` methods in `crate::environment`, and the
//! `Context` methods in `crate::command::context`.
//!
//! # Related concepts
//!
//! `liquers_lib::template::commands` holds the commands this mirrors; `ContextHost` is their host
//! and ours. Options: `errors` (`strict`, the default, `expand`, `expand_html`, `expand_md`,
//! `silent`), `html` (escape for HTML; default off, since there is no filename to derive it from)
//! and `cwd` (the key a relative partial or key resolves against; the root by default). The guide
//! is `LANGUAGE-INTEGRATION_GUIDE.md` §TEMPLATE.
//!
//! # Examples
//!
//! Run by `template04_top_level_expand_over_an_object` and `template07_…` in
//! `tests/template_TEMPLATE.rs` (`number` is a command the test registers):
//!
//! ```javascript
//! await liquers.init();
//! await liquers.expand("{{name}} has {{$ number-42}}", { name: "Ann" });   // "Ann has 42"
//! await liquers.expand("ok\n{{$ nope}}", null, { errors: "silent", full: true });
//! // { text: "ok\n", warnings: ["…nope…"] }
//! ```

use std::sync::Arc;

use liquers_core::assets::AssetRef;
use liquers_core::context::{Context, EnvRef, Environment};
use liquers_core::data_node::{AnyNode, NodeKind};
use liquers_core::error::{Error, ErrorType};
use liquers_core::command_metadata::CommandMetadataRegistry;
use liquers_core::query::{Key, Query};
use liquers_core::state::State;
use liquers_core::value::ValueInterface;
use liquers_lib::template::{
    parse_template, render as render_template, validate_template, ContextHost, ContextStack,
    ErrorMode, FrameKind, QueryRequest, RenderOptions, TemplateHost, TemplateSyntax,
};
use liquers_lib::value::Value;
use wasm_bindgen::prelude::*;

use crate::bridge::{js_to_value, serialize_to_js, ConversionPolicy};

/// Where the template comes from.
#[derive(Debug, Clone)]
pub enum TemplateSource {
    /// The template itself.
    Text(String),
    /// A query whose value is the template, read as a dependency of the render.
    Link(Query),
}

/// The data frames, laid out as the `ns-template` commands lay them out.
#[derive(Debug, Clone)]
pub enum Frames {
    /// `render`: the state at the bottom (and `.`), `data` as names above it.
    Render { state: Value, data: Value },
    /// `expand`: `data` is the only frame, and `.`.
    Expand { data: Value },
}

/// How a render runs.
#[derive(Debug, Clone)]
pub struct TemplateOptions {
    /// The error mode, as `ErrorMode::parse` reads it.
    pub errors: String,
    /// Escape `{{x}}` and `{{$ q}}` for HTML.
    pub html: bool,
    /// What a relative partial or key resolves against.
    pub cwd: Key,
    /// Return `{ text, warnings }` instead of the text (top level only).
    pub full: bool,
}

impl Default for TemplateOptions {
    fn default() -> Self {
        TemplateOptions {
            errors: "strict".to_string(),
            html: false,
            cwd: Key::new(),
            full: false,
        }
    }
}

/// The text and the warnings a recovering mode logged.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Rendered {
    pub text: String,
    pub warnings: Vec<String>,
}

/// `data` as a frame: a map, or `None` when it is empty (none or empty text). As the commands'
/// own rule, so a programmatic call refuses what the query would.
fn data_frame(data: Value) -> Result<Option<AnyNode>, Error> {
    let node = Value::data_node(Arc::new(data));
    match node.kind() {
        NodeKind::Map => Ok(Some(node)),
        NodeKind::Null => Ok(None),
        NodeKind::Text => match &node {
            AnyNode::Sync(n) if n.scalar().is_some_and(|s| s.to_text().is_empty()) => Ok(None),
            AnyNode::Sync(_) | AnyNode::Async(_) => Err(data_not_a_map("text")),
        },
        NodeKind::Bool
        | NodeKind::Integer
        | NodeKind::Float
        | NodeKind::List
        | NodeKind::Opaque => Err(data_not_a_map(liquers_lib::template::name::kind_name(node.kind()))),
    }
}

fn data_not_a_map(kind: &str) -> Error {
    Error::from_error(
        ErrorType::ParameterError,
        format!("template data must be an object of named values, or empty; got {kind}"),
    )
}

/// The template's text: given, or read through `context` as a dependency.
async fn template_text<E: Environment<Value = Value>>(
    context: &Context<E>,
    source: &TemplateSource,
    cwd: &Key,
) -> Result<String, Error> {
    match source {
        TemplateSource::Text(text) => Ok(text.clone()),
        TemplateSource::Link(query) => {
            // Resolved against `cwd` as `ContextHost` resolves a partial: a dependency must be
            // absolute.
            let state = context
                .get_dependency_state(&query.to_absolute(cwd))
                .await?;
            state.error_result()?;
            state.try_into_string()
        }
    }
}

/// Renders inside `context`: the command's own when called from a command, a temporary asset's
/// at the top level. Query tags and partials are dependencies of that asset; recovered errors are
/// warnings in its log.
pub async fn render_in<E: Environment<Value = Value>>(
    context: &Context<E>,
    source: &TemplateSource,
    frames: Frames,
    options: &TemplateOptions,
) -> Result<String, Error> {
    render_collecting(context, source, frames, options)
        .await
        .map(|rendered| rendered.text)
}

/// [`render_in`], also returning the recovered errors. Each is logged to `context` as well, as
/// `ContextHost` logs it.
async fn render_collecting<E: Environment<Value = Value>>(
    context: &Context<E>,
    source: &TemplateSource,
    frames: Frames,
    options: &TemplateOptions,
) -> Result<Rendered, Error> {
    let errors = ErrorMode::parse(&options.errors)?;
    let text = template_text(context, source, &options.cwd).await?;
    let parsed = parse_template(&text, TemplateSyntax::Mustache)?;
    let host = CollectingHost {
        inner: ContextHost::new(context.clone(), options.cwd.clone()),
        warnings: std::sync::Mutex::new(Vec::new()),
    };
    let mut stack = ContextStack::new();
    match frames {
        Frames::Render { state, data } => {
            let data = data_frame(data)?;
            if !state.is_none() {
                stack.push(
                    host.inner.node_of(&State::new().with_data(state)),
                    FrameKind::Data,
                );
            }
            if let Some(data) = data {
                stack.push(data, FrameKind::Names);
            }
        }
        Frames::Expand { data } => {
            if let Some(data) = data_frame(data)? {
                stack.push(data, FrameKind::Data);
            }
        }
    }
    let render_options = RenderOptions {
        html_escape: options.html,
        errors,
        ..RenderOptions::default()
    };
    let text = render_template(&parsed, stack, &host, &render_options).await?;
    let warnings = match host.warnings.into_inner() {
        Ok(warnings) => warnings,
        Err(poisoned) => poisoned.into_inner(),
    };
    Ok(Rendered { text, warnings })
}

/// `ContextHost`, keeping a copy of every recovered error.
///
/// A top-level render runs in a temporary asset whose log the browser never drains (there is no
/// service loop to drain it outside a run), so the warnings a `full` result returns are collected
/// here rather than read back from that log.
struct CollectingHost<E: Environment<Value = Value>> {
    inner: ContextHost<E>,
    warnings: std::sync::Mutex<Vec<String>>,
}

#[async_trait::async_trait(?Send)]
impl<E: Environment<Value = Value>> TemplateHost for CollectingHost<E> {
    async fn evaluate(&self, request: QueryRequest) -> Result<AnyNode, Error> {
        self.inner.evaluate(request).await
    }

    async fn partial(&self, key: &Key) -> Result<String, Error> {
        self.inner.partial(key).await
    }

    fn command_registry(&self) -> &CommandMetadataRegistry {
        self.inner.command_registry()
    }

    fn warn(&self, error: &Error) {
        self.inner.warn(error);
        match self.warnings.lock() {
            Ok(mut warnings) => warnings.push(error.to_string()),
            Err(poisoned) => poisoned.into_inner().push(error.to_string()),
        }
    }
}

/// Renders at the top level, inside a temporary asset of `envref`: never cached, never shared,
/// with its own log, which is where the warnings come from.
pub async fn render_top_level<E: Environment<Value = Value>>(
    envref: EnvRef<E>,
    source: TemplateSource,
    frames: Frames,
    options: TemplateOptions,
) -> Result<Rendered, Error> {
    let context = Context::new(AssetRef::new_temporary(envref), false).await;
    render_collecting(&context, &source, frames, &options).await
}

/// The template's diagnostics, as `{ severity, message, position }` objects. Runs nothing; a link
/// is read (through `context`) only to get the text.
pub async fn validate<E: Environment<Value = Value>>(
    context: &Context<E>,
    source: &TemplateSource,
) -> Result<JsValue, Error> {
    // `validate` takes no `cwd`: a relative link resolves against the root.
    let text = template_text(context, source, &Key::new()).await?;
    let envref = context.get_envref();
    let diagnostics = validate_template(
        &text,
        TemplateSyntax::Mustache,
        Some(envref.get_command_metadata_registry()),
    );
    serialize_to_js(&diagnostics).map_err(|e| conversion_error("diagnostics", e))
}

/// [`validate`] at the top level, in a temporary asset.
pub async fn validate_top_level<E: Environment<Value = Value>>(
    envref: EnvRef<E>,
    source: TemplateSource,
) -> Result<JsValue, Error> {
    let context = Context::new(AssetRef::new_temporary(envref), false).await;
    validate(&context, &source).await
}

// ---------------------------------------------------------------------------------------------
// Reading the JavaScript arguments
// ---------------------------------------------------------------------------------------------

/// A string is template text; a `Query` wrapper is a link.
pub fn source_from_js(js: &JsValue) -> Result<TemplateSource, Error> {
    if let Some(text) = js.as_string() {
        return Ok(TemplateSource::Text(text));
    }
    if crate::objects::is_query_wrapper(js) {
        let encoded = crate::objects::encode_query_wrapper(js)?;
        return Ok(TemplateSource::Link(liquers_core::parse::parse_query(&encoded)?));
    }
    Err(Error::from_error(
        ErrorType::ParameterError,
        "A template is its text (a string) or a link to it (a Query, from liquers.Query.parse)"
            .to_string(),
    ))
}

/// A state or data value, structurally converted; `undefined` and `null` are empty.
pub fn value_from_js(js: &JsValue) -> Result<Value, Error> {
    js_to_value::<Value>(js, ConversionPolicy::Structural)
}

/// `{ errors?, html?, cwd?, full? }`; unknown keys are refused, so a misspelled option is not
/// silently ignored.
pub fn options_from_js(js: &JsValue) -> Result<TemplateOptions, Error> {
    let mut options = TemplateOptions::default();
    if js.is_undefined() || js.is_null() {
        return Ok(options);
    }
    if !js.is_object() {
        return Err(option_error("the options are an object"));
    }
    let object = js_sys::Object::from(js.clone());
    for key in js_sys::Object::keys(&object).iter() {
        let name = key.as_string().unwrap_or_default();
        let value = js_sys::Reflect::get(&object, &key).unwrap_or(JsValue::UNDEFINED);
        if value.is_undefined() {
            continue;
        }
        match name.as_str() {
            "errors" => {
                options.errors = value
                    .as_string()
                    .ok_or_else(|| option_error("`errors` is a string"))?
            }
            "html" => {
                options.html = value
                    .as_bool()
                    .ok_or_else(|| option_error("`html` is a boolean"))?
            }
            "full" => {
                options.full = value
                    .as_bool()
                    .ok_or_else(|| option_error("`full` is a boolean"))?
            }
            "cwd" => {
                let text = match value.as_string() {
                    Some(text) => text,
                    None if crate::objects::is_key_wrapper(&value) => {
                        crate::objects::encode_key_wrapper(&value)?
                    }
                    None => return Err(option_error("`cwd` is a key (a string or a Key)")),
                };
                options.cwd = liquers_core::parse::parse_key(&text)?;
            }
            other => {
                return Err(option_error(&format!(
                    "unknown option {other:?}; expected errors, html, cwd or full"
                )))
            }
        }
    }
    Ok(options)
}

fn conversion_error(what: &str, e: impl std::fmt::Display) -> Error {
    Error::from_error(
        ErrorType::ConversionError,
        format!("Could not convert template {what}: {e}"),
    )
}

fn option_error(message: &str) -> Error {
    Error::from_error(
        ErrorType::ParameterError,
        format!("Template options: {message}"),
    )
}

/// The result a page receives: the text, or `{ text, warnings }` with `full`.
pub fn rendered_to_js(rendered: Rendered, full: bool) -> Result<JsValue, Error> {
    if full {
        serialize_to_js(&rendered).map_err(|e| conversion_error("rendered text", e))
    } else {
        Ok(JsValue::from_str(&rendered.text))
    }
}
