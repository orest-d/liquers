//! `lq.Context`: the execution context a Python command receives.
//!
//! A command asks for it by naming a parameter `context`. It is valid only during the call that
//! received it: the command closure invalidates it when the callable returns, so a context kept
//! past that raises `GeneralError` instead of acting on an asset that has moved on.
//!
//! `evaluate` is nested evaluation. It records the query as a dependency of the current asset,
//! and it releases the GIL while it waits, so the nested command — Python or Rust — can run on
//! another thread (AC-13). An `async def` command uses `evaluate_async` instead: the sync form
//! raises there, since it would block the loop the command runs on.
//!
//! `evaluate` also takes a pipeline: one a query expresses is that query, any other runs through
//! `Context::apply_ad_hoc`, and a relative key in it raises `NotSupportedError`, as a relative
//! query does. `render` / `expand` render a template in this context, so its query tags are the
//! command's dependencies; `data_node(x)` reads a value in place through this context.
//!
//! # Main components
//!
//! - [`Context`] (`lq.Context`): `evaluate` / `evaluate_async` (queries and pipelines), `render`,
//!   `expand`, their `_async` forms, `validate_template`, `data_node`, logging (`info`,
//!   `warning`, `error`), `is_cancelled`, and `element`, the UI element whose action runs the
//!   command (`ui.rs`).
//!
//! # Where it is used
//!
//! - `command.rs`: the command closure makes one per call and invalidates it on return.
//! - `data_node.rs`: the `Command` root of `context.data_node(state)`.
//!
//! # Related concepts
//!
//! - `liquers_core::context::Context`; `template.rs` (the render arguments); `pipeline.rs`
//!   (`pipeline_has_relative_key`); `specs/reference/PYTHON_INTEGRATION.md` §4, §14-§16.
//!
//! # Examples
//!
//! Executed in `liquers-py/tests/test_EVAL_*.py`, `test_TEMPLATE_commands.py` and
//! `test_PIPELINE_evaluate.py` (a Rust doctest cannot start the interpreter):
//!
//! ```python
//! @env.first_command
//! def outer(context):
//!     return "outer:" + context.evaluate("hello")
//! assert env.evaluate("outer") == "outer:hello"
//! ```

use std::sync::{Arc, Mutex};

use liquers_core::context::Context as CoreContext;
use liquers_core::error::Error;
use pyo3::prelude::*;

use crate::aio::{awaitable, check_blocking_call, Shut, ToPy};
use crate::environment::{block_on, Evaluable, PyEnvironment};
use crate::error::{error_to_pyerr, PyFailure};
use crate::records::Scope;
use crate::template::{build_request, rendered_to_py, Arguments, Call};
use crate::value::{value_to_py, ConversionMode};

/// The execution context of a running Python command.
#[pyclass(frozen, name = "Context", module = "liquers")]
pub struct Context {
    inner: Arc<Mutex<Option<CoreContext<PyEnvironment>>>>,
    /// The environment's shut signal, so a nested wait ends when it is shut down.
    shut: Arc<Shut>,
    /// How a value this context converts (a render's `state`) is converted: the registering
    /// environment's setting.
    conversion: ConversionMode,
    pickle: bool,
}

impl Context {
    pub fn new(context: CoreContext<PyEnvironment>, shut: Arc<Shut>) -> Self {
        Context {
            inner: Arc::new(Mutex::new(Some(context))),
            shut,
            conversion: ConversionMode::Permissive,
            pickle: false,
        }
    }

    /// The same context, converting as `conversion` and `pickle` say.
    pub fn with_conversion(mut self, conversion: ConversionMode, pickle: bool) -> Self {
        self.conversion = conversion;
        self.pickle = pickle;
        self
    }

    /// Another handle on the same context; invalidating one invalidates both.
    pub fn clone_handle(&self) -> Self {
        Context {
            inner: self.inner.clone(),
            shut: self.shut.clone(),
            conversion: self.conversion,
            pickle: self.pickle,
        }
    }

    /// Ends the context's validity. Called when the command returns.
    pub fn invalidate(&self) {
        match self.inner.lock() {
            Ok(mut inner) => *inner = None,
            Err(poisoned) => *poisoned.into_inner() = None,
        }
    }

