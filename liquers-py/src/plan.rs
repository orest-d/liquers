use liquers_core::{command_metadata, parse::parse_query, query};
use pyo3::{exceptions::PyException, prelude::*};

use crate::{command_metadata::CommandMetadataRegistry, error::Error};

#[pyclass]
pub struct Plan(pub liquers_core::plan::Plan);

#[pymethods]
impl Plan {
    #[new]
    fn new() -> Self {
        Plan(liquers_core::plan::Plan::new())
    }

    fn to_json(&self) -> PyResult<String> {
        serde_json::to_string(&self.0).map_err(|e| PyErr::new::<PyException, _>(e.to_string()))
    }

    fn to_yaml(&self) -> PyResult<String> {
        serde_yaml::to_string(&self.0).map_err(|e| PyErr::new::<PyException, _>(e.to_string()))
    }

    #[staticmethod]
    fn from_json(json: &str) -> PyResult<Self> {
        let p:liquers_core::plan::Plan = serde_json::from_str(json).map_err(|e| PyErr::new::<PyException, _>(e.to_string()))?;
        Ok(Plan(p))
    }

    #[staticmethod]
    fn from_yaml(yaml: &str) -> PyResult<Self> {
        let p:liquers_core::plan::Plan = serde_yaml::from_str(yaml).map_err(|e| PyErr::new::<PyException, _>(e.to_string()))?;
        Ok(Plan(p))
    }
}

#[pyfunction]
pub fn build_plan(query:String, command_metadata_registry:&CommandMetadataRegistry) -> PyResult<Plan> {
    let query = parse_query(&query).map_err(|e| Error(e))?;
    let cmr = &command_metadata_registry.0;
    let mut plan_builder = liquers_core::plan::PlanBuilder::new(query, cmr);
    let plan = plan_builder.build().map_err(|e|  Error(e))?;
    Ok(Plan(plan))
}

