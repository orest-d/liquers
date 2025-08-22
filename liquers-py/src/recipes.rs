use pyo3::{prelude::*, types::PyDict};
use serde_json::Value;
use std::collections::HashMap;
use liquers_core::recipes::Recipe as CoreRecipe;

#[pyclass]
#[derive(Debug, Clone)]
pub struct Recipe{pub inner: CoreRecipe}

#[pymethods]
impl Recipe {
    #[new]
    pub fn new(query: String, title: String, description: String) -> PyResult<Self> {
        Ok(Recipe {
            inner: liquers_core::recipes::Recipe::new(query, title, description)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyException, _>(e.to_string()))?,
        })
    }

    #[getter]
    pub fn query(&self) -> String {
        self.inner.query.clone()
    }
    #[setter]
    pub fn set_query(&mut self, query: String) {
        self.inner.query = query;
    }

    #[getter]
    pub fn title(&self) -> String {
        self.inner.title.clone()
    }
    #[setter]
    pub fn set_title(&mut self, title: String) {
        self.inner.title = title;
    }

    #[getter]
    pub fn description(&self) -> String {
        self.inner.description.clone()
    }
    #[setter]
    pub fn set_description(&mut self, description: String) {
        self.inner.description = description;
    }

    #[getter]
    pub fn arguments<'py>(&self, py: Python<'py>) -> &'py PyDict {
        let dict = PyDict::new(py);
        for (k, v) in &self.inner.arguments {
            dict.set_item(k, format!("{}", v)).unwrap();
        }
        dict
    }

    #[getter]
    pub fn links<'py>(&self, py: Python<'py>) -> &'py PyDict {
        let dict = PyDict::new(py);
        for (k, v) in &self.inner.links {
            dict.set_item(k, v).unwrap();
        }
        dict
    }

    pub fn to_json(&self) -> PyResult<String> {
        serde_json::to_string(&self.inner)
            .map_err(|e| pyo3::exceptions::PyException::new_err(e.to_string()))
    }

    #[staticmethod]
    pub fn from_json(json: &str) -> PyResult<Self> {
        let m: CoreRecipe = serde_json::from_str(json)
            .map_err(|e| pyo3::exceptions::PyException::new_err(e.to_string()))?;
        Ok(Recipe { inner: m })
    }

    pub fn to_yaml(&self) -> PyResult<String> {
        serde_yaml::to_string(&self.inner)
            .map_err(|e| pyo3::exceptions::PyException::new_err(e.to_string()))
    }

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
