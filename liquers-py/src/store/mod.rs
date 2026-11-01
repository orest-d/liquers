//! Stores written in Python (`specs/design/liquers-py-store/`).
//!
//! [`StoreArgumentInfo`] declares one configuration argument of a Python store or recipe provider
//! type. The adapter, the factory and the `lq.Store` handle live in this module's submodules.

use liquers_core::error::Error;
use liquers_core::store_factory::{StoreArgumentInfo as CoreStoreArgumentInfo, StoreArgumentType};
use pyo3::prelude::*;

use crate::error::pyerr_to_error;

/// One configuration argument of a store or recipe provider type.
///
/// `StoreArgumentInfo(name, argument_type="string", *, required=False, default=None, label="",
/// doc="")`. `argument_type` is one of `"string"`, `"number"`, `"boolean"`, `"array"`, `"object"`
/// and `"any"`; `default` must be JSON. Pass a list of them as `arguments=` to
/// `Environment.register_store_factory` or `register_recipe_provider_factory` when a factory's
/// signature cannot describe its arguments.
#[pyclass(frozen, name = "StoreArgumentInfo", module = "liquers")]
#[derive(Clone)]
pub struct StoreArgumentInfo(pub CoreStoreArgumentInfo);

fn argument_type(text: &str) -> PyResult<StoreArgumentType> {
    match text {
        "string" => Ok(StoreArgumentType::String),
        "number" => Ok(StoreArgumentType::Number),
        "boolean" => Ok(StoreArgumentType::Boolean),
        "array" => Ok(StoreArgumentType::Array),
        "object" => Ok(StoreArgumentType::Object),
        "any" => Ok(StoreArgumentType::Any),
        other => Err(pyo3::exceptions::PyValueError::new_err(format!(
            "unknown argument type '{other}'; expected string, number, boolean, array, object or any"
        ))),
    }
}

fn argument_type_name(argument_type: &StoreArgumentType) -> &'static str {
    match argument_type {
        StoreArgumentType::String => "string",
        StoreArgumentType::Number => "number",
        StoreArgumentType::Boolean => "boolean",
        StoreArgumentType::Array => "array",
        StoreArgumentType::Object => "object",
        StoreArgumentType::Any => "any",
    }
}

#[pymethods]
impl StoreArgumentInfo {
    #[new]
    #[pyo3(signature = (name, argument_type="string", *, required=false, default=None, label=String::new(), doc=String::new()))]
    fn new(
        py: Python<'_>,
        name: String,
        argument_type: &str,
        required: bool,
        default: Option<&Bound<'_, PyAny>>,
        label: String,
        doc: String,
    ) -> PyResult<Self> {
        let default = match default {
            Some(value) if !value.is_none() => Some(
                crate::environment::py_to_json(py, value).map_err(|e| crate::error::error_to_pyerr(py, &e))?,
            ),
            Some(_) | None => None,
        };
        let label = if label.is_empty() { name.clone() } else { label };
        Ok(StoreArgumentInfo(CoreStoreArgumentInfo {
            name,
            label,
            doc,
            argument_type: self::argument_type(argument_type)?,
            required,
            default,
        }))
    }

    /// The key as it appears in the configuration entry.
    #[getter]
    fn name(&self) -> String {
        self.0.name.clone()
    }

    /// The JSON type the argument accepts: `"string"`, `"number"`, `"boolean"`, `"array"`,
    /// `"object"` or `"any"`.
    #[getter]
    fn argument_type(&self) -> &'static str {
        argument_type_name(&self.0.argument_type)
    }

    /// True when the type cannot be built without it.
    #[getter]
    fn required(&self) -> bool {
        self.0.required
    }

    /// The value used when the key is absent, or `None`.
    #[getter]
    fn default(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        match &self.0.default {
            Some(value) => json_to_py(py, value),
            None => Ok(py.None()),
        }
    }

    /// A human-readable label.
    #[getter]
    fn label(&self) -> String {
        self.0.label.clone()
    }

    /// What the argument means.
    #[getter]
    fn doc(&self) -> String {
        self.0.doc.clone()
    }

    /// The argument as JSON text.
    fn to_json(&self) -> PyResult<String> {
        serde_json::to_string(&self.0).map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }

    fn __repr__(&self) -> String {
        format!(
            "StoreArgumentInfo({:?}, {:?}, required={})",
            self.0.name,
            argument_type_name(&self.0.argument_type),
            if self.0.required { "True" } else { "False" }
        )
    }
}

/// A JSON value as the matching Python object.
pub(crate) fn json_to_py(py: Python<'_>, value: &serde_json::Value) -> PyResult<Py<PyAny>> {
    let text = serde_json::to_string(value).map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
    Ok(py.import("json")?.call_method1("loads", (text,))?.unbind())
}

/// What `liquers._factories._declare` derived from a factory's signature.
pub(crate) struct Declared {
    pub arguments: Vec<CoreStoreArgumentInfo>,
    pub takes_kwargs: bool,
    /// The exempt names (such as `prefix`) the factory accepts.
    pub exempt_taken: Vec<String>,
}

/// Calls `_declare` for `factory`; `arguments` is the `arguments=` list, if given.
pub(crate) fn declare(
    py: Python<'_>,
    factory: &Bound<'_, PyAny>,
    arguments: Option<&Bound<'_, PyAny>>,
    exempt: &[&str],
    reserved: &[&str],
) -> Result<Declared, Error> {
    let declared = (|| -> PyResult<(String, bool, Vec<String>)> {
        let kwargs = pyo3::types::PyDict::new(py);
        kwargs.set_item("exempt", exempt.to_vec())?;
        kwargs.set_item("reserved", reserved.to_vec())?;
        py.import("liquers._factories")?
            .getattr("_declare")?
            .call((factory, arguments), Some(&kwargs))?
            .extract()
    })()
    .map_err(|e| Error::configuration_error(format!("invalid factory: {}", pyerr_to_error(py, &e))))?;
    let (json, takes_kwargs, exempt_taken) = declared;
    let arguments = serde_json::from_str(&json)
        .map_err(|e| Error::configuration_error(format!("invalid factory arguments: {e}")))?;
    Ok(Declared {
        arguments,
        takes_kwargs,
        exempt_taken,
    })
}
