//! `STORE` — browser stores and declarative store composition.
//!
//! Implements the `STORE` feature of `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md` for the browser,
//! which `specs/design/liquers-web/` deferred. Design: `specs/design/liquers-web-store/`.
//!
//! Four stores behind the existing [`liquers_core::store::AsyncStoreRouter`]:
//!
//! | Store | Config `type` | Writes |
//! |---|---|---|
//! | `LocalStorageStore` | `localstorage` | yes — the full `AsyncStore` contract |
//! | `FetchStore` | `http` / `https` | no — read-only over HTTP |
//! | `JsStore` | `js` | whatever the page object implements |
//!
//! The router itself is reused unchanged: it is already `?Send` on wasm, and routing is "the first
//! store whose `key_prefix` matches *and* whose `is_supported` returns true".
//!
//! # Two things every store here must do
//!
//! - **Override `is_supported`.** It defaults to `false`, and the router consults it, so a store
//!   that forgets is silently never selected.
//! - **Implement `set_metadata`.** It is the one `AsyncStore` method with no default.
//!
//! # Why parts of this module are free functions
//!
//! `localStorage` does not exist under Node, so anything touching it can only be tested in a real
//! browser. The logic that can *silently corrupt or misroute data* is therefore deliberately
//! expressed as pure functions over plain data — [`encoding`], [`key_guard`], and later the URL
//! builder and metadata inference — so it is covered by the fast Node loop rather than by the
//! browser suite. That is a design requirement, not a testing convenience; see
//! `specs/design/liquers-web-store/phase3-examples.md`.

pub mod builder;
pub mod encoding;
pub mod fetch;
pub mod js_store;
pub mod key_guard;
pub mod local_storage;
pub mod wrapper;

pub use encoding::{decode_envelope, encode_envelope, ByteEncoding};

use liquers_core::error::Error;
use liquers_core::metadata::{Metadata, MetadataRecord};
use liquers_core::query::Key;
use wasm_bindgen::prelude::JsValue;

/// Converts a plain JavaScript object into [`Metadata`].
///
/// **Partial metadata is accepted**, and that is the whole reason this helper exists rather than a
/// bare `Metadata::from_json`. A page writes `{media_type: "text/plain"}`, not a complete
/// `MetadataRecord`; `Metadata::from_json` cannot deserialize that into a record and falls back to
/// `Metadata::LegacyMetadata`, whose accessors then return JSON-*quoted* values —
/// `"\"text/plain\""` rather than `text/plain`. See
/// `specs/issues/CORE-LEGACY-METADATA-ACCESSORS-RETURN-JSON.md`; this normalizes at the boundary
/// so that no page has to know about it.
///
/// A document that *is* a complete record is used as-is, so nothing is lost for a caller that
/// round-trips metadata it received from Liquers.
pub(crate) fn metadata_from_js_value(
    key: &Key,
    value: &JsValue,
    store_name: &str,
) -> Result<Metadata, Error> {
    if value.is_undefined() || value.is_null() {
        let mut record = MetadataRecord::new();
        record.with_key(key.to_owned());
        return Ok(Metadata::MetadataRecord(record));
    }

    let json = js_sys::JSON::stringify(value)
        .ok()
        .and_then(|s| s.as_string())
        .ok_or_else(|| {
            Error::key_read_error(key, store_name, &"metadata is not JSON-serializable")
        })?;

    // The complete-record path first: a caller returning metadata it got from Liquers keeps
    // everything, including fields this helper does not know about.
    if let Ok(record) = serde_json::from_str::<MetadataRecord>(&json) {
        return Ok(Metadata::MetadataRecord(record));
    }

    // Otherwise, overlay the fields a page plausibly sets onto a default record.
    let parsed: serde_json::Value = serde_json::from_str(&json)
        .map_err(|e| Error::key_read_error(key, store_name, &format!("invalid metadata: {e}")))?;
    let Some(object) = parsed.as_object() else {
        return Err(Error::key_read_error(
            key,
            store_name,
            &"metadata must be an object",
        ));
    };

    let mut record = MetadataRecord::new();
    record.with_key(key.to_owned());
    if let Some(filename) = object.get("filename").and_then(|v| v.as_str()) {
        // Sets `media_type` from the extension as a side effect, so it comes before `media_type`.
        record.with_filename(filename.to_string());
    }
    if let Some(media_type) = object.get("media_type").and_then(|v| v.as_str()) {
        record.with_media_type(media_type.to_string());
    }
    if let Some(type_identifier) = object.get("type_identifier").and_then(|v| v.as_str()) {
        record.type_identifier = type_identifier.to_string();
    }
    Ok(Metadata::MetadataRecord(record))
}

/// Converts [`Metadata`] into the plain object JavaScript sees.
pub(crate) fn metadata_to_js_value(
    key: &Key,
    metadata: &Metadata,
    store_name: &str,
) -> Result<JsValue, Error> {
    let json = metadata.to_json().map_err(|e| {
        Error::key_write_error(
            key,
            store_name,
            &format!("metadata is not serializable: {e}"),
        )
    })?;
    js_sys::JSON::parse(&json).map_err(|e| {
        Error::key_write_error(
            key,
            store_name,
            &format!("metadata JSON did not parse back: {e:?}"),
        )
    })
}

pub use builder::{build_router, WebStoreFactory};
pub use fetch::FetchStore;
pub use js_store::JsStore;
pub use key_guard::check_key;
pub use local_storage::LocalStorageStore;
pub use wrapper::LiquersStore;
