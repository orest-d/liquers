//! Minimal Parquet writer (`parquet` feature). See
//! `specs/design/record-streams/phase2-architecture.md` §"Tier 3 — Parquet: writing is cheap,
//! reading is not" for the accepted subset and why this crate reads no Parquet at all:
//! `formats/mod.rs`'s `read_table` refuses `TableFormat::Parquet`, naming `liquers-lib`'s polars
//! bridge — a real-world Parquet file (pyarrow, polars, ...) routinely uses dictionary encoding,
//! snappy/zstd compression and data-page v2, none of which a from-scratch reader here would ever
//! need to produce for its own files, so a matching from-scratch *reader* would only ever handle
//! its own output. Better to refuse honestly and read through a real implementation.
//!
//! **Layout**: `PAR1`, one row group (the whole materialized batch), one column chunk per schema
//! field, one version-1 data page per chunk, `PLAIN` encoding, and a `FileMetaData` footer with
//! `liquers.schema` (the full [`RecordSchema`] as JSON) in `key_value_metadata`. A nullable field's
//! page carries definition levels (RLE/bit-packed hybrid, always emitted as a single bit-packed
//! run — see [`encode_definition_levels`]); a non-nullable field carries none (`REQUIRED`). Pages
//! are GZIP-compressed via `flate2` by default; `UNCOMPRESSED` is supported internally (used by
//! this file's own tests) but not exposed through [`write_parquet`]/`write_table`, which have no
//! compression-choice parameter yet — see [`CompressionCodec`].
//!
//! **Type mapping**: `Bool` -> `BOOLEAN` (bit-packed); `Int` -> `INT64`; `UInt` -> `INT64` with
//! `LogicalType::INTEGER(64, unsigned)` and `ConvertedType::UINT_64`, so a reader reads the bits
//! back as `u64` and statistics compare as unsigned; `Float` -> `DOUBLE`; `Text` -> `BYTE_ARRAY`
//! with `LogicalType::STRING` (plus the legacy `ConvertedType::UTF8` for older readers); `Binary`
//! -> plain `BYTE_ARRAY`; `Date` -> `INT32` with `LogicalType::DATE`; `Timestamp` -> `INT64` with
//! `LogicalType::TIMESTAMP(isAdjustedToUTC: true, unit: MICROS)`. **`Vector` is refused**, naming
//! the column: Parquet's `LIST` needs repetition levels this flat writer does not produce.
//!
//! **No generated code**, matching `formats/ipc.rs`'s decision: `formats/thrift.rs` is a small,
//! from-scratch Thrift compact-protocol encoder for the handful of structures below, built
//! directly against `parquet.thrift`'s field ids (cited in comments at each write site) rather
//! than a generated client of a Thrift crate.

#![cfg(feature = "parquet")]

use std::io::Write;

use flate2::write::GzEncoder;
use flate2::Compression;
use liquers_core::error::{Error, ErrorType};

use crate::batch::{RecordBatch, RecordView};
use crate::buffer::{AlignedBuffer, Bitmap, Buffer};
use crate::column::Column;
use crate::schema::{FieldSchema, RecordSchema};

use super::thrift::{write_unsigned_varint, CompactWriter, TYPE_BINARY, TYPE_I32, TYPE_STRUCT};

/// The four-byte magic that opens and closes a Parquet file.
const PAR1: &[u8; 4] = b"PAR1";

/// `parquet.thrift` enum wire values this writer emits. Field ids for the *structs* below are
/// cited individually, next to each write, rather than collected here.
mod pq {
    // `Type` (physical type)
    pub const BOOLEAN: i32 = 0;
    pub const INT32: i32 = 1;
    pub const INT64: i32 = 2;
    pub const DOUBLE: i32 = 5;
    pub const BYTE_ARRAY: i32 = 6;

    // `FieldRepetitionType`
    pub const REQUIRED: i32 = 0;
    pub const OPTIONAL: i32 = 1;

    // `ConvertedType` (legacy annotations, written alongside `LogicalType` for older readers)
    pub const UTF8: i32 = 0;
    pub const DATE: i32 = 6;
    pub const TIMESTAMP_MICROS: i32 = 10;
    pub const UINT_64: i32 = 14;

    // `Encoding`
    pub const PLAIN: i32 = 0;
    pub const RLE: i32 = 3;

    // `CompressionCodec`
    pub const UNCOMPRESSED: i32 = 0;
    pub const GZIP: i32 = 2;

    // `PageType`
    pub const DATA_PAGE: i32 = 0;
}

