//! Markdown table reader and writer. Tables use field labels as headers (not names). Numeric
//! columns are right-aligned using the alignment row. All cell content is escaped to prevent GFM
//! from rendering inline HTML. Reading back turns labels into names by lowercasing and replacing
//! spaces with `_`, the inverse of the default label, so a label left at its default round-trips.
//!
//! **The reader is the exact inverse of the writer's escaping**, so `read(write(x)) == x` for every
//! cell. Markdown has no null and no empty-string literal: a null is an empty cell, and an empty
//! `Text` is written as an empty HTML comment, which renders as nothing. The escapes (one pass each
//! way):
//!
//! | Character | Written as | Why |
//! |---|---|---|
//! | `\` `|` `<` `&` | `\\` `\|` `\<` `\&` | GFM backslash escapes: a cell separator, inline HTML, an entity |
//! | line feed | `<br>` | a GFM cell cannot span lines |
//! | carriage return | `&#13;` | the same, without merging into the `<br>` of a line feed |
//! | a leading or trailing space or tab | `&#32;` / `&#9;` | GFM trims cell whitespace |
//! | empty text (the whole cell) | `<!---->` | an empty HTML comment renders as nothing (CommonMark 0.30 and 0.31.2 §6.6) |
//!
//! Reading also accepts what a person writes by hand: a backslash before any ASCII punctuation,
//! `<br/>` and `<br />`, the entities `&amp;` `&lt;` `&gt;` `&quot;` and numeric `&#…;` / `&#x…;`.
//! A row that is not the header's width is an error naming its line — never silently dropped. A
//! hand-written empty cell is null; a cell that is exactly `<!---->` is `""` in a `Text` column and
//! null in any other. Only the first table of a document is read: text around it and any later
//! table are ignored, so a table can be read out of a larger document.
//!
//! See `specs/design/record-streams/phase2-architecture.md` §"Markdown and HTML".

use std::sync::Arc;

use liquers_core::error::{Error, ErrorType};

use crate::batch::{RecordBatch, RecordView};
use crate::column::FieldValue;
use crate::formats::{ReadOptions, ReadSchema, WriteOptions};
use crate::mutable::{RecordBatchMut, RecordViewMut};
use crate::schema::{FieldSchema, FieldType, RecordSchema};

// ---------------------------------------------------------------------------------------------
// Escaping
// ---------------------------------------------------------------------------------------------

/// An empty `Text` cell. GFM has no empty-string literal; an empty HTML comment is valid
/// CommonMark (0.30, 0.31.2 §6.6) and renders as nothing. The writer escapes `<`, so no escaped
/// text can produce it.
const EMPTY_TEXT_MARKER: &str = "<!---->";

/// One read cell: its unescaped text, or `None` for [`EMPTY_TEXT_MARKER`].
fn read_cell(raw: &str) -> Option<String> {
    let trimmed = raw.trim_matches(is_cell_space);
    (trimmed != EMPTY_TEXT_MARKER).then(|| unescape_markdown_cell(trimmed))
}

fn is_cell_space(c: char) -> bool {
    c == ' ' || c == '\t'
}

/// The writer's escape. See the module doc comment's table.
fn escape_markdown_cell(text: &str) -> String {
    let lead_end = text.find(|c: char| !is_cell_space(c)).unwrap_or(text.len());
    let trail_start = text.rfind(|c: char| !is_cell_space(c)).map_or(lead_end, |i| {
        // `rfind` gives the start of the last non-space char; the trailing run starts after it.
        i + text[i..].chars().next().map_or(0, char::len_utf8)
    });
    let mut out = String::with_capacity(text.len() + 8);
    for (index, c) in text.char_indices() {
        let at_edge = index < lead_end || index >= trail_start;
        match c {
            ' ' if at_edge => out.push_str("&#32;"),
            '\t' if at_edge => out.push_str("&#9;"),
            '\\' => out.push_str("\\\\"),
            '|' => out.push_str("\\|"),
            '<' => out.push_str("\\<"),
            '&' => out.push_str("\\&"),
            '\n' => out.push_str("<br>"),
            '\r' => out.push_str("&#13;"),
            other => out.push(other),
        }
    }
    out
}

/// `&name;` or `&#N;` / `&#xN;` at the start of `rest`: the character and the entity's byte length.
fn decode_entity(rest: &str) -> Option<(char, usize)> {
    let semi = rest.get(..rest.len().min(12))?.find(';')?;
    let body = rest.get(1..semi)?;
    let c = match body {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        numeric => {
            let digits = numeric.strip_prefix('#')?;
            let code = match digits.strip_prefix(['x', 'X']) {
                Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                None => digits.parse::<u32>().ok()?,
            };
            char::from_u32(code)?
        }
    };
    Some((c, semi + 1))
}

