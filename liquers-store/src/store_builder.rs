//! Store builder module for creating stores from configuration.
//!
//! This module provides functionality to instantiate store backends from
//! configuration and compose them into an `AsyncStoreRouter`.

#[cfg(feature = "opendal")]
use std::collections::HashMap;

use liquers_core::error::Error;
use liquers_core::store::{AsyncMemoryStore, AsyncStore, AsyncStoreRouter};
#[cfg(not(target_arch = "wasm32"))]
use liquers_core::store::AsyncFileStore;
#[cfg(feature = "opendal")]
use opendal::Operator;

use crate::config::{is_opendal_store_type, StoreConfig, StoreRouterConfig};
#[cfg(feature = "opendal")]
use crate::config::get_opendal_scheme;
#[cfg(feature = "opendal")]
use crate::opendal_store::AsyncOpenDALStore;

/// Creates stores of types this crate does not know about.
///
/// A factory is consulted **before** the built-in types, so an integration may also *override* a
/// built-in one. That is not a theoretical nicety: `http` and `https` are built-in OpenDAL types,
/// and a browser integration must supply a `fetch`-based implementation instead, because OpenDAL
/// cannot run there. Consulting factories second would make that impossible.
///
/// Deliberately carries no `Send`/`Sync` bound. A factory is transient — it is consumed while the
/// router is built — and only the `AsyncStore` it produces has thread requirements, which
/// `AsyncStore` already states. A bound no call site needs would exclude the browser factory,
/// which holds JavaScript handles and is `!Send`.
///
/// See specs/design/liquers-web-store/phase2-architecture.md.
pub trait StoreFactory {
    /// Store type strings this factory claims, e.g. `["localstorage"]`.
    fn store_types(&self) -> Vec<String>;

    /// Create a store from a configuration entry whose type this factory claims.
    fn create(&self, config: &StoreConfig) -> Result<Box<dyn AsyncStore>, Error>;
}

/// Builder for creating an `AsyncStoreRouter` from configuration.
pub struct StoreRouterBuilder {
    config: StoreRouterConfig,
    factories: Vec<Box<dyn StoreFactory>>,
}

impl StoreRouterBuilder {
    /// Create a new builder from configuration.
    pub fn new(config: StoreRouterConfig) -> Self {
        Self {
            config,
            factories: Vec::new(),
        }
    }

    /// Register a factory for store types this crate does not implement.
    ///
    /// Factories are consulted before the built-in types, and in registration order, so a later
    /// factory cannot shadow an earlier one.
    pub fn with_factory(mut self, factory: Box<dyn StoreFactory>) -> Self {
        self.factories.push(factory);
        self
    }

    /// Create one store, giving registered factories first refusal.
    fn create_one(&self, store_config: &StoreConfig) -> Result<Box<dyn AsyncStore>, Error> {
        for factory in &self.factories {
            if factory
                .store_types()
                .iter()
                .any(|t| t == &store_config.store_type)
            {
                return factory.create(store_config);
            }
        }
        create_store(store_config)
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
            let store = self.create_one(store_config)?;
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
            let store = self.create_one(store_config)?;
            router.add_store(store);
        }

        Ok(router)
    }
}

/// Create a store from configuration, using only this crate's built-in store types.
///
/// This function dispatches to the appropriate store constructor based on the store type.
/// It does **not** consult [`StoreFactory`] registrations — use [`StoreRouterBuilder`] for that.
///
/// A store type that exists but is unavailable in this build says so, naming the feature or the
/// target responsible. Reporting it as "unknown" instead would send the reader looking for a
/// typo in a type that is documented and real.
pub fn create_store(config: &StoreConfig) -> Result<Box<dyn AsyncStore>, Error> {
    let store_type = config.store_type.as_str();

    match store_type {
        // Built-in memory store
        "memory" => create_memory_store(config),

        // Built-in filesystem store. `AsyncFileStore` uses `tokio::fs` and is itself excluded
        // from wasm32 (liquers-core/src/store.rs), so this arm follows it.
        #[cfg(not(target_arch = "wasm32"))]
        "filesystem" => create_filesystem_store(config),
        #[cfg(target_arch = "wasm32")]
        "filesystem" => Err(Error::general_error(
            "Store type 'filesystem' is not available on wasm32: it needs tokio::fs, which \
             wasm32-unknown-unknown does not provide"
                .to_string(),
        )),

        // OpenDAL-based stores
        #[cfg(feature = "opendal")]
        _ if is_opendal_store_type(store_type) => create_opendal_store(config),
        #[cfg(not(feature = "opendal"))]
        _ if is_opendal_store_type(store_type) => Err(Error::general_error(format!(
            "Store type '{}' requires the 'opendal' feature, which is not enabled in this build",
            store_type
        ))),

        // Unknown store type
        _ => Err(Error::general_error(format!(
            "Unknown store type: '{}'",
            store_type
        ))),
    }
}

/// Create a built-in memory store.
fn create_memory_store(config: &StoreConfig) -> Result<Box<dyn AsyncStore>, Error> {
    let prefix = config.key_prefix()?;
    let store = AsyncMemoryStore::new(&prefix);
    Ok(Box::new(store))
}

/// Create a built-in filesystem store.
#[cfg(not(target_arch = "wasm32"))]
fn create_filesystem_store(config: &StoreConfig) -> Result<Box<dyn AsyncStore>, Error> {
    let prefix = config.key_prefix()?;
    let path = config.require_config_string_expanded("path")?;
    let store = AsyncFileStore::new(&path, &prefix);
    Ok(Box::new(store))
}

