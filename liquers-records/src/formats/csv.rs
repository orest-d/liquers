//! Hand-written CSV/TSV reader and writer — no `csv` crate, because a CSV file can distinguish a
//! null from an empty string only by quoting, and a reader must know whether a field *was* quoted,
//! which the `csv` crate does not report. The convention is PostgreSQL's `COPY … CSV`: an unquoted
//! empty field is null, a quoted `""` is the empty string.
//!
//! Both readers this crate's [`super::ReadSchema`] can select live here: schema-aware and strict
//! (a cell that does not parse as its declared type is an error naming the row and column), and
//! schema-less, inferring one column at a time through [`super::infer`]. TSV is the same code with
//! `separator: b'\t'`.
//!
//! See `specs/design/record-streams/phase2-architecture.md`, §"Tier 1 — in `records`, no new
//! dependency" and §"Two readers: schema-aware and schema-less".

use std::sync::Arc;

use chrono::{DateTime, Datelike, NaiveDate, SecondsFormat, Utc};
use liquers_core::error::{Error, ErrorType};

use crate::batch::{RecordBatch, RecordView};
use crate::column::FieldValue;
use crate::formats::infer;
use crate::formats::{ReadOptions, ReadReport, ReadSchema, WriteOptions};
use crate::mutable::{RecordBatchMut, RecordViewMut};
use crate::schema::{FieldSchema, FieldType, RecordSchema};

/// Days from 0001-01-01 (chrono's proleptic-Gregorian day 1) to the Unix epoch, 1970-01-01 —
/// `NaiveDate::from_ymd_opt(1970, 1, 1).unwrap().num_days_from_ce()` (a documented chrono
/// constant), hard-coded so converting a `Column::Date` day count needs no fallible date
/// construction of its own.
const UNIX_EPOCH_DAYS_FROM_CE: i32 = 719_163;

const BASE64_ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

// ---------------------------------------------------------------------------------------------
// Tokenizer
// ---------------------------------------------------------------------------------------------

/// One CSV/TSV field as parsed: its text, and whether it was quoted in the source — the only way
/// to tell a null (unquoted empty) from an explicit empty string (quoted `""`).
#[derive(Debug, Clone)]
struct RawField {
    text: String,
    quoted: bool,
}

fn is_null_cell(field: &RawField) -> bool {
    !field.quoted && field.text.is_empty()
}

/// One CSV/TSV record and the physical line it starts on (1-based, counting the header and every
/// line break, including those inside a quoted cell) — what an editor shows, and what an error
/// names.
#[derive(Debug, Clone)]
struct ParsedRow {
    line: usize,
    fields: Vec<RawField>,
}

/// RFC 4180 tokenizer: a quoted field may embed the separator, quotes (written doubled), CR and
/// LF; both `\n` and `\r\n` end a row unquoted. Not part of the public API — [`read_csv`] is.
fn parse_rows(bytes: &[u8], separator: u8) -> Result<Vec<ParsedRow>, Error> {
    fn make_field(bytes: &[u8], quoted: bool) -> Result<RawField, Error> {
        let text = String::from_utf8(bytes.to_vec())
            .map_err(|e| Error::from_error(ErrorType::ConversionError, e))?;
        Ok(RawField { text, quoted })
    }

    let quote = b'"';
    let mut rows = Vec::new();
    let mut row: Vec<RawField> = Vec::new();
    let mut field: Vec<u8> = Vec::new();
    let mut quoted = false;
    let mut in_quotes = false;
    let mut i = 0usize;
    let n = bytes.len();
    // The line `i` is on, and the line the current record started on.
    let mut line = 1usize;
    let mut row_start = 1usize;

    while i < n {
        let b = bytes[i];
        if in_quotes {
            if b == quote {
                if i + 1 < n && bytes[i + 1] == quote {
                    field.push(quote);
                    i += 2;
                } else {
                    in_quotes = false;
                    i += 1;
                }
            } else {
                // A line break inside a quoted cell: `\r\n` counts once, at its `\n`.
                if b == b'\n' || (b == b'\r' && bytes.get(i + 1) != Some(&b'\n')) {
                    line += 1;
                }
                field.push(b);
                i += 1;
            }
        } else if b == quote && field.is_empty() && !quoted {
            quoted = true;
            in_quotes = true;
            i += 1;
        } else if b == separator {
            row.push(make_field(&field, quoted)?);
            field.clear();
            quoted = false;
            i += 1;
        } else if b == b'\n' {
            row.push(make_field(&field, quoted)?);
            field.clear();
            quoted = false;
            rows.push(ParsedRow {
                line: row_start,
                fields: std::mem::take(&mut row),
            });
            i += 1;
            line += 1;
            row_start = line;
        } else if b == b'\r' {
            row.push(make_field(&field, quoted)?);
            field.clear();
            quoted = false;
            rows.push(ParsedRow {
                line: row_start,
                fields: std::mem::take(&mut row),
            });
            i += 1;
            if i < n && bytes[i] == b'\n' {
                i += 1;
            }
            line += 1;
            row_start = line;
        } else {
            field.push(b);
            i += 1;
        }
    }
    if in_quotes {
        return Err(Error::general_error(
            "read_table: CSV: unterminated quoted field".to_string(),
        ));
    }
    // A final field/row with no trailing line break.
    if !field.is_empty() || quoted || !row.is_empty() {
        row.push(make_field(&field, quoted)?);
        rows.push(ParsedRow {
            line: row_start,
            fields: row,
        });
    }
    Ok(rows)
}

// ---------------------------------------------------------------------------------------------
// Scalar <-> text conversions (shared by the declared and the post-inference readers, and by the
// writer)
// ---------------------------------------------------------------------------------------------

