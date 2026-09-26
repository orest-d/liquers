//! Row identity and provenance (`RowId`, `RowRun`, `ChunkId`, `ChunkOrigin`, `LocatorRule`,
//! `ChunkList`, `ChunkDescriptor`), the materialized table (`RecordBatch`), and the two traits
//! every table and every traversal implement (`RecordView`, `RecordStream`).
//!
//! See `specs/design/record-streams/phase2-architecture.md`, §"The types", §"Every row has an
//! implicit id", §"Columns, not rows: the batch is Arrow-laid-out", §"ChunkOrigin — identity,
//! description and retrieval", §"Provenance and validity" and §"Why `liquers-core` needs no
//! stream alias".

use std::fmt::Debug;
use std::ops::Range;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context as TaskContext, Poll};

use futures::Stream;
use liquers_core::error::Error;
use liquers_core::maybe_send::{BoxFuture, MaybeSend, MaybeSync};
use liquers_core::metadata::{AssetInfo, Metadata};
use liquers_core::query::{Key, Query};
use serde::{Deserialize, Serialize};

use crate::column::{Column, FieldValue};
use crate::schema::RecordSchema;

/// `#[serde(with = "query_format")]` for a `Query` field — the same pattern
/// `liquers_core::metadata` uses privately, reproduced here because that module does not export
/// it. `Query::encode`/`parse_query` round-trip losslessly, so this is exact, not approximate.
mod query_format {
    use liquers_core::parse::parse_query;
    use liquers_core::query::Query;
    use serde::{de, Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(query: &Query, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&query.encode())
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Query, D::Error>
    where
        D: Deserializer<'de>,
    {
        let text = String::deserialize(deserializer)?;
        parse_query(&text).map_err(de::Error::custom)
    }
}

/// (chunk index, row within the chunk) — the fallback identity every row has, whether or not its
/// schema declares an `Id` field. The chunk index is the chunk's position in the source's
/// `ChunkList`. See phase2-architecture.md §"Every row has an implicit id".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RowId {
    pub chunk: u64,
    pub row: u64,
}

/// Rows `len` long, starting at `first_row` of chunk `chunk`, and at row number `first_number` of
/// the source when that is known. A batch from one chunk has one run; a table materialized from
/// several chunks has one run per chunk. `RecordBatch::rows`' run lengths sum to `len`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RowRun {
    pub chunk: u64,
    pub first_row: u64,
    pub first_number: Option<u64>,
    pub len: usize,
}

/// A chunk's identity. The two variants are the two identity regimes of `manifest-format.md` §5:
/// an unkeyed stream identifies a chunk by the query that produces it, a keyed stream by the key
/// its chunk is stored under.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ChunkId {
    /// Unkeyed: the producing query, which is also the chunk's asset identity.
    Query(#[serde(with = "query_format")] Query),
    /// Keyed: the stored chunk's key, e.g. `data/sales/daily_0010.csv`.
    Key(Key),
}

/// A command applied to `asset`, with the id supplied as its final parameter. Rendered through
/// `ActionRequest` (a later step's `ChunkOrigin::locator_query` — not part of this one), never by
/// string templating.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LocatorRule {
    pub namespace: String,
    pub command: String,
    pub leading_parameters: Vec<String>,
}

/// Identity, description, and the **retrieval** path for the rows of one source, stored once per
/// batch (in `RecordBatch::sources`) rather than once per row. See phase2-architecture.md
/// §"ChunkOrigin — identity, description and retrieval".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChunkOrigin {
    /// The asset these rows were projected from, as a query.
    #[serde(with = "query_format")]
    pub asset: Query,
    /// The query that re-produces this source's rows. Evaluating it yields the batch again, and
    /// the `Id` field indexes into it.
    #[serde(with = "query_format")]
    pub chunk: Query,
    /// Description of the asset itself, when it has one. `None` for a source that is not an asset
    /// (a CSV row has no `AssetInfo`; the file does).
    pub info: Option<AssetInfo>,
    /// How to turn an `Id` value into a directly evaluable query, when the projection can. An
    /// optimization over `rec_id`, never a second identity mechanism.
    pub locator: Option<LocatorRule>,
}

