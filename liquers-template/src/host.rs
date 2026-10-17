//! What a renderer asks of its environment: evaluating a query, loading a partial, naming an
//! override's argument, and logging a recovered error.

use std::collections::BTreeMap;
use std::sync::Mutex;

use liquers_core::command_metadata::CommandMetadataRegistry;
use liquers_core::data_node::AnyNode;
use liquers_core::error::Error;
use liquers_core::maybe_send::{MaybeSend, MaybeSync};
use liquers_core::query::{Key, Position, Query};

/// A query a template asks the host to evaluate.
#[derive(Debug, Clone)]
pub struct QueryRequest {
    pub query: Query,
    /// Arguments that non-scalar placeholders override (`Recipe::arguments`).
    pub overrides: Vec<(String, serde_json::Value)>,
    /// The input of an apply tag (`{{$. q}}`, `{{$.name q}}`); `None` evaluates with no input.
    pub input: Option<AnyNode>,
    /// The tag, in the template.
    pub position: Position,
}

/// The environment a template is rendered in.
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait TemplateHost: MaybeSend + MaybeSync {
    /// Evaluates a query tag's query.
    async fn evaluate(&self, request: QueryRequest) -> Result<AnyNode, Error>;
    /// The text of the partial at `key`.
    async fn partial(&self, key: &Key) -> Result<String, Error>;
    /// The commands, which name the arguments that overrides fill.
    fn command_registry(&self) -> &CommandMetadataRegistry;
    /// A recovered error (expand and silent modes), for the log.
    fn warn(&self, error: &Error);
}

/// A host with partials and no queries: for templates that use data only, and for tests. A partial
/// it does not hold is empty, as the Mustache spec requires; a host over a store reports it.
#[derive(Default)]
pub struct MapHost {
    pub partials: BTreeMap<Key, String>,
    pub warnings: Mutex<Vec<Error>>,
    pub registry: CommandMetadataRegistry,
}

impl MapHost {
    pub fn new() -> Self {
        MapHost {
            partials: BTreeMap::new(),
            warnings: Mutex::new(Vec::new()),
            registry: CommandMetadataRegistry::new(),
        }
    }

    pub fn with_partial(mut self, key: Key, text: &str) -> Self {
        self.partials.insert(key, text.to_string());
        self
    }

    /// The warnings logged so far.
    pub fn warnings(&self) -> Vec<Error> {
        match self.warnings.lock() {
            Ok(w) => w.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
impl TemplateHost for MapHost {
    async fn evaluate(&self, request: QueryRequest) -> Result<AnyNode, Error> {
        Err(Error::not_supported(format!(
            "This template is rendered without an environment, so the query '{}' cannot be evaluated",
            request.query.encode()
        ))
        .with_position(&request.position))
    }

    async fn partial(&self, key: &Key) -> Result<String, Error> {
        Ok(self.partials.get(key).cloned().unwrap_or_default())
    }

    fn command_registry(&self) -> &CommandMetadataRegistry {
        &self.registry
    }

    fn warn(&self, error: &Error) {
        match self.warnings.lock() {
            Ok(mut w) => w.push(error.clone()),
            Err(poisoned) => poisoned.into_inner().push(error.clone()),
        }
    }
}
