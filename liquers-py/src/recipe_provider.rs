//! Python recipe providers (`specs/design/liquers-py-recipes/`, Phase 2).
//!
//! `Environment.register_recipe_provider_factory(type, factory)` declares an entry type of the
//! `recipes:` chain. [`PyRecipeProviderFactory`] holds every Python type of an environment and is
//! passed to `EnvironmentBuilder::with_recipe_provider_factory` on every build, so a document
//! entry `{type: <type>, …}` is created by calling the factory with the entry's other keys as
//! keywords. The object it returns is adapted once into a [`PyRecipeProvider`]: `recipe_opt` is
//! required; `assets_with_recipes`, `contains` and `directory_changed` are optional.
//!
//! `env.recipes` ([`RecipeProviders`]) asks the environment's built chain, every member included.

use std::sync::{Arc, Weak};

use async_trait::async_trait;
use liquers_core::context::EnvRef;
use liquers_core::error::Error;
use liquers_core::plan::Plan as CorePlan;
use liquers_core::query::{Key, ResourceName};
use liquers_core::recipes::{
    AsyncRecipeProvider, Recipe as CoreRecipe, RecipeProviderEntry, RecipeProviderFactory, RecipeProviderTypeInfo,
};
use pyo3::prelude::*;
use pyo3::types::PyTuple;

use crate::aio::{awaitable, LazyLoopPool, Shut, ToPy};
use crate::environment::{block_on, shut_down_error, EnvCell, PyEnvironment};
use crate::error::{error_to_pyerr, pyerr_to_error, PyFailure};
use crate::pycall::{call, FromPyReturn, LiveHandle, PyMethod};

type Method = Option<Arc<PyMethod>>;

/// The entry keywords a Python type may not take: `with_recipe_provider_factory` is consulted
/// before the kind's own factory, so the name would override the keyword.
const KEYWORDS: [&str; 4] = ["default", "trivial", "none", "no_recipes"];

/// A provider object's protocol, resolved once.
struct RecipeMethods {
    recipe_opt: Arc<PyMethod>,
    assets_with_recipes: Method,
    contains: Method,
    directory_changed: Method,
}

impl RecipeMethods {
    fn resolve(py: Python<'_>, object: &Bound<'_, PyAny>) -> Result<Self, String> {
        let method = |name: &str| -> Result<Method, String> {
            PyMethod::resolve(py, object, name)
                .map(|m| m.map(Arc::new))
                .map_err(|e| format!("its '{name}' attribute cannot be used: {e}"))
        };
        Ok(RecipeMethods {
            recipe_opt: method("recipe_opt")?.ok_or_else(|| "it has no 'recipe_opt' method".to_string())?,
            assets_with_recipes: method("assets_with_recipes")?,
            contains: method("contains")?,
            directory_changed: method("directory_changed")?,
        })
    }
}

/// A Python object as an [`AsyncRecipeProvider`].
pub(crate) struct PyRecipeProvider {
    name: String,
    methods: Arc<RecipeMethods>,
    loops: Arc<LazyLoopPool>,
    shut: Arc<Shut>,
    _handle: LiveHandle,
}

/// Pins a closure to the higher-ranked signature `pycall::call` takes.
fn arguments<F>(f: F) -> F
where
    F: for<'py> FnOnce(Python<'py>) -> PyResult<Bound<'py, PyTuple>> + Send + 'static,
{
    f
}

/// A conversion, with the GIL held; a raised exception maps through `pyerr_to_error`.
struct Convert<F>(F);

impl<T, F> FromPyReturn for Convert<F>
where
    T: Send + 'static,
    F: for<'py> FnOnce(Python<'py>, Bound<'py, PyAny>) -> Result<T, Error> + Send + 'static,
{
    type Output = T;
    fn convert(self, py: Python<'_>, returned: Bound<'_, PyAny>) -> Result<T, Error> {
        (self.0)(py, returned)
    }
    fn failure(&self, py: Python<'_>, error: PyErr) -> Error {
        pyerr_to_error(py, &error)
    }
}

/// Pins a closure to the higher-ranked signature of a result conversion.
fn converter<T, F>(f: F) -> Convert<F>
where
    F: for<'py> FnOnce(Python<'py>, Bound<'py, PyAny>) -> Result<T, Error> + Send + 'static,
{
    Convert(f)
}

