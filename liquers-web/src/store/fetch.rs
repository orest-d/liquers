//! `FetchStore` — a read-only store that fetches over HTTP.
//!
//! The URL is a configured prefix plus the key with this store's `key_prefix` removed, so a store
//! mounted at `data` maps `data/input.csv` to `{url_prefix}input.csv`. This is the one store that
//! strips its prefix; see `specs/design/liquers-web-store/phase2-architecture.md`, "Key mapping".
//!
//! # Two decisions worth knowing before reading the code
//!
//! **Listing comes from a configured key set, not from the network.** HTTP has no directory
//! listing, so the store is told which keys it has. `contains`, `is_dir` and `listdir` are answered
//! from that set, which is what makes them consistent with each other and with what `get` will
//! actually fetch. Populating the set by crawling is future work.
//!
//! **Metadata inference prefers the filename extension over the response `Content-Type`** — the
//! inverse of the usual web rule. Liquers dispatches deserialization on `data_format`, which comes
//! from the extension; a static file server that labels everything `text/plain` or
//! `application/octet-stream` would otherwise break every command downstream of a fetched asset.
//!
//! `fetch` itself is taken from [`js_sys::global`] rather than from `web_sys::Window`, so the
//! store works in a window, in a worker, and under Node — which is what allows the pure parts
//! below to be tested without a browser.

use std::collections::{BTreeMap, BTreeSet};

use async_trait::async_trait;
use liquers_core::error::Error;
use liquers_core::metadata::{Metadata, MetadataRecord, Status};
use liquers_core::query::Key;
use liquers_core::store::AsyncStore;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

use crate::store::key_guard::check_key;

/// The media type `file_extension_to_media_type` returns when it does not recognise an extension.
///
/// Used as "no information", which costs nothing: a key genuinely holding octet-stream data gets
/// the same answer from either source.
const UNKNOWN_MEDIA_TYPE: &str = "application/octet-stream";

/// Builds the URL for a key.
///
/// The key's leading `prefix` segments are removed, and the rest are joined verbatim with `/`.
/// Verbatim rather than percent-encoded: the set of URLs reachable through a `Key` is narrower
/// than HTTP's, which is an accepted limitation of this design (Phase 1 Q6). Key shapes that would
/// change the URL's meaning are refused by [`check_key`] instead of being escaped.
pub fn key_to_url(
    url_prefix: &str,
    prefix: &Key,
    key: &Key,
    store_name: &str,
) -> Result<String, Error> {
    check_key(key, store_name)?;
    if !key.has_key_prefix(prefix) {
        return Err(Error::key_not_supported(key, store_name));
    }
    let relative: Vec<&str> = key
        .iter()
        .skip(prefix.len())
        .map(|segment| segment.encode())
        .collect();
    Ok(format!("{}{}", normalize_url_prefix(url_prefix), relative.join("/")))
}

/// Ensures a URL prefix ends in `/`, so joining a relative key cannot fuse two segments.
pub fn normalize_url_prefix(url_prefix: &str) -> String {
    if url_prefix.is_empty() || url_prefix.ends_with('/') {
        url_prefix.to_string()
    } else {
        format!("{url_prefix}/")
    }
}

/// Derives metadata for a fetched key from its name and the response headers.
///
/// A free function over plain data, deliberately: it is the whole of the precedence rule, and this
/// way it is tested without a browser or a server.
pub fn infer_metadata(
    key: &Key,
    content_type: Option<&str>,
    content_length: Option<u64>,
) -> Metadata {
    let mut record = MetadataRecord::new();
    record.with_key(key.to_owned());

    // `with_filename` derives `media_type` from the extension as a side effect.
    if let Some(name) = key.filename() {
        record.with_filename(name.encode().to_string());
    }

    let from_extension = record.media_type.clone();
    if from_extension.is_empty() || from_extension == UNKNOWN_MEDIA_TYPE {
        if let Some(declared) = content_type.and_then(media_type_of) {
            record.with_media_type(declared);
        }
    }

    // `with_file_size` is on `Metadata`, not on `MetadataRecord`; the field is public.
    record.file_size = content_length;
    record.with_status(Status::Source);
    Metadata::MetadataRecord(record)
}

