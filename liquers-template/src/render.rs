//! Rendering a template, and the error modes.

use liquers_core::error::Error;
use liquers_core::query::Position;

use crate::host::TemplateHost;
use crate::mustache::ast::Template;
use crate::stack::ContextStack;

/// What a recoverable error does while rendering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorMode {
    /// The render fails with the first error; a template with a syntax error runs nothing.
    Strict,
    /// The error is written in place of its tag, and logged as a warning.
    Expand(ExpandFormat),
    /// The tag renders nothing, and the error is logged as a warning.
    Silent,
}

/// How an expanded error is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExpandFormat {
    Text,
    Html,
    Markdown,
}

impl ErrorMode {
    /// Reads a command's `errors` value: `strict`, `expand`, `expand_html`, `expand_md` or `silent`.
    pub fn parse(text: &str) -> Result<ErrorMode, Error> {
        match text {
            "strict" => Ok(ErrorMode::Strict),
            "expand" => Ok(ErrorMode::Expand(ExpandFormat::Text)),
            "expand_html" => Ok(ErrorMode::Expand(ExpandFormat::Html)),
            "expand_md" => Ok(ErrorMode::Expand(ExpandFormat::Markdown)),
            "silent" => Ok(ErrorMode::Silent),
            other => Err(Error::general_error(format!(
                "errors must be strict, expand, expand_html, expand_md or silent, not '{other}'"
            ))),
        }
    }
}

/// Options of a render.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderOptions {
    /// Escape `{{x}}` and `{{$ q}}` for HTML; `{{{x}}}` and `{{&x}}` are always raw.
    pub html_escape: bool,
    /// How deep partials may nest.
    pub max_partial_depth: usize,
    pub errors: ErrorMode,
}

impl Default for RenderOptions {
    fn default() -> Self {
        RenderOptions {
            html_escape: false,
            max_partial_depth: 32,
            errors: ErrorMode::Strict,
        }
    }
}

/// Renders `template` over `stack`.
pub async fn render<H: TemplateHost>(
    template: &Template,
    stack: ContextStack<'_>,
    host: &H,
    options: &RenderOptions,
) -> Result<String, Error> {
    match template {
        Template::Mustache(m) => crate::mustache::render::render(m, &stack, host, options).await,
    }
}

/// `&`, `<`, `>` and `"` as HTML entities.
pub fn html_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            other => out.push(other),
        }
    }
    out
}

/// An error as written in expand mode.
pub fn expand_error(error: &Error, format: ExpandFormat, fallback: &Position) -> String {
    let position = if error.position.is_unknown() {
        fallback
    } else {
        &error.position
    };
    let at = format!("{}:{}", position.line, position.column);
    match format {
        ExpandFormat::Text => format!("[template error at {at}: {}]", error.message),
        ExpandFormat::Html => format!(
            "<span class=\"liquers-template-error\" title=\"{at}\">template error: {}</span>",
            html_escape(&error.message)
        ),
        ExpandFormat::Markdown => {
            let text = format!("[template error at {at}: {}]", error.message);
            let mut longest = 0;
            let mut run = 0;
            for c in text.chars() {
                if c == '`' {
                    run += 1;
                    longest = longest.max(run);
                } else {
                    run = 0;
                }
            }
            let fence = "`".repeat(longest + 1);
            if longest > 0 {
                format!("{fence} {text} {fence}")
            } else {
                format!("{fence}{text}{fence}")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_expansions() {
        let error = Error::general_error("no field \"titel\" in map".to_string())
            .with_position(&Position::new(0, 12, 7));
        let p = Position::unknown();
        assert_eq!(
            expand_error(&error, ExpandFormat::Text, &p),
            "[template error at 12:7: no field \"titel\" in map]"
        );
        assert_eq!(
            expand_error(&error, ExpandFormat::Html, &p),
            "<span class=\"liquers-template-error\" title=\"12:7\">template error: no field &quot;titel&quot; in map</span>"
        );
        assert_eq!(
            expand_error(&error, ExpandFormat::Markdown, &p),
            "`[template error at 12:7: no field \"titel\" in map]`"
        );
        let ticks = Error::general_error("a `b`".to_string()).with_position(&Position::new(0, 1, 1));
        assert_eq!(expand_error(&ticks, ExpandFormat::Markdown, &p), "`` [template error at 1:1: a `b`] ``");
        assert!(ErrorMode::parse("warp").is_err());
        assert_eq!(ErrorMode::parse("expand_md").ok(), Some(ErrorMode::Expand(ExpandFormat::Markdown)));
    }
}
