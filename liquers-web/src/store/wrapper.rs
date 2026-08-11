//! The store, visible to JavaScript.
//!
//! The method set is deliberately the same vocabulary [`crate::store::js_store`] expects of a page
//! object, read in the other direction: what a page can call here, a page can also implement.
//!
//! Every method clones the `Arc` and copies its arguments into owned values **before** entering the
//! async block, because [`wasm_bindgen_futures::future_to_promise`] requires a `'static` future.
//! This mirrors `crate::asset`.

use std::sync::Arc;

use liquers_core::error::Error;
use liquers_core::metadata::Metadata;
use liquers_core::parse::parse_key;
use liquers_core::query::Key;
use liquers_core::store::AsyncStore;
use wasm_bindgen::prelude::*;

use crate::error::liquers_error_to_js;

/// A Liquers store, visible to JavaScript.
#[wasm_bindgen(js_name = Store)]
pub struct LiquersStore {
    inner: Arc<dyn AsyncStore>,
}

impl LiquersStore {
    pub fn new(inner: Arc<dyn AsyncStore>) -> Self {
        Self { inner }
    }
}

/// Parses a key argument, or rejects with a structured error.
fn key_of(text: &str) -> Result<Key, Error> {
    parse_key(text)
}

/// Converts `Metadata` to the plain object JavaScript sees.
fn metadata_to_js(metadata: &Metadata) -> Result<JsValue, Error> {
    crate::store::metadata_to_js_value(&Key::new(), metadata, "store")
}

/// Converts a plain JavaScript object into `Metadata`, accepting a partial document.
fn metadata_from_js(key: &Key, value: &JsValue) -> Result<Metadata, Error> {
    crate::store::metadata_from_js_value(key, value, "store")
}

/// Runs a fallible async body as a `Promise`, mapping any Liquers error to a structured rejection.
fn promise<F>(body: F) -> js_sys::Promise
where
    F: std::future::Future<Output = Result<JsValue, Error>> + 'static,
{
    wasm_bindgen_futures::future_to_promise(async move { body.await.map_err(liquers_error_to_js) })
}

#[wasm_bindgen(js_class = Store)]
impl LiquersStore {
    /// Resolves with the stored bytes as a `Uint8Array`.
    pub fn get(&self, key: &str) -> js_sys::Promise {
        let inner = self.inner.clone();
        let key = key.to_string();
        promise(async move {
            let key = key_of(&key)?;
            let (data, _) = inner.get(&key).await?;
            Ok(js_sys::Uint8Array::from(&data[..]).into())
        })
    }

    /// Resolves with the key's metadata as a plain object.
    #[wasm_bindgen(js_name = getMetadata)]
    pub fn get_metadata(&self, key: &str) -> js_sys::Promise {
        let inner = self.inner.clone();
        let key = key.to_string();
        promise(async move {
            let key = key_of(&key)?;
            metadata_to_js(&inner.get_metadata(&key).await?)
        })
    }

    /// Stores bytes, with optional metadata.
    pub fn set(&self, key: &str, data: &[u8], metadata: JsValue) -> js_sys::Promise {
        let inner = self.inner.clone();
        let key = key.to_string();
        let data = data.to_vec();
        promise(async move {
            let key = key_of(&key)?;
            let metadata = metadata_from_js(&key, &metadata)?;
            inner.set(&key, &data, &metadata).await?;
            Ok(JsValue::UNDEFINED)
        })
    }

    #[wasm_bindgen(js_name = setMetadata)]
    pub fn set_metadata(&self, key: &str, metadata: JsValue) -> js_sys::Promise {
        let inner = self.inner.clone();
        let key = key.to_string();
        promise(async move {
            let key = key_of(&key)?;
            let metadata = metadata_from_js(&key, &metadata)?;
            inner.set_metadata(&key, &metadata).await?;
            Ok(JsValue::UNDEFINED)
        })
    }

    pub fn remove(&self, key: &str) -> js_sys::Promise {
        let inner = self.inner.clone();
        let key = key.to_string();
        promise(async move {
            inner.remove(&key_of(&key)?).await?;
            Ok(JsValue::UNDEFINED)
        })
    }

    pub fn removedir(&self, key: &str) -> js_sys::Promise {
        let inner = self.inner.clone();
        let key = key.to_string();
        promise(async move {
            inner.removedir(&key_of(&key)?).await?;
            Ok(JsValue::UNDEFINED)
        })
    }

    pub fn makedir(&self, key: &str) -> js_sys::Promise {
        let inner = self.inner.clone();
        let key = key.to_string();
        promise(async move {
            inner.makedir(&key_of(&key)?).await?;
            Ok(JsValue::UNDEFINED)
        })
    }

    pub fn contains(&self, key: &str) -> js_sys::Promise {
        let inner = self.inner.clone();
        let key = key.to_string();
        promise(async move { Ok(JsValue::from_bool(inner.contains(&key_of(&key)?).await?)) })
    }

    #[wasm_bindgen(js_name = isDir)]
    pub fn is_dir(&self, key: &str) -> js_sys::Promise {
        let inner = self.inner.clone();
        let key = key.to_string();
        promise(async move { Ok(JsValue::from_bool(inner.is_dir(&key_of(&key)?).await?)) })
    }

    /// Resolves with the names directly inside a directory.
    pub fn listdir(&self, key: &str) -> js_sys::Promise {
        let inner = self.inner.clone();
        let key = key.to_string();
        promise(async move {
            let names = inner.listdir(&key_of(&key)?).await?;
            let array = js_sys::Array::new();
            for name in names {
                array.push(&JsValue::from_str(&name));
            }
            Ok(array.into())
        })
    }

    /// The store's own name, which identifies which backend answered.
    #[wasm_bindgen(js_name = storeName)]
    pub fn store_name(&self) -> String {
        self.inner.store_name()
    }
}
