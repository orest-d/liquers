//! [`FilteredSource`]: a [`RecordSource`] whose stream yields each view of an inner source
//! filtered by a predicate — lazily, chunk by chunk, never materialized.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use futures::StreamExt;
use liquers_core::error::Error;
use liquers_core::maybe_send::BoxFuture;

use crate::batch::{record_stream, BoxRecordStream, ChunkDescriptor, ChunkId, ChunkList};
use crate::schema::RecordSchema;
use crate::search::predicate::{BoundPredicate, Predicate, SearchOptions};
use crate::value::{ChunkResolver, RecordSource};
use crate::RecordView;

/// A lazily filtered source.
///
/// - `chunks` and `describe_chunk` describe the **unfiltered** input (provenance); they do not
///   say which chunks will yield rows.
/// - The stream stops after `options.limit` rows. A stop part-way through a chunk cuts that view to
///   exactly the remaining count. After a stop, `truncated()` is true.
/// - The predicate is bound once per distinct schema. Warnings from binding and evaluation are
///   kept and read with [`FilteredSource::warnings`].
#[derive(Debug)]
pub struct FilteredSource {
    inner: Arc<dyn RecordSource>,
    predicate: Arc<dyn Predicate>,
    options: SearchOptions,
    warnings: Arc<Mutex<Vec<String>>>,
    stopped: Arc<AtomicBool>,
}

impl FilteredSource {
    pub fn new(
        inner: Arc<dyn RecordSource>,
        predicate: Arc<dyn Predicate>,
        options: SearchOptions,
    ) -> Self {
        FilteredSource {
            inner,
            predicate,
            options,
            warnings: Arc::new(Mutex::new(Vec::new())),
            stopped: Arc::new(AtomicBool::new(false)),
        }
    }

    /// The warnings collected so far by streams of this source.
    pub fn warnings(&self) -> Vec<String> {
        lock(&self.warnings).clone()
    }
}

fn lock(warnings: &Mutex<Vec<String>>) -> std::sync::MutexGuard<'_, Vec<String>> {
    warnings.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

struct StreamState {
    inner: BoxRecordStream,
    predicate: Arc<dyn Predicate>,
    options: SearchOptions,
    bound: Option<(Arc<RecordSchema>, Box<dyn BoundPredicate>)>,
    remaining: Option<usize>,
    warnings: Arc<Mutex<Vec<String>>>,
    stopped: Arc<AtomicBool>,
}

impl StreamState {
    fn filter_view(
        &mut self,
        view: Arc<dyn RecordView>,
    ) -> Result<Option<Arc<dyn RecordView>>, Error> {
        let schema = Arc::clone(view.schema());
        let rebind = match &self.bound {
            Some((bound_schema, _)) => !Arc::ptr_eq(bound_schema, &schema) && **bound_schema != *schema,
            None => true,
        };
        if rebind {
            let bound = self.predicate.bind(&schema, &self.options)?;
            lock(&self.warnings).extend(bound.warnings);
            self.bound = Some((Arc::clone(&schema), bound.matcher));
        }
        let Some((_, matcher)) = &self.bound else {
            return Err(Error::general_error("FilteredSource: predicate not bound".to_string()));
        };
        let mask = matcher.mask(view.as_ref())?;
        lock(&self.warnings).extend(matcher.take_warnings());
        let filtered = view.filter(&mask)?;
        if filtered.is_empty() {
            return Ok(None);
        }
        let filtered = match self.remaining {
            Some(remaining) if filtered.len() > remaining => filtered.slice(0, remaining)?,
            Some(_) | None => filtered,
        };
        if let Some(remaining) = self.remaining.as_mut() {
            *remaining -= filtered.len();
        }
        Ok(Some(filtered))
    }
}