    /// Whether the command that received this context is still running.
    #[cfg_attr(not(test), allow(dead_code))] // the resolver asks `core()`; its tests ask this
    pub(crate) fn is_valid(&self) -> bool {
        self.core().is_some()
    }

    /// The environment's shut signal.
    pub(crate) fn shut(&self) -> Arc<Shut> {
        self.shut.clone()
    }

    /// The core context while it is valid.
    pub(crate) fn core(&self) -> Option<CoreContext<PyEnvironment>> {
        match self.inner.lock() {
            Ok(inner) => inner.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    /// The scope a value this command receives is converted in.
    pub(crate) fn scope(&self, envref: &liquers_core::context::EnvRef<PyEnvironment>) -> Scope {
        Scope::command(self, envref, self.shut.clone())
    }

    /// A render in this context, waited on like `evaluate`.
    fn render_sync(&self, py: Python<'_>, call: Call, arguments: Arguments<'_, '_>, full: bool) -> PyResult<Py<PyAny>> {
        check_blocking_call(py, call.name())?;
        let request = build_request(py, call, arguments, self.conversion, self.pickle)?;
        let context = self.get().map_err(|e| error_to_pyerr(py, &e))?;
        let rendered = block_on(py, &self.shut, liquers_lib::template::render_in_context(context, request))
            .map_err(|e| e.into_pyerr(py))?;
        rendered_to_py(py, rendered, full)
    }

    /// The awaitable form of [`Self::render_sync`].
    fn render_awaitable<'py>(
        &self,
        py: Python<'py>,
        call: Call,
        arguments: Arguments<'_, 'py>,
        full: bool,
    ) -> PyResult<Bound<'py, PyAny>> {
        crate::runtime::runtime().map_err(|e| error_to_pyerr(py, &e))?;
        let request = build_request(py, call, arguments, self.conversion, self.pickle)?;
        let context = self.get().map_err(|e| error_to_pyerr(py, &e))?;
        awaitable(
            py,
            &self.shut,
            async move {
                let rendered = liquers_lib::template::render_in_context(context, request).await?;
                Ok((Some(rendered), full))
            },
            ToPy::Rendered,
        )
    }

    fn get(&self) -> Result<CoreContext<PyEnvironment>, Error> {
        let inner = self
            .inner
            .lock()
            .map_err(|_| Error::unexpected_error("context lock poisoned".to_string()))?;
        inner.clone().ok_or_else(|| {
            Error::general_error(
                "the context is used after its command returned; it is valid only during the call"
                    .to_string(),
            )
        })
    }
}

/// What `Context.evaluate` evaluates: a query, or a pipeline no query can express. A pipeline a
/// query expresses is that query, so its dependency and its cache are the query's.
fn in_command_evaluable(
    py: Python<'_>,
    context: &CoreContext<PyEnvironment>,
    object: &Bound<'_, PyAny>,
) -> Result<Evaluable, Error> {
    match crate::environment::to_evaluable(py, object)? {
        Evaluable::Query(query) => Ok(Evaluable::Query(query)),
        Evaluable::Pipeline(pipeline) => {
            let envref = context.get_envref();
            if let Ok(query) = pipeline.to_query_with(envref.get_command_metadata_registry()) {
                return Ok(Evaluable::Query(query));
            }
            // `apply_ad_hoc` checks only a recipe's query for relative keys
            // (CONTEXT-APPLY-AD-HOC-SKIPS-RELATIVE-PIPELINE-KEYS), so the check is made here.
            if let Some(key) = crate::pipeline::pipeline_has_relative_key(&pipeline) {
                return Err(Error::not_supported(format!(
                    "Pipeline key '{key}' is relative and cannot be evaluated from a command. Take \
                     the current directory as a link argument (`-R-key/.`) and build an absolute \
                     key from it."
                )));
            }
            Ok(Evaluable::Pipeline(pipeline))
        }
    }
}

