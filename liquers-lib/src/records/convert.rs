//! `to_record` and `to_record_source`: the two async helpers every `ns-rec` command applies to
//! its input, and that a query can also call explicitly. See
//! `specs/design/record-streams/phase2-architecture.md` §"What a record command accepts".
//!
//! Dispatch runs on [`liquers_core::value::ValueInterface::identifier`] rather than on a `match`
//! over `Value`'s own enum: `Value` is `CombinedValue<SimpleValue, ExtValue>`, and neither side is
//! owned by this module, so there is no local enum to match exhaustively over in the first place.
//! `identifier()` is the same string the rest of the codebase already uses to name a value's
//! runtime type in error messages (`Error::conversion_error`), so reusing it here does not add a
//! second naming scheme.

use std::sync::Arc;

use liquers_core::context::{Context, Environment};
use liquers_core::error::{Error, ErrorType};
use liquers_core::metadata::Metadata;
use liquers_core::query::{Key, Query};
use liquers_core::value::ValueInterface;

use liquers_records::{
    from_json, read_table, InMemorySource, JsonOrient, ManifestSource, ManifestSpec, ReadOptions,
    ReadSchema, RecordSchema, RecordSource, RecordView, TableFormat,
};

use crate::value::{ExtValueInterface, Value};

/// Options `to_record`/`to_record_source` take, and that a `ns-rec` command's own arguments are
/// translated into before calling them.
#[derive(Debug, Clone, Default)]
pub struct ToRecordOptions {
    /// The table format to read bytes/text as. `None` falls back to the state's metadata — the
    /// format is never sniffed from the content (phase2-architecture.md §"The format is never
    /// sniffed").
    pub format: Option<String>,
    /// Whether the first row of a CSV/TSV table is a header. `None` defaults to `true`.
    pub header: Option<bool>,
    /// A schema to read with. `Some` makes the read schema-aware and strict (nothing guessed);
    /// `None` infers one from the data.
    pub schema: Option<Arc<RecordSchema>>,
    /// Reserved for the commands built on top of these helpers (`materialize`'s own `max_rows`) —
    /// `to_record`/`to_record_source` do not themselves walk a stream, so neither reads it.
    pub max_rows: Option<usize>,
}

/// Turns `bytes` into a table, per §"What a record command accepts": the format comes from
/// `options.format`, else from `metadata`'s own data format — never sniffed — and the schema comes
/// from `options.schema` when given, inferred otherwise. Parquet is the one format `read_table`
/// itself refuses (`liquers-records` never reads Parquet — Tier 3): it goes through
/// `crate::records::read_parquet_record_batch` instead, matched explicitly alongside every other
/// `TableFormat` variant so adding one is a compile error here too (CLAUDE.md's "Match
/// Statements").
fn read_table_from_bytes(
    bytes: &[u8],
    metadata: &Metadata,
    options: &ToRecordOptions,
) -> Result<Arc<dyn RecordView>, Error> {
    let format_name = options
        .format
        .clone()
        .unwrap_or_else(|| metadata.get_data_format());
    let format = TableFormat::from_data_format(&format_name)?;
    let read_options = ReadOptions {
        header: options.header.unwrap_or(true),
    };
    let read_schema = match &options.schema {
        Some(schema) => ReadSchema::Declared(schema.as_ref()),
        None => ReadSchema::Infer,
    };
    let batch = match format {
        TableFormat::Parquet => crate::records::read_parquet_record_batch(bytes, read_schema)?,
        TableFormat::Csv { separator } => {
            read_table(bytes, TableFormat::Csv { separator }, read_schema, &read_options)?
        }
        TableFormat::NdJson => read_table(bytes, TableFormat::NdJson, read_schema, &read_options)?,
        TableFormat::Json => read_table(bytes, TableFormat::Json, read_schema, &read_options)?,
        TableFormat::Markdown => read_table(bytes, TableFormat::Markdown, read_schema, &read_options)?,
        TableFormat::Html => read_table(bytes, TableFormat::Html, read_schema, &read_options)?,
        TableFormat::Ipc => read_table(bytes, TableFormat::Ipc, read_schema, &read_options)?,
    };
    Ok(Arc::new(batch) as Arc<dyn RecordView>)
}

/// Turns a parsed JSON `value` into a table with `from_json(.., JsonOrient::Auto, ..)`, schema-aware
/// when `options.schema` is given.
fn record_from_json_value(
    value: &serde_json::Value,
    options: &ToRecordOptions,
) -> Result<Arc<dyn RecordView>, Error> {
    let batch = match &options.schema {
        Some(schema) => from_json(value, JsonOrient::Auto, ReadSchema::Declared(schema.as_ref()))?,
        None => from_json(value, JsonOrient::Auto, ReadSchema::Infer)?,
    };
    Ok(Arc::new(batch) as Arc<dyn RecordView>)
}

