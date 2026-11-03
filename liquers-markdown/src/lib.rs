//! A lossless, query-aware Markdown block parser for Liquers.
//!
//! [`Document::parse`] splits a text into a flat list of [`Block`]s that tiles it (every byte in
//! exactly one block, so the blocks concatenated give the input back), builds a [`Section`] tree
//! over the headings, finds [`Link`]s and [`CodeSpan`]s, and resolves section [`Address`]es.
//! Parsing never fails. Link targets and code spans that are Liquers queries are recognised as
//! queries, and emphasis is not parsed, so query characters are never read as markup.
//!
//! Reference: `specs/reference/MARKDOWN.md`; design: `specs/design/markdown-navigation/`.

mod address;
mod block;
mod inline;
mod section;

use std::sync::OnceLock;

use liquers_core::error::Error;

pub use address::{Address, Step};
pub use block::{Block, BlockKind, FrontMatterFormat, Span};
pub use inline::{CodeSpan, Link, LinkKind, Resolved, TargetKind};
pub use section::Section;

/// A parsed Markdown document. It owns its text.
#[derive(Debug)]
pub struct Document {
    text: String,
    blocks: Vec<Block>,
    sections: Vec<Section>,
    inlines: OnceLock<inline::Inlines>,
}

impl std::fmt::Debug for inline::Inlines {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Inlines").field("links", &self.links.len()).field("code_spans", &self.code_spans.len()).finish()
    }
}

impl Document {
    /// Parses `text`. Never fails: malformed constructs become paragraphs or raw blocks.
    pub fn parse(text: &str) -> Document {
        let blocks = block::parse_blocks(text);
        let sections = section::build_sections(text, &blocks);
        Document { text: text.to_string(), blocks, sections, inlines: OnceLock::new() }
    }

    /// The document's text, unchanged.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The blocks, in order; their spans tile the text.
    pub fn blocks(&self) -> &[Block] {
        &self.blocks
    }

    /// The sections, in document order.
    pub fn sections(&self) -> &[Section] {
        &self.sections
    }

    /// The text of section `id`, with its subsections when `children`, with its heading block
    /// when `heading`.
    pub fn section_text(&self, id: usize, children: bool, heading: bool) -> &str {
        let Some(section) = self.sections.get(id) else { return "" };
        let start = if heading { section.span.start } else { self.blocks[section.block].span.end };
        let end = if children { section.span.end } else { section.body_end };
        &self.text[start.min(end)..end]
    }

    /// The section `address` names: exactly one, or an error naming the candidates.
    pub fn resolve(&self, address: &Address) -> Result<usize, Error> {
        address::resolve(&self.sections, address)
    }

    /// The first section whose own heading or full heading path (`A > B`) equals `name`,
    /// ignoring case and surrounding whitespace (the rule of `ns-mem/read` and `edit`).
    pub fn find_by_heading(&self, name: &str) -> Option<usize> {
        let name = name.trim();
        self.sections
            .iter()
            .position(|s| s.heading.eq_ignore_ascii_case(name) || s.path.join(" > ").eq_ignore_ascii_case(name))
    }

    /// The innermost section containing byte `offset`.
    pub fn section_at(&self, offset: usize) -> Option<usize> {
        self.sections.iter().rposition(|s| s.span.start <= offset && offset < s.span.end)
    }

    fn inlines(&self) -> &inline::Inlines {
        self.inlines.get_or_init(|| inline::scan(&self.text, &self.blocks, &|offset| self.section_at(offset)))
    }

    /// Every link and image, in order.
    pub fn links(&self) -> Vec<Link> {
        self.inlines().links.clone()
    }

    /// Every code span, in order.
    pub fn code_spans(&self) -> Vec<CodeSpan> {
        self.inlines().code_spans.clone()
    }

    /// The first heading's text.
    pub fn title(&self) -> Option<&str> {
        self.sections.first().map(|s| s.heading.as_str()).filter(|h| !h.is_empty())
    }

    /// The first paragraph, of the document (`None`) or within a section (with its
    /// subsections), its lines trimmed and joined by a space. Lines starting with `|` (a table
    /// fragment with no delimiter row) are left out, as the line scanner this replaces did.
    pub fn first_paragraph(&self, within: Option<usize>) -> Option<String> {
        let (start, end) = match within {
            Some(id) => {
                let s = self.sections.get(id)?;
                (s.span.start, s.span.end)
            }
            None => (0, self.text.len()),
        };
        self.blocks
            .iter()
            .filter(|b| b.span.start >= start && b.span.end <= end && b.kind == BlockKind::Paragraph)
            .map(|b| {
                self.text[b.span.start..b.span.end].lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('|')).collect::<Vec<_>>().join(" ")
            })
            .find(|p| !p.is_empty())
    }

    /// The front matter's format and its text between the delimiter lines.
    pub fn front_matter(&self) -> Option<(FrontMatterFormat, &str)> {
        let block = self.blocks.first()?;
        let BlockKind::FrontMatter(format) = block.kind else { return None };
        let raw = &self.text[block.span.start..block.span.end];
        let after_open = raw.find('\n').map(|i| i + 1).unwrap_or(raw.len());
        let body = &raw[after_open..];
        let before_close = body.trim_end_matches(['\n', '\r']).rfind('\n').map(|i| i + 1).unwrap_or(0);
        Some((format, &body[..before_close]))
    }

    /// The end of the front matter (0 when there is none).
    pub fn front_matter_end(&self) -> usize {
        match self.blocks.first().map(|b| &b.kind) {
            Some(BlockKind::FrontMatter(_)) => self.blocks[0].span.end,
            _ => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn front_matter_text() {
        let doc = Document::parse("---\na: 1\nb: 2\n---\n# T\n");
        assert_eq!(doc.front_matter(), Some((FrontMatterFormat::Yaml, "a: 1\nb: 2\n")));
        assert_eq!(doc.front_matter_end(), 18);
        assert_eq!(Document::parse("# T\n").front_matter(), None);
    }
}
