//! `RECORDS01`-`RECORDS11` native (Rust) counterparts, per
//! `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md:504-514` and Phase 3 §6. This file holds the nine
//! that need only the data model and sources (`RECORDS01` x2, `02`, `04`, `05`, `06`, `07`, `08`,
//! `11`). `RECORDS03` is Phase 3 §3.7 (`ipc` feature); `RECORDS09` is N/A for Rust (no separate
//! sync/async binding split); `RECORDS10` is §5.5; the two wasm counterparts of `05`/`06` are
//! Step 7.1, against `liquers-web`'s `RecordBatch` handle.

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Weak,
};

use futures::stream::StreamExt;
use liquers_core::{
    error::Error,
    maybe_send::BoxFuture,
    metadata::Metadata,
    query::{Key, Query},
};
use liquers_records::{
    record_stream, Bitmap, Buffer, ChunkId, ChunkResolver, ChunkValue, Column, FieldSchema,
    FieldType, FieldValue, InMemorySource, KeyRole, RecordBatch, RecordSchema, RecordSource,
    RecordView, RecordViewMut,
};

fn tiny_batch(n_start: i64, len: usize) -> Arc<RecordBatch> {
    let schema = Arc::new(
        RecordSchema::new(vec![FieldSchema::new("id", FieldType::Int).with_key(KeyRole::Id)])
            .expect("schema"),
    );
    let values: Vec<i64> = (0..len as i64).map(|i| n_start + i).collect();
    let column = Column::Int { validity: None, values: Buffer::from_slice(&values) };
    Arc::new(RecordBatch::new(schema, vec![column], None, None, vec![]).expect("batch"))
}

struct NullResolver;
impl ChunkResolver for NullResolver {
    fn evaluate(&self, _query: Query) -> BoxFuture<'static, Result<ChunkValue, Error>> {
        Box::pin(async { Err(Error::general_error("not used by these fixtures".to_string())) })
    }
    fn metadata(&self, _query: Query) -> BoxFuture<'static, Result<Metadata, Error>> {
        Box::pin(async { Ok(Metadata::default()) })
    }
    fn read_resource(&self, _key: Key) -> BoxFuture<'static, Result<(Vec<u8>, Metadata), Error>> {
        Box::pin(async { Err(Error::general_error("not used by these fixtures".to_string())) })
    }
}

// --- RECORDS01 — schema, field roles and chunk identity survive a round trip ------------------

#[test]
fn records01_serde_round_trip_preserves_schema_roles_and_chunk_id() -> Result<(), Box<dyn std::error::Error>> {
    let schema = Arc::new(RecordSchema::new(vec![
        FieldSchema::new("id", FieldType::Int).with_key(KeyRole::Id),
        FieldSchema::new("total", FieldType::Float).with_role(liquers_records::FieldRole::numeric()),
    ])?);
    let mut batch = RecordBatch::new(
        schema,
        vec![
            Column::Int { validity: None, values: Buffer::from_slice(&[1i64]) },
            Column::Float { validity: None, values: Buffer::from_slice(&[9.5f64]) },
        ],
        None,
        None,
        vec![],
    )?;
    batch.chunk_id = Some(ChunkId::Key(liquers_core::parse::parse_key("data/sales/daily_0000.csv")?));

    // Plain Rust `serde` — the struct's own `Serialize`/`Deserialize`, not a table format. This is
    // always lossless: every field of `RecordSchema`/`RecordBatch` derives serde.
    let json = serde_json::to_string(&batch)?;
    let round_tripped: RecordBatch = serde_json::from_str(&json)?;
    assert_eq!(round_tripped.schema, batch.schema); // roles included: FieldSchema derives PartialEq
    assert_eq!(round_tripped.chunk_id, batch.chunk_id);
    Ok(())
}

