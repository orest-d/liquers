//! Store configuration module for liquers-store.
//!
//! This module provides configuration structures and utilities for creating
//! an `AsyncStoreRouter` from declarative configuration (YAML, TOML, or JSON).

use std::collections::HashMap;

use liquers_core::error::{Error, ErrorType};
use liquers_core::parse::parse_key;
use liquers_core::query::Key;
use serde::{Deserialize, Serialize};

/// Configuration for the entire store router.
///
/// This is the top-level configuration structure that contains a list of store definitions.
/// Stores are evaluated in order - the first store whose prefix matches will handle the request.
///
/// # Example (YAML)
/// ```yaml
/// stores:
///   - type: memory
///     prefix: cache
///   - type: filesystem
///     prefix: data
///     config:
///       path: ./data
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StoreRouterConfig {
    /// List of store configurations. Order matters for routing.
    #[serde(default)]
    pub stores: Vec<StoreConfig>,
}

/// Configuration for a single store.
///
/// Each store has a type that determines its implementation, an optional prefix
/// for routing, and optional backend-specific configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoreConfig {
    /// Store type identifier (e.g., "memory", "filesystem", "s3", "fs", "ftp", etc.)
    #[serde(rename = "type")]
    pub store_type: String,

    /// Key prefix for routing. Empty string matches all keys.
    #[serde(default)]
    pub prefix: String,

    /// Backend-specific configuration as key-value pairs.
    /// Not required for some store types (e.g., memory).
    #[serde(default)]
    pub config: HashMap<String, serde_json::Value>,

    /// Reserved for future use - metadata storage configuration.
    #[serde(default)]
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

impl StoreRouterConfig {
    /// Create an empty configuration.
    pub fn new() -> Self {
        Self { stores: Vec::new() }
    }

    /// Add a store configuration.
    pub fn add_store(&mut self, store: StoreConfig) {
        self.stores.push(store);
    }

    /// Load configuration from a YAML string.
    pub fn from_yaml(yaml: &str) -> Result<Self, Error> {
        serde_yaml::from_str(yaml).map_err(|e| {
            Error::new(
                ErrorType::ParseError,
                format!("Failed to parse YAML configuration: {}", e),
            )
        })
    }

    /// Load configuration from a JSON string.
    pub fn from_json(json: &str) -> Result<Self, Error> {
        serde_json::from_str(json).map_err(|e| {
            Error::new(
                ErrorType::ParseError,
                format!("Failed to parse JSON configuration: {}", e),
            )
        })
    }

    /// Load configuration from a TOML string.
    #[cfg(feature = "toml")]
    pub fn from_toml(toml: &str) -> Result<Self, Error> {
        toml::from_str(toml).map_err(|e| {
            Error::new(
                ErrorType::ParseError,
                format!("Failed to parse TOML configuration: {}", e),
            )
        })
    }

    /// Serialize configuration to YAML string.
    pub fn to_yaml(&self) -> Result<String, Error> {
        serde_yaml::to_string(self).map_err(|e| {
            Error::new(
                ErrorType::General,
                format!("Failed to serialize configuration to YAML: {}", e),
            )
        })
    }

    /// Serialize configuration to JSON string.
    pub fn to_json(&self) -> Result<String, Error> {
        serde_json::to_string_pretty(self).map_err(|e| {
            Error::new(
                ErrorType::General,
                format!("Failed to serialize configuration to JSON: {}", e),
            )
        })
    }

    /// Expand environment variables in all configuration values.
    ///
    /// Supports `${VAR_NAME}` syntax for environment variable substitution.
    pub fn expand_env_vars(&mut self) -> Result<(), Error> {
        for store in &mut self.stores {
            store.expand_env_vars()?;
        }
        Ok(())
    }
}

impl StoreConfig {
    /// Create a new store configuration.
    pub fn new(store_type: &str) -> Self {
        Self {
            store_type: store_type.to_string(),
            prefix: String::new(),
            config: HashMap::new(),
            metadata: None,
        }
    }

    /// Set the prefix for this store.
    pub fn with_prefix(mut self, prefix: &str) -> Self {
        self.prefix = prefix.to_string();
        self
    }

    /// Add a configuration option.
    pub fn with_config(mut self, key: &str, value: impl Into<serde_json::Value>) -> Self {
        self.config.insert(key.to_string(), value.into());
        self
    }

