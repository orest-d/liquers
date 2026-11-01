//! The `ns-md` commands. Each takes a Markdown document as its input text, parses it once and
//! returns records, except `section`, which returns text.

use std::sync::Arc;

use liquers_core::error::Error;
use liquers_core::state::State;
use liquers_markdown::{Address, BlockKind, Document, LinkKind, Section, TargetKind};
use liquers_records::formats::{read_table_with_report, ReadOptions, ReadSchema, TableFormat};

use crate::records::{FieldRole, FieldSchema, FieldType, FieldValue, RecordBatchMut, RecordSchema, RecordView, RecordViewMut};
use crate::value::{ExtValueInterface, Value};

fn int(value: usize) -> FieldValue {
    FieldValue::Int(i64::try_from(value).unwrap_or(i64::MAX))
}

fn text(value: &str) -> FieldValue {
    FieldValue::Text(Arc::from(value))
}

fn optional_text(value: Option<&str>) -> FieldValue {
    value.map(text).unwrap_or(FieldValue::Null)
}

fn document(state: &State<Value>) -> Result<Document, Error> {
    Ok(Document::parse(&state.try_into_string()?))
}

/// The section `address` names, or `None` for the empty address (the whole document).
fn section_of(doc: &Document, address: &str) -> Result<Option<usize>, Error> {
    if address.is_empty() {
        return Ok(None);
    }
    doc.resolve(&Address::parse(address)?).map(Some)
}

/// Whether section `id` lies inside `within` (or `within` is the whole document).
fn inside(doc: &Document, id: usize, within: Option<usize>) -> bool {
    let Some(w) = within else { return true };
    let mut current = Some(id);
    while let Some(c) = current {
        if c == w {
            return true;
        }
        current = doc.sections()[c].parent;
    }
    false
}

fn outline_fields() -> Vec<FieldSchema> {
    vec![
        FieldSchema::new("n", FieldType::Int).with_role(FieldRole::numeric()),
        FieldSchema::new("level", FieldType::Int).with_role(FieldRole::numeric()),
        FieldSchema::new("heading", FieldType::Text).with_role(FieldRole::text().and_stored()),
        FieldSchema::new("address", FieldType::Text).with_role(FieldRole::keyword()),
        FieldSchema::new("number", FieldType::Text).with_role(FieldRole::keyword()),
        FieldSchema::new("path", FieldType::Text).with_role(FieldRole::text().and_stored()),
        FieldSchema::new("slug", FieldType::Text).with_role(FieldRole::keyword()),
        FieldSchema::new("line", FieldType::Int).with_role(FieldRole::numeric()),
        FieldSchema::new("end_line", FieldType::Int).with_role(FieldRole::numeric()),
        FieldSchema::new("words", FieldType::Int).with_role(FieldRole::numeric()),
    ]
}

fn outline_row(doc: &Document, n: usize, s: &Section) -> Vec<FieldValue> {
    let words = doc.section_text(n, true, true).split_whitespace().count();
    vec![
        int(n),
        int(usize::from(s.level)),
        text(&s.heading),
        text(&s.address),
        optional_text(s.number.as_deref()),
        text(&s.path.join(" > ")),
        text(&s.slug),
        int(s.span.line),
        int(s.span.end_line),
        int(words),
    ]
}

fn freeze(batch: RecordBatchMut) -> Result<Value, Error> {
    let view: Arc<dyn RecordView> = Arc::new(batch.freeze()?);
    Ok(Value::from_record_view(view))
}

/// `ns-md/outline`.
pub fn outline(state: &State<Value>) -> Result<Value, Error> {
    let doc = document(state)?;
    let mut batch = RecordBatchMut::new(Arc::new(RecordSchema::new(outline_fields())?));
    for (n, s) in doc.sections().iter().enumerate() {
        batch.append_row(&outline_row(&doc, n, s))?;
    }
    freeze(batch)
}

/// `ns-md/section`.
pub fn section(state: &State<Value>, address: String, children: bool, heading: bool) -> Result<Value, Error> {
    let doc = document(state)?;
    let id = doc.resolve(&Address::parse(&address)?)?;
    Ok(Value::from(doc.section_text(id, children, heading).to_string()))
}

