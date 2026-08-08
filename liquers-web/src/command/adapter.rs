//! Registering a JavaScript callable as a Liquers command.
//!
//! There is no `CommandExecutor` implementation here. `CommandRegistry` already accepts a
//! non-`Send` `'static` closure on `wasm32`, so a JavaScript command is an ordinary registered
//! async command whose closure owns a `js_sys::Function`.
//!
//! **No `RefCell` borrow and no manager guard may be held across the call into JavaScript.**
//! Arguments are converted to owned Rust values first, then JavaScript is entered.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use liquers_core::command_metadata::CommandKey;
use liquers_core::commands::{CommandArguments, CommandRegistry};
use liquers_core::context::{Context, Environment};
use liquers_core::error::{Error, ErrorType};
use liquers_core::state::State;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

use crate::bridge::{js_to_value, value_to_js, ConversionPolicy, JsValueBridge};
use crate::command::spec::{IsAsync, JsCommandSpec, StateMode};
use crate::error::js_error_to_liquers;

thread_local! {
    /// Keys whose argument list was inferred from the function source rather than declared.
    ///
    /// `CommandMetadata` has no field for this, and inventing one in `liquers-core` for a
    /// JavaScript-only concern would be wrong — so it is kept beside the registry and reported by
    /// `describeCommand` as `argumentsInferred`. Inference must never be invisible: a minified
    /// build yields correct arity and wrong names, and the only way an author notices is by being
    /// told which names were guessed.
    ///
    /// Registration is the single writer, so a rebuild — which replays through the same path —
    /// reproduces this map exactly.
    static INFERRED_ARGUMENTS: RefCell<HashSet<CommandKey>> = RefCell::new(HashSet::new());
}

/// How many JavaScript function handles the registrations currently hold.
///
/// Test-only, behind `debug-handles`. `RUNTIME05` claims that unregistering a command releases the
/// `js_sys::Function` and everything its closure captured. JavaScript cannot observe a Rust drop,
/// and `WeakRef`/`FinalizationRegistry` depend on GC timing, so a naive test is either flaky or
/// vacuous. This counts the thing that actually matters — the number of live `CallableSpec`
/// allocations, each of which owns exactly one `Function` — and does it deterministically.
#[cfg(feature = "debug-handles")]
pub fn live_handle_count() -> usize {
    LIVE_HANDLES.with(|cell| cell.get())
}

