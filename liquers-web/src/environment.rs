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

    /// Every command declaration registered so far, in order.
    ///
    /// Retained so the environment can be rebuilt: registering after the environment has been
    /// shared replays these into a fresh one. Holding the original declaration objects rather than
    /// the parsed specs keeps replay identical to first registration — one code path, so the two
    /// cannot drift.
    static REGISTERED_SPECS: RefCell<Vec<JsValue>> = const { RefCell::new(Vec::new()) };
}

/// Builds a fresh, un-shared environment with the built-in Rust command set registered.
///
/// Kept separate from `to_ref` so that JavaScript commands can be registered into it before it is
/// shared — the ordinary Rust ordering, and the one that avoids a rebuild.
pub fn new_environment() -> Result<WebEnvironment, Error> {
    let mut env = WebEnvironment::new();
    crate::builtins::register_builtin_commands(&mut env)?;
    Ok(env)
}

/// Builds a fresh environment and shares it immediately.
pub fn build_environment() -> Result<EnvRef<WebEnvironment>, Error> {
    Ok(new_environment()?.to_ref())
}

/// Registers a JavaScript command declaration on the global environment.
///
/// Registration needs `&mut CommandRegistry`, and `Environment::to_ref` consumes the environment
/// into an `Arc`, so once it has been shared there is no mutable path back to it. Rather than
/// refuse, this follows the same shape Rust code uses — build the registry, then the environment,
/// then share it — and simply does it again:
///
/// - **Before the first evaluation** the environment is still un-shared, so the command is
///   registered directly. This is the ordinary path and costs nothing.
/// - **After the first evaluation** a fresh environment is built, every declaration so far is
///   replayed into it along with the new one, and the singleton is swapped.
///
/// The cost of a rebuild is the asset cache, which is discarded with the old environment. An
/// evaluation already in flight keeps the old `EnvRef` and completes against it, so nothing is
/// interrupted — it simply does not see the new command. Registering all commands before the first
/// evaluation avoids the rebuild entirely.
pub fn register_command_on(spec: &JsValue) -> Result<(), Error> {
    // Parse first, so an invalid declaration fails before anything is mutated or rebuilt.
    let parsed = crate::command::JsCommandSpec::parse(spec)?;
    let key = parsed.key.clone();

    let replaced = existing_command_kind(&key);

    let registered_directly = PENDING_ENV.with(|cell| -> Result<bool, Error> {
        let mut guard = cell.borrow_mut();
        match guard.as_mut() {
            Some(env) => {
                crate::command::register_js_command(&mut env.command_registry, parsed)?;
                Ok(true)
            }
            None => Ok(false),
        }
    })?;

    if !registered_directly {
        if !GLOBAL_ENV.with(|g| g.borrow().is_some()) {
            return Err(Error::from_error(
                ErrorType::NotAvailable,
                "Liquers is not initialized — await liquers.init() first".to_string(),
            ));
        }
        rebuild_with(spec.clone())?;
    }

    REGISTERED_SPECS.with(|cell| cell.borrow_mut().push(spec.clone()));
    warn_on_replacement(&key.name, replaced);
    Ok(())
}

/// What was registered under a key before, if anything.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Replaced {
    Nothing,
    JavaScriptCommand,
    RustCommand,
}

fn existing_command_kind(key: &liquers_core::command_metadata::CommandKey) -> Replaced {
    let lookup = |registry: &liquers_core::command_metadata::CommandMetadataRegistry| {
        match registry.get(key.clone()) {
            Some(m) if m.module == "javascript" => Replaced::JavaScriptCommand,
            Some(_) => Replaced::RustCommand,
            None => Replaced::Nothing,
        }
    };
    let pending = PENDING_ENV.with(|cell| {
        cell.borrow()
            .as_ref()
            .map(|env| lookup(&env.command_registry.command_metadata_registry))
    });
    if let Some(kind) = pending {
        return kind;
    }
    GLOBAL_ENV
        .with(|cell| {
            cell.borrow()
                .as_ref()
                .map(|envref| lookup(envref.0.get_command_metadata_registry()))
        })
        .unwrap_or(Replaced::Nothing)
}

/// Every replacement warns. Replacement and an accidental name collision look identical at the
/// point they happen, and a shadowed built-in stays invisible until a query returns the wrong
/// thing.
fn warn_on_replacement(name: &str, replaced: Replaced) {
    let message = match replaced {
        Replaced::Nothing => return,
        Replaced::JavaScriptCommand => format!("liquers: command {name:?} was replaced"),
        Replaced::RustCommand => {
            format!("liquers: command {name:?} replaces a built-in Rust command")
        }
    };
    web_sys::console::warn_1(&JsValue::from_str(&message));
}

/// Rebuilds the shared environment from every retained declaration, plus `additional`.
///
/// Used when a command is registered after the environment has been shared.
fn rebuild_with(additional: JsValue) -> Result<(), Error> {
    let mut env = new_environment()?;

    let specs = REGISTERED_SPECS.with(|cell| cell.borrow().clone());
    for spec in specs.iter().chain(std::iter::once(&additional)) {
        // Replay through the same path as first registration, so the two cannot drift.
        let parsed = crate::command::JsCommandSpec::parse(spec)?;
        crate::command::register_js_command(&mut env.command_registry, parsed)?;
    }

    let envref = env.to_ref();
    GLOBAL_ENV.with(|cell| *cell.borrow_mut() = Some(envref));
    Ok(())
}