/// The reader's unescape, one pass — the exact inverse of [`escape_markdown_cell`], so a literal
/// `<br>` (written `\<br>`) and a line break (written `<br>`) stay distinct.
fn unescape_markdown_cell(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(c) = rest.chars().next() {
        if c == '\\' {
            match rest[1..].chars().next() {
                Some(next) if next.is_ascii_punctuation() => {
                    out.push(next);
                    rest = &rest[1 + next.len_utf8()..];
                }
                Some(_) | None => {
                    out.push('\\');
                    rest = &rest[1..];
                }
            }
            continue;
        }
        if c == '<' {
            if let Some(tag) = ["<br>", "<br/>", "<br />"].iter().find(|tag| rest.starts_with(**tag)) {
                out.push('\n');
                rest = &rest[tag.len()..];
                continue;
            }
        }
        if c == '&' {
            if let Some((decoded, len)) = decode_entity(rest) {
                out.push(decoded);
                rest = &rest[len..];
                continue;
            }
        }
        out.push(c);
        rest = &rest[c.len_utf8()..];
    }
    out
}

// ---------------------------------------------------------------------------------------------
// Reading
// ---------------------------------------------------------------------------------------------

/// Splits one table line into its raw (still escaped, untrimmed) cells: `|` separates cells
/// unless escaped as `\|`, and a leading and a trailing `|` delimit the row rather than adding an
/// empty cell at either end.
fn split_row(line: &str) -> Vec<&str> {
    let trimmed = line.trim_matches(is_cell_space);
    let bytes = trimmed.as_bytes();
    let mut cells = Vec::new();
    let mut start = 0;
    let mut i = 0;
    let mut ends_with_pipe = false;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => {
                // An escape pair is kept whole for `unescape_markdown_cell`. Cells are only ever
                // cut at an ASCII `|`, so `i` landing inside a multi-byte character is harmless.
                i += 2;
                ends_with_pipe = false;
            }
            b'|' => {
                cells.push(&trimmed[start..i]);
                start = i + 1;
                i += 1;
                ends_with_pipe = true;
            }
            _ => {
                i += 1;
                ends_with_pipe = false;
            }
        }
    }
    cells.push(trimmed.get(start..).unwrap_or(""));
    if ends_with_pipe {
        cells.pop();
    }
    if trimmed.starts_with('|') && !cells.is_empty() {
        cells.remove(0);
    }
    cells
}

/// A GFM delimiter row: every cell matches `:?-+:?` —
/// `^\|?\s*:?-+:?\s*(\|\s*:?-+:?\s*)*\|?$`. Merely containing a `-` (a date, a negative number)
/// does not make a row one.
fn is_alignment_row(cells: &[&str]) -> bool {
    !cells.is_empty()
        && cells.iter().all(|cell| {
            let cell = cell.trim_matches(is_cell_space);
            let cell = cell.strip_prefix(':').unwrap_or(cell);
            let cell = cell.strip_suffix(':').unwrap_or(cell);
            !cell.is_empty() && cell.bytes().all(|b| b == b'-')
        })
}

/// A table line: non-blank and containing a `|`.
fn is_table_line(line: &str) -> bool {
    !line.trim().is_empty() && line.contains('|')
}

/// Convert a label to a field name (lowercase, replace spaces with `_`).
fn label_to_name(label: &str) -> String {
    label.to_lowercase().replace(' ', "_")
}

/// Which schema field a header names: its name (through [`label_to_name`] or verbatim) or its
/// label.
fn declared_index(schema: &RecordSchema, header: &str) -> Option<usize> {
    let name = label_to_name(header);
    schema
        .fields
        .iter()
        .position(|field| field.name == name)
        .or_else(|| schema.fields.iter().position(|field| field.name == header || field.label == header))
}

