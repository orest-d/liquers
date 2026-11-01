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
use liquers_core::store_factory::{ArgumentCoverage, StoreFactory, StoreTypeInfo};
use pyo3::prelude::*;

use crate::aio::{LazyLoopPool, Shut};
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
        super::bind_arguments(
            &describe(config),
            "`config:`",
            &self.info.arguments,
            self.takes_kwargs,
            config.config.iter().collect(),
        )
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
            let prefix_argument = store_type.takes_prefix.then(|| ("prefix", config.prefix.clone()));
            let object = super::call_factory(py, &store_type.factory, &arguments, prefix_argument, &mount, "store")?;
            let object = object.bind(py);
            let store = PyStore::adapt(py, &config.store_type, &prefix, object, self.loops.clone(), self.shut.clone())?;
            Ok(Box::new(store))
        })
    }
}
