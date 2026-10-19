//! Query-aware templates for Liquers.
//!
//! A template is text with tags. Standard Mustache tags read the template's data; query tags
//! (`{{$ q}}`, `{{$$ q}}` and their section, partial and apply forms) evaluate Liquers queries
//! through a [`TemplateHost`]. The query inside a hygienic tag is parsed once into a
//! [`QueryTemplate`], and a placeholder is filled in the AST, so a value can never change the
//! query's structure.
//!
//! The crate depends on `liquers-core` only and adds no async runtime. The commands that render
//! templates over Liquers values live in `liquers-lib` (feature `templates`). Reference:
//! `specs/reference/TEMPLATE_LANGUAGE.md`; guide: `specs/guides/TEMPLATE_GUIDE.md`.

pub mod diagnostics;
pub mod host;
pub mod mustache;
pub mod name;
pub mod query;
pub mod render;
pub mod stack;

pub use diagnostics::{validate_template, Diagnostic, Severity};
pub use host::{MapHost, QueryRequest, TemplateHost};
pub use mustache::ast::{MustacheTemplate, Template};
pub use name::{LookupKey, Name, NamePart};
pub use query::ast::{
    ActionParameterTemplate, ActionRequestTemplate, KeyTemplate, Piece, QuerySegmentTemplate,
    QueryTemplate, ResourceQuerySegmentTemplate, Slot, TokenTemplate,
    TransformQuerySegmentTemplate,
};
pub use query::instantiate::Instantiated;
pub use query::parse::parse_query_template;
pub use render::{render, ErrorMode, ExpandFormat, RenderOptions};
pub use stack::{ContextStack, FrameKind};

use liquers_core::error::Error;
use liquers_core::query::Position;

/// The template syntaxes; one variant per syntax.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TemplateSyntax {
    Mustache,
}

/// Parses `text` as a template. Only irrecoverable errors fail; a tag that does not parse is kept
/// as an `Invalid` node ([`Template::errors`]).
pub fn parse_template(text: &str, syntax: TemplateSyntax) -> Result<Template, Error> {
    match syntax {
        TemplateSyntax::Mustache => mustache::parse::parse_mustache(text).map(Template::Mustache),
    }
}

/// The position after `text`, which starts at `pos`.
pub(crate) fn advance(pos: &Position, text: &str) -> Position {
    let mut line = pos.line.max(1);
    let mut column = pos.column.max(1);
    for c in text.chars() {
        if c == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    Position::new(pos.offset + text.len(), line, column)
}
