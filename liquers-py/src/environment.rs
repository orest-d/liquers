//! `lq.Environment`: the Python handle on a Liquers environment.
//!
//! # Retained declarations, lazy build, rebuild on change
//!
//! A built `DefaultEnvironment` is frozen: its command registry cannot change once it is shared.
//! Python, though, registers commands whenever a module is imported — before or after the first
//! evaluation. So the handle keeps every declaration it receives (configuration, Python command
//! specs, unregistrations and imported metadata), builds the environment from them on first
//! use, and marks it for a rebuild when a declaration arrives late. This is liquers-web's
//! `rebuild_with`, without the global.
//!
//! A rebuild keeps the store: the first build constructs it, and every later build is given the
//! same `Arc` through `with_async_store`, so data written with `store_set` survives (AC-10). The
//! asset cache does not survive, which is the price of a late registration. The old environment
//! moves to the retired list rather than being dropped, so an evaluation in flight against it
//! finishes, and `shutdown()` stops the retired managers with the current one.
//!
//! # Threads and the GIL
//!
//! Every call that waits releases the GIL (`py.detach`) around `block_on` on the process runtime,
//! so a Python command called from a tokio worker can take it. The state lock is never held across
//! an evaluation: it is taken to read or build the environment reference and released at once.
//!
//! The state lock is taken with `lock_py_attached`, which detaches while it waits, so a thread
//! blocked on it never stalls the interpreter (the GIL, or a free-threaded stop-the-world). And no
//! Python object is dropped while it is held: a removed command's callable is released after the
//! guard, because its `__del__` may call back into this environment and take the lock again.
//!
//! A wait is cut into short slices. Between two, `block_on` runs the pending signal handlers, so
//! Ctrl-C raises `KeyboardInterrupt` in a waiting `evaluate`, and it checks the handle's shut
//! flag, so a wait outstanding when `shutdown()` is called raises `CancelledError` instead of
//! hanging on an asset manager that no longer runs. Leaving a wait does not cancel the asset.

use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use pyo3::sync::MutexExt;
use std::time::Duration;

use liquers_core::command_metadata::{CommandKey, CommandMetadata};
use liquers_core::commands::CommandRegistry;
use liquers_core::context::EnvRef;
use liquers_core::environment_builder::EnvironmentBuilder;
use liquers_core::environment_config::EnvironmentConfig;
use liquers_core::error::Error;
use liquers_core::issue_report::{Issue as CoreIssue, IssueReport, IssueSeverity};
use liquers_core::metadata::{Metadata, MetadataRecord};
use liquers_core::parse::{parse_key, parse_query};
use liquers_core::assets::AssetManager;
use liquers_core::state::State;
use liquers_core::store::{AsyncMemoryStore, AsyncStore};
use liquers_core::query::Key;
use liquers_core::type_system::TypeRegistry;
use liquers_core::value::{DefaultValueSerializer, ValueInterface};
use liquers_lib::environment::{default_environment_builder, DefaultEnvironment, LibKind};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyString};

use crate::aio::{awaitable, check_blocking_call, refuse_shutdown_on_loop, shut_down_while_waiting, LazyLoopPool, Shut, ToPy};
use crate::command::{register_py_command, PyCommandSpec, Replaced};
use crate::error::{error_to_pyerr, pyerr_to_error, PyFailure};
use crate::runtime::runtime;
use crate::value::{py_to_value, py_type_info, value_to_py, ConversionMode, Value};

/// The environment every Python handle builds.
pub type PyEnvironment = DefaultEnvironment<Value>;

type PyBuilder = EnvironmentBuilder<Value, (), LibKind>;

/// Where a handle's environment is in its life.
enum EnvState {
    /// Not built yet, or a declaration changed since the last build.
    Pending,
    Built(EnvRef<PyEnvironment>),
    /// `shutdown()` was called; every later use fails.
    Shut,
}

/// Everything a handle retains to build, and rebuild, its environment.
struct Declarations {
    state: EnvState,
    /// The store, constructed by the first build and handed to every rebuild.
    store: Option<Arc<dyn AsyncStore>>,
    /// Environments replaced by a rebuild, kept until `shutdown()`.
    retired: Vec<EnvRef<PyEnvironment>>,
    /// Python commands, in registration order; at most one per key.
    specs: Vec<PyCommandSpec>,
    /// Keys `unregister` removed, applied after the built-ins so a built-in stays removed.
    unregistered: Vec<CommandKey>,
    /// Command metadata from `import_metadata`, applied last, so it can describe any command.
    imported: Vec<CommandMetadata>,
}

/// The shared state behind an `lq.Environment`.
pub struct EnvCell {
    declarations: Mutex<Declarations>,
    config: EnvironmentConfig,
    pub conversion: ConversionMode,
    pub pickle: bool,
    /// Fired by `shutdown()`; every wait started from this handle watches it.
    pub shut: Arc<Shut>,
    /// The loops `async def` commands run on, shared by every rebuild.
    pub loops: Arc<LazyLoopPool>,
}

