//! Table formats: `TableFormat`, `ReadSchema`, `ReadOptions`/`WriteOptions`, and the two entry
//! points every format goes through — [`read_table`] and [`write_table`]. Which reader runs
//! depends only on whether a schema is available (`ReadSchema`), not on the format; which format
//! runs depends only on `TableFormat`, never on the file's own hints.
//!
//! See `specs/design/record-streams/phase2-architecture.md`, §"Table formats: what a `RecordView`
//! writes and reads", §"Two readers: schema-aware and schema-less" and §"Construction helpers,
//! options and the provider chain".
//!
//! Every variant Phase 2 lists is present here from the start, even the ones this step does not
//! implement (`Markdown`, `Html` land in a later step; `Ipc`, `Parquet` land in Step 6.x behind
//! this crate's `ipc`/`parquet` features): a `TableFormat` value and `from_data_format`'s alias
//! table are part of Phase 2's stable surface regardless of which readers exist yet, and the
//! variant itself carries no dependency on `flatbuffers`/`flate2` — only its eventual
//! reader/writer will. Until then, [`read_table`]/[`write_table`] refuse them with a typed
//! [`liquers_core::error::Error::not_supported`], naming the format and the direction.

pub mod csv;
pub mod infer;
pub mod ndjson;
pub mod shapes;

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
    /// Arrow IPC file (Feather v2). Implemented in Step 6.x behind the crate's `ipc` feature; the
    /// variant itself needs no `flatbuffers` dependency.
    Ipc,
    /// Implemented in Step 6.x behind the crate's `parquet` feature; the variant itself needs no
    /// `flate2` dependency.
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

/// A typed placeholder for a format whose reader/writer has not landed yet (Step 3.2/3.3/6.x) —
/// never a silent `_ =>` fallthrough, since every [`TableFormat`] variant still gets its own match
/// arm in [`read_table`]/[`write_table`].
fn not_yet_supported(format: &str, direction: &str) -> Error {
    Error::not_supported(format!(
        "TableFormat::{format}: {direction} is not implemented yet"
    ))
}

/// Parses `bytes` as `format` into a [`RecordBatch`]. Which reader runs — schema-aware and strict,
/// or schema-less and inferring — depends only on `schema`. See phase2-architecture.md §"Two
/// readers: schema-aware and schema-less".
pub fn read_table(
    bytes: &[u8],
    format: TableFormat,
    schema: ReadSchema<'_>,
    options: &ReadOptions,
) -> Result<RecordBatch, Error> {
    match format {
        TableFormat::Csv { separator } => csv::read_csv(bytes, separator, schema, options),
        TableFormat::NdJson => ndjson::read_ndjson(bytes, schema, options),
        TableFormat::Json => ndjson::read_json(bytes, schema, options),
        TableFormat::Markdown => Err(not_yet_supported("Markdown", "reading")),
        TableFormat::Html => Err(not_yet_supported("Html", "reading")),
        TableFormat::Ipc => Err(not_yet_supported("Ipc", "reading")),
        TableFormat::Parquet => Err(not_yet_supported("Parquet", "reading")),
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
        TableFormat::Markdown => Err(not_yet_supported("Markdown", "writing")),
        TableFormat::Html => Err(not_yet_supported("Html", "writing")),
        TableFormat::Ipc => Err(not_yet_supported("Ipc", "writing")),
        TableFormat::Parquet => Err(not_yet_supported("Parquet", "writing")),
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
        // CORRECTED: this test predates Step 3.2, which implements `Json` reading — a call that
        // used to hit `not_yet_supported` now parses (and, for `b""`, fails with a JSON syntax
        // error unrelated to "not implemented"). `Markdown` still has no reader, so it is what
        // this test's "unimplemented format" case now needs to name.
        let error = read_table(b"", TableFormat::Markdown, ReadSchema::Infer, &ReadOptions::default())
            .expect_err("Markdown reading is not implemented yet");
        assert!(format!("{error}").contains("Markdown"));
    }

    #[test]
    fn write_table_of_an_unimplemented_format_is_not_supported_not_a_panic() {
        use crate::mutable::RecordBatchMut;
        use crate::schema::{FieldSchema, FieldType, RecordSchema};
        use std::sync::Arc;

        let schema =
            Arc::new(RecordSchema::new(vec![FieldSchema::new("a", FieldType::Int)]).expect("schema"));
        let batch = RecordBatchMut::with_capacity(schema, 0).freeze().expect("freeze");
        let error = write_table(&batch, TableFormat::Parquet, &WriteOptions::default())
            .expect_err("Parquet writing is not implemented yet");
        assert!(format!("{error}").contains("Parquet"));
    }
}
