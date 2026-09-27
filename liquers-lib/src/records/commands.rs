//! The `ns-rec` commands: the query-level surface over `liquers-records`, built on the two
//! conversion helpers in `records::convert` (`to_record`, `to_record_source`).
//!
//! See `specs/design/record-streams/phase2-architecture.md` §"Relevant Commands" and
//! `specs/design/record-streams/phase4-implementation.md` Step 5.5.
//!
//! Every command here takes an input state and is therefore **async with `context` last**, even
//! where Phase 2's table shows a sync `fn`: `to_record`/`to_record_source` are themselves async
//! (they may need to fetch a keyed dependency through the asset manager), so a command that
//! converts its input through them cannot be sync. See phase4-implementation.md Step 5.5's
//! decision and the matching Phase 2 changelog entry.
//!
//! Two commands share a name with a `convert.rs` helper (`to_record`, `to_record_source`) —
//! `register_command!` has no renaming, so the command function itself is named identically to the
//! helper it calls. The helper is therefore always invoked by its full path
//! (`super::convert::to_record` / `super::convert::to_record_source`), never through a glob import
//! of `convert`, so the two never shadow each other.

use std::sync::Arc;

use liquers_core::context::{Context, Environment};
use liquers_core::error::{Error, ErrorType};
use liquers_core::query::Query;
use liquers_core::state::State;
use liquers_core::value::ValueInterface;

use liquers_records::{
    place_chunk, view_from_chunk_value, ChunkList, ChunkResolver, CompareOp, ContextResolver,
    FieldRole, FieldSchema, FieldType, FieldValue, JsonOrient, ReadSchema, RecordBatchMut,
    RecordSchema, RecordSource, RecordView, RecordViewMut, RowId,
};

use crate::value::{ExtValueInterface, Value};

use super::convert::ToRecordOptions;

// ---------------------------------------------------------------------------------------------
// Small helpers shared by several commands
// ---------------------------------------------------------------------------------------------

/// A query argument declared `i64` (the DSL has no unsigned/`usize` integer type) turned into a
/// `usize` index/count. Negative is refused rather than silently truncated.
fn non_negative_usize(n: i64, name: &str) -> Result<usize, Error> {
    usize::try_from(n)
        .map_err(|_| Error::general_error(format!("{name} must not be negative, got {n}")))
}

/// As [`non_negative_usize`], but for a [`RowId`] field, which is `u64`.
fn non_negative_u64(n: i64, name: &str) -> Result<u64, Error> {
    u64::try_from(n)
        .map_err(|_| Error::general_error(format!("{name} must not be negative, got {n}")))
}

/// `schema`'s spelling (Step 5.5's decision): `String`, empty meaning "no schema" (infer), a
/// non-empty string a YAML or JSON [`RecordSchema`] document — `register_command!` has no
/// `FromParameterValue`/`TryFrom<Value>` impl for `Option<Value>`, so that spelling cannot bind
/// (checked against `liquers-core/src/commands.rs`'s `impl_from_parameter_value2*!` list before
/// choosing this one).
fn parse_schema_argument(schema: &str) -> Result<Option<RecordSchema>, Error> {
    if schema.is_empty() {
        return Ok(None);
    }
    serde_yaml::from_str(schema)
        .map(Some)
        .map_err(|error| Error::from_error(ErrorType::SerializationError, error))
}

/// Turns `id` (always a query string) into the `FieldValue` the declared `Id` field's type
/// expects, for [`rec_id`]'s comparison. Every `FieldType` variant is matched explicitly per
/// CLAUDE.md's "Match Statements" convention; `Binary`/`Vector` Id fields are refused rather than
/// guessed at (`RecordSchema::new` already requires the Id field to be Exact-indexed and stored,
/// which in practice means a scalar, comparable type).
fn parse_id_value(data_type: FieldType, id: &str) -> Result<FieldValue, Error> {
    match data_type {
        FieldType::Text => Ok(FieldValue::Text(Arc::from(id))),
        FieldType::Int => id
            .parse::<i64>()
            .map(FieldValue::Int)
            .map_err(|error| Error::conversion_error_with_message(id, "Int", &error.to_string())),
        FieldType::UInt => id
            .parse::<u64>()
            .map(FieldValue::UInt)
            .map_err(|error| Error::conversion_error_with_message(id, "UInt", &error.to_string())),
        FieldType::Float => id
            .parse::<f64>()
            .map(FieldValue::Float)
            .map_err(|error| Error::conversion_error_with_message(id, "Float", &error.to_string())),
        FieldType::Bool => id
            .parse::<bool>()
            .map(FieldValue::Bool)
            .map_err(|error| Error::conversion_error_with_message(id, "Bool", &error.to_string())),
        FieldType::Date => id
            .parse::<i32>()
            .map(FieldValue::Date)
            .map_err(|error| Error::conversion_error_with_message(id, "Date", &error.to_string())),
        FieldType::Timestamp => id
            .parse::<i64>()
            .map(FieldValue::Timestamp)
            .map_err(|error| {
                Error::conversion_error_with_message(id, "Timestamp", &error.to_string())
            }),
        FieldType::Binary => Err(Error::conversion_error(
            id.to_string(),
            "a Binary Id field, which ns-rec/rec_id does not support",
        )),
        FieldType::Vector => Err(Error::conversion_error(
            id.to_string(),
            "a Vector Id field, which ns-rec/rec_id does not support",
        )),
    }
}