impl EnvCell {
    pub fn new(
        config: EnvironmentConfig,
        conversion: ConversionMode,
        pickle: bool,
        async_loops: NonZeroUsize,
    ) -> Result<Self, Error> {
        Ok(EnvCell {
            declarations: Mutex::new(Declarations {
                state: EnvState::Pending,
                store: None,
                retired: Vec::new(),
                specs: Vec::new(),
                unregistered: Vec::new(),
                imported: Vec::new(),
            }),
            config,
            conversion,
            pickle,
            shut: Arc::new(Shut::default()),
            loops: Arc::new(LazyLoopPool::new(async_loops)),
        })
    }

    fn lock(&self, py: Python<'_>) -> Result<MutexGuard<'_, Declarations>, Error> {
        self.declarations
            .lock_py_attached(py)
            .map_err(|_| Error::unexpected_error("environment state lock poisoned".to_string()))
    }

    /// The built environment, building it first if a declaration changed.
    pub fn envref(&self, py: Python<'_>) -> Result<EnvRef<PyEnvironment>, Error> {
        let mut declarations = self.lock(py)?;
        match &declarations.state {
            EnvState::Built(envref) => return Ok(envref.clone()),
            EnvState::Shut => return Err(shut_down_error()),
            EnvState::Pending => {}
        }
        let envref = self.build(&declarations)?;
        if declarations.store.is_none() {
            declarations.store = Some(envref.get_async_store());
        }
        declarations.state = EnvState::Built(envref.clone());
        Ok(envref)
    }

    /// Marks the environment for a rebuild after a declaration changed.
    ///
    /// Fails after `shutdown()`, so a registration on a shut environment is not silently dropped.
    pub fn invalidate(&self, py: Python<'_>) -> Result<(), Error> {
        retire(&mut *self.lock(py)?)
    }

    /// The builder with every declaration applied, but no store yet.
    fn configured_builder(&self, declarations: &Declarations) -> Result<(PyBuilder, IssueReport), Error> {
        let mut types = TypeRegistry::from_value_type::<Value>();
        types.register(py_type_info(self.pickle))?;
        let mut builder = default_environment_builder::<Value, ()>()
            .with_type_registry(types)
            .with_asset_manager_options(self.config.assets.clone())
            .with_recipes(self.config.recipes.clone());
        crate::builtins::register(&mut builder.command_registry)?;
        for key in &declarations.unregistered {
            builder.command_registry.unregister(key.clone());
        }
        for spec in &declarations.specs {
            register_py_command(&mut builder.command_registry, spec)?;
        }
        let issues = apply_imported(&mut builder.command_registry, &declarations.imported);
        Ok((builder, issues))
    }

    fn build(&self, declarations: &Declarations) -> Result<EnvRef<PyEnvironment>, Error> {
        let (mut builder, mut issues) = self.configured_builder(declarations)?;
        issues.append(builder.validate().clone());
        if let Some(summary) = issues.short_summary("Command metadata") {
            return Err(Error::configuration_error(summary));
        }
        builder = match &declarations.store {
            Some(store) => builder.with_async_store(store.clone()),
            None if self.config.store.stores.is_empty() => {
                builder.with_async_store(Arc::new(AsyncMemoryStore::new(&Key::new())))
            }
            None => builder.with_store_config(
                self.config.store.clone(),
                Box::new(liquers_store::store_factory::default_store_factory()),
            ),
        };
        // The asset manager spawns its workers on the ambient runtime.
        let _guard = runtime()?.enter();
        builder.build()
    }

    /// Every issue a build would meet, without building.
    pub fn validate(&self, py: Python<'_>) -> Result<IssueReport, Error> {
        let declarations = self.lock(py)?;
        let (mut builder, mut issues) = self.configured_builder(&declarations)?;
        issues.append(builder.validate().clone());
        Ok(issues)
    }

    /// Retains a Python command and marks the environment for a rebuild. Reports what it replaced.
    pub fn register(&self, py: Python<'_>, spec: PyCommandSpec) -> Result<Replaced, Error> {
        // Declared before the guard, so a replaced command is dropped after the lock is released.
        let mut removed: Vec<PyCommandSpec> = Vec::new();
        let mut declarations = self.lock(py)?;
        refuse_if_shut(&declarations)?;
        let replaced = if declarations.specs.iter().any(|s| s.key == spec.key) {
            Replaced::Python
        } else if !declarations.unregistered.contains(&spec.key)
            && builtin_keys()?.contains(&spec.key)
        {
            Replaced::Builtin
        } else {
            Replaced::Nothing
        };
        take_specs(&mut declarations.specs, &spec.key, &mut removed);
        declarations.unregistered.retain(|k| k != &spec.key);
        declarations.specs.push(spec);
        retire(&mut declarations)?;
        Ok(replaced)
    }

    /// Removes the top command under `key`. `false` when there was nothing to remove.
    ///
    /// A Python command that replaced a built-in is removed first, which brings the built-in back;
    /// a second call removes the built-in.
    pub fn unregister(&self, py: Python<'_>, key: CommandKey) -> Result<bool, Error> {
        let mut taken: Vec<PyCommandSpec> = Vec::new();
        let mut declarations = self.lock(py)?;
        refuse_if_shut(&declarations)?;
        take_specs(&mut declarations.specs, &key, &mut taken);
        let removed = if !taken.is_empty() {
            true
        } else if !declarations.unregistered.contains(&key) && builtin_keys()?.contains(&key) {
            declarations.unregistered.push(key);
            true
        } else {
            false
        };
        if removed {
            retire(&mut declarations)?;
        }
        Ok(removed)
    }

    pub fn import_metadata(&self, py: Python<'_>, commands: Vec<CommandMetadata>) -> Result<(), Error> {
        let mut declarations = self.lock(py)?;
        refuse_if_shut(&declarations)?;
        declarations.imported.extend(commands);
        retire(&mut declarations)
    }

    /// Stops the current and every retired asset manager, then the loop pool. Idempotent.
    ///
    /// The asset managers stop taking work first; stopping the pool then cancels the coroutines
    /// still running, since a stopped asset manager does not cancel running assets.
    pub fn shutdown(&self, py: Python<'_>) -> Result<(), Error> {
        self.shut.fire();
        let specs: Vec<PyCommandSpec>;
        let mut declarations = self.lock(py)?;
        let previous = std::mem::replace(&mut declarations.state, EnvState::Shut);
        let mut envrefs = std::mem::take(&mut declarations.retired);
        match previous {
            EnvState::Built(envref) => envrefs.push(envref),
            EnvState::Pending | EnvState::Shut => {}
        }
        declarations.store = None;
        // Releases the Python callables now rather than when the handle is collected, but only
        // once the lock is released.
        specs = std::mem::take(&mut declarations.specs);
        drop(declarations);
        drop(specs);
        for envref in envrefs {
            envref.get_asset_manager().shutdown();
        }
        self.loops.close(py).map_err(|e| pyerr_to_error(py, &e))
    }
}

/// Moves every spec under `key` out of `specs` into `removed`, which the caller drops after its
/// guard.
fn take_specs(specs: &mut Vec<PyCommandSpec>, key: &CommandKey, removed: &mut Vec<PyCommandSpec>) {
    let (taken, kept): (Vec<_>, Vec<_>) = std::mem::take(specs).into_iter().partition(|s| &s.key == key);
    *specs = kept;
    removed.extend(taken);
}

pub(crate) fn shut_down_error() -> Error {
    Error::general_error("the environment has been shut down".to_string())
}

fn refuse_if_shut(declarations: &Declarations) -> Result<(), Error> {
    match declarations.state {
        EnvState::Shut => Err(shut_down_error()),
        EnvState::Pending | EnvState::Built(_) => Ok(()),
    }
}

/// Marks the environment for a rebuild, keeping the built one on the retired list.
fn retire(declarations: &mut Declarations) -> Result<(), Error> {
    refuse_if_shut(declarations)?;
    match std::mem::replace(&mut declarations.state, EnvState::Pending) {
        EnvState::Built(envref) => declarations.retired.push(envref),
        EnvState::Pending | EnvState::Shut => {}
    }
    Ok(())
}

/// The keys of the built-in commands, computed once.
fn builtin_keys() -> Result<&'static Vec<CommandKey>, Error> {
    static KEYS: OnceLock<Result<Vec<CommandKey>, Error>> = OnceLock::new();
    KEYS.get_or_init(|| {
        let mut registry = CommandRegistry::<PyEnvironment>::new();
        crate::builtins::register(&mut registry)?;
        Ok(registry
            .command_metadata_registry
            .commands
            .iter()
            .map(|command| command.key())
            .collect())
    })
    .as_ref()
    .map_err(|e| e.clone())
}

