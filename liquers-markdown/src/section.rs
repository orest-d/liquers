//! The section tree over the heading blocks.

use crate::block::{Block, BlockKind, Span};

/// A heading and everything up to the next heading of the same or a higher level.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    /// 1–6.
    pub level: u8,
    /// The heading's text, its lines joined by a space.
    pub heading: String,
    /// The heading's own number (`3.2` of `## 3.2 Foo`, `8` of `## 8. Storage`).
    pub number: Option<String>,
    /// The section-number path that resolves to this section (`1.8`).
    pub address: String,
    /// The headings from the top-level section down to this one.
    pub path: Vec<String>,
    /// The GitHub-style anchor of the heading, made unique with `-1`, `-2`, … .
    pub slug: String,
    /// The index of the heading block.
    pub block: usize,
    /// From the heading to the end of its last subsection.
    pub span: Span,
    /// The end (byte) and last line of the section's own text, before its first subsection.
    pub body_end: usize,
    pub body_end_line: usize,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
}

/// The number prefix of a heading: digits separated by dots, optionally ending in a dot,
/// followed by a space or the end.
pub(crate) fn heading_number(heading: &str) -> Option<String> {
    let mut end = 0;
    let mut last_digit = false;
    for (i, c) in heading.char_indices() {
        if c.is_ascii_digit() {
            last_digit = true;
            end = i + 1;
        } else if c == '.' && last_digit {
            last_digit = false;
        } else {
            break;
        }
    }
    if end == 0 {
        return None;
    }
    let rest = heading[end..].strip_prefix('.').unwrap_or(&heading[end..]);
    (rest.is_empty() || rest.starts_with(' ') || rest.starts_with('\t')).then(|| heading[..end].to_string())
}

/// GitHub's anchor rule: lower case, letters, digits, `-` and `_` kept, spaces become `-`.
pub(crate) fn slugify(heading: &str) -> String {
    heading
        .to_lowercase()
        .chars()
        .filter_map(|c| match c {
            ' ' => Some('-'),
            c if c.is_alphanumeric() || c == '-' || c == '_' => Some(c),
            _ => None,
        })
        .collect()
}