/// Compression for the pages this writer emits. [`write_parquet`] always uses [`Gzip`]; the writer
/// itself (and this file's tests, via [`write_parquet_with_codec`]) can also produce
/// [`Uncompressed`] output — `Uncompressed` is not built from non-test code today, hence the
/// `allow`.
///
/// [`Gzip`]: CompressionCodec::Gzip
/// [`Uncompressed`]: CompressionCodec::Uncompressed
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum CompressionCodec {
    Uncompressed,
    Gzip,
}

impl CompressionCodec {
    fn wire_value(self) -> i32 {
        match self {
            CompressionCodec::Uncompressed => pq::UNCOMPRESSED,
            CompressionCodec::Gzip => pq::GZIP,
        }
    }
}

fn compress(codec: CompressionCodec, bytes: &[u8]) -> Result<Vec<u8>, Error> {
    match codec {
        CompressionCodec::Uncompressed => Ok(bytes.to_vec()),
        CompressionCodec::Gzip => {
            let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
            encoder
                .write_all(bytes)
                .map_err(|e| Error::from_error(ErrorType::General, e))?;
            encoder
                .finish()
                .map_err(|e| Error::from_error(ErrorType::General, e))
        }
    }
}

/// `LogicalType` union variants this writer produces (`STRING`=1, `DATE`=6, `TIMESTAMP`=8 in
/// `parquet.thrift`'s field-id order — see [`write_logical_type`]).
enum LogicalTypeSpec {
    String,
    Date,
    TimestampMicros,
    /// `INTEGER(bitWidth = 64, isSigned = false)`: the bits are written as `INT64`, and a reader
    /// that honours the annotation reads them back as `u64`.
    UInt64,
}

/// One field's encoded page content plus everything the footer needs to describe it.
struct EncodedColumn {
    physical_type: i32,
    converted_type: Option<i32>,
    logical_type: Option<LogicalTypeSpec>,
    repetition_type: i32,
    /// `Some` (length == the column's row count) for an `OPTIONAL` field; `None` for `REQUIRED`.
    def_levels: Option<Vec<bool>>,
    /// `PLAIN`-encoded bytes for the *present* (non-null) values only, in row order.
    values: Vec<u8>,
    null_count: i64,
    /// `PLAIN`-encoded, length-prefix-free min/max bytes over the present values — absent only
    /// when there are none (every row null, or zero rows).
    min_max: Option<(Vec<u8>, Vec<u8>)>,
}

/// `BOOLEAN` values in `PLAIN` encoding: the non-null values bit-packed, least significant bit
/// first. Statistics are one byte each, `0` or `1`.
fn encode_bool(values: &Bitmap, null_mask: &Bitmap) -> (Vec<u8>, Option<(Vec<u8>, Vec<u8>)>) {
    let mut bytes = Vec::with_capacity(values.len() / 8 + 1);
    let mut written = 0usize;
    let mut min: Option<bool> = None;
    let mut max: Option<bool> = None;
    for i in 0..values.len() {
        if null_mask.get(i) {
            continue;
        }
        let v = values.get(i);
        if written % 8 == 0 {
            bytes.push(0u8);
        }
        if v {
            if let Some(last) = bytes.last_mut() {
                *last |= 1 << (written % 8);
            }
        }
        written += 1;
        min = Some(min.map_or(v, |m| m && v));
        max = Some(max.map_or(v, |m| m || v));
    }
    let stats = min
        .zip(max)
        .map(|(mn, mx)| (vec![u8::from(mn)], vec![u8::from(mx)]));
    (bytes, stats)
}

fn encode_i64(values: &[i64], null_mask: &Bitmap) -> (Vec<u8>, Option<(Vec<u8>, Vec<u8>)>) {
    let mut bytes = Vec::with_capacity(values.len() * 8);
    let mut min: Option<i64> = None;
    let mut max: Option<i64> = None;
    for (i, &v) in values.iter().enumerate() {
        if null_mask.get(i) {
            continue;
        }
        bytes.extend_from_slice(&v.to_le_bytes());
        min = Some(min.map_or(v, |m| m.min(v)));
        max = Some(max.map_or(v, |m| m.max(v)));
    }
    let stats = min.zip(max).map(|(mn, mx)| (mn.to_le_bytes().to_vec(), mx.to_le_bytes().to_vec()));
    (bytes, stats)
}

