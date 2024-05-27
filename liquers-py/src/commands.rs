use liquers_core::state::State;
use liquers_core::{register_command, value::ValueInterface};
use pyo3::prelude::*;

use crate::value::Value;
use crate::error::Error;

#[pyclass]
pub struct CommandArguments(liquers_core::commands::CommandArguments);

#[pymethods]
impl CommandArguments {
    #[new]
    fn new() -> Self {
        let ca = liquers_core::commands::CommandArguments::new(
            liquers_core::plan::ResolvedParameterValues::new(),
        );
        CommandArguments(ca)
    }
}

#[pyclass]
pub struct CommandRegistry(
    pub  liquers_core::commands::CommandRegistry<
        crate::context::EnvRefDef,
        crate::context::Environment,
        crate::value::Value,
    >,
);

fn hello()->Result<Value,Error>{
    Ok(Value::from_string("Hello".to_string()))
}

fn greet(state:&State<Value>, who:String)->Result<Value,Error>{
    let s = state.data.try_into_string()?;

    Ok(Value::from_string(format!("{}, {}!", s, who)))
}

pub fn register_commands(cr:&mut CommandRegistry) -> Result<(),liquers_core::error::Error>{
    let cr = &mut cr.0;
    register_command!(cr, hello());
    register_command!(cr, greet(state, who:String));
    Ok(())
}

#[pymethods]
impl CommandRegistry {
    #[new]
    pub fn new() -> PyResult<Self> {
        let mut cr = liquers_core::commands::CommandRegistry::new();
        let mut cr = CommandRegistry(cr);
        register_commands(&mut cr).map_err(|e| Error(e))?;
        Ok(cr)
    }
}
