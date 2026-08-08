//! Browser/JavaScript integration of Liquers, compiled to WebAssembly.
//!
//! This crate is the `wasm32` half of the language integration described in
//! `specs/guides/LANGUAGE-INTEGRATION_GUIDE.md`. A page constructs an environment, evaluates queries as
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

pub mod asset;
pub mod bridge;
pub mod builtins;
pub mod encode;
pub mod error;
pub mod environment;
pub mod eval;
pub mod objects;
pub mod typescript;
pub mod command;
pub mod default_value;
pub mod value;

pub use asset::{LiquersAsset, LiquersState};
pub use bridge::{ConversionPolicy, JsValueBridge};
pub use encode::encode_param;
pub use environment::{version, LiquersEnvironment, WebEnvironment};
pub use error::LiquersError;
pub use objects::{LiquersKey, LiquersQuery};
pub use value::{JsOpaque, ORIGIN_JAVASCRIPT};

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

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

    /// Marks this object as a Liquers value wrapper.
    ///
    /// Read by the conversion layer when a command *returns* a `Value` — most importantly the
    /// result of `liquers.opaque(x)`. A wrapper is a class instance, which structural conversion
    /// would otherwise reject, so without this marker the opaque opt-in could not survive being
    /// returned from a command. A downstream crate's own wrapper opts in the same way; see
    /// [`bridge::VALUE_WRAPPER_MARKER`].
    #[wasm_bindgen(getter, js_name = __liquersValueWrapper)]
    pub fn is_value_wrapper(&self) -> bool {
        true
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

/// Evaluates a query on the global environment, returning a `Promise`.
///
/// There is deliberately no synchronous counterpart — see [`eval`].
#[wasm_bindgen]
pub fn evaluate(query: &str) -> js_sys::Promise {
    let envref = match environment::shared_env() {
        Ok(e) => e,
        Err(e) => return js_sys::Promise::reject(&error::liquers_error_to_js(e)),
    };
    let parsed = match liquers_core::parse::parse_query(query) {
        Ok(q) => q,
        Err(e) => return js_sys::Promise::reject(&error::liquers_error_to_js(e)),
    };
    eval::evaluate_to_promise(envref, parsed)
}

/// Resolves a query to an [`LiquersAsset`] without waiting for it to become ready.
///
/// This is the low-level counterpart of [`evaluate`]: it gives access to status, metadata, logs
/// and — the reason it exists — cancellation, which a `Promise` cannot express.
#[wasm_bindgen(js_name = getAsset)]
pub fn get_asset(query: &str) -> typescript::AssetPromise {
    let reject = |e| js_sys::Promise::reject(&error::liquers_error_to_js(e)).unchecked_into();
    let envref = match environment::shared_env() {
        Ok(e) => e,
        Err(e) => return reject(e),
    };
    let parsed = match liquers_core::parse::parse_query(query) {
        Ok(q) => q,
        Err(e) => return reject(e),
    };
    wasm_bindgen_futures::future_to_promise(async move {
        match asset::get_asset_for(envref, parsed).await {
            Ok(a) => Ok(LiquersAsset::from_ref(a).into()),
            Err(e) => Err(error::liquers_error_to_js(e)),
        }
    })
    .unchecked_into()
}

/// Encodes a string for use as a query parameter.
///
/// Throws a `LiquersError` when the value contains a character the query grammar cannot represent
/// — a lone colon, most punctuation, or any non-ASCII character. **Throwing is deliberate**: the
/// core encoder passes those through unchanged and produces text that fails to parse later,
/// somewhere else. See [`encode`] for the full limitation and the issue tracking its removal.
///
/// ```javascript
/// const q = `data/filter-${liquers.encodeParam(userInput)}`;
/// ```
#[wasm_bindgen(js_name = encodeParam)]
pub fn encode_param_js(text: &str) -> Result<String, JsValue> {
    encode::encode_param(text).map_err(error::liquers_error_to_js)
}

/// Registers a JavaScript command on the global environment.
///
/// Replacing an existing command is permitted and **always warns** — replacement and an accidental
/// name collision are indistinguishable at the point they happen, and a shadowed built-in stays
/// invisible until a query quietly returns the wrong thing.
#[wasm_bindgen(js_name = registerCommand)]
pub fn register_command(spec: typescript::JsCommandSpecObject) -> Result<(), JsValue> {
    environment::register_command_on(spec.as_ref()).map_err(error::liquers_error_to_js)
}

/// Removes a command from the global environment.
///
/// Returns `false` for an unknown command rather than throwing, so teardown paths need no
/// existence check.
#[wasm_bindgen(js_name = unregisterCommand)]
pub fn unregister_command(name: &str) -> Result<bool, JsValue> {
    environment::unregister_command_on(name).map_err(error::liquers_error_to_js)
}

/// Returns the registered metadata for a command, or `null` when it is not registered.
#[wasm_bindgen(js_name = describeCommand)]
pub fn describe_command(name: &str) -> Result<typescript::JsCommandInfo, JsValue> {
    environment::describe_command_on(name)
        .map(JsCast::unchecked_into)
        .map_err(error::liquers_error_to_js)
}