/// `UInt` -> plain `INT64`: each `u64` reinterpreted as `i64` (see the module doc comment's type
/// mapping section for the documented tradeoff at or above `i64::MAX`).
fn encode_uint64_as_i64(values: &[u64], null_mask: &Bitmap) -> (Vec<u8>, Option<(Vec<u8>, Vec<u8>)>) {
    let mut bytes = Vec::with_capacity(values.len() * 8);
    let mut min: Option<u64> = None;
    let mut max: Option<u64> = None;
    for (i, &v) in values.iter().enumerate() {
        if null_mask.get(i) {
            continue;
        }
        // The same eight bytes as the `u64`; the `INTEGER(64, unsigned)` annotation tells a
        // reader to read them back as unsigned, and statistics compare as unsigned too.
        bytes.extend_from_slice(&v.to_le_bytes());
        min = Some(min.map_or(v, |m: u64| m.min(v)));
        max = Some(max.map_or(v, |m: u64| m.max(v)));
    }
    let stats = min.zip(max).map(|(mn, mx)| (mn.to_le_bytes().to_vec(), mx.to_le_bytes().to_vec()));
    (bytes, stats)
}

fn encode_i32(values: &[i32], null_mask: &Bitmap) -> (Vec<u8>, Option<(Vec<u8>, Vec<u8>)>) {
    let mut bytes = Vec::with_capacity(values.len() * 4);
    let mut min: Option<i32> = None;
    let mut max: Option<i32> = None;
    for (i, &v) in values.iter().enumerate() {
        if null_mask.get(i) {
            continue;
        }
        bytes.extend_from_slice(&v.to_le_bytes());
        min = Some(min.map_or(v, |m| m.min(v)));
        max = Some(max.map_or(v, |m| m.max(v)));
    }
    let stats = min.zip(max).map(|(mn, mx)| (mn.to_le_bytes().to_vec(), mx.to_le_bytes().to_vec()));
    (bytes, stats)
}

fn encode_f64(values: &[f64], null_mask: &Bitmap) -> (Vec<u8>, Option<(Vec<u8>, Vec<u8>)>) {
    let mut bytes = Vec::with_capacity(values.len() * 8);
    let mut min: Option<f64> = None;
    let mut max: Option<f64> = None;
    for (i, &v) in values.iter().enumerate() {
        if null_mask.get(i) {
            continue;
        }
        bytes.extend_from_slice(&v.to_le_bytes());
        if !v.is_nan() {
            min = Some(min.map_or(v, |m| m.min(v)));
            max = Some(max.map_or(v, |m| m.max(v)));
        }
    }
    // Parquet's `Statistics` rule for floating point: `-0.0 == +0.0`, so a zero min is written as
    // `-0.0` and a zero max as `+0.0`, whichever zero the data held.
    let min = min.map(|m| if m == 0.0 { -0.0 } else { m });
    let max = max.map(|m| if m == 0.0 { 0.0 } else { m });
    let stats = min.zip(max).map(|(mn, mx)| (mn.to_le_bytes().to_vec(), mx.to_le_bytes().to_vec()));
    (bytes, stats)
}

/// `Text`/`Binary`: `PLAIN` `BYTE_ARRAY` is a 4-byte little-endian length plus the bytes, per
/// value; min/max are the raw bytes with **no** length prefix, per `Statistics`' doc comment
/// ("variable-length byte arrays do not include a length prefix").
fn encode_bytes(
    offsets: &Buffer<i32>,
    data: &AlignedBuffer,
    null_mask: &Bitmap,
) -> (Vec<u8>, Option<(Vec<u8>, Vec<u8>)>) {
    let off = offsets.as_slice();
    let bytes_data = data.as_bytes();
    let n = off.len().saturating_sub(1);
    let mut out = Vec::new();
    let mut min: Option<Vec<u8>> = None;
    let mut max: Option<Vec<u8>> = None;
    for i in 0..n {
        if null_mask.get(i) {
            continue;
        }
        let start = off[i] as usize;
        let end = off[i + 1] as usize;
        let slice = &bytes_data[start..end];
        out.extend_from_slice(&(slice.len() as i32).to_le_bytes());
        out.extend_from_slice(slice);
        min = Some(match min {
            Some(m) if m.as_slice() <= slice => m,
            Some(_) | None => slice.to_vec(),
        });
        max = Some(match max {
            Some(m) if m.as_slice() >= slice => m,
            Some(_) | None => slice.to_vec(),
        });
    }
    (out, min.zip(max))
}

