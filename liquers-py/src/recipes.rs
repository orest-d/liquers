use pyo3::{prelude::*, types::PyDict};
use serde_json::Value;
use std::collections::HashMap;

#[pyclass]
#[derive(Debug, Clone)]
pub struct Recipe(pub liquers_core::recipes::Recipe);

#[pymethods]
impl Recipe {
    #[new]
    pub fn new(query: String, title: String, description: String) -> PyResult<Self> {
        Ok(Recipe(
            liquers_core::recipes::Recipe::new(query, title, description)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyException, _>(e.to_string()))?,
        ))
    }

    #[getter]
    pub fn query(&self) -> String {
        self.0.query.clone()
    }
    #[setter]
    pub fn set_query(&mut self, query: String) {
        self.0.query = query;
    }

    #[getter]
    pub fn title(&self) -> String {
        self.0.title.clone()
    }
    #[setter]
    pub fn set_title(&mut self, title: String) {
        self.0.title = title;
    }

    #[getter]
    pub fn description(&self) -> String {
        self.0.description.clone()
    }
    #[setter]
    pub fn set_description(&mut self, description: String) {
        self.0.description = description;
    }

    #[getter]
    pub fn arguments<'py>(&self, py: Python<'py>) -> &'py PyDict {
        let dict = PyDict::new(py);
        for (k, v) in &self.0.arguments {
            dict.set_item(k, format!("{}", v)).unwrap();
        }
        dict
    }

    #[getter]
    pub fn links<'py>(&self, py: Python<'py>) -> &'py PyDict {
        let dict = PyDict::new(py);
        for (k, v) in &self.0.links {
            dict.set_item(k, v).unwrap();
        }
        dict
    }
}
