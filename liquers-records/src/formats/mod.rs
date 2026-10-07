//! Table formats: `TableFormat`, `ReadSchema`, `ReadOptions`/`WriteOptions`, and the two entry
//! points every format goes through — [`read_table`] and [`write_table`]. Which reader runs
//! depends only on whether a schema is available (`ReadSchema`), not on the format; which format
//! runs depends only on `TableFormat`, never on the file's own hints.
//!
//! See `specs/design/record-streams/phase2-architecture.md`, §"Table formats: what a `RecordView`
//! writes and reads", §"Two readers: schema-aware and schema-less" and §"Construction helpers,
//! options and the provider chain".
//!
//! Every variant Phase 2 lists is present in every feature configuration: a `TableFormat` value
//! and `from_data_format`'s alias table are part of Phase 2's stable surface, and the variant
//! itself carries no dependency on `flatbuffers`/`flate2` — only its reader/writer does. `Ipc`
//! reads and writes, and `Parquet` writes, only with this crate's `ipc`/`parquet` features; without
//! them [`read_table`]/[`write_table`] refuse with a typed
//! [`liquers_core::error::Error::not_supported`], naming the format and the direction. `Html` is
//! write-only, and `Parquet` is never read here (`liquers-lib`'s polars bridge reads it).

pub mod csv;
pub mod html;
pub mod infer;
#[cfg(feature = "ipc")]
mod ipc;
pub mod markdown;
pub mod ndjson;
#[cfg(feature = "parquet")]
mod parquet;
pub mod shapes;
#[cfg(feature = "parquet")]
mod thrift;

use liquers_core::error::Error;

use crate::batch::{RecordBatch, RecordView};
use crate::schema::RecordSchema;

/// Where a schema for reading comes from — the one thing that decides which of the two readers
/// runs. See phase2-architecture.md §"Two readers: schema-aware and schema-less".
#[derive(Debug, Clone, Copy)]
pub enum ReadSchema<'a> {
    /// A schema is known: parse every cell as its declared type. Nothing is guessed.
    Declared(&'a RecordSchema),
    /// No schema: infer one from the data, per `formats::infer`'s rules.
    Infer,
}

/// Options every reader takes. `header` is hand-defaulted to `true` below — a derived `Default`
/// on a bare `bool` field would silently give `false`.
#[derive(Debug, Clone)]
pub struct ReadOptions {
    pub header: bool,
}

impl Default for ReadOptions {
    fn default() -> Self {
        ReadOptions { header: true }
    }
}

/// Options every writer takes. See [`ReadOptions`] for why `Default` is hand-written.
#[derive(Debug, Clone)]
pub struct WriteOptions {
    pub header: bool,
}

impl Default for WriteOptions {
    fn default() -> Self {
        WriteOptions { header: true }
    }
}

/// The concrete table format a [`read_table`]/[`write_table`] call names. `Csv`'s `separator`
/// covers TSV too (`b'\t'`) — one implementation, two formats, per phase2-architecture.md
/// §"Tier 1 — in `records`, no new dependency".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableFormat {
    Csv { separator: u8 },
    NdJson,
    Json,
    Markdown,
    Html,
    /// Arrow IPC file (Feather v2). Read and written behind the crate's `ipc` feature; the
    /// variant itself needs no `flatbuffers` dependency.
    Ipc,
    /// Parquet. Written behind the crate's `parquet` feature, never read here; the variant itself
    /// needs no `flate2` dependency.
    Parquet,
}

impl TableFormat {
    /// `csv`, `csv:comma`, `tsv`, `csv:tab`, `ndjson`, `jsonl`, `json`, `md`, `markdown`, `html`,
    /// `feather`/`ipc`/`arrow_ipc`/`arrow`, `parquet`. Anything else is refused, naming the format.
    /// Names and aliases match the DataFrame serializer's (`liquers-lib/src/polars/serde.rs`), so a
    /// filename means the same thing for a `polars_dataframe` and a `RecordView`.
    pub fn from_data_format(data_format: &str) -> Result<TableFormat, Error> {
        match data_format {
            "csv" | "csv:comma" => Ok(TableFormat::Csv { separator: b',' }),
            "tsv" | "csv:tab" => Ok(TableFormat::Csv { separator: b'\t' }),
            "ndjson" | "jsonl" => Ok(TableFormat::NdJson),
            "json" => Ok(TableFormat::Json),
            "md" | "markdown" => Ok(TableFormat::Markdown),
            "html" => Ok(TableFormat::Html),
            "feather" | "ipc" | "arrow_ipc" | "arrow" => Ok(TableFormat::Ipc),
            "parquet" => Ok(TableFormat::Parquet),
            other => Err(Error::general_error(format!(
                "TableFormat::from_data_format: unknown data format '{other}'"
            ))),
        }
    }
}