/// Maps one schema field's column onto its Parquet physical type, repetition, logical/converted
/// type annotation, `PLAIN`-encoded values and min/max statistics. `Vector` is refused here, naming
/// the column, before any bytes for it are written.
fn encode_field(field: &FieldSchema, column: &Column) -> Result<EncodedColumn, Error> {
    let null_mask = column.null_mask();
    let repetition_type = if field.nullable { pq::OPTIONAL } else { pq::REQUIRED };
    let def_levels = if field.nullable {
        Some((0..column.len()).map(|i| !null_mask.get(i)).collect::<Vec<bool>>())
    } else {
        None
    };
    let null_count = null_mask.count_ones() as i64;

    let (physical_type, converted_type, logical_type, values, min_max) = match column {
        Column::Bool { values, .. } => {
            let (bytes, stats) = encode_bool(values, &null_mask);
            (pq::BOOLEAN, None, None, bytes, stats)
        }
        Column::Int { values, .. } => {
            let (bytes, stats) = encode_i64(values.as_slice(), &null_mask);
            (pq::INT64, None, None, bytes, stats)
        }
        Column::UInt { values, .. } => {
            let (bytes, stats) = encode_uint64_as_i64(values.as_slice(), &null_mask);
            (pq::INT64, Some(pq::UINT_64), Some(LogicalTypeSpec::UInt64), bytes, stats)
        }
        Column::Float { values, .. } => {
            let (bytes, stats) = encode_f64(values.as_slice(), &null_mask);
            (pq::DOUBLE, None, None, bytes, stats)
        }
        Column::Text { offsets, data, .. } => {
            let (bytes, stats) = encode_bytes(offsets, data, &null_mask);
            (pq::BYTE_ARRAY, Some(pq::UTF8), Some(LogicalTypeSpec::String), bytes, stats)
        }
        Column::Binary { offsets, data, .. } => {
            let (bytes, stats) = encode_bytes(offsets, data, &null_mask);
            (pq::BYTE_ARRAY, None, None, bytes, stats)
        }
        Column::Date { values, .. } => {
            let (bytes, stats) = encode_i32(values.as_slice(), &null_mask);
            (pq::INT32, Some(pq::DATE), Some(LogicalTypeSpec::Date), bytes, stats)
        }
        Column::Timestamp { values, .. } => {
            let (bytes, stats) = encode_i64(values.as_slice(), &null_mask);
            (
                pq::INT64,
                Some(pq::TIMESTAMP_MICROS),
                Some(LogicalTypeSpec::TimestampMicros),
                bytes,
                stats,
            )
        }
        Column::Vector { .. } => {
            return Err(Error::not_supported(format!(
                "Parquet writer: column '{}' is Vector, which needs LIST repetition levels this \
                 writer does not produce",
                field.name
            )))
        }
    };

    Ok(EncodedColumn {
        physical_type,
        converted_type,
        logical_type,
        repetition_type,
        def_levels,
        values,
        null_count,
        min_max,
    })
}

/// The RLE/bit-packed hybrid encoding Parquet uses for definition/repetition levels (a flat
/// schema's max definition level is 1, so this always packs 1-bit values), emitted as a single
/// bit-packed run rather than a general RLE encoder: a reader stops once it has decoded
/// `num_values` levels, so zero-padding the last group of 8 is harmless, and one run is always a
/// legal encoding of any length. Header: `(num_groups << 1) | 1` as an unsigned varint, then the
/// bit-packed bytes themselves, LSB first within each byte.
fn encode_definition_levels(levels: &[bool]) -> Vec<u8> {
    let num_groups = levels.len().div_ceil(8);
    let mut out = Vec::with_capacity(1 + num_groups);
    write_unsigned_varint(&mut out, ((num_groups as u64) << 1) | 1);
    for group in 0..num_groups {
        let mut byte = 0u8;
        for bit in 0..8 {
            let idx = group * 8 + bit;
            if idx < levels.len() && levels[idx] {
                byte |= 1 << bit;
            }
        }
        out.push(byte);
    }
    out
}

/// The data page's uncompressed body: `[4-byte LE length][def levels]` (only when the field is
/// `OPTIONAL`) followed by the `PLAIN`-encoded present values, with no length prefix of their own.
fn build_page_body(encoded: &EncodedColumn) -> Vec<u8> {
    let mut body = Vec::new();
    if let Some(levels) = &encoded.def_levels {
        let level_bytes = encode_definition_levels(levels);
        body.extend_from_slice(&(level_bytes.len() as i32).to_le_bytes());
        body.extend_from_slice(&level_bytes);
    }
    body.extend_from_slice(&encoded.values);
    body
}

// ---- Thrift structure builders (field ids cited from `parquet.thrift` at each write) ----

/// `Statistics`: `null_count`=3, `max_value`=5, `min_value`=6. The deprecated `max`/`min` (1, 2)
/// and `distinct_count` (4) are not written.
fn write_statistics(w: &mut CompactWriter, null_count: i64, min_max: &Option<(Vec<u8>, Vec<u8>)>) {
    let mut last = 0i16;
    w.write_i64_field(&mut last, 3, null_count);
    if let Some((min, max)) = min_max {
        w.write_binary_field(&mut last, 5, max);
        w.write_binary_field(&mut last, 6, min);
    }
    w.write_stop();
}

