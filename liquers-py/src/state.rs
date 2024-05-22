use std::borrow::Borrow;

use liquers_core::value::ValueInterface;
use pyo3::{exceptions::PyException, prelude::*};


#[pyclass]
pub struct State(liquers_core::state::State<crate::value::Value>);

#[pymethods]
impl State {
    #[new]
    fn new() -> Self {
        State(liquers_core::state::State::new())
    }

    pub fn __str__(&self) -> String {
        self.0.data.try_into_string().unwrap_or_else(|_| format!("{:?}", self.0))
    }

    pub fn __repr__(&self) -> String {
        format!("{:?}", self.0)
    }

}