#[test]
fn records01_csv_documents_which_metadata_it_loses() -> Result<(), Box<dyn std::error::Error>> {
    // The counterpart to the serde test above: CSV is schema-less on the way back in
    // (§"Table formats"), so roles and chunk identity do **not** survive it — asserted, not
    // merely claimed.
    use liquers_records::formats::{read_table, write_table, ReadOptions, ReadSchema, TableFormat, WriteOptions};
    let schema = Arc::new(RecordSchema::new(vec![
        FieldSchema::new("id", FieldType::Int).with_key(KeyRole::Id),
    ])?);
    let batch = RecordBatch::new(schema, vec![Column::Int { validity: None, values: Buffer::from_slice(&[1i64]) }], None, None, vec![])?;
    let bytes = write_table(&batch, TableFormat::Csv { separator: b',' }, &WriteOptions::default())?;
    let read_back = read_table(&bytes, TableFormat::Csv { separator: b',' }, ReadSchema::Infer, &ReadOptions::default())?;
    assert_eq!(read_back.schema.id_field(), None); // the Id role is lost — never guessed back
    Ok(())
}

// --- RECORDS02 — a column read through a view equals a materialized copy ----------------------

#[test]
fn records02_column_through_a_view_equals_a_materialized_copy() -> Result<(), Box<dyn std::error::Error>> {
    // `filter`/`select_columns` are inherent methods on `dyn RecordView` (`self: &Arc<Self>`), so
    // the concrete `Arc<RecordBatch>` `tiny_batch` returns needs an explicit unsizing cast before
    // either is callable — corrected from Phase 3 §6's draft, which called `batch.filter(...)`
    // directly and would not compile.
    let batch = tiny_batch(0, 5) as Arc<dyn RecordView>;
    let mask = Bitmap::from_bools(&[true, false, true, true, false]);
    let filtered = batch.filter(&mask)?;
    let via_view = filtered.column(0)?;
    let via_materialize = filtered.materialize()?.column(0)?;
    assert_eq!(via_view.len(), via_materialize.len());
    for i in 0..via_view.len() {
        assert_eq!(via_view.get(i)?, via_materialize.get(i)?);
    }
    Ok(())
}

// --- RECORDS03 — see §3.7 (`feather_round_trip_preserves_types_roles_and_labels`, behind `ipc`).

// --- RECORDS04 — a lent buffer is read-only; a copy-on-write edit never touches the original ---

#[test]
fn records04_editing_a_shared_batch_copy_leaves_the_original_untouched() -> Result<(), Box<dyn std::error::Error>> {
    let schema = Arc::new(RecordSchema::new(vec![FieldSchema::new("value", FieldType::Int)])?);
    let batch = RecordBatch::new(schema, vec![Column::Int { validity: None, values: Buffer::from_slice(&[1i64, 2, 3]) }], None, None, vec![])?;
    let original = Arc::new(batch);
    let kept_alive = original.clone(); // a second owner — forces `into_mut` to copy, not steal

    let mut mutable = (*original).clone().into_mut();
    mutable.set_value(0, 0, &FieldValue::Int(99))?;
    let edited = mutable.freeze()?;

    assert_eq!(edited.value(0, 0)?, FieldValue::Int(99));
    assert_eq!(kept_alive.value(0, 0)?, FieldValue::Int(1)); // the original is a value: immutable
    Ok(())
}

// --- RECORDS05 — a view keeps its base alive through its own Arc, independent of the caller's --

#[test]
fn records05_view_keeps_reading_after_the_callers_arc_is_dropped() -> Result<(), Box<dyn std::error::Error>> {
    let batch = tiny_batch(0, 3);
    // Same correction as RECORDS02: `select_columns` needs an `Arc<dyn RecordView>` receiver.
    let view: Arc<dyn RecordView> = (batch.clone() as Arc<dyn RecordView>).select_columns(&["id"])?;
    drop(batch); // the caller's own handle to the base is gone; `view` holds its own Arc to it
    assert_eq!(view.value(1, 0)?, FieldValue::Int(1));
    Ok(())
}

// wasm's counterpart — a JS-visible handle surviving `memory.grow` — belongs to `liquers-web`, not
// here: see `liquers-web/tests/records_RECORDS.rs` (Step 7.1).

// --- RECORDS06 — releasing the last handle releases the value ----------------------------------