/// `KeyValue`: `key`=1, `value`=2.
fn write_key_value(w: &mut CompactWriter, key: &str, value: &str) {
    let mut last = 0i16;
    w.write_string_field(&mut last, 1, key);
    w.write_string_field(&mut last, 2, value);
    w.write_stop();
}

/// `StringType`/`DateType`/`MicroSeconds` are all empty structs in `parquet.thrift` — the enclosing
/// union field header (already written by the caller) carries their entire meaning.
fn write_empty_struct(w: &mut CompactWriter) {
    w.write_stop();
}

/// `TimeUnit` union: `MICROS`=2.
fn write_time_unit_micros(w: &mut CompactWriter) {
    let mut last = 0i16;
    w.write_struct_field_header(&mut last, 2);
    write_empty_struct(w); // MicroSeconds{}
    w.write_stop();
}

/// `TimestampType`: `isAdjustedToUTC`=1, `unit`=2.
fn write_timestamp_type(w: &mut CompactWriter) {
    let mut last = 0i16;
    w.write_bool_field(&mut last, 1, true);
    w.write_struct_field_header(&mut last, 2);
    write_time_unit_micros(w);
    w.write_stop();
}

/// `IntType`: `bitWidth`=1 (i8), `isSigned`=2.
fn write_int_type_u64(w: &mut CompactWriter) {
    let mut last = 0i16;
    w.write_i8_field(&mut last, 1, 64);
    w.write_bool_field(&mut last, 2, false);
    w.write_stop();
}

/// `LogicalType` union: `STRING`=1, `DATE`=6, `TIMESTAMP`=8, `INTEGER`=10.
fn write_logical_type(w: &mut CompactWriter, spec: &LogicalTypeSpec) {
    let mut last = 0i16;
    match spec {
        LogicalTypeSpec::String => {
            w.write_struct_field_header(&mut last, 1);
            write_empty_struct(w);
        }
        LogicalTypeSpec::Date => {
            w.write_struct_field_header(&mut last, 6);
            write_empty_struct(w);
        }
        LogicalTypeSpec::TimestampMicros => {
            w.write_struct_field_header(&mut last, 8);
            write_timestamp_type(w);
        }
        LogicalTypeSpec::UInt64 => {
            w.write_struct_field_header(&mut last, 10);
            write_int_type_u64(w);
        }
    }
    w.write_stop();
}

/// `ColumnOrder` union: `TYPE_ORDER`=1, holding an empty `TypeDefinedOrder{}`. Written once per
/// leaf column so `Statistics.min_value`/`max_value` have a well-defined sort order per the
/// `FileMetaData.column_orders` doc comment.
fn write_column_order_type_defined(w: &mut CompactWriter) {
    let mut last = 0i16;
    w.write_struct_field_header(&mut last, 1);
    write_empty_struct(w);
    w.write_stop();
}

/// The schema tree's root `SchemaElement`: no `type_`/`repetition_type` (the root has none),
/// `name`=4, `num_children`=5.
fn write_root_schema_element(w: &mut CompactWriter, num_children: i32) {
    let mut last = 0i16;
    w.write_string_field(&mut last, 4, "schema");
    w.write_i32_field(&mut last, 5, num_children);
    w.write_stop();
}

/// A leaf `SchemaElement`: `type_`=1, `repetition_type`=3, `name`=4, `converted_type`=6,
/// `logicalType`=10.
fn write_leaf_schema_element(w: &mut CompactWriter, encoded: &EncodedColumn, name: &str) {
    let mut last = 0i16;
    w.write_i32_field(&mut last, 1, encoded.physical_type);
    w.write_i32_field(&mut last, 3, encoded.repetition_type);
    w.write_string_field(&mut last, 4, name);
    if let Some(converted) = encoded.converted_type {
        w.write_i32_field(&mut last, 6, converted);
    }
    if let Some(logical) = &encoded.logical_type {
        w.write_struct_field_header(&mut last, 10);
        write_logical_type(w, logical);
    }
    w.write_stop();
}

