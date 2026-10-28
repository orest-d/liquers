use liquers_core::recipes::Recipe as CoreRecipe;
use pyo3::{prelude::*, types::PyDict};

/// A named recipe: a query with a title and a description, and optional argument and link
/// overrides.
///
/// `Recipe(query, title, description)` raises an exception when `query` does not parse. See
/// `PYTHON_GUIDE.md` §6, "Named results: recipes".
#[pyclass(from_py_object, module = "liquers")]
#[derive(Debug, Clone)]
pub struct Recipe {
    pub inner: CoreRecipe,
}

/// A list of recipes, the form of a folder's `recipes.yaml`.
///
/// `RecipeList()` is empty; `len(recipes)` is the number of recipes.
#[pyclass(from_py_object, module = "liquers")]
#[derive(Debug, Clone)]
pub struct RecipeList {
    pub inner: liquers_core::recipes::RecipeList,
}

#[pymethods]
impl RecipeList {
    #[new]
    pub fn new() -> Self {
        Self {
            inner: liquers_core::recipes::RecipeList::new(),
        }
    }

    /// Appends a copy of `recipe`.
    pub fn add_recipe(&mut self, recipe: &Recipe) {
        self.inner.add_recipe(recipe.inner.clone());
    }

    pub fn __len__(&self) -> usize {
        self.inner.len()
    }

    /// The recipe list serialized as JSON.
    pub fn to_json(&self) -> PyResult<String> {
        serde_json::to_string(&self.inner)
            .map_err(|e| pyo3::exceptions::PyException::new_err(e.to_string()))
    }

    /// Reads a recipe list from JSON text, as `to_json()` writes it.
    ///
    /// Raises an exception when the text is not valid JSON for `RecipeList`.
    #[staticmethod]
    pub fn from_json(json: &str) -> PyResult<Self> {
        let inner = serde_json::from_str(json)
            .map_err(|e| pyo3::exceptions::PyException::new_err(e.to_string()))?;
        Ok(Self { inner })
    }
}

#[pymethods]
impl Recipe {
    #[new]
    pub fn new(query: String, title: String, description: String) -> PyResult<Self> {
        Ok(Recipe {
            inner: liquers_core::recipes::Recipe::new(query, title, description)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyException, _>(e.to_string()))?,
        })
    }

    /// The recipe's query, as text. Settable.
    #[getter]
    pub fn query(&self) -> String {
        self.inner.query.clone()
    }
    #[setter]
    pub fn set_query(&mut self, query: String) {
        self.inner.query = query;
    }

    /// A short human-readable title. Settable.
    #[getter]
    pub fn title(&self) -> String {
        self.inner.title.clone()
    }
    #[setter]
    pub fn set_title(&mut self, title: String) {
        self.inner.title = title;
    }

    /// A longer description of what the recipe produces. Settable.
    #[getter]
    pub fn description(&self) -> String {
        self.inner.description.clone()
    }
    #[setter]
    pub fn set_description(&mut self, description: String) {
        self.inner.description = description;
    }

    /// The argument overrides, as a dict from argument name to the value's text.
    #[getter]
    pub fn arguments<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let dict = PyDict::new(py);
        for (k, v) in &self.inner.arguments {
            dict.set_item(k, format!("{}", v))?;
        }
        Ok(dict)
    }

    /// The link overrides, as a dict from argument name to query text.
    #[getter]
    pub fn links<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let dict = PyDict::new(py);
        for (k, v) in &self.inner.links {
            dict.set_item(k, v)?;
        }
        Ok(dict)
    }

    /// The recipe serialized as JSON.
    pub fn to_json(&self) -> PyResult<String> {
        serde_json::to_string(&self.inner)
            .map_err(|e| pyo3::exceptions::PyException::new_err(e.to_string()))
    }

    /// Reads a recipe from JSON text, as `to_json()` writes it.
    ///
    /// Raises an exception when the text is not valid JSON for `Recipe`.
    #[staticmethod]
    pub fn from_json(json: &str) -> PyResult<Self> {
        let m: CoreRecipe = serde_json::from_str(json)
            .map_err(|e| pyo3::exceptions::PyException::new_err(e.to_string()))?;
        Ok(Recipe { inner: m })
    }

    /// The recipe serialized as YAML.
    pub fn to_yaml(&self) -> PyResult<String> {
        serde_yaml::to_string(&self.inner)
            .map_err(|e| pyo3::exceptions::PyException::new_err(e.to_string()))
    }

    /// Reads a recipe from YAML text, as `to_yaml()` writes it.
    ///
    /// Raises an exception when the text is not valid YAML for `Recipe`.
    #[staticmethod]
    pub fn from_yaml(yaml: &str) -> PyResult<Self> {
        let m: CoreRecipe = serde_yaml::from_str(yaml)
            .map_err(|e| pyo3::exceptions::PyException::new_err(e.to_string()))?;
        Ok(Recipe { inner: m })
    }

    pub fn __str__(&self) -> String {
        format!("Recipe: {} - {}", self.inner.title, self.inner.query)
    }

    pub fn __repr__(&self) -> String {
        format!("{:?}", self.inner)
    }
}
