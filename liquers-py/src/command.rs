//! Registering a Python callable as a Liquers command.
//!
//! A Python command is an ordinary registered async command whose closure owns the callable
//! (`liquers-web/src/command/adapter.rs` is the precedent). The closure moves the call onto a
//! blocking thread with `spawn_blocking` and takes the GIL there, so a tokio worker never blocks
//! on the GIL and a command that waits — `context.evaluate` — does not stall the runtime.
//!
//! **Nothing borrowed from the environment is held across the call into Python.** Arguments are
//! converted to Python objects first, then the callable runs.
//!
//! An `async def` callable (or an object whose `__call__` is one) runs differently: the call that
//! makes its coroutine still happens on a blocking thread, but the coroutine runs on one of the
//! environment's event loops (`aio.rs`), and the command's future waits for it without holding a
//! thread. Dropping that future — the asset was cancelled or replaced, or the environment shut
//! down — cancels the coroutine, which sees `asyncio.CancelledError` (AC-7).
//!
//! The metadata comes from the shared declaration pipeline: the decorator in
//! `python/liquers/_commands.py` supplies the baseline (stage 1) and the author's keyword
//! arguments, and [`PyCommandSpec::new`] runs stages 2–5 with `CommandDeclaration`.

use std::sync::{Arc, Mutex};

use liquers_core::command_declaration::{CommandDeclaration, StateDelivery, WarningKind};
use liquers_core::command_metadata::{CommandKey, CommandMetadata};
use liquers_core::commands::{CommandArguments, CommandRegistry};
use liquers_core::context::Context as CoreContext;
use liquers_core::error::Error;
use liquers_core::metadata::Version;
use liquers_core::plan::ParameterValue;
use liquers_core::state::State as CoreState;
use liquers_core::value::ValueInterface;
use pyo3::prelude::*;
use pyo3::types::{PyString, PyTuple};

use crate::aio::{lock_pending, shut_down_while_waiting, CancelOnDrop, LazyLoopPool, Pending, Shut};
use crate::pycall::{joined, schedule_coroutine, LiveHandle};

/// What a command's panic and cancel messages name.
const COMMAND: &str = "a Python command";
use crate::context::Context;
use crate::environment::PyEnvironment;
use crate::error::pyerr_to_error;
use crate::value::{py_to_value, value_to_py, ConversionMode, Value};

/// The `module` recorded on a Python command's metadata, which is how a replacement is told
/// apart from a built-in one.
pub const PYTHON_MODULE: &str = "python";

/// What a registration replaced, reported to the author as a warning (AC-10).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Replaced {
    Nothing,
    Builtin,
    Python,
}

impl Replaced {
    pub fn as_str(&self) -> &'static str {
        match self {
            Replaced::Nothing => "nothing",
            Replaced::Builtin => "built-in",
            Replaced::Python => "Python",
        }
    }
}

/// The retained callable, shared by every registration of one spec.
///
/// Held by `Arc` inside the registered closure, so it — and the Python object it owns — is
/// released when the last registry holding the closure is dropped.
pub struct PyCallable {
    pub object: Py<PyAny>,
    pub delivery: StateDelivery,
    /// Where `context` goes among the positional parameters, if the function takes it.
    pub context_index: Option<usize>,
    pub conversion: ConversionMode,
    pub pickle: bool,
    /// The registering environment's shut signal, handed to the `Context` of each call.
    pub shut: Arc<Shut>,
    /// For an `async def` callable, the environment's loop pool its coroutines run on.
    pub loops: Option<Arc<LazyLoopPool>>,
    _handle: LiveHandle,
}

impl PyCallable {
    fn new(
        object: Py<PyAny>,
        delivery: StateDelivery,
        context_index: Option<usize>,
        conversion: ConversionMode,
        pickle: bool,
        shut: Arc<Shut>,
        loops: Option<Arc<LazyLoopPool>>,
    ) -> Self {
        PyCallable {
            object,
            delivery,
            context_index,
            conversion,
            pickle,
            shut,
            loops,
            _handle: LiveHandle::new(),
        }
    }
}