/// Parse a markdown table and return a RecordBatch. The table read is the first run of consecutive
/// lines containing `|`; later tables and surrounding text are ignored.
pub(crate) fn read_markdown(
    bytes: &[u8],
    schema: ReadSchema<'_>,
    options: &ReadOptions,
) -> Result<RecordBatch, Error> {
    let text = std::str::from_utf8(bytes).map_err(|e| Error::from_error(ErrorType::ConversionError, e))?;

    // (1-based line number, cells) for every line of the table; a `None` cell is the empty-text
    // marker.
    let mut rows: Vec<(usize, Vec<Option<String>>)> = Vec::new();
    let mut raw_rows: Vec<(usize, Vec<&str>)> = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if is_table_line(line) {
            raw_rows.push((index + 1, split_row(line)));
        } else if !raw_rows.is_empty() {
            break;
        }
    }

    let mut header: Option<Vec<String>> = None;
    let mut raw_iter = raw_rows.into_iter().peekable();
    if options.header {
        if let Some((_, cells)) = raw_iter.next() {
            let names: Vec<String> =
                cells.iter().map(|cell| unescape_markdown_cell(cell.trim_matches(is_cell_space))).collect();
            if let Some((line, alignment)) = raw_iter.peek() {
                if is_alignment_row(alignment) {
                    if alignment.len() != names.len() {
                        return Err(Error::general_error(format!(
                            "read_markdown: line {line}: the alignment row has {} cells, but the header has {}",
                            alignment.len(),
                            names.len()
                        )));
                    }
                    raw_iter.next();
                }
            }
            header = Some(names);
        }
    }
    for (line, cells) in raw_iter {
        rows.push((line, cells.iter().map(|cell| read_cell(cell)).collect()));
    }

    let width = match (&header, rows.first(), schema) {
        (Some(names), _, _) => names.len(),
        (None, Some((_, first)), _) => first.len(),
        (None, None, ReadSchema::Declared(declared)) => declared.fields.len(),
        (None, None, ReadSchema::Infer) => 0,
    };
    for (line, cells) in &rows {
        if cells.len() != width {
            return Err(Error::general_error(format!(
                "read_markdown: line {line} has {} cells, but the table has {width} columns",
                cells.len()
            )));
        }
    }
    let names: Vec<String> = match &header {
        Some(labels) => labels.iter().map(|label| label_to_name(label)).collect(),
        None => (0..width).map(|i| format!("col{i}")).collect(),
    };

    // For each schema field, the table column holding it (`None`: absent, read as null).
    let (record_schema, columns): (Arc<RecordSchema>, Vec<Option<usize>>) = match schema {
        ReadSchema::Declared(declared) => {
            let mut columns = vec![None; declared.fields.len()];
            if let Some(labels) = &header {
                for (col, label) in labels.iter().enumerate() {
                    match declared_index(declared, label) {
                        Some(index) => columns[index] = Some(col),
                        None => {
                            return Err(Error::general_error(format!(
                                "read_markdown: column '{label}' is not declared in the schema"
                            )))
                        }
                    }
                }
            } else {
                if width > declared.fields.len() {
                    return Err(Error::general_error(format!(
                        "read_markdown: the table has {width} columns, but the schema declares {}",
                        declared.fields.len()
                    )));
                }
                for (index, slot) in columns.iter_mut().enumerate().take(width) {
                    *slot = Some(index);
                }
            }
            for (field, column) in declared.fields.iter().zip(columns.iter()) {
                if column.is_none() && !field.nullable {
                    return Err(Error::general_error(format!(
                        "read_markdown: the table is missing non-nullable column '{}'",
                        field.name
                    )));
                }
            }
            (Arc::new(declared.clone()), columns)
        }
        ReadSchema::Infer => {
            let mut fields = Vec::with_capacity(width);
            for (col, name) in names.iter().enumerate() {
                // The marker is an empty string, not a null, exactly as a quoted `""` is in CSV.
                let cells: Vec<Option<&str>> = rows
                    .iter()
                    .map(|(_, row)| match row.get(col) {
                        Some(Some(text)) => Some(text.as_str()).filter(|text| !text.is_empty()),
                        Some(None) => Some(""),
                        None => None,
                    })
                    .collect();
                let (data_type, nullable) = super::infer::infer_column(&cells);
                let mut field = FieldSchema::new(name.clone(), data_type);
                if !nullable {
                    field = field.not_null();
                }
                fields.push(field);
            }
            (Arc::new(RecordSchema::new(fields)?), (0..width).map(Some).collect())
        }
    };

    let mut batch = RecordBatchMut::with_capacity(record_schema.clone(), rows.len());
    for (line, cells) in &rows {
        let mut values = Vec::with_capacity(record_schema.fields.len());
        for (field, column) in record_schema.fields.iter().zip(columns.iter()) {
            let cell = column.and_then(|col| cells.get(col));
            if let (Some(None), FieldType::Text) = (cell, field.data_type) {
                values.push(FieldValue::Text(Arc::from("")));
                continue;
            }
            let cell_text = match cell {
                Some(Some(text)) => text.as_str(),
                Some(None) | None => "",
            };
            let value = if cell_text.is_empty() {
                if field.nullable {
                    FieldValue::Null
                } else {
                    return Err(Error::general_error(format!(
                        "read_markdown: line {line}, column '{}': null is not allowed (not nullable)",
                        field.name
                    )));
                }
            } else {
                super::csv::parse_scalar(cell_text, field.data_type).map_err(|e| {
                    Error::general_error(format!("read_markdown: line {line}, column '{}': {e}", field.name))
                })?
            };
            values.push(value);
        }
        batch.append_row(&values)?;
    }
    batch.freeze()
}

