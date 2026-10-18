//! `lq.Environment`: the Python handle on a Liquers environment.
//!
//! # Retained declarations, lazy build, rebuild on change
//!
//! A built `DefaultEnvironment` is frozen: its command registry cannot change once it is shared.
//! Python, though, registers commands whenever a module is imported — before or after the first
//! evaluation. So the handle keeps every declaration it receives (configuration, imported
//! metadata and, from step 7, the Python command specs), builds the environment from them on first
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

use std::sync::{Arc, Mutex, MutexGuard};

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
use pyo3::types::PyString;

use crate::error::error_to_pyerr;
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
    /// Command metadata from `import_metadata`, applied after the built-ins.
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
        drop(declarations);
        for envref in envrefs {
            envref.get_asset_manager().shutdown();
        }
        Ok(())
    }
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
    fn evaluate(&self, py: Python<'_>, query: &str) -> PyResult<Py<PyAny>> {
        let result = (|| {
            let query = parse_query(query)?;
            let envref = self.cell.envref()?;
            let value = block_on(py, async move {
                let asset = envref.evaluate(query).await?;
                let state = asset.get().await?;
                Ok(state.data_unchecked().clone())
            })?;
            value_to_py(py, &value)
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
