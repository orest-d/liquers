//! [`ContextHost`]: rendering a template inside a command, with its queries evaluated through the
//! command's [`Context`].

use std::any::Any;
use std::pin::Pin;
use std::sync::Arc;

use liquers_core::command_metadata::CommandMetadataRegistry;
use liquers_core::context::{Context, EnvRef, Environment};
use liquers_core::data_node::{
    AnyNode, AsyncDataNode, DataNode, KeyIter, NodeIter, NodeKind, NodeRef, NodeStream, Scalar,
};
use liquers_core::error::Error;
use liquers_core::metadata::MetadataRecord;
use liquers_core::query::{Key, Query};
use liquers_core::recipes::Recipe;
use liquers_core::state::State;
use liquers_core::value::ValueInterface;
use liquers_template::{QueryRequest, TemplateHost};

use crate::value::{SimpleValue, Value};

/// A template host over a command's [`Context`].
///
/// A query with no input and no overrides is a dependency of the command
/// (`Context::get_dependency_state`), cached and shared; one with an apply input or a parameter
/// override is evaluated ad hoc (`Context::apply_ad_hoc`), and its own dependencies are passed on.
/// Relative keys, in queries and partials, resolve against `cwd`. A recovered error is logged as a
/// warning of the command.
pub struct ContextHost<E: Environment<Value = Value>> {
    context: Context<E>,
    envref: EnvRef<E>,
    cwd: Key,
}

impl<E: Environment<Value = Value>> ContextHost<E> {
    pub fn new(context: Context<E>, cwd: Key) -> Self {
        let envref = context.get_envref();
        ContextHost {
            context,
            envref,
            cwd,
        }
    }

    /// A state as template data. A record source streams through this context (so the chunks it
    /// reads are dependencies), a directory listing is a folder of its children, and every other
    /// value is its own `data_node`.
    ///
    /// The node answers `_meta` with the state's metadata record (none for legacy metadata), and
    /// an apply tag given it receives the value itself, whatever its type.
    pub fn node_of(&self, state: &State<Value>) -> AnyNode {
        let value = state.data_unchecked().clone();
        let meta = state.metadata.metadata_record().map(Arc::new);
        match self.inner_node_of(&value) {
            AnyNode::Sync(inner) => AnyNode::Sync(Arc::new(StateNode { value, meta, inner })),
            AnyNode::Async(inner) => AnyNode::Async(Arc::new(AsyncStateNode { meta, inner })),
        }
    }

    fn inner_node_of(&self, value: &Arc<Value>) -> AnyNode {
        #[cfg(feature = "records")]
        {
            use crate::value::ExtValueInterface;
            if let Ok(source) = value.as_record_source() {
                let resolver: Arc<dyn liquers_records::ChunkResolver> =
                    Arc::new(liquers_records::ContextResolver::new(self.context.clone()));
                return AnyNode::Async(Arc::new(liquers_records::RecordSourceNode::new(
                    source, resolver,
                )));
            }
        }
        if let Some(listing) = directory_listing(value.as_ref()) {
            return AnyNode::Async(Arc::new(super::folder::FolderNode::new(
                self.context.clone(),
                listing,
            )));
        }
        Value::data_node(value.clone())
    }

    /// An apply tag's input as a state: the value itself when the node is a state's or a value's
    /// (`node_of`), its JSON otherwise (which keeps the data and drops the metadata).
    async fn input_state(&self, node: AnyNode) -> Result<State<Value>, Error> {
        if let AnyNode::Sync(n) = &node {
            if let Some(any) = n.as_any() {
                if let Some(value) = any.downcast_ref::<Arc<Value>>() {
                    return Ok(State::new().with_data(value.as_ref().clone()));
                }
                if let Some(value) = any.downcast_ref::<SimpleValue>() {
                    return Ok(State::new().with_data(Value::new_base(value.clone())));
                }
                #[cfg(feature = "records")]
                if let Some(view) = any.downcast_ref::<Arc<dyn liquers_records::RecordView>>() {
                    use crate::value::ExtValueInterface;
                    return Ok(State::new().with_data(Value::from_record_view(view.clone())));
                }
            }
        }
        let json = node.to_json().await?;
        Ok(State::new().with_data(Value::try_from_json_value(&json)?))
    }

    fn absolute(&self, query: &Query) -> Query {
        query.to_absolute(&self.cwd)
    }
}

/// The reserved name under which a state's metadata record is read.
const META: &str = liquers_template::name::META;

