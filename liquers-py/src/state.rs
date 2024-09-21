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

    pub fn is_error(&self)->PyResult<bool>{
        self.0.metadata.is_error().map_err(|e| crate::error::Error(e).into())
    }

    pub fn get_value(&self) -> PyResult<Value>{
        if self.is_error()?{
            Err(PyException::new_err("ERROR".to_string()))
        }
        else{
            Ok((*self.0.data).clone())
        }
    }

    pub fn get(&self) -> PyResult<PyObject>{
        if self.is_error()?{
            Err(PyException::new_err("ERROR".to_string()))
        }
        else{
            Python::with_gil(|py|{
                (*self.0.data).as_pyobject(py)
            })
        }
    }

    pub fn __str__(&self) -> PyResult<String> {
        self.0.data.__str__()
    }

    pub fn __repr__(&self) -> PyResult<String> {        
        Ok(format!("State(data={}, metadata={:?})", self.0.data.__repr__()?, *self.0.metadata))
    }

}