/// A Python command declaration, retained so that every rebuild registers it again.
#[derive(Clone)]
pub struct PyCommandSpec {
    pub key: CommandKey,
    pub metadata: CommandMetadata,
    pub callable: Arc<PyCallable>,
}

impl PyCommandSpec {
    /// Runs declaration stages 2–5 over the decorator's baseline.
    ///
    /// Returns the spec and the pipeline's warnings, for the decorator to issue as Python
    /// warnings. `ReservedStateDelivery` is left out: a state parameter with any name delivering
    /// the value is the Python convention, not a mistake. `loops` is given for an `async def`
    /// callable, which is then marked async in its metadata.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        object: Py<PyAny>,
        baseline: serde_json::Value,
        declaration: &serde_json::Value,
        context_index: Option<usize>,
        implementation: Option<&[u8]>,
        conversion: ConversionMode,
        pickle: bool,
        shut: Arc<Shut>,
        loops: Option<Arc<LazyLoopPool>>,
    ) -> Result<(Self, Vec<String>), Error> {
        let mut command = CommandDeclaration::from_introspection(baseline);
        command.enhance(declaration)?;
        let mut metadata = command.finish()?;
        let warnings = command
            .warnings()
            .iter()
            .filter(|warning| match warning.kind {
                WarningKind::ReservedStateDelivery => false,
                WarningKind::ContextBeforeState | WarningKind::NoIntrospection => true,
            })
            .map(|warning| format!("command {:?}: {}", warning.command, warning.message))
            .collect();
        let delivery = command
            .registration()
            .get("state")
            .and_then(|mode| mode.as_str())
            .map(StateDelivery::from_argument_name)
            .unwrap_or(StateDelivery::None)
            .effective();
        metadata.module = PYTHON_MODULE.to_string();
        metadata.with_async(loops.is_some());
        // A changed implementation must not reuse results stored by the old one; a callable with
        // no code object gets a fresh version on every registration.
        metadata.impl_version = match implementation {
            Some(bytes) => Version::from_content(bytes),
            None => Version::new_unique(),
        };
        let spec = PyCommandSpec {
            key: metadata.key(),
            metadata,
            callable: Arc::new(PyCallable::new(
                object,
                delivery,
                context_index,
                conversion,
                pickle,
                shut,
                loops,
            )),
        };
        Ok((spec, warnings))
    }
}

/// Registers a spec, replacing whatever was registered under its key.
///
/// The existing command is unregistered first, which also discards its `impl_version`, so the
/// spec's own version is the one that counts.
pub fn register_py_command(
    registry: &mut CommandRegistry<PyEnvironment>,
    spec: &PyCommandSpec,
) -> Result<Replaced, Error> {
    let replaced = match registry.command_metadata_registry.get(spec.key.clone()) {
        None => Replaced::Nothing,
        Some(existing) if existing.module == PYTHON_MODULE => Replaced::Python,
        Some(_) => Replaced::Builtin,
    };
    registry.unregister(spec.key.clone());
    let callable = spec.callable.clone();
    registry.register_async_command(spec.key.clone(), move |state, arguments, context| {
        let callable = callable.clone();
        Box::pin(async move { call_py_command(callable, state, arguments, context).await })
    })?;
    // `register_async_command` stores a stub derived from the key; the declaration replaces it,
    // and `add_command` carries the stub's (zero) impl_version over, so the spec's is set after.
    registry.command_metadata_registry.add_command(&spec.metadata);
    if let Some(stored) = registry.command_metadata_registry.get_mut(spec.key.clone()) {
        stored.impl_version = spec.metadata.impl_version;
    }
    Ok(replaced)
}