/// Removes a command from the global environment.
///
/// Returns `false` when no such command was registered, rather than throwing, so teardown paths
/// need no existence check. Like registration, this rebuilds the environment when it has already
/// been shared.
pub fn unregister_command_on(name: &str) -> Result<bool, Error> {
    let key = liquers_core::command_metadata::CommandKey::new("", "", name);

    // Drop every retained declaration for this command, so a later rebuild does not resurrect it.
    let removed_spec = REGISTERED_SPECS.with(|cell| {
        let mut specs = cell.borrow_mut();
        let before = specs.len();
        specs.retain(|s| spec_name(s).as_deref() != Some(name));
        before != specs.len()
    });

    crate::command::adapter::forget_inference_record(&key);

    let removed_here = PENDING_ENV.with(|cell| {
        cell.borrow_mut()
            .as_mut()
            .map(|env| env.command_registry.unregister(key.clone()))
    });

    match removed_here {
        // Still un-shared: the direct removal is authoritative.
        Some(removed) => Ok(removed || removed_spec),
        // Already shared: rebuild without it.
        None => {
            if !GLOBAL_ENV.with(|g| g.borrow().is_some()) {
                return Ok(false);
            }
            if removed_spec {
                rebuild_without()?;
            }
            Ok(removed_spec)
        }
    }
}

/// Rebuilds the shared environment from the retained declarations as they now stand.
fn rebuild_without() -> Result<(), Error> {
    let mut env = new_environment()?;
    let specs = REGISTERED_SPECS.with(|cell| cell.borrow().clone());
    for spec in specs.iter() {
        let parsed = crate::command::JsCommandSpec::parse(spec)?;
        crate::command::register_js_command(&mut env.command_registry, parsed)?;
    }
    let envref = env.to_ref();
    GLOBAL_ENV.with(|cell| *cell.borrow_mut() = Some(envref));
    Ok(())
}

/// The `name` field of a declaration object, for matching retained specs.
fn spec_name(spec: &JsValue) -> Option<String> {
    js_sys::Reflect::get(spec, &JsValue::from_str("name"))
        .ok()
        .and_then(|v| v.as_string())
}

/// Returns a command's registered metadata as JSON, or `null` when it is not registered.
pub fn describe_command_on(name: &str) -> Result<JsValue, Error> {
    let key = liquers_core::command_metadata::CommandKey::new("", "", name);
    let json = PENDING_ENV
        .with(|cell| {
            cell.borrow().as_ref().and_then(|env| {
                env.command_registry
                    .command_metadata_registry
                    .get(key.clone())
                    .cloned()
            })
        })
        .or_else(|| {
            GLOBAL_ENV.with(|cell| {
                cell.borrow().as_ref().and_then(|envref| {
                    envref
                        .0
                        .get_command_metadata_registry()
                        .get(key.clone())
                        .cloned()
                })
            })
        });
    let meta = match json {
        Some(meta) => meta,
        None => return Ok(JsValue::NULL),
    };
    let described = crate::bridge::serialize_to_js(&meta).map_err(|e| {
        Error::from_error(
            ErrorType::ConversionError,
            format!("Could not convert command metadata: {e}"),
        )
    })?;

    // `CommandMetadata` skips empty collections when it serializes, which is right for a config
    // file and wrong for an API: a caller writing `describeCommand(n).arguments.map(...)` would
    // work for every command except the zero-argument ones. Normalize the shape so the field is
    // always an array.
    if js_sys::Reflect::get(&described, &"arguments".into())
        .map(|v| v.is_undefined())
        .unwrap_or(true)
    {
        set_field(&described, "arguments", &js_sys::Array::new().into())?;
    }

    // Whether the argument names were guessed from the function source. Inference is never
    // invisible — see `crate::command::adapter::arguments_were_inferred`.
    set_field(
        &described,
        "argumentsInferred",
        &JsValue::from_bool(crate::command::adapter::arguments_were_inferred(&key)),
    )?;

    Ok(described)
}

fn set_field(target: &JsValue, name: &str, value: &JsValue) -> Result<(), Error> {
    js_sys::Reflect::set(target, &JsValue::from_str(name), value)
        .map(|_| ())
        .map_err(|_| {
            Error::from_error(
                ErrorType::ConversionError,
                format!("Could not set {name:?} on the command description"),
            )
        })
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
    // Built before the borrow is taken: `new_environment` can fail, and a `?` inside the closure
    // would abandon the borrow half-written.
    let env = new_environment()?;
    PENDING_ENV.with(|cell| {
        *cell.borrow_mut() = Some(env);
    });
    Ok(())
}

/// Whether a command is registered on the pending environment. Test support.
pub fn has_command(name: &str) -> bool {
    existing_command_kind(&liquers_core::command_metadata::CommandKey::new("", "", name))
        != Replaced::Nothing
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
    REGISTERED_SPECS.with(|cell| cell.borrow_mut().clear());
    crate::command::adapter::clear_inference_records();
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
