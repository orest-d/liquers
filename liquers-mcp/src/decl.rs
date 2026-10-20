//! Declarations of tool sets: what a binary hands to [`crate::ToolRegistry::build`].
//!
//! A tool set is data, usually a YAML file compiled into the binary that serves it. Each tool is
//! one encoded query whose last action is the command; the engine derives the tool's argument
//! schema from that command's metadata, so a tool never drifts from the command it runs.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// A named group of tools sharing a prefix, a working key and an optional query tool.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolSetDecl {
    /// Prepended to every tool name of the set, e.g. `memory_`.
    pub prefix: String,
    /// The set's root, used as the recipe's `cwd`, so relative keys resolve under it.
    #[serde(default)]
    pub cwd: Option<String>,
    /// Joined into the server's instructions.
    #[serde(default)]
    pub instructions: String,
    pub tools: Vec<ToolDecl>,
    /// The power layer: a tool taking any query whose commands the allow-list permits.
    #[serde(default)]
    pub query_tool: Option<QueryToolDecl>,
    #[serde(default)]
    pub resources: Vec<ResourceDecl>,
}

/// One tool: an encoded query with exactly one action, the command the tool runs.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolDecl {
    /// The name without the set's prefix.
    pub name: String,
    /// The encoded query. Its filename picks the default output format.
    pub query: String,
    /// An argument standing for the key of the query's leading resource segment.
    #[serde(default)]
    pub key: Option<KeyArgumentDecl>,
    /// Defaults to the command's `doc`.
    #[serde(default)]
    pub description: Option<String>,
    /// Argument values the tool sets itself and does not offer.
    #[serde(default)]
    pub fixed: BTreeMap<String, serde_json::Value>,
    /// The MCP `readOnlyHint`.
    #[serde(default)]
    pub read_only: bool,
}

/// The tool argument that is joined onto the base key of the tool query.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KeyArgumentDecl {
    /// `key` or `path`, typically.
    pub name: String,
    /// Defaults to "Key relative to the root".
    #[serde(default)]
    pub description: Option<String>,
    /// When false, an empty or absent argument leaves the query's own key.
    #[serde(default)]
    pub required: bool,
}

/// The query tool of a set: any query, as long as every command it runs is allowed.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QueryToolDecl {
    /// The name without the set's prefix, e.g. `query`.
    pub name: String,
    /// `CommandKey` Display forms (`-mem-list`), or patterns ending in `*` (`-rec-*`).
    pub allow: Vec<String>,
    #[serde(default)]
    pub description: Option<String>,
}

/// A resource the server offers.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ResourceDecl {
    /// A URI template whose one variable is passed to a tool of the same set as `argument`,
    /// e.g. `liquers://memory/{+key}` reading through the `read` tool.
    Tool {
        uri_template: String,
        tool: String,
        argument: String,
    },
    /// A fixed text, such as a query guide compiled into the binary.
    Text {
        uri: String,
        name: String,
        description: String,
        text: String,
    },
    /// `liquers://commands/{namespace}`: the namespace's command reference, rendered from the
    /// registry so it never drifts from it.
    Commands {
        uri_template: String,
        namespaces: Vec<String>,
    },
}
