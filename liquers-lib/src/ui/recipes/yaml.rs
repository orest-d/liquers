//! Syntax highlighting of a `recipes.yaml` text (AC-12): keys, punctuation, scalars, comments,
//! and the value of each `query:` coloured as a query.
//!
//! The highlighter is line based and covers what recipe files use: block mappings and lists,
//! flow mappings (`- {query: a.csv, title: A}`), quoted and plain scalars and comments. A line it
//! does not understand is plain text; it never fails.

use std::ops::Range;

use liquers_core::query::{StyledQuery, StyledQueryToken};

use crate::ui::theme::SyntaxRole;

/// The coloured spans of `text`, in order, covering it completely. Roles reuse the query
/// highlighter's [`SyntaxRole`]s: a key is `Resource`, punctuation and comments `Separator`, a
/// number, boolean or null `Parameter`, other text `Plain`; a query is coloured by its own tokens.
pub fn yaml_spans(text: &str) -> Vec<(Range<usize>, SyntaxRole)> {
    let mut out = Vec::new();
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        line_spans(line, offset, &mut out);
        offset += line.len();
    }
    merge(out)
}

fn push(out: &mut Vec<(Range<usize>, SyntaxRole)>, start: usize, end: usize, role: SyntaxRole) {
    if end > start {
        out.push((start..end, role));
    }
}

/// Joins neighbouring spans of one role.
fn merge(spans: Vec<(Range<usize>, SyntaxRole)>) -> Vec<(Range<usize>, SyntaxRole)> {
    let mut out: Vec<(Range<usize>, SyntaxRole)> = Vec::new();
    for (range, role) in spans {
        match out.last_mut() {
            Some((last, last_role)) if *last_role == role && last.end == range.start => last.end = range.end,
            Some(_) | None => out.push((range, role)),
        }
    }
    out
}

fn line_spans(line: &str, base: usize, out: &mut Vec<(Range<usize>, SyntaxRole)>) {
    let body_end = line.trim_end_matches(['\n', '\r']).len();
    let mut i = 0;
    let bytes = line.as_bytes();
    // Indentation and list markers.
    loop {
        let start = i;
        while i < body_end && bytes[i] == b' ' {
            i += 1;
        }
        push(out, base + start, base + i, SyntaxRole::Plain);
        if i < body_end && bytes[i] == b'-' && (i + 1 == body_end || bytes[i + 1] == b' ') {
            push(out, base + i, base + i + 1, SyntaxRole::Separator);
            i += 1;
            continue;
        }
        break;
    }
    if i < body_end && bytes[i] == b'#' {
        push(out, base + i, base + body_end, SyntaxRole::Separator);
    } else if i < body_end && bytes[i] == b'{' {
        flow(line, i, body_end, base, out);
    } else {
        entry(line, i, body_end, base, out);
    }
    push(out, base + body_end, base + line.len(), SyntaxRole::Plain);
}

/// The end of a key (`name:` followed by a space or the end), if the text at `start` is one.
fn key_end(line: &str, start: usize, end: usize) -> Option<usize> {
    let text = &line[start..end];
    let colon = text.find(':')?;
    let key = &text[..colon];
    let simple = !key.is_empty()
        && key
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '.');
    let after = text[colon + 1..].chars().next();
    (simple && matches!(after, None | Some(' '))).then_some(start + colon)
}

/// `key: value` or a bare value, from `start` to `end`.
fn entry(line: &str, start: usize, end: usize, base: usize, out: &mut Vec<(Range<usize>, SyntaxRole)>) {
    let mut i = start;
    let mut key = "";
    if let Some(colon) = key_end(line, start, end) {
        key = &line[start..colon];
        push(out, base + start, base + colon, SyntaxRole::Resource);
        push(out, base + colon, base + colon + 1, SyntaxRole::Separator);
        i = colon + 1;
    }
    // A trailing comment, outside quotes.
    let value_end = comment_start(line, i, end).unwrap_or(end);
    value(line, i, value_end, key, base, out);
    push(out, base + value_end, base + end, SyntaxRole::Separator);
}

fn comment_start(line: &str, start: usize, end: usize) -> Option<usize> {
    let bytes = line.as_bytes();
    let mut quote: Option<u8> = None;
    for i in start..end {
        match (quote, bytes[i]) {
            (None, b'"' | b'\'') => quote = Some(bytes[i]),
            (Some(q), c) if c == q => quote = None,
            (None, b'#') if i == start || bytes[i - 1] == b' ' => return Some(i),
            (Some(_), _) | (None, _) => {}
        }
    }
    None
}

