//! `EVAL` and `ASYNCQ` — evaluating a query, and the `Promise` bridge.
//!
//! There is **no blocking evaluation entry point**, deliberately. A browser must not simulate
//! synchronous evaluation, and offering a blocking API on a single-threaded event loop is how a
//! page freezes. `ASYNCQ06`/`ASYNCQ07` are satisfied by construction rather than by discipline.

use liquers_core::assets::AssetManager;
use liquers_core::context::{EnvRef, Environment};
use liquers_core::error::Error;
use liquers_core::query::Query;
use wasm_bindgen::prelude::*;

use crate::bridge::{value_to_js, JsValueBridge};
use crate::error::liquers_error_to_js;

/// Evaluates a query, returning a `Promise`.
///
/// Generic over `E` so a downstream crate with its own environment reuses it — see
/// [`crate::bridge`]'s module rule.
///
/// The returned future owns an `EnvRef` clone and an owned `Query`; it borrows nothing, which is
/// what lets it outlive the call that created it.
pub fn evaluate_to_promise<E>(envref: EnvRef<E>, query: Query) -> js_sys::Promise
where
    E: Environment,
    E::Value: JsValueBridge,
{
    wasm_bindgen_futures::future_to_promise(async move {
        match evaluate_query(envref, query).await {
            Ok(js) => Ok(js),
            // Rejection carries a structured LiquersError, never a bare string: a string would
            // discard the error type, position, query and key (`ASYNCQ02`).
            Err(e) => Err(liquers_error_to_js(e)),
        }
    })
}

/// Evaluates a query to a JavaScript value.
pub async fn evaluate_query<E>(envref: EnvRef<E>, query: Query) -> Result<JsValue, Error>
where
    E: Environment,
    E::Value: JsValueBridge,
{
    let manager = envref.get_asset_manager();
    let asset = manager.get_asset(&query).await?;
    let state = asset.get().await?;
    value_to_js(state.data_unchecked().as_ref())
}
