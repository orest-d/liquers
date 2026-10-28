use liquers_core::parse::parse_query;
use pyo3::{exceptions::PyException, prelude::*};

use crate::{command_metadata::CommandMetadataRegistry, error::Error};

/// One step of a `Plan`: fetch a resource, run an action, set a filename, log a message, ...
///
/// Inspect a plan's steps through its JSON or YAML form (`Plan.to_json()`).
#[pyclass(from_py_object, module = "liquers")]
#[derive(Debug, Clone)]
pub struct Step(pub liquers_core::plan::Step);

#[pymethods]
impl Step {
    /// A step that logs `message` as an informational log entry.
    #[staticmethod]
    fn info(message: String) -> Self {
        Self(liquers_core::plan::Step::Info(message))
    }

    /// A step that logs `message` as a warning.
    #[staticmethod]
    fn warning(message: String) -> Self {
        Self(liquers_core::plan::Step::Warning(message))
    }

    /// A step that logs `message` as an error entry; it does not stop the evaluation by itself.
    #[staticmethod]
    fn error(message: String) -> Self {
        Self(liquers_core::plan::Step::Error(message))
    }

    /// The step serialized as JSON.
    fn to_json(&self) -> PyResult<String> {
        serde_json::to_string(&self.0).map_err(|e| PyErr::new::<PyException, _>(e.to_string()))
    }

    /// Reads a step from JSON text, as `to_json()` writes it.
    ///
    /// Raises an exception when the text is not valid JSON for `Step`.
    #[staticmethod]
    fn from_json(json: &str) -> PyResult<Self> {
        let s: liquers_core::plan::Step =
            serde_json::from_str(json).map_err(|e| PyErr::new::<PyException, _>(e.to_string()))?;
        Ok(Self(s))
    }
}

/// The value a plan resolved for one command argument: a literal, a default, a link, an injected
/// value, ...
///
/// Part of `ResolvedParameterValues`.
#[pyclass(from_py_object, module = "liquers")]
#[derive(Debug, Clone)]
pub struct ParameterValue(pub liquers_core::plan::ParameterValue);

#[pymethods]
impl ParameterValue {
    /// The unresolved parameter value; it should not remain in a plan that is executed.
    #[staticmethod]
    fn none() -> Self {
        Self(liquers_core::plan::ParameterValue::None)
    }

    /// The parameter value serialized as JSON.
    fn to_json(&self) -> PyResult<String> {
        serde_json::to_string(&self.0).map_err(|e| PyErr::new::<PyException, _>(e.to_string()))
    }

    /// Reads a parameter value from JSON text, as `to_json()` writes it.
    ///
    /// Raises an exception when the text is not valid JSON for `ParameterValue`.
    #[staticmethod]
    fn from_json(json: &str) -> PyResult<Self> {
        let s: liquers_core::plan::ParameterValue =
            serde_json::from_str(json).map_err(|e| PyErr::new::<PyException, _>(e.to_string()))?;
        Ok(Self(s))
    }
}

/// The arguments of one action step, in the order of the command's metadata.
///
/// `ResolvedParameterValues()` is the empty list.
#[pyclass(from_py_object, module = "liquers")]
#[derive(Debug, Clone)]
pub struct ResolvedParameterValues(pub liquers_core::plan::ResolvedParameterValues);

#[pymethods]
impl ResolvedParameterValues {
    #[new]
    fn new() -> Self {
        Self(liquers_core::plan::ResolvedParameterValues::new())
    }

    /// The resolved parameter values serialized as JSON.
    fn to_json(&self) -> PyResult<String> {
        serde_json::to_string(&self.0).map_err(|e| PyErr::new::<PyException, _>(e.to_string()))
    }

    /// Reads resolved parameter values from JSON text, as `to_json()` writes it.
    ///
    /// Raises an exception when the text is not valid JSON for `ResolvedParameterValues`.
    #[staticmethod]
    fn from_json(json: &str) -> PyResult<Self> {
        let s: liquers_core::plan::ResolvedParameterValues =
            serde_json::from_str(json).map_err(|e| PyErr::new::<PyException, _>(e.to_string()))?;
        Ok(Self(s))
    }
}

/// The steps that evaluating a query takes, built without running anything.
///
/// Make one with `build_plan`; `Plan()` is the empty plan. Read it through `to_json()` or
/// `to_yaml()`. See `PYTHON_GUIDE.md` §6, "Checking a query without running it".
#[pyclass(module = "liquers")]
pub struct Plan(pub liquers_core::plan::Plan);

#[pymethods]
impl Plan {
    #[new]
    fn new() -> Self {
        Plan(liquers_core::plan::Plan::new())
    }

    /// The plan serialized as JSON.
    fn to_json(&self) -> PyResult<String> {
        serde_json::to_string(&self.0).map_err(|e| PyErr::new::<PyException, _>(e.to_string()))
    }

    /// The plan serialized as YAML.
    fn to_yaml(&self) -> PyResult<String> {
        serde_yaml::to_string(&self.0).map_err(|e| PyErr::new::<PyException, _>(e.to_string()))
    }

    /// Reads a plan from JSON text, as `to_json()` writes it.
    ///
    /// Raises an exception when the text is not valid JSON for `Plan`.
    #[staticmethod]
    fn from_json(json: &str) -> PyResult<Self> {
        let p: liquers_core::plan::Plan =
            serde_json::from_str(json).map_err(|e| PyErr::new::<PyException, _>(e.to_string()))?;
        Ok(Plan(p))
    }

    /// Reads a plan from YAML text, as `to_yaml()` writes it.
    ///
    /// Raises an exception when the text is not valid YAML for `Plan`.
    #[staticmethod]
    fn from_yaml(yaml: &str) -> PyResult<Self> {
        let p: liquers_core::plan::Plan =
            serde_yaml::from_str(yaml).map_err(|e| PyErr::new::<PyException, _>(e.to_string()))?;
        Ok(Plan(p))
    }
}

/// Builds the plan of `query` against the command metadata in `command_metadata_registry`.
///
/// Nothing is evaluated. Raises `ParseError` when the query does not parse, and a `LiquersError`
/// when the plan cannot be built, for example for an unknown command or a wrong number of
/// parameters.
#[pyfunction]
pub fn build_plan(
    query: String,
    command_metadata_registry: &CommandMetadataRegistry,
) -> PyResult<Plan> {
    let query = parse_query(&query).map_err(|e| Error(e))?;
    let cmr = &command_metadata_registry.0;
    let mut plan_builder = liquers_core::plan::PlanBuilder::new(query, cmr);
    let plan = plan_builder.build().map_err(|e| Error(e))?;
    Ok(Plan(plan))
}
