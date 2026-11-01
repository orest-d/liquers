//! Inline structure the navigation needs: code spans, links, images, reference links and
//! autolinks. Emphasis is not parsed, so query characters (`~~`, `_`, `*`) are never markup.

use std::collections::HashMap;

use liquers_core::parse::parse_query;
use liquers_core::query::{Key, Query};

use crate::block::{split_lines, Block, BlockKind, Span};

/// How a link is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkKind {
    /// `[text](target)`
    Inline,
    /// `[text][ref]`, `[text][]` or `[text]`, with a `[ref]: target` definition
    Reference,
    /// `<scheme:…>`
    Autolink,
    /// `![alt](target)` or `![alt][ref]`
    Image,
}

/// What a link points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetKind {
    /// A path relative to the document (`../guides/X.md#part`).
    Relative,
    /// An anchor in the same document (`#part`).
    Anchor,
    /// A URL with a scheme (`https:`, `mailto:`) or an absolute path (`/x`).
    Url,
    /// A Liquers query (`-R/data/a.csv/-/to_text`).
    Query,
}

/// One link or image.
#[derive(Debug, Clone, PartialEq)]
pub struct Link {
    pub kind: LinkKind,
    /// The text between the brackets as written (the URL for an autolink).
    pub text: String,
    pub target: String,
    pub target_kind: TargetKind,
    /// The part after `#`, for a relative or anchor target.
    pub anchor: Option<String>,
    pub query: Option<Query>,
    pub span: Span,
    /// The innermost section the link is in.
    pub section: Option<usize>,
}

/// One code span.
#[derive(Debug, Clone, PartialEq)]
pub struct CodeSpan {
    pub text: String,
    pub span: Span,
    /// The text as a query, when it is one.
    pub query: Option<Query>,
}

/// Where a relative link leads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resolved {
    /// The key the target names.
    Key(Key),
    /// The path climbs above the store's root (`../` too many times): a dead link.
    OutsideRoot,
}

impl Link {
    /// A relative target resolved against the key of the document holding the link; `None`
    /// for any other target kind.
    pub fn resolve(&self, document_key: &Key) -> Option<Resolved> {
        if self.target_kind != TargetKind::Relative {
            return None;
        }
        let path = self.target.split(['#', '?']).next().unwrap_or("");
        if path.is_empty() {
            return None;
        }
        let mut resolved = document_key.parent();
        for part in path.split('/') {
            match part {
                "" | "." => {}
                ".." => {
                    if resolved.is_empty() {
                        return Some(Resolved::OutsideRoot);
                    }
                    resolved = resolved.parent();
                }
                name => resolved = resolved.join(name),
            }
        }
        Some(Resolved::Key(resolved))
    }

    /// The key a relative target names, when it stays inside the store.
    pub fn key(&self, document_key: &Key) -> Option<Key> {
        match self.resolve(document_key)? {
            Resolved::Key(key) => Some(key),
            Resolved::OutsideRoot => None,
        }
    }
}

/// The query a text is, when it looks like one (starts with `-` or contains `/-/`) and parses.
pub(crate) fn as_query(text: &str) -> Option<Query> {
    let text = text.trim();
    if text.is_empty() || !(text.starts_with('-') || text.contains("/-/")) {
        return None;
    }
    parse_query(text).ok()
}

fn classify(target: &str) -> (TargetKind, Option<String>, Option<Query>) {
    if let Some(anchor) = target.strip_prefix('#') {
        return (TargetKind::Anchor, Some(anchor.to_string()), None);
    }
    if let Some(query) = as_query(target) {
        return (TargetKind::Query, None, Some(query));
    }
    let scheme = target
        .find(':')
        .map(|i| &target[..i])
        .is_some_and(|s| s.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) && s.chars().all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c)));
    if scheme || target.starts_with('/') {
        return (TargetKind::Url, None, None);
    }
    let anchor = target.split_once('#').map(|(_, a)| a.to_string());
    (TargetKind::Relative, anchor, None)
}

