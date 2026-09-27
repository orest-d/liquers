//! Arrow IPC file format (Feather v2) — behind this crate's `ipc` feature.
//!
//! Writes one [`RecordBatch`] per file ([`write_ipc`]); reads any number of record-batch messages
//! back and concatenates them ([`read_ipc`], via [`RecordBatch::concat`]). The [`RecordSchema`]
//! travels as JSON in the Arrow `Schema`'s `custom_metadata`, under the key `liquers.schema`, so
//! roles, labels and the `Id` field survive a round trip; a batch's `chunk_id` travels the same
//! way, under `liquers.chunk_id`, in the `RecordBatch` message's own `custom_metadata`. See
//! `specs/design/record-streams/phase2-architecture.md` §"Tier 2 — Arrow IPC file (Feather v2)"
//! and §"Where our layout meets Arrow's, and three places it does not line up 1:1" for the design,
//! and its §"Still no claim of full Arrow support" for the accepted scope: reading refuses
//! dictionary encoding, a compressed body, 64-bit-offset (`Large*`) types, and any nesting beyond
//! `FixedSizeList(Float32)`, each with an error naming what was found.
//!
//! **No generated code** (`specs/design/record-streams/phase4-implementation.md`, "Decisions made
//! in this phase" §2): this module builds and reads Arrow's `Schema`, `Field`, `Message`,
//! `RecordBatch` (the message header) and `Footer` flatbuffer tables directly, against the field
//! layouts in Arrow's `Schema.fbs`, `Message.fbs` and `File.fbs`.
//!
//! - **Writing** uses `flatbuffers::FlatBufferBuilder` exactly as that decision names it —
//!   `start_table`/`push_slot`/`push_slot_always`/`end_table`/`create_vector`/`create_string` are
//!   all safe functions, so this half of the module has no `unsafe`.
//! - **Reading** does not use `flatbuffers::Table`: every accessor on it (`Table::new`,
//!   `Table::get`) is an `unsafe fn` — sound only because the caller already knows the vtable's
//!   shape, which is exactly our situation (the schema is fixed at compile time), but CLAUDE.md's
//!   project-wide "no `unsafe`" rule for `liquers-records` (see `buffer.rs`'s module doc comment)
//!   applies regardless. So `mod fb` below is a second, independent decoder for the same on-wire
//!   format: a small, bounds-checked, entirely safe byte-slice reader, used only by [`read_ipc`].
//!
//! Slot offsets are named `<TABLE>_VT_<FIELD>` constants, each citing the table and field name
//! they come from; a voffset is `4 + 2 * declaration_index` (slot 0 of the vtable header is the
//! vtable's own byte length, slot 1 the table's inline size — flatbuffers' *Internals* document).
//! `FieldNode`, `Buffer` and `Block` are flatbuffers *structs*, not tables — fixed-width, inline,
//! with no vtable of their own — so they are written and read as flat `i64` words instead:
//! `FieldNode`/`Buffer` are two eight-byte fields each, and `Block`'s `i32 metaDataLength` is
//! zero-extended into its own eight-byte word (sound because it is always a non-negative byte
//! count, and the target is little-endian throughout — phase2-architecture.md §"Endianness").

use std::sync::Arc;

use flatbuffers::{FlatBufferBuilder, TableFinishedWIPOffset, WIPOffset};
use liquers_core::error::{Error, ErrorType};

use crate::batch::RecordBatch;
use crate::batch::RecordView;
use crate::buffer::{AlignedBuffer, Bitmap, Buffer};
use crate::column::Column;
use crate::formats::ReadSchema;
use crate::schema::{FieldSchema, FieldType, RecordSchema};

type TableOffset = WIPOffset<TableFinishedWIPOffset>;

// ---------------------------------------------------------------------------------------------
// Named vtable slot offsets — `voffset = 4 + 2 * declaration_index` for each table's fields, in
// the order Arrow's `.fbs` sources declare them. Only slots this module actually reads or writes
// get a constant; a field it never touches (e.g. `Schema.features`, `Message.custom_metadata`'s
// sibling `variadicBufferCounts`) is named in a comment instead.
// ---------------------------------------------------------------------------------------------

/// `File.fbs` `Footer`: version(0) schema(1) dictionaries(2) recordBatches(3) custom_metadata(4).
const FOOTER_VT_VERSION: u16 = 4;
const FOOTER_VT_SCHEMA: u16 = 6;
const FOOTER_VT_DICTIONARIES: u16 = 8;
const FOOTER_VT_RECORD_BATCHES: u16 = 10;

/// `Schema.fbs` `Schema`: endianness(0) fields(1) custom_metadata(2) features(3, unused).
const SCHEMA_VT_ENDIANNESS: u16 = 4;
const SCHEMA_VT_FIELDS: u16 = 6;
const SCHEMA_VT_CUSTOM_METADATA: u16 = 8;

/// `Schema.fbs` `Field`: name(0) nullable(1) type_type(2) type_(3) dictionary(4) children(5)
/// custom_metadata(6, unused).
const FIELD_VT_NAME: u16 = 4;
const FIELD_VT_NULLABLE: u16 = 6;
const FIELD_VT_TYPE_TYPE: u16 = 8;
const FIELD_VT_TYPE: u16 = 10;
const FIELD_VT_DICTIONARY: u16 = 12;
const FIELD_VT_CHILDREN: u16 = 14;

/// `Schema.fbs` `Int`: bitWidth(0) is_signed(1).
const INT_VT_BIT_WIDTH: u16 = 4;
const INT_VT_IS_SIGNED: u16 = 6;

/// `Schema.fbs` `FloatingPoint`: precision(0).
const FLOATINGPOINT_VT_PRECISION: u16 = 4;

/// `Schema.fbs` `Date`: unit(0).
const DATE_VT_UNIT: u16 = 4;

/// `Schema.fbs` `Timestamp`: unit(0) timezone(1, unused — our `Timestamp` column carries none).
const TIMESTAMP_VT_UNIT: u16 = 4;

/// `Schema.fbs` `FixedSizeList`: listSize(0).
const FIXEDSIZELIST_VT_LIST_SIZE: u16 = 4;

/// `Schema.fbs` `KeyValue`: key(0) value(1).
const KEYVALUE_VT_KEY: u16 = 4;
const KEYVALUE_VT_VALUE: u16 = 6;

/// `Message.fbs` `Message`: version(0) header_type(1) header(2) bodyLength(3) custom_metadata(4).
const MESSAGE_VT_VERSION: u16 = 4;
const MESSAGE_VT_HEADER_TYPE: u16 = 6;
const MESSAGE_VT_HEADER: u16 = 8;
const MESSAGE_VT_BODY_LENGTH: u16 = 10;
const MESSAGE_VT_CUSTOM_METADATA: u16 = 12;

/// `Message.fbs` `RecordBatch` (the message header — not this crate's own [`RecordBatch`]):
/// length(0) nodes(1) buffers(2) compression(3) variadicBufferCounts(4, unused).
const RB_VT_LENGTH: u16 = 4;
const RB_VT_NODES: u16 = 6;
const RB_VT_BUFFERS: u16 = 8;
const RB_VT_COMPRESSION: u16 = 10;

/// `Message.fbs` `BodyCompression`: codec(0) method(1, unused — we only need to know *that* the
/// body is compressed, and with which codec, in order to refuse it by name).
const BODY_COMPRESSION_VT_CODEC: u16 = 4;

// ---------------------------------------------------------------------------------------------
// Enum / union tag values (`Schema.fbs`, `Message.fbs`).
// ---------------------------------------------------------------------------------------------

const METADATA_VERSION_V5: i16 = 4;
const ENDIANNESS_LITTLE: i16 = 0;

// `Type` union tags (`Schema.fbs`). Only the tags this reader/writer recognizes get a name; every
// other value is named generically in an error message (`type_tag_name`).
const TYPE_NONE: u8 = 0;
const TYPE_INT: u8 = 2;
const TYPE_FLOATING_POINT: u8 = 3;
const TYPE_BINARY: u8 = 4;
const TYPE_UTF8: u8 = 5;
const TYPE_BOOL: u8 = 6;
const TYPE_DATE: u8 = 8;
const TYPE_TIMESTAMP: u8 = 10;
const TYPE_FIXED_SIZE_LIST: u8 = 16;
const TYPE_LARGE_BINARY: u8 = 19;
const TYPE_LARGE_UTF8: u8 = 20;

const PRECISION_HALF: i16 = 0;
const PRECISION_SINGLE: i16 = 1;
const PRECISION_DOUBLE: i16 = 2;

const DATE_UNIT_DAY: i16 = 0;

const TIME_UNIT_MICROSECOND: i16 = 2;