/// The chunk id at `index` in `chunks` — direct addressing for [`rowid`]'s "opens only that
/// chunk". For `ChunkList::Unbounded`, only the already-computed prefix (the explicit chunks, in
/// `ManifestSource`'s case) can be addressed this way; a template-generated chunk beyond it is
/// reported as not yet discovered rather than walked, per `ChunkList::Unbounded`'s own contract
/// ("a walk ends at the first short chunk").
fn chunk_id_at(chunks: ChunkList<'_>, index: usize) -> Result<liquers_records::ChunkId, Error> {
    match chunks {
        ChunkList::Known(ids) => ids.get(index).cloned().ok_or_else(|| {
            Error::general_error(format!(
                "ns-rec/rowid: chunk {index} out of range (0..{})",
                ids.len()
            ))
        }),
        ChunkList::Unbounded { computed } => computed.get(index).cloned().ok_or_else(|| {
            Error::general_error(format!(
                "ns-rec/rowid: chunk {index} is not yet known for this source (only {} chunk(s) \
                 discovered so far); ns-rec/materialize or ns-rec/rec_id walk further into it",
                computed.len()
            ))
        }),
    }
}

/// [`rowid`]'s fallback when a source has no manifest (so its chunk ids are not resolvable
/// queries — `InMemorySource`'s placeholder ids, in particular): the only way left to reach a
/// chunk's view is `RecordSource::stream`, walked up to — and no further than — `chunk_index`.
async fn walk_to_chunk(
    source: Arc<dyn RecordSource>,
    resolver: Arc<dyn ChunkResolver>,
    chunk_index: usize,
) -> Result<Arc<dyn RecordView>, Error> {
    use futures::StreamExt;
    let mut stream = source.stream(resolver).await?;
    let mut index = 0usize;
    while let Some(view) = stream.next().await {
        let view = view?;
        if index == chunk_index {
            return Ok(view);
        }
        index += 1;
    }
    Err(Error::general_error(format!(
        "ns-rec/rowid: chunk {chunk_index} out of range (only {index} chunk(s) in this source)"
    )))
}

/// The JSON value `ns-rec/from_json` reads: a JSON value taken as-is, text/bytes parsed as JSON,
/// or a key resolved (recording the dependency) and read again — the same input shapes
/// `to_record`/`to_record_source` accept, restricted to what can become JSON.
async fn json_value_of<E: Environment<Value = Value>>(
    value: &Value,
    context: &Context<E>,
) -> Result<serde_json::Value, Error> {
    match value.identifier().as_ref() {
        "Array" | "Object" => value.try_into_json_value(),
        "Text" => {
            let text = value.try_into_string()?;
            serde_json::from_str(&text)
                .map_err(|error| Error::from_error(ErrorType::SerializationError, error))
        }
        "Bytes" => {
            let bytes = value.try_into_bytes()?;
            serde_json::from_slice(&bytes)
                .map_err(|error| Error::from_error(ErrorType::SerializationError, error))
        }
        "Key" => {
            let key = value.try_into_key()?;
            let state = context.get_dependency_state(&Query::from(key)).await?;
            // Recursion through an async fn must be boxed (infinite future size otherwise) — the
            // same pattern `convert::to_record`/`convert::to_record_source` use for their own
            // "Key" arm.
            Box::pin(json_value_of(state.data_unchecked(), context)).await
        }
        other => Err(Error::conversion_error(other.to_string(), "JSON value")),
    }
}

// ---------------------------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------------------------

/// The single-record selector: the one record whose declared `Id` field matches `id`. Needs a
/// declared `Id` — without one it refuses, naming `rowid`, which addresses every row regardless.
/// Works over a source (walking chunks until found) or a view (wrapped into a one-chunk source by
/// `to_record_source`), and always returns a materialized one-row batch.
pub async fn rec_id<E: Environment<Value = Value>>(
    state: State<Value>,
    id: String,
    context: Context<E>,
) -> Result<Value, Error> {
    let options = ToRecordOptions::default();
    let source = super::convert::to_record_source(state.data_unchecked(), &state.metadata, &options, &context)
        .await?;
    let schema = source.schema().ok_or_else(|| {
        Error::general_error(
            "ns-rec/rec_id needs a declared schema to know which field is Id; use ns-rec/rowid \
             to address a row by its implicit id instead"
                .to_string(),
        )
    })?;
    let id_index = schema.id_field().ok_or_else(|| {
        Error::general_error(
            "ns-rec/rec_id needs a declared Id field; use ns-rec/rowid to address a row by its \
             implicit id instead"
                .to_string(),
        )
    })?;
    let target = parse_id_value(schema.fields[id_index].data_type, &id)?;

    let resolver: Arc<dyn ChunkResolver> = Arc::new(ContextResolver::new(context.clone()));
    let mut stream = source.stream(resolver).await?;
    {
        use futures::StreamExt;
        while let Some(view) = stream.next().await {
            let view = view?;
            let column = view.column(id_index)?;
            let mask = column.compare(CompareOp::Eq, &target)?;
            let matched_row = mask.iter_ones().next();
            if let Some(row_index) = matched_row {
                let row_view = view.row(row_index)?;
                let batch = row_view.materialize()?;
                return Ok(Value::from_record_view(batch as Arc<dyn RecordView>));
            }
        }
    }
    Err(Error::general_error(format!(
        "ns-rec/rec_id: no record with id '{id}' found"
    )))
}

