//! Query-aware Mustache templates over Liquers values: the `ns-template` commands, the host that
//! evaluates a template's queries through a command's `Context`, and the store folder as data.
//!
//! The template language itself is the `liquers-template` crate, re-exported here. Reference:
//! `specs/reference/TEMPLATE_LANGUAGE.md`; guide: `specs/guides/TEMPLATE_GUIDE.md`.

pub use liquers_template::*;

pub mod commands;
pub mod folder;
pub mod host;
pub mod top_level;

pub use host::ContextHost;
pub use top_level::{
    render_in_context, render_top_level, FrameData, RenderRequest, Rendered, TemplateSource,
    TopLevelRender,
};