/// Adds imported command metadata to a registry.
///
/// An entry with no command behind it would plan and then fail at execution, so it is reported
/// as an error rather than added silently.
fn apply_imported(
    registry: &mut CommandRegistry<PyEnvironment>,
    imported: &[CommandMetadata],
) -> IssueReport {
    let mut issues = IssueReport::default();
    for command in imported {
        let key: CommandKey = command.key();
        if registry.command_metadata_registry.get(key).is_none() {
            issues.extend([CoreIssue::command_registry(
                IssueSeverity::Error,
                &command.realm,
                &command.namespace,
                &command.name,
                "Imported metadata names a command that is not registered",
            )]);
        }
        issues.append(command.check());
        registry.command_metadata_registry.add_command(command);
    }
    issues
}

/// The command metadata document `import_metadata` reads: a `commands` list.
#[derive(Deserialize, Default)]
#[serde(default)]
struct ImportedMetadata {
    commands: Vec<CommandMetadata>,
}

fn parse_config(py: Python<'_>, config: &Bound<'_, PyAny>) -> Result<EnvironmentConfig, Error> {
    if let Ok(text) = config.cast::<PyString>() {
        return EnvironmentConfig::from_yaml(&text.to_string());
    }
    let json = py
        .import("json")
        .and_then(|json| json.call_method1("dumps", (config,)))
        .and_then(|dumped| dumped.extract::<String>().map_err(PyErr::from))
        .map_err(|e| {
            Error::configuration_error(format!(
                "config must be a dict or a YAML string: {e}"
            ))
        })?;
    EnvironmentConfig::from_json(&json)
}

