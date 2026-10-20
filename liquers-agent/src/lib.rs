//! A file-based agent memory served over MCP and the command line.
//!
//! The binary reads one YAML configuration: the Liquers environment (`store`, `recipes`,
//! `assets`, unchanged) plus a `memory` section naming the memory's root and, optionally, tool
//! sets replacing the built-in one. Every subcommand calls a tool of the same server, so the CLI
//! and MCP share one code path and one allow-list.

use std::path::Path;

use liquers_core::context::EnvRef;
use liquers_core::environment_config::EnvironmentConfig;
use liquers_core::error::Error;
use liquers_lib::environment::{default_environment_builder, DefaultEnvironment};
use liquers_lib::ui::SimpleUIPayload;
use liquers_lib::value::Value;
use liquers_mcp::{McpServer, ResourceDecl, ToolRegistry, ToolSetDecl};
use serde::Deserialize;

/// The built-in tool set, before its `cwd` and query guide are filled in.
pub const MEMORY_TOOLS: &str = include_str!("../tools/memory.yaml");
/// The query guide served as `liquers://memory/query-guide`.
pub const QUERY_GUIDE: &str = include_str!("../tools/query-guide.md");
/// The URI of the query guide.
pub const QUERY_GUIDE_URI: &str = "liquers://memory/query-guide";

/// The environment the agent runs: the library's default, with the UI payload the `lui`
/// commands of `register_all_commands!` need.
pub type AgentEnvironment = DefaultEnvironment<Value, SimpleUIPayload>;

/// The `memory` section of the configuration.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct MemorySettings {
    /// The memory's root key; empty for the store's root.
    #[serde(default)]
    pub root: String,
    /// Tool sets replacing the built-in `memory` set.
    #[serde(default)]
    pub tools: Option<Vec<ToolSetDecl>>,
}

/// The whole configuration: the environment plus `memory`.
#[derive(Clone, Debug)]
pub struct AgentConfig {
    pub environment: EnvironmentConfig,
    pub memory: MemorySettings,
}

impl AgentConfig {
    /// Parses a YAML document. The `memory` key is taken out; everything else is the
    /// environment configuration, exactly as `EnvironmentConfig` reads it.
    pub fn from_yaml(yaml: &str) -> Result<Self, Error> {
        let mut document: serde_yaml::Value = serde_yaml::from_str(yaml)
            .map_err(|e| Error::parse_error(format!("Failed to parse the configuration: {e}")))?;
        let memory = match document.as_mapping_mut() {
            Some(map) => map.remove("memory"),
            None => None,
        };
        let memory: MemorySettings = match memory {
            Some(value) => serde_yaml::from_value(value).map_err(|e| {
                Error::parse_error(format!("Failed to parse the memory section: {e}"))
            })?,
            None => MemorySettings::default(),
        };
        let rest = serde_yaml::to_string(&document)
            .map_err(|e| Error::parse_error(format!("Failed to re-read the configuration: {e}")))?;
        let environment = if document.is_null() {
            EnvironmentConfig::default()
        } else {
            EnvironmentConfig::from_yaml(&rest)?
        };
        Ok(AgentConfig {
            environment,
            memory,
        })
    }

    /// Reads and parses a configuration file.
    pub fn load(path: &Path) -> Result<Self, Error> {
        let text = std::fs::read_to_string(path).map_err(|e| {
            Error::general_error(format!(
                "Cannot read the configuration {}: {e}",
                path.display()
            ))
        })?;
        Self::from_yaml(&text)
    }
}

/// The built-in tool set rooted at `root`.
pub fn memory_tool_set(root: &str) -> Result<ToolSetDecl, Error> {
    let mut set: ToolSetDecl = serde_yaml::from_str(MEMORY_TOOLS)
        .map_err(|e| Error::configuration_error(format!("Built-in tool set is invalid: {e}")))?;
    let root = root.trim_matches('/');
    set.cwd = if root.is_empty() {
        None
    } else {
        Some(root.to_string())
    };
    for resource in &mut set.resources {
        match resource {
            ResourceDecl::Text { uri, text, .. } => {
                if uri == QUERY_GUIDE_URI {
                    *text = QUERY_GUIDE.to_string();
                }
            }
            ResourceDecl::Tool { .. } | ResourceDecl::Commands { .. } => {}
        }
    }
    Ok(set)
}

/// Builds the environment: commands first, because building compiles the mounts' `metadata`
/// sections against the registry.
pub fn build_environment(config: &AgentConfig) -> Result<EnvRef<AgentEnvironment>, Error> {
    let mut builder = default_environment_builder::<Value, SimpleUIPayload>();
    {
        // Named by the code `register_command!` expands to.
        #[allow(unused_imports)]
        use liquers_core::context::Context;
        #[allow(dead_code)]
        type CommandEnvironment = AgentEnvironment;
        let cr = &mut builder.command_registry;
        liquers_lib::register_all_commands!(cr)?;
    }
    builder
        .with_config(
            config.environment.clone(),
            Box::new(liquers_store::store_factory::default_store_factory()),
        )
        .build()
}

/// The sentence pointing agents at the power layer.
pub const QUERY_POINTER: &str = "For anything the tools above cannot express, read \
liquers://memory/query-guide and use memory_query.";

/// Builds the server: the configured tool sets (or the built-in one), with the memory's
/// overview added to the instructions.
pub async fn build_server(config: &AgentConfig) -> Result<McpServer<AgentEnvironment>, Error> {
    build_server_with(config, true).await
}

/// As [`build_server`]; `overview: false` skips reading the overview into the instructions,
/// which only an MCP client reads (it lists every collection, so it reads every memory).
pub async fn build_server_with(
    config: &AgentConfig,
    overview: bool,
) -> Result<McpServer<AgentEnvironment>, Error> {
    let envref = build_environment(config)?;
    let (sets, builtin) = match &config.memory.tools {
        Some(sets) => (sets.clone(), false),
        None => (vec![memory_tool_set(&config.memory.root)?], true),
    };
    let registry = ToolRegistry::build(sets, envref.get_command_metadata_registry())?;
    let server = McpServer::new(envref, registry);
    if !builtin || !overview {
        return Ok(server);
    }
    let mut arguments = serde_json::Map::new();
    arguments.insert("log_lines".to_string(), serde_json::json!(0));
    let overview = match server.call("memory_overview", arguments).await {
        Ok(output) => output.text,
        Err(error) => format!("The memory overview is unavailable: {error}"),
    };
    Ok(server.with_instructions(&overview).with_instructions(QUERY_POINTER))
}
