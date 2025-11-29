use liquers_core::{context::{Context, Environment}, error::Error, state::State, value::ValueInterface};
use liquers_macro::register_command;

use crate::{environment::{CommandRegistryAccess, DefaultEnvironment}, value::Value};

/// Generic command trying to convert any value to text representation.
pub fn to_text<E:Environment>(state: &State<E::Value>, _context:Context<E>) -> Result<E::Value, Error> {
    Ok(E::Value::from_string(state.try_into_string()?))
} 

/// Generic command trying to extract metadata from the state.
pub fn to_metadata<E:Environment>(state: &State<E::Value>, _context:Context<E>) -> Result<E::Value, Error> {
    if let Some(metadata) = state.metadata.metadata_record() {
        Ok(E::Value::from_metadata(metadata))
    }
    else{
        Err(Error::general_error("Legacy metadata not supported in to_metadata command".to_string()))
    }
} 


pub fn register_commands(mut env:DefaultEnvironment<Value>) -> Result<DefaultEnvironment<Value>, Error> {
    let cr = env.get_mut_command_registry();

    type CommandEnvironment = DefaultEnvironment<Value>;
    register_command!(cr,
        fn to_text(state, context) -> result
        label: "To text"
        doc: "Convert input state to string"
        filename: "text.txt"
    )?;
    register_command!(cr, fn to_metadata(state, context) -> result
        label: "To metadata"
        doc: "Extract metadata from input state"
        filename: "metadata.json"
    )?;
    
    Ok(env)
}

pub fn register_all_commands(mut env:DefaultEnvironment<Value>) -> Result<DefaultEnvironment<Value>, Error> {
    env = register_commands(env)?;
    env = crate::egui::commands::register_commands(env)?;
    Ok(env)
}