/// Enumeration is not always possible, so a consumer handles both cases from the start. See
/// phase2-architecture.md §"The types".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkList<'a> {
    /// Every chunk is known, so reconciliation can diff a complete set — detecting additions,
    /// changes **and deletions**.
    Known(&'a [ChunkId]),
    /// The count is unknown; a walk ends at the first short chunk. Reconciliation is append-only,
    /// and **deletions cannot be detected** without a full walk.
    Unbounded { computed: &'a [ChunkId] },
}

/// Full provenance for one chunk: what re-produces it, whether it is still valid, and where its
/// rows came from. See phase2-architecture.md §"Provenance and validity: the chunk carries a
/// `Metadata`".
///
/// `liquers_core::metadata::Metadata` derives only `Debug` and `Clone` — not `PartialEq`,
/// `Serialize` or `Deserialize` — so `ChunkDescriptor` cannot carry the full derive list Phase 2's
/// code block shows. Filed as `METADATA-LACKS-SERIALIZE-DESERIALIZE-AND-PARTIALEQ`; until it is
/// fixed (or this type gets a hand-written serialization that skips `metadata` or re-derives it from
/// `MetadataRecord`), `ChunkDescriptor` is `Debug + Clone` only. It is not part of any wire format
/// in this step — `RecordSource::describe_chunk` (Step 2.6) returns it for in-process use.
#[derive(Debug, Clone)]
pub struct ChunkDescriptor {
    pub id: ChunkId,
    /// The query that re-produces this chunk.
    pub query: Query,
    /// Provenance and validity. `Metadata` already carries `query`, `version`,
    /// `dependencies: Vec<DependencyRecord { key, version }>`, `status` and `updated`.
    pub metadata: Metadata,
    pub origin: ChunkOrigin,
    /// Optional and advisory; field roles are its valuable content.
    pub schema: Option<RecordSchema>,
}

/// A batch of rows in Arrow's memory layout — the **materialized** `RecordView`. The unit of
/// memory, the form data rests in, and the form that exports to Arrow as a whole. See
/// phase2-architecture.md §"Columns, not rows: the batch is Arrow-laid-out".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecordBatch {
    /// Field names, types and roles — once per batch.
    pub schema: Arc<RecordSchema>,
    /// One column per schema field, in order. `len` rows each.
    pub columns: Vec<Column>,
    pub len: usize,
    /// Identity of the chunk these rows came from — once per batch, not per row. `None` for a
    /// table materialized from several chunks; `rows` then says which rows came from which.
    pub chunk_id: Option<ChunkId>,
    /// Where the rows came from, as runs — the implicit `RowId` of every row, and its row number
    /// when known. One run for a single-chunk batch.
    pub rows: Vec<RowRun>,
    /// Dictionary of sources; the `Source`-role column indexes it.
    pub sources: Vec<ChunkOrigin>,
}

impl RecordBatch {
    /// Validates column count, lengths and types against `schema`. `rows` defaults to one run of
    /// chunk 0 from row 0 — a standalone table, per phase2-architecture.md's Function Signatures.
    pub fn new(
        schema: Arc<RecordSchema>,
        columns: Vec<Column>,
        chunk_id: Option<ChunkId>,
        rows: Option<Vec<RowRun>>,
        sources: Vec<ChunkOrigin>,
    ) -> Result<RecordBatch, Error> {
        if columns.len() != schema.fields.len() {
            return Err(Error::general_error(format!(
                "RecordBatch::new: {} columns given, but the schema declares {} fields",
                columns.len(),
                schema.fields.len()
            )));
        }
        let len = columns.first().map(Column::len).unwrap_or(0);
        for (index, (field, column)) in schema.fields.iter().zip(columns.iter()).enumerate() {
            if column.len() != len {
                return Err(Error::general_error(format!(
                    "RecordBatch::new: column '{}' (index {index}) has {} rows, but column 0 has {len}",
                    field.name,
                    column.len()
                )));
            }
            if column.data_type() != field.data_type {
                return Err(Error::general_error(format!(
                    "RecordBatch::new: column '{}' (index {index}) is {:?}, but the schema \
                     declares {:?}",
                    field.name,
                    column.data_type(),
                    field.data_type
                )));
            }
        }
        let rows = match rows {
            Some(rows) => {
                let total: usize = rows.iter().map(|run| run.len).sum();
                if total != len {
                    return Err(Error::general_error(format!(
                        "RecordBatch::new: row runs sum to {total} rows, but the columns hold {len}"
                    )));
                }
                rows
            }
            None => vec![RowRun {
                chunk: 0,
                first_row: 0,
                first_number: Some(0),
                len,
            }],
        };
        Ok(RecordBatch {
            schema,
            columns,
            len,
            chunk_id,
            rows,
            sources,
        })
    }

