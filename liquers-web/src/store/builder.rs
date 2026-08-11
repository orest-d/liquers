//! Building a store router for a browser page from a configuration document.
//!
//! Reuses `liquers_store`'s configuration format and `StoreRouterBuilder` unchanged, contributing
//! the browser's store types through the `StoreFactory` seam. Factories are consulted **before**
//! the built-in types, which is what lets `http` mean OpenDAL natively and `fetch` here — the same
//! configuration document, a target-appropriate backend.
//!
//! ```yaml
//! stores:
//!   - type: localstorage
//!     prefix: local
//!     config: { namespace: myapp, quota_bytes: 4000000 }
//!   - type: http
//!     prefix: data
//!     config:
//!       url_prefix: https://example.org/reference/
//!       keys: [ input.csv, sub/report.json ]
//!   - type: js
//!     prefix: custom
//!     config: { object: myStore }   # a name passed to registerStoreObject
//! ```
//!
//! **No `${VAR}` expansion.** A browser has no environment, so
//! `build_without_env_expansion` is used and a config still containing `${…}` gets a
//! `console.warn` rather than a silently empty value. The syntax stays unclaimed so that
//! substitution from page-supplied variables can use it later.

use std::collections::HashMap;

use liquers_core::error::Error;
use liquers_core::parse::parse_key;
use liquers_core::query::Key;
use liquers_core::store::{AsyncStore, AsyncStoreRouter};
use liquers_store::config::{StoreConfig, StoreRouterConfig};
use liquers_store::store_builder::{StoreFactory, StoreRouterBuilder};
use wasm_bindgen::prelude::*;

use crate::store::fetch::FetchStore;
use crate::store::js_store::JsStore;
use crate::store::local_storage::LocalStorageStore;

/// Store types this crate contributes.
pub const LOCAL_STORAGE_TYPE: &str = "localstorage";
pub const JS_TYPE: &str = "js";
pub const HTTP_TYPES: [&str; 2] = ["http", "https"];

/// Creates the browser's store types from configuration.
///
/// Holds the page objects a `js` store entry can name. They are registered separately from the
/// configuration document because a JavaScript object cannot be written into YAML — the document
/// carries a *name*, and this maps names to objects.
#[derive(Default)]
pub struct WebStoreFactory {
    objects: HashMap<String, js_sys::Object>,
}

impl WebStoreFactory {
    pub fn new() -> Self {
        Self::default()
    }

    /// Names a page object so a `js` store entry can refer to it.
    pub fn register_object(&mut self, name: &str, object: js_sys::Object) {
        self.objects.insert(name.to_string(), object);
    }

    fn create_local_storage(config: &StoreConfig) -> Result<Box<dyn AsyncStore>, Error> {
        let prefix = config.key_prefix()?;
        let namespace = config
            .get_config_string("namespace")
            .unwrap_or_else(|| "liquers".to_string());
        let quota_bytes = match config.config.get("quota_bytes") {
            Some(value) => Some(value.as_u64().ok_or_else(|| {
                Error::general_error(format!(
                    "quota_bytes must be a whole number of bytes, got {value}"
                ))
            })?),
            None => None,
        };
        Ok(Box::new(LocalStorageStore::new(
            &prefix,
            &namespace,
            quota_bytes,
        )?))
    }

    fn create_fetch(config: &StoreConfig) -> Result<Box<dyn AsyncStore>, Error> {
        let prefix = config.key_prefix()?;
        let url_prefix = config.require_config_string("url_prefix")?;
        let keys = parse_key_list(config, &prefix)?;
        if keys.is_empty() {
            // Not an error: a crawling implementation will populate this later, and an empty
            // store is a usable — if useless — one. But it is almost certainly a mistake today.
            warn(&format!(
                "liquers: fetch store at prefix {prefix:?} has no `keys`, so it serves nothing"
            ));
        }
        Ok(Box::new(FetchStore::new(&prefix, &url_prefix, keys)?))
    }

    fn create_js(&self, config: &StoreConfig) -> Result<Box<dyn AsyncStore>, Error> {
        let prefix = config.key_prefix()?;
        let name = config.require_config_string("object")?;
        let object = self.objects.get(&name).ok_or_else(|| {
            Error::general_error(format!(
                "no store object named {name:?} has been registered; call \
                 registerStoreObject({name:?}, …) before configuring the store"
            ))
        })?;
        Ok(Box::new(JsStore::new(&prefix, &name, object.clone())?))
    }
}

/// Reads the `keys` list from a fetch store's configuration.
///
/// Entries are relative to the store's prefix when they do not already carry it, so a
/// configuration does not have to repeat the prefix on every line.
fn parse_key_list(config: &StoreConfig, prefix: &Key) -> Result<Vec<Key>, Error> {
    let Some(value) = config.config.get("keys") else {
        return Ok(Vec::new());
    };
    let array = value
        .as_array()
        .ok_or_else(|| Error::general_error("`keys` must be a list of key strings".to_string()))?;
    let mut keys = Vec::with_capacity(array.len());
    for entry in array {
        let text = entry.as_str().ok_or_else(|| {
            Error::general_error(format!("every `keys` entry must be a string, got {entry}"))
        })?;
        let key = parse_key(text)?;
        keys.push(if key.has_key_prefix(prefix) {
            key
        } else {
            let mut joined = prefix.to_owned();
            for segment in key.iter() {
                joined = joined.join(segment.encode());
            }
            joined
        });
    }
    Ok(keys)
}

impl StoreFactory for WebStoreFactory {
    fn store_types(&self) -> Vec<String> {
        let mut types = vec![LOCAL_STORAGE_TYPE.to_string(), JS_TYPE.to_string()];
        types.extend(HTTP_TYPES.iter().map(|t| t.to_string()));
        types
    }

    fn create(&self, config: &StoreConfig) -> Result<Box<dyn AsyncStore>, Error> {
        match config.store_type.as_str() {
            LOCAL_STORAGE_TYPE => Self::create_local_storage(config),
            JS_TYPE => self.create_js(config),
            "http" | "https" => Self::create_fetch(config),
            other => Err(Error::general_error(format!(
                "the browser store factory does not handle store type {other:?}"
            ))),
        }
    }
}

/// Builds a router for a page from a configuration document.
pub fn build_router(
    config: &StoreRouterConfig,
    factory: WebStoreFactory,
) -> Result<AsyncStoreRouter, Error> {
    warn_on_unexpanded_variables(config);
    StoreRouterBuilder::new(config.clone())
        .with_factory(Box::new(factory))
        .build_without_env_expansion()
}

/// A browser has no environment variables, so `${VAR}` is left verbatim. Saying so is better than
/// letting a configuration quietly point somewhere unintended.
fn warn_on_unexpanded_variables(config: &StoreRouterConfig) {
    for store in &config.stores {
        for (name, value) in &store.config {
            if value.as_str().map(|s| s.contains("${")).unwrap_or(false) {
                warn(&format!(
                    "liquers: store {:?} config {name:?} contains ${{…}}, which is not expanded \
                     in a browser and is used verbatim",
                    store.store_type
                ));
            }
        }
    }
}

fn warn(message: &str) {
    web_sys::console::warn_1(&JsValue::from_str(message));
}
