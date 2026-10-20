//! `ns-search/md_split` — a proof of concept for Markdown navigation
//! (`NO-MARKDOWN-NAVIGATION-COMMANDS`): Markdown sections, or lines of any text file, as records.

use std::sync::Arc;

use liquers_core::context::{Context, Environment};
use liquers_core::error::Error;
use liquers_core::state::State;

use crate::records::{
    FieldRole, FieldSchema, FieldType, FieldValue, KeyRole, RecordBatchMut, RecordSchema,
    RecordView, RecordViewMut,
};
use crate::value::{ExtValueInterface, Value};

use super::extract::{document_rows, document_text, documents};

/// One section: `(heading path, level, first line, last line)`, lines 1-based. Section 0 — the
/// text before the first heading — has no heading and level 0.
pub(crate) fn sections(text: &str) -> Vec<(Option<String>, usize, usize, usize)> {
    let lines: Vec<&str> = text.lines().collect();
    let mut headings: Vec<(usize, usize, String)> = Vec::new(); // (line index, level, title)
    let mut in_fence = false;
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        let level = trimmed.chars().take_while(|&c| c == '#').count();
        if (1..=6).contains(&level) && trimmed[level..].starts_with(' ') {
            headings.push((i, level, trimmed[level..].trim().to_string()));
        }
    }
    let mut out = Vec::new();
    let first = headings.first().map(|(i, _, _)| *i).unwrap_or(lines.len());
    if lines[..first].iter().any(|line| !line.trim().is_empty()) {
        out.push((None, 0, 1, first));
    }
    let mut path: Vec<(usize, String)> = Vec::new();
    for (h, (start, level, title)) in headings.iter().enumerate() {
        while path.last().is_some_and(|(l, _)| *l >= *level) {
            path.pop();
        }
        path.push((*level, title.clone()));
        let end = headings[h + 1..]
            .iter()
            .find(|(_, l, _)| *l <= *level)
            .map(|(i, _, _)| *i)
            .unwrap_or(lines.len());
        let heading = path.iter().map(|(_, t)| t.as_str()).collect::<Vec<_>>().join(" > ");
        out.push((Some(heading), *level, start + 1, end));
    }
    out
}

/// The first heading of a Markdown text (any level), after any front-matter.
pub(crate) fn first_heading(text: &str) -> Option<String> {
    let body = &text[liquers_records::search::front_matter_end(text)..];
    let mut in_fence = false;
    for line in body.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if !in_fence {
            if let Some(title) = heading_title(trimmed) {
                return Some(title);
            }
        }
    }
    None
}

fn heading_title(trimmed: &str) -> Option<String> {
    let level = trimmed.chars().take_while(|&c| c == '#').count();
    if (1..=6).contains(&level) && trimmed[level..].starts_with(' ') {
        let title = trimmed[level..].trim();
        (!title.is_empty()).then(|| title.to_string())
    } else {
        None
    }
}

/// The first paragraph of `lines`: consecutive prose lines joined by spaces, skipping headings,
/// fenced code, tables and HTML comments.
fn paragraph_of<'a>(lines: impl Iterator<Item = &'a str>) -> Option<String> {
    let mut in_fence = false;
    let mut in_comment = false;
    let mut out: Vec<&str> = Vec::new();
    for line in lines {
        let trimmed = line.trim();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            if !out.is_empty() {
                break;
            }
            continue;
        }
        if in_comment {
            if trimmed.contains("-->") {
                in_comment = false;
            }
            continue;
        }
        if trimmed.starts_with("<!--") {
            in_comment = !trimmed.contains("-->");
            continue;
        }
        let skip = in_fence || heading_title(trimmed).is_some() || trimmed.starts_with('|');
        if trimmed.is_empty() || skip {
            if !out.is_empty() {
                break;
            }
            continue;
        }
        out.push(trimmed);
    }
    (!out.is_empty()).then(|| out.join(" "))
}

/// The first paragraph after the front-matter.
pub(crate) fn first_paragraph(text: &str) -> Option<String> {
    paragraph_of(text[liquers_records::search::front_matter_end(text)..].lines())
}

/// The lines (1-based, inclusive) of the section named `name`: its full heading path
/// (`A > B`) or its own heading, compared without case. The first match wins.
pub(crate) fn find_section(text: &str, name: &str) -> Option<(String, usize, usize)> {
    let name = name.trim();
    sections(&blank_front_matter(text)).into_iter().find_map(|(heading, _, start, end)| {
        let heading = heading?;
        let own = heading.rsplit(" > ").next().unwrap_or(&heading);
        (heading.eq_ignore_ascii_case(name) || own.eq_ignore_ascii_case(name))
            .then_some((heading.clone(), start, end))
    })
}

/// `text` with its front-matter lines emptied, so a YAML comment is not taken for a heading and
/// line numbers still count from the top of the file.
pub(crate) fn blank_front_matter(text: &str) -> String {
    let end = liquers_records::search::front_matter_end(text);
    let lines = text[..end].lines().count();
    let mut out = "\n".repeat(lines);
    out.push_str(&text[end..]);
    out
}

/// The first paragraph of the section named `name` (see [`find_section`]).
pub(crate) fn section_paragraph(text: &str, name: &str) -> Option<String> {
    let (_, start, end) = find_section(text, name)?;
    paragraph_of(text.lines().skip(start).take(end.saturating_sub(start)))
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
            for (n, (heading, level, start, end)) in sections(&body).into_iter().enumerate() {
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
        let got = sections(text);
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
}