/// `pub(super)`: reused by [`super::ndjson`] (Binary is base64 in JSON too) and [`super::shapes`].
pub(super) fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);
        let n = ((b0 as u32) << 16) | ((b1 as u32) << 8) | (b2 as u32);
        out.push(BASE64_ALPHABET[((n >> 18) & 0x3f) as usize] as char);
        out.push(BASE64_ALPHABET[((n >> 12) & 0x3f) as usize] as char);
        out.push(if chunk.len() > 1 {
            BASE64_ALPHABET[((n >> 6) & 0x3f) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            BASE64_ALPHABET[(n & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    out
}

pub(super) fn base64_decode(text: &str) -> Result<Vec<u8>, Error> {
    fn value(byte: u8) -> Result<u32, Error> {
        match byte {
            b'A'..=b'Z' => Ok((byte - b'A') as u32),
            b'a'..=b'z' => Ok((byte - b'a' + 26) as u32),
            b'0'..=b'9' => Ok((byte - b'0' + 52) as u32),
            b'+' => Ok(62),
            b'/' => Ok(63),
            other => Err(Error::conversion_error(
                format!("{:?}", other as char),
                "base64 character (Binary)",
            )),
        }
    }
    // Strict, canonical base64 as `base64_encode` writes it: whole quartets, at most two `=` and
    // only at the end, and no set bits in the final partial sextet. Anything else is corrupt input,
    // refused rather than decoded to some other bytes.
    let malformed = |reason: &str| {
        Error::conversion_error_with_message(text, "base64 (Binary)", reason)
    };
    let bytes = text.as_bytes();
    if bytes.len() % 4 != 0 {
        return Err(malformed("length is not a multiple of 4"));
    }
    let padding = bytes.iter().rev().take_while(|&&byte| byte == b'=').count();
    if padding > 2 {
        return Err(malformed("more than two '=' padding characters"));
    }
    let cleaned = &bytes[..bytes.len() - padding];
    let mut out = Vec::with_capacity(cleaned.len() * 3 / 4 + 3);
    let mut buffer: u32 = 0;
    let mut bits: u32 = 0;
    for &byte in cleaned {
        buffer = ((buffer << 6) | value(byte)?) & 0xff_ffff;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }
    if bits > 0 && buffer & ((1 << bits) - 1) != 0 {
        return Err(malformed("non-zero bits after the last byte"));
    }
    Ok(out)
}

/// `pub(crate)`: [`super::ndjson`] and [`super::shapes`] render/parse dates the same way (an ISO
/// string), so the day-count conversion is not duplicated per format, and
/// [`crate::column::FieldValue::parse_text`] exposes it outside the crate.
pub(crate) fn parse_date(text: &str) -> Result<i32, Error> {
    let date = NaiveDate::parse_from_str(text, "%Y-%m-%d")
        .map_err(|e| Error::conversion_error(text, format!("Date (YYYY-MM-DD): {e}")))?;
    date.num_days_from_ce()
        .checked_sub(UNIX_EPOCH_DAYS_FROM_CE)
        .ok_or_else(|| Error::general_error(format!("read_table: date '{text}' is out of range")))
}

pub(super) fn format_date(days: i32) -> Result<String, Error> {
    let ce_days = days.checked_add(UNIX_EPOCH_DAYS_FROM_CE).ok_or_else(|| {
        Error::general_error(format!("write_table: date value {days} is out of range"))
    })?;
    let date = NaiveDate::from_num_days_from_ce_opt(ce_days).ok_or_else(|| {
        Error::general_error(format!("write_table: date value {days} is out of range"))
    })?;
    Ok(date.format("%Y-%m-%d").to_string())
}

pub(crate) fn parse_timestamp(text: &str) -> Result<i64, Error> {
    let parsed = DateTime::parse_from_rfc3339(text)
        .map_err(|e| Error::conversion_error(text, format!("Timestamp (RFC 3339): {e}")))?;
    Ok(parsed.with_timezone(&Utc).timestamp_micros())
}

pub(super) fn format_timestamp(micros: i64) -> Result<String, Error> {
    let dt = DateTime::<Utc>::from_timestamp_micros(micros).ok_or_else(|| {
        Error::general_error(format!("write_table: timestamp value {micros} is out of range"))
    })?;
    Ok(dt.to_rfc3339_opts(SecondsFormat::Micros, true))
}

fn parse_vector(text: &str) -> Result<FieldValue, Error> {
    let value: serde_json::Value = serde_json::from_str(text)
        .map_err(|e| Error::conversion_error(text, format!("Vector (JSON array): {e}")))?;
    let items = value
        .as_array()
        .ok_or_else(|| Error::conversion_error(text, "Vector (JSON array)"))?;
    let mut floats = Vec::with_capacity(items.len());
    for item in items {
        let number = item
            .as_f64()
            .ok_or_else(|| Error::conversion_error(text, "Vector (JSON array of numbers)"))?;
        floats.push(number as f32);
    }
    Ok(FieldValue::Vector(Arc::from(floats)))
}

/// Parses one non-null cell as `field_type` — strict, no guessing: `Text` keeps the cell verbatim
/// (so a leading zero is safe), every other type must parse or the cell is refused. Used both by
/// the declared reader (the schema's own type) and by the post-inference reader (the type
/// `infer::infer_column` already chose) — one conversion either way.
/// `pub(crate)`: [`crate::column::FieldValue::parse_text`] exposes it, and [`super::shapes`]
/// reuses it for the `columns`/`index` JSON orients, whose keys
/// are always JSON strings (object keys cannot be numbers) and must be coerced through a schema's
/// declared type the same way a CSV cell is — see phase2-architecture.md §"JSON shapes are
/// conversions": "an `Id` read from `columns` or `index` keys is text unless a schema says
/// otherwise".
pub(crate) fn parse_scalar(text: &str, field_type: FieldType) -> Result<FieldValue, Error> {
    match field_type {
        FieldType::Bool => {
            if text.eq_ignore_ascii_case("true") {
                Ok(FieldValue::Bool(true))
            } else if text.eq_ignore_ascii_case("false") {
                Ok(FieldValue::Bool(false))
            } else {
                Err(Error::conversion_error(text, "Bool (true/false)"))
            }
        }
        FieldType::Int => text
            .parse::<i64>()
            .map(FieldValue::Int)
            .map_err(|_| Error::conversion_error(text, "Int")),
        FieldType::UInt => text
            .parse::<u64>()
            .map(FieldValue::UInt)
            .map_err(|_| Error::conversion_error(text, "UInt")),
        FieldType::Float => text
            .parse::<f64>()
            .map(FieldValue::Float)
            .map_err(|_| Error::conversion_error(text, "Float")),
        FieldType::Text => Ok(FieldValue::Text(Arc::from(text))),
        FieldType::Binary => base64_decode(text).map(|bytes| FieldValue::Bytes(Arc::from(bytes))),
        FieldType::Date => parse_date(text).map(FieldValue::Date),
        FieldType::Timestamp => parse_timestamp(text).map(FieldValue::Timestamp),
        FieldType::Vector => parse_vector(text),
    }
}

/// `pub(crate)`: [`super::shapes`] reuses this to turn an `Id` value into a JSON object key —
/// `columns`/`index` keys are always strings (phase2-architecture.md §"JSON shapes are
/// conversions") — and `crate::sources::ChunkOrigin::locator_query` reuses it for the same reason,
/// one level up: a row's `Id` becomes the trailing string parameter of a locator query. One
/// canonical `FieldValue -> String` conversion, matching every variant, rather than a second one
/// duplicated at the call site.
pub(crate) fn format_value(value: &FieldValue) -> Result<Option<String>, Error> {
    match value {
        FieldValue::Null => Ok(None),
        FieldValue::Bool(v) => Ok(Some(if *v { "true".to_string() } else { "false".to_string() })),
        FieldValue::Int(v) => Ok(Some(v.to_string())),
        FieldValue::UInt(v) => Ok(Some(v.to_string())),
        // `{:?}`, not `{}`: the shortest round-trip form that always carries a point or an
        // exponent (`1.0`, `1e300`, `1e-7`, `-0.0`), so a Float column is read back as `Float`
        // by the schema-less reader rather than as `Int` (`1`) or text (300 digits).
        FieldValue::Float(v) => Ok(Some(format!("{v:?}"))),
        FieldValue::Text(v) => Ok(Some(v.to_string())),
        FieldValue::Bytes(v) => Ok(Some(base64_encode(v))),
        FieldValue::Date(days) => format_date(*days).map(Some),
        FieldValue::Timestamp(micros) => format_timestamp(*micros).map(Some),
        FieldValue::Vector(v) => serde_json::to_string(v.as_ref())
            .map(Some)
            .map_err(|e| Error::from_error(ErrorType::ConversionError, e)),
    }
}

// ---------------------------------------------------------------------------------------------
// Reading
// ---------------------------------------------------------------------------------------------

/// Where a data record is, for an error message: the physical line it starts on, plus its record
/// number when a quoted cell spanning lines has made the two disagree — "CSV line 7 (record 5)".
#[derive(Debug, Clone, Copy)]
struct RowPosition {
    line: usize,
    /// 1-based, counting data records only.
    record: usize,
    /// The line record 1 starts on: 2 with a header, 1 without.
    first_line: usize,
}

impl std::fmt::Display for RowPosition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.line == self.record + self.first_line - 1 {
            write!(f, "CSV line {}", self.line)
        } else {
            write!(f, "CSV line {} (record {})", self.line, self.record)
        }
    }
}

/// One field's value. Null handling — a cell missing from a short row, or a null cell, against
/// the field's `nullable` — is checked once here, so both readers below share it.
///
/// A cell missing from a short row is padded: null when the field is nullable, `""` when it is a
/// non-nullable `Text`, and an error for any other non-nullable type, which has no honest padding
/// value. Maintainer decision 2026-10-06, `specs/design/csv-physical-lines-short-rows/`.
fn cell_value(
    raw: Option<&RawField>,
    field_name: &str,
    field_type: FieldType,
    nullable: bool,
    position: RowPosition,
) -> Result<FieldValue, Error> {
    match raw {
        None => {
            if nullable {
                Ok(FieldValue::Null)
            } else if field_type == FieldType::Text {
                Ok(FieldValue::Text(Arc::from("")))
            } else {
                Err(Error::general_error(format!(
                    "read_table: {position}: column '{field_name}' is missing from a short row \
                     and not nullable"
                )))
            }
        }
        Some(raw) if is_null_cell(raw) => {
            if nullable {
                Ok(FieldValue::Null)
            } else {
                Err(Error::general_error(format!(
                    "read_table: {position}, column '{field_name}': null is not allowed \
                     (not nullable)"
                )))
            }
        }
        Some(raw) => parse_scalar(&raw.text, field_type).map_err(|e| {
            Error::general_error(format!(
                "read_table: {position}, column '{field_name}': {e}"
            ))
        }),
    }
}

/// The rows of one read that were shorter than the table, for the read's one aggregate warning.
#[derive(Debug, Default)]
struct ShortRows {
    count: usize,
    min_cells: usize,
    max_cells: usize,
}

impl ShortRows {
    fn note(&mut self, cells: usize) {
        if self.count == 0 {
            self.min_cells = cells;
            self.max_cells = cells;
        } else {
            self.min_cells = self.min_cells.min(cells);
            self.max_cells = self.max_cells.max(cells);
        }
        self.count += 1;
    }

    /// The decided wording; `None` when nothing was padded.
    fn warning(&self, width: usize) -> Option<String> {
        (self.count > 0).then(|| {
            format!(
                "There has been {} rows with number of cells between {} and {}, which is less \
                 than number of columns in the header ({width}).",
                self.count, self.min_cells, self.max_cells
            )
        })
    }
}

/// A row with more fields than the table has columns is refused, naming its line: the extra cell
/// has no column to go to, and dropping it would lose data silently. A shorter row is noted in
/// `short_rows` and padded by [`cell_value`]; the read reports it once, in aggregate.
fn check_row_width(
    row: &[RawField],
    width: usize,
    position: RowPosition,
    short_rows: &mut ShortRows,
) -> Result<(), Error> {
    if row.len() > width {
        return Err(Error::general_error(format!(
            "read_table: {position} has {} fields, but the table has {width} columns",
            row.len()
        )));
    }
    if row.len() < width {
        short_rows.note(row.len());
    }
    Ok(())
}

/// The schema-aware reader: strict, one pass, nothing guessed. Columns are matched by name
/// through the header when `header` is true, by position otherwise; a CSV column the schema does
/// not declare is an error naming it, and a non-nullable schema field missing from the file is
/// an error naming it too.
fn read_declared(
    rows: &[ParsedRow],
    header: bool,
    schema: &RecordSchema,
) -> Result<(RecordBatch, ReadReport), Error> {
    let (header_row, data_rows): (Option<&[RawField]>, &[ParsedRow]) = if header {
        match rows.split_first() {
            Some((h, rest)) => (Some(h.fields.as_slice()), rest),
            None => (Some(&[]), &[]),
        }
    } else {
        (None, rows)
    };

    let mut field_to_col: Vec<Option<usize>> = vec![None; schema.fields.len()];
    if let Some(header_row) = header_row {
        for (csv_col, field) in header_row.iter().enumerate() {
            match schema.index_of(&field.text) {
                Some(schema_idx) => field_to_col[schema_idx] = Some(csv_col),
                None => {
                    return Err(Error::general_error(format!(
                        "read_table: CSV column '{}' is not declared in the schema",
                        field.text
                    )))
                }
            }
        }
    } else {
        for (index, slot) in field_to_col.iter_mut().enumerate() {
            *slot = Some(index);
        }
    }

    for (index, field) in schema.fields.iter().enumerate() {
        if field_to_col[index].is_none() && !field.nullable {
            return Err(Error::general_error(format!(
                "read_table: CSV is missing non-nullable column '{}'",
                field.name
            )));
        }
    }

    let width = match header_row {
        Some(header_row) => header_row.len(),
        None => schema.fields.len(),
    };
    let mut builder = RecordBatchMut::with_capacity(Arc::new(schema.clone()), data_rows.len());
    let first_line = if header { 2 } else { 1 };
    let mut short_rows = ShortRows::default();
    for (row_index, row) in data_rows.iter().enumerate() {
        let position = RowPosition {
            line: row.line,
            record: row_index + 1,
            first_line,
        };
        check_row_width(&row.fields, width, position, &mut short_rows)?;
        let mut values = Vec::with_capacity(schema.fields.len());
        for (field_index, field) in schema.fields.iter().enumerate() {
            let raw = field_to_col[field_index].and_then(|csv_col| row.fields.get(csv_col));
            values.push(cell_value(
                raw,
                &field.name,
                field.data_type,
                field.nullable,
                position,
            )?);
        }
        builder.append_row(&values)?;
    }
    Ok((builder.freeze()?, report(&short_rows, width)))
}

fn report(short_rows: &ShortRows, width: usize) -> ReadReport {
    ReadReport {
        warnings: short_rows.warning(width).into_iter().collect(),
    }
}

fn cell_text(row: &[RawField], col: usize) -> Option<&str> {
    match row.get(col) {
        Some(field) if !is_null_cell(field) => Some(field.text.as_str()),
        Some(_) | None => None,
    }
}

/// The schema-less reader: one column at a time, [`infer::infer_column`] guesses its type from
/// every non-null cell, then every cell is parsed as that type. The `Id` is never guessed — the
/// inferred schema declares no key role at all.
fn read_inferred(rows: &[ParsedRow], header: bool) -> Result<(RecordBatch, ReadReport), Error> {
    let (names, data_rows): (Vec<String>, &[ParsedRow]) = if header {
        match rows.split_first() {
            Some((h, rest)) => (h.fields.iter().map(|field| field.text.clone()).collect(), rest),
            None => (Vec::new(), &[]),
        }
    } else {
        let width = rows.first().map(|row| row.fields.len()).unwrap_or(0);
        ((0..width).map(|i| format!("col{i}")).collect(), rows)
    };
    let width = names.len();

    let mut fields = Vec::with_capacity(width);
    for (col, name) in names.iter().enumerate() {
        let cells: Vec<Option<&str>> = data_rows
            .iter()
            .map(|row| cell_text(&row.fields, col))
            .collect();
        let (data_type, nullable) = infer::infer_column(&cells);
        let mut field = FieldSchema::new(name.clone(), data_type);
        if !nullable {
            field = field.not_null();
        }
        fields.push(field);
    }
    let field_types: Vec<FieldType> = fields.iter().map(|field| field.data_type).collect();
    let field_nullable: Vec<bool> = fields.iter().map(|field| field.nullable).collect();
    let schema = RecordSchema::new(fields)?;

    let mut builder = RecordBatchMut::with_capacity(Arc::new(schema), data_rows.len());
    let first_line = if header { 2 } else { 1 };
    let mut short_rows = ShortRows::default();
    for (row_index, row) in data_rows.iter().enumerate() {
        let position = RowPosition {
            line: row.line,
            record: row_index + 1,
            first_line,
        };
        check_row_width(&row.fields, width, position, &mut short_rows)?;
        let mut values = Vec::with_capacity(width);
        for col in 0..width {
            let raw = row.fields.get(col);
            values.push(cell_value(
                raw,
                &names[col],
                field_types[col],
                field_nullable[col],
                position,
            )?);
        }
        builder.append_row(&values)?;
    }
    Ok((builder.freeze()?, report(&short_rows, width)))
}

/// [`super::read_table_with_report`]'s CSV/TSV entry point. The report carries the one aggregate
/// warning for padded short rows.
pub(crate) fn read_csv(
    bytes: &[u8],
    separator: u8,
    schema: ReadSchema<'_>,
    options: &ReadOptions,
) -> Result<(RecordBatch, ReadReport), Error> {
    // A UTF-8 byte-order mark (Excel writes one) is an encoding marker, not part of the first cell.
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let rows = parse_rows(bytes, separator)?;
    match schema {
        ReadSchema::Declared(schema) => read_declared(&rows, options.header, schema),
        ReadSchema::Infer => read_inferred(&rows, options.header),
    }
}

// ---------------------------------------------------------------------------------------------
// Writing
// ---------------------------------------------------------------------------------------------

/// Quoted exactly when needed: the separator, a quote, CR or LF, or an empty string (so it is not
/// mistaken for null on the way back in).
fn needs_quoting(text: &str, separator: u8) -> bool {
    text.is_empty()
        || text.as_bytes().contains(&separator)
        || text.as_bytes().contains(&b'"')
        || text.as_bytes().contains(&b'\r')
        || text.as_bytes().contains(&b'\n')
}

fn render_cell(cell: Option<String>, separator: u8) -> String {
    match cell {
        None => String::new(),
        Some(text) => {
            if needs_quoting(&text, separator) {
                format!("\"{}\"", text.replace('"', "\"\""))
            } else {
                text
            }
        }
    }
}

/// [`super::write_table`]'s CSV/TSV entry point.
pub(crate) fn write_csv(
    view: &dyn RecordView,
    separator: u8,
    options: &WriteOptions,
) -> Result<Vec<u8>, Error> {
    let schema = view.schema();
    let sep_str = (separator as char).to_string();
    let mut out = String::new();

    if options.header {
        let header: Vec<String> = schema
            .fields
            .iter()
            .map(|field| render_cell(Some(field.name.clone()), separator))
            .collect();
        out.push_str(&header.join(&sep_str));
        out.push('\n');
    }

    for row in 0..view.len() {
        let mut cells = Vec::with_capacity(schema.fields.len());
        for col in 0..schema.fields.len() {
            let value = view.value(row, col)?;
            cells.push(render_cell(format_value(&value)?, separator));
        }
        out.push_str(&cells.join(&sep_str));
        out.push('\n');
    }
    Ok(out.into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Base64 is read strictly: what `base64_encode` writes round-trips, and truncated, over-padded
    /// or non-canonical input is an error rather than some other bytes.
    #[test]
    fn base64_decode_round_trips_and_refuses_malformed_input() -> Result<(), Error> {
        for bytes in [&b""[..], b"A", b"AB", b"ABC", b"ABCD", b"\x00\xff\x10"] {
            assert_eq!(base64_decode(&base64_encode(bytes))?, bytes.to_vec());
        }
        for malformed in ["A", "QUJ", "QQ", "QQ=", "Q===", "QR==", "QUI=Q==="] {
            assert!(base64_decode(malformed).is_err(), "{malformed:?} must be refused");
        }
        Ok(())
    }
    use crate::formats::{read_table, read_table_with_report, write_table, TableFormat};
    use liquers_records::{
        FieldSchema, FieldType, FieldValue, KeyRole, RecordBatchMut, RecordSchema, RecordViewMut,
    };

    fn text_schema(names: &[&str]) -> Arc<RecordSchema> {
        Arc::new(RecordSchema::new(names.iter().map(|n| FieldSchema::new(*n, FieldType::Text)).collect()).unwrap())
    }

    /// `Error` has no `From<FromUtf8Error>` — CLAUDE.md's error-handling convention is typed
    /// constructors only (`Error::from_error`, never a blanket `From` impl added just for `?` to
    /// use), so Phase 3's `String::from_utf8(bytes)?` does not compile as written. Corrected here
    /// with an explicit conversion rather than by adding a `From` impl to `liquers_core::error`
    /// (out of scope for this crate, and a wider change than one test needs).
    fn utf8(bytes: Vec<u8>) -> Result<String, Error> {
        String::from_utf8(bytes).map_err(|e| Error::from_error(ErrorType::ConversionError, e))
    }

    /// The test Phase 2 names directly (§"Tier 1 — in `records`"): an unquoted empty field is
    /// null, a quoted `""` is the empty string, and the distinction survives a round trip.
    #[test]
    fn column_null_distinct_from_empty_string() -> Result<(), Error> {
        let schema = text_schema(&["name", "note"]);
        let mut batch = RecordBatchMut::with_capacity(schema, 2);
        batch.append_row(&[FieldValue::Text(Arc::from("Alice")), FieldValue::Null])?;
        batch.append_row(&[FieldValue::Text(Arc::from("Bob")), FieldValue::Text(Arc::from(""))])?;
        let batch = batch.freeze()?;

        let bytes = write_table(&batch, TableFormat::Csv { separator: b',' }, &WriteOptions::default())?;
        let read_back = read_table(&bytes, TableFormat::Csv { separator: b',' }, ReadSchema::Infer, &ReadOptions::default())?;

        assert_eq!(read_back.value(0, 1)?, FieldValue::Null);
        assert_eq!(read_back.value(1, 1)?, FieldValue::Text(Arc::from("")));
        Ok(())
    }

    #[test]
    fn csv_quoting_corpus_separator_in_cell() -> Result<(), Error> {
        let schema = text_schema(&["cell"]);
        let mut batch = RecordBatchMut::with_capacity(schema, 2);
        batch.append_row(&[FieldValue::Text(Arc::from("a,b"))])?;
        batch.append_row(&[FieldValue::Text(Arc::from("c"))])?;
        let batch = batch.freeze()?;
        let bytes = write_table(&batch, TableFormat::Csv { separator: b',' }, &WriteOptions::default())?;
        assert!(utf8(bytes)?.contains("\"a,b\""));
        Ok(())
    }

    #[test]
    fn csv_quoting_corpus_crlf_and_lf_both_read_as_line_breaks() -> Result<(), Error> {
        let schema = text_schema(&["cell"]);
        let mut batch = RecordBatchMut::with_capacity(schema, 1);
        batch.append_row(&[FieldValue::Text(Arc::from("line1\nline2"))])?;
        let batch = batch.freeze()?;
        let bytes = write_table(&batch, TableFormat::Csv { separator: b',' }, &WriteOptions::default())?;
        let written = utf8(bytes)?;
        assert!(written.contains("\"line1\nline2\"")); // embedded newline is quoted

        // Both line-ending conventions parse as the same two data rows for a file with two records.
        let lf = b"cell\nfirst\nsecond\n";
        let crlf = b"cell\r\nfirst\r\nsecond\r\n";
        let via_lf = read_table(lf, TableFormat::Csv { separator: b',' }, ReadSchema::Infer, &ReadOptions::default())?;
        let via_crlf = read_table(crlf, TableFormat::Csv { separator: b',' }, ReadSchema::Infer, &ReadOptions::default())?;
        assert_eq!(via_lf.len, via_crlf.len);
        assert_eq!(via_lf.value(1, 0)?, via_crlf.value(1, 0)?);
        Ok(())
    }

    #[test]
    fn csv_quoting_corpus_doubled_quote_is_one_literal_quote() -> Result<(), Error> {
        let csv = b"cell\n\"a\"\"b\"\n"; // the cell `a"b`
        let batch = read_table(csv, TableFormat::Csv { separator: b',' }, ReadSchema::Infer, &ReadOptions::default())?;
        assert_eq!(batch.value(0, 0)?, FieldValue::Text(Arc::from("a\"b")));
        Ok(())
    }

    #[test]
    fn csv_malformed_error_names_the_line_number() {
        let malformed = b"name,age\nalice,thirty\n"; // "thirty" does not parse as Int
        let schema = RecordSchema::new(vec![
            FieldSchema::new("name", FieldType::Text),
            FieldSchema::new("age", FieldType::Int),
        ])
        .unwrap();
        let err = read_table(malformed, TableFormat::Csv { separator: b',' }, ReadSchema::Declared(&schema), &ReadOptions::default())
            .expect_err("thirty is not an Int");
        let message = format!("{err}");
        // The bad cell is on line 2 of the file (line 1 is the header).
        assert!(message.contains("line 2,"), "error should name line 2: {message}");
        assert!(message.contains("'age'") && message.contains("thirty"), "error should name the cell: {message}");
    }

    #[test]
    fn csv_leading_utf8_bom_is_not_part_of_the_first_header() -> Result<(), Error> {
        let csv = b"\xEF\xBB\xBFname,n\nx,1\n";
        let batch = read_table(csv, TableFormat::Csv { separator: b',' }, ReadSchema::Infer, &ReadOptions::default())?;
        assert_eq!(batch.schema.fields[0].name, "name");
        let schema = RecordSchema::new(vec![
            FieldSchema::new("name", FieldType::Text),
            FieldSchema::new("n", FieldType::Int),
        ])?;
        let batch = read_table(csv, TableFormat::Csv { separator: b',' }, ReadSchema::Declared(&schema), &ReadOptions::default())?;
        assert_eq!(batch.value(0, 1)?, FieldValue::Int(1));
        Ok(())
    }

    #[test]
    fn csv_row_wider_than_the_header_is_refused_naming_the_line() -> Result<(), Error> {
        let csv = b"a,b\n1,2\n3,4,5\n";
        let schema = RecordSchema::new(vec![
            FieldSchema::new("a", FieldType::Int),
            FieldSchema::new("b", FieldType::Int),
        ])?;
        for read_schema in [ReadSchema::Infer, ReadSchema::Declared(&schema)] {
            let error = read_table(csv, TableFormat::Csv { separator: b',' }, read_schema, &ReadOptions::default())
                .expect_err("a row with an extra cell");
            let message = format!("{error}");
            assert!(message.contains("line 3 ") && message.contains("3 fields"), "unexpected error: {message}");
        }
        // Without a header, the width is the first row's.
        let error = read_table(b"1,2\n3,4,5\n", TableFormat::Csv { separator: b',' }, ReadSchema::Infer, &ReadOptions { header: false })
            .expect_err("a row with an extra cell");
        assert!(format!("{error}").contains("line 2 "), "unexpected error: {error}");
        Ok(())
    }

    fn csv() -> TableFormat {
        TableFormat::Csv { separator: b',' }
    }

    const SHORT_ROWS: &[u8] = b"a,b,c,d\n1,2,3,4\n5,6\n7,8,9\n10,11,12,13\n";
    const SHORT_ROWS_WARNING: &str = "There has been 2 rows with number of cells between 2 and 3, \
         which is less than number of columns in the header (4).";

    #[test]
    fn csv_error_names_physical_line_and_record() {
        let error = read_table_with_report(b"a,b\n1,\"x\ny\"\n2,z,extra\n", csv(), ReadSchema::Infer, &ReadOptions::default())
            .expect_err("a row with an extra cell");
        let message = format!("{error}");
        assert!(message.contains("CSV line 4 (record 2)"), "unexpected error: {message}");
    }

    #[test]
    fn csv_short_rows_are_padded_and_reported_once() -> Result<(), Error> {
        let schema = RecordSchema::new(["a", "b", "c", "d"].iter().map(|name| FieldSchema::new(*name, FieldType::Int)).collect())?;
        let (batch, report) = read_table_with_report(SHORT_ROWS, csv(), ReadSchema::Declared(&schema), &ReadOptions::default())?;
        assert_eq!(batch.len(), 4);
        assert_eq!(batch.value(1, 1)?, FieldValue::Int(6));
        assert_eq!(batch.value(1, 2)?, FieldValue::Null);
        assert_eq!(batch.value(1, 3)?, FieldValue::Null);
        assert_eq!(batch.value(2, 3)?, FieldValue::Null);
        assert_eq!(report.warnings, vec![SHORT_ROWS_WARNING.to_string()]);
        Ok(())
    }

    #[test]
    fn csv_inferred_short_rows_are_padded_and_reported_once() -> Result<(), Error> {
        let (batch, report) = read_table_with_report(SHORT_ROWS, csv(), ReadSchema::Infer, &ReadOptions::default())?;
        assert_eq!(batch.len(), 4);
        assert_eq!(batch.value(1, 3)?, FieldValue::Null);
        assert_eq!(batch.value(3, 3)?, FieldValue::Int(13));
        assert_eq!(report.warnings, vec![SHORT_ROWS_WARNING.to_string()]);
        Ok(())
    }

    #[test]
    fn csv_short_non_nullable_text_pads_empty() -> Result<(), Error> {
        let schema = RecordSchema::new(vec![
            FieldSchema::new("a", FieldType::Int),
            FieldSchema::new("b", FieldType::Text).not_null(),
        ])?;
        let (batch, report) = read_table_with_report(b"a,b\n1\n", csv(), ReadSchema::Declared(&schema), &ReadOptions::default())?;
        assert_eq!(batch.value(0, 1)?, FieldValue::Text(Arc::from("")));
        assert_eq!(report.warnings.len(), 1);
        Ok(())
    }

    #[test]
    fn csv_short_non_nullable_int_is_an_error() -> Result<(), Error> {
        let schema = RecordSchema::new(vec![
            FieldSchema::new("a", FieldType::Text),
            FieldSchema::new("b", FieldType::Int).not_null(),
        ])?;
        let error = read_table_with_report(b"a,b\nx,1\ny\n", csv(), ReadSchema::Declared(&schema), &ReadOptions::default())
            .expect_err("nothing honest to pad an Int with");
        let message = format!("{error}");
        assert!(message.contains("CSV line 3") && message.contains("'b'"), "unexpected error: {message}");
        Ok(())
    }

    #[test]
    fn csv_trailing_newline_is_not_a_row() -> Result<(), Error> {
        let (batch, report) = read_table_with_report(b"a,b\n1,2\n", csv(), ReadSchema::Infer, &ReadOptions::default())?;
        assert_eq!(batch.len(), 1);
        assert!(report.warnings.is_empty());
        let (batch, report) = read_table_with_report(b"a,b\r\n1,2\r\n", csv(), ReadSchema::Infer, &ReadOptions::default())?;
        assert_eq!(batch.len(), 1);
        assert!(report.warnings.is_empty());
        Ok(())
    }

    #[test]
    fn csv_round_trip_scalar_column_types() -> Result<(), Error> {
        // Vector and Binary are exercised in the NDJSON/table tests, where a native JSON-ish
        // shape makes the fixture legible; CSV's own contract is the scalar types below.
        let schema = Arc::new(RecordSchema::new(vec![
            FieldSchema::new("flag", FieldType::Bool),
            FieldSchema::new("count", FieldType::Int),
            FieldSchema::new("amount", FieldType::Float),
            FieldSchema::new("label", FieldType::Text),
            FieldSchema::new("day", FieldType::Date),
            FieldSchema::new("at", FieldType::Timestamp),
        ])?);
        let mut batch = RecordBatchMut::with_capacity(schema.clone(), 1);
        batch.append_row(&[
            FieldValue::Bool(true),
            FieldValue::Int(42),
            FieldValue::Float(3.5),
            FieldValue::Text(Arc::from("hello")),
            FieldValue::Date(19_570),
            FieldValue::Timestamp(1_726_944_000_000_000),
        ])?;
        let batch = batch.freeze()?;
        let bytes = write_table(&batch, TableFormat::Csv { separator: b',' }, &WriteOptions::default())?;
        let read_back = read_table(&bytes, TableFormat::Csv { separator: b',' }, ReadSchema::Declared(&schema), &ReadOptions::default())?;
        for col in 0..schema.fields.len() {
            assert_eq!(read_back.value(0, col)?, batch.value(0, col)?, "column {col} round-trips");
        }
        Ok(())
    }

    #[test]
    fn csv_separator_is_configurable_for_tsv() -> Result<(), Error> {
        let schema = text_schema(&["a", "b"]);
        let mut batch = RecordBatchMut::with_capacity(schema, 1);
        batch.append_row(&[FieldValue::Text(Arc::from("x")), FieldValue::Text(Arc::from("y"))])?;
        let batch = batch.freeze()?;
        let bytes = write_table(&batch, TableFormat::Csv { separator: b'\t' }, &WriteOptions::default())?;
        let written = utf8(bytes)?;
        assert!(written.contains("x\ty"));
        Ok(())
    }

    #[test]
    fn schema_less_inference_canonical_int_rule_keeps_leading_zeros_as_text() -> Result<(), Error> {
        let csv = b"id,code,amount\n1,01234,+5\n2,9999,1e3\n";
        let batch = read_table(csv, TableFormat::Csv { separator: b',' }, ReadSchema::Infer, &ReadOptions::default())?;
        assert_eq!(batch.schema.fields[1].data_type, FieldType::Text); // 01234: leading zero
        assert_eq!(batch.schema.fields[2].data_type, FieldType::Text); // +5, 1e3: not canonical Int
        Ok(())
    }

    #[test]
    fn schema_less_float_column_round_trips_as_float() -> Result<(), Error> {
        // Integral, huge, tiny and negative-zero floats must not come back as Int or Text.
        let schema = Arc::new(RecordSchema::new(vec![FieldSchema::new("x", FieldType::Float).not_null()])?);
        let columns: [&[f64]; 3] = [&[1.0, 3.0], &[-0.0], &[1e300, 1e-7, 2.5, f64::INFINITY]];
        for values in columns {
            let mut batch = RecordBatchMut::with_capacity(schema.clone(), values.len());
            for v in values {
                batch.append_row(&[FieldValue::Float(*v)])?;
            }
            let batch = batch.freeze()?;
            let bytes = write_table(&batch, TableFormat::Csv { separator: b',' }, &WriteOptions::default())?;
            let back = read_table(&bytes, TableFormat::Csv { separator: b',' }, ReadSchema::Infer, &ReadOptions::default())?;
            assert_eq!(back.schema.fields[0].data_type, FieldType::Float, "{}", utf8(bytes.clone())?);
            for (row, v) in values.iter().enumerate() {
                match back.value(row, 0)? {
                    FieldValue::Float(read) => assert_eq!(read.to_bits(), v.to_bits(), "row {row}"),
                    other => panic!("row {row}: {other:?}"),
                }
            }
        }
        Ok(())
    }

    #[test]
    fn schema_less_inference_tries_bool_int_float_date_timestamp_then_text() -> Result<(), Error> {
        let csv = b"c_bool,c_int,c_float,c_date,c_ts,c_text\n\
                    true,42,3.14,2026-09-25,2026-09-25T12:00:00Z,hello\n\
                    false,43,2.71,2026-09-26,2026-09-26T13:00:00Z,world\n";
        let batch = read_table(csv, TableFormat::Csv { separator: b',' }, ReadSchema::Infer, &ReadOptions::default())?;
        assert_eq!(batch.schema.fields[0].data_type, FieldType::Bool);
        assert_eq!(batch.schema.fields[1].data_type, FieldType::Int);
        assert_eq!(batch.schema.fields[2].data_type, FieldType::Float);
        assert_eq!(batch.schema.fields[3].data_type, FieldType::Date);
        assert_eq!(batch.schema.fields[4].data_type, FieldType::Timestamp);
        assert_eq!(batch.schema.fields[5].data_type, FieldType::Text);
        Ok(())
    }

    #[test]
    fn schema_aware_read_declared_text_keeps_leading_zeros() -> Result<(), Error> {
        let csv = b"zip\n01234\n90210\n";
        let schema = RecordSchema::new(vec![FieldSchema::new("zip", FieldType::Text)])?;
        let batch = read_table(csv, TableFormat::Csv { separator: b',' }, ReadSchema::Declared(&schema), &ReadOptions::default())?;
        assert_eq!(batch.value(0, 0)?, FieldValue::Text(Arc::from("01234")));
        Ok(())
    }

    #[test]
    fn schema_aware_read_refuses_an_undeclared_column() {
        let csv = b"name,age,email\nalice,30,alice@example.com\n";
        let schema = RecordSchema::new(vec![
            FieldSchema::new("name", FieldType::Text),
            FieldSchema::new("age", FieldType::Int),
        ])
        .unwrap();
        let err = read_table(csv, TableFormat::Csv { separator: b',' }, ReadSchema::Declared(&schema), &ReadOptions::default())
            .expect_err("email is not in the schema");
        assert!(format!("{err}").contains("email"));
    }

    #[test]
    fn schema_aware_read_refuses_a_missing_non_nullable_column() {
        let csv = b"name\nalice\n"; // age is not_null and missing
        let schema = RecordSchema::new(vec![
            FieldSchema::new("name", FieldType::Text),
            FieldSchema::new("age", FieldType::Int).not_null(),
        ])
        .unwrap();
        assert!(read_table(csv, TableFormat::Csv { separator: b',' }, ReadSchema::Declared(&schema), &ReadOptions::default()).is_err());
    }

    #[test]
    fn schema_aware_read_unparsable_cell_names_row_and_column() {
        let csv = b"name,age\nalice,not_a_number\n";
        let schema = RecordSchema::new(vec![
            FieldSchema::new("name", FieldType::Text),
            FieldSchema::new("age", FieldType::Int),
        ])
        .unwrap();
        let err = read_table(csv, TableFormat::Csv { separator: b',' }, ReadSchema::Declared(&schema), &ReadOptions::default())
            .expect_err("not_a_number is not an Int");
        let message = format!("{err}");
        assert!(message.contains("age"), "error should name the column: {message}");
    }

    // --- Additional coverage beyond Phase 3 §3.1 ---

    #[test]
    fn csv_round_trip_binary_and_vector_columns() -> Result<(), Error> {
        let schema = Arc::new(RecordSchema::new(vec![
            FieldSchema::new("blob", FieldType::Binary),
            FieldSchema::new("embedding", FieldType::Vector),
        ])?);
        let mut batch = RecordBatchMut::with_capacity(schema.clone(), 1);
        batch.append_row(&[
            FieldValue::Bytes(Arc::from(vec![0u8, 1, 2, 255])),
            FieldValue::Vector(Arc::from(vec![1.5f32, -2.5, 3.0])),
        ])?;
        let batch = batch.freeze()?;
        let bytes = write_table(&batch, TableFormat::Csv { separator: b',' }, &WriteOptions::default())?;
        let read_back = read_table(&bytes, TableFormat::Csv { separator: b',' }, ReadSchema::Declared(&schema), &ReadOptions::default())?;
        assert_eq!(read_back.value(0, 0)?, batch.value(0, 0)?);
        assert_eq!(read_back.value(0, 1)?, batch.value(0, 1)?);
        Ok(())
    }

    #[test]
    fn csv_header_uses_field_names() -> Result<(), Error> {
        let schema = text_schema(&["first_name", "last_name"]);
        let mut batch = RecordBatchMut::with_capacity(schema, 1);
        batch.append_row(&[FieldValue::Text(Arc::from("a")), FieldValue::Text(Arc::from("b"))])?;
        let batch = batch.freeze()?;
        let bytes = write_table(&batch, TableFormat::Csv { separator: b',' }, &WriteOptions::default())?;
        let written = utf8(bytes)?;
        assert!(written.starts_with("first_name,last_name\n"));
        Ok(())
    }

    #[test]
    fn csv_without_header_option_omits_the_header_row() -> Result<(), Error> {
        let schema = text_schema(&["a"]);
        let mut batch = RecordBatchMut::with_capacity(schema, 1);
        batch.append_row(&[FieldValue::Text(Arc::from("x"))])?;
        let batch = batch.freeze()?;
        let options = WriteOptions { header: false };
        let bytes = write_table(&batch, TableFormat::Csv { separator: b',' }, &options)?;
        assert_eq!(utf8(bytes)?, "x\n");
        Ok(())
    }

    #[test]
    fn csv_declared_uint_and_bool_round_trip() -> Result<(), Error> {
        let schema = Arc::new(RecordSchema::new(vec![
            FieldSchema::new("count", FieldType::UInt),
            FieldSchema::new("active", FieldType::Bool),
        ])?);
        let mut batch = RecordBatchMut::with_capacity(schema.clone(), 1);
        batch.append_row(&[FieldValue::UInt(7), FieldValue::Bool(false)])?;
        let batch = batch.freeze()?;
        let bytes = write_table(&batch, TableFormat::Csv { separator: b',' }, &WriteOptions::default())?;
        let read_back = read_table(&bytes, TableFormat::Csv { separator: b',' }, ReadSchema::Declared(&schema), &ReadOptions::default())?;
        assert_eq!(read_back.value(0, 0)?, FieldValue::UInt(7));
        assert_eq!(read_back.value(0, 1)?, FieldValue::Bool(false));
        Ok(())
    }

    #[test]
    fn declared_id_field_role_is_not_disturbed_by_reading() -> Result<(), Error> {
        // A schema with an `Id` field reads normally; the role lives in the schema, not the file.
        let schema = RecordSchema::new(vec![
            FieldSchema::new("id", FieldType::Text).with_key(KeyRole::Id),
            FieldSchema::new("value", FieldType::Int),
        ])?;
        let csv = b"id,value\nabc,1\n";
        let batch = read_table(csv, TableFormat::Csv { separator: b',' }, ReadSchema::Declared(&schema), &ReadOptions::default())?;
        assert_eq!(batch.schema.id_field(), Some(0));
        assert_eq!(batch.value(0, 0)?, FieldValue::Text(Arc::from("abc")));
        Ok(())
    }

    #[test]
    fn read_declared_by_position_when_header_is_false() -> Result<(), Error> {
        let csv = b"alice,30\n";
        let schema = RecordSchema::new(vec![
            FieldSchema::new("name", FieldType::Text),
            FieldSchema::new("age", FieldType::Int),
        ])?;
        let options = ReadOptions { header: false };
        let batch = read_table(csv, TableFormat::Csv { separator: b',' }, ReadSchema::Declared(&schema), &options)?;
        assert_eq!(batch.value(0, 0)?, FieldValue::Text(Arc::from("alice")));
        assert_eq!(batch.value(0, 1)?, FieldValue::Int(30));
        Ok(())
    }

    #[test]
    fn read_inferred_without_header_uses_positional_names() -> Result<(), Error> {
        let csv = b"1,2\n3,4\n";
        let options = ReadOptions { header: false };
        let batch = read_table(csv, TableFormat::Csv { separator: b',' }, ReadSchema::Infer, &options)?;
        assert_eq!(batch.schema.fields[0].name, "col0");
        assert_eq!(batch.schema.fields[1].name, "col1");
        assert_eq!(batch.len, 2);
        Ok(())
    }

    #[test]
    fn empty_input_reads_as_an_empty_batch() -> Result<(), Error> {
        let batch = read_table(b"", TableFormat::Csv { separator: b',' }, ReadSchema::Infer, &ReadOptions::default())?;
        assert_eq!(batch.len, 0);
        Ok(())
    }

    #[test]
    fn unterminated_quote_is_an_error_not_a_panic() {
        let csv = b"cell\n\"unterminated";
        assert!(read_table(csv, TableFormat::Csv { separator: b',' }, ReadSchema::Infer, &ReadOptions::default()).is_err());
    }
}
