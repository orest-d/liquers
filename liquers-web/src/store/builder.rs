//! Building a store router for a browser page from a configuration document.
//!
//! Reuses `liquers_core`'s configuration format and `StoreRouterBuilder` unchanged, contributing
//! the browser's store types through the `StoreFactory` seam.
//!
//! **Chain order decides which implementation of a contested type name wins: the first factory to
//! resolve an entry builds it.** `build_router` chains core's store types first and this crate's
//! after, so `memory` means the same thing here as everywhere else while `localstorage`, `js`,
//! `http` and `https` are this crate's.
//!
//! `http` is the interesting one: it is an OpenDAL service natively and a `fetch` store here.
//! There is no conflict to resolve, because `liquers-store` is not a dependency of this crate at
//! all — the OpenDAL factory that would claim `http` is never in the browser's chain. Adding it
//! would change that, and under first-wins whichever factory is chained earlier would win. See
//! `specs/design/store-factories-in-core/`.
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
use liquers_core::store_config::{StoreConfig, StoreRouterConfig};
use liquers_core::store_factory::{
    core_store_factory, ChainedStoreFactory, StoreArgumentInfo, StoreArgumentType, StoreFactory,
    StoreRouterBuilder, StoreTypeInfo,
};
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

    /// `http` and `https` differ only in the scheme their `url_prefix` carries.
    fn fetch_type_info(store_type: &str) -> StoreTypeInfo {
        StoreTypeInfo::new(store_type)
            .with_label("Read-only fetch()")
            .with_doc(
                "Serves a fixed list of keys over fetch(). Read-only, and it cannot enumerate a \
                 directory, which is why the keys are listed rather than discovered.",
            )
            .with_argument(
                StoreArgumentInfo::new("url_prefix", StoreArgumentType::String)
                    .required()
                    .with_doc("Base URL each key is appended to."),
            )
            .with_argument(
                StoreArgumentInfo::new("keys", StoreArgumentType::Array).with_doc(
                    "Keys this store serves, as a list of strings. Entries not already carrying \
                     the store's prefix are taken as relative to it.",
                ),
            )
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
    /// The browser's store types, described.
    ///
    /// `ArgumentCoverage` is left at its `Complete` default: Liquers owns these three types, so
    /// this list *is* their specification rather than guidance about someone else's surface.
    fn store_types(&self) -> Vec<StoreTypeInfo> {
        vec![
            StoreTypeInfo::new(LOCAL_STORAGE_TYPE)
                .with_label("Browser localStorage")
                .with_doc(
                    "Persists in the page's localStorage. Survives a reload, is bounded by the \
                     browser's per-origin quota, and is not shared between origins.",
                )
                .with_argument(
                    StoreArgumentInfo::new("namespace", StoreArgumentType::String)
                        .with_doc(
                            "Key prefix inside localStorage, so two applications on one origin do \
                             not collide. Defaults to `liquers`.",
                        )
                        .with_default(serde_json::Value::String("liquers".to_string())),
                )
                .with_argument(
                    StoreArgumentInfo::new("quota_bytes", StoreArgumentType::Number)
                        .with_doc("Refuse writes beyond this many bytes. Whole number."),
                ),
            StoreTypeInfo::new(JS_TYPE)
                .with_label("JavaScript object")
                .with_doc(
                    "Delegates to a page object registered with registerStoreObject. The object \
                     supplies the store methods; see the JavaScript store protocol.",
                )
                .with_argument(
                    StoreArgumentInfo::new("object", StoreArgumentType::String)
                        .required()
                        .with_doc(
                            "The registered name. A JavaScript object cannot be written into a \
                             configuration document, so the document carries a name and \
                             registerStoreObject maps it to the object.",
                        ),
                ),
            Self::fetch_type_info("http"),
            Self::fetch_type_info("https"),
        ]
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
    let chain = ChainedStoreFactory::new()
        .chain(Box::new(core_store_factory()))
        .chain(Box::new(factory));
    StoreRouterBuilder::new(config.clone(), Box::new(chain)).build_without_env_expansion()
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