#[test]
fn records06_dropping_the_last_arc_makes_the_weak_handle_unresolvable() -> Result<(), Box<dyn std::error::Error>> {
    let batch = tiny_batch(0, 1);
    let view: Arc<dyn RecordView> = batch;
    let weak: Weak<dyn RecordView> = Arc::downgrade(&view);
    assert!(weak.upgrade().is_some());
    drop(view);
    assert!(weak.upgrade().is_none());
    Ok(())
}

// --- RECORDS07 — a manifest-backed source is traversed one chunk at a time ---------------------

/// A minimal `RecordSource` over an explicit id list, used only to exercise the streaming
/// *contract* — `futures::stream::unfold` resolving one id at a time — independent of
/// `ManifestSource::stream`'s own (now-written) body.
#[derive(Debug)]
struct SequentialSource {
    ids: Vec<ChunkId>,
}

impl RecordSource for SequentialSource {
    fn stream(
        self: Arc<Self>,
        resolver: Arc<dyn ChunkResolver>,
    ) -> BoxFuture<'static, Result<liquers_records::BoxRecordStream, Error>> {
        Box::pin(async move {
            let ids = self.ids.clone();
            let inner = futures::stream::unfold((ids.into_iter(), resolver), move |(mut ids, resolver)| async move {
                let id = ids.next()?;
                let query = match &id {
                    ChunkId::Query(q) => q.clone(),
                    ChunkId::Key(_) => return None, // this fixture only produces query-identified chunks
                };
                let result = resolver.evaluate(query).await.map(|cv| match cv {
                    ChunkValue::View(v) => v,
                    ChunkValue::Source(_) => unreachable!("this fixture's resolver only ever returns ChunkValue::View"),
                    ChunkValue::Bytes { .. } => unreachable!("this fixture's resolver only ever returns ChunkValue::View"),
                });
                Some((result, (ids, resolver)))
            });
            Ok(record_stream(Box::pin(inner), None))
        })
    }
    fn chunks(&self) -> liquers_records::ChunkList<'_> {
        liquers_records::ChunkList::Known(&self.ids)
    }
    fn describe_chunk<'a>(
        &'a self,
        _id: &'a ChunkId,
        _resolver: &'a dyn ChunkResolver,
    ) -> BoxFuture<'a, Result<liquers_records::ChunkDescriptor, Error>> {
        Box::pin(async { Err(Error::general_error("not needed by this fixture".to_string())) })
    }
}

struct CountingResolver {
    outstanding: Arc<AtomicUsize>,
    max_outstanding: Arc<AtomicUsize>,
}
impl ChunkResolver for CountingResolver {
    fn evaluate(&self, query: Query) -> BoxFuture<'static, Result<ChunkValue, Error>> {
        let outstanding = self.outstanding.clone();
        let max_outstanding = self.max_outstanding.clone();
        Box::pin(async move {
            let n = outstanding.fetch_add(1, Ordering::SeqCst) + 1;
            max_outstanding.fetch_max(n, Ordering::SeqCst);
            let index: i64 = query
                .encode()
                .strip_prefix("chunk-")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            let view = tiny_batch(index, 1) as Arc<dyn RecordView>;
            outstanding.fetch_sub(1, Ordering::SeqCst); // released once this chunk is resolved
            Ok(ChunkValue::View(view))
        })
    }
    fn metadata(&self, _query: Query) -> BoxFuture<'static, Result<Metadata, Error>> {
        Box::pin(async { Ok(Metadata::default()) })
    }
    fn read_resource(&self, _key: Key) -> BoxFuture<'static, Result<(Vec<u8>, Metadata), Error>> {
        Box::pin(async { Err(Error::general_error("not used by this fixture".to_string())) })
    }
}