/// A scalar value from `start` to `end`; the value of `query` is coloured as a query.
fn value(line: &str, start: usize, end: usize, key: &str, base: usize, out: &mut Vec<(Range<usize>, SyntaxRole)>) {
    let text = &line[start..end];
    let lead = text.len() - text.trim_start().len();
    let trail = text.len() - text.trim_end().len();
    push(out, base + start, base + start + lead, SyntaxRole::Plain);
    let (vs, ve) = (start + lead, end - trail);
    if vs < ve {
        let v = &line[vs..ve];
        let quoted = v.len() >= 2
            && ((v.starts_with('"') && v.ends_with('"')) || (v.starts_with('\'') && v.ends_with('\'')));
        let (is, ie) = if quoted { (vs + 1, ve - 1) } else { (vs, ve) };
        if quoted {
            push(out, base + vs, base + is, SyntaxRole::Separator);
        }
        let inner = &line[is..ie];
        if key == "query" && query_spans(inner, base + is, out) {
        } else if !quoted && is_scalar_literal(inner) {
            push(out, base + is, base + ie, SyntaxRole::Parameter);
        } else {
            push(out, base + is, base + ie, SyntaxRole::Plain);
        }
        if quoted {
            push(out, base + ie, base + ve, SyntaxRole::Separator);
        }
    }
    push(out, base + ve, base + end, SyntaxRole::Plain);
}

fn is_scalar_literal(text: &str) -> bool {
    matches!(text, "true" | "false" | "null" | "~") || text.parse::<f64>().is_ok()
}

/// Colours `text` as a query when its tokens spell it exactly; false otherwise.
fn query_spans(text: &str, base: usize, out: &mut Vec<(Range<usize>, SyntaxRole)>) -> bool {
    let Ok(query) = liquers_core::parse::parse_query(text) else {
        return false;
    };
    let styled = StyledQuery::from(query);
    let pieces: Vec<(&str, SyntaxRole)> = styled
        .tokens
        .iter()
        .map(|token| {
            let t = match token {
                StyledQueryToken::StringParameter(t)
                | StyledQueryToken::Entity(t)
                | StyledQueryToken::Separator(t)
                | StyledQueryToken::ResourceName(t)
                | StyledQueryToken::ActionName(t)
                | StyledQueryToken::Header(t)
                | StyledQueryToken::Highlight(t) => t.as_str(),
            };
            (t, SyntaxRole::from(token))
        })
        .collect();
    if pieces.iter().map(|(t, _)| *t).collect::<String>() != text {
        return false;
    }
    let mut at = base;
    for (t, role) in pieces {
        push(out, at, at + t.len(), role);
        at += t.len();
    }
    true
}

/// A flow mapping `{key: value, …}` from `start` (the brace) to `end`.
fn flow(line: &str, start: usize, end: usize, base: usize, out: &mut Vec<(Range<usize>, SyntaxRole)>) {
    let bytes = line.as_bytes();
    push(out, base + start, base + start + 1, SyntaxRole::Separator);
    let close = line[..end].rfind('}').filter(|c| *c > start).unwrap_or(end);
    let mut i = start + 1;
    while i < close {
        // The next comma outside quotes.
        let mut quote: Option<u8> = None;
        let mut j = i;
        while j < close {
            match (quote, bytes[j]) {
                (None, b'"' | b'\'') => quote = Some(bytes[j]),
                (Some(q), c) if c == q => quote = None,
                (None, b',') => break,
                (Some(_), _) | (None, _) => {}
            }
            j += 1;
        }
        let lead = line[i..j].len() - line[i..j].trim_start().len();
        push(out, base + i, base + i + lead, SyntaxRole::Plain);
        entry(line, i + lead, j, base, out);
        if j < close {
            push(out, base + j, base + j + 1, SyntaxRole::Separator);
        }
        i = j + 1;
    }
    if close < end {
        push(out, base + close, base + close + 1, SyntaxRole::Separator);
        let rest = close + 1;
        let comment = comment_start(line, rest, end).unwrap_or(end);
        push(out, base + rest, base + comment, SyntaxRole::Plain);
        push(out, base + comment, base + end, SyntaxRole::Separator);
    }
}

/// The line and message of a YAML parse error of a recipe list, for the YAML window (AC-12).
pub fn parse_recipes(text: &str) -> Result<liquers_core::recipes::RecipeList, (Option<usize>, String)> {
    if text.trim().is_empty() {
        return Ok(liquers_core::recipes::RecipeList::new());
    }
    serde_yaml::from_str(text).map_err(|e| (e.location().map(|l| l.line()), e.to_string()))
}
