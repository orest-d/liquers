//! The value-adapter trait (`RecordValue`), the resolver interface a source reaches evaluation
//! through (`ChunkResolver`, `ChunkValue`), and `RecordSource` itself, with its provided
//! `materialize`.
//!
//! `liquers-records` cannot name `liquers-lib`'s `Value`, which sits above it, so a source reaches
//! a command's value type through the adapter trait below — the pattern `ValueExtension` already
//! uses in the other direction. See `specs/design/record-streams/phase2-architecture.md`,
//! §"`ChunkResolver` — how a source reaches evaluation" and §"Value extension — `ExtValue`, not
//! core's `Value`".

use std::fmt::Debug;
use std::sync::Arc;

use liquers_core::error::Error;
use liquers_core::maybe_send::{BoxFuture, MaybeSend, MaybeSync};
use liquers_core::metadata::Metadata;
use liquers_core::query::{Key, Query};
use liquers_core::value::ValueInterface;

use crate::batch::{
    BoxRecordStream, ChunkDescriptor, ChunkId, ChunkList, RecordBatch, RecordStreamExt, RecordView,
};
use crate::manifest::ManifestSpec;
use crate::schema::RecordSchema;

/// Default `max_rows` for `RecordSource::materialize`, used by a command's default parameter
/// (Step 5.x) when a caller does not name one. `1_000_000`, per phase2-architecture.md §"A source
/// serializes only as its manifest" — raised explicitly (`materialize-5000000`), never unbounded.
/// The trait method itself always takes `max_rows` explicitly; Rust has no default parameter
/// values, so this constant is where the documented default actually lives.
pub const DEFAULT_MATERIALIZE_MAX_ROWS: usize = 1_000_000;

/// Something that can be asked, repeatedly, for a stream of views. The `Iterable` of this design:
/// shareable, never consumed by use.
pub trait RecordSource: Debug + MaybeSend + MaybeSync + 'static {
    /// Open a fresh traversal. Callable any number of times — this is what replaces `rewind`.
    /// `Arc<Self>` and an owned resolver make the stream `'static`, which an HTTP body needs: it
    /// outlives the handler that opened it (phase2-architecture.md §"Streaming a record source
    /// over HTTP").
    fn stream(
        self: Arc<Self>,
        resolver: Arc<dyn ChunkResolver>,
    ) -> BoxFuture<'static, Result<BoxRecordStream, Error>>;

    /// Chunks without producing any records — the reconciliation primitive. Sync, no I/O.
    fn chunks(&self) -> ChunkList<'_>;

    /// Full provenance for one chunk. Reads metadata, so it is async.
    fn describe_chunk<'a>(
        &'a self,
        id: &'a ChunkId,
        resolver: &'a dyn ChunkResolver,
    ) -> BoxFuture<'a, Result<ChunkDescriptor, Error>>;

    /// The schema every view will have, when the producer promises one. Declared, not assumed —
    /// see phase2-architecture.md §"Schema uniformity is declared, not assumed".
    fn schema(&self) -> Option<Arc<RecordSchema>> {
        None
    }

    /// A producer's report that it stopped early.
    fn truncated(&self) -> bool {
        false
    }

    /// The manifest, when this source has one — the **only** byte form a source has. `Some` for a
    /// `ManifestSource` (Step 4.1); `None` for every other source, which is then stored as
    /// metadata only and re-derived from its recipe (phase2-architecture.md §"A source serializes
    /// only as its manifest").
    fn manifest(&self) -> Option<&ManifestSpec> {
        None
    }

    /// Every row, as one table: open a stream and drain it. Refused past `max_rows`, and refused
    /// when the chunks' schemas differ — `RecordBatch::concat`, which `RecordStreamExt::materialize`
    /// calls internally, names the first differing field. Provided; a source that can do better —
    /// one batch already in memory, a database that returns a result set whole — overrides it.
    fn materialize(
        self: Arc<Self>,
        resolver: Arc<dyn ChunkResolver>,
        max_rows: usize,
    ) -> BoxFuture<'static, Result<Arc<RecordBatch>, Error>> {
        Box::pin(async move {
            let stream = RecordSource::stream(self, resolver).await?;
            RecordStreamExt::materialize(stream, max_rows).await
        })
    }
}