impl RecordSource for FilteredSource {
    fn stream(
        self: Arc<Self>,
        resolver: Arc<dyn ChunkResolver>,
    ) -> BoxFuture<'static, Result<BoxRecordStream, Error>> {
        Box::pin(async move {
            let inner = RecordSource::stream(Arc::clone(&self.inner), resolver).await?;
            let schema = inner.schema();
            let state = StreamState {
                inner,
                predicate: Arc::clone(&self.predicate),
                options: self.options.clone(),
                bound: None,
                remaining: self.options.limit,
                warnings: Arc::clone(&self.warnings),
                stopped: Arc::clone(&self.stopped),
            };
            let stream = futures::stream::unfold(state, |mut state| async move {
                loop {
                    if state.remaining == Some(0) {
                        state.stopped.store(true, Ordering::SeqCst);
                        return None;
                    }
                    match state.inner.next().await {
                        None => return None,
                        Some(Err(error)) => return Some((Err(error), state)),
                        Some(Ok(view)) => match state.filter_view(view) {
                            Ok(Some(filtered)) => return Some((Ok(filtered), state)),
                            Ok(None) => continue,
                            Err(error) => return Some((Err(error), state)),
                        },
                    }
                }
            });
            Ok(record_stream(Box::pin(stream), schema))
        })
    }

    fn chunks(&self) -> ChunkList<'_> {
        self.inner.chunks()
    }

    fn describe_chunk<'a>(
        &'a self,
        id: &'a ChunkId,
        resolver: &'a dyn ChunkResolver,
    ) -> BoxFuture<'a, Result<ChunkDescriptor, Error>> {
        self.inner.describe_chunk(id, resolver)
    }

    fn schema(&self) -> Option<Arc<RecordSchema>> {
        self.inner.schema()
    }

    fn truncated(&self) -> bool {
        self.inner.truncated() || self.stopped.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::column::FieldValue;
    use crate::mutable::{RecordBatchMut, RecordViewMut};
    use crate::schema::{FieldRole, FieldSchema, FieldType};
    use crate::search::rank::search_view;
    use crate::search::syntax::SearchExpr;
    use crate::search::kernels::text_at;
    use crate::search::predicate::{Bound, RankTerm};
    use crate::value::ChunkValue;
    use crate::Bitmap;
    use liquers_core::metadata::Metadata;
    use liquers_core::query::{Key, Query};
    use std::sync::atomic::AtomicUsize;

    fn chunk(name: &str, texts: &[&str]) -> Arc<dyn RecordView> {
        let schema = Arc::new(
            RecordSchema::new(vec![
                FieldSchema::new(name, FieldType::Text).with_role(FieldRole::text()),
            ])
            .expect("schema"),
        );
        let mut batch = RecordBatchMut::with_capacity(schema, texts.len());
        for text in texts {
            batch.append_row(&[FieldValue::Text(Arc::from(*text))]).expect("row");
        }
        Arc::new(batch.freeze().expect("freeze"))
    }

    /// Streams fixed views and counts how many the consumer pulled.
    #[derive(Debug)]
    struct CountingSource {
        views: Vec<Arc<dyn RecordView>>,
        pulled: Arc<AtomicUsize>,
    }

    impl RecordSource for CountingSource {
        fn stream(
            self: Arc<Self>,
            _resolver: Arc<dyn ChunkResolver>,
        ) -> BoxFuture<'static, Result<BoxRecordStream, Error>> {
            Box::pin(async move {
                let pulled = Arc::clone(&self.pulled);
                let views = self.views.clone();
                let inner = futures::stream::iter(views).map(move |view| {
                    pulled.fetch_add(1, Ordering::SeqCst);
                    Ok(view)
                });
                Ok(record_stream(Box::pin(inner), None))
            })
        }

        fn chunks(&self) -> ChunkList<'_> {
            ChunkList::Known(&[])
        }

        fn describe_chunk<'a>(
            &'a self,
            _id: &'a ChunkId,
            _resolver: &'a dyn ChunkResolver,
        ) -> BoxFuture<'a, Result<ChunkDescriptor, Error>> {
            Box::pin(async move { Err(Error::general_error("not used".to_string())) })
        }
    }

    struct NoResolver;

    impl ChunkResolver for NoResolver {
        fn evaluate(&self, _query: Query) -> BoxFuture<'static, Result<ChunkValue, Error>> {
            Box::pin(async move { Err(Error::general_error("not used".to_string())) })
        }
        fn metadata(&self, _query: Query) -> BoxFuture<'static, Result<Metadata, Error>> {
            Box::pin(async move { Ok(Metadata::new()) })
        }
        fn read_resource(&self, _key: Key) -> BoxFuture<'static, Result<(Vec<u8>, Metadata), Error>> {
            Box::pin(async move { Err(Error::general_error("not used".to_string())) })
        }
    }

    async fn collect(source: Arc<FilteredSource>) -> Vec<String> {
        let mut stream = RecordSource::stream(source, Arc::new(NoResolver)).await.expect("stream");
        let mut out = Vec::new();
        while let Some(view) = stream.next().await {
            let view = view.expect("view");
            let column = view.column(0).expect("column");
            for row in 0..view.len() {
                out.push(text_at(&column, row).unwrap_or("").to_string());
            }
        }
        out
    }

    #[tokio::test]
    async fn filtered_source_is_lazy_and_stops_at_limit() {
        let pulled = Arc::new(AtomicUsize::new(0));
        let inner = Arc::new(CountingSource {
            views: vec![
                chunk("text", &["x1", "y", "x2"]),
                chunk("text", &["x3", "x4"]),
                chunk("text", &["x5"]),
                chunk("text", &["x6"]),
            ],
            pulled: Arc::clone(&pulled),
        });
        let expr: Arc<dyn Predicate> = Arc::new(SearchExpr::try_from("x").expect("parse"));
        let options = SearchOptions { limit: Some(3), ..SearchOptions::default() };
        let source = Arc::new(FilteredSource::new(inner, expr, options));
        assert!(!source.truncated());
        let rows = collect(Arc::clone(&source)).await;
        assert_eq!(rows, vec!["x1", "x2", "x3"]);
        // The second chunk was cut to the one remaining row; the third was never pulled.
        assert_eq!(pulled.load(Ordering::SeqCst), 2);
        assert!(source.truncated());
    }

    #[tokio::test]
    async fn filtered_source_rebinds_on_schema_change() {
        let inner = Arc::new(CountingSource {
            views: vec![chunk("a", &["hit", "miss"]), chunk("b", &["hit again"])],
            pulled: Arc::new(AtomicUsize::new(0)),
        });
        let expr: Arc<dyn Predicate> = Arc::new(SearchExpr::try_from("hit a:*").expect("parse"));
        let source = Arc::new(FilteredSource::new(inner, expr, SearchOptions::default()));
        let rows = collect(Arc::clone(&source)).await;
        // `a:*` exists only in the first schema: the second chunk binds afresh, warns, matches nothing.
        assert_eq!(rows, vec!["hit"]);
        let warnings = source.warnings();
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].contains("'a'"), "{warnings:?}");
    }

    /// A hand-written language: "every row whose first column contains the word". It ranks the
    /// same word, so its search must agree with the built-in `word` expression exactly.
    #[derive(Debug)]
    struct ContainsFirst(String);

    struct ContainsFirstBound(String);

    impl BoundPredicate for ContainsFirstBound {
        fn mask(&self, view: &dyn RecordView) -> Result<Bitmap, Error> {
            let column = view.column(0)?;
            let bits: Vec<bool> = (0..view.len())
                .map(|row| text_at(&column, row).is_some_and(|t| t.to_lowercase().contains(&self.0)))
                .collect();
            Ok(Bitmap::from_bools(&bits))
        }
        fn evidence(&self, view: &dyn RecordView) -> Result<Vec<(usize, Bitmap)>, Error> {
            Ok(vec![(0, self.mask(view)?)])
        }
    }

    impl Predicate for ContainsFirst {
        fn bind(&self, _schema: &RecordSchema, _options: &SearchOptions) -> Result<Bound, Error> {
            Ok(Bound { matcher: Box::new(ContainsFirstBound(self.0.clone())), warnings: vec![] })
        }
        fn matches_json(&self, _value: &serde_json::Value, _options: &SearchOptions) -> Result<bool, Error> {
            Ok(false)
        }
        fn has_field_tests(&self) -> bool {
            false
        }
        fn rank_terms(&self) -> Vec<RankTerm> {
            vec![RankTerm::Term(self.0.clone())]
        }
        fn to_syntax(&self) -> String {
            self.0.clone()
        }
    }

    #[test]
    fn custom_predicate_type_filters_and_ranks_like_builtin() {
        let view = chunk("text", &["one word", "word word word", "none", "a word here"]);
        let builtin = SearchExpr::try_from("word").expect("parse");
        let custom = ContainsFirst("word".to_string());
        for rank in [false, true] {
            let (a, ra) = search_view(&view, &builtin, &SearchOptions::default(), rank).expect("builtin");
            let (b, rb) = search_view(&view, &custom, &SearchOptions::default(), rank).expect("custom");
            assert_eq!(ra, rb);
            assert_eq!(a.len(), b.len());
            for row in 0..a.len() {
                for col in 0..a.schema().fields.len() {
                    assert_eq!(a.value(row, col).expect("a"), b.value(row, col).expect("b"), "row {row} col {col} rank {rank}");
                }
            }
        }
    }
}
