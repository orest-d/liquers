//! Phase 3 §5.7 (fixed): `RecordSource::stream`'s doc comment claims the returned stream is
//! `'static` because it owns an `Arc<Self>` and an owned resolver. Checked here by moving the
//! stream into a spawned task *after* the source and the local resolver handle are both dropped —
//! this could not compile, let alone run, if the stream borrowed either.

use std::sync::Arc;

use futures::stream::StreamExt;
use liquers_core::{
    error::Error,
    maybe_send::BoxFuture,
    metadata::Metadata,
    query::{Key, Query},
};
use liquers_records::{ChunkResolver, ChunkValue, InMemorySource, RecordSource};

struct EmptyResolver;
impl ChunkResolver for EmptyResolver {
    fn evaluate(&self, _query: Query) -> BoxFuture<'static, Result<ChunkValue, Error>> {
        Box::pin(async {
            Err(Error::general_error(
                "not needed: InMemorySource resolves no chunk queries".to_string(),
            ))
        })
    }
    fn metadata(&self, _query: Query) -> BoxFuture<'static, Result<Metadata, Error>> {
        Box::pin(async { Ok(Metadata::default()) })
    }
    fn read_resource(&self, _key: Key) -> BoxFuture<'static, Result<(Vec<u8>, Metadata), Error>> {
        Box::pin(async { Err(Error::general_error("not needed".to_string())) })
    }
}

#[tokio::test]
async fn stream_outlives_the_source_arc_that_opened_it() -> Result<(), Box<dyn std::error::Error>> {
    let view: Arc<dyn liquers_records::RecordView> = Arc::new(liquers_records::RecordBatch::new(
        Arc::new(liquers_records::RecordSchema::new(vec![
            liquers_records::FieldSchema::new("value", liquers_records::FieldType::Int),
        ])?),
        vec![liquers_records::Column::Int {
            validity: None,
            values: liquers_records::Buffer::from_slice(&[1i64, 2, 3]),
        }],
        None,
        None,
        vec![],
    )?);
    let source = Arc::new(InMemorySource::new(vec![view]));
    let resolver: Arc<dyn ChunkResolver> = Arc::new(EmptyResolver);

    // `RecordSource::stream` takes `self: Arc<Self>` by value — cloning the `Arc` here, rather
    // than moving `source` itself, is what leaves a caller-side handle to drop afterward. (The
    // original sketch called `source.stream(...)` directly, which would move `source` and make
    // the following `drop(source)` a use-after-move — corrected here.)
    let stream = source.clone().stream(resolver.clone()).await?;
    drop(source); // the value a caller normally holds is gone
    drop(resolver); // and so is the local resolver handle

    // Moving the stream into a spawned task proves `'static`: it could not compile, let alone
    // run, if the stream borrowed either dropped value.
    let rows: usize = tokio::spawn(async move {
        let batches: Vec<_> = stream.collect().await;
        batches.into_iter().flatten().map(|b| b.len()).sum()
    })
    .await?;

    assert_eq!(rows, 3);
    Ok(())
}