/// Extracts the media type from a `Content-Type` header, dropping parameters such as `charset`.
///
/// `None` for a header that carries no usable type, so the caller keeps the extension's answer
/// rather than overwriting it with an empty string.
pub fn media_type_of(content_type: &str) -> Option<String> {
    let base = content_type.split(';').next()?.trim();
    if base.is_empty() {
        None
    } else {
        Some(base.to_ascii_lowercase())
    }
}

/// Builds the parent → child-name index for a set of known keys.
///
/// Every ancestor of every key becomes a directory, so `listdir` and `is_dir` agree with
/// `contains` without a network round trip.
pub fn directory_index(keys: &BTreeSet<Key>) -> BTreeMap<Key, BTreeSet<String>> {
    let mut dirs: BTreeMap<Key, BTreeSet<String>> = BTreeMap::new();
    for key in keys {
        for depth in 0..key.len() {
            let parent = key.prefix_of_size(depth).unwrap_or_default();
            if let Some(child) = key.iter().nth(depth) {
                dirs.entry(parent).or_default().insert(child.encode().to_string());
            }
        }
    }
    dirs
}

/// A read-only store that fetches over HTTP.
pub struct FetchStore {
    prefix: Key,
    url_prefix: String,
    keys: BTreeSet<Key>,
    dirs: BTreeMap<Key, BTreeSet<String>>,
}

impl FetchStore {
    /// Creates a store serving `keys` from `url_prefix`.
    ///
    /// Keys are validated here rather than at first use: a configuration naming an unusable key is
    /// a mistake worth reporting when the page starts, not when something eventually reads it.
    pub fn new(prefix: &Key, url_prefix: &str, keys: Vec<Key>) -> Result<Self, Error> {
        let name = format!("{} fetch store", prefix);
        let mut set = BTreeSet::new();
        for key in keys {
            check_key(&key, &name)?;
            if !key.has_key_prefix(prefix) {
                return Err(Error::key_not_supported(&key, &name));
            }
            set.insert(key);
        }
        let dirs = directory_index(&set);
        Ok(Self {
            prefix: prefix.to_owned(),
            url_prefix: normalize_url_prefix(url_prefix),
            keys: set,
            dirs,
        })
    }

    fn url_for(&self, key: &Key) -> Result<String, Error> {
        key_to_url(&self.url_prefix, &self.prefix, key, &self.store_name())
    }

    /// Issues a request and returns the response.
    ///
    /// `fetch` is read off the global object rather than `web_sys::Window`, so this works in a
    /// window, a worker and under Node. `method` is passed in a plain options object, avoiding
    /// `RequestInit`, whose builder API has changed shape between web-sys releases.
    async fn request(&self, key: &Key, url: &str, method: &str) -> Result<web_sys::Response, Error> {
        let global = js_sys::global();
        let function = js_sys::Reflect::get(&global, &JsValue::from_str("fetch"))
            .ok()
            .and_then(|f| f.dyn_into::<js_sys::Function>().ok())
            .ok_or_else(|| {
                Error::key_read_error(
                    key,
                    &self.store_name(),
                    &"no global `fetch` is available in this JavaScript environment",
                )
            })?;

        let options = js_sys::Object::new();
        js_sys::Reflect::set(
            &options,
            &JsValue::from_str("method"),
            &JsValue::from_str(method),
        )
        .map_err(|_| {
            Error::key_read_error(key, &self.store_name(), &"could not build request options")
        })?;

        let promise = function
            .call2(&global, &JsValue::from_str(url), &options)
            .map_err(|e| self.js_read_error(key, url, &e))?;
        let promise: js_sys::Promise = promise
            .dyn_into()
            .map_err(|_| Error::key_read_error(key, &self.store_name(), &"fetch did not return a Promise"))?;

        let resolved = JsFuture::from(promise)
            .await
            .map_err(|e| self.js_read_error(key, url, &e))?;
        resolved.dyn_into::<web_sys::Response>().map_err(|_| {
            Error::key_read_error(key, &self.store_name(), &"fetch did not resolve to a Response")
        })
    }

    fn js_read_error(&self, key: &Key, url: &str, error: &JsValue) -> Error {
        let detail = error
            .as_string()
            .or_else(|| js_sys::Reflect::get(error, &JsValue::from_str("message")).ok()?.as_string())
            .unwrap_or_else(|| format!("{error:?}"));
        Error::key_read_error(
            key,
            &self.store_name(),
            &format!("request to {url} failed: {detail}"),
        )
    }

