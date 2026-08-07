//! Registering a JavaScript callable as a Liquers command.
//!
//! There is no `CommandExecutor` implementation here. `CommandRegistry` already accepts a
//! non-`Send` `'static` closure on `wasm32`, so a JavaScript command is an ordinary registered
//! async command whose closure owns a `js_sys::Function`.
//!
//! **No `RefCell` borrow and no manager guard may be held across the call into JavaScript.**
//! Arguments are converted to owned Rust values first, then JavaScript is entered.

use std::rc::Rc;

use liquers_core::commands::{CommandArguments, CommandRegistry};
use liquers_core::context::{Context, Environment};
use liquers_core::error::{Error, ErrorType};
use liquers_core::state::State;
use liquers_core::value::ValueInterface;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

use crate::bridge::{js_to_value, value_to_js, ConversionPolicy, JsValueBridge};
use crate::command::spec::{IsAsync, JsCommandSpec, StateMode};
use crate::error::js_error_to_liquers;

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
        ..
    } = spec;

    let shared = Rc::new(CallableSpec {
        run,
        state_mode,
        is_async,
        name: key.name.clone(),
    });

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
pub struct CallableSpec {
    pub run: js_sys::Function,
    pub state_mode: StateMode,
    pub is_async: IsAsync,
    pub name: String,
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
            // Until a State wrapper is exported, the state is presented as its value. Stated
            // rather than silently substituted: a command declaring `state: "state"` gets the
            // value, not metadata access.
            js_args.push(&value_to_js(state.data_unchecked().as_ref())?);
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