/// `MessageHeader` union tags (`Message.fbs`).
const MESSAGE_HEADER_NONE: u8 = 0;
const MESSAGE_HEADER_SCHEMA: u8 = 1;
const MESSAGE_HEADER_DICTIONARY_BATCH: u8 = 2;
const MESSAGE_HEADER_RECORD_BATCH: u8 = 3;

const COMPRESSION_ZSTD: i8 = 1;

const LIQUERS_SCHEMA_KEY: &str = "liquers.schema";
const LIQUERS_CHUNK_ID_KEY: &str = "liquers.chunk_id";

/// The file's magic string — `"ARROW1"`, per `File.fbs`'s header comment. The file opens with it
/// plus two zero padding bytes (eight in total, keeping the first message 8-byte aligned) and
/// closes with it again, bare, after the footer length.
const ARROW_MAGIC: &[u8; 6] = b"ARROW1";
const CONTINUATION_MARKER: u32 = 0xFFFF_FFFF;

fn round_up_8(n: usize) -> usize {
    (n + 7) & !7
}

/// Names an Arrow `Type` union tag for an error message. Not a match on a Liquers-owned enum
/// (`u8` is Arrow's own wire representation), so an unrecognized value falls through to a
/// catch-all arm rather than needing one per undocumented future tag.
fn type_tag_name(tag: u8) -> String {
    match tag {
        TYPE_NONE => "NONE".to_string(),
        1 => "Null".to_string(),
        TYPE_INT => "Int".to_string(),
        TYPE_FLOATING_POINT => "FloatingPoint".to_string(),
        TYPE_BINARY => "Binary".to_string(),
        TYPE_UTF8 => "Utf8".to_string(),
        TYPE_BOOL => "Bool".to_string(),
        7 => "Decimal".to_string(),
        TYPE_DATE => "Date".to_string(),
        9 => "Time".to_string(),
        TYPE_TIMESTAMP => "Timestamp".to_string(),
        11 => "Interval".to_string(),
        12 => "List".to_string(),
        13 => "Struct".to_string(),
        14 => "Union".to_string(),
        15 => "FixedSizeBinary".to_string(),
        TYPE_FIXED_SIZE_LIST => "FixedSizeList".to_string(),
        17 => "Map".to_string(),
        18 => "Duration".to_string(),
        TYPE_LARGE_BINARY => "LargeBinary".to_string(),
        TYPE_LARGE_UTF8 => "LargeUtf8".to_string(),
        21 => "LargeList".to_string(),
        22 => "RunEndEncoded".to_string(),
        23 => "BinaryView".to_string(),
        24 => "Utf8View".to_string(),
        25 => "ListView".to_string(),
        26 => "LargeListView".to_string(),
        other => format!("unknown type tag {other}"),
    }
}

// =================================================================================================
// Writing
// =================================================================================================

fn write_key_value(builder: &mut FlatBufferBuilder, key: &str, value: &str) -> TableOffset {
    let key_off = builder.create_string(key);
    let value_off = builder.create_string(value);
    let start = builder.start_table();
    builder.push_slot_always::<WIPOffset<&str>>(KEYVALUE_VT_KEY, key_off);
    builder.push_slot_always::<WIPOffset<&str>>(KEYVALUE_VT_VALUE, value_off);
    builder.end_table(start)
}

fn write_int_type(builder: &mut FlatBufferBuilder, bit_width: i32, is_signed: bool) -> TableOffset {
    let start = builder.start_table();
    builder.push_slot::<i32>(INT_VT_BIT_WIDTH, bit_width, 0);
    builder.push_slot::<bool>(INT_VT_IS_SIGNED, is_signed, false);
    builder.end_table(start)
}

fn write_floating_point_type(builder: &mut FlatBufferBuilder, precision: i16) -> TableOffset {
    let start = builder.start_table();
    builder.push_slot::<i16>(FLOATINGPOINT_VT_PRECISION, precision, PRECISION_HALF);
    builder.end_table(start)
}

/// `Utf8`, `Binary` and `Bool` are all empty tables in `Schema.fbs` — no fields, just a marker
/// that the `Type` union's tag byte already carries.
fn write_empty_type(builder: &mut FlatBufferBuilder) -> TableOffset {
    let start = builder.start_table();
    builder.end_table(start)
}

fn write_date_type(builder: &mut FlatBufferBuilder) -> TableOffset {
    let start = builder.start_table();
    builder.push_slot::<i16>(DATE_VT_UNIT, DATE_UNIT_DAY, 1 /* Date.fbs default: Millisecond */);
    builder.end_table(start)
}

fn write_timestamp_type(builder: &mut FlatBufferBuilder) -> TableOffset {
    let start = builder.start_table();
    builder.push_slot::<i16>(TIMESTAMP_VT_UNIT, TIME_UNIT_MICROSECOND, 0 /* default: Second */);
    builder.end_table(start)
}

fn write_fixed_size_list_type(builder: &mut FlatBufferBuilder, list_size: i32) -> TableOffset {
    let start = builder.start_table();
    builder.push_slot::<i32>(FIXEDSIZELIST_VT_LIST_SIZE, list_size, 0);
    builder.end_table(start)
}

/// Builds one Arrow `Field` table for `field`, using `dim` (the column's actual vector width) when
/// `field.data_type` is [`FieldType::Vector`]. See phase2-architecture.md's mapping table: every
/// other `FieldType` is a direct, unnested Arrow type; `Vector` alone gets a `FixedSizeList` node
/// with a single `item: Float32` child, per Arrow's *Fixed-size list layout*.
fn build_field(builder: &mut FlatBufferBuilder, field: &FieldSchema, dim: Option<usize>) -> Result<TableOffset, Error> {
    let name_off = builder.create_string(&field.name);
    let (type_tag, type_off, children_off) = match field.data_type {
        FieldType::Bool => (TYPE_BOOL, write_empty_type(builder), None),
        FieldType::Int => (TYPE_INT, write_int_type(builder, 64, true), None),
        FieldType::UInt => (TYPE_INT, write_int_type(builder, 64, false), None),
        FieldType::Float => (TYPE_FLOATING_POINT, write_floating_point_type(builder, PRECISION_DOUBLE), None),
        FieldType::Text => (TYPE_UTF8, write_empty_type(builder), None),
        FieldType::Binary => (TYPE_BINARY, write_empty_type(builder), None),
        FieldType::Date => (TYPE_DATE, write_date_type(builder), None),
        FieldType::Timestamp => (TYPE_TIMESTAMP, write_timestamp_type(builder), None),
        FieldType::Vector => {
            let dim = dim.ok_or_else(|| {
                Error::general_error(format!(
                    "ipc: field '{}' is declared Vector but its column carries no width",
                    field.name
                ))
            })?;
            let item_name = builder.create_string("item");
            let item_type = write_floating_point_type(builder, PRECISION_SINGLE);
            let item_start = builder.start_table();
            builder.push_slot_always::<WIPOffset<&str>>(FIELD_VT_NAME, item_name);
            builder.push_slot::<u8>(FIELD_VT_TYPE_TYPE, TYPE_FLOATING_POINT, TYPE_NONE);
            builder.push_slot_always::<TableOffset>(FIELD_VT_TYPE, item_type);
            let item_field = builder.end_table(item_start);
            let children_vec = builder.create_vector(&[item_field]);
            (TYPE_FIXED_SIZE_LIST, write_fixed_size_list_type(builder, dim as i32), Some(children_vec))
        }
    };
    let start = builder.start_table();
    builder.push_slot_always::<WIPOffset<&str>>(FIELD_VT_NAME, name_off);
    builder.push_slot::<bool>(FIELD_VT_NULLABLE, field.nullable, false);
    builder.push_slot::<u8>(FIELD_VT_TYPE_TYPE, type_tag, TYPE_NONE);
    builder.push_slot_always::<TableOffset>(FIELD_VT_TYPE, type_off);
    if let Some(children) = children_off {
        builder.push_slot_always(FIELD_VT_CHILDREN, children);
    }
    Ok(builder.end_table(start))
}