/// The refusal for a format whose reader/writer this build does not enable (`Ipc`/`Parquet`
/// without the `ipc`/`parquet` features) — never a silent `_ =>` fallthrough, since every
/// [`TableFormat`] variant still gets its own match arm in [`read_table`]/[`write_table`].
#[cfg(any(not(feature = "ipc"), not(feature = "parquet")))]
fn feature_disabled(format: &str, direction: &str) -> Error {
    Error::not_supported(format!(
        "TableFormat::{format}: {direction} needs liquers-records' '{}' feature, which this build \
         does not enable",
        format.to_lowercase()
    ))
}

/// Parses `bytes` as `format` into a [`RecordBatch`]. Which reader runs — schema-aware and strict,
/// or schema-less and inferring — depends only on `schema`. See phase2-architecture.md §"Two
/// readers: schema-aware and schema-less".
///
/// Warnings the reader collects (padded short CSV rows) go to stderr, since there is no log here;
/// [`read_table_with_report`] returns them instead.
pub fn read_table(
    bytes: &[u8],
    format: TableFormat,
    schema: ReadSchema<'_>,
    options: &ReadOptions,
) -> Result<RecordBatch, Error> {
    let (batch, report) = read_table_with_report(bytes, format, schema, options)?;
    for warning in &report.warnings {
        eprintln!("read_table: {warning}");
    }
    Ok(batch)
}

/// What a read noticed but did not refuse — padded short CSV rows, so far. Empty for a clean read.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ReadReport {
    pub warnings: Vec<String>,
}

/// [`read_table`], also returning the warnings the reader collected instead of writing them to
/// stderr. Use it where there is a log to put them in — a command's `Context::warning`.
pub fn read_table_with_report(
    bytes: &[u8],
    format: TableFormat,
    schema: ReadSchema<'_>,
    options: &ReadOptions,
) -> Result<(RecordBatch, ReadReport), Error> {
    let clean = |batch: RecordBatch| (batch, ReadReport::default());
    match format {
        TableFormat::Csv { separator } => csv::read_csv(bytes, separator, schema, options),
        TableFormat::NdJson => ndjson::read_ndjson(bytes, schema, options).map(clean),
        TableFormat::Json => ndjson::read_json(bytes, schema, options).map(clean),
        TableFormat::Markdown => markdown::read_markdown(bytes, schema, options).map(clean),
        TableFormat::Html => Err(Error::not_supported(
            "TableFormat::Html: reading is not supported (write-only format)".to_string()
        )),
        #[cfg(feature = "ipc")]
        TableFormat::Ipc => ipc::read_ipc(bytes, schema).map(clean),
        #[cfg(not(feature = "ipc"))]
        TableFormat::Ipc => Err(feature_disabled("Ipc", "reading")),
        // Unlike `Ipc` without its feature, this is not a disabled feature — `liquers-records`
        // never reads Parquet, in any feature configuration (phase2-architecture.md §"Tier 3 —
        // Parquet: writing is cheap, reading is not"). A real-world Parquet file routinely uses
        // dictionary encoding, snappy/zstd compression and data page v2; reading goes through
        // `liquers-lib`'s polars bridge, which delegates to a real, general reader instead of one
        // built to handle only this crate's own output.
        TableFormat::Parquet => Err(Error::not_supported(
            "TableFormat::Parquet: liquers-records does not read Parquet; read it through \
             liquers-lib's polars bridge (the 'polars' feature)"
                .to_string(),
        )),
    }
}