    /// Fails when schemas differ, naming the first differing field. `chunk_id` is kept only when
    /// a single batch is "concatenated" with itself; otherwise `None`, per phase2-architecture.md
    /// §"Columns, not rows" ("`None` for a table materialized from several chunks").
    pub fn concat(batches: &[RecordBatch]) -> Result<RecordBatch, Error> {
        let first = batches
            .first()
            .ok_or_else(|| Error::general_error("RecordBatch::concat: no batches given".to_string()))?;
        let schema = first.schema.clone();
        for batch in &batches[1..] {
            if batch.schema.fields.len() != schema.fields.len() {
                return Err(Error::general_error(format!(
                    "RecordBatch::concat: schemas differ in field count ({} vs {})",
                    schema.fields.len(),
                    batch.schema.fields.len()
                )));
            }
            for (index, (a, b)) in schema.fields.iter().zip(batch.schema.fields.iter()).enumerate() {
                if a.name != b.name || a.data_type != b.data_type {
                    return Err(Error::general_error(format!(
                        "RecordBatch::concat: schemas differ at field {index} ('{}': {:?} vs '{}': {:?})",
                        a.name, a.data_type, b.name, b.data_type
                    )));
                }
            }
        }

        // A field is nullable in the result when it is nullable in any input: chunks read without a
        // schema infer nullability from the nulls they happen to hold, so they may disagree.
        let needs_widening = schema.fields.iter().enumerate().any(|(index, field)| {
            !field.nullable && batches.iter().any(|batch| batch.schema.fields[index].nullable)
        });
        let schema = if needs_widening {
            let fields = schema
                .fields
                .iter()
                .enumerate()
                .map(|(index, field)| {
                    let mut field = field.clone();
                    field.nullable = batches.iter().any(|batch| batch.schema.fields[index].nullable);
                    field
                })
                .collect();
            Arc::new(RecordSchema::new(fields)?)
        } else {
            schema
        };

        let mut columns = Vec::with_capacity(schema.fields.len());
        for col_index in 0..schema.fields.len() {
            let per_batch: Vec<Column> = batches
                .iter()
                .map(|batch| batch.columns[col_index].clone())
                .collect();
            columns.push(Column::concat(&per_batch)?);
        }

        let len: usize = batches.iter().map(|batch| batch.len).sum();
        let mut rows = Vec::new();
        for batch in batches {
            rows.extend(batch.rows.iter().copied());
        }
        let mut sources = Vec::new();
        for batch in batches {
            sources.extend(batch.sources.iter().cloned());
        }
        let chunk_id = if batches.len() == 1 {
            first.chunk_id.clone()
        } else {
            None
        };

        Ok(RecordBatch {
            schema,
            columns,
            len,
            chunk_id,
            rows,
            sources,
        })
    }
}

/// Any finite table with random access: a batch, a projection, a row selection, a derived column,
/// a generated table. **Synchronous** — anything that must `.await` is a source. See
/// phase2-architecture.md §"Views: `RecordView` and its implementations".
pub trait RecordView: Debug + MaybeSend + MaybeSync + 'static {
    fn schema(&self) -> &Arc<RecordSchema>;
    fn len(&self) -> usize;

    /// **The one required read**: rows `rows` of column `col`, in Arrow layout with `Arc`-shared
    /// buffers. Out of range is an error, never a panic.
    fn column_range(&self, col: usize, rows: Range<usize>) -> Result<Column, Error>;

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn column(&self, col: usize) -> Result<Column, Error> {
        self.column_range(col, 0..self.len())
    }

    fn value(&self, row: usize, col: usize) -> Result<FieldValue, Error> {
        self.column_range(col, row..row + 1)?.get(0)
    }

    /// A `RecordBatch` with the same rows: `column()` for every field. `RecordBatch` overrides
    /// this with a shallow clone; other views fall back to this default, which reads every column
    /// in full and approximates provenance as a single run starting at row 0's `row_id`/
    /// `row_number` — exact for a view whose rows stay contiguous (a slice, a projection, a
    /// derived column), approximate for one that reorders them (a filter's `RowIndexView`, Step
    /// 2.4), which is free to override it.
    fn materialize(&self) -> Result<Arc<RecordBatch>, Error> {
        let schema = self.schema().clone();
        let mut columns = Vec::with_capacity(schema.fields.len());
        for col in 0..schema.fields.len() {
            columns.push(self.column(col)?);
        }
        let rows = if self.len() == 0 {
            Vec::new()
        } else {
            let first_id = self.row_id(0)?;
            let first_number = self.row_number(0)?;
            vec![RowRun {
                chunk: first_id.chunk,
                first_row: first_id.row,
                first_number,
                len: self.len(),
            }]
        };
        RecordBatch::new(
            schema,
            columns,
            self.chunk_id().cloned(),
            Some(rows),
            self.origins().to_vec(),
        )
        .map(Arc::new)
    }

    /// The typed fast path — not an `Any` downcast. `Some` only for a `RecordBatch`.
    fn as_batch(&self) -> Option<&RecordBatch> {
        None
    }

    /// The chunk these rows came from, when they came from one.
    fn chunk_id(&self) -> Option<&ChunkId> {
        None
    }

    /// The origin dictionary the `Source`-role column indexes.
    fn origins(&self) -> &[ChunkOrigin] {
        &[]
    }

    /// The **implicit** identity of `row` — the chunk it came from and its position there.
    /// Always available: a table that came from no chunk is chunk 0. Views that select rows map
    /// through their base.
    fn row_id(&self, row: usize) -> Result<RowId, Error> {
        Ok(RowId {
            chunk: 0,
            row: row as u64,
        })
    }

    /// `row`'s number across the whole source, when the rows before its chunk have been counted.
    fn row_number(&self, row: usize) -> Result<Option<u64>, Error> {
        Ok(Some(row as u64))
    }
}

