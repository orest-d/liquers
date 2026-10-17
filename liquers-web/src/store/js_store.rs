//! `JsStore` — a store implemented by the page, adapted to `AsyncStore`.
//!
//! This is the guide's literal reading of the `STORE` feature: a *language value* adapted to the
//! `AsyncStore` contract. The other three stores in this module are the converse — Rust stores
//! exposed *to* the language — and both readings are in scope
//! (`specs/design/liquers-web-store/`, Phase 1 Q1).
//!
//! # The protocol
//!
//! One vocabulary serves both directions: these are the methods this adapter calls on a page
//! object, and the methods [`crate::store::wrapper::LiquersStore`] exposes to a page. Keys cross as
//! strings, data as `Uint8Array`, metadata as a plain object holding `Metadata` JSON.
//!
//! | Method | Required | Absent ⇒ |
//! |---|---|---|
//! | `get(key)` → `{data, metadata}`, or `null`/`undefined` when absent | **yes** | construction fails |
//! | `getMetadata(key)` → object, or `null`/`undefined` when absent | no | derived from `get` |
//! | `set` / `setMetadata` / `remove` / `removedir` / `makedir` | no | `KeyNotSupported` |
//! | `contains(key)` / `isDir(key)` / `listdir(key)` | no | `KeyNotSupported` |
//! | `isSupported(key)` → bool (**sync**) | no | every key under the prefix is supported |
//! | `getData` / `setData` / `removeData` / `listdirData` / `dataFacts` | no, all five or none | no data-only access |
//! | `renameData(from, to)`; property `atomicWrite: true` | no | no rename; writes not atomic |
//! | `getStoredMetadata(key)` → object, or `null` when it stores none | no | `KeyNotSupported` |
//!
//! **Data-only access** (`specs/design/store-adapters/`) is what a `metadata` section in the
//! store's configuration builds on: the configured handlers and storage model run in Rust, around
//! this store, and reach it only through these methods. They never touch metadata. `dataFacts`
//! returns `{size, modified?, precision?, etag?}` or `null` for an absent object, with `modified`
//! an RFC 3339 string and `precision` one of `seconds`, `millis`, `micros`, `nanos` (default:
//! the precision the string is written in). `getStoredMetadata` is needed only by the `native`
//! storage model. An object without all five data methods reports no data-only access, and a
//! `metadata` section on its mount is refused when the store is built.
//!
//! **Absent optional methods produce an error rather than the trait's default.** `AsyncStore`
//! defaults `contains` to `false` and `listdir` to `[]` — permissive values that make a
//! half-written store look like an empty one, so `STORE03`'s listing invariants would pass
//! vacuously. Failing loudly is worth more than a default that lies. `isSupported` is the one
//! exception: it is synchronous, and a store answering "no" to everything is invisible to the
//! router, so its absence means *supported*.
//!
//! **Absence is `null` or `undefined`; failure is a thrown value.** Either read method returning
//! `null`/`undefined` becomes `KeyNotFound`, and anything thrown becomes `KeyReadError` — the
//! distinction STORE_SEMANTICS §4 makes load-bearing. So a `getMetadata` returning `null` for a key
//! that exists reads as absent; before 2026-09-29 it produced an empty record, and a delegate that
//! relied on that should return `{}`.
//!
//! **Directories.** A directory has no data, so the data path reports it absent. `get_metadata`
//! then asks `isDir`, and if it answers truthy returns directory metadata whose `children` come
//! from `listdir` (STORE_SEMANTICS §2). Only absence leads there: a thrown value is returned as the
//! failure it is.
//!
//! Methods are resolved **once, at construction**, so a missing one fails at registration with a
//! message naming it, rather than as a `TypeError` at first use — and so removing a method from
//! the object afterwards cannot change the store's behaviour.
//!
//! Every method may return a value or a Promise; thenables are awaited using the same rule as
//! JavaScript commands ([`crate::command::is_thenable`]).

use async_trait::async_trait;
use liquers_core::error::{Error, ErrorType};
use liquers_core::metadata::{BackendTime, Metadata, TimePrecision};
use liquers_core::query::Key;
use liquers_core::store::{AsyncStore, DataCapabilities, DataFacts};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

use crate::command::is_thenable;
use crate::error::js_error_to_liquers;
use crate::store::key_guard::check_key;

