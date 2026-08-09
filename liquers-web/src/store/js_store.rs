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
//! | `get(key)` → `{data, metadata}` | **yes** | construction fails |
//! | `getMetadata(key)` → object | no | derived from `get` |
//! | `set` / `setMetadata` / `remove` / `removedir` / `makedir` | no | `KeyNotSupported` |
//! | `contains(key)` / `isDir(key)` / `listdir(key)` | no | `KeyNotSupported` |
//! | `isSupported(key)` → bool (**sync**) | no | every key under the prefix is supported |
//!
//! **Absent optional methods produce an error rather than the trait's default.** `AsyncStore`
//! defaults `contains` to `false` and `listdir` to `[]` — permissive values that make a
//! half-written store look like an empty one, so `STORE03`'s listing invariants would pass
//! vacuously. Failing loudly is worth more than a default that lies. `isSupported` is the one
//! exception: it is synchronous, and a store answering "no" to everything is invisible to the
//! router, so its absence means *supported*.
//!
//! Methods are resolved **once, at construction**, so a missing one fails at registration with a
//! message naming it, rather than as a `TypeError` at first use — and so removing a method from
//! the object afterwards cannot change the store's behaviour.
//!
//! Every method may return a value or a Promise; thenables are awaited using the same rule as
//! JavaScript commands ([`crate::command::is_thenable`]).

use async_trait::async_trait;
use liquers_core::error::{Error, ErrorType};
use liquers_core::metadata::Metadata;
use liquers_core::query::Key;
use liquers_core::store::AsyncStore;
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
    async fn call(&self, function: &js_sys::Function, args: &js_sys::Array) -> Result<JsValue, Error> {
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
        let data = js_sys::Reflect::get(&result, &JsValue::from_str("data")).map_err(|e| {
            js_error_to_liquers(e, ErrorType::KeyReadError)
        })?;
        let metadata = js_sys::Reflect::get(&result, &JsValue::from_str("metadata"))
            .unwrap_or(JsValue::UNDEFINED);
        Ok((
            self.bytes_from_js(key, &data)?,
            self.metadata_from_js(key, &metadata)?,
        ))
    }

    async fn get_metadata(&self, key: &Key) -> Result<Metadata, Error> {
        check_key(key, &self.store_name())?;
        match &self.methods.get_metadata {
            Some(function) => {
                let value = self.call(function, &self.args1(key)).await?;
                self.metadata_from_js(key, &value)
            }
            // Deriving it from `get` costs a body read, but a store that can serve data can always
            // answer this, so refusing would be worse than being slow.
            None => self.get(key).await.map(|(_, metadata)| metadata),
        }
    }

    async fn set(&self, key: &Key, data: &[u8], metadata: &Metadata) -> Result<(), Error> {
        check_key(key, &self.store_name())?;
        let function = self.methods.set.as_ref().ok_or_else(|| self.missing(key, "set"))?;
        let args = js_sys::Array::new();
        args.push(&JsValue::from_str(&key.encode()));
        args.push(&js_sys::Uint8Array::from(data).into());
        args.push(&self.metadata_to_js(key, metadata)?);
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
        let function = self.methods.remove.as_ref().ok_or_else(|| self.missing(key, "remove"))?;
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
        let function = self.methods.makedir.as_ref().ok_or_else(|| self.missing(key, "makedir"))?;
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
        let function = self.methods.is_dir.as_ref().ok_or_else(|| self.missing(key, "isDir"))?;
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