/// Builds the Arrow `Schema` table for `schema`/`columns`: one `Field` per schema field (in
/// order), plus `custom_metadata` carrying the whole [`RecordSchema`] as JSON under
/// `liquers.schema` — the lossless path phase2-architecture.md's §"Tier 2" promises.
fn build_schema_table(builder: &mut FlatBufferBuilder, schema: &RecordSchema, columns: &[Column]) -> Result<TableOffset, Error> {
    let mut field_offsets = Vec::with_capacity(schema.fields.len());
    for (field, column) in schema.fields.iter().zip(columns.iter()) {
        if column.data_type() != field.data_type {
            return Err(Error::general_error(format!(
                "ipc: column '{}' is {:?} but the schema declares {:?}",
                field.name,
                column.data_type(),
                field.data_type
            )));
        }
        let dim = match column {
            Column::Vector { dim, .. } => Some(*dim),
            Column::Bool { .. }
            | Column::Int { .. }
            | Column::UInt { .. }
            | Column::Float { .. }
            | Column::Text { .. }
            | Column::Binary { .. }
            | Column::Date { .. }
            | Column::Timestamp { .. } => None,
        };
        field_offsets.push(build_field(builder, field, dim)?);
    }
    let fields_vec = builder.create_vector(&field_offsets);

    let schema_json =
        serde_json::to_string(schema).map_err(|e| Error::from_error(ErrorType::SerializationError, e))?;
    let kv = write_key_value(builder, LIQUERS_SCHEMA_KEY, &schema_json);
    let metadata_vec = builder.create_vector(&[kv]);

    let start = builder.start_table();
    builder.push_slot::<i16>(SCHEMA_VT_ENDIANNESS, ENDIANNESS_LITTLE, ENDIANNESS_LITTLE);
    builder.push_slot_always(SCHEMA_VT_FIELDS, fields_vec);
    builder.push_slot_always(SCHEMA_VT_CUSTOM_METADATA, metadata_vec);
    Ok(builder.end_table(start))
}

/// Writes a flatbuffers vector of fixed-width structs (`FieldNode`, `Buffer`, or `Block`), each
/// given as its declaration-order fields flattened to `i64` words (see the module doc comment for
/// why `Block`'s `i32` field is safe to zero-extend this way). Building it from the `i64`/`u32`
/// pushes `FlatBufferBuilder` already knows how to write avoids implementing `flatbuffers::Push`
/// ourselves — its `push` method is `unsafe fn`, which this crate does not write.
fn write_struct_vector(builder: &mut FlatBufferBuilder, items: &[Vec<i64>]) -> WIPOffset<()> {
    for item in items.iter().rev() {
        for word in item.iter().rev() {
            builder.push::<i64>(*word);
        }
    }
    WIPOffset::new(builder.push::<u32>(items.len() as u32).value())
}

fn validity_null_count(validity: &Option<Bitmap>) -> i64 {
    match validity {
        Some(bitmap) => (bitmap.len() - bitmap.count_ones()) as i64,
        None => 0,
    }
}

/// Appends one buffer's bytes to `body`, padded to Arrow's required 8-byte boundary, and records
/// its `Buffer{offset, length}` (the *unpadded* logical length — [COLUMNAR] *Buffer Alignment and
/// Padding* only requires the padding, not that length include it).
fn push_body_buffer(body: &mut Vec<u8>, buffers: &mut Vec<Vec<i64>>, bytes: &[u8]) {
    let offset = body.len() as i64;
    let length = bytes.len() as i64;
    body.extend_from_slice(bytes);
    let padded = round_up_8(body.len());
    body.resize(padded, 0);
    buffers.push(vec![offset, length]);
}

/// A validity buffer is written whenever the column has one; when it does not (no nulls at all),
/// [COLUMNAR] *Validity bitmaps* permits omitting it, which this writer represents as a zero-length
/// entry rather than skipping the slot — buffer *count* per field is otherwise fixed by its type.
fn push_validity_buffer(body: &mut Vec<u8>, buffers: &mut Vec<Vec<i64>>, validity: &Option<Bitmap>) {
    match validity {
        Some(bitmap) => push_body_buffer(body, buffers, bitmap.as_bytes()),
        None => push_body_buffer(body, buffers, &[]),
    }
}

struct EncodedBody {
    bytes: Vec<u8>,
    nodes: Vec<Vec<i64>>,
    buffers: Vec<Vec<i64>>,
}

/// Flattens `columns` into one Arrow record-batch body: a `FieldNode` and one or more `Buffer`s per
/// field, in the pre-ordered, depth-first order `Schema.fields` implies — recursing once, into the
/// single `Float32` child, for a [`Column::Vector`] field. See phase2-architecture.md's mapping
/// table for which buffers each `Column` variant contributes.
fn encode_body(columns: &[Column]) -> Result<EncodedBody, Error> {
    let mut body = Vec::new();
    let mut nodes: Vec<Vec<i64>> = Vec::new();
    let mut buffers: Vec<Vec<i64>> = Vec::new();

    for column in columns {
        let len = column.len() as i64;
        match column {
            Column::Bool { validity, values } => {
                nodes.push(vec![len, validity_null_count(validity)]);
                push_validity_buffer(&mut body, &mut buffers, validity);
                push_body_buffer(&mut body, &mut buffers, values.as_bytes());
            }
            Column::Int { validity, values } => {
                nodes.push(vec![len, validity_null_count(validity)]);
                push_validity_buffer(&mut body, &mut buffers, validity);
                push_body_buffer(&mut body, &mut buffers, values.as_bytes());
            }
            Column::UInt { validity, values } => {
                nodes.push(vec![len, validity_null_count(validity)]);
                push_validity_buffer(&mut body, &mut buffers, validity);
                push_body_buffer(&mut body, &mut buffers, values.as_bytes());
            }
            Column::Float { validity, values } => {
                nodes.push(vec![len, validity_null_count(validity)]);
                push_validity_buffer(&mut body, &mut buffers, validity);
                push_body_buffer(&mut body, &mut buffers, values.as_bytes());
            }
            Column::Date { validity, values } => {
                nodes.push(vec![len, validity_null_count(validity)]);
                push_validity_buffer(&mut body, &mut buffers, validity);
                push_body_buffer(&mut body, &mut buffers, values.as_bytes());
            }
            Column::Timestamp { validity, values } => {
                nodes.push(vec![len, validity_null_count(validity)]);
                push_validity_buffer(&mut body, &mut buffers, validity);
                push_body_buffer(&mut body, &mut buffers, values.as_bytes());
            }
            Column::Text { validity, offsets, data } | Column::Binary { validity, offsets, data } => {
                nodes.push(vec![len, validity_null_count(validity)]);
                push_validity_buffer(&mut body, &mut buffers, validity);
                push_body_buffer(&mut body, &mut buffers, offsets.as_bytes());
                push_body_buffer(&mut body, &mut buffers, data.as_bytes());
            }
            Column::Vector { validity, dim, data } => {
                nodes.push(vec![len, validity_null_count(validity)]);
                push_validity_buffer(&mut body, &mut buffers, validity);
                // The child Float32 values array: no per-element validity of its own (only the
                // whole vector can be null, at the parent), so null_count is always 0 and the
                // validity buffer is always the zero-length "absent" marker.
                let child_len = len * (*dim as i64);
                nodes.push(vec![child_len, 0]);
                push_body_buffer(&mut body, &mut buffers, &[]);
                push_body_buffer(&mut body, &mut buffers, data.as_bytes());
            }
        }
    }

    Ok(EncodedBody { bytes: body, nodes, buffers })
}

/// Appends one encapsulated message — `0xFFFFFFFF` continuation, the padded metadata size, the
/// (now padded) flatbuffer `message_bytes`, then `body` (padded to 8 bytes in turn) — and returns
/// `(metaDataLength, bodyLength)` for the footer's `Block` entry, per `File.fbs` `Block`'s doc
/// comment: "index to the start of the record block ... this is past the Message header", i.e.
/// `bodyPosition = Block.offset + Block.metaDataLength`.
fn append_encapsulated_message(out: &mut Vec<u8>, message_bytes: &[u8], body: &[u8]) -> (i32, i64) {
    let padded_meta = round_up_8(message_bytes.len());
    out.extend_from_slice(&CONTINUATION_MARKER.to_le_bytes());
    out.extend_from_slice(&(padded_meta as i32).to_le_bytes());
    out.extend_from_slice(message_bytes);
    out.resize(out.len() + (padded_meta - message_bytes.len()), 0);
    let meta_data_length = 8 + padded_meta as i32;

    out.extend_from_slice(body);
    let padded_body = round_up_8(body.len());
    out.resize(out.len() + (padded_body - body.len()), 0);
    (meta_data_length, padded_body as i64)
}