/// Runs a Python command on a blocking thread, with the GIL.
async fn call_py_command(
    callable: Arc<PyCallable>,
    state: CoreState<Value>,
    arguments: CommandArguments<PyEnvironment>,
    context: CoreContext<PyEnvironment>,
) -> Result<Value, Error> {
    if let Some(loops) = callable.loops.clone() {
        return call_async_py_command(callable, loops, state, arguments, context).await;
    }
    joined(
        tokio::task::spawn_blocking(move || {
            let asset = context.get_asset_ref();
            crate::exit::command_call(asset, |py| {
                call_with_gil(py, &callable, &state, &arguments, context)
            })
        })
        .await,
        COMMAND,
    )
}

/// Runs an `async def` command: makes the coroutine on a blocking thread, runs it on a pool loop,
/// and waits for it without holding a thread.
async fn call_async_py_command(
    callable: Arc<PyCallable>,
    loops: Arc<LazyLoopPool>,
    state: CoreState<Value>,
    arguments: CommandArguments<PyEnvironment>,
    context: CoreContext<PyEnvironment>,
) -> Result<Value, Error> {
    let pending = Arc::new(Mutex::new(Pending::default()));
    // Made first: from here on, dropping this future cancels whatever has been started.
    let _guard = CancelOnDrop(pending.clone());
    let (done_tx, done_rx) = tokio::sync::oneshot::channel::<()>();
    let started = {
        let callable = callable.clone();
        let pending = pending.clone();
        tokio::task::spawn_blocking(move || {
            let asset = context.get_asset_ref();
            crate::exit::command_call(asset, |py| {
                start_coroutine(py, &callable, &loops, &pending, &state, &arguments, context, done_tx)
            })
        })
        .await
    };
    joined(started, COMMAND)?;
    let shut = callable.shut.clone();
    tokio::select! {
        // A dropped sender means the future was released without completing.
        done = done_rx => done.map_err(|_| {
            Error::cancelled("an async command's coroutine was dropped before it finished".to_string())
        })?,
        // A coroutine that ignores cancellation never completes; the shutdown does not wait for it.
        () = shut.wait() => return Err(shut_down_while_waiting()),
    }
    let finished = tokio::task::spawn_blocking(move || {
        crate::exit::attach_tracked(|py| coroutine_result(py, &callable, &pending))
    })
    .await;
    joined(finished, COMMAND)
}

/// Calls the callable, checks that it made a coroutine, and schedules it on a pool loop.
#[allow(clippy::too_many_arguments)]
fn start_coroutine(
    py: Python<'_>,
    callable: &PyCallable,
    loops: &LazyLoopPool,
    pending: &Mutex<Pending>,
    state: &CoreState<Value>,
    arguments: &CommandArguments<PyEnvironment>,
    context: CoreContext<PyEnvironment>,
    done_tx: tokio::sync::oneshot::Sender<()>,
) -> Result<(), Error> {
    let pool = loops.get(py)?;
    let (args, context) = prepare_call(py, callable, state, arguments, context)?;
    if !lock_pending(pending).keep_context(context) {
        return Err(Error::cancelled("the command was cancelled before it started".to_string()));
    }
    let scheduled = callable.object.bind(py).call1(args).and_then(|coroutine| {
        schedule_coroutine(py, coroutine, pool.pick().event_loop(py), pending, done_tx)
    });
    scheduled.map_err(|e| pyerr_to_error(py, &e))
}

/// The coroutine's result, once its future is done; the context is invalidated here.
fn coroutine_result(py: Python<'_>, callable: &PyCallable, pending: &Mutex<Pending>) -> Result<Value, Error> {
    let future = lock_pending(pending).finish().ok_or_else(|| {
        Error::cancelled("an async command's coroutine was cancelled".to_string())
    })?;
    let future = future.bind(py);
    let cancelled = future
        .call_method0("cancelled")
        .and_then(|cancelled| cancelled.is_truthy())
        .map_err(|e| pyerr_to_error(py, &e))?;
    if cancelled {
        return Err(Error::cancelled("an async command's coroutine was cancelled".to_string()));
    }
    let result = future.call_method0("result").map_err(|e| pyerr_to_error(py, &e))?;
    py_to_value(&result, callable.conversion, callable.pickle)
}