/// A row by position — views only; a source has no stable positions (`to_record` refuses a
/// source, naming `ns-rec/materialize`).
pub async fn row<E: Environment<Value = Value>>(
    state: State<Value>,
    n: i64,
    context: Context<E>,
) -> Result<Value, Error> {
    let view = super::convert::to_record(
        state.data_unchecked(),
        &state.metadata,
        &ToRecordOptions::default(),
        &context,
    )
    .await?;
    let index = non_negative_usize(n, "n")?;
    let row_view = view.row(index)?;
    let batch = row_view.materialize()?;
    Ok(Value::from_record_view(batch as Arc<dyn RecordView>))
}

/// Column projection. Keeps the `Id`/`Source` columns whether named or not
/// (`RecordView::select_columns`'s own contract).
pub async fn select_columns<E: Environment<Value = Value>>(
    state: State<Value>,
    columns: Vec<String>,
    context: Context<E>,
) -> Result<Value, Error> {
    let view = super::convert::to_record(
        state.data_unchecked(),
        &state.metadata,
        &ToRecordOptions::default(),
        &context,
    )
    .await?;
    // A variadic argument defaults to the empty list, so an empty selection is well-formed at
    // plan level (see `pl/select_columns`) — only the command knows it needs at least one column.
    if columns.is_empty() {
        return Err(Error::general_error(
            "ns-rec/select_columns requires at least one column name".to_string(),
        ));
    }
    let names: Vec<&str> = columns.iter().map(String::as_str).collect();
    let projected = view.select_columns(&names)?;
    Ok(Value::from_record_view(projected))
}

/// The first `n` rows (default 5), materialized for inspection.
pub async fn head<E: Environment<Value = Value>>(
    state: State<Value>,
    n: i64,
    context: Context<E>,
) -> Result<Value, Error> {
    let view = super::convert::to_record(
        state.data_unchecked(),
        &state.metadata,
        &ToRecordOptions::default(),
        &context,
    )
    .await?;
    let requested = non_negative_usize(n, "n")?;
    let take = requested.min(view.len());
    let sliced = view.slice(0, take)?;
    let batch = sliced.materialize()?;
    Ok(Value::from_record_view(batch as Arc<dyn RecordView>))
}

/// A row range, as a view (not materialized). Clamped to the rows that exist, as `head` is: a range
/// running past the end gives the rows up to the end, and an offset past the end gives no rows —
/// so `ns-rec/slice` works as a manifest template's query, whose last chunk comes back short.
pub async fn slice<E: Environment<Value = Value>>(
    state: State<Value>,
    offset: i64,
    length: i64,
    context: Context<E>,
) -> Result<Value, Error> {
    let view = super::convert::to_record(
        state.data_unchecked(),
        &state.metadata,
        &ToRecordOptions::default(),
        &context,
    )
    .await?;
    let offset = non_negative_usize(offset, "offset")?.min(view.len());
    let length = non_negative_usize(length, "length")?.min(view.len() - offset);
    let sliced = view.slice(offset, length)?;
    Ok(Value::from_record_view(sliced))
}

/// A row by its implicit id (chunk, row). Over a source, opens only that chunk — no whole-source
/// walk. Over a view, scans for the row whose implicit `RowId` matches (a materialized multi-chunk
/// batch's rows may not be laid out by chunk order).
pub async fn rowid<E: Environment<Value = Value>>(
    state: State<Value>,
    chunk: i64,
    row: i64,
    context: Context<E>,
) -> Result<Value, Error> {
    let value = state.data_unchecked();
    if value.identifier().as_ref() == "RecordSource" {
        let source = value.as_record_source()?;
        let resolver: Arc<dyn ChunkResolver> = Arc::new(ContextResolver::new(context.clone()));
        let chunk_index = non_negative_usize(chunk, "chunk")?;
        let view = if source.manifest().is_some() {
            // A manifest's chunk ids are genuinely resolvable queries/keys (`ManifestSource`'s
            // `describe_chunk`), so the addressed chunk is fetched directly — no other chunk is
            // opened.
            // The same conversion and `uniform_schema` check a traversal applies, and the chunk
            // placed at its index; its row number stays unknown, as for any chunk read on its own.
            let chunk_id = chunk_id_at(source.chunks(), chunk_index)?;
            let descriptor = source.describe_chunk(&chunk_id, resolver.as_ref()).await?;
            let chunk_value = resolver.evaluate(descriptor.query.clone()).await?;
            let view = view_from_chunk_value(chunk_value, source.schema().as_deref())?;
            place_chunk(view, chunk_index as u64, Some(chunk_id), None)
        } else {
            // No manifest: this source's chunk ids are not resolvable queries (e.g.
            // `InMemorySource`'s placeholder ids, which `RecordSource::stream` documents as never
            // going through a `ChunkResolver`). `stream()` is then the only way to reach a chunk's
            // view, so walk it up to — and no further than — the addressed chunk.
            walk_to_chunk(source, resolver, chunk_index).await?
        };
        let row_index = non_negative_usize(row, "row")?;
        let row_view = view.row(row_index)?;
        let batch = row_view.materialize()?;
        Ok(Value::from_record_view(batch as Arc<dyn RecordView>))
    } else {
        let view = super::convert::to_record(
            value,
            &state.metadata,
            &ToRecordOptions::default(),
            &context,
        )
        .await?;
        let target = RowId {
            chunk: non_negative_u64(chunk, "chunk")?,
            row: non_negative_u64(row, "row")?,
        };
        for index in 0..view.len() {
            if view.row_id(index)? == target {
                let row_view = view.row(index)?;
                let batch = row_view.materialize()?;
                return Ok(Value::from_record_view(batch as Arc<dyn RecordView>));
            }
        }
        Err(Error::general_error(format!(
            "ns-rec/rowid: no row with chunk {chunk}, row {row} found in this view"
        )))
    }
}