/// Serializes `view`'s materialized rows as an Arrow IPC file (Feather v2): the magic, a `Schema`
/// message, one `RecordBatch` message, and a `Footer` indexing it. See the module doc comment and
/// phase2-architecture.md §"Tier 2" for the format and its accepted subset.
pub(crate) fn write_ipc(view: &dyn RecordView) -> Result<Vec<u8>, Error> {
    let batch = view.materialize()?;
    let schema: &RecordSchema = &batch.schema;

    let mut builder = FlatBufferBuilder::new();

    // ---- Schema message (first in the file) ----
    let schema_table = build_schema_table(&mut builder, schema, &batch.columns)?;
    let schema_message = {
        let start = builder.start_table();
        builder.push_slot::<i16>(MESSAGE_VT_VERSION, METADATA_VERSION_V5, 0);
        builder.push_slot::<u8>(MESSAGE_VT_HEADER_TYPE, MESSAGE_HEADER_SCHEMA, MESSAGE_HEADER_NONE);
        builder.push_slot_always::<TableOffset>(MESSAGE_VT_HEADER, schema_table);
        builder.push_slot::<i64>(MESSAGE_VT_BODY_LENGTH, 0, 0);
        builder.end_table(start)
    };
    builder.finish(schema_message, None);
    let schema_message_bytes = builder.finished_data().to_vec();
    builder.reset();

    let mut out = Vec::new();
    out.extend_from_slice(ARROW_MAGIC);
    out.extend_from_slice(&[0u8, 0u8]);
    append_encapsulated_message(&mut out, &schema_message_bytes, &[]);

    // ---- One RecordBatch message ----
    let EncodedBody { bytes: body, nodes, buffers } = encode_body(&batch.columns)?;
    let nodes_vec = write_struct_vector(&mut builder, &nodes);
    let buffers_vec = write_struct_vector(&mut builder, &buffers);
    let record_batch_header = {
        let start = builder.start_table();
        builder.push_slot::<i64>(RB_VT_LENGTH, batch.len as i64, 0);
        builder.push_slot_always(RB_VT_NODES, nodes_vec);
        builder.push_slot_always(RB_VT_BUFFERS, buffers_vec);
        builder.end_table(start)
    };
    let chunk_metadata_vec = match &batch.chunk_id {
        Some(chunk_id) => {
            let json = serde_json::to_string(chunk_id)
                .map_err(|e| Error::from_error(ErrorType::SerializationError, e))?;
            let kv = write_key_value(&mut builder, LIQUERS_CHUNK_ID_KEY, &json);
            Some(builder.create_vector(&[kv]))
        }
        None => None,
    };
    let rb_message = {
        let start = builder.start_table();
        builder.push_slot::<i16>(MESSAGE_VT_VERSION, METADATA_VERSION_V5, 0);
        builder.push_slot::<u8>(MESSAGE_VT_HEADER_TYPE, MESSAGE_HEADER_RECORD_BATCH, MESSAGE_HEADER_NONE);
        builder.push_slot_always::<TableOffset>(MESSAGE_VT_HEADER, record_batch_header);
        builder.push_slot::<i64>(MESSAGE_VT_BODY_LENGTH, body.len() as i64, 0);
        if let Some(metadata) = chunk_metadata_vec {
            builder.push_slot_always(MESSAGE_VT_CUSTOM_METADATA, metadata);
        }
        builder.end_table(start)
    };
    builder.finish(rb_message, None);
    let rb_message_bytes = builder.finished_data().to_vec();
    builder.reset();

    let block_offset = out.len() as i64;
    let (meta_data_length, body_length) = append_encapsulated_message(&mut out, &rb_message_bytes, &body);

    // ---- Footer ----
    let footer_schema_table = build_schema_table(&mut builder, schema, &batch.columns)?;
    let block_words = vec![block_offset, meta_data_length as i64, body_length];
    let blocks_vec = write_struct_vector(&mut builder, &[block_words]);
    let footer = {
        let start = builder.start_table();
        builder.push_slot::<i16>(FOOTER_VT_VERSION, METADATA_VERSION_V5, 0);
        builder.push_slot_always(FOOTER_VT_SCHEMA, footer_schema_table);
        builder.push_slot_always(FOOTER_VT_RECORD_BATCHES, blocks_vec);
        builder.end_table(start)
    };
    builder.finish(footer, None);
    let footer_bytes = builder.finished_data();
    out.extend_from_slice(footer_bytes);
    out.extend_from_slice(&(footer_bytes.len() as i32).to_le_bytes());
    out.extend_from_slice(ARROW_MAGIC);

    Ok(out)
}

// =================================================================================================
// A safe, hand-rolled flatbuffer reader (no `flatbuffers::Table`; see the module doc comment)
// =================================================================================================

mod fb {
    use liquers_core::error::{Error, ErrorType};

    fn truncated(what: &str) -> Error {
        Error::general_error(format!("ipc: truncated or malformed flatbuffer ({what})"))
    }

    fn slice(buf: &[u8], pos: usize, len: usize) -> Result<&[u8], Error> {
        buf.get(pos..pos + len).ok_or_else(|| truncated("field runs past the buffer"))
    }

    pub(super) fn read_u8(buf: &[u8], pos: usize) -> Result<u8, Error> {
        Ok(slice(buf, pos, 1)?[0])
    }

    fn read_scalar<T: bytemuck::Pod>(buf: &[u8], pos: usize) -> Result<T, Error> {
        let bytes = slice(buf, pos, std::mem::size_of::<T>())?;
        bytemuck::try_pod_read_unaligned(bytes).map_err(|e| Error::from_error(ErrorType::ConversionError, e))
    }

    pub(super) fn read_i16(buf: &[u8], pos: usize) -> Result<i16, Error> {
        read_scalar(buf, pos)
    }
    pub(super) fn read_u16(buf: &[u8], pos: usize) -> Result<u16, Error> {
        read_scalar(buf, pos)
    }
    pub(super) fn read_i32(buf: &[u8], pos: usize) -> Result<i32, Error> {
        read_scalar(buf, pos)
    }
    pub(super) fn read_u32(buf: &[u8], pos: usize) -> Result<u32, Error> {
        read_scalar(buf, pos)
    }
    pub(super) fn read_i64(buf: &[u8], pos: usize) -> Result<i64, Error> {
        read_scalar(buf, pos)
    }