/// `ColumnMetaData`: `type_`=1, `encodings`=2, `path_in_schema`=3, `codec`=4, `num_values`=5,
/// `total_uncompressed_size`=6, `total_compressed_size`=7, `data_page_offset`=9, `statistics`=12.
#[allow(clippy::too_many_arguments)]
fn write_column_metadata(
    w: &mut CompactWriter,
    encoded: &EncodedColumn,
    name: &str,
    codec: i32,
    num_values: i64,
    total_uncompressed_size: i64,
    total_compressed_size: i64,
    data_page_offset: i64,
) {
    let mut last = 0i16;
    w.write_i32_field(&mut last, 1, encoded.physical_type);
    w.write_list_field_header(&mut last, 2);
    w.write_list_header(TYPE_I32, 1);
    w.write_i32_elem(pq::PLAIN);
    w.write_list_field_header(&mut last, 3);
    w.write_list_header(TYPE_BINARY, 1);
    w.write_string_elem(name);
    w.write_i32_field(&mut last, 4, codec);
    w.write_i64_field(&mut last, 5, num_values);
    w.write_i64_field(&mut last, 6, total_uncompressed_size);
    w.write_i64_field(&mut last, 7, total_compressed_size);
    w.write_i64_field(&mut last, 9, data_page_offset);
    w.write_struct_field_header(&mut last, 12);
    write_statistics(w, encoded.null_count, &encoded.min_max);
    w.write_stop();
}

/// `PageHeader`: `type_`=1, `uncompressed_page_size`=2, `compressed_page_size`=3,
/// `data_page_header`=5. `DataPageHeader`: `num_values`=1, `encoding`=2,
/// `definition_level_encoding`=3, `repetition_level_encoding`=4, `statistics`=5.
fn write_page_header(
    w: &mut CompactWriter,
    uncompressed_size: i32,
    compressed_size: i32,
    num_values: i32,
    null_count: i64,
    min_max: &Option<(Vec<u8>, Vec<u8>)>,
) {
    let mut last = 0i16;
    w.write_i32_field(&mut last, 1, pq::DATA_PAGE);
    w.write_i32_field(&mut last, 2, uncompressed_size);
    w.write_i32_field(&mut last, 3, compressed_size);
    w.write_struct_field_header(&mut last, 5);
    {
        let mut dph_last = 0i16;
        w.write_i32_field(&mut dph_last, 1, num_values);
        w.write_i32_field(&mut dph_last, 2, pq::PLAIN);
        w.write_i32_field(&mut dph_last, 3, pq::RLE);
        w.write_i32_field(&mut dph_last, 4, pq::RLE);
        w.write_struct_field_header(&mut dph_last, 5);
        write_statistics(w, null_count, min_max);
        w.write_stop(); // end DataPageHeader
    }
    w.write_stop(); // end PageHeader
}

struct ColumnLayout {
    encoded: EncodedColumn,
    data_page_offset: i64,
    total_uncompressed_size: i64,
    total_compressed_size: i64,
}

fn write_parquet_with_codec(view: &dyn RecordView, codec: CompressionCodec) -> Result<Vec<u8>, Error> {
    let batch: std::sync::Arc<RecordBatch> = view.materialize()?;
    let schema: &RecordSchema = &batch.schema;

    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(PAR1);

    let mut layouts: Vec<ColumnLayout> = Vec::with_capacity(schema.fields.len());
    for (field, column) in schema.fields.iter().zip(batch.columns.iter()) {
        let encoded = encode_field(field, column)?;
        let body = build_page_body(&encoded);
        let uncompressed_size = body.len();
        let compressed_body = compress(codec, &body)?;
        let compressed_size = compressed_body.len();

        let mut page_writer = CompactWriter::new();
        write_page_header(
            &mut page_writer,
            uncompressed_size as i32,
            compressed_size as i32,
            column.len() as i32,
            encoded.null_count,
            &encoded.min_max,
        );
        let page_header_bytes = page_writer.buf;

        let data_page_offset = out.len() as i64;
        out.extend_from_slice(&page_header_bytes);
        out.extend_from_slice(&compressed_body);

        layouts.push(ColumnLayout {
            total_uncompressed_size: (page_header_bytes.len() + uncompressed_size) as i64,
            total_compressed_size: (page_header_bytes.len() + compressed_size) as i64,
            data_page_offset,
            encoded,
        });
    }

    let schema_json =
        serde_json::to_string(schema).map_err(|e| Error::from_error(ErrorType::SerializationError, e))?;

    let mut w = CompactWriter::new();
    let mut last = 0i16;
    w.write_i32_field(&mut last, 1, 1); // FileMetaData.version
    w.write_list_field_header(&mut last, 2); // FileMetaData.schema
    w.write_list_header(TYPE_STRUCT, 1 + schema.fields.len());
    write_root_schema_element(&mut w, schema.fields.len() as i32);
    for (field, layout) in schema.fields.iter().zip(layouts.iter()) {
        write_leaf_schema_element(&mut w, &layout.encoded, &field.name);
    }
    w.write_i64_field(&mut last, 3, batch.len as i64); // FileMetaData.num_rows

    w.write_list_field_header(&mut last, 4); // FileMetaData.row_groups
    w.write_list_header(TYPE_STRUCT, 1);
    {
        let mut rg_last = 0i16;
        w.write_list_field_header(&mut rg_last, 1); // RowGroup.columns
        w.write_list_header(TYPE_STRUCT, schema.fields.len());
        let mut total_byte_size: i64 = 0;
        for (field, layout) in schema.fields.iter().zip(layouts.iter()) {
            total_byte_size += layout.total_uncompressed_size;
            let mut cc_last = 0i16;
            w.write_i64_field(&mut cc_last, 2, 0); // ColumnChunk.file_offset (deprecated: 0)
            w.write_struct_field_header(&mut cc_last, 3); // ColumnChunk.meta_data
            write_column_metadata(
                &mut w,
                &layout.encoded,
                &field.name,
                codec.wire_value(),
                batch.len as i64,
                layout.total_uncompressed_size,
                layout.total_compressed_size,
                layout.data_page_offset,
            );
            w.write_stop(); // end ColumnChunk
        }
        w.write_i64_field(&mut rg_last, 2, total_byte_size); // RowGroup.total_byte_size
        w.write_i64_field(&mut rg_last, 3, batch.len as i64); // RowGroup.num_rows
        w.write_stop(); // end RowGroup
    }

    w.write_list_field_header(&mut last, 5); // FileMetaData.key_value_metadata
    w.write_list_header(TYPE_STRUCT, 1);
    write_key_value(&mut w, "liquers.schema", &schema_json);

    w.write_string_field(&mut last, 6, "liquers-records"); // FileMetaData.created_by

    w.write_list_field_header(&mut last, 7); // FileMetaData.column_orders
    w.write_list_header(TYPE_STRUCT, schema.fields.len());
    for _ in 0..schema.fields.len() {
        write_column_order_type_defined(&mut w);
    }
    w.write_stop(); // end FileMetaData

    let footer_bytes = w.buf;
    out.extend_from_slice(&footer_bytes);
    out.extend_from_slice(&(footer_bytes.len() as i32).to_le_bytes());
    out.extend_from_slice(PAR1);

    Ok(out)
}

