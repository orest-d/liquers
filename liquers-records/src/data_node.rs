//! Record views and sources as [`DataNode`]s: a list of rows, each row a map from field name to
//! cell. A view is read synchronously and builds a row only when it is asked for; a source can
//! only stream through a [`ChunkResolver`], so it is an [`AsyncDataNode`] that holds one. See
//! `specs/design/liquers-mustache/data-node-interface.md`.

use std::borrow::Cow;
use std::pin::Pin;
use std::sync::Arc;

use futures::StreamExt;
use liquers_core::data_node::{
    AnyNode, AsyncDataNode, DataNode, KeyIter, NodeIter, NodeKind, NodeRef, NodeStream, Scalar,
};
use liquers_core::error::Error;

use crate::batch::RecordView;
use crate::column::FieldValue;
use crate::formats::ndjson::scalar_to_json;
use crate::value::{ChunkResolver, RecordSource, DEFAULT_MATERIALIZE_MAX_ROWS};

/// A cell as a node: a scalar, or the JSON form of a vector. Bytes, dates and timestamps are text
/// in the form a JSON table writes them (base64, ISO 8601, RFC 3339).
fn cell_node<'a>(value: FieldValue) -> Option<NodeRef<'a>> {
    match value {
        FieldValue::Null => Some(NodeRef::Scalar(Scalar::Null)),
        FieldValue::Bool(b) => Some(NodeRef::Scalar(Scalar::Bool(b))),
        FieldValue::Int(i) => Some(NodeRef::Scalar(Scalar::Integer(i))),
        FieldValue::UInt(u) => Some(NodeRef::Scalar(match i64::try_from(u) {
            Ok(i) => Scalar::Integer(i),
            Err(_) => Scalar::Float(u as f64),
        })),
        FieldValue::Float(f) => Some(NodeRef::Scalar(Scalar::Float(f))),
        FieldValue::Text(t) => Some(NodeRef::Scalar(Scalar::Text(Cow::Owned(t.to_string())))),
        FieldValue::Vector(_) => scalar_to_json(&value)
            .ok()
            .map(|json| NodeRef::Owned(Box::new(json))),
        FieldValue::Bytes(_) | FieldValue::Date(_) | FieldValue::Timestamp(_) => {
            match scalar_to_json(&value).ok()? {
                serde_json::Value::String(text) => Some(NodeRef::Scalar(Scalar::Text(Cow::Owned(text)))),
                other => Some(NodeRef::Owned(Box::new(other))),
            }
        }
    }
}

/// Whatever holds the view a row reads from: borrowed from a [`RecordViewNode`], or shared when the
/// row came out of a source's stream.
trait ViewHolder: liquers_core::maybe_send::MaybeSend + liquers_core::maybe_send::MaybeSync {
    fn view(&self) -> &dyn RecordView;
}

impl ViewHolder for &dyn RecordView {
    fn view(&self) -> &dyn RecordView {
        *self
    }
}

impl ViewHolder for Arc<dyn RecordView> {
    fn view(&self) -> &dyn RecordView {
        self.as_ref()
    }
}

/// One row: a map in schema field order. A cell that cannot be read is missing.
struct RowNode<V: ViewHolder> {
    view: V,
    row: usize,
}

impl<V: ViewHolder> DataNode for RowNode<V> {
    fn kind(&self) -> NodeKind {
        NodeKind::Map
    }
    fn scalar(&self) -> Option<Scalar<'_>> {
        None
    }
    fn get(&self, name: &str) -> Option<NodeRef<'_>> {
        let view = self.view.view();
        let col = view.schema().index_of(name)?;
        cell_node(view.value(self.row, col).ok()?)
    }
    fn len(&self) -> Option<usize> {
        Some(self.view.view().schema().fields.len())
    }
    fn items(&self) -> Box<dyn NodeIter<'_> + '_> {
        let view = self.view.view();
        Box::new(
            (0..view.schema().fields.len())
                .filter_map(move |col| view.value(self.row, col).ok().and_then(cell_node)),
        )
    }
    fn keys(&self) -> Box<dyn KeyIter<'_> + '_> {
        Box::new(
            self.view
                .view()
                .schema()
                .fields
                .iter()
                .map(|field| Cow::Borrowed(field.name.as_str())),
        )
    }
    fn to_json(&self) -> Result<serde_json::Value, Error> {
        crate::formats::ndjson::row_to_json(self.view.view(), self.row)
    }
}

