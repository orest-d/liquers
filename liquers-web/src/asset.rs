//! `Asset` and `State` — the low-level evaluation surface beside `evaluate`.
//!
//! `evaluate(query)` is the convenience path: it resolves with a converted JavaScript value and
//! discards everything else. This module exposes the asset itself, which is what makes three
//! things possible that the convenience path cannot express:
//!
//! - **Cancellation** (`ASYNCQ04`). The cancellable thing is the *asset*, not the `Promise` — a
//!   `Promise` has no cancellation in JavaScript. See the limitation below.
//! - **Metadata and logs** (`EVAL03`). Available on the resolved `State`, so a caller can read the
//!   type identifier, the status, and the log entries a command wrote.
//! - **Status inspection**.
//!
//! # Limitation: cancellation is inert in this phase
//!
//! On `wasm32` the environment uses `ImmediateAssetManager`, which follows from Phase 1 decision 5
//! — evaluate immediately, no background scheduler, because a browser is not expected to run long
//! background calculations. A consequence, measured rather than assumed: **`get_asset` returns an
//! asset that has already reached a terminal status.** By the time a caller holds the handle there
//! is nothing left to cancel, so `cancel()` succeeds and changes nothing, and `get()` returns the
//! computed value.
//!
//! `cancel()` is therefore a no-op today. It is exposed anyway because it is the correct *shape*:
//! the surface does not change when a deferred asset manager arrives, only the behaviour. Callers
//! must treat `cancel()` as a request that may be ignored — which is its documented contract on
//! native too, where an asset past `Processing` also ignores it.
//!
//! Tracked as `WEB-CANCELLATION-INERT` in `specs/ISSUES.md`. The condition that reverses it is a
//! wasm asset manager that defers evaluation past `get_asset`.
//!
//! # Why every method returns a `Promise`
//!
//! Phase 2's signature list wrote `status()` and `cancel()` as synchronous. They are not:
//! `AssetRef::status` and `AssetRef::cancel` are both `async` in `liquers-core`, because the asset
//! state lives behind an async lock. Exposing them synchronously would require blocking, which is
//! exactly what this crate refuses to do on an event loop. They are `Promise`s instead — a
//! deviation from Phase 2 recorded in Phase 4's execution notes.

use liquers_core::assets::{AssetManager, AssetRef};
use liquers_core::context::{EnvRef, Environment};
use liquers_core::error::{Error, ErrorType};
use liquers_core::metadata::{Metadata, Status};
use liquers_core::query::Query;
use liquers_core::state::State;
use wasm_bindgen::prelude::*;

use crate::bridge::value_to_js;
use crate::environment::WebEnvironment;
use crate::error::liquers_error_to_js;
use crate::LiquersValue;

/// The lowercase name of a status, as it crosses to JavaScript.
///
/// Explicit rather than derived from `Debug`, so that renaming a variant in `liquers-core` cannot
/// silently change the string a page matches on. Exhaustive by house rule, so a new variant is a
/// compile error here.
pub fn status_name(status: Status) -> &'static str {
    match status {
        Status::None => "none",
        Status::Directory => "directory",
        Status::Recipe => "recipe",
        Status::Submitted => "submitted",
        Status::Dependencies => "dependencies",
        Status::Processing => "processing",
        Status::Partial => "partial",
        Status::Error => "error",
        Status::Storing => "storing",
        Status::Ready => "ready",
        Status::Expired => "expired",
        Status::Cancelled => "cancelled",
        Status::Source => "source",
        Status::Override => "override",
        Status::Volatile => "volatile",
    }
}

/// Resolves a query to an asset without waiting for it to become ready.
///
/// Generic over `E`, like everything else that is not a `#[wasm_bindgen]` export.
pub async fn get_asset_for<E>(envref: EnvRef<E>, query: Query) -> Result<AssetRef<E>, Error>
where
    E: Environment,
{
    envref.get_asset_manager().get_asset(&query).await
}

/// Converts a `Metadata` to a plain JavaScript object.
///
/// Plain JSON rather than a wrapper class: metadata is a read-only bag whose shape is already
/// defined by its `serde` representation, and a class would have to be kept in step with it by
/// hand.
pub fn metadata_to_js(metadata: &Metadata) -> Result<JsValue, Error> {
    // `Metadata` is an enum over a typed record and untyped legacy JSON, and does not implement
    // `Serialize` itself — the two variants would serialize to different shapes. Its `to_json`
    // flattens that distinction, which is exactly what a JavaScript caller needs.
    let json: serde_json::Value = metadata
        .to_json()
        .and_then(|s| serde_json::from_str(&s))
        .map_err(|e| {
            Error::from_error(
                ErrorType::SerializationError,
                format!("Could not serialize metadata: {e}"),
            )
        })?;
    crate::bridge::serialize_to_js(&json).map_err(|e| {
        Error::from_error(
            ErrorType::ConversionError,
            format!("Could not convert metadata: {e}"),
        )
    })
}