    /// A view onto one flatbuffer table: not `flatbuffers::Table` (see the module doc comment) —
    /// a from-scratch, bounds-checked reader over the same wire format.
    #[derive(Clone, Copy)]
    pub(super) struct FbTable<'a> {
        buf: &'a [u8],
        pos: usize,
    }

    impl<'a> FbTable<'a> {
        /// The table whose root offset (a `u32`, forward-relative to `offset`) is stored at
        /// `offset` — what `FlatBufferBuilder::finish` writes at the start of its output, and
        /// what every `ForwardsUOffset` in the format points through.
        pub(super) fn root(buf: &'a [u8], offset: usize) -> Result<FbTable<'a>, Error> {
            let rel = read_u32(buf, offset)? as usize;
            let pos = offset.checked_add(rel).ok_or_else(|| truncated("root offset overflow"))?;
            Ok(FbTable { buf, pos })
        }

        /// `vtable_pos = table_pos - soffset`, where `soffset` (signed) is stored at `table_pos` —
        /// the standard flatbuffers table-to-vtable indirection.
        fn vtable_pos(&self) -> Result<usize, Error> {
            let soffset = read_i32(self.buf, self.pos)?;
            let pos = self.pos as i64 - soffset as i64;
            usize::try_from(pos).map_err(|_| truncated("vtable position is negative"))
        }

        /// The absolute location of slot `voffset`'s stored value, or `None` when this table's
        /// vtable does not reach that far (the field was never written, or this table predates a
        /// schema that added it — both mean "absent", handled the same way as a zero entry).
        fn field_loc(&self, voffset: u16) -> Result<Option<usize>, Error> {
            let vtable_pos = self.vtable_pos()?;
            let vtable_len = read_u16(self.buf, vtable_pos)? as usize;
            let voffset = voffset as usize;
            if voffset + 2 > vtable_len {
                return Ok(None);
            }
            let field_rel = read_u16(self.buf, vtable_pos + voffset)? as usize;
            if field_rel == 0 {
                return Ok(None);
            }
            Ok(Some(self.pos + field_rel))
        }

        /// Dereferences the `u32` forward-relative offset stored at `loc` (how every
        /// table/string/vector reference is stored, once its slot's value location is known).
        fn indirect(&self, loc: usize) -> Result<usize, Error> {
            let rel = read_u32(self.buf, loc)? as usize;
            Ok(loc + rel)
        }

        pub(super) fn i8_field(&self, voffset: u16, default: i8) -> Result<i8, Error> {
            match self.field_loc(voffset)? {
                Some(loc) => Ok(read_u8(self.buf, loc)? as i8),
                None => Ok(default),
            }
        }
        pub(super) fn u8_field(&self, voffset: u16, default: u8) -> Result<u8, Error> {
            match self.field_loc(voffset)? {
                Some(loc) => read_u8(self.buf, loc),
                None => Ok(default),
            }
        }
        pub(super) fn i16_field(&self, voffset: u16, default: i16) -> Result<i16, Error> {
            match self.field_loc(voffset)? {
                Some(loc) => read_i16(self.buf, loc),
                None => Ok(default),
            }
        }
        pub(super) fn i32_field(&self, voffset: u16, default: i32) -> Result<i32, Error> {
            match self.field_loc(voffset)? {
                Some(loc) => read_i32(self.buf, loc),
                None => Ok(default),
            }
        }
        pub(super) fn bool_field(&self, voffset: u16, default: bool) -> Result<bool, Error> {
            match self.field_loc(voffset)? {
                Some(loc) => Ok(read_u8(self.buf, loc)? != 0),
                None => Ok(default),
            }
        }

        pub(super) fn table_field(&self, voffset: u16) -> Result<Option<FbTable<'a>>, Error> {
            match self.field_loc(voffset)? {
                Some(loc) => Ok(Some(FbTable { buf: self.buf, pos: self.indirect(loc)? })),
                None => Ok(None),
            }
        }

        pub(super) fn string_field(&self, voffset: u16) -> Result<Option<&'a str>, Error> {
            match self.field_loc(voffset)? {
                None => Ok(None),
                Some(loc) => {
                    let str_pos = self.indirect(loc)?;
                    let len = read_u32(self.buf, str_pos)? as usize;
                    let bytes = slice(self.buf, str_pos + 4, len)?;
                    std::str::from_utf8(bytes)
                        .map(Some)
                        .map_err(|e| Error::from_error(ErrorType::ConversionError, e))
                }
            }
        }

        /// A vector-of-tables (or vector-of-strings-as-tables) field: `Schema.fields`,
        /// `Field.children`, and every `custom_metadata` (a vector of `KeyValue`).
        pub(super) fn table_vector_field(&self, voffset: u16) -> Result<Vec<FbTable<'a>>, Error> {
            match self.field_loc(voffset)? {
                None => Ok(Vec::new()),
                Some(loc) => {
                    let vec_pos = self.indirect(loc)?;
                    let count = read_u32(self.buf, vec_pos)? as usize;
                    let mut out = Vec::with_capacity(count);
                    for i in 0..count {
                        let elem_loc = vec_pos + 4 + i * 4;
                        let target = self.indirect(elem_loc)?;
                        out.push(FbTable { buf: self.buf, pos: target });
                    }
                    Ok(out)
                }
            }
        }

        /// A vector of 16-byte, two-`i64`-word structs — `FieldNode` or `Buffer`.
        pub(super) fn struct2_vector_field(&self, voffset: u16) -> Result<Vec<(i64, i64)>, Error> {
            match self.field_loc(voffset)? {
                None => Ok(Vec::new()),
                Some(loc) => {
                    let vec_pos = self.indirect(loc)?;
                    let count = read_u32(self.buf, vec_pos)? as usize;
                    let data_pos = vec_pos + 4;
                    let mut out = Vec::with_capacity(count);
                    for i in 0..count {
                        let base = data_pos + i * 16;
                        out.push((read_i64(self.buf, base)?, read_i64(self.buf, base + 8)?));
                    }
                    Ok(out)
                }
            }
        }

        /// A vector of 24-byte `Block` structs: `(offset, metaDataLength, bodyLength)`.
        pub(super) fn block_vector_field(&self, voffset: u16) -> Result<Vec<(i64, i32, i64)>, Error> {
            match self.field_loc(voffset)? {
                None => Ok(Vec::new()),
                Some(loc) => {
                    let vec_pos = self.indirect(loc)?;
                    let count = read_u32(self.buf, vec_pos)? as usize;
                    let data_pos = vec_pos + 4;
                    let mut out = Vec::with_capacity(count);
                    for i in 0..count {
                        let base = data_pos + i * 24;
                        let offset = read_i64(self.buf, base)?;
                        let meta_data_length = read_i32(self.buf, base + 8)?;
                        let body_length = read_i64(self.buf, base + 16)?;
                        out.push((offset, meta_data_length, body_length));
                    }
                    Ok(out)
                }
            }
        }
    }
}

// =================================================================================================
// Reading
// =================================================================================================

/// One Arrow `Field`, decoded to what this reader needs: its logical [`FieldType`], nullability,
/// and — only for `FixedSizeList(Float32)` — its width.
#[derive(Debug)]
struct ArrowField<'a> {
    name: &'a str,
    nullable: bool,
    data_type: FieldType,
    dim: Option<usize>,
}

fn refuse_dictionary(field_table: &fb::FbTable<'_>, name: &str) -> Result<(), Error> {
    if field_table.table_field(FIELD_VT_DICTIONARY)?.is_some() {
        return Err(Error::not_supported(format!(
            "Arrow IPC: field '{name}' is dictionary-encoded, which this reader refuses"
        )));
    }
    Ok(())
}

fn decode_arrow_field<'a>(field_table: &fb::FbTable<'a>) -> Result<ArrowField<'a>, Error> {
    let name = field_table
        .string_field(FIELD_VT_NAME)?
        .ok_or_else(|| Error::general_error("ipc: a Field has no name".to_string()))?;
    refuse_dictionary(field_table, name)?;
    let nullable = field_table.bool_field(FIELD_VT_NULLABLE, false)?;
    let type_tag = field_table.u8_field(FIELD_VT_TYPE_TYPE, TYPE_NONE)?;
    let type_table = field_table
        .table_field(FIELD_VT_TYPE)?
        .ok_or_else(|| Error::general_error(format!("ipc: field '{name}' has no type table")))?;

    let (data_type, dim) = match type_tag {
        TYPE_INT => {
            let bit_width = type_table.i32_field(INT_VT_BIT_WIDTH, 0)?;
            if bit_width != 64 {
                return Err(Error::not_supported(format!(
                    "Arrow IPC: field '{name}' is Int(bitWidth={bit_width}); only 64-bit integers are supported"
                )));
            }
            let is_signed = type_table.bool_field(INT_VT_IS_SIGNED, false)?;
            (if is_signed { FieldType::Int } else { FieldType::UInt }, None)
        }
        TYPE_FLOATING_POINT => {
            let precision = type_table.i16_field(FLOATINGPOINT_VT_PRECISION, PRECISION_HALF)?;
            if precision != PRECISION_DOUBLE {
                return Err(Error::not_supported(format!(
                    "Arrow IPC: field '{name}' is FloatingPoint(precision={precision}); only Double (Float64) is supported"
                )));
            }
            (FieldType::Float, None)
        }
        TYPE_UTF8 => (FieldType::Text, None),
        TYPE_BINARY => (FieldType::Binary, None),
        TYPE_BOOL => (FieldType::Bool, None),
        TYPE_DATE => {
            let unit = type_table.i16_field(DATE_VT_UNIT, 1)?;
            if unit != DATE_UNIT_DAY {
                return Err(Error::not_supported(format!(
                    "Arrow IPC: field '{name}' is Date(unit={unit}); only DateUnit::Day (Date32) is supported"
                )));
            }
            (FieldType::Date, None)
        }
        TYPE_TIMESTAMP => {
            let unit = type_table.i16_field(TIMESTAMP_VT_UNIT, 0)?;
            if unit != TIME_UNIT_MICROSECOND {
                return Err(Error::not_supported(format!(
                    "Arrow IPC: field '{name}' is Timestamp(unit={unit}); only TimeUnit::Microsecond is supported"
                )));
            }
            (FieldType::Timestamp, None)
        }
        TYPE_FIXED_SIZE_LIST => {
            let list_size = type_table.i32_field(FIXEDSIZELIST_VT_LIST_SIZE, 0)?;
            let children = field_table.table_vector_field(FIELD_VT_CHILDREN)?;
            if children.len() != 1 {
                return Err(Error::not_supported(format!(
                    "Arrow IPC: field '{name}' is FixedSizeList with {} children; only a single Float32 child is supported",
                    children.len()
                )));
            }
            let child = &children[0];
            refuse_dictionary(child, name)?;
            let child_tag = child.u8_field(FIELD_VT_TYPE_TYPE, TYPE_NONE)?;
            if child_tag != TYPE_FLOATING_POINT {
                return Err(Error::not_supported(format!(
                    "Arrow IPC: field '{name}' is FixedSizeList<{}>; only FixedSizeList<Float32> is supported",
                    type_tag_name(child_tag)
                )));
            }
            let child_type = child
                .table_field(FIELD_VT_TYPE)?
                .ok_or_else(|| Error::general_error(format!("ipc: field '{name}''s item field has no type table")))?;
            let child_precision = child_type.i16_field(FLOATINGPOINT_VT_PRECISION, PRECISION_HALF)?;
            if child_precision != PRECISION_SINGLE {
                return Err(Error::not_supported(format!(
                    "Arrow IPC: field '{name}' is FixedSizeList<FloatingPoint(precision={child_precision})>; only FixedSizeList<Float32> is supported"
                )));
            }
            (FieldType::Vector, Some(list_size.max(0) as usize))
        }
        TYPE_LARGE_UTF8 => {
            return Err(Error::not_supported(format!(
                "Arrow IPC: field '{name}' is LargeUtf8 (64-bit offsets), which this reader refuses"
            )))
        }
        TYPE_LARGE_BINARY => {
            return Err(Error::not_supported(format!(
                "Arrow IPC: field '{name}' is LargeBinary (64-bit offsets), which this reader refuses"
            )))
        }
        other => {
            return Err(Error::not_supported(format!(
                "Arrow IPC: field '{name}' has unsupported Arrow type {} (tag {other})",
                type_tag_name(other)
            )))
        }
    };
    Ok(ArrowField { name, nullable, data_type, dim })
}

