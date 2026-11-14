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
//! hanging on an asset manager that no longer runs. Leaving the wait for a query does not cancel
//! its asset, which others may share; a pipeline's or a render's asset is private to the call, so
//! leaving that wait cancels it (`closure.rs`).
//!
//! # Main components
//!
//! - [`Environment`] (`lq.Environment`): evaluation of queries and pipelines, the store, command
//!   registration, renders (`render`, `expand`, `validate_template`, `template_command`),
//!   pipelines (`pipeline_command`, `pipeline_editor`, `query_editor`), data nodes, shutdown.
//! - [`to_evaluable`], [`evaluate_evaluable`], [`pipeline_recipe`]: what `evaluate` takes, and how
//!   a pipeline runs (a pipeline-only recipe through `apply_queued`).
//! - [`EnvCell`]: the retained declarations, the lazy build and the rebuild, and the current
//!   build the servers read.
//! - [`crate::config::PyConfig`]: what `config=` and the keywords resolve to; `serve` starts a
//!   server over the environment (`server.rs`).
//! - [`block_on`]: the sliced, GIL-free wait every blocking call uses.
//!
//! # Where it is used
//!
//! - Every Python entry point that needs an environment: `lq.Environment`, the default
//!   environment of `python/liquers/_env.py`, and the factories of `store/` and
//!   `recipe_provider.rs`.
//! - [`crate::server::CellSource`], which answers HTTP requests from the current build.
//!
//! # Related concepts
//!
//! - `command.rs` and `declared.rs`: the commands the declarations hold.
//! - `template.rs`, `closure.rs`: the top-level render and its wait.
//! - [`crate::config`] parses the configuration document; this module applies it.
//! - `specs/reference/PYTHON_INTEGRATION.md` §3.
//!
//! # Examples
//!
//! ```python
//! env = lq.Environment()
//! env.store_set("templates/greet.md", "Hello, {{.}}!")
//! env.template_command("greeting", template="-R/templates/greet.md")
//! assert env.render(lq.parse("-R/templates/greet.md"), state="Ann") == "Hello, Ann!"
//! env.shutdown()
//! ```
//!
//! Executed in `liquers-py/tests/test_ENVIRON_*.py` and `test_TEMPLATE_*.py`.

use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, RwLock};

use pyo3::sync::MutexExt;
use std::time::Duration;

use liquers_core::command_metadata::{CommandKey, CommandMetadata};
use liquers_core::commands::CommandRegistry;
use liquers_core::context::EnvRef;
use liquers_core::environment_builder::EnvironmentBuilder;
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
use liquers_lib::ui::SimpleUIPayload;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyString};

use crate::aio::{awaitable, check_blocking_call, refuse_shutdown_on_loop, shut_down_while_waiting, LazyLoopPool, Shut, ToPy};
use crate::command::{register_py_command, PyCommandSpec, Replaced};
use crate::config::PyConfig;
use crate::server::{ServerSlot, SHUTDOWN_DEADLINE};
use crate::store::factory::{PyStoreFactory, PyStoreType};
use crate::recipe_provider::{PyRecipeProviderFactory, PyRecipeProviderType};
use liquers_core::store_factory::{ChainedStoreFactory, StoreFactory, StoreTypeInfo};
use crate::error::{error_to_pyerr, pyerr_to_error, PyFailure};
use crate::runtime::runtime;
use crate::records::Scope;
use crate::value::{py_to_value, py_type_info, value_to_py, ConversionMode, Value};

/// The environment every Python handle builds.
pub type PyEnvironment = DefaultEnvironment<Value, SimpleUIPayload>;

type PyBuilder = EnvironmentBuilder<Value, SimpleUIPayload, LibKind>;

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
    /// Commands defined by declaration (template commands), in registration order; at most one
    /// per key, and never a key a Python command has.
    declared: Vec<crate::declared::DeclaredCommand>,
    /// Command metadata from `import_metadata`, applied last, so it can describe any command.
    imported: Vec<CommandMetadata>,
    /// Python store types, in registration order (`register_store_factory`).
    store_types: Vec<Arc<PyStoreType>>,
    /// Python recipe provider types, in registration order (`register_recipe_provider_factory`).
    recipe_types: Vec<Arc<PyRecipeProviderType>>,
}

/// The shared state behind an `lq.Environment`.
pub struct EnvCell {
    declarations: Mutex<Declarations>,
    /// The whole configuration document, parsed at construction, so an unknown section or key
    /// fails at `lq.Environment` (`crate::config`).
    pub config: PyConfig,
    /// The built environment, readable without the GIL (a server reads it per request). Written
    /// only while the declarations lock is held: set by `envref`, cleared by `retire` and
    /// `shutdown`.
    current: RwLock<Option<EnvRef<PyEnvironment>>>,
    /// The servers started from this environment; stopped ones are pruned on each access.
    servers: Mutex<ServerList>,
    /// The UI models made by `ui()`, disposed by `shutdown()` after the servers.
    uis: Mutex<crate::ui::UiList>,
    pub conversion: ConversionMode,
    pub pickle: bool,
    /// Fired by `shutdown()`; every wait started from this handle watches it.
    pub shut: Arc<Shut>,
    /// The loops `async def` commands run on, shared by every rebuild.
    pub loops: Arc<LazyLoopPool>,
}

impl EnvCell {
    /// A handle on `config`. The command metadata registry file it names is read here, so a
    /// missing or unparsable file raises at `lq.Environment`; its commands are applied to every
    /// build as `import_metadata`'s are.
    pub fn new(
        config: PyConfig,
        conversion: ConversionMode,
        pickle: bool,
        async_loops: NonZeroUsize,
    ) -> Result<Self, Error> {
        let imported = match &config.command_registry {
            Some(path) => read_command_registry(path)?,
            None => Vec::new(),
        };
        Ok(EnvCell {
            declarations: Mutex::new(Declarations {
                state: EnvState::Pending,
                store: None,
                retired: Vec::new(),
                specs: Vec::new(),
                unregistered: Vec::new(),
                declared: Vec::new(),
                imported,
                store_types: Vec::new(),
                recipe_types: Vec::new(),
            }),
            config,
            current: RwLock::new(None),
            servers: Mutex::new(ServerList::default()),
            uis: Mutex::new(crate::ui::UiList::default()),
            conversion,
            pickle,
            shut: Arc::new(Shut::default()),
            loops: Arc::new(LazyLoopPool::new(async_loops)),
        })
    }

