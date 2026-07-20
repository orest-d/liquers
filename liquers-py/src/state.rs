use std::borrow::Borrow;

use liquers_core::{error::Error, value::ValueInterface};
use pyo3::{exceptions::PyException, prelude::*};

use crate::value::Value;

#[pyclass]
pub struct State(pub liquers_core::state::State<crate::value::Value>);

#[pymethods]
impl State {
    #[new]
    fn new() -> Self {
        State(liquers_core::state::State::new())
    }

    pub fn is_error(&self) -> PyResult<bool> {
        self.0
            .metadata
            .is_error()
            .map_err(|e| crate::error::Error(e).into())
    }

    pub fn get_value(&self) -> PyResult<Value> {
        // WP-2 contract: requesting a value from an error/cancelled state raises (with the typed
        // error message), while a value-bearing state returns the value.
        match self.0.value() {
            Ok(value) => Ok((*value).clone()),
            Err(e) => Err(crate::error::Error(e).into()),
        }
    }

    pub fn get(&self) -> PyResult<PyObject> {
        match self.0.value() {
            Ok(value) => Python::with_gil(|py| value.as_pyobject(py)),
            Err(e) => Err(crate::error::Error(e).into()),
        }
    }

    pub fn __str__(&self) -> PyResult<String> {
        // Display accessors show the underlying (possibly none) value without raising.
        self.0.data_unchecked().__str__()
    }

    pub fn __repr__(&self) -> PyResult<String> {
        Ok(format!(
            "State(data={}, metadata={:?})",
            self.0.data_unchecked().__repr__()?,
            *self.0.metadata
        ))
    }
}
