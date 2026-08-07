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
    /// The environment while it is still mutable — before it has been shared.
    ///
    /// Command registration needs `&mut CommandRegistry`, and `Environment::to_ref` *consumes*
    /// the environment into an `Arc`, after which no mutable path exists (and
    /// `get_command_executor` returns a reference, so the executor cannot live behind a
    /// `RefCell` either). So the environment is held here, mutable, until the first evaluation
    /// shares it — see `PENDING_ENV` / `GLOBAL_ENV` and the limitation documented on
    /// [`register_command_on`].
    static PENDING_ENV: RefCell<Option<WebEnvironment>> = const { RefCell::new(None) };

    /// The shared environment, created by the first evaluation.
    ///
    /// Left unset when initialization fails, so a retry can succeed (`ENVIRON04`).
    static GLOBAL_ENV: RefCell<Option<EnvRef<WebEnvironment>>> = const { RefCell::new(None) };
}

/// Builds a fresh environment with the standard command set registered.
pub fn build_environment() -> Result<EnvRef<WebEnvironment>, Error> {
    let env = WebEnvironment::new();
    Ok(env.to_ref())
}

/// Registers a JavaScript command declaration on the pending global environment.
///
/// **Limitation, and it is a real one:** commands can only be registered *before* the first
/// evaluation. `Environment::to_ref` consumes the environment into an `Arc` and
/// `Environment::get_command_executor` hands out a reference, so once the environment is shared
/// there is no path to `&mut CommandRegistry`. Registering afterwards returns a typed error rather
/// than silently doing nothing.
///
/// This blocks the per-route registration pattern that motivated `unregister`, and the fix belongs
/// in `liquers-core` — see `POST-INIT-COMMAND-REGISTRATION` in `specs/ISSUES.md`.
pub fn register_command_on(spec: &JsValue) -> Result<(), Error> {
    let parsed = crate::command::JsCommandSpec::parse(spec)?;
    let key = parsed.key.clone();

    PENDING_ENV.with(|cell| {
        let mut guard = cell.borrow_mut();
        let env = guard.as_mut().ok_or_else(|| {
            if GLOBAL_ENV.with(|g| g.borrow().is_some()) {
                Error::from_error(
                    ErrorType::NotSupported,
                    format!(
                        "Cannot register command {:?}: the environment has already been shared by an \
                         evaluation, and Liquers does not yet support registering commands \
                         afterwards. Register commands before the first evaluate().",
                        key.name
                    ),
                )
            } else {
                Error::from_error(
                    ErrorType::NotAvailable,
                    "Liquers is not initialized — await liquers.init() first".to_string(),
                )
            }
        })?;

        let existed = env
            .command_registry
            .command_metadata_registry
            .get(key.clone())
            .is_some();
        let was_javascript = env
            .command_registry
            .command_metadata_registry
            .get(key.clone())
            .map(|m| m.module == "javascript")
            .unwrap_or(false);

        crate::command::register_js_command(&mut env.command_registry, parsed)?;

        // Every replacement warns: replacement and an accidental collision look identical at the
        // point they happen, and a shadowed built-in stays invisible until a query returns the
        // wrong thing.
        if existed {
            let message = if was_javascript {
                format!("liquers: command {:?} was replaced", key.name)
            } else {
                format!(
                    "liquers: command {:?} replaces a built-in Rust command",
                    key.name
                )
            };
            web_sys::console::warn_1(&JsValue::from_str(&message));
        }
        Ok(())
    })
}

/// Removes a command from the pending global environment.
pub fn unregister_command_on(name: &str) -> Result<bool, Error> {
    let key = liquers_core::command_metadata::CommandKey::new("", "", name);
    PENDING_ENV.with(|cell| {
        let mut guard = cell.borrow_mut();
        let env = guard.as_mut().ok_or_else(|| {
            Error::from_error(
                ErrorType::NotSupported,
                "Cannot unregister after the environment has been shared by an evaluation"
                    .to_string(),
            )
        })?;
        Ok(env.command_registry.unregister(key))
    })
}

/// Returns a command's registered metadata as JSON, or `null` when it is not registered.
pub fn describe_command_on(name: &str) -> Result<JsValue, Error> {
    let key = liquers_core::command_metadata::CommandKey::new("", "", name);
    let json = PENDING_ENV.with(|cell| {
        cell.borrow()
            .as_ref()
            .and_then(|env| {
                env.command_registry
                    .command_metadata_registry
                    .get(key.clone())
                    .cloned()
            })
    });
    match json {
        Some(meta) => serde_wasm_bindgen::to_value(&meta).map_err(|e| {
            Error::from_error(
                ErrorType::ConversionError,
                format!("Could not convert command metadata: {e}"),
            )
        }),
        None => Ok(JsValue::NULL),
    }
}

/// Shares the pending environment, creating the `EnvRef` on first use.
pub fn shared_env() -> Result<EnvRef<WebEnvironment>, Error> {
    if let Some(existing) = GLOBAL_ENV.with(|cell| cell.borrow().clone()) {
        return Ok(existing);
    }
    let env = PENDING_ENV
        .with(|cell| cell.borrow_mut().take())
        .ok_or_else(|| {
            Error::from_error(
                ErrorType::NotAvailable,
                "Liquers is not initialized — await liquers.init() before evaluating".to_string(),
            )
        })?;
    let envref = env.to_ref();
    GLOBAL_ENV.with(|cell| *cell.borrow_mut() = Some(envref.clone()));
    Ok(envref)
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
        || PENDING_ENV.with(|cell| cell.borrow().is_some())
}

/// Initializes the singleton if it is not already initialized.
///
/// Idempotent (`ENVIRON03`): a second call returns the existing environment rather than replacing
/// it, so commands registered before it are not silently discarded.
/// Returns `Ok(())` once the environment exists, whether it was created now or already present.
///
/// Deliberately does **not** return an `EnvRef`: the environment stays un-shared and mutable so
/// that commands can be registered, and the `EnvRef` is created by [`shared_env`] on the first
/// evaluation. Handing one out here would either share the environment too early — making
/// registration impossible — or hand back a throwaway that is not the one evaluation will use.
pub fn init_global() -> Result<(), Error> {
    if is_initialized() {
        return Ok(());
    }
    PENDING_ENV.with(|cell| {
        *cell.borrow_mut() = Some(WebEnvironment::new());
    });
    Ok(())
}

/// Whether a command is registered on the pending environment. Test support.
pub fn has_command(name: &str) -> bool {
    let key = liquers_core::command_metadata::CommandKey::new("", "", name);
    PENDING_ENV.with(|cell| {
        cell.borrow()
            .as_ref()
            .map(|env| {
                env.command_registry
                    .command_metadata_registry
                    .get(key.clone())
                    .is_some()
            })
            .unwrap_or(false)
    })
}

/// Clears the singleton. Test support, and the shutdown path.
///
/// Idempotent (`ENVIRON06`): clearing an unset singleton is not an error.
pub fn reset_global() {
    GLOBAL_ENV.with(|cell| {
        *cell.borrow_mut() = None;
    });
    PENDING_ENV.with(|cell| {
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