fn call_with_gil(
    py: Python<'_>,
    callable: &PyCallable,
    state: &CoreState<Value>,
    arguments: &CommandArguments<PyEnvironment>,
    context: CoreContext<PyEnvironment>,
) -> Result<Value, Error> {
    let (args, context) = prepare_call(py, callable, state, arguments, context)?;
    let result = callable.object.bind(py).call1(args);
    // The context is valid only during the call; a record source the command received resolves
    // through the environment from here on.
    context.invalidate();
    let result = result.map_err(|e| pyerr_to_error(py, &e))?;
    if is_coroutine(py, &result)? {
        // Closed so Python does not also warn that it was never awaited.
        let _ = result.call_method0("close");
        return Err(Error::execution_error(
            "a command returned a coroutine; declare its function `async def` so it is awaited"
                .to_string(),
        ));
    }
    py_to_value(&result, callable.conversion, callable.pickle)
}

fn is_coroutine(py: Python<'_>, object: &Bound<'_, PyAny>) -> Result<bool, Error> {
    crate::pycall::is_coroutine(py, object).map_err(|e| pyerr_to_error(py, &e))
}

/// The positional arguments of a call, and the `Context` wrapper of the call.
///
/// The wrapper is always made, even when the function takes no `context`: a record source among
/// the arguments resolves through it while the command runs, so its chunks are recorded as
/// dependencies (`records/resolver.rs`). The caller invalidates it when the command ends.
fn prepare_call<'py>(
    py: Python<'py>,
    callable: &PyCallable,
    state: &CoreState<Value>,
    arguments: &CommandArguments<PyEnvironment>,
    context: CoreContext<PyEnvironment>,
) -> Result<(Bound<'py, PyTuple>, Context), Error> {
    let envref = context.get_envref();
    let context = Context::new(context, callable.shut.clone());
    let scope = context.scope(&envref);
    let mut positional: Vec<Py<PyAny>> = Vec::with_capacity(arguments.len() + 2);
    match &callable.delivery {
        StateDelivery::None => {}
        StateDelivery::State => {
            positional.push(crate::asset::State::from_core(py, state, &scope)?.into_any());
        }
        StateDelivery::Value | StateDelivery::Reserved(_) => {
            positional.push(value_to_py(py, state.data_unchecked().as_ref(), &scope)?);
        }
        StateDelivery::Text => {
            let text = state.try_into_string()?;
            positional.push(PyString::new(py, &text).into_any().unbind());
        }
    }
    for i in 0..arguments.len() {
        match arguments.get_parameter(i, "argument")? {
            ParameterValue::MultipleParameters(name, elements) => {
                for element in elements {
                    let json = element.value().ok_or_else(|| {
                        Error::general_error(format!(
                            "an element of the multiple argument '{name}' was not resolved"
                        ))
                    })?;
                    positional.push(value_to_py(py, &Value::try_from_json_value(&json)?, &scope)?);
                }
            }
            ParameterValue::DefaultValue(_, _)
            | ParameterValue::ParameterValue(_, _, _)
            | ParameterValue::OverrideValue(_, _)
            | ParameterValue::DefaultLink(_, _)
            | ParameterValue::ParameterLink(_, _, _)
            | ParameterValue::OverrideLink(_, _)
            | ParameterValue::EnumLink(_, _, _)
            | ParameterValue::Placeholder(_)
            | ParameterValue::Injected(_)
            | ParameterValue::None => {
                positional.push(value_to_py(py, &arguments.get_value(i, "argument")?, &scope)?);
            }
        }
    }
    if let Some(index) = callable.context_index {
        let object = Py::new(py, context.clone_handle())
            .map_err(|e| pyerr_to_error(py, &e))?
            .into_any();
        positional.insert(index.min(positional.len()), object);
    }
    let args = PyTuple::new(py, positional).map_err(|e| pyerr_to_error(py, &e))?;
    Ok((args, context))
}
