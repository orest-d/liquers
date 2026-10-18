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

use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use liquers_core::command_metadata::{CommandKey, CommandMetadata};
use liquers_core::commands::CommandRegistry;
use liquers_core::context::EnvRef;
use liquers_core::environment_builder::EnvironmentBuilder;
use liquers_core::environment_config::EnvironmentConfig;
use liquers_core::error::Error;
use liquers_core::issue_report::{Issue as CoreIssue, IssueReport, IssueSeverity};
use liquers_core::metadata::{Metadata, MetadataRecord};
use liquers_core::parse::{parse_key, parse_query};
use liquers_core::recipes::RecipeProviderChoice;
use liquers_core::assets::AssetManager;
use liquers_core::state::State;
use liquers_core::store::{AsyncMemoryStore, AsyncStore};
use liquers_core::query::Key;
use liquers_core::type_system::TypeRegistry;
use liquers_core::value::{DefaultValueSerializer, ValueInterface};
use liquers_lib::environment::{default_environment_builder, DefaultEnvironment, LibKind};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyString};

use crate::command::{register_py_command, PyCommandSpec, Replaced};
use crate::error::{error_to_pyerr, pyerr_to_error};
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
}

impl EnvCell {
    pub fn new(config: EnvironmentConfig, conversion: ConversionMode, pickle: bool) -> Result<Self, Error> {
        if config.recipes != RecipeProviderChoice::default() {
            return Err(Error::configuration_error(
                "a recipe provider other than the default cannot be configured from Python yet"
                    .to_string(),
            ));
        }
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
        })
    }

    fn lock(&self) -> Result<MutexGuard<'_, Declarations>, Error> {
        self.declarations
            .lock()
            .map_err(|_| Error::unexpected_error("environment state lock poisoned".to_string()))
    }

    /// The built environment, building it first if a declaration changed.
    pub fn envref(&self) -> Result<EnvRef<PyEnvironment>, Error> {
        let mut declarations = self.lock()?;
        match &declarations.state {
            EnvState::Built(envref) => return Ok(envref.clone()),
            EnvState::Shut => {
                return Err(Error::general_error(
                    "the environment has been shut down".to_string(),
                ))
            }
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
    pub fn invalidate(&self) -> Result<(), Error> {
        let mut declarations = self.lock()?;
        let previous = std::mem::replace(&mut declarations.state, EnvState::Pending);
        match previous {
            EnvState::Built(envref) => declarations.retired.push(envref),
            EnvState::Pending => {}
            EnvState::Shut => {
                declarations.state = EnvState::Shut;
                return Err(Error::general_error(
                    "the environment has been shut down".to_string(),
                ));
            }
        }
        Ok(())
    }

    /// The builder with every declaration applied, but no store yet.
    fn configured_builder(&self, declarations: &Declarations) -> Result<(PyBuilder, IssueReport), Error> {
        let mut types = TypeRegistry::from_value_type::<Value>();
        types.register(py_type_info(self.pickle))?;
        let mut builder = default_environment_builder::<Value, ()>()
            .with_type_registry(types)
            .with_asset_manager_options(self.config.assets.clone());
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
    pub fn validate(&self) -> Result<IssueReport, Error> {
        let declarations = self.lock()?;
        let (mut builder, mut issues) = self.configured_builder(&declarations)?;
        issues.append(builder.validate().clone());
        Ok(issues)
    }

    /// Retains a Python command and marks the environment for a rebuild. Reports what it replaced.
    pub fn register(&self, spec: PyCommandSpec) -> Result<Replaced, Error> {
        let replaced = {
            let mut declarations = self.lock()?;
            if matches!(declarations.state, EnvState::Shut) {
                return Err(Error::general_error(
                    "the environment has been shut down".to_string(),
                ));
            }
            let replaced = if declarations.specs.iter().any(|s| s.key == spec.key) {
                Replaced::Python
            } else if !declarations.unregistered.contains(&spec.key)
                && builtin_keys()?.contains(&spec.key)
            {
                Replaced::Builtin
            } else {
                Replaced::Nothing
            };
            declarations.specs.retain(|s| s.key != spec.key);
            declarations.unregistered.retain(|k| k != &spec.key);
            declarations.specs.push(spec);
            replaced
        };
        self.invalidate()?;
        Ok(replaced)
    }

    /// Removes a command, Python or built-in. `false` when there was nothing to remove.
    pub fn unregister(&self, key: CommandKey) -> Result<bool, Error> {
        let removed = {
            let mut declarations = self.lock()?;
            let before = declarations.specs.len();
            declarations.specs.retain(|s| s.key != key);
            let was_spec = declarations.specs.len() != before;
            let was_builtin =
                !declarations.unregistered.contains(&key) && builtin_keys()?.contains(&key);
            if was_builtin {
                declarations.unregistered.push(key);
            }
            was_spec || was_builtin
        };
        if removed {
            self.invalidate()?;
        }
        Ok(removed)
    }

    pub fn import_metadata(&self, commands: Vec<CommandMetadata>) -> Result<(), Error> {
        self.lock()?.imported.extend(commands);
        self.invalidate()
    }

    /// Stops the current and every retired asset manager. Idempotent.
    pub fn shutdown(&self) -> Result<(), Error> {
        let mut declarations = self.lock()?;
        let previous = std::mem::replace(&mut declarations.state, EnvState::Shut);
        let mut envrefs = std::mem::take(&mut declarations.retired);
        match previous {
            EnvState::Built(envref) => envrefs.push(envref),
            EnvState::Pending | EnvState::Shut => {}
        }
        declarations.store = None;
        // Releases the Python callables now rather than when the handle is collected.
        declarations.specs.clear();
        drop(declarations);
        for envref in envrefs {
            envref.get_asset_manager().shutdown();
        }
        Ok(())
    }
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

/// A Liquers environment: commands, a store and an asset manager.
///
/// Built lazily on first use from everything declared on it; see the module documentation.
#[pyclass(frozen, name = "Environment", module = "liquers")]
pub struct Environment {
    pub cell: Arc<EnvCell>,
}

/// Runs a future on the process runtime with the GIL released.
pub fn block_on<T: Send>(
    py: Python<'_>,
    future: impl std::future::Future<Output = Result<T, Error>> + Send,
) -> Result<T, Error> {
    py.detach(|| runtime()?.block_on(future))
}

#[pymethods]
impl Environment {
    #[new]
    #[pyo3(signature = (*, config=None, conversion="permissive", pickle=false))]
    fn new(
        py: Python<'_>,
        config: Option<&Bound<'_, PyAny>>,
        conversion: &str,
        pickle: bool,
    ) -> PyResult<Self> {
        let built = (|| {
            let config = match config {
                Some(config) => parse_config(py, config)?,
                None => EnvironmentConfig::default(),
            };
            EnvCell::new(config, ConversionMode::parse(conversion)?, pickle)
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
        let result = (|| {
            let query = to_query(query)?;
            let envref = self.cell.envref()?;
            let value = block_on(py, async move {
                let asset = envref.evaluate(query).await?;
                let state = asset.get().await?;
                state.value()
            })?;
            value_to_py(py, &value)
        })();
        result.map_err(|e| error_to_pyerr(py, &e))
    }

    /// Evaluates a query and returns the result as a `State`, metadata included.
    fn evaluate_state(&self, py: Python<'_>, query: &Bound<'_, PyAny>) -> PyResult<Py<crate::asset::State>> {
        let result = (|| {
            let query = to_query(query)?;
            let envref = self.cell.envref()?;
            let state = block_on(py, async move {
                let asset = envref.evaluate(query).await?;
                asset.get().await?.value_state()
            })?;
            crate::asset::State::from_core(py, &state)
        })();
        result.map_err(|e| error_to_pyerr(py, &e))
    }

    /// Starts evaluating a query and returns its `AssetRef` without waiting for the value.
    fn get_asset(&self, py: Python<'_>, query: &Bound<'_, PyAny>) -> PyResult<crate::asset::AssetRef> {
        let result = (|| {
            let query = to_query(query)?;
            let envref = self.cell.envref()?;
            let asset = block_on(py, async move { envref.evaluate(query).await })?;
            Ok(crate::asset::AssetRef::new(asset))
        })();
        result.map_err(|e| error_to_pyerr(py, &e))
    }

    /// Writes a value to the store under `key`.
    ///
    /// The value is serialized here, so a value with no byte form — an opaque Python object with
    /// the pickle codec off — raises `SerializationError` instead of being kept as metadata only
    /// (AC-6). The data format is always recorded, so the read path never has to guess it.
    fn store_set(&self, py: Python<'_>, key: &str, data: &Bound<'_, PyAny>) -> PyResult<()> {
        let result = (|| {
            let key = parse_key(key)?;
            let value = py_to_value(data, self.cell.conversion, self.cell.pickle)?;
            let data_format = value.default_data_format().to_string();
            value.as_bytes(&data_format)?;
            let mut record = MetadataRecord::new();
            record.data_format = Some(data_format);
            let state = State::from_value_and_metadata(
                value,
                Arc::new(Metadata::MetadataRecord(record)),
            );
            let envref = self.cell.envref()?;
            block_on(py, async move {
                envref.get_asset_manager().set_state(&key, state).await
            })
        })();
        result.map_err(|e| error_to_pyerr(py, &e))
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

    /// Called by the decorators in `liquers._commands`; returns what the command replaced.
    fn _register_command(
        &self,
        py: Python<'_>,
        callable: Py<PyAny>,
        baseline: &Bound<'_, PyAny>,
        declaration: &Bound<'_, PyAny>,
        context_index: Option<usize>,
        implementation: Option<Vec<u8>>,
    ) -> PyResult<&'static str> {
        let result = (|| {
            let baseline = py_to_json(py, baseline)?;
            let declaration = py_to_json(py, declaration)?;
            let spec = PyCommandSpec::new(
                callable,
                baseline,
                &declaration,
                context_index,
                implementation.as_deref(),
                self.cell.conversion,
                self.cell.pickle,
            )?;
            self.cell.register(spec)
        })();
        result
            .map(|replaced| replaced.as_str())
            .map_err(|e| error_to_pyerr(py, &e))
    }

    /// Removes a command, Python or built-in. Returns whether there was one.
    #[pyo3(signature = (name, namespace=""))]
    fn unregister(&self, py: Python<'_>, name: &str, namespace: &str) -> PyResult<bool> {
        self.cell
            .unregister(CommandKey::new("", namespace, name))
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
            let envref = self.cell.envref()?;
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
        crate::command::live_handle_count()
    }

    /// Imports command metadata from YAML: a document with a `commands` list.
    ///
    /// Problems are reported by `validate()`, and make the next evaluation raise
    /// `ConfigurationError`; only an unparseable document raises here.
    fn import_metadata(&self, py: Python<'_>, yaml: &str) -> PyResult<()> {
        let result = serde_yaml::from_str::<ImportedMetadata>(yaml)
            .map_err(|e| Error::configuration_error(format!("invalid command metadata: {e}")))
            .and_then(|imported| self.cell.import_metadata(imported.commands));
        result.map_err(|e| error_to_pyerr(py, &e))
    }

    /// Every issue the next build would meet, without building.
    fn validate(&self, py: Python<'_>) -> PyResult<Vec<Issue>> {
        match self.cell.validate() {
            Ok(report) => Ok(report.issues().iter().map(Issue::from).collect()),
            Err(e) => Err(error_to_pyerr(py, &e)),
        }
    }

    /// Stops the environment's background work. A second call does nothing; any later use raises
    /// `GeneralError`.
    fn shutdown(&self, py: Python<'_>) -> PyResult<()> {
        self.cell.shutdown().map_err(|e| error_to_pyerr(py, &e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_environment_evaluates_a_builtin() -> Result<(), Error> {
        let cell = EnvCell::new(EnvironmentConfig::default(), ConversionMode::Permissive, false)?;
        let envref = cell.envref()?;
        let value = runtime()?.block_on(async move {
            let asset = envref.evaluate(parse_query("commands_doc")?).await?;
            asset.get().await
        })?;
        assert!(value.try_into_string()?.contains("to_text"));
        Ok(())
    }

    #[test]
    fn a_rebuild_keeps_the_store_and_retires_the_old_environment() -> Result<(), Error> {
        let cell = EnvCell::new(EnvironmentConfig::default(), ConversionMode::Permissive, false)?;
        let first = cell.envref()?;
        cell.invalidate()?;
        let second = cell.envref()?;
        assert!(Arc::ptr_eq(&first.get_async_store(), &second.get_async_store()));
        assert_eq!(cell.lock()?.retired.len(), 1);
        Ok(())
    }

    #[test]
    fn shutdown_is_idempotent_and_later_use_fails() -> Result<(), Error> {
        let cell = EnvCell::new(EnvironmentConfig::default(), ConversionMode::Permissive, false)?;
        cell.envref()?;
        cell.shutdown()?;
        cell.shutdown()?;
        assert!(cell.envref().is_err());
        assert!(cell.invalidate().is_err());
        Ok(())
    }

    #[test]
    fn imported_metadata_for_an_unregistered_command_is_an_error() -> Result<(), Error> {
        let cell = EnvCell::new(EnvironmentConfig::default(), ConversionMode::Permissive, false)?;
        let mut ghost = CommandMetadata::new("ghost");
        ghost.label = "Ghost".to_string();
        cell.import_metadata(vec![ghost])?;
        let report = cell.validate()?;
        assert!(report.has_errors());
        let err = match cell.envref() {
            Ok(_) => panic!("a build with an invalid import succeeded"),
            Err(e) => e,
        };
        assert_eq!(err.error_type, liquers_core::error::ErrorType::ConfigurationError);
        Ok(())
    }

    #[test]
    fn a_non_default_recipe_provider_is_refused() {
        let mut config = EnvironmentConfig::default();
        config.recipes = RecipeProviderChoice::Trivial;
        assert!(EnvCell::new(config, ConversionMode::Permissive, false).is_err());
    }
}
