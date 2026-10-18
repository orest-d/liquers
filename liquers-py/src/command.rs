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
//! The metadata comes from the shared declaration pipeline: the decorator in
//! `python/liquers/_commands.py` supplies the baseline (stage 1) and the author's keyword
//! arguments, and [`PyCommandSpec::new`] runs stages 2–5 with `CommandDeclaration`.

use std::sync::Arc;

use liquers_core::command_declaration::{CommandDeclaration, StateDelivery};
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
}

impl PyCallable {
    fn new(
        object: Py<PyAny>,
        delivery: StateDelivery,
        context_index: Option<usize>,
        conversion: ConversionMode,
        pickle: bool,
    ) -> Self {
        #[cfg(feature = "debug-handles")]
        LIVE_HANDLES.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        PyCallable {
            object,
            delivery,
            context_index,
            conversion,
            pickle,
        }
    }
}

#[cfg(feature = "debug-handles")]
static LIVE_HANDLES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// How many Python callables the registrations of every environment currently hold.
///
/// Test-only, behind `debug-handles`, for `RUNTIME05`: Python cannot observe a Rust drop
/// deterministically, so the count of live [`PyCallable`]s — each owning exactly one callable — is
/// what the test asserts on.
#[cfg(feature = "debug-handles")]
pub fn live_handle_count() -> usize {
    LIVE_HANDLES.load(std::sync::atomic::Ordering::SeqCst)
}

#[cfg(feature = "debug-handles")]
impl Drop for PyCallable {
    fn drop(&mut self) {
        LIVE_HANDLES.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
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
    pub fn new(
        object: Py<PyAny>,
        baseline: serde_json::Value,
        declaration: &serde_json::Value,
        context_index: Option<usize>,
        implementation: Option<&[u8]>,
        conversion: ConversionMode,
        pickle: bool,
    ) -> Result<Self, Error> {
        let mut command = CommandDeclaration::from_introspection(baseline);
        command.enhance(declaration)?;
        let mut metadata = command.finish()?;
        for warning in command.warnings() {
            eprintln!("command {:?}: {}", warning.command, warning.message);
        }
        let delivery = command
            .registration()
            .get("state")
            .and_then(|mode| mode.as_str())
            .map(StateDelivery::from_argument_name)
            .unwrap_or(StateDelivery::None)
            .effective();
        metadata.module = PYTHON_MODULE.to_string();
        // A changed implementation must not reuse results stored by the old one; a callable with
        // no code object gets a fresh version on every registration.
        metadata.impl_version = match implementation {
            Some(bytes) => Version::from_content(bytes),
            None => Version::new_unique(),
        };
        Ok(PyCommandSpec {
            key: metadata.key(),
            metadata,
            callable: Arc::new(PyCallable::new(
                object,
                delivery,
                context_index,
                conversion,
                pickle,
            )),
        })
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
    let joined = tokio::task::spawn_blocking(move || {
        Python::attach(|py| call_with_gil(py, &callable, &state, &arguments, context))
    })
    .await;
    match joined {
        Ok(result) => result,
        Err(e) if e.is_panic() => Err(Error::unexpected_error(format!(
            "a Python command panicked: {e}"
        ))),
        Err(e) => Err(Error::cancelled(format!("a Python command was cancelled: {e}"))),
    }
}

fn call_with_gil(
    py: Python<'_>,
    callable: &PyCallable,
    state: &CoreState<Value>,
    arguments: &CommandArguments<PyEnvironment>,
    context: CoreContext<PyEnvironment>,
) -> Result<Value, Error> {
    let mut positional: Vec<Py<PyAny>> = Vec::with_capacity(arguments.len() + 2);
    match &callable.delivery {
        StateDelivery::None => {}
        StateDelivery::State => {
            positional.push(crate::asset::State::from_core(py, state)?.into_any());
        }
        StateDelivery::Value | StateDelivery::Reserved(_) => {
            positional.push(value_to_py(py, state.data_unchecked().as_ref())?);
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
                    positional.push(value_to_py(py, &Value::try_from_json_value(&json)?)?);
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
                positional.push(value_to_py(py, &arguments.get_value(i, "argument")?)?);
            }
        }
    }
    let context = callable.context_index.map(|index| (index, Context::new(context)));
    if let Some((index, context)) = &context {
        let object = Py::new(py, context.clone_handle())
            .map_err(|e| pyerr_to_error(py, &e))?
            .into_any();
        positional.insert((*index).min(positional.len()), object);
    }
    let args = PyTuple::new(py, positional).map_err(|e| pyerr_to_error(py, &e))?;
    let result = callable.object.bind(py).call1(args);
    // The context is valid only during the call.
    if let Some((_, context)) = &context {
        context.invalidate();
    }
    let result = result.map_err(|e| pyerr_to_error(py, &e))?;
    py_to_value(&result, callable.conversion, callable.pickle)
}