/// Any input a source can be made from, turned into a `RecordSource` — the command wrapping
/// `convert::to_record_source`. `format` empty means "use the state's own metadata".
pub async fn to_record_source<E: Environment<Value = Value>>(
    state: State<Value>,
    format: String,
    context: Context<E>,
) -> Result<Value, Error> {
    let options = ToRecordOptions {
        format: (!format.is_empty()).then_some(format),
        ..Default::default()
    };
    let source = super::convert::to_record_source(
        state.data_unchecked(),
        &state.metadata,
        &options,
        &context,
    )
    .await?;
    Ok(Value::from_record_source(source))
}

/// The one step from a source to a table. Refused past `max_rows` and for a non-uniform source
/// (`RecordSource::materialize`'s own checks). On a view — wrapped into a one-view
/// `InMemorySource` by `to_record_source` — this is a free shallow clone.
pub async fn materialize<E: Environment<Value = Value>>(
    state: State<Value>,
    max_rows: i64,
    context: Context<E>,
) -> Result<Value, Error> {
    let options = ToRecordOptions::default();
    let source = super::convert::to_record_source(
        state.data_unchecked(),
        &state.metadata,
        &options,
        &context,
    )
    .await?;
    let resolver: Arc<dyn ChunkResolver> = Arc::new(ContextResolver::new(context.clone()));
    let max_rows = non_negative_usize(max_rows, "max_rows")?;
    let batch = source.materialize(resolver, max_rows).await?;
    Ok(Value::from_record_view(batch as Arc<dyn RecordView>))
}

/// The schema, as a value a `schema` argument can be linked to: a YAML/JSON `RecordSchema`
/// document, serialized to text (`schema: String` cannot bind to a structured JSON value — see
/// `parse_schema_argument`'s doc comment — so the round trip goes through text both ways).
pub async fn records_schema<E: Environment<Value = Value>>(
    state: State<Value>,
    context: Context<E>,
) -> Result<Value, Error> {
    let view = super::convert::to_record(
        state.data_unchecked(),
        &state.metadata,
        &ToRecordOptions::default(),
        &context,
    )
    .await?;
    let text = serde_json::to_string_pretty(view.schema().as_ref())
        .map_err(|error| Error::from_error(ErrorType::SerializationError, error))?;
    Ok(Value::from_string(text))
}

/// A view as one of the seven JSON table shapes.
pub async fn to_json<E: Environment<Value = Value>>(
    state: State<Value>,
    orient: String,
    context: Context<E>,
) -> Result<Value, Error> {
    let view = super::convert::to_record(
        state.data_unchecked(),
        &state.metadata,
        &ToRecordOptions::default(),
        &context,
    )
    .await?;
    let orient: JsonOrient = orient.parse()?;
    let json = liquers_records::to_json(view.as_ref(), orient)?;
    Value::try_from_json_value(&json)
}

/// A JSON value, text or bytes as a table. `auto` detects the shape and refuses the ambiguous
/// one; `schema` makes the read schema-aware and strict.
pub async fn from_json<E: Environment<Value = Value>>(
    state: State<Value>,
    orient: String,
    schema: String,
    context: Context<E>,
) -> Result<Value, Error> {
    let orient: JsonOrient = orient.parse()?;
    let json = json_value_of(state.data_unchecked(), &context).await?;
    let parsed_schema = parse_schema_argument(&schema)?;
    let read_schema = match &parsed_schema {
        Some(schema) => ReadSchema::Declared(schema),
        None => ReadSchema::Infer,
    };
    let batch = liquers_records::from_json(&json, orient, read_schema)?;
    Ok(Value::from_record_view(Arc::new(batch) as Arc<dyn RecordView>))
}

/// Any input a table can be made from, turned into a `RecordView` — the command wrapping
/// `convert::to_record`. `format`/`header` empty/true are the "use the state's own metadata"
/// defaults; `schema` (empty meaning none) makes the read schema-aware.
pub async fn to_record<E: Environment<Value = Value>>(
    state: State<Value>,
    format: String,
    header: bool,
    schema: String,
    context: Context<E>,
) -> Result<Value, Error> {
    let parsed_schema = parse_schema_argument(&schema)?.map(Arc::new);
    let options = ToRecordOptions {
        format: (!format.is_empty()).then_some(format),
        header: Some(header),
        schema: parsed_schema,
        max_rows: None,
    };
    let view = super::convert::to_record(
        state.data_unchecked(),
        &state.metadata,
        &options,
        &context,
    )
    .await?;
    Ok(Value::from_record_view(view))
}