/// Evaluates `evaluable` as a dependency of the command's asset and waits for its state.
async fn evaluate_in_command(
    context: &CoreContext<PyEnvironment>,
    evaluable: Evaluable,
) -> Result<liquers_core::state::State<liquers_lib::value::Value>, Error> {
    match evaluable {
        Evaluable::Query(query) => {
            let asset = context.evaluate(&query).await?;
            context.wait_for_dependency(&asset).await
        }
        Evaluable::Pipeline(pipeline) => {
            let asset = context
                .apply_ad_hoc(
                    crate::environment::pipeline_recipe(pipeline),
                    liquers_core::state::State::new(),
                )
                .await?;
            asset.get().await
        }
    }
}

#[pymethods]
impl Context {
    /// `x` as a data node read in this command's context: an `lq.State` whose value needs the
    /// context (a record source, a directory listing) is read asynchronously through it, with the
    /// `_async` members, while the command runs; anything else is read as `lq.data_node(x)`.
    fn data_node(&self, py: Python<'_>, x: &Bound<'_, PyAny>) -> crate::data_node::DataNodeView {
        if let Ok(state) = x.cast::<crate::asset::State>() {
            return crate::data_node::DataNodeView::of_command(
                state.get().core().clone(),
                self.clone_handle(),
            );
        }
        crate::data_node::data_node(py, x)
    }

    /// Evaluates a query as a dependency of the current asset and returns its value.
    ///
    /// A pipeline is accepted too. One a query expresses is that query; any other runs in an
    /// ad-hoc asset whose dependencies become the command's. A relative key raises
    /// `NotSupportedError`, as a relative query does.
    fn evaluate(&self, py: Python<'_>, query: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        check_blocking_call(py, "evaluate")?;
        let result = (|| -> Result<_, PyFailure> {
            let context = self.get()?;
            let evaluable = in_command_evaluable(py, &context, query)?;
            let scope = self.scope(&context.get_envref());
            let value = block_on(py, &self.shut, async move {
                evaluate_in_command(&context, evaluable).await?.value()
            })?;
            Ok(value_to_py(py, &value, &scope)?)
        })();
        result.map_err(|e| e.into_pyerr(py))
    }