    fn lock(&self, py: Python<'_>) -> Result<MutexGuard<'_, Declarations>, Error> {
        // A factory runs while its environment is built, under this lock: using the environment
        // from it would wait for itself.
        if is_building(self) {
            return Err(Error::configuration_error(
                "a factory may not use its environment while the environment is being built".to_string(),
            ));
        }
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
        let envref = {
            let _building = BuildGuard::enter(self);
            self.build(&declarations)?
        };
        if declarations.store.is_none() {
            declarations.store = Some(envref.get_async_store());
        }
        declarations.state = EnvState::Built(envref.clone());
        self.set_current(Some(envref.clone()));
        Ok(envref)
    }

    /// The built environment without taking the GIL or the declarations lock: `None` before the
    /// first build, after a registration retired it, and after `shutdown()`.
    pub fn current_without_gil(&self) -> Result<Option<EnvRef<PyEnvironment>>, Error> {
        match self.current.read() {
            Ok(current) => Ok(current.clone()),
            Err(_) => Err(Error::unexpected_error("environment current-build lock poisoned".to_string())),
        }
    }

    fn set_current(&self, envref: Option<EnvRef<PyEnvironment>>) {
        // Dropped after the write guard: the old build's last reference may run long drops.
        let previous = match self.current.write() {
            Ok(mut current) => std::mem::replace(&mut *current, envref),
            Err(poisoned) => std::mem::replace(&mut *poisoned.into_inner(), envref),
        };
        drop(previous);
    }

    /// Records a server started from this environment, or refuses it once `shutdown()` has
    /// taken the list: a server bound while the environment was shutting down is the caller's to
    /// stop, since `shutdown()` no longer sees it.
    pub(crate) fn add_server(&self, slot: Arc<ServerSlot>) -> Result<(), Error> {
        let mut servers = lock_servers(&self.servers);
        if servers.closed {
            return Err(shut_down_error());
        }
        servers.slots.retain(|s| s.server.is_running());
        servers.slots.push(slot);
        Ok(())
    }

    /// Records a UI model made from this environment, or refuses it after `shutdown()`.
    pub(crate) fn add_ui(&self, model: std::sync::Weak<crate::ui::UiModel>) -> Result<(), Error> {
        match self.uis.lock() {
            Ok(mut uis) => uis.add(model),
            Err(poisoned) => poisoned.into_inner().add(model),
        }
    }

    /// The running servers started from this environment.
    pub(crate) fn running_servers(&self) -> Vec<Arc<ServerSlot>> {
        let mut servers = lock_servers(&self.servers);
        servers.slots.retain(|s| s.server.is_running());
        servers.slots.clone()
    }

    /// Marks the environment for a rebuild after a declaration changed.
    ///
    /// Fails after `shutdown()`, so a registration on a shut environment is not silently dropped.
    pub fn invalidate(&self, py: Python<'_>) -> Result<(), Error> {
        retire(self, &mut *self.lock(py)?)
    }

    /// The builder with every declaration applied, but no store yet.
    fn configured_builder(&self, declarations: &Declarations) -> Result<(PyBuilder, IssueReport), Error> {
        let mut types = TypeRegistry::from_value_type::<Value>();
        types.register(py_type_info(self.pickle))?;
        let mut builder = default_environment_builder::<Value, SimpleUIPayload>()
            .with_type_registry(types)
            .with_asset_manager_options(self.config.environment.assets.clone())
            .with_recipes(self.config.environment.recipes.clone())
            .with_recipe_provider_factory(Box::new(PyRecipeProviderFactory {
                types: declarations.recipe_types.clone(),
                loops: self.loops.clone(),
                shut: self.shut.clone(),
            }));
        // Fields are applied one by one rather than through `with_config`, so a new field of
        // `EnvironmentConfig` has to be listed here too.
        if let Some(namespaces) = &self.config.environment.default_namespaces {
            builder = builder.with_default_namespaces(namespaces.clone());
        }
        crate::builtins::register(&mut builder.command_registry)?;
        for key in &declarations.unregistered {
            builder.command_registry.unregister(key.clone());
        }
        for spec in &declarations.specs {
            register_py_command(&mut builder.command_registry, spec)?;
        }
        for declared in &declarations.declared {
            declared.apply(&mut builder.command_registry)?;
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
            None if self.config.environment.store.stores.is_empty() => {
                builder.with_async_store(Arc::new(AsyncMemoryStore::new(&Key::new())))
            }
            None => builder.with_store_config(
                self.config.environment.store.clone(),
                Box::new(self.store_factory(declarations)),
            ),
        };
        // The asset manager spawns its workers on the ambient runtime.
        let _guard = runtime()?.enter();
        builder.build()
    }

    /// The store factory chain: the built-in types, then the Python ones.
    fn store_factory(&self, declarations: &Declarations) -> ChainedStoreFactory {
        ChainedStoreFactory::new()
            .chain(Box::new(liquers_store::store_factory::default_store_factory()))
            .chain(Box::new(PyStoreFactory {
                types: declarations.store_types.clone(),
                loops: self.loops.clone(),
                shut: self.shut.clone(),
            }))
    }

    /// The store types a configuration can name: the built-in ones, then the Python ones.
    pub fn store_types(&self, py: Python<'_>) -> Result<Vec<StoreTypeInfo>, Error> {
        let declarations = self.lock(py)?;
        refuse_if_shut(&declarations)?;
        let builtin = liquers_store::store_factory::default_store_factory().store_types();
        let python = declarations.store_types.iter().map(|t| t.info.clone());
        Ok(builtin.into_iter().chain(python).collect())
    }

    /// Registers a Python store type; refused when the name is taken. Marks the environment for a
    /// rebuild, which keeps the store if one was built (so the type serves only a configuration
    /// whose earlier build failed for want of it).
    pub(crate) fn register_store_type(&self, py: Python<'_>, store_type: PyStoreType) -> Result<(), Error> {
        let name = store_type.info.store_type.clone();
        let mut declarations = self.lock(py)?;
        refuse_if_shut(&declarations)?;
        let builtin = liquers_store::store_factory::default_store_factory().store_types();
        if builtin.iter().any(|t| t.store_type == name) || name.starts_with("opendal_") {
            return Err(Error::configuration_error(format!(
                "store type '{name}' is a built-in store type; choose another name"
            )));
        }
        if declarations.store_types.iter().any(|t| t.info.store_type == name) {
            return Err(Error::configuration_error(format!(
                "store type '{name}' is already registered"
            )));
        }
        declarations.store_types.push(Arc::new(store_type));
        retire(self, &mut declarations)
    }

    /// Registers a Python recipe provider type; refused when the name is one of the kind's entry
    /// types, a keyword, or registered before. Marks the environment for a rebuild.
    pub(crate) fn register_recipe_provider_type(&self, py: Python<'_>, provider_type: PyRecipeProviderType) -> Result<(), Error> {
        let name = provider_type.info.provider_type.clone();
        if crate::recipe_provider::reserved_names().iter().any(|n| *n == name) {
            return Err(Error::configuration_error(format!(
                "recipe provider type '{name}' is a built-in entry type or keyword; choose another name"
            )));
        }
        let mut declarations = self.lock(py)?;
        refuse_if_shut(&declarations)?;
        if declarations.recipe_types.iter().any(|t| t.info.provider_type == name) {
            return Err(Error::configuration_error(format!(
                "recipe provider type '{name}' is already registered"
            )));
        }
        declarations.recipe_types.push(Arc::new(provider_type));
        retire(self, &mut declarations)
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
        } else if declarations.declared.iter().any(|d| d.key() == &spec.key) {
            Replaced::Declared
        } else if !declarations.unregistered.contains(&spec.key)
            && builtin_keys()?.contains(&spec.key)
        {
            Replaced::Builtin
        } else {
            Replaced::Nothing
        };
        take_specs(&mut declarations.specs, &spec.key, &mut removed);
        declarations.declared.retain(|d| d.key() != &spec.key);
        declarations.unregistered.retain(|k| k != &spec.key);
        declarations.specs.push(spec);
        retire(self, &mut declarations)?;
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
        let declared_before = declarations.declared.len();
        declarations.declared.retain(|d| d.key() != &key);
        let removed = if !taken.is_empty() || declarations.declared.len() != declared_before {
            true
        } else if !declarations.unregistered.contains(&key) && builtin_keys()?.contains(&key) {
            declarations.unregistered.push(key);
            true
        } else {
            false
        };
        if removed {
            retire(self, &mut declarations)?;
        }
        Ok(removed)
    }

