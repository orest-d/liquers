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

#[pyclass]
pub struct CommandMetadata(pub liquers_core::command_metadata::CommandMetadata);

#[pymethods]
impl CommandMetadata {
    #[new]
    fn new(name: &str) -> Self {
        CommandMetadata(liquers_core::command_metadata::CommandMetadata::new(name))
    }

    fn to_json(&self) -> PyResult<String> {
        serde_json::to_string(&self.0).map_err(|e| PyErr::new::<PyException, _>(e.to_string()))
    }

    fn to_yaml(&self) -> PyResult<String> {
        serde_yaml::to_string(&self.0).map_err(|e| PyErr::new::<PyException, _>(e.to_string()))
    }

    #[staticmethod]
    fn from_json(json: &str) -> PyResult<Self> {
        let a:liquers_core::command_metadata::CommandMetadata = serde_json::from_str(json).map_err(|e| PyErr::new::<PyException, _>(e.to_string()))?;
        Ok(CommandMetadata(a))
    }

    #[staticmethod]
    fn from_yaml(yaml: &str) -> PyResult<Self> {
        let a:liquers_core::command_metadata::CommandMetadata = serde_yaml::from_str(yaml).map_err(|e| PyErr::new::<PyException, _>(e.to_string()))?;
        Ok(CommandMetadata(a))
    }
}

#[pyclass]
pub struct CommandMetadataRegistry(pub liquers_core::command_metadata::CommandMetadataRegistry);

#[pymethods]
impl CommandMetadataRegistry {
    #[new]
    fn new() -> Self {
        CommandMetadataRegistry(liquers_core::command_metadata::CommandMetadataRegistry::new())
    }

    fn add_command(&mut self, command:&CommandMetadata){
        self.0.add_command(&command.0);
    }

    fn to_json(&self) -> PyResult<String> {
        serde_json::to_string(&self.0).map_err(|e| PyErr::new::<PyException, _>(e.to_string()))
    }

    fn to_yaml(&self) -> PyResult<String> {
        serde_yaml::to_string(&self.0).map_err(|e| PyErr::new::<PyException, _>(e.to_string()))
    }

    #[staticmethod]
    fn from_json(json: &str) -> PyResult<Self> {
        let a:liquers_core::command_metadata::CommandMetadataRegistry = serde_json::from_str(json).map_err(|e| PyErr::new::<PyException, _>(e.to_string()))?;
        Ok(CommandMetadataRegistry(a))
    }

    #[staticmethod]
    fn from_yaml(yaml: &str) -> PyResult<Self> {
        let a:liquers_core::command_metadata::CommandMetadataRegistry = serde_yaml::from_str(yaml).map_err(|e| PyErr::new::<PyException, _>(e.to_string()))?;
        Ok(CommandMetadataRegistry(a))
    }
}
