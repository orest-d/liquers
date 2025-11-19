use liquers_core::{context::{Context, Environment}, error::Error, state::State, value::ValueInterface};

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