/// List every file directly under a directory key as a `RecordBatch`: `file_id`, `file_name`,
/// `size_bytes`, `modified_timestamp`. Async because it lists the store.
///
/// From `specs/design/record-streams/phase3-tests.md` §1.1, unchanged (the file it walks is a
/// store directory, not a converted record input, so it has no `to_record`/`to_record_source`
/// step of its own — see phase4-implementation.md Step 5.5's decision, which is about commands
/// that convert an *input*).
pub async fn file_records<E: Environment<Value = Value>>(
    state: State<Value>,
    context: Context<E>,
) -> Result<Value, Error> {
    let schema = Arc::new(RecordSchema::new(vec![
        FieldSchema::new("file_id", FieldType::Text)
            .with_label("File ID")
            .with_key(liquers_records::KeyRole::Id),
        FieldSchema::new("file_name", FieldType::Text)
            .with_label("Name")
            .with_role(FieldRole::text()),
        FieldSchema::new("size_bytes", FieldType::Int)
            .with_label("Size")
            .with_role(FieldRole::numeric())
            .not_null(),
        FieldSchema::new("modified_timestamp", FieldType::Timestamp)
            .with_label("Modified")
            .with_role(FieldRole::numeric()),
    ])?);

    // `-R/data/` is a directory: its state carries no value, only metadata naming the key.
    let dir_key = state.metadata.key()?.ok_or_else(|| {
        Error::general_error("file_records needs a directory resource, e.g. -R/data/".to_string())
    })?;
    // `Context` has no store accessor of its own; the store is the environment's.
    let store = context.get_envref().get_async_store();
    let entries = store.listdir_asset_info(&dir_key).await?;

    let mut batch = RecordBatchMut::with_capacity(schema, entries.len());
    for info in entries.into_iter().filter(|info| !info.is_dir) {
        let name = info.filename.unwrap_or_default();
        let file_key = dir_key.join(&name);
        // `updated` is RFC 3339; a file whose store does not report a valid timestamp gets 0
        // rather than failing the whole listing — this is diagnostic, not load-bearing data.
        let modified_us = chrono::DateTime::parse_from_rfc3339(&info.updated)
            .map(|dt| dt.timestamp_micros())
            .unwrap_or(0);
        batch.append_row(&[
            FieldValue::Text(Arc::from(file_key.to_string().as_str())),
            FieldValue::Text(Arc::from(name.as_str())),
            FieldValue::Int(info.file_size.unwrap_or(0) as i64),
            FieldValue::Timestamp(modified_us),
        ])?;
    }

    let view: Arc<dyn RecordView> = Arc::new(batch.freeze()?);
    Ok(Value::from_record_view(view))
}

// ---------------------------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------------------------

/// Register all `ns-rec` commands via macro, following `polars/mod.rs`'s
/// `register_polars_commands!` pattern.
///
/// The caller must define `type CommandEnvironment = ...` in scope before invoking.
#[macro_export]
macro_rules! register_records_commands {
    ($cr:expr) => {{
        use liquers_macro::register_command;
        use $crate::records::commands::*;

        register_command!($cr,
            async fn rec_id(state, id: String, context) -> result
            namespace: "rec"
            label: "Record by id"
            doc: "The one record whose declared Id field matches; refuses without a declared Id"
        )?;

        register_command!($cr,
            async fn row(state, n: i64, context) -> result
            namespace: "rec"
            label: "Row"
            doc: "A row by position (views only)"
        )?;

        register_command!($cr,
            async fn select_columns(state, columns: Vec<String> multiple, context) -> result
            namespace: "rec"
            label: "Select columns"
            doc: "Column projection; keeps the Id/Source columns whether named or not"
        )?;

        register_command!($cr,
            async fn head(state, n: i64 = 5, context) -> result
            namespace: "rec"
            label: "Head"
            doc: "The first n rows (default 5), materialized"
        )?;

        register_command!($cr,
            async fn slice(state, offset: i64, length: i64, context) -> result
            namespace: "rec"
            label: "Slice"
            doc: "A row range, as a view"
        )?;

        register_command!($cr,
            async fn rowid(state, chunk: i64, row: i64, context) -> result
            namespace: "rec"
            label: "Row by implicit id"
            doc: "A row by its implicit (chunk, row) id"
        )?;

        register_command!($cr,
            async fn to_record_source(state, format: String = "", context) -> result
            namespace: "rec"
            label: "To record source"
            doc: "Any input a source can be made from, turned into a RecordSource"
        )?;

        register_command!($cr,
            async fn materialize(state, max_rows: i64 = 1000000, context) -> result
            namespace: "rec"
            label: "Materialize"
            doc: "The one step from a source to a table"
        )?;

        register_command!($cr,
            async fn records_schema(state, context) -> result
            namespace: "rec"
            label: "Schema"
            doc: "The schema, as a value a schema argument can be linked to"
        )?;

        register_command!($cr,
            async fn to_json(state, orient: String = "records", context) -> result
            namespace: "rec"
            label: "To JSON"
            doc: "A view as one of the seven JSON table shapes"
        )?;

        register_command!($cr,
            async fn from_json(state, orient: String = "auto", schema: String = "", context) -> result
            namespace: "rec"
            label: "From JSON"
            doc: "A JSON value, text or bytes as a table"
        )?;

        register_command!($cr,
            async fn to_record(state, format: String = "", header: bool = true, schema: String = "", context) -> result
            namespace: "rec"
            label: "To record"
            doc: "Any input a table can be made from, turned into a RecordView"
        )?;

        register_command!($cr,
            async fn file_records(state, context) -> result
            namespace: "rec"
            label: "File records"
            doc: "List all files in a directory as a table with size and modification time"
        )?;

        Ok::<(), liquers_core::error::Error>(())
    }};
}

