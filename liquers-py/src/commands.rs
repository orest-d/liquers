use liquers_core::context::ContextInterface;
use liquers_core::state::State;
use liquers_core::{register_command, value::ValueInterface};
use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};

use crate::error::Error;
use crate::value::{value_to_pyobject, Value};

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

fn json2py(py: Python, json: &serde_json::Value) -> PyResult<PyObject> {
    match json {
        serde_json::Value::Null => Ok(py.None()),
        serde_json::Value::Bool(x) => Ok(x.into_py(py)),
        serde_json::Value::Number(x) => {
            if x.is_i64() {
                Ok(x.as_i64().unwrap().into_py(py))
            } else if x.is_u64() {
                Ok(x.as_u64().unwrap().into_py(py))
            } else if x.is_f64() {
                Ok(x.as_f64().unwrap().into_py(py))
            } else {
                Err(PyErr::new::<PyException, _>(
                    "Number is not i64, u64 or f64",
                ))
            }
        }
        serde_json::Value::String(x) => Ok(x.into_py(py)),
        serde_json::Value::Array(v) => {
            let list = PyList::empty_bound(py);
            for x in v.iter() {
                list.append(json2py(py, x)?)?;
            }
            Ok(list.into())
        }
        serde_json::Value::Object(o) => {
            let dict = PyDict::new_bound(py);
            for (k, v) in o.iter() {
                dict.set_item(k, json2py(py, v)?)?;
            }
            Ok(dict.into())
        }
    }
}

fn parameter2py(
    py: Python,
    p: &liquers_core::plan::ParameterValue,
    context: &liquers_core::context::Context<
        crate::context::EnvRefDef,
        crate::context::Environment,
    >,
) -> PyResult<PyObject> {
    match p {
        liquers_core::plan::ParameterValue::DefaultValue(x) => json2py(py, x),
        liquers_core::plan::ParameterValue::DefaultLink(_) => todo!(),
        liquers_core::plan::ParameterValue::ParameterValue(x, pos) => json2py(py, x),
        liquers_core::plan::ParameterValue::ParameterLink(_, _) => todo!(),
        liquers_core::plan::ParameterValue::EnumLink(_, _) => todo!(),
        liquers_core::plan::ParameterValue::Injected(name) => {
            if name=="context"{
                Ok(crate::context::Context(context.clone_context()).into_py(py))
            }
            else{
                Err(PyErr::new::<PyException, _>(
                    format!("Injected parameter '{name}' is not supported"),
                ))
            }
        },
        liquers_core::plan::ParameterValue::None => todo!(),
        liquers_core::plan::ParameterValue::MultipleParameters(vec) => todo!(),
    }
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
    let pass_state: String = arg.get(&context)?;

    let state_value: Option<PyObject> = if pass_state == "no" {
        None
    } else if pass_state == "pyobject" {
        Some(Python::with_gil(|py| {
            value_to_pyobject(&(*state.data), py)
        })?)
    } else if pass_state == "state" {
        Some(Python::with_gil(|py| {
            crate::state::State(state.clone()).into_py(py)
        }))
    } else if pass_state == "value" {
        let data = (&*state.data).clone();
        Some(Python::with_gil(|py| data.into_py(py)))
    } else {
        return Err(liquers_core::error::Error::unexpected_error(format!(
            "Invalid pass_state in pycall: '{pass_state}'"
        )));
    };

    //let s = state.data.try_into_string()?;
    let res = Python::with_gil(|py| {
        let m = if module == "builtins" || module == "" {
            PyModule::import(py, "builtins")?
        } else {
            PyModule::import(py, &*module)?
        };
        let mut argv: Vec<PyObject> = Vec::new();

        if let Some(sv) = state_value {
            argv.push(sv);
        }
        for (i, a) in arg.parameters.0.iter().enumerate().skip(3) {
            match a {
                liquers_core::plan::ParameterValue::MultipleParameters(vec) => {
                    for p in vec.iter() {
                        argv.push(parameter2py(py, p, &context)?);
                    }
                }
                _ => {
                    argv.push(parameter2py(py, a, &context)?);
                }
            }
        }
        let argv_pytuple = PyTuple::new_bound(py, argv);

        let f = m.getattr(&*function)?;
        let res = f.call1(argv_pytuple)?;
        //let builtins = PyModule::import_bound(py, "builtins")?;
        //builtins.getattr("print")?.call1(("PRINT <", &s, ">"))?;
        Ok(Value::Py { value: res.into() })
    })
    .map_err(|e: PyErr| {
        liquers_core::error::Error::general_error(format!("Python exception: {e}"))
    })?;
    Ok(res)
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
            liquers_core::command_metadata::ArgumentInfo::argument("pass_state")
                .with_default("pyobject")
                .with_type(liquers_core::command_metadata::ArgumentType::Enum(
                    liquers_core::command_metadata::EnumArgument::new("PassState")
                        .with_alternative("no")
                        .with_alternative("pyobject")
                        .with_alternative("value")
                        .with_alternative("state"),
                )),
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
