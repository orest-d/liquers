use liquers_core::recipes::Recipe;
use pyo3::{prelude::*, types::PyDict};
use serde_json::Value;
use std::collections::HashMap;

#[pyclass]
pub struct PyRecipe {
    pub inner: Recipe,
}

#[pymethods]
impl PyRecipe {
    #[new]
    pub fn new(query: String, title: String, description: String) -> Self {
        PyRecipe {
            inner: Recipe {
                query,
                title,
                description,
                arguments: HashMap::new(),
                links: HashMap::new(),
            },
        }
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
    #[setter]
    pub fn set_arguments(&mut self, args: HashMap<String, Value>) {
        self.inner.arguments = args;
    }

    #[getter]
    pub fn links<'py>(&self, py: Python<'py>) -> &'py PyDict {
        let dict = PyDict::new(py);
        for (k, v) in &self.inner.links {
            dict.set_item(k, v).unwrap();
        }
        dict
    }
    #[setter]
    pub fn set_links(&mut self, links: HashMap<String, String>) {
        self.inner.links = links;
    }
}