impl RecordView for RecordBatch {
    fn schema(&self) -> &Arc<RecordSchema> {
        &self.schema
    }

    fn len(&self) -> usize {
        self.len
    }

    fn column_range(&self, col: usize, rows: Range<usize>) -> Result<Column, Error> {
        let column = self.columns.get(col).ok_or_else(|| {
            Error::general_error(format!(
                "RecordBatch::column_range: column index {col} out of range (0..{})",
                self.columns.len()
            ))
        })?;
        column.slice(rows.start, rows.len())
    }

    fn materialize(&self) -> Result<Arc<RecordBatch>, Error> {
        Ok(Arc::new(self.clone()))
    }

    fn as_batch(&self) -> Option<&RecordBatch> {
        Some(self)
    }

    fn chunk_id(&self) -> Option<&ChunkId> {
        self.chunk_id.as_ref()
    }

    fn origins(&self) -> &[ChunkOrigin] {
        &self.sources
    }

    fn row_id(&self, row: usize) -> Result<RowId, Error> {
        let mut offset = 0usize;
        for run in &self.rows {
            if row < offset + run.len {
                let within = (row - offset) as u64;
                return Ok(RowId {
                    chunk: run.chunk,
                    row: run.first_row + within,
                });
            }
            offset += run.len;
        }
        Err(Error::general_error(format!(
            "RecordBatch::row_id: row {row} out of range (0..{})",
            self.len
        )))
    }

    fn row_number(&self, row: usize) -> Result<Option<u64>, Error> {
        let mut offset = 0usize;
        for run in &self.rows {
            if row < offset + run.len {
                let within = (row - offset) as u64;
                return Ok(run.first_number.map(|number| number + within));
            }
            offset += run.len;
        }
        Err(Error::general_error(format!(
            "RecordBatch::row_number: row {row} out of range (0..{})",
            self.len
        )))
    }
}

/// One traversal: a `futures::Stream` of views, plus the one thing a consumer needs **before**
/// the first item — the schema, when promised. `MaybeSend` is a **supertrait**, so `dyn
/// RecordStream` carries the right `Send`-ness on each target by transitivity and no per-target
/// `BoxStream` alias is needed — see phase2-architecture.md §"Why `liquers-core` needs no stream
/// alias".
pub trait RecordStream: Stream<Item = Result<Arc<dyn RecordView>, Error>> + MaybeSend + 'static {
    fn schema(&self) -> Option<Arc<RecordSchema>>;
}

pub type BoxRecordStream = Pin<Box<dyn RecordStream>>;

