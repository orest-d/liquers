//! `ns-search/md_split` — a proof of concept for Markdown navigation
//! (`NO-MARKDOWN-NAVIGATION-COMMANDS`): Markdown sections, or lines of any text file, as records.
//! Sections come from the `liquers-markdown` block parser; the helpers below adapt it for
//! `ns-mem` and the `markdown_tiers` store step.

use std::sync::Arc;

use liquers_core::context::{Context, Environment};
use liquers_core::error::Error;
use liquers_core::state::State;
use liquers_markdown::Document;

use crate::records::{
    FieldRole, FieldSchema, FieldType, FieldValue, KeyRole, RecordBatchMut, RecordSchema,
    RecordView, RecordViewMut,
};
use crate::value::{ExtValueInterface, Value};

use super::extract::{document_rows, document_text, documents};

/// One section: `(heading path, level, first line, last line)`, lines 1-based; a section runs to
/// its last subsection. Section 0, the text before the first heading, has no heading and level
/// 0; with `skip_front_matter` the front matter does not count as such text.
pub(crate) fn sections(text: &str, skip_front_matter: bool) -> Vec<(Option<String>, usize, usize, usize)> {
    let doc = Document::parse(text);
    let mut out = Vec::new();
    let first = doc.sections().first().map(|s| s.span.start).unwrap_or(text.len());
    let from = if skip_front_matter { doc.front_matter_end() } else { 0 };
    if first > from && !text[from..first].trim().is_empty() {
        let last_line = text[..first].lines().count();
        out.push((None, 0, 1, last_line));
    }
    for s in doc.sections() {
        out.push((Some(s.path.join(" > ")), usize::from(s.level), s.span.line, s.span.end_line));
    }
    out
}

/// The first heading of a Markdown text (any level), after any front-matter.
pub(crate) fn first_heading(text: &str) -> Option<String> {
    Document::parse(text).title().map(str::to_string)
}

/// The first paragraph after the front-matter.
pub(crate) fn first_paragraph(text: &str) -> Option<String> {
    Document::parse(text).first_paragraph(None)
}

/// The lines (1-based, inclusive) of the section named `name`: its full heading path
/// (`A > B`) or its own heading, compared without case. The first match wins.
pub(crate) fn find_section(text: &str, name: &str) -> Option<(String, usize, usize)> {
    let doc = Document::parse(text);
    let s = &doc.sections()[doc.find_by_heading(name)?];
    Some((s.path.join(" > "), s.span.line, s.span.end_line))
}

/// The first paragraph of the section named `name` (see [`find_section`]).
pub(crate) fn section_paragraph(text: &str, name: &str) -> Option<String> {
    let doc = Document::parse(text);
    let id = doc.find_by_heading(name)?;
    doc.first_paragraph(Some(id))
}

fn schema() -> Result<Arc<RecordSchema>, Error> {
    Ok(Arc::new(RecordSchema::new(vec![
        FieldSchema::new("id", FieldType::Text).with_key(KeyRole::Id),
        FieldSchema::new("key", FieldType::Text).with_role(FieldRole::keyword()),
        FieldSchema::new("filename", FieldType::Text).with_role(FieldRole::text().and_stored()),
        FieldSchema::new("n", FieldType::Int).with_role(FieldRole::numeric()),
        FieldSchema::new("line", FieldType::Int).with_role(FieldRole::numeric()),
        FieldSchema::new("end_line", FieldType::Int).with_role(FieldRole::numeric()),
        FieldSchema::new("heading", FieldType::Text).with_role(FieldRole::text().and_stored()),
        FieldSchema::new("level", FieldType::Int).with_role(FieldRole::numeric()),
        FieldSchema::new("text", FieldType::Text).with_role(FieldRole::text()),
    ])?))
}

fn int(value: usize) -> FieldValue {
    FieldValue::Int(i64::try_from(value).unwrap_or(i64::MAX))
}