    /// Parse the prefix as a Key.
    pub fn key_prefix(&self) -> Result<Key, Error> {
        if self.prefix.is_empty() {
            Ok(Key::new())
        } else {
            parse_key(&self.prefix)
        }
    }

    /// Get a config value as a string.
    pub fn get_config_string(&self, key: &str) -> Option<String> {
        self.config.get(key).and_then(|v| {
            match v {
                serde_json::Value::String(s) => Some(s.clone()),
                _ => v.as_str().map(|s| s.to_string()),
            }
        })
    }

    /// Get a config value as a string, with environment variable expansion.
    pub fn get_config_string_expanded(&self, key: &str) -> Option<Result<String, Error>> {
        self.get_config_string(key).map(|s| expand_env_vars(&s))
    }

    /// Get a required config value as a string.
    pub fn require_config_string(&self, key: &str) -> Result<String, Error> {
        self.get_config_string(key).ok_or_else(|| {
            Error::new(
                ErrorType::General,
                format!(
                    "Missing required configuration '{}' for store type '{}'",
                    key, self.store_type
                ),
            )
        })
    }

    /// Get a required config value as a string, with environment variable expansion.
    pub fn require_config_string_expanded(&self, key: &str) -> Result<String, Error> {
        let value = self.require_config_string(key)?;
        expand_env_vars(&value)
    }

    /// Convert config to a HashMap<String, String> for OpenDAL.
    /// Expands environment variables in all values.
    pub fn config_as_string_map(&self) -> Result<HashMap<String, String>, Error> {
        let mut result = HashMap::new();
        for (key, value) in &self.config {
            let string_value = match value {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Bool(b) => b.to_string(),
                serde_json::Value::Number(n) => n.to_string(),
                _ => value.to_string(),
            };
            result.insert(key.clone(), expand_env_vars(&string_value)?);
        }
        Ok(result)
    }

    /// Expand environment variables in all configuration values.
    pub fn expand_env_vars(&mut self) -> Result<(), Error> {
        let mut expanded_config = HashMap::new();
        for (key, value) in &self.config {
            let expanded_value = match value {
                serde_json::Value::String(s) => {
                    serde_json::Value::String(expand_env_vars(s)?)
                }
                other => other.clone(),
            };
            expanded_config.insert(key.clone(), expanded_value);
        }
        self.config = expanded_config;
        Ok(())
    }
}

/// Expand environment variables in a string.
///
/// Supports `${VAR_NAME}` syntax. If the environment variable is not set,
/// returns an error.
///
/// # Example
/// ```
/// use liquers_store::config::expand_env_vars;
/// std::env::set_var("MY_VAR", "hello");
/// assert_eq!(expand_env_vars("prefix_${MY_VAR}_suffix").unwrap(), "prefix_hello_suffix");
/// ```
pub fn expand_env_vars(input: &str) -> Result<String, Error> {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '$' && chars.peek() == Some(&'{') {
            chars.next(); // consume '{'
            let mut var_name = String::new();
            loop {
                match chars.next() {
                    Some('}') => break,
                    Some(ch) => var_name.push(ch),
                    None => {
                        return Err(Error::new(
                            ErrorType::ParseError,
                            format!("Unclosed environment variable reference in: {}", input),
                        ))
                    }
                }
            }
            let value = std::env::var(&var_name).map_err(|_| {
                Error::new(
                    ErrorType::General,
                    format!("Environment variable '{}' is not set", var_name),
                )
            })?;
            result.push_str(&value);
        } else {
            result.push(c);
        }
    }

    Ok(result)
}

/// Known store types that map to OpenDAL backends.
/// These are the type strings that will be handled via OpenDAL's via_iter.
pub const OPENDAL_STORE_TYPES: &[&str] = &[
    "fs",
    "s3",
    "ftp",
    "gcs",
    "azblob",
    "sftp",
    "webdav",
    "github",
    "hdfs",
    "webhdfs",
    "http",
    "https",
    "redis",
    "mongodb",
    "postgresql",
    "mysql",
    "sqlite",
    "dropbox",
    "onedrive",
    "gdrive",
    "ipfs",
];

/// Check if a store type should be handled by OpenDAL.
pub fn is_opendal_store_type(store_type: &str) -> bool {
    OPENDAL_STORE_TYPES.contains(&store_type) || store_type.starts_with("opendal_")
}

