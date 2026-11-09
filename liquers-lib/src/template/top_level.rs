//! Rendering a template from Rust, with its frames given directly: inside a command or at the
//! top level of a program.
//!
//! A language integration's `render` and `expand` (and the `ns-template` commands) all call
//! [`render_in_context`], so there is one render. Inside a command it runs in the command's
//! [`Context`]; at the top level [`render_top_level`] runs it in a closure asset
//! ([`AssetManager::closure_asset`]), so query tags are evaluated with the same status,
//! cancellation, log and dependency recording as a command's.
//!
//! # Main components
//!
//! - [`RenderRequest`]: the template ([`TemplateSource`]), its frames ([`FrameData`] with a
//!   [`FrameKind`]), the error mode, `cwd` for relative keys, and HTML escaping.
//! - [`render_in_context`]: renders a request in a context and returns [`Rendered`] (the text and
//!   one warning per recovered error, with its position).
//! - [`render_top_level`] and [`TopLevelRender`]: the same at the top level, with the asset first,
//!   so the caller can cancel it.
//!
//! # Where it is used
//!
//! - [`commands`](super::commands): `ns-template/render` and `ns-template/expand` build a request
//!   from their arguments.
//! - `liquers-py`: `Environment.render` / `expand` (top level) and `Context.render` / `expand`.
//! - `liquers-lib/tests/template_top_level.rs`.
//!
//! # Related concepts
//!
//! - [`ContextHost`]: the host that evaluates a template's queries and partials through a
//!   context; this module builds one per render and adds the frames and options.
//! - [`liquers_template::render`]: the renderer over any host; this module supplies the Liquers
//!   host and the asset.
//! - `validate` (`liquers_template::validate_template`): diagnostics only, needs no context.
//!
//! # Examples
//!
//! ```
//! # use liquers_core::{context::{Environment, SimpleEnvironment}, query::Key};
//! # use liquers_lib::{template::{top_level::*, ErrorMode}, value::Value};
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let envref = SimpleEnvironment::<Value>::new().to_ref();
//! let request = RenderRequest {
//!     template: TemplateSource::Text("Hello, {{.}}!".into()),
//!     frames: vec![(FrameData::State(liquers_core::state::State::new().with_data(Value::from("Ann"))),
//!                   liquers_lib::template::FrameKind::Data)],
//!     errors: ErrorMode::Strict, cwd: Key::new(), html_escape: false,
//! };
//! let rendered = render_top_level(&envref, request)?.run().await?;
//! assert_eq!(rendered.map(|r| r.text).as_deref(), Some("Hello, Ann!"));
//! # Ok(()) }
//! ```
//!
//! More: `liquers-lib/tests/template_top_level.rs`; reference:
//! `specs/reference/TEMPLATE_LANGUAGE.md` §Rendering from Rust.

use liquers_core::assets::{AssetManager, AssetRef, ClosureAsset};
use liquers_core::context::{Context, EnvRef, Environment};
use liquers_core::data_node::AnyNode;
use liquers_core::error::Error;
use liquers_core::metadata::LogEntry;
use liquers_core::query::{Key, Query};
use liquers_core::state::State;
use liquers_template::{
    parse_template, render as render_template, ContextStack, ErrorMode, FrameKind, RenderOptions,
    TemplateSyntax,
};

use super::host::ContextHost;
use crate::value::Value;

/// Where the template text comes from.
#[derive(Debug, Clone)]
pub enum TemplateSource {
    /// The text itself.
    Text(String),
    /// A query whose state is the text; relative keys resolve against the request's `cwd`. It is
    /// read as a dependency of the render's asset.
    Link(Query),
}

/// The data of one frame.
#[derive(Clone)]
pub enum FrameData {
    /// A state, read through [`ContextHost::node_of`] (so a directory listing or a record source
    /// reads through the render's context, and `_meta` is the state's metadata).
    State(State<Value>),
    /// A node as it is.
    Node(AnyNode),
}

