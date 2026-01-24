use liquers_core::{
    commands::CommandRegistry, context::Context, error::Error, state::State, value::ValueInterface,
};
use liquers_macro::register_command;
use crate::value::{ExtValueInterface, Value, simple::SimpleValue};
use crate::environment::CommandRegistryAccess;
use std::collections::BTreeMap;

use super::util::try_to_polars_dataframe;

/// Get DataFrame shape (rows and columns)
fn shape(state: &State<Value>) -> Result<Value, Error> {
    let df = try_to_polars_dataframe(state)?;

    let (nrows, ncols) = df.shape();

    let mut map = BTreeMap::new();
    map.insert("nrows".to_string(), SimpleValue::from(nrows as i32));
    map.insert("ncols".to_string(), SimpleValue::from(ncols as i32));

    Ok(Value::Base(SimpleValue::Object { value: map }))
}

/// Get number of rows
fn nrows(state: &State<Value>) -> Result<Value, Error> {
    let df = try_to_polars_dataframe(state)?;
    let nrows = df.height() as i32;
    Ok(Value::from(nrows))
}

/// Get number of columns
fn ncols(state: &State<Value>) -> Result<Value, Error> {
    let df = try_to_polars_dataframe(state)?;
    let ncols = df.width() as i32;
    Ok(Value::from(ncols))
}

/// Get DataFrame schema (column names and types)
fn schema(state: &State<Value>) -> Result<Value, Error> {
    let df = try_to_polars_dataframe(state)?;

    let schema = df.schema();
    let mut map = BTreeMap::new();

    for (name, dtype) in schema.iter() {
        map.insert(name.to_string(), SimpleValue::from(format!("{:?}", dtype)));
    }

    Ok(Value::Base(SimpleValue::Object { value: map }))
}

/// Register info commands
pub fn register_commands(env: &mut crate::environment::DefaultEnvironment<Value>) -> Result<(), Error> {
    type CommandEnvironment = crate::environment::DefaultEnvironment<Value>;
    let cr = env.get_mut_command_registry();
    register_command!(cr,
        fn shape(state) -> result
        namespace: "pl"
        label: "Shape"
        doc: "Get DataFrame shape (rows and columns)"
    )?;

    register_command!(cr,
        fn nrows(state) -> result
        namespace: "pl"
        label: "Number of rows"
        doc: "Get number of rows in DataFrame"
    )?;

    register_command!(cr,
        fn ncols(state) -> result
        namespace: "pl"
        label: "Number of columns"
        doc: "Get number of columns in DataFrame"
    )?;

    register_command!(cr,
        fn schema(state) -> result
        namespace: "pl"
        label: "Schema"
        doc: "Get DataFrame schema (column names and types)"
    )?;

    Ok(())
}