// ---------------------------------------------------------------------------------------------
// Writing
// ---------------------------------------------------------------------------------------------

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
            .map(|field| escape_markdown_cell(&field.label))
            .collect();
        out.push('|');
        out.push(' ');
        out.push_str(&headers.join(" | "));
        out.push_str(" |\n");

        // Write the alignment row (text left-aligned by default, numeric right-aligned)
        let alignments: Vec<&str> = schema
            .fields
            .iter()
            .map(|field| match field.data_type {
                FieldType::Bool | FieldType::Int | FieldType::UInt | FieldType::Float => "---:",
                FieldType::Text
                | FieldType::Binary
                | FieldType::Date
                | FieldType::Timestamp
                | FieldType::Vector => "---",
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
            match &value {
                FieldValue::Text(text) if text.is_empty() => {
                    cells.push(EMPTY_TEXT_MARKER.to_string());
                }
                FieldValue::Null
                | FieldValue::Bool(_)
                | FieldValue::Int(_)
                | FieldValue::UInt(_)
                | FieldValue::Float(_)
                | FieldValue::Text(_)
                | FieldValue::Bytes(_)
                | FieldValue::Date(_)
                | FieldValue::Timestamp(_)
                | FieldValue::Vector(_) => {
                    let cell_text = super::csv::format_value(&value)?.unwrap_or_default();
                    cells.push(escape_markdown_cell(&cell_text));
                }
            }
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

    fn text_batch(cells: &[&str]) -> Result<crate::batch::RecordBatch, Error> {
        let schema = Arc::new(RecordSchema::new(vec![FieldSchema::new("cell", FieldType::Text)])?);
        let mut batch = RecordBatchMut::with_capacity(schema, cells.len());
        for cell in cells {
            batch.append_row(&[FieldValue::Text(Arc::from(*cell))])?;
        }
        batch.freeze()
    }

    fn read(text: &str, schema: ReadSchema<'_>, header: bool) -> Result<crate::batch::RecordBatch, Error> {
        read_markdown(text.as_bytes(), schema, &ReadOptions { header })
    }

    #[test]
    fn markdown_empty_text_round_trips() -> Result<(), Error> {
        let schema = Arc::new(RecordSchema::new(vec![FieldSchema::new("cell", FieldType::Text)])?);
        let cells = [FieldValue::Null, FieldValue::Text(Arc::from("")), FieldValue::Text(Arc::from(" ")), FieldValue::Text(Arc::from("a"))];
        let mut batch = RecordBatchMut::with_capacity(schema.clone(), cells.len());
        for cell in &cells {
            batch.append_row(std::slice::from_ref(cell))?;
        }
        let bytes = write_markdown(&batch.freeze()?, &WriteOptions::default())?;
        let text = utf8(bytes.clone())?;
        assert_eq!(text, "| cell |\n| --- |\n|  |\n| <!----> |\n| &#32; |\n| a |\n");
        for read_schema in [ReadSchema::Infer, ReadSchema::Declared(&schema)] {
            let back = read_markdown(&bytes, read_schema, &ReadOptions::default())?;
            for (row, cell) in cells.iter().enumerate() {
                assert_eq!(&back.value(row, 0)?, cell, "row {row}");
            }
        }
        Ok(())
    }

    #[test]
    fn markdown_empty_cell_is_null() -> Result<(), Error> {
        let batch = read("| a | b |\n|---|---|\n| x |  |\n", ReadSchema::Infer, true)?;
        assert_eq!(batch.value(0, 1)?, FieldValue::Null);
        Ok(())
    }

    #[test]
    fn markdown_empty_text_marker_in_int_column_is_null() -> Result<(), Error> {
        let schema = RecordSchema::new(vec![FieldSchema::new("n", FieldType::Int)])?;
        let batch = read("| n |\n|---|\n| <!----> |\n| 2 |\n", ReadSchema::Declared(&schema), true)?;
        assert_eq!(batch.value(0, 0)?, FieldValue::Null);
        assert_eq!(batch.value(1, 0)?, FieldValue::Int(2));
        Ok(())
    }

    #[test]
    fn markdown_reads_only_first_table() -> Result<(), Error> {
        let document = "Intro text.\n\n| a |\n|---|\n| 1 |\n\nBetween.\n\n| b | c |\n|---|---|\n| 2 | 3 |\n";
        let batch = read(document, ReadSchema::Infer, true)?;
        assert_eq!(batch.schema.fields.len(), 1);
        assert_eq!(batch.schema.fields[0].name, "a");
        assert_eq!(batch.len, 1);
        Ok(())
    }

    #[test]
    fn markdown_reads_its_own_output_back_exactly() -> Result<(), Error> {
        let cells = [
            "a|b", "c\\d", "literal <br> tag", "line1\nline2", "  padded  ", "\ttab", "x\r\ny",
            "&amp; stays", "\\|", "trailing \\", "-5", "2020-01-01", "ü|ñ",
        ];
        let batch = text_batch(&cells)?;
        let bytes = write_markdown(&batch, &WriteOptions::default())?;
        let back = read_markdown(&bytes, ReadSchema::Infer, &ReadOptions::default())?;
        assert_eq!(back.schema.fields.len(), 1, "no phantom column: {}", utf8(bytes.clone())?);
        assert_eq!(back.schema.fields[0].name, "cell");
        assert_eq!(back.len, cells.len());
        for (row, cell) in cells.iter().enumerate() {
            assert_eq!(back.value(row, 0)?, FieldValue::Text(Arc::from(*cell)), "row {row}");
        }
        Ok(())
    }

    #[test]
    fn markdown_without_header_reads_every_line_as_data() -> Result<(), Error> {
        let schema = Arc::new(RecordSchema::new(vec![
            FieldSchema::new("day", FieldType::Date),
            FieldSchema::new("delta", FieldType::Int),
        ])?);
        let mut batch = RecordBatchMut::with_capacity(schema, 2);
        batch.append_row(&[FieldValue::Date(18262), FieldValue::Int(-5)])?;
        batch.append_row(&[FieldValue::Date(18263), FieldValue::Int(7)])?;
        let batch = batch.freeze()?;
        let bytes = write_markdown(&batch, &WriteOptions { header: false })?;
        let back = read_markdown(&bytes, ReadSchema::Infer, &ReadOptions { header: false })?;
        assert_eq!(back.len, 2);
        assert_eq!(back.schema.fields[0].name, "col0");
        assert_eq!(back.value(0, 0)?, FieldValue::Date(18262));
        assert_eq!(back.value(0, 1)?, FieldValue::Int(-5));
        Ok(())
    }

    #[test]
    fn markdown_data_row_with_a_dash_is_not_an_alignment_row() -> Result<(), Error> {
        let back = read("| day | n |\n| 2020-01-01 | -5 |\n| 2020-01-02 | 3 |\n", ReadSchema::Infer, true)?;
        assert_eq!(back.len, 2);
        assert_eq!(back.value(0, 1)?, FieldValue::Int(-5));
        Ok(())
    }

    #[test]
    fn markdown_ragged_row_is_an_error_naming_the_line() {
        let error = read("| a | b |\n|---|---|\n| 1 | 2 |\n| 3 |\n", ReadSchema::Infer, true)
            .expect_err("a row with too few cells");
        let message = format!("{error}");
        assert!(message.contains("line 4"), "unexpected error: {message}");
    }

    #[test]
    fn markdown_declared_schema_is_matched_by_header_name() -> Result<(), Error> {
        let schema = RecordSchema::new(vec![
            FieldSchema::new("user_name", FieldType::Text),
            FieldSchema::new("count", FieldType::Int),
        ])?;
        let back = read("| count | user name |\n|---:|---|\n| 1 | x |\n", ReadSchema::Declared(&schema), true)?;
        assert_eq!(back.value(0, 0)?, FieldValue::Text(Arc::from("x")));
        assert_eq!(back.value(0, 1)?, FieldValue::Int(1));
        Ok(())
    }

    #[test]
    fn markdown_declared_schema_refuses_an_undeclared_column() -> Result<(), Error> {
        let schema = RecordSchema::new(vec![FieldSchema::new("a", FieldType::Int)])?;
        let error = read("| a | extra |\n|---|---|\n| 1 | 2 |\n", ReadSchema::Declared(&schema), true)
            .expect_err("extra is not declared");
        assert!(format!("{error}").contains("extra"));
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