/// Create an OpenDAL-based store.
#[cfg(feature = "opendal")]
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
#[cfg(feature = "opendal")]
fn create_opendal_operator(
    scheme: &str,
    config: HashMap<String, String>,
) -> Result<Operator, Error> {
    let config_pairs: Vec<(String, String)> = config.into_iter().collect();

    Operator::via_iter(scheme, config_pairs).map_err(|e| {
        Error::general_error(format!(
            "Failed to create OpenDAL operator for scheme '{}': {}",
            scheme, e
        ))
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
    use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
    use std::sync::Arc;

    #[test]
    fn test_create_memory_store() {
        let config = StoreConfig::new("memory").with_prefix("cache");
        let store = create_store(&config).unwrap();
        assert_eq!(store.store_name(), "cache Async memory store");
    }

    #[test]
    fn test_create_filesystem_store() {
        let config = StoreConfig::new("filesystem")
            .with_prefix("local")
            .with_config("path", "./test_data");
        let store = create_store(&config).unwrap();
        assert!(store.store_name().contains("Async file store"));
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

    /// A factory that counts how many times it was asked to build something.
    ///
    /// The counter is the assertion: "the router was built" does not distinguish a factory that
    /// ran from a built-in that ran instead, and that distinction is the whole point of
    /// `factory02`.
    struct CountingFactory {
        types: Vec<String>,
        calls: Arc<AtomicUsize>,
    }

    impl StoreFactory for CountingFactory {
        fn store_types(&self) -> Vec<String> {
            self.types.clone()
        }

        fn create(&self, config: &StoreConfig) -> Result<Box<dyn AsyncStore>, Error> {
            self.calls.fetch_add(1, AtomicOrdering::SeqCst);
            let prefix = config.key_prefix()?;
            Ok(Box::new(AsyncMemoryStore::new(&prefix)))
        }
    }

    fn counting_factory(types: &[&str]) -> (Box<dyn StoreFactory>, Arc<AtomicUsize>) {
        let calls = Arc::new(AtomicUsize::new(0));
        let factory = CountingFactory {
            types: types.iter().map(|t| t.to_string()).collect(),
            calls: calls.clone(),
        };
        (Box::new(factory), calls)
    }

    /// factory01 — a registered factory builds a type this crate does not know.
    #[test]
    fn factory01_custom_type_is_created() -> Result<(), Box<dyn std::error::Error>> {
        let (factory, calls) = counting_factory(&["testtype"]);
        let mut config = StoreRouterConfig::new();
        config.add_store(StoreConfig::new("testtype").with_prefix("custom"));

        let router = StoreRouterBuilder::new(config)
            .with_factory(factory)
            .build()?;

        assert_eq!(calls.load(AtomicOrdering::SeqCst), 1);
        assert!(router.is_supported(&liquers_core::parse::parse_key("custom/a.txt")?));
        Ok(())
    }

    /// factory02 — a factory claiming a built-in type wins.
    ///
    /// `http` is a built-in OpenDAL type. A browser integration must override it with a
    /// `fetch`-based store, because OpenDAL cannot run on wasm32. If factories were consulted
    /// *after* the built-ins, that override would be impossible and this crate would silently
    /// hand back a store that cannot exist on the target.
    #[test]
    fn factory02_factory_precedes_builtin() -> Result<(), Box<dyn std::error::Error>> {
        assert!(
            crate::config::is_opendal_store_type("http"),
            "precondition: `http` must be a built-in type for this test to mean anything"
        );

        let (factory, calls) = counting_factory(&["http"]);
        let mut config = StoreRouterConfig::new();
        config.add_store(StoreConfig::new("http").with_prefix("web"));

        let router = StoreRouterBuilder::new(config)
            .with_factory(factory)
            .build()?;

        assert_eq!(
            calls.load(AtomicOrdering::SeqCst),
            1,
            "the factory must be consulted before the built-in OpenDAL dispatch"
        );
        assert!(router.is_supported(&liquers_core::parse::parse_key("web/a.txt")?));
        Ok(())
    }

    /// factory03 — a type no factory claims still reaches the built-ins.
    #[test]
    fn factory03_unclaimed_type_falls_through() -> Result<(), Box<dyn std::error::Error>> {
        let (factory, calls) = counting_factory(&["testtype"]);
        let mut config = StoreRouterConfig::new();
        config.add_store(StoreConfig::new("memory").with_prefix("cache"));

        let router = StoreRouterBuilder::new(config)
            .with_factory(factory)
            .build()?;

        assert_eq!(calls.load(AtomicOrdering::SeqCst), 0);
        assert!(router.is_supported(&liquers_core::parse::parse_key("cache/a.txt")?));
        Ok(())
    }

    /// factory04 — a real type that this build cannot provide says which feature is missing.
    ///
    /// "Unknown store type" would send the reader hunting for a typo in a type that is real and
    /// documented. Only meaningful when the feature is actually off.
    #[cfg(not(feature = "opendal"))]
    #[test]
    fn factory04_gated_type_names_the_feature() {
        let config = StoreConfig::new("s3").with_prefix("remote");
        let message = match create_store(&config) {
            Ok(_) => panic!("`s3` must not be constructible without the opendal feature"),
            Err(e) => e.message,
        };
        assert!(
            message.contains("opendal"),
            "the error must name the missing feature, got: {message}"
        );
        assert!(
            !message.contains("Unknown store type"),
            "a gated-off type is not an unknown type, got: {message}"
        );
    }

    #[test]
    fn test_filesystem_missing_path() {
        let config = StoreConfig::new("filesystem").with_prefix("local");
        let result = create_store(&config);
        assert!(result.is_err());
    }
}