/// Everything a render needs besides its context.
#[derive(Clone)]
pub struct RenderRequest {
    pub template: TemplateSource,
    /// The frames, bottom first.
    pub frames: Vec<(FrameData, FrameKind)>,
    pub errors: ErrorMode,
    /// What relative keys, in queries, partials and a template link, resolve against; `Key::new()`
    /// is the root.
    pub cwd: Key,
    pub html_escape: bool,
}

/// A render's result.
#[derive(Debug, Clone, PartialEq)]
pub struct Rendered {
    pub text: String,
    /// One warning per error the mode recovered, with its template position; the same entries
    /// are in the asset's log.
    pub warnings: Vec<LogEntry>,
}

/// Renders `request` in `context`, a command's or a closure asset's. Query tags and a template link
/// are dependencies of that context's asset; recovered errors go to its log and to the warnings.
pub async fn render_in_context<E: Environment<Value = Value>>(
    context: Context<E>,
    request: RenderRequest,
) -> Result<Rendered, Error> {
    let RenderRequest {
        template,
        frames,
        errors,
        cwd,
        html_escape,
    } = request;
    let text = match template {
        TemplateSource::Text(text) => text,
        TemplateSource::Link(query) => {
            let state = context
                .get_dependency_state(&query.to_absolute(&cwd))
                .await?;
            state.error_result()?;
            state.try_into_string()?
        }
    };
    let parsed = parse_template(&text, TemplateSyntax::Mustache)?;
    let host = ContextHost::new(context, cwd);
    let mut stack = ContextStack::new();
    for (data, kind) in frames {
        let node = match data {
            FrameData::State(state) => host.node_of(&state),
            FrameData::Node(node) => node,
        };
        stack.push(node, kind);
    }
    let options = RenderOptions {
        html_escape,
        errors,
        ..RenderOptions::default()
    };
    let text = render_template(&parsed, stack, &host, &options).await?;
    Ok(Rendered {
        text,
        warnings: host.take_warnings(),
    })
}

/// Prepares a top-level render. Text is parsed now, so a parse error fails here, with no asset;
/// then a closure asset titled `template render` is made, ready to run.
pub fn render_top_level<E: Environment<Value = Value>>(
    envref: &EnvRef<E>,
    request: RenderRequest,
) -> Result<TopLevelRender<E>, Error> {
    match &request.template {
        TemplateSource::Text(text) => {
            parse_template(text, TemplateSyntax::Mustache)?;
        }
        TemplateSource::Link(_) => {}
    }
    let closure = envref.get_asset_manager().closure_asset("template render")?;
    Ok(TopLevelRender { closure, request })
}

/// A top-level render, made by [`render_top_level`]: its asset, and a run that consumes it.
pub struct TopLevelRender<E: Environment> {
    closure: ClosureAsset<E>,
    request: RenderRequest,
}

impl<E: Environment<Value = Value>> TopLevelRender<E> {
    /// The render's asset: clone it to cancel the render or to read its log afterwards. It ends
    /// `Ready` holding the text, not keyed and never stored.
    pub fn asset(&self) -> &AssetRef<E> {
        self.closure.asset()
    }

    /// Runs the render (native). `Ok(None)`: the asset was cancelled before it started; `Err`: the
    /// render failed or was cancelled while running. See [`ClosureAsset::run`].
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn run(self) -> Result<Option<Rendered>, Error> {
        let request = self.request;
        self.closure
            .run(
                move |context| async move {
                    let rendered = render_in_context(context, request).await?;
                    Ok((Value::from(rendered.text.clone()), rendered))
                },
                None,
            )
            .await
    }

    /// As [`Self::run`], in the caller's task (wasm32, inline managers).
    pub async fn run_inline(self) -> Result<Option<Rendered>, Error> {
        let request = self.request;
        self.closure
            .run_inline(
                move |context| async move {
                    let rendered = render_in_context(context, request).await?;
                    Ok((Value::from(rendered.text.clone()), rendered))
                },
                None,
            )
            .await
    }
}