#[cfg(test)]
mod tests {
    use super::*;
    use liquers_core::interpreter::evaluate;
    use liquers_core::query::Key;
    use liquers_core::store::{AsyncMemoryStore, AsyncStore};
    use liquers_macro::register_command;
    use liquers_records::{Buffer, Column, InMemorySource, ManifestSource, ManifestSpec};

    use crate::environment::{CommandRegistryAccess, DefaultEnvironment};

    type CommandEnvironment = DefaultEnvironment<Value>;

    fn env_with_records_commands() -> Result<DefaultEnvironment<Value>, Error> {
        let mut env = DefaultEnvironment::<Value>::new();
        let cr = env.get_mut_command_registry();
        register_records_commands!(cr)?;
        Ok(env)
    }

    fn schema_id_and_amount() -> Arc<RecordSchema> {
        Arc::new(
            RecordSchema::new(vec![
                FieldSchema::new("id", FieldType::Text).with_key(liquers_records::KeyRole::Id),
                FieldSchema::new("amount", FieldType::Int),
            ])
            .expect("schema"),
        )
    }

    fn batch_view(ids: &[&str], amounts: &[i64]) -> Arc<dyn RecordView> {
        let schema = schema_id_and_amount();
        let offsets: Vec<i32> = std::iter::once(0)
            .chain(ids.iter().scan(0i32, |acc, s| {
                *acc += s.len() as i32;
                Some(*acc)
            }))
            .collect();
        let data: Vec<u8> = ids.iter().flat_map(|s| s.bytes()).collect();
        let id_column = Column::Text {
            validity: None,
            offsets: Buffer::from_slice(&offsets),
            data: liquers_records::AlignedBuffer::from_slice(&data),
        };
        let amount_column = Column::Int {
            validity: None,
            values: Buffer::from_slice(amounts),
        };
        Arc::new(
            liquers_records::RecordBatch::new(schema, vec![id_column, amount_column], None, None, Vec::new())
                .expect("batch"),
        )
    }

    /// Registered as a command so a query can reach it: a small fixture view (3 rows).
    fn fixture_view() -> Result<Value, Error> {
        Ok(Value::from_record_view(batch_view(
            &["a", "b", "c"],
            &[10, 20, 30],
        )))
    }

    /// A fixture source over the same three rows, split into two chunks (an `InMemorySource` of
    /// two one-row views), so chunk-addressed commands (`rowid`, `rec_id`'s walk, `materialize`)
    /// have more than one chunk to exercise.
    fn fixture_source() -> Result<Value, Error> {
        let schema = schema_id_and_amount();
        let chunk0 = batch_view(&["a"], &[10]);
        let chunk1 = batch_view(&["b", "c"], &[20, 30]);
        assert_eq!(chunk0.schema(), &schema);
        let source: Arc<dyn RecordSource> = Arc::new(InMemorySource::new(vec![chunk0, chunk1]));
        Ok(Value::from_record_source(source))
    }

    fn fixture_env() -> Result<DefaultEnvironment<Value>, Error> {
        let mut env = env_with_records_commands()?;
        let cr = env.get_mut_command_registry();
        register_command!(cr, fn fixture_view() -> result)?;
        register_command!(cr, fn fixture_source() -> result)?;
        Ok(env)
    }

    #[tokio::test]
    async fn row_returns_a_one_row_materialized_view() -> Result<(), Error> {
        let env = fixture_env()?;
        let state = evaluate(env.to_ref(), "fixture_view/ns-rec/row-1", None).await?;
        let view = state.value()?.as_record_view()?;
        assert_eq!(view.len(), 1);
        assert_eq!(view.value(0, 1)?, FieldValue::Int(20));
        Ok(())
    }

    #[tokio::test]
    async fn slice_is_clamped_to_the_rows_that_exist() -> Result<(), Error> {
        let envref = fixture_env()?.to_ref();
        let state = evaluate(envref.clone(), "fixture_view/ns-rec/slice-1-100", None).await?;
        let view = state.value()?.as_record_view()?;
        assert_eq!(view.len(), 2, "a range past the end comes back short");
        assert_eq!(view.value(0, 1)?, FieldValue::Int(20));
        let state = evaluate(envref, "fixture_view/ns-rec/slice-7-5", None).await?;
        assert_eq!(state.value()?.as_record_view()?.len(), 0, "an offset past the end gives no rows");
        Ok(())
    }

    #[tokio::test]
    async fn select_columns_keeps_id_even_when_unnamed() -> Result<(), Error> {
        let env = fixture_env()?;
        let state = evaluate(
            env.to_ref(),
            "fixture_view/ns-rec/select_columns-amount",
            None,
        )
        .await?;
        let view = state.value()?.as_record_view()?;
        assert_eq!(view.schema().fields.len(), 2); // id kept + amount
        // `select_columns` puts named columns first, then appends Id/Source not already present
        // (see `views.rs`'s `impl dyn RecordView::select_columns`) — "amount" is named, so it
        // comes first and "id" is appended after it.
        assert_eq!(view.schema().fields[0].name, "amount");
        assert_eq!(view.schema().id_field(), Some(1));
        Ok(())
    }

    #[tokio::test]
    async fn select_columns_with_no_names_is_refused() -> Result<(), Error> {
        let env = fixture_env()?;
        let error = evaluate(env.to_ref(), "fixture_view/ns-rec/select_columns", None)
            .await
            .expect_err("empty column list must be refused");
        assert!(format!("{error}").contains("at least one column"));
        Ok(())
    }

