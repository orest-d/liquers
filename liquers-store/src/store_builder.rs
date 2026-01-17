//! Store builder module for creating stores from configuration.
//!
//! This module provides functionality to instantiate store backends from
//! configuration and compose them into an `AsyncStoreRouter`.

use std::collections::HashMap;

use liquers_core::error::{Error, ErrorType};
use liquers_core::store::{AsyncStore, AsyncStoreRouter, AsyncStoreWrapper, FileStore, MemoryStore};
use opendal::Operator;

use crate::config::{get_opendal_scheme, is_opendal_store_type, StoreConfig, StoreRouterConfig};
use crate::opendal_store::AsyncOpenDALStore;

/// Builder for creating an `AsyncStoreRouter` from configuration.
pub struct StoreRouterBuilder {
    config: StoreRouterConfig,
}

impl StoreRouterBuilder {
    /// Create a new builder from configuration.
    pub fn new(config: StoreRouterConfig) -> Self {
        Self { config }
    }

    /// Create a builder from a YAML string.
    pub fn from_yaml(yaml: &str) -> Result<Self, Error> {
        let config = StoreRouterConfig::from_yaml(yaml)?;
        Ok(Self::new(config))
    }

    /// Create a builder from a JSON string.
    pub fn from_json(json: &str) -> Result<Self, Error> {
        let config = StoreRouterConfig::from_json(json)?;
        Ok(Self::new(config))
    }

    /// Build the `AsyncStoreRouter` from the configuration.
    ///
    /// This method:
    /// 1. Expands environment variables in all configuration values
    /// 2. Creates store instances for each configuration entry
    /// 3. Composes them into an `AsyncStoreRouter`
    pub fn build(mut self) -> Result<AsyncStoreRouter, Error> {
        // Expand environment variables
        self.config.expand_env_vars()?;

        let mut router = AsyncStoreRouter::new();

        for store_config in &self.config.stores {
            let store = create_store(store_config)?;
            router.add_store(store);
        }

        Ok(router)
    }

    /// Build the `AsyncStoreRouter` without expanding environment variables.
    ///
    /// Use this when environment variables have already been expanded
    /// or when you want to handle expansion manually.
    pub fn build_without_env_expansion(self) -> Result<AsyncStoreRouter, Error> {
        let mut router = AsyncStoreRouter::new();

        for store_config in &self.config.stores {
            let store = create_store(store_config)?;
            router.add_store(store);
        }

        Ok(router)
    }
}

/// Create a store from configuration.
///
/// This function dispatches to the appropriate store constructor based on the store type.
pub fn create_store(config: &StoreConfig) -> Result<Box<dyn AsyncStore>, Error> {
    let store_type = config.store_type.as_str();

    match store_type {
        // Built-in memory store
        "memory" => create_memory_store(config),

        // Built-in filesystem store
        "filesystem" => create_filesystem_store(config),

        // OpenDAL-based stores
        _ if is_opendal_store_type(store_type) => create_opendal_store(config),

        // Unknown store type
        _ => Err(Error::new(
            ErrorType::General,
            format!("Unknown store type: '{}'", store_type),
        )),
    }
}

/// Create a built-in memory store.
fn create_memory_store(config: &StoreConfig) -> Result<Box<dyn AsyncStore>, Error> {
    let prefix = config.key_prefix()?;
    let store = MemoryStore::new(&prefix);
    // Wrap sync store in async wrapper
    Ok(Box::new(AsyncStoreWrapper(store)))
}

/// Create a built-in filesystem store.
fn create_filesystem_store(config: &StoreConfig) -> Result<Box<dyn AsyncStore>, Error> {
    let prefix = config.key_prefix()?;
    let path = config.require_config_string_expanded("path")?;
    let store = FileStore::new(&path, &prefix);
    // Wrap sync store in async wrapper
    Ok(Box::new(AsyncStoreWrapper(store)))
}

/// Create an OpenDAL-based store.
fn create_opendal_store(config: &StoreConfig) -> Result<Box<dyn AsyncStore>, Error> {
    let prefix = config.key_prefix()?;
    let scheme = get_opendal_scheme(&config.store_type);

    // Convert config to string map for OpenDAL
    let config_map = config.config_as_string_map()?;

    // Create OpenDAL operator using via_iter for dynamic dispatch
    let operator = create_opendal_operator(scheme, config_map)?;

    let store = AsyncOpenDALStore::new(operator, prefix);
    Ok(Box::new(store))
}

/// Create an OpenDAL Operator from scheme and configuration.
fn create_opendal_operator(
    scheme: &str,
    config: HashMap<String, String>,
) -> Result<Operator, Error> {
    let config_pairs: Vec<(String, String)> = config.into_iter().collect();

    Operator::via_iter(scheme, config_pairs).map_err(|e| {
        Error::new(
            ErrorType::General,
            format!("Failed to create OpenDAL operator for scheme '{}': {}", scheme, e),
        )
    })
}

/// Convenience function to create an `AsyncStoreRouter` from a YAML configuration string.
///
/// # Example
/// ```ignore
/// use liquers_store::store_builder::create_router_from_yaml;
///
/// let yaml = r#"
/// stores:
///   - type: memory
///     prefix: cache
///   - type: filesystem
///     prefix: data
///     config:
///       path: ./data
/// "#;
///
/// let router = create_router_from_yaml(yaml)?;
/// ```
pub fn create_router_from_yaml(yaml: &str) -> Result<AsyncStoreRouter, Error> {
    StoreRouterBuilder::from_yaml(yaml)?.build()
}

/// Convenience function to create an `AsyncStoreRouter` from a JSON configuration string.
pub fn create_router_from_json(json: &str) -> Result<AsyncStoreRouter, Error> {
    StoreRouterBuilder::from_json(json)?.build()
}

#[cfg(test)]
mod tests {
    use super::*;
    use liquers_core::store::AsyncStore;

    #[test]
    fn test_create_memory_store() {
        let config = StoreConfig::new("memory").with_prefix("cache");
        let store = create_store(&config).unwrap();
        assert_eq!(store.store_name(), "cache Memory store");
    }

    #[test]
    fn test_create_filesystem_store() {
        let config = StoreConfig::new("filesystem")
            .with_prefix("local")
            .with_config("path", "./test_data");
        let store = create_store(&config).unwrap();
        assert!(store.store_name().contains("File store"));
    }

    #[tokio::test]
    async fn test_store_router_from_yaml() {
        let yaml = r#"
stores:
  - type: memory
    prefix: cache
  - type: memory
    prefix: temp
"#;
        let router = create_router_from_yaml(yaml).unwrap();

        // Test that router can find stores
        let key = liquers_core::parse::parse_key("cache/test.txt").unwrap();
        assert!(router.is_supported(&key));

        let key2 = liquers_core::parse::parse_key("temp/data.json").unwrap();
        assert!(router.is_supported(&key2));
    }

    #[tokio::test]
    async fn test_store_router_from_json() {
        let json = r#"{
            "stores": [
                {"type": "memory", "prefix": "mem1"},
                {"type": "memory", "prefix": "mem2"}
            ]
        }"#;
        let router = create_router_from_json(json).unwrap();

        let key = liquers_core::parse::parse_key("mem1/file.txt").unwrap();
        assert!(router.is_supported(&key));
    }

    #[test]
    fn test_unknown_store_type() {
        let config = StoreConfig::new("unknown_type");
        let result = create_store(&config);
        assert!(result.is_err());
    }

    #[test]
    fn test_filesystem_missing_path() {
        let config = StoreConfig::new("filesystem").with_prefix("local");
        let result = create_store(&config);
        assert!(result.is_err());
    }
}