pub(crate) fn build_sections(text: &str, blocks: &[Block]) -> Vec<Section> {
    let mut out: Vec<Section> = Vec::new();
    let mut stack: Vec<usize> = Vec::new();
    let mut top_count = 0;
    let mut slugs: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let text_end = text.len();
    let last_line = blocks.last().map(|b| b.span.end_line).unwrap_or(0);
    for (b, block) in blocks.iter().enumerate() {
        let BlockKind::Heading { level, text: t, .. } = &block.kind else { continue };
        let heading = text[t.start..t.end].split_whitespace().collect::<Vec<_>>().join(" ");
        while stack.last().is_some_and(|&s| out[s].level >= *level) {
            stack.pop();
        }
        let parent = stack.last().copied();
        let ordinal = match parent {
            Some(p) => out[p].children.len() + 1,
            None => {
                top_count += 1;
                top_count
            }
        };
        let address = match parent {
            Some(p) => format!("{}.{ordinal}", out[p].address),
            None => ordinal.to_string(),
        };
        let mut path = parent.map(|p| out[p].path.clone()).unwrap_or_default();
        path.push(heading.clone());
        let base = slugify(&heading);
        let count = slugs.entry(base.clone()).or_insert(0);
        let slug = if *count == 0 { base.clone() } else { format!("{base}-{count}") };
        *count += 1;
        let id = out.len();
        if let Some(p) = parent {
            out[p].children.push(id);
        }
        out.push(Section {
            level: *level,
            number: heading_number(&heading),
            heading,
            address,
            path,
            slug,
            block: b,
            span: Span { start: block.span.start, end: text_end, line: block.span.line, end_line: last_line },
            body_end: text_end,
            body_end_line: last_line,
            parent,
            children: Vec::new(),
        });
        stack.push(id);
    }
    // Ends: a section runs to the next heading of the same or a higher level; its own text to
    // the next heading of any level.
    for i in 0..out.len() {
        if let Some(next) = out.get(i + 1) {
            let next_block = &blocks[next.block];
            out[i].body_end = next_block.span.start;
            out[i].body_end_line = next_block.span.line - 1;
        }
        if let Some(j) = (i + 1..out.len()).find(|&j| out[j].level <= out[i].level) {
            let next_block = &blocks[out[j].block];
            out[i].span.end = next_block.span.start;
            out[i].span.end_line = next_block.span.line - 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use crate::Document;

    #[test]
    fn sections_nest_by_level() {
        let doc = Document::parse("intro\n# A\none\n## B\ntwo\n```\n# not a heading\n```\n# C\nthree");
        let got: Vec<_> = doc
            .sections()
            .iter()
            .map(|s| (s.path.join(" > "), s.level, s.span.line, s.span.end_line, s.parent))
            .collect();
        assert_eq!(
            got,
            vec![("A".into(), 1, 2, 8, None), ("A > B".into(), 2, 4, 8, Some(0)), ("C".into(), 1, 9, 10, None)]
        );
        assert_eq!(doc.sections()[0].children, vec![1]);
        assert_eq!(doc.sections()[0].body_end_line, 3);
    }

    #[test]
    fn section_numbers_and_addresses() {
        let doc = Document::parse("# Title\n## 1. Layers\n## 3.2 Foo\n### x\n## Notes\n# Two\n## Notes\n");
        let got: Vec<_> = doc
            .sections()
            .iter()
            .map(|s| (s.address.as_str(), s.number.as_deref(), s.slug.as_str()))
            .collect();
        assert_eq!(
            got,
            vec![
                ("1", None, "title"),
                ("1.1", Some("1"), "1-layers"),
                ("1.2", Some("3.2"), "32-foo"),
                ("1.2.1", None, "x"),
                ("1.3", None, "notes"),
                ("2", None, "two"),
                ("2.1", None, "notes-1"),
            ]
        );
        for (i, s) in doc.sections().iter().enumerate() {
            let address = crate::Address::parse(&s.address).expect("address");
            assert_eq!(doc.resolve(&address).expect("resolves"), i);
        }
    }

    #[test]
    fn section_text_children_and_heading_flags() {
        let text = "# A\none\n## B\ntwo\n# C\n";
        let doc = Document::parse(text);
        assert_eq!(doc.section_text(0, true, true), "# A\none\n## B\ntwo\n");
        assert_eq!(doc.section_text(0, false, true), "# A\none\n");
        assert_eq!(doc.section_text(0, false, false), "one\n");
        assert_eq!(doc.section_text(1, true, false), "two\n");
    }

    #[test]
    fn find_by_heading_matches_own_or_path() {
        let doc = Document::parse("# Title\n## Problem\nx\n# Other\n## problem\n");
        assert_eq!(doc.find_by_heading("problem"), Some(1));
        assert_eq!(doc.find_by_heading(" Other > PROBLEM "), Some(3));
        assert_eq!(doc.find_by_heading("missing"), None);
    }

    #[test]
    fn title_and_first_paragraph() {
        let text = "---\n# yaml comment\ntitle: t\n---\n<!-- note -->\n# Title\n\n```\ncode\n```\nFirst line\nsecond.\n\n## Problem\n\n| a |\nThe problem.\n";
        let doc = Document::parse(text);
        assert_eq!(doc.title(), Some("Title"));
        assert_eq!(doc.first_paragraph(None).as_deref(), Some("First line second."));
        let problem = doc.find_by_heading("problem").expect("section");
        assert_eq!(doc.first_paragraph(Some(problem)).as_deref(), Some("The problem."));
    }

    #[test]
    fn parses_a_large_document() {
        let mut text = String::new();
        for i in 0..2000 {
            text.push_str(&format!("## Section {i}\n\nSome text with a [link](x{i}.md) and `code`.\n\n"));
        }
        assert!(text.len() > 100_000);
        let doc = Document::parse(&text);
        assert_eq!(doc.sections().len(), 2000);
        assert_eq!(doc.links().len(), 2000);
        assert_eq!(doc.blocks().iter().map(|b| b.span.end - b.span.start).sum::<usize>(), text.len());
    }
}
