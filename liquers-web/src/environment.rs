//! `ENVIRON` — the environment, its global singleton, and explicit instances.
//!
//! # The borrow rule
//!
//! The singleton lives in a `thread_local! { RefCell<...> }`. **No `RefCell` borrow may be held
//! across an `await` or across a call into JavaScript.** Every accessor here clones the `EnvRef`
//! out and drops the borrow before returning, which is why [`with_global`] returns a clone rather
//! than lending a reference.
//!
//! This is the single most important invariant in the crate. wasm is single-threaded, so a borrow
//! held across a yield is not a data race — it is an `already borrowed` panic, or a deadlock if the
//! held borrow is what the resumed task needs. `RefCell` rather than a lock is a deliberate choice:
//! it makes the mistake loud instead of silent.
//!
//! # Why there is no new `Environment` implementation
//!
//! `liquers_lib::environment::DefaultEnvironment` is generic over the value type and already
//! selects the inline asset manager on `wasm32`, so it is used directly. See
//! `specs/liquers-web/phase2-architecture.md`.

use std::cell::RefCell;

use liquers_core::context::{EnvRef, Environment};
use liquers_core::error::{Error, ErrorType};
use liquers_lib::environment::DefaultEnvironment;
use liquers_lib::value::Value;
use wasm_bindgen::prelude::*;

use crate::error::liquers_error_to_js;

/// The concrete environment. A type alias, not a newtype, so a downstream crate can substitute its
/// own `DefaultEnvironment<MyValue, P>` and reuse every generic function in this crate.
pub type WebEnvironment = DefaultEnvironment<Value, ()>;

thread_local! {
    /// The global singleton, unset until `init()` resolves.
    ///
    /// Left unset when initialization fails, so a retry can succeed (`ENVIRON04`).
    static GLOBAL_ENV: RefCell<Option<EnvRef<WebEnvironment>>> = const { RefCell::new(None) };
}

/// Builds a fresh environment with the standard command set registered.
pub fn build_environment() -> Result<EnvRef<WebEnvironment>, Error> {
    let env = WebEnvironment::new();
    Ok(env.to_ref())
}

/// Returns a clone of the global `EnvRef`, or an error when `init()` has not run.
///
/// Returns a **clone**, deliberately: it drops the `RefCell` borrow before the caller can do
/// anything that might re-enter. Handing out a reference would make the borrow rule impossible to
/// keep.
pub fn with_global() -> Result<EnvRef<WebEnvironment>, Error> {
    GLOBAL_ENV.with(|cell| {
        cell.borrow()
            .as_ref()
            .cloned()
            .ok_or_else(|| {
                Error::from_error(
                    ErrorType::NotAvailable,
                    "Liquers is not initialized — await liquers.init() before evaluating"
                        .to_string(),
                )
            })
    })
}

/// Whether the singleton has been initialized.
pub fn is_initialized() -> bool {
    GLOBAL_ENV.with(|cell| cell.borrow().is_some())
}

/// Initializes the singleton if it is not already initialized.
///
/// Idempotent (`ENVIRON03`): a second call returns the existing environment rather than replacing
/// it, so commands registered before it are not silently discarded.
pub fn init_global() -> Result<EnvRef<WebEnvironment>, Error> {
    if let Ok(existing) = with_global() {
        return Ok(existing);
    }
    let envref = build_environment()?;
    GLOBAL_ENV.with(|cell| {
        *cell.borrow_mut() = Some(envref.clone());
    });
    Ok(envref)
}

/// Clears the singleton. Test support, and the shutdown path.
///
/// Idempotent (`ENVIRON06`): clearing an unset singleton is not an error.
pub fn reset_global() {
    GLOBAL_ENV.with(|cell| {
        *cell.borrow_mut() = None;
    });
}

/// A Liquers environment, visible to JavaScript.
///
/// Two lifecycles are supported (Phase 1 decision 4). The **singleton** is the documented default
/// and is reached through the module-level functions; an **explicit instance** is created with
/// `new Environment()` and shares no state with the singleton, which is what makes test isolation
/// possible (`ENVIRON05`).
#[wasm_bindgen(js_name = Environment)]
pub struct LiquersEnvironment {
    envref: EnvRef<WebEnvironment>,
}

#[wasm_bindgen(js_class = Environment)]
impl LiquersEnvironment {
    /// Creates an isolated environment. Registers nothing with, and shares nothing with, the
    /// singleton.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Result<LiquersEnvironment, JsValue> {
        build_environment()
            .map(|envref| LiquersEnvironment { envref })
            .map_err(liquers_error_to_js)
    }

    /// A handle to the global singleton. Throws when `init()` has not resolved.
    #[wasm_bindgen(js_name = global)]
    pub fn global() -> Result<LiquersEnvironment, JsValue> {
        with_global()
            .map(|envref| LiquersEnvironment { envref })
            .map_err(liquers_error_to_js)
    }
}

impl LiquersEnvironment {
    pub fn envref(&self) -> &EnvRef<WebEnvironment> {
        &self.envref
    }

    pub fn from_envref(envref: EnvRef<WebEnvironment>) -> Self {
        LiquersEnvironment { envref }
    }
}

/// The `liquers-web` crate version, and the `liquers-core` version it was built against.
///
/// Sourced from Cargo rather than hand-maintained, so `PACKAGE04` compares against something that
/// cannot drift.
#[wasm_bindgen]
pub fn version() -> String {
    format!(
        "liquers-web {} (liquers-core {})",
        env!("CARGO_PKG_VERSION"),
        liquers_core::VERSION,
    )
}