/// Whether a parsed YAML/JSON document's top-level mapping carries the manifest discriminator
/// (`manifest: record-stream`). Checked on the raw document, not by attempting to deserialize it
/// as a [`ManifestSpec`] directly: `ManifestSpec`'s own `manifest` field is `#[serde(default)]`,
/// so a document with no `manifest` key at all would otherwise deserialize as if it had the
/// discriminator (see `liquers-records/src/manifest.rs`'s `ManifestKind::default`).
fn yaml_has_manifest_discriminator(text: &str) -> bool {
    match serde_yaml::from_str::<serde_yaml::Value>(text) {
        Ok(document) => document.get("manifest").and_then(|value| value.as_str())
            == Some("record-stream"),
        Err(_) => false,
    }
}

/// The JSON-value counterpart of [`yaml_has_manifest_discriminator`] — used for a structured
/// `SimpleValue::Array`/`SimpleValue::Object` rather than text/bytes (Step 0.2: a stored
/// `*.manifest.yaml` with no type metadata loads as a structured value, not text).
fn json_has_manifest_discriminator(value: &serde_json::Value) -> bool {
    value.get("manifest").and_then(|value| value.as_str()) == Some("record-stream")
}

/// Any input a table can be made from, turned into a [`RecordView`] — the async helper every
/// `ns-rec` command applies to its state, per phase2-architecture.md §"What a record command
/// accepts".
pub async fn to_record(
    value: &Value,
    metadata: &Metadata,
    options: &ToRecordOptions,
    context: &Context<impl Environment<Value = Value>>,
) -> Result<Arc<dyn RecordView>, Error> {
    match value.identifier().as_ref() {
        "RecordView" => value.as_record_view(),
        "RecordSource" => Err(Error::conversion_error_with_message(
            "RecordSource",
            "RecordView",
            "a source is not a table; materialize it first with ns-rec/materialize",
        )),
        "Bytes" => {
            let bytes = value.try_into_bytes()?;
            read_table_from_bytes(&bytes, metadata, options)
        }
        "Text" => {
            let text = value.try_into_string()?;
            read_table_from_bytes(text.as_bytes(), metadata, options)
        }
        "Array" | "Object" => {
            let json = value.try_into_json_value()?;
            record_from_json_value(&json, options)
        }
        "Key" => {
            let key = value.try_into_key()?;
            let state = context.get_dependency_state(&Query::from(key)).await?;
            Box::pin(to_record(
                state.data_unchecked(),
                &state.metadata,
                options,
                context,
            ))
            .await
        }
        other => Err(Error::conversion_error(other.to_string(), "RecordView")),
    }
}

/// Any input a source can be made from, turned into a [`RecordSource`] — the async helper every
/// `ns-rec` command applies to its state, per phase2-architecture.md §"What a record command
/// accepts" and §"Getting a source from a manifest file".
pub async fn to_record_source(
    value: &Value,
    metadata: &Metadata,
    options: &ToRecordOptions,
    context: &Context<impl Environment<Value = Value>>,
) -> Result<Arc<dyn RecordSource>, Error> {
    match value.identifier().as_ref() {
        "RecordSource" => {
            let source = value.as_record_source()?;
            match source.manifest() {
                // A keyless stored manifest — what `deserialize_from_bytes` returns for a
                // `type_identifier: RecordSource`, since it receives no metadata — is re-keyed
                // from this state's own metadata, not taken as-is (phase2-architecture.md
                // §"Getting a source from a manifest file"): otherwise a manifest written by
                // Liquers and read back would silently treat its keyed chunks as unkeyed.
                //
                // Re-keyed only when this state names a manifest key: a source handed on from a
                // previous command (`…/ns-rec/to_record_source/-/ns-rec/…`) arrives with the
                // evaluating asset's metadata, which names no key, and must keep the one it has.
                Some(spec) => match manifest_key(metadata)? {
                    Some(key) => {
                        let rekeyed = ManifestSource::new(spec.clone(), Some(key))?;
                        Ok(Arc::new(rekeyed) as Arc<dyn RecordSource>)
                    }
                    None => Ok(source),
                },
                None => Ok(source),
            }
        }
        "RecordView" => {
            let view = value.as_record_view()?;
            Ok(Arc::new(InMemorySource::new(vec![view])) as Arc<dyn RecordSource>)
        }
        "Bytes" => {
            let bytes = value.try_into_bytes()?;
            bytes_to_record_source(&bytes, value, metadata, options, context).await
        }
        "Text" => {
            let text = value.try_into_string()?;
            bytes_to_record_source(text.as_bytes(), value, metadata, options, context).await
        }
        "Array" | "Object" => {
            let json = value.try_into_json_value()?;
            if json_has_manifest_discriminator(&json) {
                let spec: ManifestSpec = serde_json::from_value(json)
                    .map_err(|error| Error::from_error(ErrorType::SerializationError, error))?;
                manifest_source(spec, metadata)
            } else {
                let view = to_record(value, metadata, options, context).await?;
                Ok(Arc::new(InMemorySource::new(vec![view])) as Arc<dyn RecordSource>)
            }
        }
        "Key" => {
            let key = value.try_into_key()?;
            let state = context.get_dependency_state(&Query::from(key)).await?;
            Box::pin(to_record_source(
                state.data_unchecked(),
                &state.metadata,
                options,
                context,
            ))
            .await
        }
        other => Err(Error::conversion_error(other.to_string(), "RecordSource")),
    }
}

