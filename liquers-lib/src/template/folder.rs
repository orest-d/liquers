//! A store folder as template data: a map from child name to child, each read only when a
//! template asks for it.

use std::pin::Pin;

use futures::StreamExt;
use liquers_core::context::{Context, Environment};
use liquers_core::data_node::{AnyNode, AsyncDataNode, NodeKind, NodeStream, Scalar};
use liquers_core::error::Error;
use liquers_core::parse::parse_query;
use liquers_core::query::{Key, Query};

use crate::value::Value;

/// A folder: its children in listing order. A file child is read through
/// `Context::get_dependency_state`, so it is a dependency of the render; a folder child is
/// another `FolderNode`, listed the same way.
pub struct FolderNode<E: Environment<Value = Value>> {
    context: Context<E>,
    children: Vec<(String, Key, bool)>,
}

impl<E: Environment<Value = Value>> FolderNode<E> {
    /// A folder with `children` as `(name, key, is_dir)`.
    pub fn new(context: Context<E>, children: Vec<(String, Key, bool)>) -> Self {
        FolderNode { context, children }
    }

    async fn child(&self, key: &Key, is_dir: bool) -> Result<AnyNode, Error> {
        let query: Query = if is_dir {
            parse_query(&format!("-R-dir/{}", key.encode()))?
        } else {
            Query::from(key)
        };
        let state = self.context.get_dependency_state(&query).await?;
        state.error_result()?;
        let host = super::host::ContextHost::new(self.context.clone(), Key::new());
        Ok(host.node_of(&state))
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<E: Environment<Value = Value>> AsyncDataNode for FolderNode<E> {
    fn kind(&self) -> NodeKind {
        NodeKind::Map
    }
    async fn scalar(&self) -> Result<Option<Scalar<'static>>, Error> {
        Ok(None)
    }
    async fn get(&self, name: &str) -> Result<Option<AnyNode>, Error> {
        match self.children.iter().find(|(child, _, _)| child == name) {
            Some((_, key, is_dir)) => self.child(key, *is_dir).await.map(Some),
            None => Ok(None),
        }
    }
    fn items(&self) -> Pin<Box<dyn NodeStream<'_> + '_>> {
        Box::pin(
            futures::stream::iter(self.children.iter())
                .then(move |(_, key, is_dir)| self.child(key, *is_dir)),
        )
    }
    /// The children's names: a folder is not read whole to be written out.
    async fn to_json(&self) -> Result<serde_json::Value, Error> {
        Ok(serde_json::Value::Array(
            self.children
                .iter()
                .map(|(name, _, _)| serde_json::Value::String(name.clone()))
                .collect(),
        ))
    }
}