struct DecodedSchema<'a> {
    arrow_fields: Vec<ArrowField<'a>>,
    liquers_schema_json: Option<&'a str>,
}

fn decode_schema<'a>(schema_table: &fb::FbTable<'a>) -> Result<DecodedSchema<'a>, Error> {
    let field_tables = schema_table.table_vector_field(SCHEMA_VT_FIELDS)?;
    let mut arrow_fields = Vec::with_capacity(field_tables.len());
    for field_table in &field_tables {
        arrow_fields.push(decode_arrow_field(field_table)?);
    }
    let metadata = schema_table.table_vector_field(SCHEMA_VT_CUSTOM_METADATA)?;
    let mut liquers_schema_json = None;
    for kv in &metadata {
        if kv.string_field(KEYVALUE_VT_KEY)? == Some(LIQUERS_SCHEMA_KEY) {
            liquers_schema_json = kv.string_field(KEYVALUE_VT_VALUE)?;
        }
    }
    Ok(DecodedSchema { arrow_fields, liquers_schema_json })
}

/// The [`RecordSchema`] a read uses: the declared one (checked for field-count and type agreement)
/// when `requested` names one, else `liquers.schema` from the file's metadata when present, else
/// one derived straight from the Arrow fields — phase2-architecture.md's "reading without
/// `liquers.schema` derives a schema from the Arrow fields".
fn effective_schema(decoded: &DecodedSchema<'_>, requested: ReadSchema<'_>) -> Result<RecordSchema, Error> {
    match requested {
        ReadSchema::Declared(declared) => {
            if declared.fields.len() != decoded.arrow_fields.len() {
                return Err(Error::general_error(format!(
                    "ipc: declared schema has {} fields but the file has {}",
                    declared.fields.len(),
                    decoded.arrow_fields.len()
                )));
            }
            for (declared_field, arrow_field) in declared.fields.iter().zip(decoded.arrow_fields.iter()) {
                if declared_field.data_type != arrow_field.data_type {
                    return Err(Error::general_error(format!(
                        "ipc: declared field '{}' is {:?} but the file's field '{}' is {:?}",
                        declared_field.name, declared_field.data_type, arrow_field.name, arrow_field.data_type
                    )));
                }
            }
            Ok(declared.clone())
        }
        ReadSchema::Infer => match decoded.liquers_schema_json {
            Some(json) => {
                let schema: RecordSchema =
                    serde_json::from_str(json).map_err(|e| Error::from_error(ErrorType::ConversionError, e))?;
                if schema.fields.len() != decoded.arrow_fields.len() {
                    return Err(Error::general_error(
                        "ipc: the embedded liquers.schema field count does not match the Arrow schema".to_string(),
                    ));
                }
                Ok(schema)
            }
            None => {
                let fields = decoded
                    .arrow_fields
                    .iter()
                    .map(|f| {
                        let field = FieldSchema::new(f.name, f.data_type);
                        if f.nullable {
                            field
                        } else {
                            field.not_null()
                        }
                    })
                    .collect();
                RecordSchema::new(fields)
            }
        },
    }
}

fn next_node(nodes: &[(i64, i64)], idx: &mut usize) -> Result<(i64, i64), Error> {
    let node = *nodes
        .get(*idx)
        .ok_or_else(|| Error::general_error("ipc: not enough field nodes for the schema".to_string()))?;
    *idx += 1;
    Ok(node)
}

fn next_buffer(buffers: &[(i64, i64)], idx: &mut usize) -> Result<(i64, i64), Error> {
    let buf = *buffers
        .get(*idx)
        .ok_or_else(|| Error::general_error("ipc: not enough buffers for the schema".to_string()))?;
    *idx += 1;
    Ok(buf)
}

fn read_buffer_bytes<'a>(body: &'a [u8], buf: (i64, i64)) -> Result<&'a [u8], Error> {
    let (offset, length) = buf;
    let offset = usize::try_from(offset).map_err(|e| Error::from_error(ErrorType::ConversionError, e))?;
    let length = usize::try_from(length).map_err(|e| Error::from_error(ErrorType::ConversionError, e))?;
    body.get(offset..offset + length)
        .ok_or_else(|| Error::general_error("ipc: a buffer runs past the record batch body".to_string()))
}

fn read_validity(body: &[u8], buf: (i64, i64), len: usize) -> Result<Option<Bitmap>, Error> {
    let bytes = read_buffer_bytes(body, buf)?;
    if bytes.is_empty() {
        Ok(None)
    } else if bytes.len() < len.div_ceil(8) {
        Err(Error::general_error(
            "ipc: validity buffer is shorter than the row count needs".to_string(),
        ))
    } else {
        Ok(Some(Bitmap::from_bytes(bytes, len)))
    }
}

fn read_scalar_buffer<T: bytemuck::Pod>(bytes: &[u8]) -> Result<Buffer<T>, Error> {
    let width = std::mem::size_of::<T>();
    if bytes.len() % width != 0 {
        return Err(Error::general_error(format!(
            "ipc: a buffer of {} bytes is not a multiple of {width}",
            bytes.len()
        )));
    }
    let mut values = Vec::with_capacity(bytes.len() / width);
    for chunk in bytes.chunks_exact(width) {
        values.push(
            bytemuck::try_pod_read_unaligned::<T>(chunk)
                .map_err(|e| Error::from_error(ErrorType::ConversionError, e))?,
        );
    }
    Ok(Buffer::from_slice(&values))
}