fn bound_decorator(
    env: &Bound<'_, Environment>,
    source: bool,
    callable: Option<&Bound<'_, PyAny>>,
    options: Option<&Bound<'_, PyDict>>,
) -> PyResult<Py<PyAny>> {
    let py = env.py();
    let decorate = py
        .import("liquers._commands")?
        .call_method1("_bound", (env, source))?;
    let callable = match callable {
        Some(callable) => callable.clone(),
        None => py.None().into_bound(py),
    };
    Ok(decorate.call((callable,), options)?.unbind())
}

/// Whether a command callable is an `async def` function, or an object whose `__call__` is one.
fn is_coroutine_function(py: Python<'_>, callable: &Bound<'_, PyAny>) -> Result<bool, Error> {
    let detected = (|| -> PyResult<bool> {
        let inspect = py.import("inspect")?;
        if inspect.call_method1("iscoroutinefunction", (callable,))?.is_truthy()? {
            return Ok(true);
        }
        if inspect.call_method1("isroutine", (callable,))?.is_truthy()? {
            return Ok(false);
        }
        match callable.get_type().getattr("__call__") {
            Ok(call) => inspect.call_method1("iscoroutinefunction", (call,))?.is_truthy(),
            Err(_) => Ok(false),
        }
    })();
    detected.map_err(|e| pyerr_to_error(py, &e))
}

/// A query given as text or as an `lq.Query`.
pub fn to_query(query: &Bound<'_, PyAny>) -> Result<liquers_core::query::Query, Error> {
    if let Ok(text) = query.cast::<PyString>() {
        return parse_query(&text.to_string());
    }
    match query.cast::<crate::query::Query>() {
        Ok(wrapped) => Ok(wrapped.borrow().0.clone()),
        Err(_) => Err(Error::general_error(format!(
            "a query must be a str or an lq.Query, not {}",
            query
                .get_type()
                .name()
                .map(|n| n.to_string())
                .unwrap_or_else(|_| "?".to_string())
        ))),
    }
}

/// A JSON-compatible Python object (the decorator's documents) as JSON.
fn py_to_json(py: Python<'_>, object: &Bound<'_, PyAny>) -> Result<serde_json::Value, Error> {
    let text = py
        .import("json")
        .and_then(|json| json.call_method1("dumps", (object,)))
        .and_then(|dumped| dumped.extract::<String>().map_err(PyErr::from))
        .map_err(|e| pyerr_to_error(py, &e))?;
    serde_json::from_str(&text).map_err(|e| Error::general_error(format!("invalid declaration: {e}")))
}

/// One validation finding.
#[pyclass(frozen, name = "Issue", module = "liquers")]
pub struct Issue {
    /// `"debug"`, `"info"`, `"warning"` or `"error"`.
    #[pyo3(get)]
    severity: String,
    /// The finding, as text.
    #[pyo3(get)]
    message: String,
    /// The command key (`realm-namespace-name`) the finding is about, if any.
    #[pyo3(get)]
    key: Option<String>,
}

#[pymethods]
impl Issue {
    fn __repr__(&self) -> String {
        match &self.key {
            Some(key) => format!("Issue({}: {} on {key})", self.severity, self.message),
            None => format!("Issue({}: {})", self.severity, self.message),
        }
    }
}

impl From<&CoreIssue> for Issue {
    fn from(issue: &CoreIssue) -> Self {
        let key = match issue {
            CoreIssue::Generic { .. } => None,
            CoreIssue::CommandRegistry {
                realm,
                namespace,
                name,
                ..
            } => Some(format!("{realm}-{namespace}-{name}")),
        };
        Issue {
            severity: issue.severity().to_string(),
            message: issue.message().to_string(),
            key,
        }
    }
}