    /// Awaitable `evaluate`, for `async def` commands.
    fn evaluate_async<'py>(&self, py: Python<'py>, query: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        let prepared = (|| -> Result<_, Error> {
            crate::runtime::runtime()?;
            let context = self.get()?;
            Ok((in_command_evaluable(py, &context, query)?, context))
        })();
        let (evaluable, context) = prepared.map_err(|e| error_to_pyerr(py, &e))?;
        let scope = self.scope(&context.get_envref());
        awaitable(
            py,
            &self.shut,
            async move { Ok((evaluate_in_command(&context, evaluable).await?.value()?, scope)) },
            ToPy::Value,
        )
    }

    /// Renders `template` over `state` (the context, `{{.}}`) and `data` (names looked up above
    /// it), in this command's context: its query tags are dependencies of the command, and the
    /// errors a recovering `errors` mode handles are warnings in the command's log. Returns the
    /// text, or with `full=True` an `lq.RenderResult` with the warnings too.
    ///
    /// `template` is the text (`str`) or a link (`lq.Query`, `lq.Key`); relative keys resolve
    /// against `cwd` (an `lq.Key`, default the root). `html=True` escapes substituted text.
    #[pyo3(signature = (template, state=None, data=None, *, errors="strict", cwd=None, html=false, full=false))]
    #[allow(clippy::too_many_arguments)]
    fn render(
        &self,
        py: Python<'_>,
        template: &Bound<'_, PyAny>,
        state: Option<&Bound<'_, PyAny>>,
        data: Option<&Bound<'_, PyAny>>,
        errors: &str,
        cwd: Option<&Bound<'_, PyAny>>,
        html: bool,
        full: bool,
    ) -> PyResult<Py<PyAny>> {
        let arguments = Arguments { template, state, data, errors, cwd, html };
        self.render_sync(py, Call::Render, arguments, full)
    }

    /// Expands `template` over `data` (the context, `{{.}}`), as `render` does with no state.
    #[pyo3(signature = (template, data=None, *, errors="strict", cwd=None, html=false, full=false))]
    #[allow(clippy::too_many_arguments)]
    fn expand(
        &self,
        py: Python<'_>,
        template: &Bound<'_, PyAny>,
        data: Option<&Bound<'_, PyAny>>,
        errors: &str,
        cwd: Option<&Bound<'_, PyAny>>,
        html: bool,
        full: bool,
    ) -> PyResult<Py<PyAny>> {
        let arguments = Arguments { template, state: None, data, errors, cwd, html };
        self.render_sync(py, Call::Expand, arguments, full)
    }

    /// Awaitable `render`, for `async def` commands.
    #[pyo3(signature = (template, state=None, data=None, *, errors="strict", cwd=None, html=false, full=false))]
    #[allow(clippy::too_many_arguments)]
    fn render_async<'py>(
        &self,
        py: Python<'py>,
        template: &Bound<'py, PyAny>,
        state: Option<&Bound<'py, PyAny>>,
        data: Option<&Bound<'py, PyAny>>,
        errors: &str,
        cwd: Option<&Bound<'py, PyAny>>,
        html: bool,
        full: bool,
    ) -> PyResult<Bound<'py, PyAny>> {
        let arguments = Arguments { template, state, data, errors, cwd, html };
        self.render_awaitable(py, Call::Render, arguments, full)
    }

    /// Awaitable `expand`, for `async def` commands.
    #[pyo3(signature = (template, data=None, *, errors="strict", cwd=None, html=false, full=false))]
    #[allow(clippy::too_many_arguments)]
    fn expand_async<'py>(
        &self,
        py: Python<'py>,
        template: &Bound<'py, PyAny>,
        data: Option<&Bound<'py, PyAny>>,
        errors: &str,
        cwd: Option<&Bound<'py, PyAny>>,
        html: bool,
        full: bool,
    ) -> PyResult<Bound<'py, PyAny>> {
        let arguments = Arguments { template, state: None, data, errors, cwd, html };
        self.render_awaitable(py, Call::Expand, arguments, full)
    }

    /// Every problem in template `text`, as `lq.Diagnostic`s, checked against the commands this
    /// environment has; nothing is evaluated.
    fn validate_template(&self, py: Python<'_>, text: &str) -> PyResult<Vec<crate::template::Diagnostic>> {
        let context = self.get().map_err(|e| error_to_pyerr(py, &e))?;
        let envref = context.get_envref();
        Ok(crate::template::validate(text, envref.get_command_metadata_registry()))
    }

    /// Adds an info entry to the asset's log.
    fn info(&self, py: Python<'_>, message: &str) -> PyResult<()> {
        self.get()
            .and_then(|context| context.info(message))
            .map_err(|e| error_to_pyerr(py, &e))
    }

    /// Adds a warning to the asset's log.
    fn warning(&self, py: Python<'_>, message: &str) -> PyResult<()> {
        self.get()
            .and_then(|context| context.warning(message))
            .map_err(|e| error_to_pyerr(py, &e))
    }

    /// Adds an error-level entry to the asset's log, which also fails the evaluation: the command
    /// keeps running, and once it returns the query raises `ExecutionError` with the first such
    /// message.
    fn error(&self, py: Python<'_>, message: &str) -> PyResult<()> {
        self.get()
            .and_then(|context| context.error(message))
            .map_err(|e| error_to_pyerr(py, &e))
    }

    /// Whether cancellation of the asset has been requested. A command that sees `True` should
    /// stop, for example by raising `lq.CancelledError`.
    fn is_cancelled(&self, py: Python<'_>) -> PyResult<bool> {
        self.get()
            .map(|context| context.is_cancelled())
            .map_err(|e| error_to_pyerr(py, &e))
    }

    /// The handle of the UI element whose action runs this command (`ui.submit(query, handle)`
    /// or a control's action), or `None` outside a UI action.
    #[getter]
    fn element(&self, py: Python<'_>) -> PyResult<Option<u64>> {
        use liquers_lib::ui::UIPayload;
        self.get()
            .map(|context| context.get_payload_clone().and_then(|p| p.handle()).map(|h| h.0))
            .map_err(|e| error_to_pyerr(py, &e))
    }
}