/// The protocol methods, resolved once at construction.
struct JsStoreMethods {
    get: js_sys::Function,
    get_metadata: Option<js_sys::Function>,
    set: Option<js_sys::Function>,
    set_metadata: Option<js_sys::Function>,
    remove: Option<js_sys::Function>,
    removedir: Option<js_sys::Function>,
    contains: Option<js_sys::Function>,
    is_dir: Option<js_sys::Function>,
    listdir: Option<js_sys::Function>,
    makedir: Option<js_sys::Function>,
    is_supported: Option<js_sys::Function>,
    get_data: Option<js_sys::Function>,
    set_data: Option<js_sys::Function>,
    remove_data: Option<js_sys::Function>,
    rename_data: Option<js_sys::Function>,
    listdir_data: Option<js_sys::Function>,
    data_facts: Option<js_sys::Function>,
    get_stored_metadata: Option<js_sys::Function>,
    /// The object's `atomicWrite` property, read once with the methods.
    atomic_write: bool,
}

/// Adapts a JavaScript object implementing the store protocol to [`AsyncStore`].
pub struct JsStore {
    prefix: Key,
    name: String,
    /// The page's object. `!Send`/`!Sync`, which is one reason this crate is wasm32-only.
    object: js_sys::Object,
    methods: JsStoreMethods,
}

/// Reads a function-valued property, or `None` when it is absent or not callable.
fn method(object: &js_sys::Object, name: &str) -> Option<js_sys::Function> {
    js_sys::Reflect::get(object, &JsValue::from_str(name))
        .ok()
        .and_then(|value| value.dyn_into::<js_sys::Function>().ok())
}

impl JsStore {
    /// Adapts `object`, failing immediately if the mandatory part of the protocol is missing.
    pub fn new(prefix: &Key, name: &str, object: js_sys::Object) -> Result<Self, Error> {
        let get = method(&object, "get").ok_or_else(|| {
            Error::general_error(format!(
                "store {name:?} must provide a `get(key)` method; it is the one method with no \
                 fallback"
            ))
        })?;

        let methods = JsStoreMethods {
            get,
            get_metadata: method(&object, "getMetadata"),
            set: method(&object, "set"),
            set_metadata: method(&object, "setMetadata"),
            remove: method(&object, "remove"),
            removedir: method(&object, "removedir"),
            contains: method(&object, "contains"),
            is_dir: method(&object, "isDir"),
            listdir: method(&object, "listdir"),
            makedir: method(&object, "makedir"),
            is_supported: method(&object, "isSupported"),
            get_data: method(&object, "getData"),
            set_data: method(&object, "setData"),
            remove_data: method(&object, "removeData"),
            rename_data: method(&object, "renameData"),
            listdir_data: method(&object, "listdirData"),
            data_facts: method(&object, "dataFacts"),
            get_stored_metadata: method(&object, "getStoredMetadata"),
            atomic_write: js_sys::Reflect::get(&object, &JsValue::from_str("atomicWrite"))
                .map(|v| v.as_bool() == Some(true))
                .unwrap_or(false),
        };

        Ok(Self {
            prefix: prefix.to_owned(),
            name: name.to_string(),
            object,
            methods,
        })
    }

    fn missing(&self, key: &Key, method_name: &str) -> Error {
        Error::key_not_supported(
            key,
            &format!("{} (no `{}` method)", self.store_name(), method_name),
        )
    }

    /// Calls a protocol method, awaiting the result when it is thenable.
    async fn call(
        &self,
        function: &js_sys::Function,
        args: &js_sys::Array,
    ) -> Result<JsValue, Error> {
        let result = function
            .apply(&self.object, args)
            .map_err(|e| js_error_to_liquers(e, ErrorType::KeyReadError))?;
        if is_thenable(&result) {
            let promise: js_sys::Promise = result.unchecked_into();
            JsFuture::from(promise)
                .await
                .map_err(|e| js_error_to_liquers(e, ErrorType::KeyReadError))
        } else {
            Ok(result)
        }
    }

    fn args1(&self, key: &Key) -> js_sys::Array {
        let args = js_sys::Array::new();
        args.push(&JsValue::from_str(&key.encode()));
        args
    }

    fn metadata_from_js(&self, key: &Key, value: &JsValue) -> Result<Metadata, Error> {
        crate::store::metadata_from_js_value(key, value, &self.store_name())
    }