fn type_name(value: &Bound<'_, PyAny>) -> String {
    value.get_type().name().map(|n| n.to_string()).unwrap_or_default()
}

/// What `recipe_opt` returned, as a recipe for `key`: `lq.Recipe`, query text, or `None`. A
/// recipe without a `cwd` gets the key's folder.
fn recipe_of(value: &Bound<'_, PyAny>, key: &Key, subject: &str) -> Result<Option<CoreRecipe>, Error> {
    if value.is_none() {
        return Ok(None);
    }
    let mut recipe = if let Ok(recipe) = value.cast::<crate::recipes::Recipe>() {
        recipe.borrow().inner.clone()
    } else if let Ok(query) = value.extract::<String>() {
        CoreRecipe::new(query, String::new(), String::new()).map_err(|e| e.with_key(key))?
    } else {
        return Err(Error::execution_error(format!(
            "{subject}: 'recipe_opt' for '{key}' must return lq.Recipe, query text or None, not {}",
            type_name(value)
        ))
        .with_key(key));
    };
    if recipe.cwd.is_none() {
        recipe.cwd = Some(key.parent().encode());
    }
    Ok(Some(recipe))
}

/// A listing of `folder`: names, or `lq.Key`s directly in `folder`.
fn listing_of(value: &Bound<'_, PyAny>, folder: &Key, subject: &str) -> Result<Vec<ResourceName>, Error> {
    if value.is_none() {
        return Ok(Vec::new());
    }
    let refuse = |what: String| {
        Error::execution_error(format!(
            "{subject}: 'assets_with_recipes' for '{folder}' must return names or lq.Key in that folder; {what}"
        ))
        .with_key(folder)
    };
    let iterator = value
        .try_iter()
        .map_err(|_| refuse(format!("got {}", type_name(value))))?;
    let mut names = Vec::new();
    for item in iterator {
        let item = item.map_err(|e| refuse(e.to_string()))?;
        if let Ok(k) = item.cast::<crate::query::Key>() {
            let k = k.borrow().0.clone();
            match k.filename() {
                Some(name) if k.parent() == *folder => names.push(name.clone()),
                Some(_) | None => return Err(refuse(format!("'{k}' is not in it"))),
            }
        } else if let Ok(name) = item.extract::<String>() {
            if name.is_empty() || name.contains('/') {
                return Err(refuse(format!("'{name}' is not a name in it")));
            }
            names.push(ResourceName::new(name));
        } else {
            return Err(refuse(format!("got an entry of type {}", type_name(&item))));
        }
    }
    Ok(names)
}

impl PyRecipeProvider {
    /// Adapts `object`, the provider of the chain entry `entry`.
    pub(crate) fn adapt(
        py: Python<'_>,
        entry: &str,
        object: &Bound<'_, PyAny>,
        loops: Arc<LazyLoopPool>,
        shut: Arc<Shut>,
    ) -> Result<Self, Error> {
        let methods = RecipeMethods::resolve(py, object).map_err(|reason| {
            Error::configuration_error(format!("The Python recipe provider of {entry} cannot be used: {reason}"))
        })?;
        Ok(PyRecipeProvider {
            name: entry.to_string(),
            methods: Arc::new(methods),
            loops,
            shut,
            _handle: LiveHandle::new(),
        })
    }

    fn subject(&self) -> String {
        format!("the recipe provider of {}", self.name)
    }

    async fn invoke<T, F>(&self, method: &Arc<PyMethod>, key: &Key, f: F) -> Result<T, Error>
    where
        T: Send + 'static,
        F: for<'py> FnOnce(Python<'py>, Bound<'py, PyAny>) -> Result<T, Error> + Send + 'static,
    {
        let owned = key.clone();
        let args = arguments(move |py| PyTuple::new(py, [Py::new(py, crate::query::Key(owned))?]));
        call(method.clone(), self.loops.clone(), self.shut.clone(), self.subject(), args, converter(f)).await
    }

    async fn listing(&self, folder: &Key) -> Result<Vec<ResourceName>, Error> {
        let Some(method) = &self.methods.assets_with_recipes else {
            return Ok(Vec::new());
        };
        let subject = self.subject();
        let owned = folder.clone();
        self.invoke(method, folder, move |_py, r| listing_of(&r, &owned, &subject)).await
    }
}