#[cfg(feature = "debug-handles")]
thread_local! {
    static LIVE_HANDLES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Whether a command's argument list was inferred rather than declared.
pub fn arguments_were_inferred(key: &CommandKey) -> bool {
    INFERRED_ARGUMENTS.with(|cell| cell.borrow().contains(key))
}

/// Forgets one command's inference record. Called when a command is unregistered.
pub fn forget_inference_record(key: &CommandKey) {
    INFERRED_ARGUMENTS.with(|cell| {
        cell.borrow_mut().remove(key);
    });
}

/// Forgets every inference record. Called by environment teardown.
pub fn clear_inference_records() {
    INFERRED_ARGUMENTS.with(|cell| cell.borrow_mut().clear());
}

/// Registers a parsed declaration into a command registry.
///
/// Registered on the **async** path, because a JavaScript `run` may return a Promise and only the
/// async executor can await one. A sync command is additionally registered on the sync path so
/// that `CommandExecutor::execute` can serve it without going through a future.
pub fn register_js_command<E>(
    registry: &mut CommandRegistry<E>,
    spec: JsCommandSpec,
) -> Result<(), Error>
where
    E: Environment,
    E::Value: JsValueBridge,
{
    let JsCommandSpec {
        key,
        metadata,
        state_mode,
        is_async,
        run,
        arguments_inferred,
    } = spec;

    INFERRED_ARGUMENTS.with(|cell| {
        let mut set = cell.borrow_mut();
        if arguments_inferred {
            set.insert(key.clone());
        } else {
            // A re-registration that declares its arguments must clear an earlier inference.
            set.remove(&key);
        }
    });

    let shared = Rc::new(CallableSpec::new(run, state_mode, is_async, key.name.clone()));

    let for_async = shared.clone();
    registry.register_async_command(key.clone(), move |state, args, context| {
        let callable = for_async.clone();
        Box::pin(async move { call_js_command(callable, state, args, context).await })
    })?;

    // Replace the metadata built by `register_async_command` (which is a bare key-derived stub)
    // with the parsed declaration, so `describeCommand` reports what the author actually wrote.
    registry.command_metadata_registry.add_command(&metadata);
    Ok(())
}

/// The retained callable and how to call it.
///
/// Held by `Rc` inside the registered closure, so it — and the `js_sys::Function` it owns — is
/// released when the registry drops that closure. That is the mechanism `unregister` relies on,
/// and what `live_handle_count` observes.
pub struct CallableSpec {
    pub run: js_sys::Function,
    pub state_mode: StateMode,
    pub is_async: IsAsync,
    pub name: String,
}

impl CallableSpec {
    fn new(run: js_sys::Function, state_mode: StateMode, is_async: IsAsync, name: String) -> Self {
        #[cfg(feature = "debug-handles")]
        LIVE_HANDLES.with(|cell| cell.set(cell.get() + 1));
        CallableSpec {
            run,
            state_mode,
            is_async,
            name,
        }
    }
}

#[cfg(feature = "debug-handles")]
impl Drop for CallableSpec {
    fn drop(&mut self) {
        LIVE_HANDLES.with(|cell| cell.set(cell.get().saturating_sub(1)));
    }
}

/// Invokes a JavaScript command.
///
/// Ordering matters: every argument is converted to an owned JavaScript value *before* the call,
/// and nothing borrowed from the environment is held while JavaScript runs.
pub async fn call_js_command<E>(
    callable: Rc<CallableSpec>,
    state: State<E::Value>,
    args: CommandArguments<E>,
    _context: Context<E>,
) -> Result<E::Value, Error>
where
    E: Environment,
    E::Value: JsValueBridge,
{
    let js_args = js_sys::Array::new();

    // The state argument, when the command takes one.
    match callable.state_mode {
        StateMode::None => {}
        StateMode::Value => {
            js_args.push(&value_to_js(state.data_unchecked().as_ref())?);
        }
        StateMode::Text => {
            let text = state.try_into_string()?;
            js_args.push(&JsValue::from_str(&text));
        }
        StateMode::State => {
            // A plain object rather than the exported `State` class: this function is generic over
            // the value type, and the class is concrete, so a downstream crate with its own value
            // type could not receive one. The object carries what the mode promises — the value,
            // the metadata, the status and the log.
            let obj = js_sys::Object::new();
            let set = |k: &str, v: &JsValue| -> Result<(), Error> {
                js_sys::Reflect::set(&obj, &JsValue::from_str(k), v)
                    .map(|_| ())
                    .map_err(|_| {
                        Error::from_error(
                            ErrorType::ConversionError,
                            format!("Could not set {k:?} on the state argument"),
                        )
                    })
            };
            set("value", &value_to_js(state.data_unchecked().as_ref())?)?;
            set("metadata", &crate::asset::metadata_to_js(state.metadata.as_ref())?)?;
            set(
                "status",
                &JsValue::from_str(crate::asset::status_name(state.metadata.status())),
            )?;
            set("log", &crate::asset::log_to_js(state.metadata.as_ref())?)?;
            js_args.push(&obj.into());
        }
    }

    // Then the declared parameters, resolved by the planner before they reach here.
    for i in 0..args.len() {
        let value = args.get_value(i, "argument")?;
        js_args.push(&value_to_js(&value)?);
    }

    let result = callable
        .run
        .apply(&JsValue::NULL, &js_args)
        .map_err(|e| js_error_to_liquers(e, ErrorType::ExecutionError))?;

    let resolved = match (callable.is_async, is_thenable(&result)) {
        // Declared sync but returned a Promise: refuse rather than silently return an
        // un-awaited Promise as the command's value, which would surface much later as a
        // baffling opaque value.
        (IsAsync::Sync, true) => {
            return Err(Error::from_error(
                ErrorType::ExecutionError,
                format!(
                    "Command {:?} is declared synchronous but returned a Promise. Declare it \
                     `async: true`, or return a value.",
                    callable.name
                ),
            ))
        }
        (IsAsync::Async, false) => result,
        (IsAsync::Async, true) | (IsAsync::Auto, true) => {
            let promise: js_sys::Promise = result.unchecked_into();
            JsFuture::from(promise)
                .await
                .map_err(|e| js_error_to_liquers(e, ErrorType::ExecutionError))?
        }
        (IsAsync::Sync, false) | (IsAsync::Auto, false) => result,
    };

    // A command may return an opaque value it created with `liquers.opaque(...)`; structural
    // conversion is still the default for everything else.
    js_to_value::<E::Value>(&resolved, ConversionPolicy::Structural)
}

/// Whether a value is a Promise (or any thenable).
fn is_thenable(v: &JsValue) -> bool {
    if !v.is_object() {
        return false;
    }
    js_sys::Reflect::get(v, &JsValue::from_str("then"))
        .map(|then| then.is_function())
        .unwrap_or(false)
}

use wasm_bindgen::JsCast;