/// Reads one `RecordBatch` message (a `File.fbs` `Block`'s worth) into a [`RecordBatch`], refusing
/// a dictionary batch, a compressed body, or a node/buffer count that does not match `schema`.
fn read_one_record_batch(
    bytes: &[u8],
    block_offset: i64,
    meta_data_length: i32,
    body_length: i64,
    schema: &Arc<RecordSchema>,
    arrow_fields: &[ArrowField<'_>],
) -> Result<RecordBatch, Error> {
    let block_offset = usize::try_from(block_offset).map_err(|e| Error::from_error(ErrorType::ConversionError, e))?;
    let meta_data_length =
        usize::try_from(meta_data_length).map_err(|e| Error::from_error(ErrorType::ConversionError, e))?;
    let body_length = usize::try_from(body_length).map_err(|e| Error::from_error(ErrorType::ConversionError, e))?;

    let marker = fb::read_u32(bytes, block_offset)?;
    let metadata_start = if marker == CONTINUATION_MARKER { block_offset + 8 } else { block_offset + 4 };
    let message = fb::FbTable::root(bytes, metadata_start)?;

    let header_type = message.u8_field(MESSAGE_VT_HEADER_TYPE, MESSAGE_HEADER_NONE)?;
    if header_type == MESSAGE_HEADER_DICTIONARY_BATCH {
        return Err(Error::not_supported(
            "Arrow IPC: a dictionary batch message, which this reader refuses".to_string(),
        ));
    }
    if header_type != MESSAGE_HEADER_RECORD_BATCH {
        return Err(Error::general_error(format!(
            "ipc: expected a RecordBatch message, found MessageHeader tag {header_type}"
        )));
    }
    let rb_header = message
        .table_field(MESSAGE_VT_HEADER)?
        .ok_or_else(|| Error::general_error("ipc: RecordBatch message has no header".to_string()))?;

    if let Some(compression) = rb_header.table_field(RB_VT_COMPRESSION)? {
        let codec = compression.i8_field(BODY_COMPRESSION_VT_CODEC, 0)?;
        let name = if codec == COMPRESSION_ZSTD { "ZSTD" } else { "LZ4_FRAME" };
        return Err(Error::not_supported(format!(
            "Arrow IPC: record batch body is compressed ({name}), which this reader refuses"
        )));
    }

    let nodes = rb_header.struct2_vector_field(RB_VT_NODES)?;
    let buffers = rb_header.struct2_vector_field(RB_VT_BUFFERS)?;

    let body_start = block_offset + meta_data_length;
    let body = bytes
        .get(body_start..body_start + body_length)
        .ok_or_else(|| Error::general_error("ipc: record batch body runs past the file".to_string()))?;

    let mut node_idx = 0usize;
    let mut buf_idx = 0usize;
    let mut columns = Vec::with_capacity(schema.fields.len());
    for (field, arrow_field) in schema.fields.iter().zip(arrow_fields.iter()) {
        let (node_len, _null_count) = next_node(&nodes, &mut node_idx)?;
        let validity_buf = next_buffer(&buffers, &mut buf_idx)?;
        let validity = read_validity(body, validity_buf, node_len as usize)?;
        let column = match field.data_type {
            FieldType::Bool => {
                let values_buf = next_buffer(&buffers, &mut buf_idx)?;
                let values_bytes = read_buffer_bytes(body, values_buf)?;
                Column::Bool { validity, values: Bitmap::from_bytes(values_bytes, node_len as usize) }
            }
            FieldType::Int => {
                let values_buf = next_buffer(&buffers, &mut buf_idx)?;
                let values_bytes = read_buffer_bytes(body, values_buf)?;
                Column::Int { validity, values: read_scalar_buffer::<i64>(values_bytes)? }
            }
            FieldType::UInt => {
                let values_buf = next_buffer(&buffers, &mut buf_idx)?;
                let values_bytes = read_buffer_bytes(body, values_buf)?;
                Column::UInt { validity, values: read_scalar_buffer::<u64>(values_bytes)? }
            }
            FieldType::Float => {
                let values_buf = next_buffer(&buffers, &mut buf_idx)?;
                let values_bytes = read_buffer_bytes(body, values_buf)?;
                Column::Float { validity, values: read_scalar_buffer::<f64>(values_bytes)? }
            }
            FieldType::Date => {
                let values_buf = next_buffer(&buffers, &mut buf_idx)?;
                let values_bytes = read_buffer_bytes(body, values_buf)?;
                Column::Date { validity, values: read_scalar_buffer::<i32>(values_bytes)? }
            }
            FieldType::Timestamp => {
                let values_buf = next_buffer(&buffers, &mut buf_idx)?;
                let values_bytes = read_buffer_bytes(body, values_buf)?;
                Column::Timestamp { validity, values: read_scalar_buffer::<i64>(values_bytes)? }
            }
            FieldType::Text => {
                let offsets_buf = next_buffer(&buffers, &mut buf_idx)?;
                let data_buf = next_buffer(&buffers, &mut buf_idx)?;
                let offsets_bytes = read_buffer_bytes(body, offsets_buf)?;
                let data_bytes = read_buffer_bytes(body, data_buf)?;
                Column::Text {
                    validity,
                    offsets: read_scalar_buffer::<i32>(offsets_bytes)?,
                    data: AlignedBuffer::from_slice(data_bytes),
                }
            }
            FieldType::Binary => {
                let offsets_buf = next_buffer(&buffers, &mut buf_idx)?;
                let data_buf = next_buffer(&buffers, &mut buf_idx)?;
                let offsets_bytes = read_buffer_bytes(body, offsets_buf)?;
                let data_bytes = read_buffer_bytes(body, data_buf)?;
                Column::Binary {
                    validity,
                    offsets: read_scalar_buffer::<i32>(offsets_bytes)?,
                    data: AlignedBuffer::from_slice(data_bytes),
                }
            }
            FieldType::Vector => {
                let dim = arrow_field.dim.ok_or_else(|| {
                    Error::general_error(format!(
                        "ipc: field '{}' is Vector but the file's FixedSizeList has no recorded width",
                        field.name
                    ))
                })?;
                let (_child_len, _child_null_count) = next_node(&nodes, &mut node_idx)?;
                // The child's own validity buffer is always the "absent" zero-length marker (see
                // `encode_body`); no per-element nulls are modeled, so it is read and discarded.
                let _child_validity_buf = next_buffer(&buffers, &mut buf_idx)?;
                let child_data_buf = next_buffer(&buffers, &mut buf_idx)?;
                let data_bytes = read_buffer_bytes(body, child_data_buf)?;
                Column::Vector { validity, dim, data: read_scalar_buffer::<f32>(data_bytes)? }
            }
        };
        columns.push(column);
    }

    let chunk_id = {
        let metadata = message.table_vector_field(MESSAGE_VT_CUSTOM_METADATA)?;
        let mut chunk_id = None;
        for kv in &metadata {
            if kv.string_field(KEYVALUE_VT_KEY)? == Some(LIQUERS_CHUNK_ID_KEY) {
                if let Some(json) = kv.string_field(KEYVALUE_VT_VALUE)? {
                    chunk_id = Some(
                        serde_json::from_str(json).map_err(|e| Error::from_error(ErrorType::ConversionError, e))?,
                    );
                }
            }
        }
        chunk_id
    };

    RecordBatch::new(schema.clone(), columns, chunk_id, None, Vec::new())
}

/// Reads an Arrow IPC file (Feather v2) written by [`write_ipc`] or another Arrow implementation,
/// within the subset the module doc comment describes. `schema` picks the reader per
/// `formats::mod`'s "two readers" design — [`effective_schema`] resolves it either way.
pub(crate) fn read_ipc(bytes: &[u8], schema: ReadSchema<'_>) -> Result<RecordBatch, Error> {
    let magic_len = ARROW_MAGIC.len();
    if bytes.len() < magic_len * 2 + 2 + 4 {
        return Err(Error::general_error("ipc: file is too small to be an Arrow IPC file".to_string()));
    }
    if &bytes[..magic_len] != ARROW_MAGIC || &bytes[bytes.len() - magic_len..] != ARROW_MAGIC {
        return Err(Error::general_error("ipc: missing the 'ARROW1' magic".to_string()));
    }

    let footer_len_pos = bytes.len() - magic_len - 4;
    let footer_len = fb::read_i32(bytes, footer_len_pos)?;
    let footer_len = usize::try_from(footer_len).map_err(|e| Error::from_error(ErrorType::ConversionError, e))?;
    let footer_start = footer_len_pos
        .checked_sub(footer_len)
        .ok_or_else(|| Error::general_error("ipc: footer length exceeds the file".to_string()))?;

    let footer_table = fb::FbTable::root(bytes, footer_start)?;

    let dictionaries = footer_table.block_vector_field(FOOTER_VT_DICTIONARIES)?;
    if !dictionaries.is_empty() {
        return Err(Error::not_supported(
            "Arrow IPC: the file lists dictionary batches, which this reader refuses".to_string(),
        ));
    }

    let schema_table = footer_table
        .table_field(FOOTER_VT_SCHEMA)?
        .ok_or_else(|| Error::general_error("ipc: footer has no schema".to_string()))?;
    let decoded_schema = decode_schema(&schema_table)?;
    let record_schema = Arc::new(effective_schema(&decoded_schema, schema)?);

    let blocks = footer_table.block_vector_field(FOOTER_VT_RECORD_BATCHES)?;
    if blocks.is_empty() {
        let columns = decoded_schema.arrow_fields.iter().map(|f| Column::empty(f.data_type)).collect();
        return RecordBatch::new(record_schema, columns, None, Some(Vec::new()), Vec::new());
    }

    let mut batches = Vec::with_capacity(blocks.len());
    for (offset, meta_data_length, body_length) in blocks {
        batches.push(read_one_record_batch(
            bytes,
            offset,
            meta_data_length,
            body_length,
            &record_schema,
            &decoded_schema.arrow_fields,
        )?);
    }
    RecordBatch::concat(&batches)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats::{ReadOptions, ReadSchema, TableFormat, WriteOptions};
    use crate::mutable::RecordBatchMut;
    use crate::schema::{FieldRole, KeyRole};
    use crate::{formats, RecordViewMut};
    use liquers_core::query::Key;
    use std::sync::Arc;

    fn sample_batch() -> Result<RecordBatch, Error> {
        let schema = Arc::new(RecordSchema::new(vec![
            FieldSchema::new("id", FieldType::Int).with_key(KeyRole::Id),
            FieldSchema::new("name", FieldType::Text).with_role(FieldRole::text()),
        ])?);
        let mut batch = RecordBatchMut::with_capacity(schema, 1);
        batch.append_row(&[crate::column::FieldValue::Int(1), crate::column::FieldValue::Text(Arc::from("alice"))])?;
        batch.freeze()
    }

    #[test]
    fn feather_round_trip_preserves_types_roles_and_labels() -> Result<(), Error> {
        let batch = sample_batch()?;
        let bytes = formats::write_table(&batch, TableFormat::Ipc, &WriteOptions::default())?;
        let read_back = formats::read_table(&bytes, TableFormat::Ipc, ReadSchema::Infer, &ReadOptions::default())?;
        assert_eq!(read_back.schema.id_field(), Some(0));
        assert_eq!(read_back.value(0, 1)?, crate::column::FieldValue::Text(Arc::from("alice")));
        Ok(())
    }

    #[test]
    fn feather_preserves_chunk_id_in_custom_metadata() -> Result<(), Error> {
        let mut batch = sample_batch()?;
        batch.chunk_id = Some(crate::batch::ChunkId::Key(Key::new()));
        let bytes = formats::write_table(&batch, TableFormat::Ipc, &WriteOptions::default())?;
        let read_back = formats::read_table(&bytes, TableFormat::Ipc, ReadSchema::Infer, &ReadOptions::default())?;
        assert!(read_back.chunk_id.is_some());
        Ok(())
    }

    #[test]
    fn feather_round_trips_null_values() -> Result<(), Error> {
        let schema = Arc::new(RecordSchema::new(vec![
            FieldSchema::new("a", FieldType::Int),
            FieldSchema::new("b", FieldType::Text),
        ])?);
        let mut batch = RecordBatchMut::with_capacity(schema, 2);
        batch.append_row(&[crate::column::FieldValue::Null, crate::column::FieldValue::Text(Arc::from("x"))])?;
        batch.append_row(&[crate::column::FieldValue::Int(7), crate::column::FieldValue::Null])?;
        let batch = batch.freeze()?;
        let bytes = write_ipc(&batch)?;
        let read_back = read_ipc(&bytes, ReadSchema::Infer)?;
        assert_eq!(read_back.value(0, 0)?, crate::column::FieldValue::Null);
        assert_eq!(read_back.value(1, 0)?, crate::column::FieldValue::Int(7));
        assert_eq!(read_back.value(0, 1)?, crate::column::FieldValue::Text(Arc::from("x")));
        assert_eq!(read_back.value(1, 1)?, crate::column::FieldValue::Null);
        Ok(())
    }

    #[test]
    fn feather_round_trips_every_scalar_type() -> Result<(), Error> {
        let schema = Arc::new(RecordSchema::new(vec![
            FieldSchema::new("b", FieldType::Bool),
            FieldSchema::new("i", FieldType::Int),
            FieldSchema::new("u", FieldType::UInt),
            FieldSchema::new("f", FieldType::Float),
            FieldSchema::new("bin", FieldType::Binary),
            FieldSchema::new("d", FieldType::Date),
            FieldSchema::new("t", FieldType::Timestamp),
        ])?);
        let mut batch = RecordBatchMut::with_capacity(schema, 1);
        batch.append_row(&[
            crate::column::FieldValue::Bool(true),
            crate::column::FieldValue::Int(-5),
            crate::column::FieldValue::UInt(9),
            crate::column::FieldValue::Float(1.5),
            crate::column::FieldValue::Bytes(Arc::from(&b"xy"[..])),
            crate::column::FieldValue::Date(19700),
            crate::column::FieldValue::Timestamp(123_456_789),
        ])?;
        let batch = batch.freeze()?;
        let bytes = write_ipc(&batch)?;
        let read_back = read_ipc(&bytes, ReadSchema::Infer)?;
        assert_eq!(read_back.value(0, 0)?, crate::column::FieldValue::Bool(true));
        assert_eq!(read_back.value(0, 1)?, crate::column::FieldValue::Int(-5));
        assert_eq!(read_back.value(0, 2)?, crate::column::FieldValue::UInt(9));
        assert_eq!(read_back.value(0, 3)?, crate::column::FieldValue::Float(1.5));
        assert_eq!(read_back.value(0, 4)?, crate::column::FieldValue::Bytes(Arc::from(&b"xy"[..])));
        assert_eq!(read_back.value(0, 5)?, crate::column::FieldValue::Date(19700));
        assert_eq!(read_back.value(0, 6)?, crate::column::FieldValue::Timestamp(123_456_789));
        Ok(())
    }

    #[test]
    fn feather_round_trips_vector_columns() -> Result<(), Error> {
        let schema = Arc::new(RecordSchema::new(vec![FieldSchema::new("v", FieldType::Vector)])?);
        let mut batch = RecordBatchMut::with_capacity(schema, 2);
        batch.append_row(&[crate::column::FieldValue::Vector(Arc::from(vec![1.0f32, 2.0, 3.0]))])?;
        batch.append_row(&[crate::column::FieldValue::Vector(Arc::from(vec![4.0f32, 5.0, 6.0]))])?;
        let batch = batch.freeze()?;
        let bytes = write_ipc(&batch)?;
        let read_back = read_ipc(&bytes, ReadSchema::Infer)?;
        assert_eq!(read_back.value(0, 0)?, crate::column::FieldValue::Vector(Arc::from(vec![1.0f32, 2.0, 3.0])));
        assert_eq!(read_back.value(1, 0)?, crate::column::FieldValue::Vector(Arc::from(vec![4.0f32, 5.0, 6.0])));
        Ok(())
    }

    #[test]
    fn feather_read_with_declared_schema_checks_types() -> Result<(), Error> {
        let batch = sample_batch()?;
        let bytes = write_ipc(&batch)?;
        let wrong_schema = RecordSchema::new(vec![FieldSchema::new("id", FieldType::Text)])?;
        let error = read_ipc(&bytes, ReadSchema::Declared(&wrong_schema)).expect_err("field count mismatch");
        assert!(format!("{error}").contains("declared schema"));
        Ok(())
    }

    #[test]
    fn feather_read_refuses_dictionary_encoded_field() -> Result<(), Error> {
        // A minimal, hand-built `Field` claiming dictionary encoding — the unit-test substitute
        // for a real dictionary-encoded fixture (see IPC-READER-UNTESTED-ON-DICTIONARY-OR-
        // COMPRESSED-INPUT). `dictionary` only needs to be *present*; its own contents are never
        // read (decode_arrow_field refuses on presence alone).
        let mut builder = FlatBufferBuilder::new();
        let name = builder.create_string("category");
        let dict_start = builder.start_table();
        let dict_offset = builder.end_table(dict_start);
        let type_offset = write_empty_type(&mut builder);
        let field_start = builder.start_table();
        builder.push_slot_always::<WIPOffset<&str>>(FIELD_VT_NAME, name);
        builder.push_slot::<u8>(FIELD_VT_TYPE_TYPE, TYPE_UTF8, TYPE_NONE);
        builder.push_slot_always::<TableOffset>(FIELD_VT_TYPE, type_offset);
        builder.push_slot_always::<TableOffset>(FIELD_VT_DICTIONARY, dict_offset);
        let field_offset = builder.end_table(field_start);
        builder.finish(field_offset, None);
        let bytes = builder.finished_data().to_vec();

        let field_table = fb::FbTable::root(&bytes, 0)?;
        let error = decode_arrow_field(&field_table).expect_err("dictionary-encoded field is refused");
        assert!(format!("{error}").contains("dictionary"));
        Ok(())
    }

    #[test]
    fn feather_read_refuses_missing_magic() {
        let error = read_ipc(b"not an arrow file", ReadSchema::Infer).expect_err("missing magic");
        assert!(format!("{error}").contains("ARROW1") || format!("{error}").contains("small"));
    }

    #[test]
    fn type_tag_name_covers_the_supported_tags() {
        assert_eq!(type_tag_name(TYPE_INT), "Int");
        assert_eq!(type_tag_name(TYPE_FIXED_SIZE_LIST), "FixedSizeList");
        assert!(type_tag_name(200).contains("200"));
    }

    // --- Fixture-dependent tests (Phase 3 §3.7's sketch, no longer `#[ignore]`d): fixtures
    // generated once by `liquers-lib`'s `generate_dictionary_and_compressed_fixtures` (`cargo test
    // -p liquers-lib --test records_ipc_polars -- --ignored`) and checked in under
    // `liquers-records/tests/fixtures/`. See that test for how each was produced. ---

    #[test]
    fn feather_read_refuses_dictionary_encoded_batches() {
        let bytes = include_bytes!("../../tests/fixtures/dictionary_encoded.ipc");
        let error = read_ipc(bytes, ReadSchema::Infer).expect_err("dictionary-encoded IPC file is refused");
        assert!(format!("{error}").contains("dictionar"), "unexpected error: {error}");
    }

    #[test]
    fn feather_read_refuses_compressed_bodies() {
        let bytes = include_bytes!("../../tests/fixtures/compressed_body.ipc");
        let error = read_ipc(bytes, ReadSchema::Infer).expect_err("compressed-body IPC file is refused");
        let message = format!("{error}");
        assert!(message.contains("compressed") && message.contains("LZ4"), "unexpected error: {message}");
    }
}