/// A record view as a list of rows.
pub struct RecordViewNode {
    view: Arc<dyn RecordView>,
}

impl RecordViewNode {
    pub fn new(view: Arc<dyn RecordView>) -> Self {
        RecordViewNode { view }
    }
}

impl DataNode for RecordViewNode {
    fn kind(&self) -> NodeKind {
        NodeKind::List
    }
    fn scalar(&self) -> Option<Scalar<'_>> {
        None
    }
    fn get(&self, _name: &str) -> Option<NodeRef<'_>> {
        None
    }
    fn len(&self) -> Option<usize> {
        Some(self.view.len())
    }
    fn items(&self) -> Box<dyn NodeIter<'_> + '_> {
        let view: &dyn RecordView = self.view.as_ref();
        Box::new((0..view.len()).map(move |row| NodeRef::Owned(Box::new(RowNode { view, row }))))
    }
    fn keys(&self) -> Box<dyn KeyIter<'_> + '_> {
        Box::new(std::iter::empty())
    }
    fn index(&self, i: usize) -> Option<NodeRef<'_>> {
        let view: &dyn RecordView = self.view.as_ref();
        (i < view.len()).then(|| NodeRef::Owned(Box::new(RowNode { view, row: i })) as NodeRef<'_>)
    }
    fn to_json(&self) -> Result<serde_json::Value, Error> {
        crate::formats::shapes::to_json(self.view.as_ref(), crate::formats::shapes::JsonOrient::Records)
    }
}

/// A record source as a list of rows, read by streaming it through `resolver`. Each row is a sync
/// node over the view it arrived in.
pub struct RecordSourceNode {
    source: Arc<dyn RecordSource>,
    resolver: Arc<dyn ChunkResolver>,
}

