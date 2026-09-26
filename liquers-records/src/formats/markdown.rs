//! Markdown table reader and writer. Tables use field labels as headers (not names). Numeric
//! columns are right-aligned using the alignment row. All cell content is escaped to prevent GFM
//! from rendering inline HTML. Reading back turns labels into names by lowercasing and replacing
//! spaces with `_`, the inverse of the default label, so a label left at its default round-trips.
//!
//! See `specs/design/record-streams/phase2-architecture.md` §"Markdown and HTML".

use std::sync::Arc;

use liquers_core::error::{Error, ErrorType};

use crate::batch::{RecordBatch, RecordView};
use crate::column::FieldValue;
use crate::formats::{ReadOptions, ReadSchema, WriteOptions};
use crate::mutable::{RecordBatchMut, RecordViewMut};
use crate::schema::{FieldSchema, FieldType, RecordSchema};

/// Unescape markdown cell content (reverse of `escape_markdown_cell`).
fn unescape_markdown_cell(text: &str) -> String {
    // Remove leading and trailing whitespace, then reverse escaping in order
    let text = text.trim();
    text.replace("<br>", "\n")
        .replace("\\\\", "\\")  // Must come before other backslash escapes
        .replace("\\|", "|")
        .replace("\\<", "<")
        .replace("\\&", "&")
}

/// Convert a label to a field name (lowercase, replace spaces with `_`).
fn label_to_name(label: &str) -> String {
    label.to_lowercase().replace(' ', "_")
}

/// Parse a markdown table and return a RecordBatch.
pub(crate) fn read_markdown(
    bytes: &[u8],
    schema: ReadSchema<'_>,
    _options: &ReadOptions,
) -> Result<RecordBatch, Error> {
    let text = String::from_utf8(bytes.to_vec())
        .map_err(|e| Error::from_error(ErrorType::ConversionError, e))?;

    let lines: Vec<&str> = text.lines().collect();
    if lines.is_empty() {
        return match schema {
            ReadSchema::Declared(s) => {
                RecordBatchMut::with_capacity(Arc::new(s.clone()), 0).freeze()
            }
            ReadSchema::Infer => {
                let schema = RecordSchema::new(vec![])?;
                RecordBatchMut::with_capacity(Arc::new(schema), 0).freeze()
            }
        };
    }

    // Find the first row (skip leading non-table lines if any)
    let mut line_idx = 0;
    while line_idx < lines.len() && (lines[line_idx].trim().is_empty() || !lines[line_idx].contains('|')) {
        line_idx += 1;
    }

    if line_idx >= lines.len() {
        // No table found
        let schema = RecordSchema::new(vec![])?;
        return RecordBatchMut::with_capacity(Arc::new(schema), 0).freeze();
    }

    // Parse header row
    let header_row = lines[line_idx];
    let headers: Vec<String> = header_row
        .split('|')
        .skip(1) // Skip leading |
        .take_while(|s| !s.is_empty() || line_idx + 1 < lines.len()) // Take until trailing |
        .map(|s| unescape_markdown_cell(s))
        .collect();

    // Skip alignment row if present
    if line_idx + 1 < lines.len() && lines[line_idx + 1].contains('-') {
        line_idx += 1;
    }
    line_idx += 1;

    // Build schema from headers
    let field_names: Vec<String> = headers.iter().map(|h| label_to_name(h)).collect();
    let num_cols = field_names.len();

    // First pass: collect all data rows to infer schema
    let mut data_rows: Vec<Vec<String>> = Vec::new();
    let mut tmp_idx = line_idx;
    while tmp_idx < lines.len() {
        let line = lines[tmp_idx].trim();
        if !line.is_empty() && line.contains('|') {
            let cells: Vec<String> = line
                .split('|')
                .skip(1)
                .take(num_cols)
                .map(|s| unescape_markdown_cell(s))
                .collect();
            if cells.len() == num_cols {
                data_rows.push(cells);
            }
        }
        tmp_idx += 1;
    }

    // Build schema from headers and inferred types
    let inferred_schema = match schema {
        ReadSchema::Declared(s) => Arc::new(s.clone()),
        ReadSchema::Infer => {
            let mut fields = Vec::with_capacity(num_cols);
            for col in 0..num_cols {
                let cells: Vec<Option<&str>> = data_rows
                    .iter()
                    .map(|row| {
                        let text = row.get(col).map(|s| s.as_str()).unwrap_or("");
                        if text.is_empty() { None } else { Some(text) }
                    })
                    .collect();
                let (data_type, nullable) = super::infer::infer_column(&cells);
                let mut field = FieldSchema::new(field_names[col].clone(), data_type);
                if !nullable {
                    field = field.not_null();
                }
                fields.push(field);
            }
            Arc::new(RecordSchema::new(fields)?)
        }
    };

    // Parse data rows
    let mut batch = RecordBatchMut::with_capacity(inferred_schema.clone(), data_rows.len());

    for (row_index, row_cells) in data_rows.iter().enumerate() {
        let mut values = Vec::with_capacity(inferred_schema.fields.len());
        for (col, field) in inferred_schema.fields.iter().enumerate() {
            let cell_text = row_cells.get(col).map(|s| s.as_str()).unwrap_or("");
            let value = if cell_text.is_empty() {
                if field.nullable {
                    FieldValue::Null
                } else {
                    return Err(Error::general_error(format!(
                        "read_markdown: row {}, column '{}': null is not allowed (not nullable)",
                        line_idx + row_index + 1,
                        field.name
                    )));
                }
            } else {
                super::csv::parse_scalar(cell_text, field.data_type).map_err(|e| {
                    Error::general_error(format!(
                        "read_markdown: row {}, column '{}': {e}",
                        line_idx + row_index + 1,
                        field.name
                    ))
                })?
            };
            values.push(value);
        }
        batch.append_row(&values)?;
    }

    batch.freeze()
}