/// Get the OpenDAL scheme from a store type.
///
/// If the type starts with "opendal_", strips that prefix.
/// Otherwise returns the type as-is.
pub fn get_opendal_scheme(store_type: &str) -> &str {
    store_type.strip_prefix("opendal_").unwrap_or(store_type)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_env_vars_simple() {
        std::env::set_var("TEST_VAR_1", "hello");
        assert_eq!(expand_env_vars("${TEST_VAR_1}").unwrap(), "hello");
        assert_eq!(
            expand_env_vars("prefix_${TEST_VAR_1}_suffix").unwrap(),
            "prefix_hello_suffix"
        );
    }

    #[test]
    fn test_expand_env_vars_multiple() {
        std::env::set_var("TEST_VAR_A", "aaa");
        std::env::set_var("TEST_VAR_B", "bbb");
        assert_eq!(
            expand_env_vars("${TEST_VAR_A}/${TEST_VAR_B}").unwrap(),
            "aaa/bbb"
        );
    }

    #[test]
    fn test_expand_env_vars_no_vars() {
        assert_eq!(expand_env_vars("no variables here").unwrap(), "no variables here");
    }

    #[test]
    fn test_expand_env_vars_missing() {
        let result = expand_env_vars("${NONEXISTENT_VAR_12345}");
        assert!(result.is_err());
    }

    #[test]
    fn test_expand_env_vars_unclosed() {
        let result = expand_env_vars("${UNCLOSED");
        assert!(result.is_err());
    }

    #[test]
    fn test_store_router_config_yaml() {
        let yaml = r#"
stores:
  - type: memory
    prefix: cache
  - type: filesystem
    prefix: data
    config:
      path: ./data
"#;
        let config = StoreRouterConfig::from_yaml(yaml).unwrap();
        assert_eq!(config.stores.len(), 2);
        assert_eq!(config.stores[0].store_type, "memory");
        assert_eq!(config.stores[0].prefix, "cache");
        assert_eq!(config.stores[1].store_type, "filesystem");
        assert_eq!(config.stores[1].prefix, "data");
        assert_eq!(
            config.stores[1].get_config_string("path"),
            Some("./data".to_string())
        );
    }

    #[test]
    fn test_store_router_config_json() {
        let json = r#"{
            "stores": [
                {"type": "memory", "prefix": "temp"},
                {"type": "fs", "prefix": "files", "config": {"root": "/data"}}
            ]
        }"#;
        let config = StoreRouterConfig::from_json(json).unwrap();
        assert_eq!(config.stores.len(), 2);
        assert_eq!(config.stores[0].store_type, "memory");
        assert_eq!(config.stores[1].store_type, "fs");
    }

    #[test]
    fn test_store_config_builder() {
        let config = StoreConfig::new("s3")
            .with_prefix("remote")
            .with_config("bucket", "my-bucket")
            .with_config("region", "us-east-1");

        assert_eq!(config.store_type, "s3");
        assert_eq!(config.prefix, "remote");
        assert_eq!(config.get_config_string("bucket"), Some("my-bucket".to_string()));
        assert_eq!(config.get_config_string("region"), Some("us-east-1".to_string()));
    }

    #[test]
    fn test_key_prefix_parsing() {
        let config = StoreConfig::new("memory").with_prefix("data/cache");
        let key = config.key_prefix().unwrap();
        assert_eq!(key.encode(), "data/cache");
    }

    #[test]
    fn test_key_prefix_empty() {
        let config = StoreConfig::new("memory");
        let key = config.key_prefix().unwrap();
        assert!(key.is_empty());
    }

    #[test]
    fn test_is_opendal_store_type() {
        assert!(is_opendal_store_type("s3"));
        assert!(is_opendal_store_type("fs"));
        assert!(is_opendal_store_type("opendal_custom"));
        assert!(!is_opendal_store_type("memory"));
        assert!(!is_opendal_store_type("filesystem"));
    }

    #[test]
    fn test_get_opendal_scheme() {
        assert_eq!(get_opendal_scheme("s3"), "s3");
        assert_eq!(get_opendal_scheme("opendal_fs"), "fs");
        assert_eq!(get_opendal_scheme("opendal_custom"), "custom");
    }
}