    /// Declares a command (a template command), replacing any command under its key. The next
    /// build is tried first: an error raises and leaves every declaration as it was.
    pub(crate) fn declare(&self, py: Python<'_>, command: crate::declared::DeclaredCommand) -> Result<Replaced, Error> {
        let key = command.key().clone();
        let mut removed: Vec<PyCommandSpec> = Vec::new();
        let mut declarations = self.lock(py)?;
        refuse_if_shut(&declarations)?;
        let replaced = if declarations.specs.iter().any(|s| s.key == key) {
            Replaced::Python
        } else if declarations.declared.iter().any(|d| d.key() == &key) {
            Replaced::Declared
        } else if !declarations.unregistered.contains(&key) && builtin_keys()?.contains(&key) {
            Replaced::Builtin
        } else {
            Replaced::Nothing
        };
        let previous = declarations.declared.clone();
        declarations.declared.retain(|d| d.key() != &key);
        declarations.declared.push(command);
        // A declared command is applied after the Python ones and replaces one with its key, so
        // the trial needs no change to `specs`.
        let trial = (|| {
            let (mut builder, mut issues) = self.configured_builder(&declarations)?;
            issues.append(builder.validate().clone());
            match issues.short_summary("Command metadata") {
                Some(summary) => Err(Error::configuration_error(summary)),
                None => Ok(()),
            }
        })();
        if let Err(error) = trial {
            declarations.declared = previous;
            return Err(error);
        }
        take_specs(&mut declarations.specs, &key, &mut removed);
        declarations.unregistered.retain(|k| k != &key);
        retire(self, &mut declarations)?;
        Ok(replaced)
    }

    pub fn import_metadata(&self, py: Python<'_>, commands: Vec<CommandMetadata>) -> Result<(), Error> {
        let mut declarations = self.lock(py)?;
        refuse_if_shut(&declarations)?;
        declarations.imported.extend(commands);
        retire(self, &mut declarations)
    }