/// `ns-md/sections`.
pub fn sections(state: &State<Value>, address: String, level: i64) -> Result<Value, Error> {
    let doc = document(state)?;
    let within = section_of(&doc, &address)?;
    let mut fields = outline_fields();
    fields.push(FieldSchema::new("text", FieldType::Text).with_role(FieldRole::text()));
    let mut batch = RecordBatchMut::new(Arc::new(RecordSchema::new(fields)?));
    for (n, s) in doc.sections().iter().enumerate() {
        if !inside(&doc, n, within) || (level > 0 && i64::from(s.level) != level) {
            continue;
        }
        let mut row = outline_row(&doc, n, s);
        row.push(text(doc.section_text(n, true, true)));
        batch.append_row(&row)?;
    }
    freeze(batch)
}

fn link_kind(kind: LinkKind) -> &'static str {
    match kind {
        LinkKind::Inline => "inline",
        LinkKind::Reference => "reference",
        LinkKind::Autolink => "autolink",
        LinkKind::Image => "image",
    }
}

fn target_kind(kind: TargetKind) -> &'static str {
    match kind {
        TargetKind::Relative => "relative",
        TargetKind::Anchor => "anchor",
        TargetKind::Url => "url",
        TargetKind::Query => "query",
    }
}

/// `ns-md/links`.
pub fn links(state: &State<Value>, address: String) -> Result<Value, Error> {
    let doc = document(state)?;
    let within = section_of(&doc, &address)?;
    let input_key = state.metadata.key().ok().flatten();
    let schema = RecordSchema::new(vec![
        FieldSchema::new("kind", FieldType::Text).with_role(FieldRole::keyword()),
        FieldSchema::new("text", FieldType::Text).with_role(FieldRole::text().and_stored()),
        FieldSchema::new("target", FieldType::Text).with_role(FieldRole::keyword()),
        FieldSchema::new("target_kind", FieldType::Text).with_role(FieldRole::keyword()),
        FieldSchema::new("anchor", FieldType::Text).with_role(FieldRole::keyword()),
        FieldSchema::new("key", FieldType::Text).with_role(FieldRole::keyword()),
        FieldSchema::new("section", FieldType::Text).with_role(FieldRole::keyword()),
        FieldSchema::new("line", FieldType::Int).with_role(FieldRole::numeric()),
    ])?;
    let mut batch = RecordBatchMut::new(Arc::new(schema));
    for link in doc.links() {
        let in_scope = match (within, link.section) {
            (None, _) => true,
            (Some(w), Some(s)) => inside(&doc, s, Some(w)),
            (Some(_), None) => false,
        };
        if !in_scope {
            continue;
        }
        let key = input_key.as_ref().and_then(|k| link.key(k)).map(|k| k.to_string());
        let section = link.section.map(|s| doc.sections()[s].address.as_str());
        batch.append_row(&[
            text(link_kind(link.kind)),
            text(&link.text),
            text(&link.target),
            text(target_kind(link.target_kind)),
            optional_text(link.anchor.as_deref()),
            optional_text(key.as_deref()),
            optional_text(section),
            int(link.span.line),
        ])?;
    }
    freeze(batch)
}

/// `ns-md/table`.
pub fn table(state: &State<Value>, address: String, n: i64) -> Result<Value, Error> {
    let doc = document(state)?;
    let id = doc.resolve(&Address::parse(&address)?)?;
    let s = &doc.sections()[id];
    let tables: Vec<_> = doc
        .blocks()
        .iter()
        .filter(|b| b.kind == BlockKind::Table && b.span.start >= s.span.start && b.span.end <= s.span.end)
        .collect();
    let index = usize::try_from(n).ok().filter(|&i| i >= 1).ok_or_else(|| {
        Error::general_error(format!("md/table: n must be 1 or more, not {n}"))
    })?;
    let block = tables.get(index - 1).ok_or_else(|| {
        Error::general_error(format!(
            "md/table: section {} ({}) has {} table(s), no table {index}",
            s.address,
            s.heading,
            tables.len()
        ))
    })?;
    let bytes = doc.text()[block.span.start..block.span.end].as_bytes();
    let (batch, _report) = read_table_with_report(bytes, TableFormat::Markdown, ReadSchema::Infer, &ReadOptions::default())?;
    let view: Arc<dyn RecordView> = Arc::new(batch);
    Ok(Value::from_record_view(view))
}
