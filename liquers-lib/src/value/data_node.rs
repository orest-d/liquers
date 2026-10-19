//! [`DataNode`] for [`SimpleValue`], mirroring core's impl for its own `Value`, so a template (or
//! anything else reading values by name) walks a value without converting the whole of it.

use std::any::Any;
use std::borrow::Cow;

use liquers_core::data_node::{
    snapshot_field, snapshot_keys, DataNode, KeyIter, NodeIter, NodeKind, NodeRef, OpaqueNode,
    Scalar,
};
use liquers_core::error::Error;
use liquers_core::value::ValueInterface;

use crate::value::simple::SimpleValue;

impl DataNode for SimpleValue {
    fn kind(&self) -> NodeKind {
        match self {
            SimpleValue::None {} => NodeKind::Null,
            SimpleValue::Bool { .. } => NodeKind::Bool,
            SimpleValue::I32 { .. } | SimpleValue::I64 { .. } => NodeKind::Integer,
            SimpleValue::F64 { .. } => NodeKind::Float,
            SimpleValue::Text { .. } | SimpleValue::Query { .. } | SimpleValue::Key { .. } => {
                NodeKind::Text
            }
            SimpleValue::Array { .. } | SimpleValue::AssetInfo { .. } => NodeKind::List,
            SimpleValue::Object { .. }
            | SimpleValue::Metadata { .. }
            | SimpleValue::Recipe { .. }
            | SimpleValue::CommandMetadata { .. } => NodeKind::Map,
            SimpleValue::Bytes { .. } => NodeKind::Opaque,
        }
    }
    fn scalar(&self) -> Option<Scalar<'_>> {
        match self {
            SimpleValue::None {} => Some(Scalar::Null),
            SimpleValue::Bool { value } => Some(Scalar::Bool(*value)),
            SimpleValue::I32 { value } => Some(Scalar::Integer(i64::from(*value))),
            SimpleValue::I64 { value } => Some(Scalar::Integer(*value)),
            SimpleValue::F64 { value } => Some(Scalar::Float(*value)),
            SimpleValue::Text { value } => Some(Scalar::Text(Cow::Borrowed(value))),
            SimpleValue::Query { value } => Some(Scalar::Text(Cow::Owned(value.encode()))),
            SimpleValue::Key { value } => Some(Scalar::Text(Cow::Owned(value.encode()))),
            SimpleValue::Array { .. }
            | SimpleValue::Object { .. }
            | SimpleValue::Bytes { .. }
            | SimpleValue::Metadata { .. }
            | SimpleValue::AssetInfo { .. }
            | SimpleValue::Recipe { .. }
            | SimpleValue::CommandMetadata { .. } => None,
        }
    }
    fn get(&self, name: &str) -> Option<NodeRef<'_>> {
        match self {
            SimpleValue::Object { value } => {
                value.get(name).map(|v| NodeRef::Borrowed(v as &dyn DataNode))
            }
            SimpleValue::Metadata { value } => value.get(name),
            SimpleValue::Recipe { value } => snapshot_field(value, name),
            SimpleValue::CommandMetadata { value } => snapshot_field(value, name),
            SimpleValue::None {}
            | SimpleValue::Bool { .. }
            | SimpleValue::I32 { .. }
            | SimpleValue::I64 { .. }
            | SimpleValue::F64 { .. }
            | SimpleValue::Text { .. }
            | SimpleValue::Array { .. }
            | SimpleValue::Bytes { .. }
            | SimpleValue::AssetInfo { .. }
            | SimpleValue::Query { .. }
            | SimpleValue::Key { .. } => None,
        }
    }
    fn len(&self) -> Option<usize> {
        match self {
            SimpleValue::Array { value } => Some(value.len()),
            SimpleValue::AssetInfo { value } => Some(value.len()),
            SimpleValue::None {}
            | SimpleValue::Bool { .. }
            | SimpleValue::I32 { .. }
            | SimpleValue::I64 { .. }
            | SimpleValue::F64 { .. }
            | SimpleValue::Text { .. }
            | SimpleValue::Object { .. }
            | SimpleValue::Bytes { .. }
            | SimpleValue::Metadata { .. }
            | SimpleValue::Recipe { .. }
            | SimpleValue::CommandMetadata { .. }
            | SimpleValue::Query { .. }
            | SimpleValue::Key { .. } => None,
        }
    }
    fn items(&self) -> Box<dyn NodeIter<'_> + '_> {
        match self {
            SimpleValue::Array { value } => {
                Box::new(value.iter().map(|v| NodeRef::Borrowed(v as &dyn DataNode)))
            }
            SimpleValue::AssetInfo { value } => {
                Box::new(value.iter().map(|info| match serde_json::to_value(info) {
                    Ok(json) => NodeRef::Owned(Box::new(json)),
                    Err(_) => NodeRef::Owned(Box::new(OpaqueNode)),
                }))
            }
            SimpleValue::None {}
            | SimpleValue::Bool { .. }
            | SimpleValue::I32 { .. }
            | SimpleValue::I64 { .. }
            | SimpleValue::F64 { .. }
            | SimpleValue::Text { .. }
            | SimpleValue::Object { .. }
            | SimpleValue::Bytes { .. }
            | SimpleValue::Metadata { .. }
            | SimpleValue::Recipe { .. }
            | SimpleValue::CommandMetadata { .. }
            | SimpleValue::Query { .. }
            | SimpleValue::Key { .. } => Box::new(std::iter::empty()),
        }
    }
    fn keys(&self) -> Box<dyn KeyIter<'_> + '_> {
        match self {
            SimpleValue::Object { value } => {
                Box::new(value.keys().map(|k| Cow::Borrowed(k.as_str())))
            }
            SimpleValue::Metadata { value } => value.keys(),
            SimpleValue::Recipe { value } => snapshot_keys(value),
            SimpleValue::CommandMetadata { value } => snapshot_keys(value),
            SimpleValue::None {}
            | SimpleValue::Bool { .. }
            | SimpleValue::I32 { .. }
            | SimpleValue::I64 { .. }
            | SimpleValue::F64 { .. }
            | SimpleValue::Text { .. }
            | SimpleValue::Array { .. }
            | SimpleValue::Bytes { .. }
            | SimpleValue::AssetInfo { .. }
            | SimpleValue::Query { .. }
            | SimpleValue::Key { .. } => Box::new(std::iter::empty()),
        }
    }
    fn index(&self, i: usize) -> Option<NodeRef<'_>> {
        match self {
            SimpleValue::Array { value } => {
                value.get(i).map(|v| NodeRef::Borrowed(v as &dyn DataNode))
            }
            SimpleValue::None {}
            | SimpleValue::Bool { .. }
            | SimpleValue::I32 { .. }
            | SimpleValue::I64 { .. }
            | SimpleValue::F64 { .. }
            | SimpleValue::Text { .. }
            | SimpleValue::Object { .. }
            | SimpleValue::Bytes { .. }
            | SimpleValue::Metadata { .. }
            | SimpleValue::AssetInfo { .. }
            | SimpleValue::Recipe { .. }
            | SimpleValue::CommandMetadata { .. }
            | SimpleValue::Query { .. }
            | SimpleValue::Key { .. } => self.items().nth(i),
        }
    }
    fn to_json(&self) -> Result<serde_json::Value, Error> {
        match self {
            SimpleValue::Query { value } => Ok(serde_json::Value::String(value.encode())),
            SimpleValue::Key { value } => Ok(serde_json::Value::String(value.encode())),
            SimpleValue::None {}
            | SimpleValue::Bool { .. }
            | SimpleValue::I32 { .. }
            | SimpleValue::I64 { .. }
            | SimpleValue::F64 { .. }
            | SimpleValue::Text { .. }
            | SimpleValue::Array { .. }
            | SimpleValue::Object { .. }
            | SimpleValue::Bytes { .. }
            | SimpleValue::Metadata { .. }
            | SimpleValue::AssetInfo { .. }
            | SimpleValue::Recipe { .. }
            | SimpleValue::CommandMetadata { .. } => self.try_into_json_value(),
        }
    }
    fn as_any(&self) -> Option<&dyn Any> {
        Some(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn simple_value_as_data_node() {
        let mut map = BTreeMap::new();
        map.insert("title".to_string(), SimpleValue::from("T"));
        map.insert(
            "tags".to_string(),
            SimpleValue::Array {
                value: vec![SimpleValue::from("a"), SimpleValue::from(2i64)],
            },
        );
        let value = SimpleValue::Object { value: map };
        assert_eq!(value.kind(), NodeKind::Map);
        let title = value.get("title").expect("title");
        assert_eq!(title.node().scalar().map(|s| s.to_text().into_owned()), Some("T".to_string()));
        let tags = value.get("tags").expect("tags");
        assert_eq!(tags.node().len(), Some(2));
        let second = tags.node().index(1).expect("item");
        assert_eq!(second.node().kind(), NodeKind::Integer);
        let keys: Vec<String> = value.keys().map(|k| k.into_owned()).collect();
        assert_eq!(keys, vec!["tags", "title"]);
    }
}