/// A Liquers environment: commands, a store and the assets evaluated in it.
///
/// `Environment()` takes keyword arguments only: `config` (a dict or YAML text configuring the
/// store and the asset manager), `conversion`, `pickle`, `async_loops` and `job_capacity`. It is
/// built on first use from the commands registered on it, and runs until `shutdown()`. See
/// `PYTHON_GUIDE.md` §4.
#[pyclass(frozen, name = "Environment", module = "liquers")]
pub struct Environment {
    pub cell: Arc<EnvCell>,
}

/// How long a wait runs between two checks for a signal or a shutdown.
const WAIT_SLICE: Duration = Duration::from_millis(100);

/// Runs a future on the process runtime with the GIL released.
///
/// Between slices of [`WAIT_SLICE`] it runs Python's pending signal handlers — an exception they
/// raise (`KeyboardInterrupt`) ends the wait and is re-raised as is — and checks `shut`, ending
/// the wait with `Cancelled` once the environment is shut down. Ending a wait drops the future;
/// it does not cancel an asset the future was waiting for.
pub fn block_on<T: Send>(
    py: Python<'_>,
    shut: &Shut,
    future: impl std::future::Future<Output = Result<T, Error>> + Send,
) -> Result<T, PyFailure> {
    let runtime = runtime()?;
    let mut future = std::pin::pin!(future);
    loop {
        // The timer is made inside `block_on`: it needs the runtime's context.
        let slice = py.detach(|| {
            runtime.block_on(async { tokio::time::timeout(WAIT_SLICE, future.as_mut()).await })
        });
        match slice {
            Ok(result) => return result.map_err(PyFailure::from),
            Err(_elapsed) => {
                py.check_signals().map_err(PyFailure::Python)?;
                if shut.is_set() {
                    return Err(shut_down_while_waiting().into());
                }
            }
        }
    }
}

impl Environment {
    /// The parsed query and the built environment an `_async` method needs, so that a bad query
    /// or a shut environment raises at call time.
    fn query_and_envref(
        &self,
        py: Python<'_>,
        query: &Bound<'_, PyAny>,
    ) -> PyResult<(liquers_core::query::Query, EnvRef<PyEnvironment>)> {
        let prepared = (|| -> Result<_, Error> {
            // Bound first, so the async bridge never builds a runtime of its own.
            crate::runtime::runtime()?;
            Ok((to_query(query)?, self.cell.envref(py)?))
        })();
        prepared.map_err(|e| error_to_pyerr(py, &e))
    }

    /// The key and the state `store_set` writes. The value is serialized here, so a value with no
    /// byte form raises `SerializationError` instead of being kept as metadata only (AC-6), and
    /// the data format is always recorded, so the read path never has to guess it.
    fn stored_state(&self, key: &str, data: &Bound<'_, PyAny>) -> Result<(Key, State<Value>), Error> {
        let key = parse_key(key)?;
        let value = py_to_value(data, self.cell.conversion, self.cell.pickle)?;
        let data_format = value.default_data_format().to_string();
        value.as_bytes(&data_format)?;
        let mut record = MetadataRecord::new();
        record.data_format = Some(data_format);
        let state = State::from_value_and_metadata(value, Arc::new(Metadata::MetadataRecord(record)));
        Ok((key, state))
    }
}

#[pymethods]
impl Environment {
    #[new]
    #[pyo3(signature = (*, config=None, conversion="permissive", pickle=false, async_loops=1, job_capacity=None))]
    fn new(
        py: Python<'_>,
        config: Option<&Bound<'_, PyAny>>,
        conversion: &str,
        pickle: bool,
        async_loops: i64,
        job_capacity: Option<i64>,
    ) -> PyResult<Self> {
        let built = (|| {
            let mut config = match config {
                Some(config) => parse_config(py, config)?,
                None => EnvironmentConfig::default(),
            };
            let async_loops = usize::try_from(async_loops)
                .ok()
                .and_then(NonZeroUsize::new)
                .ok_or_else(|| {
                    Error::configuration_error(format!(
                        "async_loops must be at least 1, not {async_loops}"
                    ))
                })?;
            if let Some(capacity) = job_capacity {
                let capacity = usize::try_from(capacity)
                    .ok()
                    .filter(|capacity| *capacity >= 1)
                    .ok_or_else(|| {
                        Error::configuration_error(format!(
                            "job_capacity must be at least 1, not {capacity}"
                        ))
                    })?;
                match config.assets.job_capacity {
                    Some(configured) if configured != capacity => {
                        return Err(Error::configuration_error(format!(
                            "job_capacity={capacity} conflicts with the configured assets.job_capacity of {configured}"
                        )))
                    }
                    Some(_) | None => config.assets.job_capacity = Some(capacity),
                }
            }
            EnvCell::new(config, ConversionMode::parse(conversion)?, pickle, async_loops)
        })();
        match built {
            Ok(cell) => Ok(Environment {
                cell: Arc::new(cell),
            }),
            Err(e) => Err(error_to_pyerr(py, &e)),
        }
    }