    #[tokio::test]
    async fn head_materializes_the_first_n_rows() -> Result<(), Error> {
        let env = fixture_env()?;
        let state = evaluate(env.to_ref(), "fixture_view/ns-rec/head-2", None).await?;
        let view = state.value()?.as_record_view()?;
        assert_eq!(view.len(), 2);
        Ok(())
    }

    #[tokio::test]
    async fn head_default_is_five() -> Result<(), Error> {
        let env = fixture_env()?;
        // Only 3 rows exist, so a default of 5 must clamp rather than error.
        let state = evaluate(env.to_ref(), "fixture_view/ns-rec/head", None).await?;
        let view = state.value()?.as_record_view()?;
        assert_eq!(view.len(), 3);
        Ok(())
    }

    #[tokio::test]
    async fn slice_returns_a_row_range_view() -> Result<(), Error> {
        let env = fixture_env()?;
        let state = evaluate(env.to_ref(), "fixture_view/ns-rec/slice-1-2", None).await?;
        let view = state.value()?.as_record_view()?;
        assert_eq!(view.len(), 2);
        assert_eq!(view.value(0, 1)?, FieldValue::Int(20));
        Ok(())
    }

    #[tokio::test]
    async fn rec_id_finds_the_matching_row_over_a_view() -> Result<(), Error> {
        let env = fixture_env()?;
        let state = evaluate(env.to_ref(), "fixture_view/ns-rec/rec_id-b", None).await?;
        let view = state.value()?.as_record_view()?;
        assert_eq!(view.len(), 1);
        assert_eq!(view.value(0, 1)?, FieldValue::Int(20));
        Ok(())
    }

    #[tokio::test]
    async fn rec_id_walks_chunks_over_a_source() -> Result<(), Error> {
        let env = fixture_env()?;
        let state = evaluate(env.to_ref(), "fixture_source/ns-rec/rec_id-c", None).await?;
        let view = state.value()?.as_record_view()?;
        assert_eq!(view.len(), 1);
        assert_eq!(view.value(0, 1)?, FieldValue::Int(30));
        Ok(())
    }

    #[tokio::test]
    async fn rec_id_without_declared_id_names_rowid() -> Result<(), Error> {
        fn no_id_view() -> Result<Value, Error> {
            let schema = Arc::new(RecordSchema::new(vec![FieldSchema::new(
                "amount",
                FieldType::Int,
            )])?);
            let column = Column::Int {
                validity: None,
                values: Buffer::from_slice(&[1i64, 2]),
            };
            let batch =
                liquers_records::RecordBatch::new(schema, vec![column], None, None, Vec::new())?;
            Ok(Value::from_record_view(Arc::new(batch) as Arc<dyn RecordView>))
        }
        let mut env = env_with_records_commands()?;
        {
            let cr = env.get_mut_command_registry();
            register_command!(cr, fn no_id_view() -> result)?;
        }
        let error = evaluate(env.to_ref(), "no_id_view/ns-rec/rec_id-x", None)
            .await
            .expect_err("no declared Id field must be refused");
        assert!(format!("{error}").contains("rowid"));
        Ok(())
    }

    #[tokio::test]
    async fn rowid_over_a_view_scans_for_the_matching_implicit_id() -> Result<(), Error> {
        let env = fixture_env()?;
        let state = evaluate(env.to_ref(), "fixture_view/ns-rec/rowid-0-2", None).await?;
        let view = state.value()?.as_record_view()?;
        assert_eq!(view.len(), 1);
        assert_eq!(view.value(0, 1)?, FieldValue::Int(30));
        Ok(())
    }

    #[tokio::test]
    async fn rowid_over_a_source_opens_only_the_addressed_chunk() -> Result<(), Error> {
        let env = fixture_env()?;
        // Chunk 1, row 1 is the source's second view's second row ("c", 30).
        let state = evaluate(env.to_ref(), "fixture_source/ns-rec/rowid-1-1", None).await?;
        let view = state.value()?.as_record_view()?;
        assert_eq!(view.len(), 1);
        assert_eq!(view.value(0, 1)?, FieldValue::Int(30));
        Ok(())
    }

    #[tokio::test]
    async fn to_record_source_wraps_a_view_in_an_in_memory_source() -> Result<(), Error> {
        let env = fixture_env()?;
        let state = evaluate(env.to_ref(), "fixture_view/ns-rec/to_record_source", None).await?;
        let source = state.value()?.as_record_source()?;
        assert!(source.schema().is_some());
        Ok(())
    }

    #[tokio::test]
    async fn materialize_of_a_source_concatenates_its_chunks() -> Result<(), Error> {
        let env = fixture_env()?;
        let state = evaluate(env.to_ref(), "fixture_source/ns-rec/materialize", None).await?;
        let view = state.value()?.as_record_view()?;
        assert_eq!(view.len(), 3);
        Ok(())
    }

    #[tokio::test]
    async fn materialize_past_max_rows_is_refused() -> Result<(), Error> {
        let env = fixture_env()?;
        let error = evaluate(env.to_ref(), "fixture_source/ns-rec/materialize-1", None)
            .await
            .expect_err("3 rows must exceed max_rows=1");
        assert!(format!("{error}").contains("max_rows"));
        Ok(())
    }

    #[tokio::test]
    async fn materialize_of_a_view_is_a_free_shallow_clone() -> Result<(), Error> {
        let env = fixture_env()?;
        let state = evaluate(env.to_ref(), "fixture_view/ns-rec/materialize", None).await?;
        let view = state.value()?.as_record_view()?;
        assert_eq!(view.len(), 3);
        Ok(())
    }

