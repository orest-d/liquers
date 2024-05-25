use pyo3::{exceptions::PyException, prelude::*};
use crate::{context::{EnvRef, Environment}, error::Error, state::State};
use liquers_core::interpreter::PlanInterpreter;

#[pyfunction]
pub fn evaluate(query:String) -> PyResult<State> {
    //let cmr = &command_metadata_registry.0;
    let envref = Environment::new().to_ref();

    let mut pi = PlanInterpreter::new(envref);
    let state = pi.evaluate(query).map_err(|e| Error(e))?;
    Ok(State(state))
}