    /// Stops the current and every retired asset manager, then the loop pool. Idempotent.
    ///
    /// The asset managers stop taking work first; stopping the pool then cancels the coroutines
    /// still running, since a stopped asset manager does not cancel running assets.
    pub fn shutdown(&self, py: Python<'_>) -> Result<(), Error> {
        // The servers first, while the environment still answers: their requests in flight may
        // finish (up to the deadline) before the waits are cancelled. Closing the list under its
        // lock makes a server still binding (`serve_async`, another thread) stop itself instead.
        let servers = {
            let mut list = lock_servers(&self.servers);
            list.closed = true;
            std::mem::take(&mut list.slots)
        };
        if !servers.is_empty() {
            py.detach(|| {
                for slot in &servers {
                    slot.stop_blocking(SHUTDOWN_DEADLINE);
                }
            });
        }
        drop(servers);
        // Then the UI models: their tasks stop before the asset managers they evaluate with.
        let uis = match self.uis.lock() {
            Ok(mut uis) => uis.close(),
            Err(poisoned) => poisoned.into_inner().close(),
        };
        for model in &uis {
            model.dispose(py);
        }
        drop(uis);
        self.shut.fire();
        let specs: Vec<PyCommandSpec>;
        let mut declarations = self.lock(py)?;
        let previous = std::mem::replace(&mut declarations.state, EnvState::Shut);
        self.set_current(None);
        let mut envrefs = std::mem::take(&mut declarations.retired);
        match previous {
            EnvState::Built(envref) => envrefs.push(envref),
            EnvState::Pending | EnvState::Shut => {}
        }
        let store = declarations.store.take();
        // Releases the Python callables now rather than when the handle is collected, but only
        // once the lock is released.
        specs = std::mem::take(&mut declarations.specs);
        let store_types = std::mem::take(&mut declarations.store_types);
        let recipe_types = std::mem::take(&mut declarations.recipe_types);
        drop(declarations);
        drop(specs);
        drop(store_types);
        drop(recipe_types);
        drop(store);
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

thread_local! {
    /// The environments being built on this thread, by address.
    static BUILDING: std::cell::RefCell<Vec<usize>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Marks an environment as being built on this thread until dropped.
struct BuildGuard(usize);

impl BuildGuard {
    fn enter(cell: &EnvCell) -> Self {
        let id = cell as *const EnvCell as usize;
        BUILDING.with(|building| building.borrow_mut().push(id));
        BuildGuard(id)
    }
}

impl Drop for BuildGuard {
    fn drop(&mut self) {
        BUILDING.with(|building| {
            let mut building = building.borrow_mut();
            if let Some(index) = building.iter().rposition(|id| *id == self.0) {
                building.remove(index);
            }
        });
    }
}

fn is_building(cell: &EnvCell) -> bool {
    let id = cell as *const EnvCell as usize;
    BUILDING.with(|building| building.borrow().contains(&id))
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

/// Marks the environment for a rebuild, keeping the built one on the retired list. Called with
/// the declarations lock held, so `current` is cleared in step with the state.
fn retire(cell: &EnvCell, declarations: &mut Declarations) -> Result<(), Error> {
    refuse_if_shut(declarations)?;
    match std::mem::replace(&mut declarations.state, EnvState::Pending) {
        EnvState::Built(envref) => declarations.retired.push(envref),
        EnvState::Pending | EnvState::Shut => {}
    }
    cell.set_current(None);
    Ok(())
}

/// An environment's servers, and whether `shutdown()` has closed the list to new ones.
#[derive(Default)]
struct ServerList {
    closed: bool,
    slots: Vec<Arc<ServerSlot>>,
}

fn lock_servers(servers: &Mutex<ServerList>) -> MutexGuard<'_, ServerList> {
    match servers.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// The commands of a command metadata registry file (`specs/command_registry.yaml`'s format;
/// only `commands` is read), as `import_metadata` takes them.
fn read_command_registry(path: &std::path::Path) -> Result<Vec<CommandMetadata>, Error> {
    let text = std::fs::read_to_string(path).map_err(|e| {
        Error::general_error(format!("cannot read the command registry file {}: {e}", path.display()))
    })?;
    serde_yaml::from_str::<ImportedMetadata>(&text)
        .map(|imported| imported.commands)
        .map_err(|e| {
            Error::configuration_error(format!(
                "the command registry file {} is not a command metadata registry: {e}",
                path.display()
            ))
        })
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

/// Starts evaluating `evaluable`: a query as `EnvRef::evaluate`, a pipeline as a pipeline-only
/// recipe through the job queue.
pub(crate) async fn evaluate_evaluable(
    envref: &EnvRef<PyEnvironment>,
    evaluable: Evaluable,
) -> Result<liquers_core::assets::AssetRef<PyEnvironment>, Error> {
    match evaluable {
        Evaluable::Query(query) => envref.evaluate(query).await,
        Evaluable::Pipeline(pipeline) => {
            envref
                .get_asset_manager()
                .apply_queued(pipeline_recipe(pipeline), liquers_core::state::State::new())
                .await
        }
    }
}

/// `get_asset` takes queries only.
fn refuse_pipeline(object: &Bound<'_, PyAny>) -> PyResult<()> {
    if object.cast::<crate::pipeline::Pipeline>().is_ok()
        || object.is_instance_of::<pyo3::types::PyList>()
        || object.is_instance_of::<pyo3::types::PyTuple>()
        || object.is_instance_of::<PyDict>()
    {
        return Err(pyo3::exceptions::PyTypeError::new_err(
            "get_asset takes a query; a pipeline's asset is private to one evaluation (use evaluate)",
        ));
    }
    Ok(())
}

/// What `evaluate` takes: a query, or a pipeline.
pub(crate) enum Evaluable {
    Query(liquers_core::query::Query),
    Pipeline(liquers_core::pipeline::Pipeline),
}

/// A `str` or an `lq.Query` is a query; an `lq.Pipeline`, a list, a tuple or a dict is a pipeline.
pub(crate) fn to_evaluable(py: Python<'_>, object: &Bound<'_, PyAny>) -> Result<Evaluable, Error> {
    if object.is_instance_of::<PyString>() || object.cast::<crate::query::Query>().is_ok() {
        return to_query(object).map(Evaluable::Query);
    }
    if object.cast::<crate::pipeline::Pipeline>().is_ok()
        || object.is_instance_of::<pyo3::types::PyList>()
        || object.is_instance_of::<pyo3::types::PyTuple>()
        || object.is_instance_of::<PyDict>()
    {
        return crate::pipeline::pipeline_from_py(object)
            .map(Evaluable::Pipeline)
            .map_err(|e| pyerr_to_error(py, &e));
    }
    to_query(object).map(Evaluable::Query)
}

/// The recipe a pipeline evaluates as: the pipeline alone, with no query.
pub(crate) fn pipeline_recipe(pipeline: liquers_core::pipeline::Pipeline) -> liquers_core::recipes::Recipe {
    liquers_core::recipes::Recipe {
        pipeline: Some(pipeline),
        ..Default::default()
    }
}

/// A JSON-compatible Python object (the decorator's documents) as JSON.
pub(crate) fn py_to_json(py: Python<'_>, object: &Bound<'_, PyAny>) -> Result<serde_json::Value, Error> {
    let text = py
        .import("json")
        .and_then(|json| {
            let options = PyDict::new(py);
            options.set_item("default", wrap_pyfunction!(json_default, py)?)?;
            json.call_method("dumps", (object,), Some(&options))
        })
        .and_then(|dumped| dumped.extract::<String>().map_err(PyErr::from))
        .map_err(|e| pyerr_to_error(py, &e))?;
    serde_json::from_str(&text).map_err(|e| Error::general_error(format!("invalid declaration: {e}")))
}

/// `json.dumps`'s `default=` for the declaration documents: an `lq.Link` default becomes
/// `{"Query": …}`; anything else is refused as `json` refuses it.
#[pyfunction]
fn json_default(object: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
    match object.cast::<crate::query::Link>() {
        Ok(link) => link.call_method0("_declaration").map(Bound::unbind),
        Err(_) => Err(pyo3::exceptions::PyTypeError::new_err(format!(
            "Object of type {} is not JSON serializable",
            object.get_type().name().map(|n| n.to_string()).unwrap_or_else(|_| "?".to_string())
        ))),
    }
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

/// The keywords of `serve` that override the configuration's `server:` section.
struct ServeOverrides {
    host: Option<String>,
    port: Option<u16>,
    prefix: Option<String>,
    apis: Option<Vec<String>>,
    read_only: Option<bool>,
    web: Option<liquers_axum::WebSetting>,
}

/// `web=` as a setting: a bool turns the web folder on or off, a str names its store folder.
fn web_setting(py: Python<'_>, web: Option<&Bound<'_, PyAny>>) -> PyResult<Option<liquers_axum::WebSetting>> {
    let web = match web {
        None => return Ok(None),
        Some(web) if web.is_none() => return Ok(None),
        Some(web) => web,
    };
    if let Ok(enabled) = web.cast::<pyo3::types::PyBool>() {
        return Ok(Some(liquers_axum::WebSetting::Enabled(enabled.is_true())));
    }
    if let Ok(root) = web.cast::<PyString>() {
        return Ok(Some(liquers_axum::WebSetting::Root(root.to_string())));
    }
    Err(error_to_pyerr(
        py,
        &Error::configuration_error("web must be True, False or a store folder (a str)".to_string()),
    ))
}

impl Environment {
    /// The configuration's `server:` section with `serve`'s keywords applied, checked; builds the
    /// environment first, so a broken configuration (or a shut environment) raises here.
    fn server_config(&self, py: Python<'_>, overrides: ServeOverrides) -> PyResult<liquers_axum::ServerConfig> {
        let result = (|| -> Result<_, Error> {
            self.cell.envref(py)?;
            let mut config = self.cell.config.server.clone();
            if let Some(host) = overrides.host {
                config.host = host;
            }
            if let Some(port) = overrides.port {
                config.port = port;
            }
            if let Some(prefix) = overrides.prefix {
                config.prefix = prefix;
            }
            if let Some(apis) = overrides.apis {
                config.apis = apis
                    .iter()
                    .map(|name| {
                        serde_json::from_value::<liquers_axum::Api>(serde_json::Value::String(name.clone())).map_err(|_| {
                            Error::configuration_error(format!(
                                "unknown API '{name}' (known: query, store, assets, recipes)"
                            ))
                        })
                    })
                    .collect::<Result<_, _>>()?;
            }
            if let Some(read_only) = overrides.read_only {
                config.read_only = read_only;
            }
            if let Some(web) = overrides.web {
                config.web = web;
            }
            config.validate()?;
            Ok(config)
        })();
        result.map_err(|e| error_to_pyerr(py, &e))
    }

    /// A top-level render: a closure asset, waited on with the GIL released.
    fn render_top(
        &self,
        py: Python<'_>,
        call: crate::template::Call,
        arguments: crate::template::Arguments<'_, '_>,
        full: bool,
    ) -> PyResult<Py<PyAny>> {
        check_blocking_call(py, call.name())?;
        let request = crate::template::build_request(py, call, arguments, self.cell.conversion, self.cell.pickle)?;
        let render = (|| {
            let envref = self.cell.envref(py)?;
            liquers_lib::template::render_top_level(&envref, request)
        })()
        .map_err(|e| error_to_pyerr(py, &e))?;
        let asset = render.asset().clone();
        let outcome = crate::closure::wait_for_run(py, &self.cell.shut, asset, render.run())
            .map_err(|e| e.into_pyerr(py))?;
        let rendered = outcome
            .ok_or_else(crate::template::not_started)
            .map_err(|e| error_to_pyerr(py, &e))?;
        crate::template::rendered_to_py(py, rendered, full)
    }

    /// The awaitable form of [`Self::render_top`].
    fn render_top_async<'py>(
        &self,
        py: Python<'py>,
        call: crate::template::Call,
        arguments: crate::template::Arguments<'_, 'py>,
        full: bool,
    ) -> PyResult<Bound<'py, PyAny>> {
        let request = crate::template::build_request(py, call, arguments, self.cell.conversion, self.cell.pickle)?;
        let render = (|| {
            runtime()?;
            let envref = self.cell.envref(py)?;
            liquers_lib::template::render_top_level(&envref, request)
        })()
        .map_err(|e| error_to_pyerr(py, &e))?;
        let asset = render.asset().clone();
        crate::closure::awaitable_run(py, &self.cell.shut, asset, render.run(), move |result| {
            ToPy::Rendered(result.map(|rendered| (rendered, full)))
        })
    }

    /// The parsed query and the built environment an `_async` method needs, so that a bad query
    /// or a shut environment raises at call time.
    fn evaluable_and_envref(
        &self,
        py: Python<'_>,
        query: &Bound<'_, PyAny>,
    ) -> PyResult<(Evaluable, EnvRef<PyEnvironment>)> {
        let prepared = (|| -> Result<_, Error> {
            crate::runtime::runtime()?;
            Ok((to_evaluable(py, query)?, self.cell.envref(py)?))
        })();
        prepared.map_err(|e| error_to_pyerr(py, &e))
    }

    /// Submits `pipeline` as a pipeline-only recipe, taking a job-queue slot as `evaluate` does.
    fn start_pipeline(
        &self,
        py: Python<'_>,
        envref: &EnvRef<PyEnvironment>,
        pipeline: liquers_core::pipeline::Pipeline,
    ) -> Result<liquers_core::assets::AssetRef<PyEnvironment>, PyFailure> {
        let envref = envref.clone();
        Ok(block_on(py, &self.cell.shut, async move {
            envref
                .get_asset_manager()
                .apply_queued(pipeline_recipe(pipeline), liquers_core::state::State::new())
                .await
        })?)
    }

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
    /// Renders `template` over `state` (the context, `{{.}}`) and `data` (names looked up above
    /// it) at the top level. The render runs as an asset of its own, as a command would: its query
    /// tags are its dependencies and the errors a recovering `errors` mode handles are warnings in
    /// its log. Returns the text, or with `full=True` an `lq.RenderResult` with the warnings too.
    ///
    /// `template` is the text (`str`) or a link (`lq.Query`, `lq.Key`); relative keys resolve
    /// against `cwd` (an `lq.Key`, default the root). `html=True` escapes substituted text.
    /// `state` is an `lq.State` or any value; `data` reads in place and must be a mapping.
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
        let arguments = crate::template::Arguments { template, state, data, errors, cwd, html };
        self.render_top(py, crate::template::Call::Render, arguments, full)
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
        let arguments = crate::template::Arguments { template, state: None, data, errors, cwd, html };
        self.render_top(py, crate::template::Call::Expand, arguments, full)
    }

    /// Awaitable `render`. Cancelling the awaiting task cancels the render's asset.
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
        let arguments = crate::template::Arguments { template, state, data, errors, cwd, html };
        self.render_top_async(py, crate::template::Call::Render, arguments, full)
    }

    /// Awaitable `expand`. Cancelling the awaiting task cancels the render's asset.
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
        let arguments = crate::template::Arguments { template, state: None, data, errors, cwd, html };
        self.render_top_async(py, crate::template::Call::Expand, arguments, full)
    }

    /// Declares a template command `name`: an alias of `ns-template/render` whose template is the
    /// link `template` (a query `str`, an `lq.Query`, or an `lq.Key` read as `-R/<key>`; never
    /// template text). It needs no function, survives every rebuild, and takes the render's
    /// arguments: `data`, `errors` and `cwd` (default the folder of the input's key). The last
    /// registration under a name wins, whether a Python command or a declared one; a declaration
    /// that would not build raises and changes nothing.
    #[pyo3(signature = (name, template, *, namespace="", label=None, doc=None, filename=None))]
    #[allow(clippy::too_many_arguments)]
    fn template_command(
        &self,
        py: Python<'_>,
        name: &str,
        template: &Bound<'_, PyAny>,
        namespace: &str,
        label: Option<String>,
        doc: Option<String>,
        filename: Option<String>,
    ) -> PyResult<()> {
        let link = if let Ok(key) = template.cast::<crate::query::Key>() {
            liquers_core::query::Query::from(&key.borrow().0)
        } else {
            to_query(template).map_err(|e| match template.cast::<PyString>() {
                Ok(_) => error_to_pyerr(py, &e),
                Err(_) => pyo3::exceptions::PyTypeError::new_err(
                    "a template command's template must be a link: a query str, an lq.Query or an lq.Key",
                ),
            })?
        };
        let overrides = crate::declared::Overrides { label, doc, filename };
        let command = crate::declared::template_command(CommandKey::new("", namespace, name), link, overrides);
        self.cell
            .declare(py, command)
            .map(|_replaced| ())
            .map_err(|e| error_to_pyerr(py, &e))
    }

    /// Declares a pipeline command `name` that runs `body` (an `lq.Pipeline`, a list of elements
    /// or a dict in the pipeline's JSON form). `arguments` name its placeholders, in order: a `str`
    /// is an argument of any type with no default, an `lq.ArgumentInfo` is taken as is. Like
    /// `template_command`, it needs no function, survives every rebuild, the last registration
    /// under a name wins, and a body that does not plan raises and changes nothing.
    #[pyo3(signature = (name, body, *, arguments=Vec::new(), namespace="", label=None, doc=None, filename=None))]
    #[allow(clippy::too_many_arguments)]
    fn pipeline_command(
        &self,
        py: Python<'_>,
        name: &str,
        body: &Bound<'_, PyAny>,
        arguments: Vec<Bound<'_, PyAny>>,
        namespace: &str,
        label: Option<String>,
        doc: Option<String>,
        filename: Option<String>,
    ) -> PyResult<()> {
        let pipeline = crate::pipeline::pipeline_from_py(body)?;
        let arguments = arguments
            .iter()
            .map(|argument| {
                if let Ok(name) = argument.extract::<String>() {
                    return Ok(liquers_core::command_metadata::ArgumentInfo::any_argument(&name));
                }
                match argument.cast::<crate::command_metadata::ArgumentInfo>() {
                    Ok(info) => Ok(info.borrow().0.clone()),
                    Err(_) => Err(pyo3::exceptions::PyTypeError::new_err(
                        "a pipeline command argument is a str (its name) or an lq.ArgumentInfo",
                    )),
                }
            })
            .collect::<PyResult<Vec<_>>>()?;
        let command = crate::declared::DeclaredCommand::Pipeline {
            key: CommandKey::new("", namespace, name),
            pipeline,
            arguments,
            overrides: crate::declared::Overrides { label, doc, filename },
        };
        self.cell
            .declare(py, command)
            .map(|_replaced| ())
            .map_err(|e| error_to_pyerr(py, &e))
    }

    /// A `PipelineEditor` on `pipeline` (default: an empty one), checking each edit against this
    /// environment's commands as they are now; a command registered later is unknown to it.
    #[pyo3(signature = (pipeline=None))]
    fn pipeline_editor(&self, py: Python<'_>, pipeline: Option<&Bound<'_, PyAny>>) -> PyResult<crate::pipeline::PipelineEditor> {
        let pipeline = pipeline.map(crate::pipeline::pipeline_from_py).transpose()?;
        let cmr = crate::pipeline::editor_registry(self, py).map_err(|e| error_to_pyerr(py, &e))?;
        let editor = match pipeline {
            Some(pipeline) => liquers_core::pipeline::PipelineEditor::from_pipeline(pipeline, cmr),
            None => Ok(liquers_core::pipeline::PipelineEditor::new(cmr)),
        };
        editor
            .map(crate::pipeline::PipelineEditor::new)
            .map_err(|e| error_to_pyerr(py, &e))
    }

    /// A `QueryEditor` on `query` (a `str` or an `lq.Query`; default: an empty one), checking each
    /// edit against this environment's commands as they are now.
    #[pyo3(signature = (query=None))]
    fn query_editor(&self, py: Python<'_>, query: Option<&Bound<'_, PyAny>>) -> PyResult<crate::pipeline::QueryEditor> {
        let query = query.map(to_query).transpose().map_err(|e| error_to_pyerr(py, &e))?;
        let cmr = crate::pipeline::editor_registry(self, py).map_err(|e| error_to_pyerr(py, &e))?;
        let editor = match query {
            Some(query) => liquers_core::pipeline::QueryEditor::from_query(&query, cmr),
            None => Ok(liquers_core::pipeline::QueryEditor::new(cmr)),
        };
        editor
            .map(crate::pipeline::QueryEditor::new)
            .map_err(|e| error_to_pyerr(py, &e))
    }

    /// Every problem in template `text`, as `lq.Diagnostic`s, checked against this environment's
    /// commands; nothing is evaluated. (`validate()` checks the command registry itself.)
    fn validate_template(&self, py: Python<'_>, text: &str) -> PyResult<Vec<crate::template::Diagnostic>> {
        let envref = self.cell.envref(py).map_err(|e| error_to_pyerr(py, &e))?;
        Ok(crate::template::validate(text, envref.get_command_metadata_registry()))
    }

    /// `x` as a data node read at the top level of this environment: an `lq.State` whose value
    /// needs a context (a record source, a directory listing) is read asynchronously, each read in
    /// its own ad-hoc asset; anything else is read as `lq.data_node(x)`.
    fn data_node(&self, py: Python<'_>, x: &Bound<'_, PyAny>) -> PyResult<crate::data_node::DataNodeView> {
        if let Ok(state) = x.cast::<crate::asset::State>() {
            let envref = self.cell.envref(py).map_err(|e| error_to_pyerr(py, &e))?;
            return Ok(crate::data_node::DataNodeView::of_env(
                state.get().core().clone(),
                &envref,
                self.cell.shut.clone(),
            ));
        }
        Ok(crate::data_node::data_node(py, x))
    }

    #[new]
    #[pyo3(signature = (*, config=None, conversion=None, pickle=None, async_loops=None, job_capacity=None))]
    fn new(
        py: Python<'_>,
        config: Option<&Bound<'_, PyAny>>,
        conversion: Option<&str>,
        pickle: Option<bool>,
        async_loops: Option<i64>,
        job_capacity: Option<i64>,
    ) -> PyResult<Self> {
        // A keyword wins over the file's `python:` / `assets:` setting, which wins over the
        // built-in default (`configuration-document` Decision 2).
        let built = (|| {
            let mut config = PyConfig::from_python(py, config)?;
            let async_loops = match async_loops {
                Some(loops) => usize::try_from(loops).ok().and_then(NonZeroUsize::new).ok_or_else(|| {
                    Error::configuration_error(format!("async_loops must be at least 1, not {loops}"))
                })?,
                None => config.python.async_loops.unwrap_or(NonZeroUsize::MIN),
            };
            if let Some(capacity) = job_capacity {
                let capacity = usize::try_from(capacity)
                    .ok()
                    .filter(|capacity| *capacity >= 1)
                    .ok_or_else(|| {
                        Error::configuration_error(format!(
                            "job_capacity must be at least 1, not {capacity}"
                        ))
                    })?;
                config.environment.assets.job_capacity = Some(capacity);
            }
            let conversion = match conversion {
                Some(conversion) => ConversionMode::parse(conversion)?,
                None => match &config.python.conversion {
                    Some(conversion) => ConversionMode::parse(conversion)?,
                    None => ConversionMode::Permissive,
                },
            };
            let pickle = pickle.or(config.python.pickle).unwrap_or(false);
            EnvCell::new(config, conversion, pickle, async_loops)
        })();
        match built {
            Ok(cell) => Ok(Environment {
                cell: Arc::new(cell),
            }),
            Err(e) => Err(error_to_pyerr(py, &e)),
        }
    }

    /// A new Liquers UI model on this environment (`lq.UI`): show queries as elements, submit
    /// actions, follow statuses. Each call makes a separate model; its elements evaluate in the
    /// background until `dispose()` or `shutdown()`. See `PYTHON_GUIDE.md` "Show a Liquers UI".
    fn ui(&self, py: Python<'_>) -> PyResult<crate::ui::UI> {
        crate::ui::UI::new(py, &self.cell)
    }

    /// Serves this environment over the Liquers HTTP API and returns the running `lq.Server`.
    ///
    /// Settings come from the configuration's `server:` section; each keyword given overrides it
    /// (`host`, `port`, `prefix`, `apis` — a list of `"query"`, `"store"`, `"assets"`,
    /// `"recipes"` —, `read_only`, and `web`: `True`, `False` or a store folder). The defaults
    /// serve every API under `/liquer` on `127.0.0.1:8080`, with writes enabled and the store's
    /// `web/` folder at `/liquer/web/`. Binds before returning (`port=0` picks a free port). With
    /// `background=False` it serves until the server is stopped or Ctrl-C, which stops it and
    /// raises `KeyboardInterrupt`.
    #[pyo3(signature = (*, host=None, port=None, prefix=None, apis=None, read_only=None, web=None, background=true))]
    #[allow(clippy::too_many_arguments)]
    fn serve(
        &self,
        py: Python<'_>,
        host: Option<String>,
        port: Option<u16>,
        prefix: Option<String>,
        apis: Option<Vec<String>>,
        read_only: Option<bool>,
        web: Option<&Bound<'_, PyAny>>,
        background: bool,
    ) -> PyResult<crate::server::Server> {
        if !background {
            check_blocking_call(py, "serve")?;
        }
        let overrides = ServeOverrides { host, port, prefix, apis, read_only, web: web_setting(py, web)? };
        let config = self.server_config(py, overrides)?;
        crate::server::register_exit_hook(py)?;
        let cell = self.cell.clone();
        let slot = block_on(py, &self.cell.shut, crate::server::start(cell, config))
            .map_err(|e| e.into_pyerr(py))?;
        let handle = crate::server::Server { slot: slot.clone() };
        if background {
            return Ok(handle);
        }
        let waited = block_on(py, &self.cell.shut, async {
            slot.server.stopped().await;
            Ok(())
        });
        match waited {
            Ok(()) => Ok(handle),
            Err(failure) => {
                let slot = slot.clone();
                py.detach(move || slot.stop_blocking(SHUTDOWN_DEADLINE));
                match failure {
                    // Stopped by `env.shutdown()` while serving: a normal end.
                    PyFailure::Liquers(_) if self.cell.shut.is_set() => Ok(handle),
                    PyFailure::Liquers(_) | PyFailure::Python(_) => Err(failure.into_pyerr(py)),
                }
            }
        }
    }

    /// Awaitable `serve`, a coroutine: binds, then serves until the awaiting task is cancelled
    /// (which stops the server gracefully) or the server is stopped.
    /// `asyncio.create_task(env.serve_async())` runs it beside other work. The settings are
    /// checked when it is called, and binding starts then too. Takes `serve`'s keywords except
    /// `background`.
    #[pyo3(signature = (*, host=None, port=None, prefix=None, apis=None, read_only=None, web=None))]
    #[allow(clippy::too_many_arguments)]
    fn serve_async<'py>(
        &self,
        py: Python<'py>,
        host: Option<String>,
        port: Option<u16>,
        prefix: Option<String>,
        apis: Option<Vec<String>>,
        read_only: Option<bool>,
        web: Option<&Bound<'py, PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let overrides = ServeOverrides { host, port, prefix, apis, read_only, web: web_setting(py, web)? };
        let config = self.server_config(py, overrides)?;
        crate::server::register_exit_hook(py)?;
        crate::runtime::runtime().map_err(|e| error_to_pyerr(py, &e))?;
        let locals = pyo3_async_runtimes::tokio::get_current_locals(py)?;
        let cell = self.cell.clone();
        let future = pyo3_async_runtimes::tokio::future_into_py_with_locals(py, locals, async move {
            let slot = match crate::server::start(cell, config).await {
                Ok(slot) => slot,
                Err(e) => return Err(crate::exit::attach_tracked(|py| error_to_pyerr(py, &e))),
            };
            // Dropped when asyncio cancels the task (the future is dropped then) or when the
            // server stopped by itself: either way the stop token is cancelled.
            let _stop = crate::server::StopOnDrop(slot.server.stop_token());
            slot.server.stopped().await;
            Ok(())
        })?;
        // A coroutine rather than the bare future, so `asyncio.create_task` accepts it; cancelling
        // the task cancels the awaited future, which drops the server task above.
        py.import("liquers._env")?.getattr("_awaiting")?.call1((future,))
    }