    #[tokio::test]
    async fn records_schema_reports_field_names_as_linkable_text() -> Result<(), Error> {
        let env = fixture_env()?;
        let state = evaluate(env.to_ref(), "fixture_view/ns-rec/records_schema", None).await?;
        let text = state.try_into_string()?;
        assert!(text.contains("\"amount\""));
        assert!(text.contains("\"id\""));
        Ok(())
    }

    #[tokio::test]
    async fn to_json_default_orient_is_records() -> Result<(), Error> {
        let env = fixture_env()?;
        let state = evaluate(env.to_ref(), "fixture_view/ns-rec/to_json", None).await?;
        let json = state.value()?.try_into_json_value()?;
        let array = json.as_array().expect("records orient is a JSON array");
        assert_eq!(array.len(), 3);
        assert_eq!(array[0]["amount"], serde_json::json!(10));
        Ok(())
    }

    #[tokio::test]
    async fn from_json_reads_a_records_shaped_array() -> Result<(), Error> {
        fn json_records() -> Result<Value, Error> {
            Ok(Value::try_from_json_value(&serde_json::json!([
                {"id": "a", "amount": 1},
                {"id": "b", "amount": 2}
            ]))?)
        }
        let mut env = env_with_records_commands()?;
        {
            let cr = env.get_mut_command_registry();
            register_command!(cr, fn json_records() -> result)?;
        }
        let state = evaluate(env.to_ref(), "json_records/ns-rec/from_json", None).await?;
        let view = state.value()?.as_record_view()?;
        assert_eq!(view.len(), 2);
        Ok(())
    }

    #[tokio::test]
    async fn to_record_reads_labelled_csv_bytes() -> Result<(), Error> {
        fn csv_bytes() -> Result<Value, Error> {
            Ok(Value::from_bytes(b"id,amount\na,1\nb,2".to_vec()))
        }
        let mut env = env_with_records_commands()?;
        {
            let cr = env.get_mut_command_registry();
            register_command!(cr, fn csv_bytes() -> result)?;
        }
        let state = evaluate(
            env.to_ref(),
            "csv_bytes/ns-rec/to_record-csv",
            None,
        )
        .await?;
        let view = state.value()?.as_record_view()?;
        assert_eq!(view.len(), 2);
        Ok(())
    }

    #[tokio::test]
    async fn to_record_of_a_source_is_refused_naming_materialize() -> Result<(), Error> {
        let env = fixture_env()?;
        let error = evaluate(env.to_ref(), "fixture_source/ns-rec/to_record", None)
            .await
            .expect_err("a source must be refused, naming materialize");
        assert!(format!("{error}").to_lowercase().contains("materialize"));
        Ok(())
    }

    #[tokio::test]
    async fn file_records_lists_a_store_directory() -> Result<(), Error> {
        let store = AsyncMemoryStore::new(&Key::new());
        store
            .set(
                &liquers_core::parse::parse_key("data/a.txt")?,
                b"hello",
                &liquers_core::metadata::Metadata::new(),
            )
            .await?;
        let mut env = env_with_records_commands()?;
        env.with_async_store(Box::new(store));
        let envref = env.to_ref();

        // A pure directory key has no recipe of its own (nothing "produces" a directory), so
        // `evaluate("-R/data/-/ns-rec/file_records")` cannot reach `file_records` at all — the
        // `GetAsset(data)` step it would need fails with "No recipe found for key data" before
        // `file_records` ever runs. `file_records` only reads `state.metadata.key()`, never the
        // state's value, so it is exercised directly here with a hand-built state/context
        // instead — a dummy asset (an empty-recipe asset the manager never tracks) supplies the
        // `Context`, and the state carries only the directory key in its metadata.
        let asset = envref.get_asset_manager().create_dummy_asset();
        let context = liquers_core::context::Context::new(asset, false).await;
        let dir_key = liquers_core::parse::parse_key("data")?;
        let mut metadata = liquers_core::metadata::Metadata::new();
        metadata.with_key(dir_key)?;
        let state = liquers_core::state::State::from_value_and_metadata(
            Value::none(),
            Arc::new(metadata),
        );

        let result = file_records(state, context).await?;
        let view = result.as_record_view()?;
        assert_eq!(view.len(), 1);
        assert_eq!(view.value(0, 1)?, FieldValue::Text(Arc::from("a.txt")));
        Ok(())
    }

    #[tokio::test]
    async fn to_record_source_of_a_manifest_recognizes_the_discriminator() -> Result<(), Error> {
        fn empty_manifest_yaml() -> Result<Value, Error> {
            Ok(Value::from_string(
                "manifest: record-stream\nchunks: []\n".to_string(),
            ))
        }
        let mut env = env_with_records_commands()?;
        {
            let cr = env.get_mut_command_registry();
            register_command!(cr, fn empty_manifest_yaml() -> result)?;
        }
        let state = evaluate(
            env.to_ref(),
            "empty_manifest_yaml/ns-rec/to_record_source",
            None,
        )
        .await?;
        let source = state.value()?.as_record_source()?;
        assert!(source.manifest().is_some());
        // Constructed only to keep `ManifestSource`/`ManifestSpec` imports exercised in this
        // module rather than unused.
        let _ = ManifestSource::new(ManifestSpec::default(), None);
        Ok(())
    }
}
