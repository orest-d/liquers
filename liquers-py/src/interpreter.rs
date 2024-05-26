use std::sync::Arc;

use pyo3::{exceptions::PyException, prelude::*};
use crate::{commands::CommandRegistry, context::{EnvRefDef, Environment}, error::Error, state::State};
use liquers_core::interpreter::PlanInterpreter;

#[pyfunction]
pub fn evaluate(query:String) -> PyResult<State> {
    //let cmr = &command_metadata_registry.0;
    //let envref = Environment::new().to_ref();
    let cr = CommandRegistry::new()?;
    let mut env = Environment::new();
    env.command_registry = cr.0;
    let envref = liquers_core::context::ArcEnvRef(Arc::new(env));

    let mut pi = PlanInterpreter::new(envref);
    let state = pi.evaluate(query).map_err(|e| Error(e))?;
    Ok(State(state))
}