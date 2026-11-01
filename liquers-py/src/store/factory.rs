//! Python store types: `Environment.register_store_factory` and the [`StoreFactory`] holding them.
//!
//! A registered type is a Python factory plus its declaration (`liquers._factories._declare`).
//! [`PyStoreFactory`] is chained **after** `liquers_store::store_factory::default_store_factory()`
//! and registration refuses a name that chain already resolves, so a Python type never shadows a
//! built-in one. `create` runs inside the environment's build, which holds the GIL; it checks the
//! entry's `config:` keys against the declaration, calls the factory with them as keywords (and
//! `prefix`, when the factory takes it) and adapts the result ([`super::adapter::PyStore`]).

use std::sync::Arc;

use liquers_core::error::Error;
use liquers_core::query::Key;
use liquers_core::store::AsyncStore;
use liquers_core::store_config::StoreConfig;
use liquers_core::store_factory::{ArgumentCoverage, StoreArgumentInfo, StoreFactory, StoreTypeInfo};
use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::aio::{LazyLoopPool, Shut};
use crate::error::pyerr_to_error;
use crate::pycall::LiveHandle;
use crate::store::adapter::PyStore;

/// One registered Python store type.
pub(crate) struct PyStoreType {
    pub(crate) info: StoreTypeInfo,
    factory: Py<PyAny>,
    takes_prefix: bool,
    takes_kwargs: bool,
    _handle: LiveHandle,
}

impl PyStoreType {
    /// Declares `factory` as the store type `name`.
    pub(crate) fn new(
        py: Python<'_>,
        name: &str,
        factory: &Bound<'_, PyAny>,
        label: &str,
        doc: &str,
        arguments: Option<&Bound<'_, PyAny>>,
    ) -> Result<Self, Error> {
        if name.is_empty() {
            return Err(Error::configuration_error("a store type name may not be empty".to_string()));
        }
        let declared = super::declare(py, factory, arguments, &["prefix"], &["type"])?;
        let coverage = if declared.takes_kwargs {
            ArgumentCoverage::Partial {
                authority: format!("the Python factory of store type '{name}'"),
            }
        } else {
            ArgumentCoverage::Complete
        };
        Ok(PyStoreType {
            info: StoreTypeInfo {
                store_type: name.to_string(),
                label: if label.is_empty() { name.to_string() } else { label.to_string() },
                doc: doc.to_string(),
                arguments: declared.arguments,
                availability: Default::default(),
                coverage,
            },
            factory: factory.clone().unbind(),
            takes_prefix: declared.exempt_taken.iter().any(|n| n == "prefix"),
            takes_kwargs: declared.takes_kwargs,
            _handle: LiveHandle::new(),
        })
    }

    fn arguments_for(&self, config: &StoreConfig) -> Result<Vec<(String, serde_json::Value)>, Error> {
        let mount = describe(config);
        let mut values = Vec::new();
        for argument in &self.info.arguments {
            match config.config.get(&argument.name) {
                Some(value) => values.push((argument.name.clone(), value.clone())),
                None if argument.required => {
                    return Err(Error::configuration_error(format!(
                        "{mount}: the required argument '{}' is missing from `config:`",
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
        let known = |name: &str| self.info.arguments.iter().any(|a: &StoreArgumentInfo| a.name == name);
        let mut unknown: Vec<&String> = config.config.keys().filter(|k| !known(k)).collect();
        unknown.sort();
        if !unknown.is_empty() {
            if !self.takes_kwargs {
                return Err(Error::configuration_error(format!(
                    "{mount}: unknown `config:` key(s) {}; the factory takes {}",
                    unknown.iter().map(|k| format!("'{k}'")).collect::<Vec<_>>().join(", "),
                    if self.info.arguments.is_empty() {
                        "none".to_string()
                    } else {
                        self.info.arguments.iter().map(|a| format!("'{}'", a.name)).collect::<Vec<_>>().join(", ")
                    }
                )));
            }
            for key in unknown {
                values.push((key.clone(), config.config[key].clone()));
            }
        }
        Ok(values)
    }
}

/// `store type 'notes' at prefix 'notes'`, for error messages.
fn describe(config: &StoreConfig) -> String {
    format!("the mount of store type '{}' at prefix '{}'", config.store_type, config.prefix)
}

/// Every Python store type of one environment.
pub(crate) struct PyStoreFactory {
    pub(crate) types: Vec<Arc<PyStoreType>>,
    pub(crate) loops: Arc<LazyLoopPool>,
    pub(crate) shut: Arc<Shut>,
}

impl StoreFactory for PyStoreFactory {
    fn store_types(&self) -> Vec<StoreTypeInfo> {
        self.types.iter().map(|t| t.info.clone()).collect()
    }

    fn create(&self, config: &StoreConfig) -> Result<Box<dyn AsyncStore>, Error> {
        let store_type = self
            .types
            .iter()
            .find(|t| t.info.store_type == config.store_type)
            .ok_or_else(|| {
                liquers_core::store_factory::unknown_store_type_error(&config.store_type, &self.store_types())
            })?;
        let prefix: Key = liquers_core::parse::parse_key(&config.prefix)?;
        let arguments = store_type.arguments_for(config)?;
        let mount = describe(config);
        // Re-entrant: the build holds the GIL already.
        Python::attach(|py| -> Result<Box<dyn AsyncStore>, Error> {
            let called = (|| -> PyResult<Py<PyAny>> {
                let kwargs = PyDict::new(py);
                for (name, value) in &arguments {
                    kwargs.set_item(name, super::json_to_py(py, value)?)?;
                }
                if store_type.takes_prefix {
                    kwargs.set_item("prefix", config.prefix.clone())?;
                }
                Ok(store_type.factory.bind(py).call((), Some(&kwargs))?.unbind())
            })();
            let object = match called {
                Ok(object) => object,
                Err(e) => {
                    let cause = pyerr_to_error(py, &e);
                    let error = Error::configuration_error(format!("{mount}: the factory raised: {}", cause.message));
                    return Err(match cause.traceback.clone() {
                        Some(traceback) => error.with_traceback(traceback),
                        None => error,
                    });
                }
            };
            let object = object.bind(py);
            if crate::pycall::is_coroutine(py, object).unwrap_or(false)
                || object.hasattr("__await__").unwrap_or(false)
            {
                if crate::pycall::is_coroutine(py, object).unwrap_or(false) {
                    let _ = object.call_method0("close");
                }
                return Err(Error::configuration_error(format!(
                    "{mount}: the factory returned an awaitable; a store factory must return the store itself"
                )));
            }
            let store = PyStore::adapt(py, &config.store_type, &prefix, object, self.loops.clone(), self.shut.clone())?;
            Ok(Box::new(store))
        })
    }
}