    /// Maps a non-success status onto the right error.
    ///
    /// 404 is `KeyNotFound` rather than a read error, because "the server does not have it" is the
    /// same fact as "the key is absent", and the asset layer treats the two differently.
    fn status_error(&self, key: &Key, url: &str, status: u16) -> Error {
        if status == 404 {
            Error::key_not_found(key)
        } else {
            Error::key_read_error(
                key,
                &self.store_name(),
                &format!("request to {url} returned HTTP {status}"),
            )
        }
    }

    fn header(response: &web_sys::Response, name: &str) -> Option<String> {
        response.headers().get(name).ok().flatten()
    }

    fn metadata_from_response(&self, key: &Key, response: &web_sys::Response, body_len: Option<u64>) -> Metadata {
        let content_type = Self::header(response, "content-type");
        let content_length = body_len.or_else(|| {
            Self::header(response, "content-length").and_then(|v| v.trim().parse::<u64>().ok())
        });
        infer_metadata(key, content_type.as_deref(), content_length)
    }

    fn directory_metadata(&self, key: &Key) -> Metadata {
        Metadata::MetadataRecord(self.default_metadata(key, true))
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl AsyncStore for FetchStore {
    fn store_name(&self) -> String {
        format!("{} fetch store", self.prefix)
    }

    fn key_prefix(&self) -> Key {
        self.prefix.to_owned()
    }

    async fn get(&self, key: &Key) -> Result<(Vec<u8>, Metadata), Error> {
        let url = self.url_for(key)?;
        let response = self.request(key, &url, "GET").await?;
        if !response.ok() {
            return Err(self.status_error(key, &url, response.status()));
        }
        let buffer = response
            .array_buffer()
            .map_err(|e| self.js_read_error(key, &url, &e))?;
        let buffer = JsFuture::from(buffer)
            .await
            .map_err(|e| self.js_read_error(key, &url, &e))?;
        let data = js_sys::Uint8Array::new(&buffer).to_vec();
        let metadata = self.metadata_from_response(key, &response, Some(data.len() as u64));
        Ok((data, metadata))
    }

    /// Uses `HEAD`, so reading metadata does not download the body.
    ///
    /// Servers that reject `HEAD` (405, or 501 for "not implemented") get a `GET` instead — a
    /// static host is entitled to support only `GET`, and failing there would make metadata
    /// unavailable for an object that reads perfectly well.
    async fn get_metadata(&self, key: &Key) -> Result<Metadata, Error> {
        if self.is_dir(key).await? {
            return Ok(self.directory_metadata(key));
        }
        let url = self.url_for(key)?;
        let response = self.request(key, &url, "HEAD").await?;
        if response.ok() {
            return Ok(self.metadata_from_response(key, &response, None));
        }
        match response.status() {
            405 | 501 => {
                let (_, metadata) = self.get(key).await?;
                Ok(metadata)
            }
            status => Err(self.status_error(key, &url, status)),
        }
    }

    /// Read-only. `set_metadata` has no default in the trait, so it must be written out.
    async fn set_metadata(&self, key: &Key, _metadata: &Metadata) -> Result<(), Error> {
        Err(Error::key_not_supported(key, &self.store_name()))
    }

    async fn contains(&self, key: &Key) -> Result<bool, Error> {
        Ok(self.keys.contains(key) || self.dirs.contains_key(key))
    }

    async fn is_dir(&self, key: &Key) -> Result<bool, Error> {
        Ok(self.dirs.contains_key(key))
    }

    async fn listdir(&self, key: &Key) -> Result<Vec<String>, Error> {
        Ok(self
            .dirs
            .get(key)
            .map(|names| names.iter().cloned().collect())
            .unwrap_or_default())
    }

    async fn keys(&self) -> Result<Vec<Key>, Error> {
        Ok(self.keys.iter().cloned().collect())
    }

    /// Must be overridden: the trait default is `false`, and `AsyncStoreRouter` consults it, so a
    /// store that leaves the default is silently never selected.
    fn is_supported(&self, key: &Key) -> bool {
        key.has_key_prefix(&self.prefix) && check_key(key, "fetch store").is_ok()
    }
}