    /// The running servers started from this environment.
    #[getter]
    fn servers(&self) -> Vec<crate::server::Server> {
        self.cell
            .running_servers()
            .into_iter()
            .map(|slot| crate::server::Server { slot })
            .collect()
    }

    /// Evaluates a query and returns its value as a Python object.
    ///
    /// `query` is a `str` or an `lq.Query`, or a pipeline: an `lq.Pipeline`, a list of elements or
    /// a dict in the pipeline's JSON form. A pipeline runs as a recipe of its own, in a private
    /// asset that an interrupt cancels.
    fn evaluate(&self, py: Python<'_>, query: &Bound<'_, PyAny>) -> PyResult<Py<PyAny>> {
        check_blocking_call(py, "evaluate")?;
        let result = (|| -> Result<_, PyFailure> {
            let evaluable = to_evaluable(py, query)?;
            let envref = self.cell.envref(py)?;
            let scope = Scope::from_envref(&envref, self.cell.shut.clone());
            let value = match evaluable {
                Evaluable::Query(query) => block_on(py, &self.cell.shut, async move {
                    let asset = envref.evaluate(query).await?;
                    let state = asset.get().await?;
                    state.value()
                })?,
                Evaluable::Pipeline(pipeline) => {
                    let asset = self.start_pipeline(py, &envref, pipeline)?;
                    let waited = asset.clone();
                    crate::closure::wait_for_asset(py, &self.cell.shut, asset, async move {
                        waited.get().await?.value()
                    })?
                }
            };
            Ok(value_to_py(py, &value, &scope)?)
        })();
        result.map_err(|e| e.into_pyerr(py))
    }

