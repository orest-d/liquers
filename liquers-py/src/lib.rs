extern crate serde;
#[macro_use]
extern crate serde_derive;

use pyo3::prelude::*;

pub mod command_metadata;
pub mod context;
pub mod dependencies;
pub mod error;
pub mod expiration;
pub mod metadata;
pub mod parse;
pub mod plan;
pub mod query;
pub mod recipes;
pub mod runtime;
pub mod value;

use crate::query::*;

/// The native extension, `liquers._native`; the `liquers` package (`python/liquers/`) re-exports it.
#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Stored `py.Object` values are read back through this decoder; an environment with the
    // pickle codec off lists no formats for the type, so the read path never reaches it there.
    liquers_lib::value::foreign::register_foreign_decoder(
        crate::value::PY_OBJECT_TYPE_IDENTIFIER,
        crate::value::decode_pickle,
    );
    m.add_class::<Position>()?;
    m.add_class::<ActionParameter>()?;
    m.add_class::<ResourceName>()?;
    m.add_class::<ActionRequest>()?;
    m.add_class::<SegmentHeader>()?;
    m.add_class::<TransformQuerySegment>()?;
    m.add_class::<Key>()?;
    m.add_class::<ResourceQuerySegment>()?;
    m.add_class::<Query>()?;
    m.add_class::<QuerySource>()?;
    m.add_function(wrap_pyfunction!(crate::parse::parse, m)?)?;
    m.add_function(wrap_pyfunction!(crate::parse::parse_key, m)?)?;

    m.add_class::<crate::command_metadata::EnumArgumentType>()?;
    m.add_class::<crate::command_metadata::EnumArgument>()?;
    m.add_class::<crate::command_metadata::ArgumentType>()?;
    m.add_class::<crate::command_metadata::ArgumentInfo>()?;
    m.add_class::<crate::command_metadata::CommandDefinition>()?;
    m.add_class::<crate::command_metadata::CommandPreset>()?;
    m.add_class::<crate::command_metadata::CommandMetadata>()?;
    m.add_class::<crate::command_metadata::CommandKey>()?;
    m.add_class::<crate::command_metadata::CommandMetadataRegistry>()?;

    m.add_class::<crate::metadata::Metadata>()?;
    m.add_class::<crate::metadata::Version>()?;
    m.add_class::<crate::metadata::DependencyKey>()?;
    m.add_class::<crate::metadata::DependencyRecord>()?;
    m.add_class::<crate::metadata::MetadataRecord>()?;
    m.add_class::<crate::metadata::AssetInfo>()?;
    m.add_class::<crate::metadata::Status>()?;
    m.add_class::<crate::metadata::LogEntry>()?;
    m.add_class::<crate::metadata::LogEntryKind>()?;

    m.add_class::<crate::error::ErrorType>()?;
    m.add_class::<crate::error::Error>()?;

    m.add_class::<crate::expiration::Expires>()?;
    m.add_class::<crate::expiration::ExpirationTime>()?;

    m.add_class::<crate::dependencies::DependencyRelation>()?;
    m.add_class::<crate::dependencies::PlanDependency>()?;

    m.add_class::<crate::plan::Plan>()?;
    m.add_class::<crate::plan::Step>()?;
    m.add_class::<crate::plan::ParameterValue>()?;
    m.add_class::<crate::plan::ResolvedParameterValues>()?;
    m.add_function(wrap_pyfunction!(crate::plan::build_plan, m)?)?;

    m.add_class::<crate::recipes::Recipe>()?;
    m.add_class::<crate::recipes::RecipeList>()?;

    Ok(())
}