#[async_trait]
impl AsyncRecipeProvider<PyEnvironment> for PyRecipeProvider {
    async fn has_recipes(&self, key: &Key, _envref: EnvRef<PyEnvironment>) -> Result<bool, Error> {
        Ok(!self.listing(key).await?.is_empty())
    }

    async fn assets_with_recipes(&self, key: &Key, _envref: EnvRef<PyEnvironment>) -> Result<Vec<ResourceName>, Error> {
        self.listing(key).await
    }

    async fn recipe_plan(&self, key: &Key, envref: EnvRef<PyEnvironment>) -> Result<CorePlan, Error> {
        let recipe = self.recipe(key, envref.clone()).await?;
        recipe.plan_with_dependencies(envref, Some(key)).await.map_err(|e| e.with_key(key))
    }

    async fn recipe(&self, key: &Key, envref: EnvRef<PyEnvironment>) -> Result<CoreRecipe, Error> {
        self.recipe_opt(key, envref).await?.ok_or_else(|| Error::key_not_found(key))
    }

    async fn recipe_opt(&self, key: &Key, _envref: EnvRef<PyEnvironment>) -> Result<Option<CoreRecipe>, Error> {
        let subject = self.subject();
        let owned = key.clone();
        self.invoke(&self.methods.recipe_opt, key, move |_py, r| recipe_of(&r, &owned, &subject)).await
    }

    async fn contains(&self, key: &Key, _envref: EnvRef<PyEnvironment>) -> Result<bool, Error> {
        if let Some(method) = &self.methods.contains {
            return self
                .invoke(method, key, |_py, r| r.is_truthy().map_err(|e| Error::execution_error(e.to_string())))
                .await;
        }
        let Some(name) = key.filename() else {
            return Ok(false);
        };
        Ok(self.listing(&key.parent()).await?.iter().any(|listed| listed == name))
    }

    async fn directory_changed(&self, dir: &Key) {
        if let Some(method) = &self.methods.directory_changed {
            if let Err(e) = self.invoke(method, dir, |_py, _r| Ok(())).await {
                eprintln!("{}: 'directory_changed' for '{dir}' failed: {}", self.subject(), e.message);
            }
        }
    }
}

/// One registered Python recipe provider type.
pub(crate) struct PyRecipeProviderType {
    pub(crate) info: RecipeProviderTypeInfo,
    factory: Py<PyAny>,
    takes_kwargs: bool,
    _handle: LiveHandle,
}

impl PyRecipeProviderType {
    /// Declares `factory` as the entry type `name`.
    pub(crate) fn new(
        py: Python<'_>,
        name: &str,
        factory: &Bound<'_, PyAny>,
        label: &str,
        doc: &str,
        arguments: Option<&Bound<'_, PyAny>>,
    ) -> Result<Self, Error> {
        if name.is_empty() {
            return Err(Error::configuration_error("a recipe provider type name may not be empty".to_string()));
        }
        let declared = crate::store::declare(py, factory, arguments, &[], &["type"])?;
        let mut info = RecipeProviderTypeInfo::new(name);
        if !label.is_empty() {
            info.label = label.to_string();
        }
        info.doc = doc.to_string();
        info.arguments = declared.arguments;
        Ok(PyRecipeProviderType {
            info,
            factory: factory.clone().unbind(),
            takes_kwargs: declared.takes_kwargs,
            _handle: LiveHandle::new(),
        })
    }
}

/// The names a Python type may not take: the kind's own entry types and the keywords.
pub(crate) fn reserved_names() -> Vec<String> {
    use liquers_core::environment_builder::AssetManagerKind;
    let mut names: Vec<String> = liquers_lib::environment::LibKind::recipe_provider_factory::<PyEnvironment>()
        .provider_types()
        .into_iter()
        .map(|t| t.provider_type)
        .collect();
    names.extend(KEYWORDS.iter().map(|k| k.to_string()));
    names
}

/// `the entry {"type":"reports","folder":"reports"}`, for error messages.
fn describe(entry: &RecipeProviderEntry) -> String {
    let text = serde_json::to_string(entry).unwrap_or_else(|_| format!("'{}'", entry.provider_type));
    format!("the recipe provider entry {text}")
}