/// Serializes `view` as `format`.
pub fn write_table(
    view: &dyn RecordView,
    format: TableFormat,
    options: &WriteOptions,
) -> Result<Vec<u8>, Error> {
    match format {
        TableFormat::Csv { separator } => csv::write_csv(view, separator, options),
        TableFormat::NdJson => ndjson::write_ndjson(view, options),
        TableFormat::Json => ndjson::write_json(view, options),
        TableFormat::Markdown => markdown::write_markdown(view, options),
        TableFormat::Html => html::write_html(view, options),
        #[cfg(feature = "ipc")]
        TableFormat::Ipc => ipc::write_ipc(view),
        #[cfg(not(feature = "ipc"))]
        TableFormat::Ipc => Err(feature_disabled("Ipc", "writing")),
        #[cfg(feature = "parquet")]
        TableFormat::Parquet => parquet::write_parquet(view),
        #[cfg(not(feature = "parquet"))]
        TableFormat::Parquet => Err(feature_disabled("Parquet", "writing")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_format_from_data_format_resolves_every_alias() -> Result<(), Error> {
        assert_eq!(TableFormat::from_data_format("csv")?, TableFormat::Csv { separator: b',' });
        assert_eq!(TableFormat::from_data_format("csv:comma")?, TableFormat::Csv { separator: b',' });
        assert_eq!(TableFormat::from_data_format("tsv")?, TableFormat::Csv { separator: b'\t' });
        assert_eq!(TableFormat::from_data_format("csv:tab")?, TableFormat::Csv { separator: b'\t' });
        assert_eq!(TableFormat::from_data_format("ndjson")?, TableFormat::NdJson);
        assert_eq!(TableFormat::from_data_format("jsonl")?, TableFormat::NdJson);
        assert_eq!(TableFormat::from_data_format("json")?, TableFormat::Json);
        assert_eq!(TableFormat::from_data_format("md")?, TableFormat::Markdown);
        assert_eq!(TableFormat::from_data_format("markdown")?, TableFormat::Markdown);
        assert_eq!(TableFormat::from_data_format("html")?, TableFormat::Html);
        Ok(())
    }

    #[test]
    fn table_format_from_data_format_refuses_unknown_names() {
        assert!(TableFormat::from_data_format("xlsx").is_err());
    }

    #[test]
    fn read_and_write_options_default_header_to_true() {
        // The pitfall this guards: a derived `Default` on a bare `bool` would give `false`.
        assert!(ReadOptions::default().header);
        assert!(WriteOptions::default().header);
    }

    // --- Additional coverage beyond Phase 3 §3.6 ---

    #[test]
    fn table_format_from_data_format_resolves_ipc_and_parquet_aliases() -> Result<(), Error> {
        assert_eq!(TableFormat::from_data_format("feather")?, TableFormat::Ipc);
        assert_eq!(TableFormat::from_data_format("ipc")?, TableFormat::Ipc);
        assert_eq!(TableFormat::from_data_format("arrow_ipc")?, TableFormat::Ipc);
        assert_eq!(TableFormat::from_data_format("arrow")?, TableFormat::Ipc);
        assert_eq!(TableFormat::from_data_format("parquet")?, TableFormat::Parquet);
        Ok(())
    }

    #[test]
    fn read_table_of_an_unimplemented_format_is_not_supported_not_a_panic() {
        // Parquet reading is never implemented here, in any feature configuration: it is read
        // through liquers-lib's polars bridge instead.
        let error = read_table(b"", TableFormat::Parquet, ReadSchema::Infer, &ReadOptions::default())
            .expect_err("liquers-records never reads Parquet");
        let message = format!("{error}").to_lowercase();
        assert!(message.contains("parquet"));
        assert!(message.contains("polars"));
    }

    #[test]
    #[cfg(not(feature = "parquet"))]
    fn write_table_of_an_unimplemented_format_is_not_supported_not_a_panic() {
        use crate::mutable::RecordBatchMut;
        use crate::schema::{FieldSchema, FieldType, RecordSchema};
        use std::sync::Arc;

        let schema =
            Arc::new(RecordSchema::new(vec![FieldSchema::new("a", FieldType::Int)]).expect("schema"));
        let batch = RecordBatchMut::with_capacity(schema, 0).freeze().expect("freeze");
        let error = write_table(&batch, TableFormat::Parquet, &WriteOptions::default())
            .expect_err("Parquet writing needs the parquet feature");
        assert!(format!("{error}").contains("Parquet"));
    }
}