/// The log entries a metadata record carries, as a JavaScript array.
///
/// Always an array — an absent log is an empty one, not a missing field. A free function rather
/// than only a `State` method because the generic command adapter needs it too, for the
/// `state: "state"` mode.
pub fn log_to_js(metadata: &Metadata) -> Result<JsValue, Error> {
    let entries = match metadata {
        Metadata::MetadataRecord(m) => serde_json::to_value(&m.log),
        // Legacy metadata is untyped JSON; take the `log` array if it has one.
        Metadata::LegacyMetadata(v) => Ok(v
            .get("log")
            .cloned()
            .unwrap_or_else(|| serde_json::Value::Array(Vec::new()))),
    }
    .map_err(|e| {
        Error::from_error(
            ErrorType::SerializationError,
            format!("Could not read the log: {e}"),
        )
    })?;
    crate::bridge::serialize_to_js(&entries).map_err(|e| {
        Error::from_error(
            ErrorType::ConversionError,
            format!("Could not convert the log: {e}"),
        )
    })
}

/// An asset, visible to JavaScript.
#[wasm_bindgen(js_name = Asset)]
pub struct LiquersAsset {
    inner: AssetRef<WebEnvironment>,
}

impl LiquersAsset {
    pub fn from_ref(inner: AssetRef<WebEnvironment>) -> Self {
        LiquersAsset { inner }
    }

    pub fn asset_ref(&self) -> &AssetRef<WebEnvironment> {
        &self.inner
    }
}

#[wasm_bindgen(js_class = Asset)]
impl LiquersAsset {
    /// Resolves with the current status as a lowercase string.
    pub fn status(&self) -> js_sys::Promise {
        let inner = self.inner.clone();
        wasm_bindgen_futures::future_to_promise(async move {
            Ok(JsValue::from_str(status_name(inner.status().await)))
        })
    }

    /// Resolves with the asset's [`LiquersState`] once it is ready.
    ///
    /// Rejects with a `LiquersError` if the asset fails or is cancelled.
    pub fn get(&self) -> js_sys::Promise {
        let inner = self.inner.clone();
        wasm_bindgen_futures::future_to_promise(async move {
            match inner.get().await {
                Ok(state) => Ok(LiquersState::from_state(state).into()),
                Err(e) => Err(liquers_error_to_js(e)),
            }
        })
    }

    /// Requests cancellation.
    ///
    /// Resolves once the request has been registered — not once the asset has stopped, and not as
    /// a promise that anything was stopped. Cancelling an asset that has already reached a
    /// terminal status is a no-op, not an error, and **under the immediate asset manager every
    /// asset a caller can reach is already terminal** — see this module's limitation section.
    pub fn cancel(&self) -> js_sys::Promise {
        let inner = self.inner.clone();
        wasm_bindgen_futures::future_to_promise(async move {
            match inner.cancel().await {
                Ok(()) => Ok(JsValue::UNDEFINED),
                Err(e) => Err(liquers_error_to_js(e)),
            }
        })
    }

    /// Resolves with the asset's descriptive information as a plain object.
    #[wasm_bindgen(js_name = getAssetInfo)]
    pub fn get_asset_info(&self) -> js_sys::Promise {
        let inner = self.inner.clone();
        wasm_bindgen_futures::future_to_promise(async move {
            match inner.get_asset_info().await {
                Ok(info) => crate::bridge::serialize_to_js(&info).map_err(|e| {
                    liquers_error_to_js(Error::from_error(
                        ErrorType::ConversionError,
                        format!("Could not convert asset info: {e}"),
                    ))
                }),
                Err(e) => Err(liquers_error_to_js(e)),
            }
        })
    }
}

/// A state — a value together with its metadata — visible to JavaScript.
#[wasm_bindgen(js_name = State)]
pub struct LiquersState {
    inner: State<liquers_lib::value::Value>,
}

impl LiquersState {
    pub fn from_state(inner: State<liquers_lib::value::Value>) -> Self {
        LiquersState { inner }
    }

    pub fn state(&self) -> &State<liquers_lib::value::Value> {
        &self.inner
    }
}

#[wasm_bindgen(js_class = State)]
impl LiquersState {
    /// The value, as a `Value` wrapper.
    ///
    /// Throws when the state carries an error rather than data — `State::value` is the guarded
    /// accessor, and going around it is how an error state silently becomes `null`.
    pub fn value(&self) -> Result<LiquersValue, JsValue> {
        match self.inner.value() {
            Ok(v) => Ok(LiquersValue::from_value((*v).clone())),
            Err(e) => Err(liquers_error_to_js(e)),
        }
    }

    /// The value, converted structurally to JavaScript — the same conversion `evaluate` performs.
    #[wasm_bindgen(js_name = toJS)]
    pub fn to_js(&self) -> Result<JsValue, JsValue> {
        let value = self.inner.value().map_err(liquers_error_to_js)?;
        value_to_js(value.as_ref()).map_err(liquers_error_to_js)
    }

    /// The metadata, as a plain object.
    pub fn metadata(&self) -> Result<JsValue, JsValue> {
        metadata_to_js(self.inner.metadata.as_ref()).map_err(liquers_error_to_js)
    }

    /// The log entries recorded while producing this state (`EVAL03`).
    ///
    /// A separate accessor rather than only a field of `metadata()`, because reading logs after a
    /// failure is the common case and should not require knowing the metadata layout.
    pub fn log(&self) -> Result<JsValue, JsValue> {
        log_to_js(self.inner.metadata.as_ref()).map_err(liquers_error_to_js)
    }

    /// The status of this state, as a lowercase string.
    #[wasm_bindgen(getter)]
    pub fn status(&self) -> String {
        status_name(self.inner.metadata.status()).to_string()
    }
}