/// What a source needs from the environment. Object-safe, so `dyn RecordSource` can take it, and
/// free of any concrete value type, which `liquers-records` cannot name.
pub trait ChunkResolver: MaybeSend + MaybeSync + 'static {
    /// Evaluate a chunk query and wait for its value, as the records crate can use it.
    fn evaluate(&self, query: Query) -> BoxFuture<'static, Result<ChunkValue, Error>>;

    /// Read a chunk's metadata without producing its value — the store's metadata for a keyed
    /// chunk, the asset manager's for an unkeyed one.
    fn metadata(&self, query: Query) -> BoxFuture<'static, Result<Metadata, Error>>;

    /// A stored chunk's bytes and metadata, **without** deserializing them — so a manifest with a
    /// declared schema can parse the bytes with it. Records the key as a dependency when the
    /// resolver is a `ContextResolver` (`liquers-lib`, Step 5.x).
    fn read_resource(&self, key: Key) -> BoxFuture<'static, Result<(Vec<u8>, Metadata), Error>>;
}

/// A chunk's value as the records crate sees it.
#[derive(Debug, Clone)]
pub enum ChunkValue {
    View(Arc<dyn RecordView>),
    Source(Arc<dyn RecordSource>),
    /// Anything else, as bytes in its data format — parsed as a table by the source.
    Bytes { data: Vec<u8>, metadata: Metadata },
}

/// How the records crate reads and builds a Liquers value without knowing its type. Implemented
/// by `liquers-lib`'s `Value`; an integration with its own value type implements it too.
///
/// This crate has no test-local implementor: core's `Value` has no variant that can hold a view,
/// so a test-only impl could not honestly implement `from_record_view` / `from_record_source`.
/// `RecordValue` is exercised where `liquers-lib`'s `Value` implements it (Steps 5.2 and 5.6);
/// here its tests are limited to `ChunkResolver` fixtures that return `ChunkValue` directly.
pub trait RecordValue: ValueInterface {
    fn as_record_view(&self) -> Option<Arc<dyn RecordView>>;
    fn as_record_source(&self) -> Option<Arc<dyn RecordSource>>;
    fn from_record_view(view: Arc<dyn RecordView>) -> Self;
    fn from_record_source(source: Arc<dyn RecordSource>) -> Self;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::buffer::Buffer;
    use crate::column::{Column, FieldValue};
    use crate::schema::{FieldSchema, FieldType, KeyRole};
    use futures::StreamExt;
    use liquers_core::parse::parse_query;
    use std::sync::Mutex;

    fn schema_with_two_fields() -> Arc<RecordSchema> {
        Arc::new(
            RecordSchema::new(vec![
                FieldSchema::new("id", FieldType::Text).with_key(KeyRole::Id),
                FieldSchema::new("amount", FieldType::Int),
            ])
            .expect("schema"),
        )
    }

    fn other_schema() -> Arc<RecordSchema> {
        Arc::new(
            RecordSchema::new(vec![
                FieldSchema::new("id", FieldType::Text).with_key(KeyRole::Id),
                FieldSchema::new("amount", FieldType::Float), // differs from Int
            ])
            .expect("schema"),
        )
    }

    fn batch_view(schema: Arc<RecordSchema>, ids: &[&str], amounts: &[i64]) -> Arc<dyn RecordView> {
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
        Arc::new(
            RecordBatch::new(schema, vec![id_column, amount_column], None, None, Vec::new())
                .expect("batch"),
        )
    }

    fn float_batch_view(schema: Arc<RecordSchema>, ids: &[&str], amounts: &[f64]) -> Arc<dyn RecordView> {
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
        let amount_column = Column::Float {
            validity: None,
            values: Buffer::from_slice(amounts),
        };
        Arc::new(
            RecordBatch::new(schema, vec![id_column, amount_column], None, None, Vec::new())
                .expect("batch"),
        )
    }

    /// A fixture resolver: a fixed map from encoded query to the `ChunkValue` it resolves to,
    /// per phase2-architecture.md's Step 2.6 test note ("`ChunkResolver` fixtures that return
    /// `ChunkValue` directly").
    #[derive(Debug, Default)]
    struct FixtureResolver {
        views: Mutex<std::collections::HashMap<String, Arc<dyn RecordView>>>,
    }

    impl FixtureResolver {
        fn new() -> Self {
            FixtureResolver {
                views: Mutex::new(std::collections::HashMap::new()),
            }
        }