/// A state's node: the value's own node, plus `_meta` and the value itself for an apply tag.
struct StateNode {
    value: Arc<Value>,
    meta: Option<Arc<MetadataRecord>>,
    inner: Arc<dyn DataNode>,
}

impl DataNode for StateNode {
    fn kind(&self) -> NodeKind {
        self.inner.kind()
    }
    fn scalar(&self) -> Option<Scalar<'_>> {
        self.inner.scalar()
    }
    fn get(&self, name: &str) -> Option<NodeRef<'_>> {
        if name == META {
            return self
                .meta
                .as_ref()
                .map(|meta| NodeRef::Borrowed(meta.as_ref() as &dyn DataNode));
        }
        self.inner.get(name)
    }
    fn len(&self) -> Option<usize> {
        self.inner.len()
    }
    fn items(&self) -> Box<dyn NodeIter<'_> + '_> {
        self.inner.items()
    }
    fn keys(&self) -> Box<dyn KeyIter<'_> + '_> {
        self.inner.keys()
    }
    fn to_json(&self) -> Result<serde_json::Value, Error> {
        self.inner.to_json()
    }
    fn index(&self, i: usize) -> Option<NodeRef<'_>> {
        self.inner.index(i)
    }
    fn is_truthy(&self) -> bool {
        self.inner.is_truthy()
    }
    /// The value (an `Arc<Value>`), so an apply tag passes it on unconverted.
    fn as_any(&self) -> Option<&dyn Any> {
        Some(&self.value)
    }
}

/// As [`StateNode`], for a value read on demand (a record source, a folder).
struct AsyncStateNode {
    meta: Option<Arc<MetadataRecord>>,
    inner: Arc<dyn AsyncDataNode>,
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl AsyncDataNode for AsyncStateNode {
    fn kind(&self) -> NodeKind {
        self.inner.kind()
    }
    async fn scalar(&self) -> Result<Option<Scalar<'static>>, Error> {
        self.inner.scalar().await
    }
    async fn get(&self, name: &str) -> Result<Option<AnyNode>, Error> {
        if name == META {
            return Ok(self
                .meta
                .as_ref()
                .map(|meta| AnyNode::Sync(meta.clone() as Arc<dyn DataNode>)));
        }
        self.inner.get(name).await
    }
    fn items(&self) -> Pin<Box<dyn NodeStream<'_> + '_>> {
        self.inner.items()
    }
    async fn to_json(&self) -> Result<serde_json::Value, Error> {
        self.inner.to_json().await
    }
    async fn len(&self) -> Result<Option<usize>, Error> {
        self.inner.len().await
    }
    async fn index(&self, i: usize) -> Result<Option<AnyNode>, Error> {
        self.inner.index(i).await
    }
}

/// The entries of a directory listing (`-R-dir/…`), as `(name, key, is_dir)`; `None` for any other
/// value, or a listing with an entry that has no key.
fn directory_listing(value: &Value) -> Option<Vec<(String, Key, bool)>> {
    match value {
        Value::Base(SimpleValue::AssetInfo { value }) => value
            .iter()
            .map(|info| {
                let key = info.key.clone()?;
                let name = key.filename()?.encode().to_string();
                Some((name, key, info.is_dir))
            })
            .collect(),
        Value::Base(_) | Value::Extended(_) => None,
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl<E: Environment<Value = Value>> TemplateHost for ContextHost<E> {
    async fn evaluate(&self, request: QueryRequest) -> Result<AnyNode, Error> {
        let query = self.absolute(&request.query);
        let state = if request.input.is_none() && request.overrides.is_empty() {
            self.context.get_dependency_state(&query).await?
        } else {
            let mut recipe = Recipe::from(&query);
            recipe.arguments = request.overrides.into_iter().collect();
            let input = match request.input {
                Some(node) => self.input_state(node).await?,
                None => State::new(),
            };
            self.context.apply_ad_hoc(recipe, input).await?.get().await?
        };
        state.error_result()?;
        Ok(self.node_of(&state))
    }

    async fn partial(&self, key: &Key) -> Result<String, Error> {
        let query = Query::from(key.to_absolute(&self.cwd));
        let state = self.context.get_dependency_state(&query).await?;
        state.error_result()?;
        state.try_into_string()
    }

    fn command_registry(&self) -> &CommandMetadataRegistry {
        self.envref.get_command_metadata_registry()
    }

    fn warn(&self, error: &Error) {
        // Logging a warning fails only when the asset is gone, and then there is no log to keep.
        let _ = self.context.warning(&error.to_string());
    }
}