fn text(value: &str) -> FieldValue {
    FieldValue::Text(Arc::from(value))
}

/// `ns-search/md_split`. See the command's `doc`.
pub async fn md_split<E: Environment<Value = Value>>(
    state: State<Value>,
    unit: String,
    context: Context<E>,
) -> Result<Value, Error> {
    let by_section = match unit.as_str() {
        "section" => true,
        "line" => false,
        other => {
            return Err(Error::general_error(format!(
                "md_split: unit must be section or line, not '{other}'"
            )))
        }
    };
    let view = document_rows(state, &context).await?;
    let mut batch = RecordBatchMut::new(schema()?);
    for document in documents(&view)? {
        if by_section && !document.filename.ends_with(".md") {
            continue;
        }
        let Some(body) = document_text(&view, &document, &context).await? else { continue };
        let key = &document.key;
        if by_section {
            let lines: Vec<&str> = body.lines().collect();
            for (n, (heading, level, start, end)) in sections(&body, false).into_iter().enumerate() {
                batch.append_row(&[
                    text(&format!("{key}#{n}")),
                    text(key),
                    text(&document.filename),
                    int(n),
                    int(start),
                    int(end),
                    heading.as_deref().map(text).unwrap_or(FieldValue::Null),
                    if level == 0 { FieldValue::Null } else { int(level) },
                    text(&lines[start - 1..end].join("\n")),
                ])?;
            }
        } else {
            for (i, line) in body.lines().enumerate() {
                let n = i + 1;
                batch.append_row(&[
                    text(&format!("{key}#{n}")),
                    text(key),
                    text(&document.filename),
                    int(n),
                    int(n),
                    int(n),
                    FieldValue::Null,
                    FieldValue::Null,
                    text(line),
                ])?;
            }
        }
    }
    let result: Arc<dyn RecordView> = Arc::new(batch.freeze()?);
    Ok(Value::from_record_view(result))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headings_and_paragraphs_skip_front_matter_and_code() {
        let text = "---\n# yaml comment\ntitle: t\n---\n<!-- note -->\n# Title\n\n```\ncode\n```\nFirst line\nsecond.\n\n## Problem\n\n| a |\nThe problem.\n";
        assert_eq!(first_heading(text).as_deref(), Some("Title"));
        assert_eq!(first_paragraph(text).as_deref(), Some("First line second."));
        assert_eq!(section_paragraph(text, "problem").as_deref(), Some("The problem."));
        assert_eq!(section_paragraph(text, "Title > Problem").as_deref(), Some("The problem."));
        assert!(section_paragraph(text, "Missing").is_none());
    }

    #[test]
    fn sections_follow_heading_levels_and_skip_fences() {
        let text = "intro\n# A\none\n## B\ntwo\n```\n# not a heading\n```\n# C\nthree";
        let got = sections(text, false);
        assert_eq!(
            got,
            vec![
                (None, 0, 1, 1),
                (Some("A".to_string()), 1, 2, 8),
                (Some("A > B".to_string()), 2, 4, 8),
                (Some("C".to_string()), 1, 9, 10),
            ]
        );
    }

    #[test]
    fn md_split_counts_setext_and_ignores_html_headings() {
        let text = "Title\n=====\n\n<div>\n# not a heading\n</div>\n\n    # indented code\n\nSub\n---\nbody\n";
        let got = sections(text, false);
        assert_eq!(
            got,
            vec![(Some("Title".to_string()), 1, 1, 12), (Some("Title > Sub".to_string()), 2, 10, 12)]
        );
    }

    #[test]
    fn front_matter_is_not_a_preamble_when_skipped() {
        let text = "---\na: 1\n---\n# T\n";
        assert_eq!(sections(text, true), vec![(Some("T".to_string()), 1, 4, 4)]);
        assert_eq!(sections(text, false)[0], (None, 0, 1, 3));
    }
}
