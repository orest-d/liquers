//! A polars `DataFrame` as a [`DataNode`]: a list of rows, each a map from column name to cell,
//! built one at a time when it is asked for, so a template never converts the whole frame.

use std::borrow::Cow;
use std::sync::Arc;

use liquers_core::data_node::{DataNode, KeyIter, NodeIter, NodeKind, NodeRef, Scalar};
use liquers_core::error::Error;
use polars::prelude::{AnyValue, DataFrame};

/// A cell as a scalar: null, a bool, an integer, a float or a string, and anything else as its
/// display text.
fn cell(value: AnyValue<'_>) -> Scalar<'static> {
    if value.is_null() {
        return Scalar::Null;
    }
    if let AnyValue::Boolean(b) = value {
        return Scalar::Bool(b);
    }
    if let Some(text) = value.get_str() {
        return Scalar::Text(Cow::Owned(text.to_string()));
    }
    if value.is_integer() {
        if let Some(i) = value.extract::<i64>() {
            return Scalar::Integer(i);
        }
    }
    if value.is_float() {
        if let Some(f) = value.extract::<f64>() {
            return Scalar::Float(f);
        }
    }
    Scalar::Text(Cow::Owned(value.to_string()))
}

fn scalar_json(scalar: &Scalar<'_>) -> serde_json::Value {
    scalar.to_json().unwrap_or(serde_json::Value::Null)
}

struct FrameRow<'a> {
    frame: &'a DataFrame,
    row: usize,
}

impl FrameRow<'_> {
    fn value(&self, name: &str) -> Option<Scalar<'static>> {
        let column = self.frame.column(name).ok()?;
        column.get(self.row).ok().map(cell)
    }
}

impl DataNode for FrameRow<'_> {
    fn kind(&self) -> NodeKind {
        NodeKind::Map
    }
    fn scalar(&self) -> Option<Scalar<'_>> {
        None
    }
    fn get(&self, name: &str) -> Option<NodeRef<'_>> {
        self.value(name).map(NodeRef::Scalar)
    }
    fn len(&self) -> Option<usize> {
        Some(self.frame.width())
    }
    fn items(&self) -> Box<dyn NodeIter<'_> + '_> {
        Box::new(
            self.frame
                .get_column_names()
                .into_iter()
                .filter_map(move |name| self.value(name.as_str()).map(NodeRef::Scalar)),
        )
    }
    fn keys(&self) -> Box<dyn KeyIter<'_> + '_> {
        Box::new(
            self.frame
                .get_column_names()
                .into_iter()
                .map(|name| Cow::Borrowed(name.as_str())),
        )
    }
    fn to_json(&self) -> Result<serde_json::Value, Error> {
        let mut map = serde_json::Map::new();
        for name in self.frame.get_column_names() {
            let value = self.value(name.as_str()).unwrap_or(Scalar::Null);
            map.insert(name.to_string(), scalar_json(&value));
        }
        Ok(serde_json::Value::Object(map))
    }
}

/// A frame as a list of rows.
pub struct DataFrameNode {
    frame: Arc<DataFrame>,
}

impl DataFrameNode {
    pub fn new(frame: Arc<DataFrame>) -> Self {
        DataFrameNode { frame }
    }
}

impl DataNode for DataFrameNode {
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
        Some(self.frame.height())
    }
    fn items(&self) -> Box<dyn NodeIter<'_> + '_> {
        let frame: &DataFrame = self.frame.as_ref();
        Box::new((0..frame.height()).map(move |row| NodeRef::Owned(Box::new(FrameRow { frame, row }))))
    }
    fn keys(&self) -> Box<dyn KeyIter<'_> + '_> {
        Box::new(std::iter::empty())
    }
    fn index(&self, i: usize) -> Option<NodeRef<'_>> {
        let frame: &DataFrame = self.frame.as_ref();
        (i < frame.height()).then(|| NodeRef::Owned(Box::new(FrameRow { frame, row: i })) as NodeRef<'_>)
    }
    fn to_json(&self) -> Result<serde_json::Value, Error> {
        let frame: &DataFrame = self.frame.as_ref();
        (0..frame.height())
            .map(|row| FrameRow { frame, row }.to_json())
            .collect::<Result<Vec<_>, Error>>()
            .map(serde_json::Value::Array)
    }
    /// The frame itself (an `Arc<DataFrame>`), so a host can pass it on without a copy.
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(&self.frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use polars::prelude::*;

    #[test]
    fn frame_rows_are_maps() {
        let df = df!("name" => ["a", "b"], "n" => [1i64, 2]).expect("frame");
        let node = DataFrameNode::new(Arc::new(df));
        assert_eq!(node.len(), Some(2));
        let row = node.index(1).expect("row");
        let name = row.node().get("name").expect("name");
        assert_eq!(name.node().scalar().map(|s| s.to_text().into_owned()), Some("b".to_string()));
        assert_eq!(row.node().get("n").and_then(|n| n.node().scalar().map(|s| s.kind())), Some(NodeKind::Integer));
        assert_eq!(node.to_json().expect("json"), serde_json::json!([{"name": "a", "n": 1}, {"name": "b", "n": 2}]));
    }
}