/// Escape `|`, backslash, line breaks, `<` and `&` for GFM safety.
fn escape_markdown_cell(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('|', "\\|")
        .replace('\n', "<br>")
        .replace('\r', "")
        .replace('<', "\\<")
        .replace('&', "\\&")
}

/// Get the label for a field (already populated with default if not explicitly set).
fn field_label(field: &FieldSchema) -> String {
    field.label.clone()
}

/// Write a Markdown table. Headers use field labels, and numeric columns are right-aligned.
pub(crate) fn write_markdown(
    view: &dyn RecordView,
    options: &WriteOptions,
) -> Result<Vec<u8>, Error> {
    let schema = view.schema();
    let mut out = String::new();

    if options.header {
        // Write the header row with labels
        let headers: Vec<String> = schema
            .fields
            .iter()
            .map(|field| escape_markdown_cell(&field_label(field)))
            .collect();
        out.push('|');
        out.push(' ');
        out.push_str(&headers.join(" | "));
        out.push_str(" |\n");

        // Write the alignment row (text left-aligned by default, numeric right-aligned)
        let alignments: Vec<&str> = schema
            .fields
            .iter()
            .map(|field| {
                match field.data_type {
                    FieldType::Bool | FieldType::Int | FieldType::UInt | FieldType::Float => "---:",
                    _ => "---",
                }
            })
            .collect();
        out.push('|');
        out.push(' ');
        out.push_str(&alignments.join(" | "));
        out.push_str(" |\n");
    }

    // Write data rows
    for row in 0..view.len() {
        let mut cells = Vec::with_capacity(schema.fields.len());
        for col in 0..schema.fields.len() {
            let value = view.value(row, col)?;
            let cell_text = match value {
                FieldValue::Null => String::new(),
                FieldValue::Bool(v) => if v { "true".to_string() } else { "false".to_string() },
                FieldValue::Int(v) => v.to_string(),
                FieldValue::UInt(v) => v.to_string(),
                FieldValue::Float(v) => v.to_string(),
                FieldValue::Text(v) => v.to_string(),
                FieldValue::Bytes(v) => super::csv::base64_encode(&v),
                FieldValue::Date(days) => super::csv::format_date(days)?,
                FieldValue::Timestamp(micros) => super::csv::format_timestamp(micros)?,
                FieldValue::Vector(v) => serde_json::to_string(v.as_ref())
                    .map_err(|e| Error::from_error(liquers_core::error::ErrorType::ConversionError, e))?,
            };
            cells.push(escape_markdown_cell(&cell_text));
        }
        out.push('|');
        out.push(' ');
        out.push_str(&cells.join(" | "));
        out.push_str(" |\n");
    }

    Ok(out.into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use crate::mutable::RecordBatchMut;
    use crate::schema::{FieldSchema, FieldType, RecordSchema};
    use liquers_records::{FieldValue, RecordViewMut};

    fn utf8(bytes: Vec<u8>) -> Result<String, Error> {
        String::from_utf8(bytes).map_err(|e| Error::from_error(liquers_core::error::ErrorType::ConversionError, e))
    }

    #[test]
    fn markdown_escapes_pipe_and_backslash() -> Result<(), Error> {
        let schema = Arc::new(RecordSchema::new(vec![FieldSchema::new("cell", FieldType::Text)])?);
        let mut batch = RecordBatchMut::with_capacity(schema, 2);
        batch.append_row(&[FieldValue::Text(Arc::from("a|b"))])?;
        batch.append_row(&[FieldValue::Text(Arc::from("c\\d"))])?;
        let batch = batch.freeze()?;
        let bytes = write_markdown(&batch, &WriteOptions::default())?;
        let text = utf8(bytes)?;
        assert!(text.contains("a\\|b"));
        assert!(text.contains("c\\\\d"));
        Ok(())
    }

    #[test]
    fn markdown_header_uses_the_label() -> Result<(), Error> {
        let schema = Arc::new(RecordSchema::new(vec![FieldSchema::new("user_id", FieldType::Int)])?);
        let mut batch = RecordBatchMut::with_capacity(schema, 1);
        batch.append_row(&[FieldValue::Int(1)])?;
        let batch = batch.freeze()?;
        let bytes = write_markdown(&batch, &WriteOptions::default())?;
        let text = utf8(bytes)?;
        assert!(text.contains("user id")); // default label: name with `_` replaced by space
        Ok(())
    }

    #[test]
    fn markdown_right_aligns_numeric_columns() -> Result<(), Error> {
        let schema = Arc::new(RecordSchema::new(vec![
            FieldSchema::new("name", FieldType::Text),
            FieldSchema::new("amount", FieldType::Int),
        ])?);
        let mut batch = RecordBatchMut::with_capacity(schema, 1);
        batch.append_row(&[FieldValue::Text(Arc::from("a")), FieldValue::Int(1)])?;
        let batch = batch.freeze()?;
        let bytes = write_markdown(&batch, &WriteOptions::default())?;
        let text = utf8(bytes)?;
        // The GFM alignment row marks a right-aligned column with a trailing colon: `---:`.
        let alignment_row = text.lines().nth(1).expect("alignment row");
        let cells: Vec<&str> = alignment_row.trim_matches('|').split('|').collect();
        assert!(!cells[0].trim().ends_with(':')); // name: left/default
        assert!(cells[1].trim().ends_with(':')); // amount: right-aligned
        Ok(())
    }
}