/// Shared by `to_record_source`'s `"Bytes"`/`"Text"` arms: a manifest when `bytes` (as UTF-8 text)
/// carries the discriminator, otherwise a table read the way `to_record` would, wrapped in an
/// [`InMemorySource`].
async fn bytes_to_record_source(
    bytes: &[u8],
    value: &Value,
    metadata: &Metadata,
    options: &ToRecordOptions,
    context: &Context<impl Environment<Value = Value>>,
) -> Result<Arc<dyn RecordSource>, Error> {
    if let Ok(text) = std::str::from_utf8(bytes) {
        if yaml_has_manifest_discriminator(text) {
            let spec: ManifestSpec = serde_yaml::from_str(text)
                .map_err(|error| Error::from_error(ErrorType::SerializationError, error))?;
            return manifest_source(spec, metadata);
        }
    }
    let view = to_record(value, metadata, options, context).await?;
    Ok(Arc::new(InMemorySource::new(vec![view])) as Arc<dyn RecordSource>)
}

/// The filename suffix that makes a stored document a manifest `ManifestRecipeProvider` serves.
const MANIFEST_SUFFIX: &str = ".manifest.yaml";

/// The key a manifest read from this state lives at — the state's metadata key, **only** when its
/// filename ends in `.manifest.yaml`.
///
/// The key names the chunk keys' folder and prefix, and those keys are served by
/// `ManifestRecipeProvider`, which reads `*.manifest.yaml` only. A manifest stored under any other
/// name (`data/sales.yaml`) would name chunk keys (`data/sales.yaml_0000.csv`) nothing can ever
/// serve, so it is treated as keyless: its chunks are unkeyed queries.
fn manifest_key(metadata: &Metadata) -> Result<Option<Key>, Error> {
    Ok(metadata.key()?.filter(|key| {
        key.filename()
            .is_some_and(|name| name.encode().ends_with(MANIFEST_SUFFIX))
    }))
}

/// A [`ManifestSource`] for `spec`, keyed by [`manifest_key`].
///
/// A keyless manifest declaring per-chunk or shared `arguments`/`links` is refused here: its
/// chunks are unkeyed queries, which have no recipe for arguments to act through, so they would be
/// silently dropped (phase2-architecture.md §"A. Chunk keys": such arguments are refused "at the
/// latest when a stream is opened on a manifest that is still keyless"). This is the last point a
/// key can be supplied, so keylessness is final here. `ManifestSpec::check_unkeyed_chunk_arguments`
/// does not cover it — it counts a chunk whose query ends in a filename as keyed whether or not the
/// manifest has a key.
fn manifest_source(spec: ManifestSpec, metadata: &Metadata) -> Result<Arc<dyn RecordSource>, Error> {
    let key = manifest_key(metadata)?;
    if key.is_none() {
        refuse_keyless_arguments(&spec)?;
    }
    Ok(Arc::new(ManifestSource::new(spec, key)?) as Arc<dyn RecordSource>)
}

fn refuse_keyless_arguments(spec: &ManifestSpec) -> Result<(), Error> {
    let why = "but the manifest has no key (it was not read from a `*.manifest.yaml` file), so its \
               chunks are unkeyed queries and the arguments could not be applied";
    if !spec.arguments.is_empty() || !spec.links.is_empty() {
        return Err(Error::general_error(format!(
            "manifest: shared arguments or links are declared, {why}"
        )));
    }
    for chunk in &spec.chunks {
        if !chunk.arguments.is_empty() || !chunk.links.is_empty() {
            return Err(Error::general_error(format!(
                "manifest: chunk \"{}\" declares per-chunk arguments or links, {why}",
                chunk.query
            )));
        }
    }
    Ok(())
}
