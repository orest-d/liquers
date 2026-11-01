//! Stores written in Python (`specs/design/liquers-py-store/`).
//!
//! [`StoreArgumentInfo`] declares one configuration argument of a Python store or recipe provider
//! type. The adapter, the factory and the `lq.Store` handle live in this module's submodules.

pub(crate) mod adapter;
pub(crate) mod factory;
pub mod handle;

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

/// The keyword arguments a factory is called with: each declared argument from `given` (or its
/// default), refusing a missing required one, and the undeclared keys, refused unless the factory
/// takes `**kwargs`. `subject` and `section` name the entry and where its arguments are written
/// in error messages. Shared by store and recipe provider factories.
pub(crate) fn bind_arguments(
    subject: &str,
    section: &str,
    declared: &[CoreStoreArgumentInfo],
    takes_kwargs: bool,
    given: Vec<(&String, &serde_json::Value)>,
) -> Result<Vec<(String, serde_json::Value)>, Error> {
    let lookup = |name: &str| given.iter().find(|(k, _)| k.as_str() == name).map(|(_, v)| *v);
    let mut values = Vec::new();
    for argument in declared {
        match lookup(&argument.name) {
            Some(value) => values.push((argument.name.clone(), value.clone())),
            None if argument.required => {
                return Err(Error::configuration_error(format!(
                    "{subject}: the required argument '{}' is missing from {section}",
                    argument.name
                )))
            }
            None => {
                if let Some(default) = &argument.default {
                    values.push((argument.name.clone(), default.clone()));
                }
            }
        }
    }
    let known = |name: &str| declared.iter().any(|a| a.name == name);
    let mut unknown: Vec<(&String, &serde_json::Value)> =
        given.iter().filter(|(k, _)| !known(k)).copied().collect();
    unknown.sort_by(|a, b| a.0.cmp(b.0));
    if !unknown.is_empty() {
        if !takes_kwargs {
            return Err(Error::configuration_error(format!(
                "{subject}: unknown {section} key(s) {}; the factory takes {}",
                unknown.iter().map(|(k, _)| format!("'{k}'")).collect::<Vec<_>>().join(", "),
                if declared.is_empty() {
                    "none".to_string()
                } else {
                    declared.iter().map(|a| format!("'{}'", a.name)).collect::<Vec<_>>().join(", ")
                }
            )));
        }
        for (key, value) in unknown {
            values.push((key.clone(), value.clone()));
        }
    }
    Ok(values)
}

/// Calls a factory with `arguments` (and `extra`, when given) as keywords, inside a build.
///
/// A raised exception becomes a `ConfigurationError` naming `subject` that keeps the traceback;
/// an awaitable result is refused (closed first, if a coroutine), since a factory runs
/// synchronously. `what` names the product in that message ("store", "recipe provider").
pub(crate) fn call_factory(
    py: Python<'_>,
    factory: &Py<PyAny>,
    arguments: &[(String, serde_json::Value)],
    extra: Option<(&str, String)>,
    subject: &str,
    what: &str,
) -> Result<Py<PyAny>, Error> {
    let called = (|| -> PyResult<Py<PyAny>> {
        let kwargs = pyo3::types::PyDict::new(py);
        for (name, value) in arguments {
            kwargs.set_item(name, json_to_py(py, value)?)?;
        }
        if let Some((name, value)) = extra {
            kwargs.set_item(name, value)?;
        }
        Ok(factory.bind(py).call((), Some(&kwargs))?.unbind())
    })();
    let object = match called {
        Ok(object) => object,
        Err(e) => {
            let cause = pyerr_to_error(py, &e);
            let error = Error::configuration_error(format!("{subject}: the factory raised: {}", cause.message));
            return Err(match cause.traceback.clone() {
                Some(traceback) => error.with_traceback(traceback),
                None => error,
            });
        }
    };
    let bound = object.bind(py);
    let coroutine = crate::pycall::is_coroutine(py, bound).unwrap_or(false);
    if coroutine || bound.hasattr("__await__").unwrap_or(false) {
        if coroutine {
            let _ = bound.call_method0("close");
        }
        return Err(Error::configuration_error(format!(
            "{subject}: the factory returned an awaitable; a {what} factory must return the {what} itself"
        )));
    }
    Ok(object)
}
