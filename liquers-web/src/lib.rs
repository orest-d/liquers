//! Browser/JavaScript integration of Liquers, compiled to WebAssembly.
//!
//! This crate is the `wasm32` half of the language integration described in
//! `specs/LANGUAGE-INTEGRATION_GUIDE.md`. A page constructs an environment, evaluates queries as
//! `Promise`s, and registers commands written in JavaScript.
//!
//! # Target
//!
//! **This crate only functions on `wasm32`.** `JsValue` is `!Send`/`!Sync` on every target, and on
//! native the `MaybeSend`/`MaybeSync` markers resolve to `Send`/`Sync`, so the bridge types cannot
//! exist there. Rather than fail to compile natively — which would break
//! `cargo check --workspace` for everyone — the functional body is `wasm32`-gated and a native
//! build produces an intentionally empty crate. The workspace's `default-members` also excludes
//! this crate, so the native test loop never builds it.
//!
//! Build it with:
//!
//! ```text
//! cargo check -p liquers-web --target wasm32-unknown-unknown
//! wasm-pack test --headless --chrome liquers-web
//! ```
//!
//! # Architecture
//!
//! There is no new `Environment` and no new `CommandExecutor`.
//! `liquers_lib::environment::DefaultEnvironment` is already generic over the value type and
//! already selects the inline asset manager on `wasm32`, and the executor closure aliases already
//! drop `Send`/`Sync` there — so a JavaScript command is an ordinary registered async command
//! whose closure owns a `js_sys::Function`. This crate contributes a value bridge, a
//! `#[wasm_bindgen]` object/eval/command surface, and a `Promise` bridge.
//!
//! See `specs/liquers-web/` for the full design.

#![cfg(target_arch = "wasm32")]

pub mod bridge;
pub mod error;
pub mod environment;
pub mod objects;
pub mod default_value;
pub mod value;

pub use bridge::{ConversionPolicy, JsValueBridge};
pub use environment::{version, LiquersEnvironment, WebEnvironment};
pub use error::LiquersError;
pub use objects::{LiquersKey, LiquersQuery};
pub use value::{JsOpaque, ORIGIN_JAVASCRIPT};

use wasm_bindgen::prelude::*;

/// Initializes the global environment.
///
/// Returns a `Promise` rather than blocking, as the guide requires of a browser integration — even
/// though the current implementation has nothing to await, because changing a synchronous
/// initializer into an asynchronous one later would break every caller.
///
/// Idempotent: a second call resolves with the existing environment rather than replacing it, so
/// commands registered in between survive (`ENVIRON03`). A failed initialization leaves the
/// singleton unset so a retry can succeed (`ENVIRON04`).
#[wasm_bindgen]
pub fn init() -> js_sys::Promise {
    console_error_panic_hook::set_once();
    match environment::init_global() {
        Ok(_) => js_sys::Promise::resolve(&JsValue::UNDEFINED),
        Err(e) => js_sys::Promise::reject(&error::liquers_error_to_js(e)),
    }
}

/// Whether [`init`] has completed.
#[wasm_bindgen(js_name = isInitialized)]
pub fn is_initialized() -> bool {
    environment::is_initialized()
}

/// Releases the global environment.
///
/// Idempotent (`ENVIRON06`) — shutting down an uninitialized environment is not an error.
#[wasm_bindgen]
pub fn shutdown() {
    environment::reset_global();
}

/// Retains a JavaScript value opaquely — the explicit opt-in of Phase 1 decision 2.
///
/// Structural conversion is the default everywhere else; this is the only way a value is carried
/// by identity, so opacity is never accidental.
#[wasm_bindgen]
pub fn opaque(value: JsValue) -> Result<LiquersValue, JsValue> {
    bridge::opaque_value::<liquers_lib::value::Value>(value)
        .map(LiquersValue::from_value)
        .map_err(error::liquers_error_to_js)
}

/// A Liquers value, visible to JavaScript.
#[wasm_bindgen(js_name = Value)]
pub struct LiquersValue {
    inner: liquers_lib::value::Value,
}

#[wasm_bindgen(js_class = Value)]
impl LiquersValue {
    /// Converts back to a JavaScript value. An opaque value returns the original object.
    #[wasm_bindgen(js_name = toJS)]
    pub fn to_js(&self) -> Result<JsValue, JsValue> {
        bridge::value_to_js(&self.inner).map_err(error::liquers_error_to_js)
    }

    #[wasm_bindgen(getter, js_name = typeName)]
    pub fn type_name(&self) -> String {
        use liquers_core::value::ValueInterface;
        self.inner.type_name().to_string()
    }

    #[wasm_bindgen(getter)]
    pub fn identifier(&self) -> String {
        use liquers_core::value::ValueInterface;
        self.inner.identifier().to_string()
    }

    /// Whether this value holds a foreign-language value by identity rather than structurally.
    #[wasm_bindgen(getter, js_name = isOpaque)]
    pub fn is_opaque(&self) -> bool {
        use crate::bridge::JsValueBridge;
        matches!(self.inner.as_js_opaque(), Ok(Some(_)))
    }
}

impl LiquersValue {
    pub fn from_value(inner: liquers_lib::value::Value) -> Self {
        LiquersValue { inner }
    }

    pub fn inner(&self) -> &liquers_lib::value::Value {
        &self.inner
    }
}

// The command and evaluation surface is added by milestone M4 of
// `specs/liquers-web/phase4-implementation.md`.