/// Serializes `view`'s materialized rows as a minimal, GZIP-compressed Parquet file. See the
/// module doc comment for the layout and type mapping; `Vector` columns are refused, naming the
/// column.
pub(crate) fn write_parquet(view: &dyn RecordView) -> Result<Vec<u8>, Error> {
    write_parquet_with_codec(view, CompressionCodec::Gzip)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats::{write_table, TableFormat, WriteOptions};
    use crate::schema::FieldType;
    use std::sync::Arc;

    #[test]
    fn float_statistics_write_a_zero_min_as_negative_and_a_zero_max_as_positive() {
        // Parquet's Statistics rule for floating point: a min of ±0 is written -0.0 and a max of
        // ±0 is written +0.0, since -0.0 == +0.0 and a reader cannot trust which one it got.
        let min_bits = |values: &[f64]| {
            let (_, stats) = encode_f64(values, &Bitmap::new(values.len()));
            stats.expect("stats")
        };
        for values in [&[0.0f64][..], &[-0.0], &[0.0, -0.0], &[-0.0, 0.0]] {
            let (min, max) = min_bits(values);
            assert_eq!(min, (-0.0f64).to_le_bytes().to_vec(), "min of {values:?}");
            assert_eq!(max, 0.0f64.to_le_bytes().to_vec(), "max of {values:?}");
        }
        let (min, max) = min_bits(&[-1.0, 0.0]);
        assert_eq!(min, (-1.0f64).to_le_bytes().to_vec());
        assert_eq!(max, 0.0f64.to_le_bytes().to_vec());
        let (min, max) = min_bits(&[-0.0, 2.0]);
        assert_eq!(min, (-0.0f64).to_le_bytes().to_vec());
        assert_eq!(max, 2.0f64.to_le_bytes().to_vec());
    }

    #[test]
    fn parquet_writer_refuses_vector_columns() -> Result<(), Error> {
        let schema = Arc::new(RecordSchema::new(vec![FieldSchema::new("embedding", FieldType::Vector)])?);
        let column = Column::Vector { validity: None, dim: 3, data: Buffer::from_slice(&[0.1f32, 0.2, 0.3]) };
        let batch = RecordBatch::new(schema, vec![column], None, None, vec![])?;
        let err = write_table(&batch, TableFormat::Parquet, &WriteOptions::default())
            .expect_err("Vector needs LIST repetition levels this writer does not produce");
        let message = format!("{err}").to_lowercase();
        assert!(message.contains("vector") || message.contains("list"));
        Ok(())
    }

    #[test]
    fn parquet_write_produces_the_par1_magic_at_both_ends() -> Result<(), Error> {
        let schema = Arc::new(RecordSchema::new(vec![FieldSchema::new("id", FieldType::Int)])?);
        let batch = RecordBatch::new(
            schema,
            vec![Column::Int { validity: None, values: Buffer::from_slice(&[1i64, 2, 3]) }],
            None,
            None,
            vec![],
        )?;
        let bytes = write_table(&batch, TableFormat::Parquet, &WriteOptions::default())?;
        assert_eq!(&bytes[0..4], PAR1);
        assert_eq!(&bytes[bytes.len() - 4..], PAR1);
        Ok(())
    }

    #[test]
    fn uncompressed_and_gzip_both_produce_a_well_formed_file() -> Result<(), Error> {
        let schema = Arc::new(RecordSchema::new(vec![FieldSchema::new("id", FieldType::Int)])?);
        let batch = RecordBatch::new(
            schema,
            vec![Column::Int { validity: None, values: Buffer::from_slice(&[1i64, 2, 3]) }],
            None,
            None,
            vec![],
        )?;
        let uncompressed = write_parquet_with_codec(&batch, CompressionCodec::Uncompressed)?;
        let gzipped = write_parquet_with_codec(&batch, CompressionCodec::Gzip)?;
        assert_eq!(&uncompressed[0..4], PAR1);
        assert_eq!(&gzipped[0..4], PAR1);
        // Not a strict inequality in general, but for this payload gzip's header/trailer overhead
        // makes it larger than three plain 8-byte integers — a cheap sanity check that the two
        // codecs actually produced different bytes.
        assert_ne!(uncompressed, gzipped);
        Ok(())
    }

    #[test]
    fn encode_definition_levels_round_trips_through_manual_bit_unpacking() {
        let levels = vec![true, false, true, true, false, false, true, false, true];
        let encoded = encode_definition_levels(&levels);
        // header: num_groups = ceil(9/8) = 2 -> (2 << 1) | 1 = 5
        assert_eq!(encoded[0], 5);
        // unpack and compare against the input, ignoring padding past levels.len()
        let mut unpacked = Vec::new();
        for &byte in &encoded[1..] {
            for bit in 0..8 {
                unpacked.push((byte >> bit) & 1 == 1);
            }
        }
        assert_eq!(&unpacked[..levels.len()], levels.as_slice());
    }

    #[test]
    fn nullable_field_writes_definition_levels_non_nullable_does_not() -> Result<(), Error> {
        let schema = Arc::new(RecordSchema::new(vec![
            FieldSchema::new("required", FieldType::Int).not_null(),
            FieldSchema::new("optional", FieldType::Int),
        ])?);
        let required = Column::Int { validity: None, values: Buffer::from_slice(&[1i64, 2]) };
        let optional = Column::Int {
            validity: Some(Bitmap::from_bools(&[true, false])),
            values: Buffer::from_slice(&[1i64, 0]),
        };
        let batch = RecordBatch::new(schema.clone(), vec![required, optional], None, None, vec![])?;

        let required_encoded = encode_field(&schema.fields[0], &batch.columns[0])?;
        assert_eq!(required_encoded.repetition_type, pq::REQUIRED);
        assert!(required_encoded.def_levels.is_none());

        let optional_encoded = encode_field(&schema.fields[1], &batch.columns[1])?;
        assert_eq!(optional_encoded.repetition_type, pq::OPTIONAL);
        assert_eq!(optional_encoded.def_levels, Some(vec![true, false]));
        assert_eq!(optional_encoded.null_count, 1);
        Ok(())
    }

    #[test]
    fn statistics_are_absent_when_every_value_is_null() -> Result<(), Error> {
        let schema = Arc::new(RecordSchema::new(vec![FieldSchema::new("v", FieldType::Int)])?);
        let column = Column::Int {
            validity: Some(Bitmap::from_bools(&[false, false])),
            values: Buffer::from_slice(&[0i64, 0]),
        };
        let encoded = encode_field(&schema.fields[0], &column)?;
        assert_eq!(encoded.null_count, 2);
        assert!(encoded.min_max.is_none());
        Ok(())
    }

    #[test]
    fn text_min_max_compare_bytes_lexicographically() -> Result<(), Error> {
        let schema = Arc::new(RecordSchema::new(vec![FieldSchema::new("label", FieldType::Text)])?);
        let column = Column::Text {
            validity: None,
            offsets: Buffer::from_slice(&[0i32, 5, 10, 15]),
            data: AlignedBuffer::from_slice(b"carolalicebobbb"),
        };
        let encoded = encode_field(&schema.fields[0], &column)?;
        let (min, max) = encoded.min_max.expect("stats present");
        assert_eq!(min, b"alice");
        assert_eq!(max, b"carol");
        Ok(())
    }
}