    /// Evaluates a query and returns its value as a Python object.
    fn evaluate(&self, py: Python<'_>, query: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        check_blocking_call(py, "evaluate")?;
        let result = (|| -> Result<_, PyFailure> {
            let query = to_query(query)?;
            let envref = self.cell.envref(py)?;
            let value = block_on(py, &self.cell.shut, async move {
                let asset = envref.evaluate(query).await?;
                let state = asset.get().await?;
                state.value()
            })?;
            Ok(value_to_py(py, &value)?)
        })();
        result.map_err(|e| e.into_pyerr(py))
    }

    /// Evaluates a query and returns the result as a `State`, metadata included.
    fn evaluate_state(&self, py: Python<'_>, query: &Bound<'_, PyAny>) -> PyResult<Py<crate::asset::State>> {
        check_blocking_call(py, "evaluate_state")?;
        let result = (|| -> Result<_, PyFailure> {
            let query = to_query(query)?;
            let envref = self.cell.envref(py)?;
            let state = block_on(py, &self.cell.shut, async move {
                let asset = envref.evaluate(query).await?;
                asset.get().await?.value_state()
            })?;
            Ok(crate::asset::State::from_core(py, &state)?)
        })();
        result.map_err(|e| e.into_pyerr(py))
    }

    /// Starts evaluating a query and returns its `AssetRef` without waiting for the value.
    fn get_asset(&self, py: Python<'_>, query: &Bound<'_, PyAny>) -> PyResult<crate::asset::AssetRef> {
        check_blocking_call(py, "get_asset")?;
        let result = (|| -> Result<_, PyFailure> {
            let query = to_query(query)?;
            let envref = self.cell.envref(py)?;
            let asset = block_on(py, &self.cell.shut, async move { envref.evaluate(query).await })?;
            Ok(crate::asset::AssetRef::new(asset, self.cell.shut.clone()))
        })();
        result.map_err(|e| e.into_pyerr(py))
    }

    /// Writes a value to the store under `key`.
    ///
    /// A value with no byte form — an opaque Python object with the pickle codec off — raises
    /// `SerializationError`.
    fn store_set(&self, py: Python<'_>, key: &str, data: &Bound<'_, PyAny>) -> PyResult<()> {
        check_blocking_call(py, "store_set")?;
        let result = (|| -> Result<_, PyFailure> {
            let (key, state) = self.stored_state(key, data)?;
            let envref = self.cell.envref(py)?;
            block_on(py, &self.cell.shut, async move {
                envref.get_asset_manager().set_state(&key, state).await
            })
        })();
        result.map_err(|e| e.into_pyerr(py))
    }