/// Gives any stream of views the `RecordStream` interface — how a combinator chain (`map`,
/// `filter_map`, `then`) becomes a record stream again. `S: Unpin` is met by `Box::pin(stream)`.
pub fn record_stream<S>(inner: S, schema: Option<Arc<RecordSchema>>) -> BoxRecordStream
where
    S: Stream<Item = Result<Arc<dyn RecordView>, Error>> + Unpin + MaybeSend + 'static,
{
    struct Wrapped<S> {
        inner: S,
        schema: Option<Arc<RecordSchema>>,
    }

    impl<S> Stream for Wrapped<S>
    where
        S: Stream<Item = Result<Arc<dyn RecordView>, Error>> + Unpin,
    {
        type Item = Result<Arc<dyn RecordView>, Error>;

        fn poll_next(self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<Option<Self::Item>> {
            let this = self.get_mut();
            Pin::new(&mut this.inner).poll_next(cx)
        }
    }

    impl<S> RecordStream for Wrapped<S>
    where
        S: Stream<Item = Result<Arc<dyn RecordView>, Error>> + Unpin + MaybeSend + 'static,
    {
        fn schema(&self) -> Option<Arc<RecordSchema>> {
            self.schema.clone()
        }
    }

    Box::pin(Wrapped { inner, schema })
}

/// `materialize` for a traversal already open. An extension trait rather than a method of
/// `RecordStream`, because draining consumes the stream and it is held as a boxed trait object.
/// `max_rows` is a limit rather than a promise: a source whose chunk count is unknown cannot say
/// in advance whether it is small.
pub trait RecordStreamExt {
    fn materialize(self, max_rows: usize) -> BoxFuture<'static, Result<Arc<RecordBatch>, Error>>;
}

impl RecordStreamExt for BoxRecordStream {
    fn materialize(self, max_rows: usize) -> BoxFuture<'static, Result<Arc<RecordBatch>, Error>> {
        use futures::StreamExt;

        Box::pin(async move {
            let schema = self.schema();
            let mut stream = self;
            let mut batches: Vec<Arc<RecordBatch>> = Vec::new();
            let mut total = 0usize;
            while let Some(item) = stream.next().await {
                let view = item?;
                let batch = view.materialize()?;
                total += batch.len;
                if total > max_rows {
                    return Err(Error::general_error(format!(
                        "RecordStreamExt::materialize: stream exceeds max_rows ({max_rows})"
                    )));
                }
                batches.push(batch);
            }
            match (batches.is_empty(), schema) {
                (true, Some(schema)) => {
                    let columns = schema.fields.iter().map(|field| Column::empty(field.data_type)).collect();
                    RecordBatch::new(schema, columns, None, Some(Vec::new()), Vec::new()).map(Arc::new)
                }
                (true, None) => Err(Error::general_error(
                    "RecordStreamExt::materialize: empty stream with no declared schema".to_string(),
                )),
                (false, _) => {
                    let owned: Vec<RecordBatch> = batches.iter().map(|batch| (**batch).clone()).collect();
                    RecordBatch::concat(&owned).map(Arc::new)
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::Buffer;
    use crate::schema::{FieldSchema, FieldType, KeyRole};
    use liquers_core::parse::parse_query;

    fn schema_with_two_fields() -> Arc<RecordSchema> {
        Arc::new(
            RecordSchema::new(vec![
                FieldSchema::new("id", FieldType::Text).with_key(KeyRole::Id),
                FieldSchema::new("amount", FieldType::Int),
            ])
            .expect("schema"),
        )
    }

    fn batch(schema: Arc<RecordSchema>, ids: &[&str], amounts: &[i64]) -> RecordBatch {
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
            data: crate::buffer::AlignedBuffer::from_slice(&data),
        };
        let amount_column = Column::Int {
            validity: None,
            values: Buffer::from_slice(amounts),
        };
        RecordBatch::new(schema, vec![id_column, amount_column], None, None, Vec::new()).expect("batch")
    }

    #[test]
    fn record_batch_new_defaults_rows_to_one_run_of_chunk_zero() {
        let schema = schema_with_two_fields();
        let b = batch(schema, &["a", "b"], &[1, 2]);
        assert_eq!(b.len, 2);
        assert_eq!(
            b.rows,
            vec![RowRun {
                chunk: 0,
                first_row: 0,
                first_number: Some(0),
                len: 2,
            }]
        );
    }

    #[test]
    fn record_batch_new_rejects_wrong_column_count() {
        let schema = schema_with_two_fields();
        let column = Column::Int {
            validity: None,
            values: Buffer::from_slice(&[1i64]),
        };
        assert!(RecordBatch::new(schema, vec![column], None, None, Vec::new()).is_err());
    }

    #[test]
    fn record_batch_new_rejects_mismatched_column_lengths() {
        let schema = schema_with_two_fields();
        let id_column = Column::Text {
            validity: None,
            offsets: Buffer::from_slice(&[0i32, 1, 2]),
            data: crate::buffer::AlignedBuffer::from_slice(b"ab"),
        };
        let amount_column = Column::Int {
            validity: None,
            values: Buffer::from_slice(&[1i64]), // one row, not two
        };
        assert!(RecordBatch::new(schema, vec![id_column, amount_column], None, None, Vec::new()).is_err());
    }

    #[test]
    fn record_batch_new_rejects_type_mismatch_against_schema() {
        let schema = schema_with_two_fields();
        let id_column = Column::Text {
            validity: None,
            offsets: Buffer::from_slice(&[0i32, 1]),
            data: crate::buffer::AlignedBuffer::from_slice(b"a"),
        };
        let amount_column = Column::Text {
            // schema declares Int
            validity: None,
            offsets: Buffer::from_slice(&[0i32, 1]),
            data: crate::buffer::AlignedBuffer::from_slice(b"x"),
        };
        assert!(RecordBatch::new(schema, vec![id_column, amount_column], None, None, Vec::new()).is_err());
    }

    #[test]
    fn record_batch_new_rejects_row_runs_not_summing_to_len() {
        let schema = schema_with_two_fields();
        let id_column = Column::Text {
            validity: None,
            offsets: Buffer::from_slice(&[0i32, 1, 2]),
            data: crate::buffer::AlignedBuffer::from_slice(b"ab"),
        };
        let amount_column = Column::Int {
            validity: None,
            values: Buffer::from_slice(&[1i64, 2]),
        };
        let bad_rows = vec![RowRun {
            chunk: 0,
            first_row: 0,
            first_number: Some(0),
            len: 5, // does not match the 2 actual rows
        }];
        assert!(RecordBatch::new(
            schema,
            vec![id_column, amount_column],
            None,
            Some(bad_rows),
            Vec::new()
        )
        .is_err());
    }

    #[test]
    fn record_batch_concat_joins_rows_and_columns() {
        let schema = schema_with_two_fields();
        let b1 = batch(schema.clone(), &["a", "b"], &[1, 2]);
        let b2 = batch(schema, &["c"], &[3]);
        let joined = RecordBatch::concat(&[b1, b2]).expect("concat");
        assert_eq!(joined.len, 3);
        assert_eq!(joined.value(0, 1).expect("value"), FieldValue::Int(1));
        assert_eq!(joined.value(2, 1).expect("value"), FieldValue::Int(3));
        // Concatenated from two batches: chunk_id is None per phase2-architecture.md.
        assert_eq!(joined.chunk_id, None);
        assert_eq!(joined.rows.len(), 2); // one run per input batch
    }

    #[test]
    fn record_batch_concat_fails_naming_first_differing_field() {
        let schema_a = schema_with_two_fields();
        let schema_b = Arc::new(
            RecordSchema::new(vec![
                FieldSchema::new("id", FieldType::Text).with_key(KeyRole::Id),
                FieldSchema::new("amount", FieldType::Float), // differs from Int
            ])
            .expect("schema"),
        );
        let b1 = batch(schema_a, &["a"], &[1]);
        let b2_id_column = Column::Text {
            validity: None,
            offsets: Buffer::from_slice(&[0i32, 1]),
            data: crate::buffer::AlignedBuffer::from_slice(b"c"),
        };
        let b2_amount_column = Column::Float {
            validity: None,
            values: Buffer::from_slice(&[1.5f64]),
        };
        let b2 = RecordBatch::new(schema_b, vec![b2_id_column, b2_amount_column], None, None, Vec::new())
            .expect("batch");
        let error = RecordBatch::concat(&[b1, b2]).expect_err("schemas differ");
        assert!(format!("{error}").contains("amount"));
    }

    #[test]
    fn record_batch_concat_of_no_batches_is_an_error() {
        assert!(RecordBatch::concat(&[]).is_err());
    }

    #[test]
    fn record_view_column_and_value_use_column_range() {
        let schema = schema_with_two_fields();
        let b = batch(schema, &["a", "b", "c"], &[10, 20, 30]);
        let view: &dyn RecordView = &b;
        assert_eq!(view.len(), 3);
        assert!(!view.is_empty());
        assert_eq!(view.value(1, 1).expect("value"), FieldValue::Int(20));
        let column = view.column(1).expect("column");
        assert_eq!(column.len(), 3);
    }

    #[test]
    fn record_view_materialize_on_a_batch_is_a_shallow_clone() {
        let schema = schema_with_two_fields();
        let b = batch(schema, &["a"], &[1]);
        let view: &dyn RecordView = &b;
        let materialized = view.materialize().expect("materialize");
        assert_eq!(*materialized, b);
    }

    #[test]
    fn record_batch_row_id_and_row_number_follow_runs() {
        let schema = schema_with_two_fields();
        let mut b = batch(schema, &["a", "b", "c"], &[1, 2, 3]);
        b.rows = vec![
            RowRun {
                chunk: 5,
                first_row: 10,
                first_number: Some(100),
                len: 2,
            },
            RowRun {
                chunk: 7,
                first_row: 0,
                first_number: Some(200),
                len: 1,
            },
        ];
        let view: &dyn RecordView = &b;
        assert_eq!(
            view.row_id(0).expect("row_id"),
            RowId { chunk: 5, row: 10 }
        );
        assert_eq!(
            view.row_id(1).expect("row_id"),
            RowId { chunk: 5, row: 11 }
        );
        assert_eq!(
            view.row_id(2).expect("row_id"),
            RowId { chunk: 7, row: 0 }
        );
        assert_eq!(view.row_number(0).expect("row_number"), Some(100));
        assert_eq!(view.row_number(2).expect("row_number"), Some(200));
        assert!(view.row_id(3).is_err());
    }

    #[test]
    fn record_view_default_row_id_is_chunk_zero() {
        // A minimal RecordView that relies entirely on the trait's provided defaults.
        #[derive(Debug)]
        struct Bare {
            schema: Arc<RecordSchema>,
            column: Column,
        }
        impl RecordView for Bare {
            fn schema(&self) -> &Arc<RecordSchema> {
                &self.schema
            }
            fn len(&self) -> usize {
                self.column.len()
            }
            fn column_range(&self, col: usize, rows: Range<usize>) -> Result<Column, Error> {
                assert_eq!(col, 0);
                self.column.slice(rows.start, rows.len())
            }
        }
        let schema = Arc::new(RecordSchema::new(vec![FieldSchema::new("v", FieldType::Int)]).expect("schema"));
        let bare = Bare {
            schema,
            column: Column::Int {
                validity: None,
                values: Buffer::from_slice(&[7i64, 8]),
            },
        };
        assert_eq!(bare.row_id(1).expect("row_id"), RowId { chunk: 0, row: 1 });
        assert_eq!(bare.row_number(1).expect("row_number"), Some(1));
        assert!(bare.as_batch().is_none());
        assert!(bare.origins().is_empty());
        assert!(bare.chunk_id().is_none());
        let materialized = bare.materialize().expect("materialize");
        assert_eq!(materialized.len, 2);
        assert_eq!(materialized.value(0, 0).expect("value"), FieldValue::Int(7));
    }

    /// Compile-time object-safety check for `RecordView`, per Step 2.3's requirement: this
    /// function only type-checks if `RecordView` can be used as `Arc<dyn RecordView>`.
    fn assert_usable_as_arc_dyn_record_view(view: Arc<dyn RecordView>) -> Arc<dyn RecordView> {
        view
    }

    #[test]
    fn record_view_is_object_safe_as_arc_dyn() {
        let schema = schema_with_two_fields();
        let b = batch(schema, &["a"], &[1]);
        let view: Arc<dyn RecordView> = Arc::new(b);
        let view = assert_usable_as_arc_dyn_record_view(view);
        assert_eq!(view.len(), 1);
    }

    #[test]
    fn row_id_and_chunk_id_serde_roundtrip() {
        let row_id = RowId { chunk: 3, row: 42 };
        let json = serde_json::to_string(&row_id).expect("serialize");
        let restored: RowId = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored, row_id);

        let chunk_id = ChunkId::Key(liquers_core::query::Key::new());
        let json = serde_json::to_string(&chunk_id).expect("serialize");
        let restored: ChunkId = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored, chunk_id);
    }

    #[test]
    fn chunk_id_query_variant_serde_roundtrips_through_query_format() {
        let query = parse_query("data/x.csv").expect("parse");
        let chunk_id = ChunkId::Query(query);
        let json = serde_json::to_string(&chunk_id).expect("serialize");
        assert!(json.contains("data/x.csv"));
        let restored: ChunkId = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored, chunk_id);
    }

    #[test]
    fn chunk_origin_serde_roundtrip() {
        let origin = ChunkOrigin {
            asset: parse_query("data/sales.csv").expect("parse"),
            chunk: parse_query("data/sales.csv").expect("parse"),
            info: None,
            locator: Some(LocatorRule {
                namespace: "ns-csv".to_string(),
                command: "row".to_string(),
                leading_parameters: Vec::new(),
            }),
        };
        let json = serde_json::to_string(&origin).expect("serialize");
        let restored: ChunkOrigin = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored, origin);
    }

    #[test]
    fn chunk_list_known_and_unbounded_are_distinct() {
        let ids = vec![ChunkId::Key(liquers_core::query::Key::new())];
        let known = ChunkList::Known(&ids);
        let unbounded = ChunkList::Unbounded { computed: &ids };
        assert_ne!(known, unbounded);
        match known {
            ChunkList::Known(list) => assert_eq!(list.len(), 1),
            ChunkList::Unbounded { .. } => panic!("expected Known"),
        }
    }

    #[tokio::test]
    async fn record_stream_ext_materializes_all_views_into_one_batch() {
        let schema = schema_with_two_fields();
        let b1 = batch(schema.clone(), &["a"], &[1]);
        let b2 = batch(schema.clone(), &["b"], &[2]);
        let views: Vec<Result<Arc<dyn RecordView>, Error>> = vec![
            Ok(Arc::new(b1) as Arc<dyn RecordView>),
            Ok(Arc::new(b2) as Arc<dyn RecordView>),
        ];
        let stream = record_stream(futures::stream::iter(views), Some(schema));
        let materialized = stream.materialize(100).await.expect("materialize");
        assert_eq!(materialized.len, 2);
        assert_eq!(materialized.value(0, 1).expect("value"), FieldValue::Int(1));
        assert_eq!(materialized.value(1, 1).expect("value"), FieldValue::Int(2));
    }

    #[tokio::test]
    async fn record_stream_ext_reports_schema_before_the_first_item() {
        let schema = schema_with_two_fields();
        let views: Vec<Result<Arc<dyn RecordView>, Error>> = Vec::new();
        let stream = record_stream(futures::stream::iter(views), Some(schema.clone()));
        assert_eq!(stream.schema().map(|s| s.fields.len()), Some(schema.fields.len()));
    }

    #[tokio::test]
    async fn record_stream_ext_materialize_of_empty_stream_with_schema_yields_empty_batch() {
        let schema = schema_with_two_fields();
        let views: Vec<Result<Arc<dyn RecordView>, Error>> = Vec::new();
        let stream = record_stream(futures::stream::iter(views), Some(schema));
        let materialized = stream.materialize(10).await.expect("materialize");
        assert_eq!(materialized.len, 0);
    }

    #[tokio::test]
    async fn record_stream_ext_materialize_exceeding_max_rows_is_an_error() {
        let schema = schema_with_two_fields();
        let b1 = batch(schema.clone(), &["a", "b"], &[1, 2]);
        let views: Vec<Result<Arc<dyn RecordView>, Error>> = vec![Ok(Arc::new(b1) as Arc<dyn RecordView>)];
        let stream = record_stream(futures::stream::iter(views), Some(schema));
        assert!(stream.materialize(1).await.is_err());
    }

    #[tokio::test]
    async fn record_stream_ext_propagates_an_error_item() {
        let views: Vec<Result<Arc<dyn RecordView>, Error>> =
            vec![Err(Error::general_error("boom".to_string()))];
        let stream = record_stream(futures::stream::iter(views), None);
        assert!(stream.materialize(10).await.is_err());
    }

    #[test]
    fn concat_widens_nullability_when_any_input_is_nullable() -> Result<(), Error> {
        let not_null = Arc::new(RecordSchema::new(vec![
            FieldSchema::new("x", FieldType::Int).not_null(),
        ])?);
        let nullable = Arc::new(RecordSchema::new(vec![FieldSchema::new("x", FieldType::Int)])?);
        let a = RecordBatch::new(
            not_null,
            vec![Column::Int { validity: None, values: Buffer::from_slice(&[1i64]) }],
            None,
            None,
            vec![],
        )?;
        let b = RecordBatch::new(
            nullable,
            vec![Column::Int {
                validity: Some(crate::buffer::Bitmap::from_bools(&[false])),
                values: Buffer::from_slice(&[0i64]),
            }],
            None,
            None,
            vec![],
        )?;
        let joined = RecordBatch::concat(&[a, b])?;
        assert!(joined.schema.fields[0].nullable);
        assert_eq!(joined.value(1, 0)?, FieldValue::Null);
        Ok(())
    }
}
