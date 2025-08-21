extern crate serde;
#[macro_use]
extern crate serde_derive;

use pyo3::{exceptions::PyException, prelude::*};
pub mod parse;
pub mod store;
pub mod metadata;
pub mod cache;
pub mod error;
pub mod value;
pub mod state;
pub mod command_metadata;
pub mod commands;
pub mod context;
pub mod plan;
pub mod interpreter;
pub mod recipes;
use crate::parse::*;
use crate::error::Error;

/// A Python module implemented in Rust.
#[pymodule]
fn liquers_py(py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<Position>()?;
    m.add_class::<ActionParameter>()?;
    m.add_class::<ResourceName>()?;
    m.add_class::<ActionRequest>()?;
    m.add_class::<SegmentHeader>()?;
    m.add_class::<TransformQuerySegment>()?;
    m.add_class::<Key>()?;
    m.add_class::<ResourceQuerySegment>()?;
    m.add_class::<Query>()?;
    m.add_function(wrap_pyfunction!(crate::parse::parse, m)?)?;
    m.add_function(wrap_pyfunction!(crate::parse::parse_key, m)?)?;

    m.add_class::<crate::metadata::Metadata>()?;
    m.add_class::<crate::metadata::MetadataRecord>()?;
    m.add_class::<crate::metadata::AssetInfo>()?;

    m.add_class::<crate::recipes::Recipe>()?;

    m.add_class::<crate::store::Store>()?;
    m.add_function(wrap_pyfunction!(crate::store::local_filesystem_store, m)?)?;

    m.add_class::<crate::cache::Cache>()?;
    m.add_function(wrap_pyfunction!(crate::cache::memory_cache, m)?)?;

    m.add_class::<crate::error::ErrorType>()?;
    m.add_class::<crate::error::Error>()?;

    m.add_class::<crate::value::Value>()?;

    m.add_class::<crate::command_metadata::ArgumentInfo>()?;
    m.add_class::<crate::command_metadata::CommandMetadata>()?;
    m.add_class::<crate::command_metadata::CommandKey>()?;
    m.add_class::<crate::command_metadata::CommandMetadataRegistry>()?;

    m.add_class::<crate::commands::CommandArguments>()?;
    m.add_class::<crate::commands::CommandRegistry>()?;

    m.add_class::<crate::context::Environment>()?;
    m.add_class::<crate::context::Context>()?;

    m.add_class::<crate::state::State>()?;

    m.add_class::<crate::plan::Plan>()?;
    m.add_function(wrap_pyfunction!(crate::plan::build_plan, m)?)?;

    m.add_function(wrap_pyfunction!(crate::interpreter::evaluate, m)?)?;
    m.add_function(wrap_pyfunction!(crate::interpreter::evaluate_with_cmr, m)?)?;

    Ok(())
}