impl RecordSourceNode {
    pub fn new(source: Arc<dyn RecordSource>, resolver: Arc<dyn ChunkResolver>) -> Self {
        RecordSourceNode { source, resolver }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl AsyncDataNode for RecordSourceNode {
    fn kind(&self) -> NodeKind {
        NodeKind::List
    }
    async fn scalar(&self) -> Result<Option<Scalar<'static>>, Error> {
        Ok(None)
    }
    async fn get(&self, _name: &str) -> Result<Option<AnyNode>, Error> {
        Ok(None)
    }
    fn items(&self) -> Pin<Box<dyn NodeStream<'_> + '_>> {
        let opened = RecordSource::stream(self.source.clone(), self.resolver.clone());
        Box::pin(
            futures::stream::once(opened)
                .map(|stream| match stream {
                    Ok(views) => views
                        .map(|view| match view {
                            Ok(view) => futures::stream::iter(
                                (0..view.len())
                                    .map(|row| {
                                        Ok(AnyNode::Sync(Arc::new(RowNode {
                                            view: view.clone(),
                                            row,
                                        })))
                                    })
                                    .collect::<Vec<_>>(),
                            )
                            .left_stream(),
                            Err(e) => futures::stream::iter(vec![Err(e)]).right_stream(),
                        })
                        .flatten()
                        .left_stream(),
                    Err(e) => futures::stream::iter(vec![Err(e)]).right_stream(),
                })
                .flatten(),
        )
    }
    async fn to_json(&self) -> Result<serde_json::Value, Error> {
        let batch = RecordSource::materialize(
            self.source.clone(),
            self.resolver.clone(),
            DEFAULT_MATERIALIZE_MAX_ROWS,
        )
        .await?;
        crate::formats::shapes::to_json(batch.as_ref(), crate::formats::shapes::JsonOrient::Records)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formats::shapes::{from_json, JsonOrient};
    use crate::formats::ReadSchema;
    use crate::sources::InMemorySource;
    use crate::value::ChunkValue;
    use liquers_core::maybe_send::BoxFuture;
    use liquers_core::metadata::Metadata;
    use liquers_core::query::{Key, Query};

    fn view() -> Arc<dyn RecordView> {
        let json = serde_json::json!([
            {"id": "A-1", "title": "first", "open": true},
            {"id": "A-2", "title": "second", "open": false}
        ]);
        Arc::new(from_json(&json, JsonOrient::Records, ReadSchema::Infer).expect("batch"))
    }

    fn texts(node: &dyn DataNode, field: &str) -> Vec<String> {
        node.items()
            .filter_map(|row| {
                row.node()
                    .get(field)
                    .and_then(|cell| cell.node().scalar().map(|s| s.to_text().into_owned()))
            })
            .collect()
    }

    #[test]
    fn record_view_rows_as_maps() {
        let node = RecordViewNode::new(view());
        assert_eq!(node.kind(), NodeKind::List);
        assert_eq!(node.len(), Some(2));
        assert_eq!(texts(&node, "title"), vec!["first", "second"]);
        let first = node.items().next().expect("a row");
        let keys: Vec<String> = first.node().keys().map(|k| k.into_owned()).collect();
        // Schema order; inference from a JSON object sorts the fields.
        assert_eq!(keys, vec!["id", "open", "title"]);
        assert!(first.node().get("missing").is_none());
        assert_eq!(
            node.to_json().ok(),
            Some(serde_json::json!([
                {"id": "A-1", "title": "first", "open": true},
                {"id": "A-2", "title": "second", "open": false}
            ]))
        );
    }

    #[test]
    fn record_view_index_builds_one_row() {
        let node = RecordViewNode::new(view());
        let second = node.index(1).expect("row 1");
        let open = second.node().get("open").expect("open");
        assert!(!open.node().is_truthy());
        assert_eq!(
            second.node().get("id").and_then(|c| c.node().scalar().map(|s| s.into_owned())),
            Some(Scalar::Text(Cow::Owned("A-2".to_string())))
        );
        assert!(node.index(2).is_none());
    }

    /// `InMemorySource` reads nothing through its resolver.
    struct NoResolver;

    impl ChunkResolver for NoResolver {
        fn evaluate(&self, _query: Query) -> BoxFuture<'static, Result<ChunkValue, Error>> {
            Box::pin(async { Err(Error::general_error("no resolver".to_string())) })
        }
        fn metadata(&self, _query: Query) -> BoxFuture<'static, Result<Metadata, Error>> {
            Box::pin(async { Err(Error::general_error("no resolver".to_string())) })
        }
        fn read_resource(&self, _key: Key) -> BoxFuture<'static, Result<(Vec<u8>, Metadata), Error>> {
            Box::pin(async { Err(Error::general_error("no resolver".to_string())) })
        }
    }

    #[tokio::test]
    async fn record_source_streams_rows() {
        let source: Arc<dyn RecordSource> = Arc::new(InMemorySource::new(vec![view(), view()]));
        let node = RecordSourceNode::new(source, Arc::new(NoResolver));
        assert_eq!(AsyncDataNode::kind(&node), NodeKind::List);
        let mut titles = Vec::new();
        let mut rows = node.items();
        while let Some(row) = rows.next().await {
            let row = row.expect("row");
            let row = row.as_sync().expect("a row is sync");
            let title = row.get("title").and_then(|c| c.node().scalar().map(|s| s.to_text().into_owned()));
            titles.push(title.unwrap_or_default());
        }
        assert_eq!(titles, vec!["first", "second", "first", "second"]);
        drop(rows);
        let third = node.index(2).await.expect("read").expect("row 2");
        assert_eq!(
            third.to_json().await.ok(),
            Some(serde_json::json!({"id": "A-1", "title": "first", "open": true}))
        );
        let json = AsyncDataNode::to_json(&node).await.expect("json");
        assert_eq!(json.as_array().map(|a| a.len()), Some(4));
    }
}