    fn metadata_to_js(&self, key: &Key, metadata: &Metadata) -> Result<JsValue, Error> {
        crate::store::metadata_to_js_value(key, metadata, &self.store_name())
    }

    fn bytes_from_js(&self, key: &Key, value: &JsValue) -> Result<Vec<u8>, Error> {
        if let Some(array) = value.dyn_ref::<js_sys::Uint8Array>() {
            return Ok(array.to_vec());
        }
        if value.is_instance_of::<js_sys::ArrayBuffer>() || js_sys::Array::is_array(value) {
            return Ok(js_sys::Uint8Array::new(value).to_vec());
        }
        Err(Error::key_read_error(
            key,
            &self.store_name(),
            &"`get` must return `data` as a Uint8Array, ArrayBuffer or array of byte values",
        ))
    }

    fn truthy(value: &JsValue) -> bool {
        value.as_bool().unwrap_or_else(|| value.is_truthy())
    }

    /// True when the object offers the five methods data-only access needs.
    fn has_data_access(&self) -> bool {
        self.methods.get_data.is_some()
            && self.methods.set_data.is_some()
            && self.methods.remove_data.is_some()
            && self.methods.listdir_data.is_some()
            && self.methods.data_facts.is_some()
    }

    /// A data-only method, or the refusal of a store without data-only access.
    fn data_method<'a>(
        &self,
        function: &'a Option<js_sys::Function>,
        key: &Key,
        name: &str,
    ) -> Result<&'a js_sys::Function, Error> {
        match function {
            Some(function) => Ok(function),
            None => Err(Error::not_supported(format!(
                "Store '{}' does not offer data-only access: its object has no `{name}` method \
                 (key '{key}')",
                self.store_name()
            ))),
        }
    }

    /// `dataFacts`' answer as [`DataFacts`].
    fn facts_from_js(&self, key: &Key, value: &JsValue) -> Result<DataFacts, Error> {
        let field = |name: &str| js_sys::Reflect::get(value, &JsValue::from_str(name)).ok();
        let size = field("size").and_then(|v| v.as_f64()).ok_or_else(|| {
            Error::key_read_error(
                key,
                &self.store_name(),
                &"`dataFacts` must return an object with a numeric `size`",
            )
        })?;
        let precision = match field("precision").and_then(|v| v.as_string()) {
            Some(name) => Some(TimePrecision::from_name(&name).ok_or_else(|| {
                Error::key_read_error(
                    key,
                    &self.store_name(),
                    &format!("`dataFacts` precision '{name}' is not seconds, millis, micros or nanos"),
                )
            })?),
            None => None,
        };
        let modified = match field("modified").and_then(|v| v.as_string()) {
            Some(text) => Some(BackendTime::parse(&text, precision)?),
            None => None,
        };
        Ok(DataFacts {
            size: size as u64,
            modified,
            etag: field("etag").and_then(|v| v.as_string()),
        })
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl AsyncStore for JsStore {
    fn store_name(&self) -> String {
        format!("{} JavaScript store", self.name)
    }

    fn key_prefix(&self) -> Key {
        self.prefix.to_owned()
    }

    async fn get(&self, key: &Key) -> Result<(Vec<u8>, Metadata), Error> {
        check_key(key, &self.store_name())?;
        let result = self.call(&self.methods.get, &self.args1(key)).await?;
        if result.is_undefined() || result.is_null() {
            return Err(Error::key_not_found(key));
        }
        let data = js_sys::Reflect::get(&result, &JsValue::from_str("data"))
            .map_err(|e| js_error_to_liquers(e, ErrorType::KeyReadError))?;
        let metadata = js_sys::Reflect::get(&result, &JsValue::from_str("metadata"))
            .unwrap_or(JsValue::UNDEFINED);
        Ok((
            self.bytes_from_js(key, &data)?,
            self.metadata_from_js(key, &metadata)?,
        ))
    }

    /// The data path first, then — only when it says the key is absent — the directory branch.
    ///
    /// Data first because `isDir` is optional in the protocol, and because a file read then costs
    /// one call rather than two. A thrown value is a failure (`KeyReadError`) and is returned as
    /// is: a failure is never reinterpreted as a directory.
    async fn get_metadata(&self, key: &Key) -> Result<Metadata, Error> {
        check_key(key, &self.store_name())?;
        let data_path = match &self.methods.get_metadata {
            Some(function) => {
                let value = self.call(function, &self.args1(key)).await?;
                // `null`/`undefined` means absent, as it does for `get`. `metadata_from_js` would
                // turn it into an empty record, which is right for the write direction — where it
                // means "no metadata supplied" — and wrong here.
                if value.is_null() || value.is_undefined() {
                    Err(Error::key_not_found(key))
                } else {
                    self.metadata_from_js(key, &value)
                }
            }
            // Deriving it from `get` costs a body read, but a store that can serve data can always
            // answer this, so refusing would be worse than being slow.
            None => self.get(key).await.map(|(_, metadata)| metadata),
        };
        match data_path {
            Err(e) if e.error_type == ErrorType::KeyNotFound => {
                // A directory has no data, so the data path reports it absent (STORE_SEMANTICS §2).
                let is_dir = match &self.methods.is_dir {
                    Some(function) => Self::truthy(&self.call(function, &self.args1(key)).await?),
                    None => false,
                };
                if is_dir {
                    let mut record = self.default_metadata(key, true);
                    record.children = self.listdir_asset_info(key).await?;
                    Ok(Metadata::MetadataRecord(record))
                } else {
                    Err(e)
                }
            }
            other => other,
        }
    }

    async fn set(&self, key: &Key, data: &[u8], metadata: &Metadata) -> Result<(), Error> {
        check_key(key, &self.store_name())?;
        let function = self
            .methods
            .set
            .as_ref()
            .ok_or_else(|| self.missing(key, "set"))?;
        let args = js_sys::Array::new();
        args.push(&JsValue::from_str(&key.encode()));
        args.push(&js_sys::Uint8Array::from(data).into());
        let mut metadata = metadata.clone();
        liquers_core::store::seed_filename_from_key(&mut metadata, key);
        args.push(&self.metadata_to_js(key, &metadata)?);
        self.call(function, &args).await.map(|_| ())
    }

    async fn set_metadata(&self, key: &Key, metadata: &Metadata) -> Result<(), Error> {
        check_key(key, &self.store_name())?;
        let function = self
            .methods
            .set_metadata
            .as_ref()
            .ok_or_else(|| self.missing(key, "setMetadata"))?;
        let args = js_sys::Array::new();
        args.push(&JsValue::from_str(&key.encode()));
        args.push(&self.metadata_to_js(key, metadata)?);
        self.call(function, &args).await.map(|_| ())
    }

    async fn remove(&self, key: &Key) -> Result<(), Error> {
        check_key(key, &self.store_name())?;
        let function = self
            .methods
            .remove
            .as_ref()
            .ok_or_else(|| self.missing(key, "remove"))?;
        self.call(function, &self.args1(key)).await.map(|_| ())
    }

    async fn removedir(&self, key: &Key) -> Result<(), Error> {
        check_key(key, &self.store_name())?;
        let function = self
            .methods
            .removedir
            .as_ref()
            .ok_or_else(|| self.missing(key, "removedir"))?;
        self.call(function, &self.args1(key)).await.map(|_| ())
    }

    async fn makedir(&self, key: &Key) -> Result<(), Error> {
        check_key(key, &self.store_name())?;
        let function = self
            .methods
            .makedir
            .as_ref()
            .ok_or_else(|| self.missing(key, "makedir"))?;
        self.call(function, &self.args1(key)).await.map(|_| ())
    }

    async fn contains(&self, key: &Key) -> Result<bool, Error> {
        check_key(key, &self.store_name())?;
        let function = self
            .methods
            .contains
            .as_ref()
            .ok_or_else(|| self.missing(key, "contains"))?;
        let value = self.call(function, &self.args1(key)).await?;
        Ok(Self::truthy(&value))
    }

    async fn is_dir(&self, key: &Key) -> Result<bool, Error> {
        check_key(key, &self.store_name())?;
        let function = self
            .methods
            .is_dir
            .as_ref()
            .ok_or_else(|| self.missing(key, "isDir"))?;
        let value = self.call(function, &self.args1(key)).await?;
        Ok(Self::truthy(&value))
    }

    async fn listdir(&self, key: &Key) -> Result<Vec<String>, Error> {
        check_key(key, &self.store_name())?;
        let function = self
            .methods
            .listdir
            .as_ref()
            .ok_or_else(|| self.missing(key, "listdir"))?;
        let value = self.call(function, &self.args1(key)).await?;
        if !js_sys::Array::is_array(&value) {
            return Err(Error::key_read_error(
                key,
                &self.store_name(),
                &"`listdir` must return an array of names",
            ));
        }
        Ok(js_sys::Array::from(&value)
            .iter()
            .filter_map(|name| name.as_string())
            .collect())
    }

    async fn get_data(&self, key: &Key) -> Result<Vec<u8>, Error> {
        check_key(key, &self.store_name())?;
        let function = self.data_method(&self.methods.get_data, key, "getData")?;
        let value = self.call(function, &self.args1(key)).await?;
        if value.is_null() || value.is_undefined() {
            return Err(Error::key_not_found(key));
        }
        self.bytes_from_js(key, &value)
    }

    async fn set_data(&self, key: &Key, data: &[u8]) -> Result<(), Error> {
        check_key(key, &self.store_name())?;
        let function = self.data_method(&self.methods.set_data, key, "setData")?;
        let args = self.args1(key);
        args.push(&js_sys::Uint8Array::from(data).into());
        self.call(function, &args).await.map(|_| ())
    }

    async fn remove_data(&self, key: &Key) -> Result<(), Error> {
        check_key(key, &self.store_name())?;
        let function = self.data_method(&self.methods.remove_data, key, "removeData")?;
        self.call(function, &self.args1(key)).await.map(|_| ())
    }

    async fn rename_data(&self, from: &Key, to: &Key) -> Result<(), Error> {
        check_key(from, &self.store_name())?;
        check_key(to, &self.store_name())?;
        let function = self.data_method(&self.methods.rename_data, from, "renameData")?;
        let args = self.args1(from);
        args.push(&JsValue::from_str(&to.encode()));
        self.call(function, &args).await.map(|_| ())
    }

    async fn listdir_data(&self, key: &Key) -> Result<Vec<String>, Error> {
        check_key(key, &self.store_name())?;
        let function = self.data_method(&self.methods.listdir_data, key, "listdirData")?;
        let value = self.call(function, &self.args1(key)).await?;
        if value.is_null() || value.is_undefined() {
            return Ok(Vec::new());
        }
        if !js_sys::Array::is_array(&value) {
            return Err(Error::key_read_error(
                key,
                &self.store_name(),
                &"`listdirData` must return an array of names",
            ));
        }
        Ok(js_sys::Array::from(&value)
            .iter()
            .filter_map(|name| name.as_string())
            .collect())
    }

    async fn data_facts(&self, key: &Key) -> Result<Option<DataFacts>, Error> {
        check_key(key, &self.store_name())?;
        let function = self.data_method(&self.methods.data_facts, key, "dataFacts")?;
        let value = self.call(function, &self.args1(key)).await?;
        if value.is_null() || value.is_undefined() {
            return Ok(None);
        }
        self.facts_from_js(key, &value).map(Some)
    }

    async fn get_stored_metadata(&self, key: &Key) -> Result<Option<Metadata>, Error> {
        check_key(key, &self.store_name())?;
        let function = self
            .methods
            .get_stored_metadata
            .as_ref()
            .ok_or_else(|| self.missing(key, "getStoredMetadata"))?;
        let value = self.call(function, &self.args1(key)).await?;
        if value.is_null() || value.is_undefined() {
            return Ok(None);
        }
        self.metadata_from_js(key, &value).map(Some)
    }

    /// From the methods found at construction: data access needs all five data methods.
    fn data_capabilities(&self) -> DataCapabilities {
        if !self.has_data_access() {
            return DataCapabilities::default();
        }
        DataCapabilities {
            data_access: true,
            atomic_write: self.methods.atomic_write,
            rename: self.methods.rename_data.is_some(),
            modified_time: false,
            etag: false,
        }
    }

    /// Must be overridden: the trait default is `false`, and `AsyncStoreRouter` consults it.
    ///
    /// Synchronous, so a page's `isSupported` must be synchronous too; a Promise here is truthy
    /// and would claim every key. The absence of the method means "supported", which is the safe
    /// default in the other direction — a store nobody can route to is invisible.
    fn is_supported(&self, key: &Key) -> bool {
        if !key.has_key_prefix(&self.prefix) || check_key(key, &self.name).is_err() {
            return false;
        }
        match &self.methods.is_supported {
            Some(function) => function
                .call1(&self.object, &JsValue::from_str(&key.encode()))
                .map(|v| Self::truthy(&v))
                .unwrap_or(false),
            None => true,
        }
    }
}