/// Every Python recipe provider type of one environment.
pub(crate) struct PyRecipeProviderFactory {
    pub(crate) types: Vec<Arc<PyRecipeProviderType>>,
    pub(crate) loops: Arc<LazyLoopPool>,
    pub(crate) shut: Arc<Shut>,
}

impl RecipeProviderFactory<PyEnvironment> for PyRecipeProviderFactory {
    fn provider_types(&self) -> Vec<RecipeProviderTypeInfo> {
        self.types.iter().map(|t| t.info.clone()).collect()
    }

    fn create(&self, entry: &RecipeProviderEntry) -> Result<Arc<dyn AsyncRecipeProvider<PyEnvironment>>, Error> {
        let provider_type = self
            .types
            .iter()
            .find(|t| t.info.provider_type == entry.provider_type)
            .ok_or_else(|| {
                liquers_core::recipes::unknown_recipe_provider_error(entry, None, &self.provider_types())
            })?;
        let subject = describe(entry);
        let arguments = crate::store::bind_arguments(
            &subject,
            "the entry",
            &provider_type.info.arguments,
            provider_type.takes_kwargs,
            entry.arguments.iter().collect(),
        )?;
        // Re-entrant: the build holds the GIL already.
        Python::attach(|py| -> Result<Arc<dyn AsyncRecipeProvider<PyEnvironment>>, Error> {
            let object =
                crate::store::call_factory(py, &provider_type.factory, &arguments, None, &subject, "recipe provider")?;
            let provider = PyRecipeProvider::adapt(py, &subject, object.bind(py), self.loops.clone(), self.shut.clone())?;
            Ok(Arc::new(provider))
        })
    }
}

/// The environment's recipe provider chain, as Python sees it: `env.recipes`.
///
/// Asks every member of the configured `recipes:` chain, the built-in ones and the Python ones,
/// as an evaluation does. Keys are `lq.Key` or key text. Every method has an awaitable `_async`
/// twin; after the environment's `shutdown()` every call raises. See `PYTHON_GUIDE.md`, "Recipe
/// providers".
#[pyclass(frozen, name = "RecipeProviders", module = "liquers")]
pub struct RecipeProviders {
    cell: Weak<EnvCell>,
}

type Chain = Arc<dyn AsyncRecipeProvider<PyEnvironment>>;

impl RecipeProviders {
    pub(crate) fn of_environment(cell: &Arc<EnvCell>) -> Self {
        RecipeProviders {
            cell: Arc::downgrade(cell),
        }
    }

    fn resolve(&self, py: Python<'_>) -> Result<(Chain, EnvRef<PyEnvironment>, Arc<Shut>), Error> {
        crate::runtime::runtime()?;
        let cell = self.cell.upgrade().ok_or_else(shut_down_error)?;
        let envref = cell.envref(py)?;
        Ok((envref.get_recipe_provider(), envref, cell.shut.clone()))
    }

    fn blocking<T, F, Fut>(&self, py: Python<'_>, f: F) -> PyResult<T>
    where
        T: Send,
        F: FnOnce(Chain, EnvRef<PyEnvironment>) -> Fut,
        Fut: std::future::Future<Output = Result<T, Error>> + Send,
    {
        crate::aio::check_blocking_call(py, "a RecipeProviders method")?;
        let result = (|| -> Result<T, PyFailure> {
            let (chain, envref, shut) = self.resolve(py)?;
            block_on(py, &shut, f(chain, envref))
        })();
        result.map_err(|e| e.into_pyerr(py))
    }

    fn deferred<'py, T, F, Fut>(
        &self,
        py: Python<'py>,
        f: F,
        wrap: fn(Result<T, Error>) -> ToPy,
    ) -> PyResult<Bound<'py, PyAny>>
    where
        T: Send + 'static,
        F: FnOnce(Chain, EnvRef<PyEnvironment>) -> Fut,
        Fut: std::future::Future<Output = Result<T, Error>> + Send + 'static,
    {
        let (chain, envref, shut) = self.resolve(py).map_err(|e| error_to_pyerr(py, &e))?;
        awaitable(py, &shut, f(chain, envref), wrap)
    }
}

fn key_of(py: Python<'_>, key: &Bound<'_, PyAny>) -> PyResult<Key> {
    crate::store::handle::key_of(py, key)
}

