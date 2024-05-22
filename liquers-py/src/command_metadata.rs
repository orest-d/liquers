use pyo3::{exceptions::PyException, prelude::*};

#[pyclass]
pub struct ArgumentInfo(liquers_core::command_metadata::ArgumentInfo);

#[pymethods]
impl ArgumentInfo {
    #[new]
    fn new(name: &str) -> Self {
        ArgumentInfo(liquers_core::command_metadata::ArgumentInfo::any_argument(name))
    }

    fn to_json(&self) -> PyResult<String> {
        serde_json::to_string(&self.0).map_err(|e| PyErr::new::<PyException, _>(e.to_string()))
    }

    fn to_yaml(&self) -> PyResult<String> {
        serde_yaml::to_string(&self.0).map_err(|e| PyErr::new::<PyException, _>(e.to_string()))
    }

    #[staticmethod]
    fn from_json(json: &str) -> PyResult<Self> {
        let a:liquers_core::command_metadata::ArgumentInfo = serde_json::from_str(json).map_err(|e| PyErr::new::<PyException, _>(e.to_string()))?;
        Ok(ArgumentInfo(a))
    }

    #[staticmethod]
    fn from_yaml(yaml: &str) -> PyResult<Self> {
        let a:liquers_core::command_metadata::ArgumentInfo = serde_yaml::from_str(yaml).map_err(|e| PyErr::new::<PyException, _>(e.to_string()))?;
        Ok(ArgumentInfo(a))
    }
}