fn normalize_label(label: &str) -> String {
    label.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

fn has_inlines(kind: &BlockKind) -> bool {
    match kind {
        BlockKind::Paragraph | BlockKind::List | BlockKind::BlockQuote | BlockKind::Table | BlockKind::Heading { .. } => true,
        BlockKind::FrontMatter(_)
        | BlockKind::FencedCode { .. }
        | BlockKind::IndentedCode
        | BlockKind::Html
        | BlockKind::ThematicBreak
        | BlockKind::Blank => false,
    }
}

/// `(label, target, line start, line end)` of a `[label]: target` definition line.
fn definition(line: &str) -> Option<(String, String)> {
    let t = line.trim_start();
    if line.len() - t.len() > 3 {
        return None;
    }
    let t = t.trim_start_matches('>').trim_start();
    let rest = t.strip_prefix('[')?;
    let close = rest.find("]:")?;
    let label = &rest[..close];
    if label.trim().is_empty() || label.contains('[') {
        return None;
    }
    let dest = rest[close + 2..].trim();
    let dest = if let Some(inner) = dest.strip_prefix('<') {
        inner.split('>').next().unwrap_or("")
    } else {
        dest.split_whitespace().next().unwrap_or("")
    };
    (!dest.is_empty()).then(|| (normalize_label(label), dest.to_string()))
}

/// Byte offset → 1-based line, by binary search over line starts.
struct LineIndex(Vec<usize>);

impl LineIndex {
    fn new(text: &str) -> LineIndex {
        LineIndex(split_lines(text).iter().map(|l| l.start).collect())
    }
    fn line(&self, offset: usize) -> usize {
        match self.0.binary_search(&offset) {
            Ok(i) => i + 1,
            Err(i) => i.max(1),
        }
    }
    fn span(&self, start: usize, end: usize) -> Span {
        Span { start, end, line: self.line(start), end_line: self.line(end.saturating_sub(1).max(start)) }
    }
}

/// The end of the backtick run starting at `pos` and, if it closes, the closing run's start.
fn code_span_at(bytes: &[u8], pos: usize, end: usize) -> (usize, Option<usize>) {
    let n = bytes[pos..end].iter().take_while(|&&b| b == b'`').count();
    let mut i = pos + n;
    while i < end {
        if bytes[i] == b'`' {
            let m = bytes[i..end].iter().take_while(|&&b| b == b'`').count();
            if m == n {
                return (pos + n, Some(i));
            }
            i += m;
        } else {
            i += 1;
        }
    }
    (pos + n, None)
}

/// The index of the `]` matching the `[` at `pos`, skipping escapes and code spans.
fn matching_bracket(bytes: &[u8], pos: usize, end: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut i = pos;
    while i < end {
        match bytes[i] {
            b'\\' => i += 1,
            b'`' => {
                let (after, close) = code_span_at(bytes, i, end);
                if let Some(c) = close {
                    i = c + (after - i) - 1;
                } else {
                    i = after - 1;
                }
            }
            b'[' => depth += 1,
            b']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// `(target, end after ')')` of an inline destination starting after `(` at `pos`.
fn inline_destination(text: &str, pos: usize, end: usize) -> Option<(String, usize)> {
    let bytes = text.as_bytes();
    let mut i = pos;
    let skip_ws = |mut i: usize| {
        while i < end && (bytes[i] == b' ' || bytes[i] == b'\t' || bytes[i] == b'\n' || bytes[i] == b'\r') {
            i += 1;
        }
        i
    };
    i = skip_ws(i);
    let target;
    if i < end && bytes[i] == b'<' {
        let close = text[i + 1..end].find('>')? + i + 1;
        target = text[i + 1..close].to_string();
        i = close + 1;
    } else {
        let start = i;
        let mut depth = 0usize;
        while i < end {
            match bytes[i] {
                b'\\' => i += 1,
                b'(' => depth += 1,
                b')' if depth == 0 => break,
                b')' => depth -= 1,
                b' ' | b'\t' | b'\n' | b'\r' => break,
                _ => {}
            }
            i += 1;
        }
        target = text[start..i.min(end)].to_string();
    }
    i = skip_ws(i);
    if i < end && (bytes[i] == b'"' || bytes[i] == b'\'' || bytes[i] == b'(') {
        let closer = if bytes[i] == b'(' { b')' } else { bytes[i] };
        let mut j = i + 1;
        while j < end && bytes[j] != closer {
            if bytes[j] == b'\\' {
                j += 1;
            }
            j += 1;
        }
        if j >= end {
            return None;
        }
        i = skip_ws(j + 1);
    }
    (i < end && bytes[i] == b')').then(|| (target, i + 1))
}

fn autolink(text: &str, pos: usize, end: usize) -> Option<(String, usize)> {
    let rest = &text[pos + 1..end];
    let close = rest.find('>')?;
    let inner = &rest[..close];
    let colon = inner.find(':')?;
    let scheme = &inner[..colon];
    let ok = (2..=32).contains(&scheme.len())
        && scheme.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        && scheme.chars().all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c))
        && !inner.contains(|c: char| c.is_whitespace() || c == '<');
    ok.then(|| (inner.to_string(), pos + 1 + close + 1))
}

pub(crate) struct Inlines {
    pub links: Vec<Link>,
    pub code_spans: Vec<CodeSpan>,
}

/// Scans every block that holds inline text.
pub(crate) fn scan(text: &str, blocks: &[Block], section_of: &dyn Fn(usize) -> Option<usize>) -> Inlines {
    let lines = LineIndex::new(text);
    let mut definitions: HashMap<String, String> = HashMap::new();
    let mut skipped: Vec<(usize, usize)> = Vec::new();
    for block in blocks.iter().filter(|b| matches!(b.kind, BlockKind::Paragraph | BlockKind::BlockQuote | BlockKind::List)) {
        for line in split_lines(&text[block.span.start..block.span.end]) {
            let content = &text[block.span.start + line.start..block.span.start + line.content_end];
            if let Some((label, target)) = definition(content) {
                definitions.entry(label).or_insert(target);
                skipped.push((block.span.start + line.start, block.span.start + line.end));
            }
        }
    }
    let bytes = text.as_bytes();
    let mut links = Vec::new();
    let mut code_spans = Vec::new();
    for block in blocks.iter().filter(|b| has_inlines(&b.kind)) {
        let end = block.span.end;
        let mut pos = block.span.start;
        while pos < end {
            if let Some(&(_, skip_end)) = skipped.iter().find(|(a, b)| *a <= pos && pos < *b) {
                pos = skip_end;
                continue;
            }
            match bytes[pos] {
                b'\\' => pos += 2,
                b'`' => {
                    let (after, close) = code_span_at(bytes, pos, end);
                    match close {
                        Some(c) => {
                            let n = after - pos;
                            let mut inner = &text[after..c];
                            if inner.len() >= 2 && inner.starts_with(' ') && inner.ends_with(' ') && !inner.trim().is_empty() {
                                inner = &inner[1..inner.len() - 1];
                            }
                            let inner = inner.replace(['\n', '\r'], " ");
                            code_spans.push(CodeSpan {
                                query: as_query(&inner),
                                text: inner,
                                span: lines.span(pos, c + n),
                            });
                            pos = c + n;
                        }
                        None => pos = after,
                    }
                }
                b'<' => match autolink(text, pos, end) {
                    Some((url, after)) => {
                        let (target_kind, anchor, query) = classify(&url);
                        links.push(Link {
                            kind: LinkKind::Autolink,
                            text: url.clone(),
                            target: url,
                            target_kind,
                            anchor,
                            query,
                            span: lines.span(pos, after),
                            section: section_of(pos),
                        });
                        pos = after;
                    }
                    None => pos += 1,
                },
                b'[' | b'!' => {
                    let image = bytes[pos] == b'!';
                    let open = if image { pos + 1 } else { pos };
                    if image && (open >= end || bytes[open] != b'[') {
                        pos += 1;
                        continue;
                    }
                    let Some(close) = matching_bracket(bytes, open, end) else {
                        pos += 1;
                        continue;
                    };
                    let label_text = &text[open + 1..close];
                    let after = close + 1;
                    let mut found: Option<(LinkKind, String, usize)> = None;
                    if after < end && bytes[after] == b'(' {
                        if let Some((target, stop)) = inline_destination(text, after + 1, end) {
                            found = Some((if image { LinkKind::Image } else { LinkKind::Inline }, target, stop));
                        }
                    } else if after < end && bytes[after] == b'[' {
                        if let Some(rel) = text[after + 1..end].find(']') {
                            let label = &text[after + 1..after + 1 + rel];
                            let label = if label.trim().is_empty() { label_text } else { label };
                            if let Some(target) = definitions.get(&normalize_label(label)) {
                                let kind = if image { LinkKind::Image } else { LinkKind::Reference };
                                found = Some((kind, target.clone(), after + 1 + rel + 1));
                            }
                        }
                    } else if let Some(target) = definitions.get(&normalize_label(label_text)) {
                        let kind = if image { LinkKind::Image } else { LinkKind::Reference };
                        found = Some((kind, target.clone(), after));
                    }
                    match found {
                        Some((kind, target, stop)) => {
                            let (target_kind, anchor, query) = classify(&target);
                            links.push(Link {
                                kind,
                                text: label_text.to_string(),
                                target,
                                target_kind,
                                anchor,
                                query,
                                span: lines.span(pos, stop),
                                section: section_of(pos),
                            });
                            pos = stop;
                        }
                        None => pos += 1,
                    }
                }
                _ => pos += 1,
            }
        }
    }
    // Keep the byte offsets on character boundaries: every jump above lands after an ASCII byte.
    Inlines { links, code_spans }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Document;
    use liquers_core::parse::parse_key;

    fn targets(text: &str) -> Vec<(LinkKind, String, TargetKind)> {
        Document::parse(text).links().into_iter().map(|l| (l.kind, l.target, l.target_kind)).collect()
    }

    #[test]
    fn inline_links_images_and_autolinks() {
        let got = targets("See [a](x.md \"title\"), ![img](p.png), <https://e.org/x> and [s](#part).\n");
        assert_eq!(
            got,
            vec![
                (LinkKind::Inline, "x.md".into(), TargetKind::Relative),
                (LinkKind::Image, "p.png".into(), TargetKind::Relative),
                (LinkKind::Autolink, "https://e.org/x".into(), TargetKind::Url),
                (LinkKind::Inline, "#part".into(), TargetKind::Anchor),
            ]
        );
    }

    #[test]
    fn reference_links_resolve_definitions() {
        let got = targets("Read [the guide][g], [Other][] and [short].\n\n[g]: ../guides/G.md\n[other]: <o.md>\n[Short]: s.md \"t\"\n");
        assert_eq!(
            got,
            vec![
                (LinkKind::Reference, "../guides/G.md".into(), TargetKind::Relative),
                (LinkKind::Reference, "o.md".into(), TargetKind::Relative),
                (LinkKind::Reference, "s.md".into(), TargetKind::Relative),
            ]
        );
    }

    #[test]
    fn links_inside_code_spans_are_ignored() {
        let doc = Document::parse("Code `[a](b.md)` and ``x ` [c](d.md)`` but [e](f.md).\n\n```\n[g](h.md)\n```\n");
        let got: Vec<_> = doc.links().into_iter().map(|l| l.target).collect();
        assert_eq!(got, vec!["f.md".to_string()]);
        let spans: Vec<_> = doc.code_spans().into_iter().map(|c| c.text).collect();
        assert_eq!(spans, vec!["[a](b.md)".to_string(), "x ` [c](d.md)".to_string()]);
    }

    #[test]
    fn query_targets_are_queries() {
        let doc = Document::parse("A [query](-R/data/a_b.csv/-/replace-a~~b-x_y) and `-R/x/-/ns-md/outline`.\n");
        let links = doc.links();
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, "-R/data/a_b.csv/-/replace-a~~b-x_y");
        assert_eq!(links[0].target_kind, TargetKind::Query);
        assert!(links[0].query.is_some());
        let spans = doc.code_spans();
        assert!(spans[0].query.is_some());
    }

    #[test]
    fn link_resolve_against_key() {
        let doc = Document::parse("[a](../guides/G.md#x) [b](./c.md) [c](../../../out.md) [d](#here) [e](https://x.org)\n");
        let key = parse_key("specs/reference/R.md").expect("key");
        let got: Vec<_> = doc.links().iter().map(|l| l.key(&key).map(|k| k.to_string())).collect();
        assert_eq!(
            got,
            vec![Some("specs/guides/G.md".to_string()), Some("specs/reference/c.md".to_string()), None, None, None]
        );
        assert_eq!(doc.links()[2].resolve(&key), Some(Resolved::OutsideRoot));
        assert_eq!(doc.links()[3].resolve(&key), None);
        assert_eq!(doc.links()[0].anchor.as_deref(), Some("x"));
    }

    #[test]
    fn absolute_paths_and_schemes_are_urls() {
        let got = targets("[a](/abs/x.md) [b](mailto:x@y.org) [c](ftp://h/x) [d](https://h)\n");
        assert!(got.iter().all(|(_, _, kind)| *kind == TargetKind::Url), "{got:?}");
    }

    #[test]
    fn links_know_their_section_and_line() {
        let doc = Document::parse("# A\n\n[x](x.md)\n\n## B\n\ntext [y](y.md)\n");
        let got: Vec<_> = doc.links().iter().map(|l| (l.section, l.span.line)).collect();
        assert_eq!(got, vec![(Some(0), 3), (Some(1), 7)]);
    }
}