fn recipe_to_py(recipe: Option<CoreRecipe>) -> Option<crate::recipes::Recipe> {
    recipe.map(|inner| crate::recipes::Recipe { inner })
}

#[pymethods]
impl RecipeProviders {
    /// The recipe for `key` (an `lq.Recipe`), or `None` when no member has one.
    fn recipe_opt(&self, py: Python<'_>, key: &Bound<'_, PyAny>) -> PyResult<Option<crate::recipes::Recipe>> {
        let key = key_of(py, key)?;
        self.blocking(py, |c, e| async move { c.recipe_opt(&key, e).await }).map(recipe_to_py)
    }

    /// Awaitable `recipe_opt`.
    fn recipe_opt_async<'py>(&self, py: Python<'py>, key: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        let key = key_of(py, key)?;
        self.deferred(py, |c, e| async move { c.recipe_opt(&key, e).await }, ToPy::Recipe)
    }

    /// The recipe for `key` (an `lq.Recipe`). Raises `GeneralError` when no member has one
    /// (`RECIPE-MISSING-RECIPE-IS-A-GENERAL-ERROR`).
    fn recipe(&self, py: Python<'_>, key: &Bound<'_, PyAny>) -> PyResult<crate::recipes::Recipe> {
        let key = key_of(py, key)?;
        let inner = self.blocking(py, |c, e| async move { c.recipe(&key, e).await })?;
        Ok(crate::recipes::Recipe { inner })
    }

    /// Awaitable `recipe`.
    fn recipe_async<'py>(&self, py: Python<'py>, key: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        let key = key_of(py, key)?;
        self.deferred(py, |c, e| async move { c.recipe(&key, e).await }, |r| ToPy::Recipe(r.map(Some)))
    }

    /// The plan of the recipe for `key`, with its dependencies analysed: an `lq.Plan`.
    fn recipe_plan(&self, py: Python<'_>, key: &Bound<'_, PyAny>) -> PyResult<crate::plan::Plan> {
        let key = key_of(py, key)?;
        self.blocking(py, |c, e| async move { c.recipe_plan(&key, e).await }).map(crate::plan::Plan)
    }

    /// Awaitable `recipe_plan`.
    fn recipe_plan_async<'py>(&self, py: Python<'py>, key: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        let key = key_of(py, key)?;
        self.deferred(py, |c, e| async move { c.recipe_plan(&key, e).await }, ToPy::Plan)
    }

    /// The names of the recipe-backed assets every member lists in `folder`: `lq.ResourceName`s.
    fn assets_with_recipes(&self, py: Python<'_>, folder: &Bound<'_, PyAny>) -> PyResult<Vec<crate::query::ResourceName>> {
        let folder = key_of(py, folder)?;
        let names = self.blocking(py, |c, e| async move { c.assets_with_recipes(&folder, e).await })?;
        Ok(names.into_iter().map(crate::query::ResourceName).collect())
    }

    /// Awaitable `assets_with_recipes`.
    fn assets_with_recipes_async<'py>(&self, py: Python<'py>, folder: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        let folder = key_of(py, folder)?;
        self.deferred(py, |c, e| async move { c.assets_with_recipes(&folder, e).await }, ToPy::ResourceNames)
    }

    /// Whether `key` is among the assets its folder lists.
    fn contains(&self, py: Python<'_>, key: &Bound<'_, PyAny>) -> PyResult<bool> {
        let key = key_of(py, key)?;
        self.blocking(py, |c, e| async move { c.contains(&key, e).await })
    }

    /// Awaitable `contains`.
    fn contains_async<'py>(&self, py: Python<'py>, key: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        let key = key_of(py, key)?;
        self.deferred(py, |c, e| async move { c.contains(&key, e).await }, ToPy::Bool)
    }

    /// Whether some member can produce `key`, listed or not.
    fn can_make(&self, py: Python<'_>, key: &Bound<'_, PyAny>) -> PyResult<bool> {
        let key = key_of(py, key)?;
        self.blocking(py, |c, e| async move { c.can_make(&key, e).await })
    }

    /// Awaitable `can_make`.
    fn can_make_async<'py>(&self, py: Python<'py>, key: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        let key = key_of(py, key)?;
        self.deferred(py, |c, e| async move { c.can_make(&key, e).await }, ToPy::Bool)
    }
}
