//! Splitting a template into text and tags.
//!
//! A tag's end is found by balancing the delimiters before its content is read, so a query tag can
//! hold placeholders (`{{$ cmd-{{x}}}}`) and a bad content can be kept as an error while parsing
//! continues after it. Set-delimiter tags take effect as they are met. Standalone tags (a section,
//! close, comment, partial or set-delimiter tag alone on its line) lose their line here, and keep
//! it in their [`Standalone`] for `encode()`.

use liquers_core::error::Error;
use liquers_core::query::Position;

use crate::advance;
use crate::mustache::ast::Standalone;

pub(crate) struct Tag {
    /// The whole tag, delimiters included.
    pub source: String,
    /// Between the delimiters; for a triple tag, inside the extra braces.
    pub content: String,
    pub triple: bool,
    pub open: String,
    pub close: String,
    pub position: Position,
    pub content_position: Position,
    pub standalone: Option<Standalone>,
    /// The delimiters a set-delimiter tag sets.
    pub set_delimiter: Option<(String, String)>,
}

impl Tag {
    /// Whether the tag may stand alone on its line.
    fn standalone_kind(&self) -> bool {
        if self.triple {
            return false;
        }
        matches!(
            self.content.trim_start().chars().next(),
            Some('#' | '^' | '/' | '!' | '>' | '=')
        )
    }
}

pub(crate) enum Token {
    Text { text: String, position: Position },
    Tag(Tag),
}

/// Where the tag that starts at `from` (just after the opening delimiter) ends: the index of its
/// closing delimiter. Nested delimiter pairs are skipped.
fn tag_end(text: &str, from: usize, open: &str, close: &str) -> Option<usize> {
    if open == close {
        return text[from..].find(close).map(|i| from + i);
    }
    let mut depth = 0usize;
    let mut at = from;
    loop {
        let next_open = text[at..].find(open).map(|i| at + i);
        let next_close = text[at..].find(close).map(|i| at + i)?;
        match next_open {
            Some(o) if o < next_close => {
                depth += 1;
                at = o + open.len();
            }
            Some(_) | None => {
                if depth == 0 {
                    return Some(next_close);
                }
                depth -= 1;
                at = next_close + close.len();
            }
        }
    }
}

fn set_delimiter(content: &str, position: &Position) -> Result<Option<(String, String)>, Error> {
    let trimmed = content.trim();
    if !trimmed.starts_with('=') {
        return Ok(None);
    }
    let bad = || {
        Error::parse_error(format!(
            "bad set-delimiter tag '{content}': it is '=', two delimiters separated by whitespace, '='"
        ))
        .with_position(position)
    };
    if trimmed.len() < 2 || !trimmed.ends_with('=') {
        return Err(bad());
    }
    let inner = &trimmed[1..trimmed.len() - 1];
    let parts: Vec<&str> = inner.split_whitespace().collect();
    match parts.as_slice() {
        [open, close] if !open.contains('=') && !close.contains('=') => {
            Ok(Some((open.to_string(), close.to_string())))
        }
        _ => Err(bad()),
    }
}

/// Splits `text` into tokens. Fails only when the structure is unknown: a tag with no closing
/// delimiter, or a bad set-delimiter tag.
pub(crate) fn lex(text: &str) -> Result<Vec<Token>, Error> {
    let mut tokens = Vec::new();
    let mut open = String::from("{{");
    let mut close = String::from("}}");
    let mut at = 0;
    let mut position = Position::new(0, 1, 1);
    while at < text.len() {
        let Some(rel) = text[at..].find(open.as_str()) else {
            tokens.push(Token::Text {
                text: text[at..].to_string(),
                position: position.clone(),
            });
            break;
        };
        let tag_start = at + rel;
        if tag_start > at {
            tokens.push(Token::Text {
                text: text[at..tag_start].to_string(),
                position: position.clone(),
            });
            position = advance(&position, &text[at..tag_start]);
        }
        let tag_position = position.clone();
        let content_start = tag_start + open.len();
        let triple = open == "{{" && text[content_start..].starts_with('{');
        let (content, tag_stop, content_position) = if triple {
            let triple_close = format!("}}{close}");
            let end = tag_end(text, content_start + 1, &open, &triple_close).ok_or_else(|| {
                Error::parse_error(format!("tag has no closing '{triple_close}'"))
                    .with_position(&tag_position)
            })?;
            (
                text[content_start + 1..end].to_string(),
                end + triple_close.len(),
                advance(&tag_position, &text[tag_start..content_start + 1]),
            )
        } else {
            let end = tag_end(text, content_start, &open, &close).ok_or_else(|| {
                Error::parse_error(format!("tag has no closing '{close}'")).with_position(&tag_position)
            })?;
            (
                text[content_start..end].to_string(),
                end + close.len(),
                advance(&tag_position, &text[tag_start..content_start]),
            )
        };
        let source = text[tag_start..tag_stop].to_string();
        let new_delimiters = if triple {
            None
        } else {
            set_delimiter(&content, &tag_position)?
        };
        tokens.push(Token::Tag(Tag {
            source,
            content,
            triple,
            open: open.clone(),
            close: close.clone(),
            position: tag_position,
            content_position,
            standalone: None,
            set_delimiter: new_delimiters.clone(),
        }));
        position = advance(&position, &text[tag_start..tag_stop]);
        at = tag_stop;
        if let Some((o, c)) = new_delimiters {
            open = o;
            close = c;
        }
    }
    mark_standalone(&mut tokens);
    Ok(tokens)
}

