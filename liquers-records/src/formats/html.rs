//! HTML table writer — write-only format. Security-critical: every cell, label and description
//! is HTML-escaped to prevent XSS attacks.
//!
//! The output is a `<table class="liquers-records">` with `<thead>` and `<tbody>`. Numeric cells
//! carry `class="num"`, nulls carry `class="null"`, and field descriptions become the header's
//! `title` attribute.
//!
//! See `specs/design/record-streams/phase2-architecture.md` §"Markdown and HTML".

use liquers_core::error::Error;

use crate::batch::RecordView;
use crate::column::FieldValue;
use crate::formats::WriteOptions;

/// HTML-escape all five critical characters: `&`, `<`, `>`, `"`, `'`.
fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Get the label for a field (already populated with default if not explicitly set).
fn field_label(field: &crate::schema::FieldSchema) -> String {
    field.label.clone()
}

/// Write an HTML table. Headers use field labels, numeric cells carry `class="num"`, and nulls
/// carry `class="null"`. Every cell, label and description is HTML-escaped.
pub(crate) fn write_html(
    view: &dyn RecordView,
    options: &WriteOptions,
) -> Result<Vec<u8>, Error> {
    let schema = view.schema();
    let mut out = String::new();

    out.push_str("<table class=\"liquers-records\">\n");

    if options.header {
        out.push_str("<thead>\n");
        out.push_str("<tr>\n");

        for field in &schema.fields {
            let label = escape_html(&field_label(field));
            let title_attr = if !field.description.is_empty() {
                format!(" title=\"{}\"", escape_html(&field.description))
            } else {
                String::new()
            };
            out.push_str(&format!("<th{title_attr}>{label}</th>\n"));
        }

        out.push_str("</tr>\n");
        out.push_str("</thead>\n");
    }

    out.push_str("<tbody>\n");
    for row in 0..view.len() {
        out.push_str("<tr>\n");
        for col in 0..schema.fields.len() {
            let value = view.value(row, col)?;

            let (cell_content, class_name) = match &value {
                FieldValue::Null => {
                    (String::new(), "null")
                }
                FieldValue::Bool(v) => {
                    (if *v { "true".to_string() } else { "false".to_string() }, "")
                }
                FieldValue::Int(v) => {
                    (v.to_string(), "num")
                }
                FieldValue::UInt(v) => {
                    (v.to_string(), "num")
                }
                FieldValue::Float(v) => {
                    (v.to_string(), "num")
                }
                FieldValue::Text(v) => {
                    (v.to_string(), "")
                }
                FieldValue::Bytes(v) => {
                    (super::csv::base64_encode(&v), "")
                }
                FieldValue::Date(days) => {
                    (super::csv::format_date(*days)?, "")
                }
                FieldValue::Timestamp(micros) => {
                    (super::csv::format_timestamp(*micros)?, "")
                }
                FieldValue::Vector(v) => {
                    (serde_json::to_string(v.as_ref())
                        .map_err(|e| Error::from_error(liquers_core::error::ErrorType::ConversionError, e))?, "")
                }
            };

            let escaped_content = escape_html(&cell_content);
            let class_attr = if class_name.is_empty() {
                String::new()
            } else {
                format!(" class=\"{class_name}\"")
            };

            out.push_str(&format!("<td{class_attr}>{escaped_content}</td>\n"));
        }
        out.push_str("</tr>\n");
    }
    out.push_str("</tbody>\n");

    out.push_str("</table>\n");

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
    fn html_escapes_cell_content() -> Result<(), Error> {
        let schema = Arc::new(RecordSchema::new(vec![FieldSchema::new("content", FieldType::Text)])?);
        let mut batch = RecordBatchMut::with_capacity(schema, 1);
        batch.append_row(&[FieldValue::Text(Arc::from("<script>alert('xss')</script>"))])?;
        let batch = batch.freeze()?;
        let bytes = write_html(&batch, &WriteOptions::default())?;
        let text = utf8(bytes)?;
        assert!(text.contains("&lt;script&gt;"));
        assert!(!text.contains("<script>"));
        Ok(())
    }

    #[test]
    fn html_escapes_label_and_description() -> Result<(), Error> {
        let schema = Arc::new(RecordSchema::new(vec![FieldSchema::new("data", FieldType::Text)
            .with_label("User \"Name\"")
            .with_description("Field with <tag>")])?);
        let mut batch = RecordBatchMut::with_capacity(schema, 1);
        batch.append_row(&[FieldValue::Text(Arc::from("x"))])?;
        let batch = batch.freeze()?;
        let bytes = write_html(&batch, &WriteOptions::default())?;
        let text = utf8(bytes)?;
        assert!(text.contains("User &quot;Name&quot;"));
        assert!(text.contains("&lt;tag&gt;")); // the description, in the header's `title` attribute
        Ok(())
    }

    #[test]
    fn html_structure_is_a_table_with_thead_and_tbody() -> Result<(), Error> {
        let schema = Arc::new(RecordSchema::new(vec![FieldSchema::new("a", FieldType::Text)])?);
        let mut batch = RecordBatchMut::with_capacity(schema, 1);
        batch.append_row(&[FieldValue::Text(Arc::from("x"))])?;
        let batch = batch.freeze()?;
        let bytes = write_html(&batch, &WriteOptions::default())?;
        let text = utf8(bytes)?;
        assert!(text.contains("<table class=\"liquers-records\">"));
        assert!(text.contains("<thead>"));
        assert!(text.contains("<tbody>"));
        Ok(())
    }

    #[test]
    fn html_numeric_cells_and_nulls_carry_their_class() -> Result<(), Error> {
        let schema = Arc::new(RecordSchema::new(vec![FieldSchema::new("amount", FieldType::Int)])?);
        let mut batch = RecordBatchMut::with_capacity(schema, 2);
        batch.append_row(&[FieldValue::Int(5)])?;
        batch.append_row(&[FieldValue::Null])?;
        let batch = batch.freeze()?;
        let bytes = write_html(&batch, &WriteOptions::default())?;
        let text = utf8(bytes)?;
        assert!(text.contains("class=\"num\""));
        assert!(text.contains("class=\"null\""));
        Ok(())
    }
}
