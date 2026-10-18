//! `lq.State`: a value together with its metadata.
//!
//! A command whose first parameter is named `state` receives one (`StateDelivery::State`), and
//! `Environment.evaluate_state` returns one. It is a read-only snapshot (VALUE13): the value is
//! converted once, when the state is made, and the metadata is a copy.

use liquers_core::error::Error;
use liquers_core::state::State as CoreState;
use pyo3::prelude::*;

use crate::error::pyerr_to_error;
use crate::metadata::Metadata;
use crate::value::{value_to_py, Value};

/// A value and its metadata.
#[pyclass(frozen, name = "State", module = "liquers")]
pub struct State {
    value: Py<PyAny>,
    metadata: Py<Metadata>,
}

impl State {
    pub fn from_core(py: Python<'_>, state: &CoreState<Value>) -> Result<Py<State>, Error> {
        let value = value_to_py(py, state.data_unchecked().as_ref())?;
        let metadata = Py::new(
            py,
            Metadata {
                inner: state.metadata.as_ref().clone(),
            },
        )
        .map_err(|e| pyerr_to_error(py, &e))?;
        Py::new(py, State { value, metadata }).map_err(|e| pyerr_to_error(py, &e))
    }
}

#[pymethods]
impl State {
    /// The value, as a Python object.
    #[getter]
    fn value(&self, py: Python<'_>) -> Py<PyAny> {
        self.value.clone_ref(py)
    }

    /// The state's `Metadata`.
    #[getter]
    fn metadata(&self, py: Python<'_>) -> Py<Metadata> {
        self.metadata.clone_ref(py)
    }

    fn __repr__(&self, py: Python<'_>) -> String {
        let value = self
            .value
            .bind(py)
            .repr()
            .map(|r| r.to_string())
            .unwrap_or_else(|_| "<value>".to_string());
        format!("State({value})")
    }
}