        fn with_view(self, query: &str, view: Arc<dyn RecordView>) -> Self {
            self.views
                .lock()
                .expect("lock")
                .insert(query.to_string(), view);
            self
        }
    }

    impl ChunkResolver for FixtureResolver {
        fn evaluate(&self, query: Query) -> BoxFuture<'static, Result<ChunkValue, Error>> {
            let encoded = query.encode();
            let view = self.views.lock().expect("lock").get(&encoded).cloned();
            Box::pin(async move {
                match view {
                    Some(view) => Ok(ChunkValue::View(view)),
                    None => Err(Error::general_error(format!(
                        "FixtureResolver: no view registered for query '{encoded}'"
                    ))),
                }
            })
        }

        fn metadata(&self, _query: Query) -> BoxFuture<'static, Result<Metadata, Error>> {
            Box::pin(async move { Ok(Metadata::new()) })
        }

        fn read_resource(&self, _key: Key) -> BoxFuture<'static, Result<(Vec<u8>, Metadata), Error>> {
            Box::pin(async move {
                Err(Error::general_error(
                    "FixtureResolver: read_resource is not used by this test".to_string(),
                ))
            })
        }
    }

    /// A minimal `RecordSource`: a fixed list of chunk queries, each resolved through the
    /// `ChunkResolver` it is given. Test-local, per Step 2.6's note that the records crate has no
    /// honest `RecordValue` implementor.
    #[derive(Debug)]
    struct TestSource {
        ids: Vec<ChunkId>,
        schema: Option<Arc<RecordSchema>>,
    }

    impl RecordSource for TestSource {
        fn stream(
            self: Arc<Self>,
            resolver: Arc<dyn ChunkResolver>,
        ) -> BoxFuture<'static, Result<BoxRecordStream, Error>> {
            Box::pin(async move {
                let schema = self.schema.clone();
                let ids = self.ids.clone();
                let inner = futures::stream::iter(ids.into_iter()).then(move |id| {
                    let resolver = resolver.clone();
                    async move {
                        let query = match id {
                            ChunkId::Query(query) => query,
                            ChunkId::Key(_) => {
                                return Err(Error::general_error(
                                    "TestSource: keyed chunks are not used by this fixture"
                                        .to_string(),
                                ))
                            }
                        };
                        match resolver.evaluate(query).await? {
                            ChunkValue::View(view) => Ok(view),
                            ChunkValue::Source(_) => Err(Error::general_error(
                                "TestSource: unexpected Source chunk value".to_string(),
                            )),
                            ChunkValue::Bytes { .. } => Err(Error::general_error(
                                "TestSource: unexpected Bytes chunk value".to_string(),
                            )),
                        }
                    }
                });
                Ok(crate::batch::record_stream(Box::pin(inner), schema))
            })
        }

        fn chunks(&self) -> ChunkList<'_> {
            ChunkList::Known(&self.ids)
        }

        fn describe_chunk<'a>(
            &'a self,
            id: &'a ChunkId,
            _resolver: &'a dyn ChunkResolver,
        ) -> BoxFuture<'a, Result<ChunkDescriptor, Error>> {
            let id = id.clone();
            Box::pin(async move {
                let query = match &id {
                    ChunkId::Query(query) => query.clone(),
                    ChunkId::Key(_) => {
                        return Err(Error::general_error(
                            "TestSource: keyed chunks are not used by this fixture".to_string(),
                        ))
                    }
                };
                Ok(ChunkDescriptor {
                    id,
                    query: query.clone(),
                    metadata: Metadata::new(),
                    origin: crate::batch::ChunkOrigin {
                        asset: query.clone(),
                        chunk: query,
                        info: None,
                        locator: None,
                    },
                    schema: None,
                })
            })
        }

        fn schema(&self) -> Option<Arc<RecordSchema>> {
            self.schema.clone()
        }
    }

    fn query_id(text: &str) -> ChunkId {
        ChunkId::Query(parse_query(text).expect("parse"))
    }

    // --- Compile-time object-safety checks -------------------------------------------------

    /// Only type-checks if `RecordSource` can be used as `Arc<dyn RecordSource>`.
    fn assert_usable_as_arc_dyn_record_source(source: Arc<dyn RecordSource>) -> Arc<dyn RecordSource> {
        source
    }

    /// Only type-checks if `ChunkResolver` can be used as `Arc<dyn ChunkResolver>`.
    fn assert_usable_as_arc_dyn_chunk_resolver(
        resolver: Arc<dyn ChunkResolver>,
    ) -> Arc<dyn ChunkResolver> {
        resolver
    }

    #[test]
    fn record_source_is_object_safe_as_arc_dyn() {
        let source: Arc<dyn RecordSource> = Arc::new(TestSource {
            ids: Vec::new(),
            schema: None,
        });
        let source = assert_usable_as_arc_dyn_record_source(source);
        assert!(matches!(source.chunks(), ChunkList::Known(ids) if ids.is_empty()));
    }

    #[test]
    fn chunk_resolver_is_object_safe_as_arc_dyn() {
        let resolver: Arc<dyn ChunkResolver> = Arc::new(FixtureResolver::new());
        let _resolver = assert_usable_as_arc_dyn_chunk_resolver(resolver);
    }

    #[test]
    fn chunk_value_constructs_all_three_variants() {
        let schema = schema_with_two_fields();
        let view = batch_view(schema, &["a"], &[1]);
        let _ = ChunkValue::View(view.clone());

        let source: Arc<dyn RecordSource> = Arc::new(TestSource {
            ids: Vec::new(),
            schema: None,
        });
        let _ = ChunkValue::Source(source);

        let _ = ChunkValue::Bytes {
            data: vec![1, 2, 3],
            metadata: Metadata::new(),
        };
    }

    // --- RecordSource::materialize -----------------------------------------------------------

    #[tokio::test]
    async fn record_source_materialize_concatenates_two_uniform_chunks() {
        let schema = schema_with_two_fields();
        let view_a = batch_view(schema.clone(), &["a"], &[1]);
        let view_b = batch_view(schema.clone(), &["b"], &[2]);
        let resolver: Arc<dyn ChunkResolver> = Arc::new(
            FixtureResolver::new()
                .with_view("data/a.csv", view_a)
                .with_view("data/b.csv", view_b),
        );
        let source: Arc<dyn RecordSource> = Arc::new(TestSource {
            ids: vec![query_id("data/a.csv"), query_id("data/b.csv")],
            schema: Some(schema),
        });

        let batch = source
            .materialize(resolver, 100)
            .await
            .expect("materialize");
        assert_eq!(batch.len, 2);
        assert_eq!(batch.value(0, 1).expect("value"), FieldValue::Int(1));
        assert_eq!(batch.value(1, 1).expect("value"), FieldValue::Int(2));
    }

    #[tokio::test]
    async fn record_source_materialize_of_a_non_uniform_source_names_the_differing_field() {
        let schema_a = schema_with_two_fields();
        let schema_b = other_schema();
        let view_a = batch_view(schema_a.clone(), &["a"], &[1]);
        let view_b = float_batch_view(schema_b, &["b"], &[2.5]);
        let resolver: Arc<dyn ChunkResolver> = Arc::new(
            FixtureResolver::new()
                .with_view("data/a.csv", view_a)
                .with_view("data/b.csv", view_b),
        );
        let source: Arc<dyn RecordSource> = Arc::new(TestSource {
            ids: vec![query_id("data/a.csv"), query_id("data/b.csv")],
            // The source declares no single schema — chunks disagree, per
            // phase2-architecture.md §"Schema uniformity is declared, not assumed".
            schema: None,
        });

        let error = source
            .materialize(resolver, 100)
            .await
            .expect_err("non-uniform chunks must be refused");
        assert!(format!("{error}").contains("amount"));
    }

    #[tokio::test]
    async fn record_source_materialize_exceeding_max_rows_is_refused() {
        let schema = schema_with_two_fields();
        let view_a = batch_view(schema.clone(), &["a"], &[1]);
        let view_b = batch_view(schema.clone(), &["b"], &[2]);
        let resolver: Arc<dyn ChunkResolver> = Arc::new(
            FixtureResolver::new()
                .with_view("data/a.csv", view_a)
                .with_view("data/b.csv", view_b),
        );
        let source: Arc<dyn RecordSource> = Arc::new(TestSource {
            ids: vec![query_id("data/a.csv"), query_id("data/b.csv")],
            schema: Some(schema),
        });

        let error = source
            .materialize(resolver, 1)
            .await
            .expect_err("exceeding max_rows must be refused");
        assert!(format!("{error}").contains("max_rows"));
    }
}