fn is_blank(text: &str) -> bool {
    text.chars().all(|c| c == ' ' || c == '\t')
}

/// The whitespace and line ending a text starts with, when it starts with a line end.
fn leading_line_end(text: &str) -> Option<usize> {
    let blank = text.len() - text.trim_start_matches([' ', '\t']).len();
    let rest = &text[blank..];
    if rest.starts_with("\r\n") {
        Some(blank + 2)
    } else if rest.starts_with('\n') {
        Some(blank + 1)
    } else {
        None
    }
}

fn mark_standalone(tokens: &mut [Token]) {
    // The checks read the text as it was lexed; the trims are applied to a copy.
    let original: Vec<Option<String>> = tokens
        .iter()
        .map(|t| match t {
            Token::Text { text, .. } => Some(text.clone()),
            Token::Tag(_) => None,
        })
        .collect();
    let count = tokens.len();
    for i in 0..count {
        let eligible = match &tokens[i] {
            Token::Tag(tag) => tag.standalone_kind(),
            Token::Text { .. } => false,
        };
        if !eligible {
            continue;
        }
        // At the start of a line: the text before is blank since its last line end, and holds a
        // line end (or is the first token).
        let indent = match i.checked_sub(1) {
            None => Some(String::new()),
            Some(p) => match &original[p] {
                Some(text) => match text.rfind('\n') {
                    Some(nl) if is_blank(&text[nl + 1..]) => Some(text[nl + 1..].to_string()),
                    Some(_) => None,
                    None if p == 0 && is_blank(text) => Some(text.clone()),
                    None => None,
                },
                None => None,
            },
        };
        let Some(indent) = indent else { continue };
        let newline = match original.get(i + 1) {
            None => Some(String::new()),
            Some(Some(text)) => match leading_line_end(text) {
                Some(n) => Some(text[..n].to_string()),
                None if i + 2 == count && is_blank(text) => Some(text.clone()),
                None => None,
            },
            Some(None) => None,
        };
        let Some(newline) = newline else { continue };
        if let Some(p) = i.checked_sub(1) {
            if let Token::Text { text, .. } = &mut tokens[p] {
                let keep = text.len() - indent.len();
                text.truncate(keep);
            }
        }
        if let Some(Token::Text { text, position }) = tokens.get_mut(i + 1) {
            *position = advance(position, &text[..newline.len()]);
            text.replace_range(..newline.len(), "");
        }
        if let Token::Tag(tag) = &mut tokens[i] {
            tag.standalone = Some(Standalone { indent, newline });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shape(text: &str) -> Vec<String> {
        lex(text)
            .expect("lex")
            .into_iter()
            .map(|t| match t {
                Token::Text { text, .. } => format!("T{text:?}"),
                Token::Tag(tag) => format!(
                    "G{:?}{}",
                    tag.content,
                    if tag.standalone.is_some() { "s" } else { "" }
                ),
            })
            .collect()
    }

    #[test]
    fn nested_braces_make_one_tag() {
        assert_eq!(shape("a{{$ cmd-{{x}}/q}}b"), vec!["T\"a\"", "G\"$ cmd-{{x}}/q\"", "T\"b\""]);
        assert_eq!(shape("{{{x}}}"), vec!["G\"x\""]);
    }

    #[test]
    fn standalone_lines() {
        assert_eq!(shape("a\n  {{#x}}  \nb\n{{/x}}\n"), vec!["T\"a\\n\"", "G\"#x\"s", "T\"b\\n\"", "G\"/x\"s", "T\"\""]);
        assert_eq!(shape("a {{#x}}\n"), vec!["T\"a \"", "G\"#x\"", "T\"\\n\""]);
    }

    #[test]
    fn set_delimiter_takes_effect() {
        assert_eq!(shape("{{=<% %>=}}<%x%>{{y}}"), vec!["G\"=<% %>=\"", "G\"x\"", "T\"{{y}}\""]);
        assert!(lex("{{=<% %>}}").is_err());
        assert!(lex("{{x").is_err());
    }
}