    /// Awaitable `evaluate`: an `asyncio.Future` of the caller's running loop.
    ///
    /// A malformed query raises at call time. Cancelling the future ends only the wait; the
    /// evaluation goes on.
    fn evaluate_async<'py>(&self, py: Python<'py>, query: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        let (query, envref) = self.query_and_envref(py, query)?;
        awaitable(
            py,
            &self.cell.shut,
            async move {
                let asset = envref.evaluate(query).await?;
                asset.get().await?.value()
            },
            ToPy::Value,
        )
    }

    /// Awaitable `evaluate_state`.
    fn evaluate_state_async<'py>(&self, py: Python<'py>, query: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        let (query, envref) = self.query_and_envref(py, query)?;
        awaitable(
            py,
            &self.cell.shut,
            async move {
                let asset = envref.evaluate(query).await?;
                asset.get().await?.value_state()
            },
            ToPy::State,
        )
    }

    /// Awaitable `get_asset`: resolves to the `AssetRef` once the evaluation has started.
    fn get_asset_async<'py>(&self, py: Python<'py>, query: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        let (query, envref) = self.query_and_envref(py, query)?;
        let shut = self.cell.shut.clone();
        awaitable(
            py,
            &self.cell.shut,
            async move { Ok((envref.evaluate(query).await?, shut)) },
            ToPy::Asset,
        )
    }

    /// Awaitable `store_set`. The value is converted and serialized at call time.
    fn store_set_async<'py>(&self, py: Python<'py>, key: &str, data: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        let prepared = (|| -> Result<_, Error> {
            crate::runtime::runtime()?;
            let (key, state) = self.stored_state(key, data)?;
            Ok((key, state, self.cell.envref(py)?))
        })();
        let (key, state, envref) = prepared.map_err(|e| error_to_pyerr(py, &e))?;
        awaitable(
            py,
            &self.cell.shut,
            async move { envref.get_asset_manager().set_state(&key, state).await },
            ToPy::Unit,
        )
    }

    /// The decorator bound to this environment: `@env.command`, or `@env.command(name=…)`.
    #[pyo3(signature = (r#fn=None, **options))]
    fn command(
        slf: &Bound<'_, Self>,
        r#fn: Option<&Bound<'_, PyAny>>,
        options: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Py<PyAny>> {
        bound_decorator(slf, false, r#fn, options)
    }

    /// The source-command decorator bound to this environment: `@env.first_command`.
    #[pyo3(signature = (r#fn=None, **options))]
    fn first_command(
        slf: &Bound<'_, Self>,
        r#fn: Option<&Bound<'_, PyAny>>,
        options: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Py<PyAny>> {
        bound_decorator(slf, true, r#fn, options)
    }

    /// Called by the decorators in `liquers._commands`; returns what the command replaced and the
    /// declaration's warnings.
    fn _register_command(
        &self,
        py: Python<'_>,
        callable: Py<PyAny>,
        baseline: &Bound<'_, PyAny>,
        declaration: &Bound<'_, PyAny>,
        context_index: Option<usize>,
        implementation: Option<Vec<u8>>,
    ) -> PyResult<(&'static str, Vec<String>)> {
        let result = (|| {
            let baseline = py_to_json(py, baseline)?;
            let declaration = py_to_json(py, declaration)?;
            let is_async = is_coroutine_function(py, callable.bind(py))?;
            let (spec, warnings) = PyCommandSpec::new(
                callable,
                baseline,
                &declaration,
                context_index,
                implementation.as_deref(),
                self.cell.conversion,
                self.cell.pickle,
                self.cell.shut.clone(),
                is_async.then(|| self.cell.loops.clone()),
            )?;
            Ok((self.cell.register(py, spec)?, warnings))
        })();
        result
            .map(|(replaced, warnings)| (replaced.as_str(), warnings))
            .map_err(|e| error_to_pyerr(py, &e))
    }

    /// Removes a command, Python or built-in. Returns whether there was one.
    #[pyo3(signature = (name, namespace=""))]
    fn unregister(&self, py: Python<'_>, name: &str, namespace: &str) -> PyResult<bool> {
        self.cell
            .unregister(py, CommandKey::new("", namespace, name))
            .map_err(|e| error_to_pyerr(py, &e))
    }

    /// The metadata of a registered command, as the built environment holds it.
    #[pyo3(signature = (name, namespace=""))]
    fn command_metadata(
        &self,
        py: Python<'_>,
        name: &str,
        namespace: &str,
    ) -> PyResult<crate::command_metadata::CommandMetadata> {
        let result = (|| {
            let envref = self.cell.envref(py)?;
            let key = CommandKey::new("", namespace, name);
            envref
                .get_command_metadata_registry()
                .get(key.clone())
                .cloned()
                .map(crate::command_metadata::CommandMetadata)
                .ok_or_else(|| Error::general_error(format!("no command {key} is registered")))
        })();
        result.map_err(|e| error_to_pyerr(py, &e))
    }

    /// The number of Python callables held by every environment's registrations (test-only).
    #[cfg(feature = "debug-handles")]
    #[staticmethod]
    fn live_handle_count() -> usize {
        crate::pycall::live_handle_count()
    }

    /// Imports command metadata from YAML: a document with a `commands` list.
    ///
    /// Problems are reported by `validate()`, and make the next evaluation raise
    /// `ConfigurationError`; only an unparseable document raises here.
    fn import_metadata(&self, py: Python<'_>, yaml: &str) -> PyResult<()> {
        let result = serde_yaml::from_str::<ImportedMetadata>(yaml)
            .map_err(|e| Error::configuration_error(format!("invalid command metadata: {e}")))
            .and_then(|imported| self.cell.import_metadata(py, imported.commands));
        result.map_err(|e| error_to_pyerr(py, &e))
    }

    /// Every issue the next build would meet, without building.
    fn validate(&self, py: Python<'_>) -> PyResult<Vec<Issue>> {
        match self.cell.validate(py) {
            Ok(report) => Ok(report.issues().iter().map(Issue::from).collect()),
            Err(e) => Err(error_to_pyerr(py, &e)),
        }
    }

    /// Stops the environment's background work. A second call does nothing; any later use raises
    /// `GeneralError`.
    fn shutdown(&self, py: Python<'_>) -> PyResult<()> {
        refuse_shutdown_on_loop()?;
        self.cell.shutdown(py).map_err(|e| error_to_pyerr(py, &e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs `f` attached to an interpreter, as every `EnvCell` method expects.
    fn attached<T>(f: impl for<'py> FnOnce(Python<'py>) -> Result<T, Error>) -> Result<T, Error> {
        Python::initialize();
        Python::attach(f)
    }

    fn new_cell() -> Result<EnvCell, Error> {
        EnvCell::new(EnvironmentConfig::default(), ConversionMode::Permissive, false, NonZeroUsize::MIN)
    }

    #[test]
    fn a_fresh_environment_evaluates_a_builtin() -> Result<(), Error> {
        attached(|py| {
            let cell = new_cell()?;
            let envref = cell.envref(py)?;
            let value = py.detach(|| {
                runtime()?.block_on(async move {
                    let asset = envref.evaluate(parse_query("commands_doc")?).await?;
                    asset.get().await
                })
            })?;
            assert!(value.try_into_string()?.contains("to_text"));
            Ok(())
        })
    }

    #[test]
    fn a_rebuild_keeps_the_store_and_retires_the_old_environment() -> Result<(), Error> {
        attached(|py| {
            let cell = new_cell()?;
            let first = cell.envref(py)?;
            cell.invalidate(py)?;
            let second = cell.envref(py)?;
            assert!(Arc::ptr_eq(&first.get_async_store(), &second.get_async_store()));
            assert_eq!(cell.lock(py)?.retired.len(), 1);
            Ok(())
        })
    }

    #[test]
    fn shutdown_is_idempotent_and_later_use_fails() -> Result<(), Error> {
        attached(|py| {
            let cell = new_cell()?;
            cell.envref(py)?;
            cell.shutdown(py)?;
            cell.shutdown(py)?;
            assert!(cell.envref(py).is_err());
            assert!(cell.invalidate(py).is_err());
            Ok(())
        })
    }

    #[test]
    fn imported_metadata_for_an_unregistered_command_is_an_error() -> Result<(), Error> {
        attached(|py| {
            let cell = new_cell()?;
            let mut ghost = CommandMetadata::new("ghost");
            ghost.label = "Ghost".to_string();
            cell.import_metadata(py, vec![ghost])?;
            let report = cell.validate(py)?;
            assert!(report.has_errors());
            let err = match cell.envref(py) {
                Ok(_) => panic!("a build with an invalid import succeeded"),
                Err(e) => e,
            };
            assert_eq!(err.error_type, liquers_core::error::ErrorType::ConfigurationError);
            Ok(())
        })
    }

    /// A thread waiting for the state lock must not keep the interpreter: here the holder needs
    /// to attach before it can release the lock. With a plain `Mutex::lock` the waiter holds the
    /// GIL while it blocks, and on a GIL build the two deadlock (AC-8, audit finding 2).
    #[test]
    fn lock_is_taken_detached() -> Result<(), Box<dyn std::error::Error>> {
        Python::initialize();
        let cell = Arc::new(new_cell()?);
        let (held_tx, held_rx) = std::sync::mpsc::channel();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let holder = {
            let cell = cell.clone();
            let done_tx = done_tx.clone();
            std::thread::spawn(move || {
                let guard = cell.declarations.lock();
                let _ = held_tx.send(());
                std::thread::sleep(Duration::from_millis(200));
                Python::attach(|_py| ());
                drop(guard);
                let _ = done_tx.send("holder");
            })
        };
        held_rx.recv_timeout(Duration::from_secs(10))?;
        let waiter = {
            let cell = cell.clone();
            std::thread::spawn(move || {
                let validated = Python::attach(|py| cell.validate(py).is_ok());
                let _ = done_tx.send(if validated { "waiter" } else { "waiter failed" });
            })
        };
        let mut finished = vec![
            done_rx.recv_timeout(Duration::from_secs(10))?,
            done_rx.recv_timeout(Duration::from_secs(10))?,
        ];
        finished.sort_unstable();
        assert_eq!(finished, vec!["holder", "waiter"]);
        holder.join().map_err(|_| "holder panicked")?;
        waiter.join().map_err(|_| "waiter panicked")?;
        Ok(())
    }

    /// AC-5 regression guard: a query that needs no Python finishes while another thread holds
    /// the GIL. It passes today; it is here so a change that makes the runtime wait on the GIL
    /// fails. A free-threaded build has no GIL to hold, so the check is skipped there.
    #[test]
    fn pure_rust_query_completes_while_gil_is_held() -> Result<(), Box<dyn std::error::Error>> {
        Python::initialize();
        let cell = new_cell()?;
        let envref = Python::attach(|py| cell.envref(py))?;
        Python::attach(|py| -> Result<(), Box<dyn std::error::Error>> {
            let free_threaded = py
                .import("sysconfig")?
                .call_method1("get_config_var", ("Py_GIL_DISABLED",))?
                .is_truthy()?;
            if free_threaded {
                eprintln!("skipped: a free-threaded build has no GIL to hold");
                return Ok(());
            }
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let evaluated = runtime().and_then(|runtime| {
                    runtime.block_on(async move {
                        let asset = envref.evaluate(parse_query("commands_doc")?).await?;
                        asset.get().await
                    })
                });
                let _ = tx.send(evaluated.is_ok());
            });
            // This thread stays attached for the whole wait.
            assert!(rx.recv_timeout(Duration::from_secs(10))?);
            Ok(())
        })
    }
}