#[tokio::test]
async fn records07_stream_never_holds_more_than_one_chunk_resolution_at_a_time() -> Result<(), Box<dyn std::error::Error>> {
    // `Query` has no `FromStr` (`parse_query` is the parser entry point — CLAUDE.md's
    // `liquers-validate` guidance), so this cannot be `i.to_string().parse()` as Phase 3 §6's
    // draft had it (that would not compile). `"chunk-<i>"` also sidesteps any ambiguity a bare
    // number might have as a resource path vs. an action.
    let ids: Vec<ChunkId> = (0..5)
        .map(|i| {
            ChunkId::Query(
                liquers_core::parse::parse_query(&format!("chunk-{i}")).expect("test query"),
            )
        })
        .collect();
    let source = Arc::new(SequentialSource { ids });
    let outstanding = Arc::new(AtomicUsize::new(0));
    let max_outstanding = Arc::new(AtomicUsize::new(0));
    let resolver: Arc<dyn ChunkResolver> = Arc::new(CountingResolver {
        outstanding: outstanding.clone(),
        max_outstanding: max_outstanding.clone(),
    });

    let mut stream = source.stream(resolver).await?;
    let mut count = 0;
    while let Some(view) = stream.next().await {
        let _ = view?;
        count += 1;
    }
    assert_eq!(count, 5);
    assert_eq!(max_outstanding.load(Ordering::SeqCst), 1); // never more than one resident
    Ok(())
}

// --- RECORDS08 — draining a stream and materializing yield identical rows ----------------------

#[tokio::test]
async fn records08_stream_drain_and_materialize_agree() -> Result<(), Box<dyn std::error::Error>> {
    let views: Vec<Arc<dyn RecordView>> = vec![tiny_batch(0, 2), tiny_batch(2, 2)];
    let source = Arc::new(InMemorySource::new(views));
    let resolver: Arc<dyn ChunkResolver> = Arc::new(NullResolver);

    let stream = source.clone().stream(resolver.clone()).await?;
    let drained: Vec<i64> = stream
        .filter_map(|r| async { r.ok() })
        .flat_map(|v| futures::stream::iter((0..v.len()).map(move |i| v.value(i, 0).unwrap())))
        .filter_map(|fv| async move { match fv { FieldValue::Int(n) => Some(n), _ => None } })
        .collect()
        .await;

    let materialized = source.materialize(resolver, 1_000).await?;
    let mut from_batch = Vec::new();
    for i in 0..materialized.len {
        if let FieldValue::Int(n) = materialized.value(i, 0)? {
            from_batch.push(n);
        }
    }
    assert_eq!(drained, from_batch);
    Ok(())
}

// --- RECORDS09 — NA. Rust has no separate sync/async-model split within one binding: `RecordView`
// is always synchronous and `RecordSource` always async (§"Views are synchronous"), so there is no
// "language has no async model" case to fall back from.

// --- RECORDS11 — a user-defined RecordView gives the same rows through column/value/materialize -

struct ConstantView {
    schema: Arc<RecordSchema>,
    len: usize,
    value: i64,
}
impl std::fmt::Debug for ConstantView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConstantView").field("len", &self.len).finish()
    }
}
impl RecordView for ConstantView {
    fn schema(&self) -> &Arc<RecordSchema> { &self.schema }
    fn len(&self) -> usize { self.len }
    fn column_range(&self, col: usize, rows: std::ops::Range<usize>) -> Result<Column, Error> {
        if col != 0 {
            return Err(Error::general_error(format!("ConstantView has one column, got {col}")));
        }
        if rows.end > self.len {
            return Err(Error::general_error(format!("range {rows:?} out of bounds for len {}", self.len)));
        }
        let values = vec![self.value; rows.len()];
        Ok(Column::Int { validity: None, values: Buffer::from_slice(&values) })
    }
}

#[test]
fn records11_a_user_defined_view_agrees_across_column_value_and_materialize() -> Result<(), Box<dyn std::error::Error>> {
    let schema = Arc::new(RecordSchema::new(vec![FieldSchema::new("value", FieldType::Int)])?);
    let view = ConstantView { schema, len: 4, value: 7 };
    let materialized = view.materialize()?;
    for i in 0..view.len() {
        assert_eq!(view.value(i, 0)?, FieldValue::Int(7));
        assert_eq!(materialized.value(i, 0)?, FieldValue::Int(7));
    }
    assert_eq!(view.column(0)?.len(), 4);
    Ok(())
}