    /// Evaluates a query or a pipeline and returns the result as a `State`, metadata included.
    fn evaluate_state(&self, py: Python<'_>, query: &Bound<'_, PyAny>) -> PyResult<Py<crate::asset::State>> {
        check_blocking_call(py, "evaluate_state")?;
        let result = (|| -> Result<_, PyFailure> {
            let evaluable = to_evaluable(py, query)?;
            let envref = self.cell.envref(py)?;
            let scope = Scope::from_envref(&envref, self.cell.shut.clone());
            let state = match evaluable {
                Evaluable::Query(query) => block_on(py, &self.cell.shut, async move {
                    let asset = envref.evaluate(query).await?;
                    asset.get().await?.value_state()
                })?,
                Evaluable::Pipeline(pipeline) => {
                    let asset = self.start_pipeline(py, &envref, pipeline)?;
                    let waited = asset.clone();
                    crate::closure::wait_for_asset(py, &self.cell.shut, asset, async move {
                        waited.get().await?.value_state()
                    })?
                }
            };
            Ok(crate::asset::State::from_core(py, &state, &scope)?)
        })();
        result.map_err(|e| e.into_pyerr(py))
    }

    /// Starts evaluating a query and returns its `AssetRef` without waiting for the value.
    ///
    /// A pipeline raises `TypeError`: its asset is private to one evaluation (use `evaluate`).
    fn get_asset(&self, py: Python<'_>, query: &Bound<'_, PyAny>) -> PyResult<crate::asset::AssetRef> {
        refuse_pipeline(query)?;
        check_blocking_call(py, "get_asset")?;
        let result = (|| -> Result<_, PyFailure> {
            let query = to_query(query)?;
            let envref = self.cell.envref(py)?;
            let scope = Scope::from_envref(&envref, self.cell.shut.clone());
            let asset = block_on(py, &self.cell.shut, async move { envref.evaluate(query).await })?;
            Ok(crate::asset::AssetRef::new(asset, self.cell.shut.clone(), scope))
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
        let (evaluable, envref) = self.evaluable_and_envref(py, query)?;
        let scope = Scope::from_envref(&envref, self.cell.shut.clone());
        awaitable(
            py,
            &self.cell.shut,
            async move {
                let asset = evaluate_evaluable(&envref, evaluable).await?;
                Ok((asset.get().await?.value()?, scope))
            },
            ToPy::Value,
        )
    }

    /// Awaitable `evaluate_state`.
    fn evaluate_state_async<'py>(&self, py: Python<'py>, query: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        let (evaluable, envref) = self.evaluable_and_envref(py, query)?;
        let scope = Scope::from_envref(&envref, self.cell.shut.clone());
        awaitable(
            py,
            &self.cell.shut,
            async move {
                let asset = evaluate_evaluable(&envref, evaluable).await?;
                Ok((asset.get().await?.value_state()?, scope))
            },
            ToPy::State,
        )
    }

    /// Awaitable `get_asset`: resolves to the `AssetRef` once the evaluation has started.
    fn get_asset_async<'py>(&self, py: Python<'py>, query: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        refuse_pipeline(query)?;
        let (query, envref) = self.query_and_envref(py, query)?;
        let shut = self.cell.shut.clone();
        let scope = Scope::from_envref(&envref, shut.clone());
        awaitable(
            py,
            &self.cell.shut,
            async move { Ok((envref.evaluate(query).await?, shut, scope)) },
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

    /// Registers `factory` as the store type `type`, which a store configuration entry can then
    /// name (`{"type": type, "prefix": …, "config": {…}}`).
    ///
    /// `factory` is any callable that is not `async def`; its keyword parameters are the entry's
    /// `config:` arguments (a default makes one optional), and a parameter named `prefix` receives
    /// the mount's prefix. Pass `arguments=[StoreArgumentInfo(…)]` when its signature cannot be
    /// inspected. It is called when the environment is built, and may not use the environment.
    /// The name may not be a built-in store type or one registered before.
    #[pyo3(signature = (r#type, factory, *, label=String::new(), doc=String::new(), arguments=None))]
    fn register_store_factory(
        &self,
        py: Python<'_>,
        r#type: &str,
        factory: &Bound<'_, PyAny>,
        label: String,
        doc: String,
        arguments: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        let registered = PyStoreType::new(py, r#type, factory, &label, &doc, arguments)
            .and_then(|store_type| self.cell.register_store_type(py, store_type));
        registered.map_err(|e| error_to_pyerr(py, &e))
    }

    /// Registers `factory` as the recipe provider entry type `type`, which a `recipes:` entry can
    /// then name (`{"type": type, …}`; the entry's other keys are the factory's arguments).
    ///
    /// `factory` is any callable that is not `async def`; its keyword parameters are the entry's
    /// arguments (a default makes one optional). Pass `arguments=[StoreArgumentInfo(…)]` when its
    /// signature cannot be inspected. It returns the provider: an object with `recipe_opt(key)`
    /// (returning `lq.Recipe`, query text or `None`) and optionally `assets_with_recipes(folder)`,
    /// `contains(key)` and `directory_changed(folder)`, each `def` or `async def`. It is called when
    /// the environment is built, and may not use the environment. The name may not be a built-in
    /// entry type, a keyword (`default`, `trivial`, `none`, `no_recipes`) or one registered before.
    #[pyo3(signature = (r#type, factory, *, label=String::new(), doc=String::new(), arguments=None))]
    fn register_recipe_provider_factory(
        &self,
        py: Python<'_>,
        r#type: &str,
        factory: &Bound<'_, PyAny>,
        label: String,
        doc: String,
        arguments: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<()> {
        let registered = PyRecipeProviderType::new(py, r#type, factory, &label, &doc, arguments)
            .and_then(|provider_type| self.cell.register_recipe_provider_type(py, provider_type));
        registered.map_err(|e| error_to_pyerr(py, &e))
    }

    /// The environment's recipe provider chain, every configured member included: an
    /// `lq.RecipeProviders`.
    #[getter]
    fn recipes(&self) -> crate::recipe_provider::RecipeProviders {
        crate::recipe_provider::RecipeProviders::of_environment(&self.cell)
    }

    /// The environment's store, with every configured mount: an `lq.Store`.
    #[getter]
    fn store(&self) -> crate::store::handle::Store {
        crate::store::handle::Store::of_environment(&self.cell)
    }

    /// The Liquers version this extension was built with, as Rust writes it (`"0.1.0"`); the
    /// same for every Liquers crate and every environment. `liquers.__version__` is its PEP 440
    /// form. Reading it neither builds the environment nor fails after `shutdown()`.
    #[getter]
    fn liquers_version(&self) -> &'static str {
        liquers_core::VERSION
    }

    /// The store types a configuration can name, built-in then Python, each a dict with
    /// `store_type`, `label`, `doc`, `arguments`, `availability` and `coverage`.
    fn store_types(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let listed = self.cell.store_types(py).and_then(|types| {
            serde_json::to_value(&types).map_err(|e| Error::general_error(e.to_string()))
        });
        let listed = listed.map_err(|e| error_to_pyerr(py, &e))?;
        crate::store::json_to_py(py, &listed)
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
        EnvCell::new(PyConfig::default(), ConversionMode::Permissive, false, NonZeroUsize::MIN)
    }

    #[test]
    fn liquers_version_is_core_version() -> Result<(), Error> {
        attached(|py| {
            let cell = new_cell()?;
            let envref = cell.envref(py)?;
            let reported = liquers_core::context::Environment::liquers_version(envref.0.as_ref());
            assert_eq!(reported, liquers_core::VERSION);
            assert_eq!(crate::pep440(reported), crate::pep440(liquers_core::VERSION));
            Ok(())
        })
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

    /// A server that finishes binding after `shutdown()` took the server list (`serve_async`, or
    /// `serve` on another thread) is refused and stopped, so no listener outlives the environment.
    #[test]
    fn a_server_bound_after_shutdown_is_stopped() -> Result<(), Error> {
        attached(|py| {
            let cell = Arc::new(new_cell()?);
            cell.envref(py)?;
            cell.shutdown(py)?;
            let config = liquers_axum::ServerConfig { port: 0, ..liquers_axum::ServerConfig::default() };
            let started = py.detach(|| runtime()?.block_on(crate::server::start(cell.clone(), config)));
            match started {
                Ok(slot) => panic!("a server started on a shut environment: {}", slot.server.local_addr()),
                Err(e) => assert!(e.to_string().contains("shut down"), "{e}"),
            }
            assert!(cell.running_servers().is_empty());
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
