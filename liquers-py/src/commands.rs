use liquers_core::context::ContextInterface;
use liquers_core::state::State;
use liquers_core::{register_command, value::ValueInterface};
use pyo3::prelude::*;

use crate::error::Error;
use crate::value::Value;

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

fn hello() -> Result<Value, Error> {
    Ok(Value::from_string("Hello".to_string()))
}

fn greet(state: &State<Value>, who: String) -> Result<Value, Error> {
    let s = state.data.try_into_string()?;

    Ok(Value::from_string(format!("{}, {}!", s, who)))
}

fn pyprint(state: &State<Value>) -> Result<Value, Error> {
    let s = state.data.try_into_string()?;
    Python::with_gil(|py| {
        let builtins = PyModule::import_bound(py, "builtins")?;
        builtins.getattr("print")?.call1(("PRINT <", &s, ">"))?;
        Ok(())
    })
    .map_err(|e: PyErr| {
        liquers_core::error::Error::general_error(format!("Python exception: {e}"))
    })?;

    Ok(Value::none())
}

fn pycall(
    state: &State<Value>,
    arg: &mut liquers_core::commands::CommandArguments,
    context: liquers_core::context::Context<crate::context::EnvRefDef, crate::context::Environment>,
) -> Result<Value, liquers_core::error::Error> {
    println!("pycall");
    context.info("pycall called");
    //let context_par = arg.pop_parameter()?;
    let module: String = arg.get(&context)?;
    let function: String = arg.get(&context)?;
    let argv_par = arg.pop_parameter()?;
    let argv = argv_par
        .value()
        .map(|x| x.as_array().map(|xx| 
            xx.iter().map(|xxx| 
                match xxx {
                    serde_json::Value::Null => format!("null"),
                    serde_json::Value::Bool(x) => format!("{x}"),
                    serde_json::Value::Number(x) => format!("{x}"),
                    serde_json::Value::String(x) => x.to_string(),
                    serde_json::Value::Array(x) => format!("{x:?}"),
                    serde_json::Value::Object(x) => format!("{x:?}"),
                }
            ).collect::<Vec<String>>()))
        .flatten()
        .ok_or_else(|| {
            liquers_core::error::Error::general_error(format!("pycall argv is not an array"))
        })?;

    println!("pycall {}.{}({:?})", module, function, argv);
    for arg in arg.parameters.0.iter() {
        println!("arg: {:?}", arg);
    }

    let s = state.data.try_into_string()?;
    Python::with_gil(|py| {
        let m = if module == "builtins" || module == ""{
            PyModule::import(py, "builtins")?
        } else {
            PyModule::import(py, &*module)?
        };
        let f = m.getattr(&*function)?;
        let res = f.call1((&s, argv))?;
        //let builtins = PyModule::import_bound(py, "builtins")?;
        //builtins.getattr("print")?.call1(("PRINT <", &s, ">"))?;
        Ok(())
    })
    .map_err(|e: PyErr| {
        liquers_core::error::Error::general_error(format!("Python exception: {e}"))
    })?;

    Ok(Value::none())
}

pub fn register_commands(cr: &mut CommandRegistry) -> Result<(), liquers_core::error::Error> {
    let cr = &mut cr.0;
    register_command!(cr, hello());
    register_command!(cr, greet(state, who:String));
    register_command!(cr, pyprint(state));
    {
        let reg_command_metadata = cr.register_command("pycall", pycall)?;
        reg_command_metadata.with_name("pycall");
        reg_command_metadata.with_state_argument(
            liquers_core::command_metadata::ArgumentInfo::argument("state"),
        );
        /*
        reg_command_metadata.with_argument(
            liquers_core::command_metadata::ArgumentInfo::argument("context").set_injected(),
        );
        */
        reg_command_metadata
            .with_argument(liquers_core::command_metadata::ArgumentInfo::string_argument("module"));
        reg_command_metadata.with_argument(
            liquers_core::command_metadata::ArgumentInfo::string_argument("function"),
        );
        reg_command_metadata.with_argument(
            liquers_core::command_metadata::ArgumentInfo::argument("argv").set_multiple(),
        );
    };
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
