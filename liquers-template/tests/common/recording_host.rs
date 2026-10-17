//! A host that records every query a template asks for and answers from a map.

use std::collections::BTreeMap;
use std::sync::Mutex;

use liquers_core::command_metadata::{ArgumentInfo, CommandMetadata, CommandMetadataRegistry};
use liquers_core::data_node::AnyNode;
use liquers_core::error::Error;
use liquers_core::query::Key;
use liquers_template::{QueryRequest, TemplateHost};

/// A recorded request, with the input as JSON.
#[derive(Debug, Clone, PartialEq)]
pub struct Recorded {
    pub query: String,
    pub overrides: Vec<(String, serde_json::Value)>,
    pub input: Option<serde_json::Value>,
}

pub struct RecordingHost {
    /// Answers by encoded query; anything else answers `"ok"`.
    pub answers: BTreeMap<String, Result<serde_json::Value, String>>,
    pub partials: BTreeMap<Key, String>,
    pub requests: Mutex<Vec<Recorded>>,
    pub warnings: Mutex<Vec<Error>>,
    pub registry: CommandMetadataRegistry,
}

impl RecordingHost {
    /// A registry in which `cmd` has one argument, `arg`.
    pub fn new() -> Self {
        let mut registry = CommandMetadataRegistry::new();
        let mut cmd = CommandMetadata::new("cmd");
        cmd.arguments = vec![ArgumentInfo::any_argument("arg")];
        registry.add_command(&cmd);
        RecordingHost {
            answers: BTreeMap::new(),
            partials: BTreeMap::new(),
            requests: Mutex::new(Vec::new()),
            warnings: Mutex::new(Vec::new()),
            registry,
        }
    }

    pub fn answer(mut self, query: &str, value: serde_json::Value) -> Self {
        self.answers.insert(query.to_string(), Ok(value));
        self
    }

    pub fn fail(mut self, query: &str, message: &str) -> Self {
        self.answers.insert(query.to_string(), Err(message.to_string()));
        self
    }

    pub fn partial(mut self, key: &str, text: &str) -> Self {
        self.partials
            .insert(liquers_core::parse::parse_key(key).expect("key"), text.to_string());
        self
    }

    pub fn requests(&self) -> Vec<Recorded> {
        self.requests.lock().expect("lock").clone()
    }

    pub fn warnings(&self) -> Vec<Error> {
        self.warnings.lock().expect("lock").clone()
    }
}

#[async_trait::async_trait]
impl TemplateHost for RecordingHost {
    async fn evaluate(&self, request: QueryRequest) -> Result<AnyNode, Error> {
        let input = match &request.input {
            Some(node) => Some(node.to_json().await?),
            None => None,
        };
        let query = request.query.encode();
        self.requests.lock().expect("lock").push(Recorded {
            query: query.clone(),
            overrides: request.overrides.clone(),
            input,
        });
        match self.answers.get(&query) {
            Some(Ok(value)) => Ok(AnyNode::from_json(value.clone())),
            Some(Err(message)) => Err(Error::general_error(message.clone())),
            None => Ok(AnyNode::from_json(serde_json::json!("ok"))),
        }
    }

    async fn partial(&self, key: &Key) -> Result<String, Error> {
        self.partials
            .get(key)
            .cloned()
            .ok_or_else(|| Error::key_not_found(key))
    }

    fn command_registry(&self) -> &CommandMetadataRegistry {
        &self.registry
    }

    fn warn(&self, error: &Error) {
        self.warnings.lock().expect("lock").push(error.clone());
    }
}
