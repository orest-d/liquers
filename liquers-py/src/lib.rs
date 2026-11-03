extern crate serde;
#[macro_use]
extern crate serde_derive;

use pyo3::prelude::*;

pub mod aio;
pub mod command_metadata;
pub mod context;
pub mod asset;
pub mod builtins;
pub mod command;
pub mod dependencies;
pub mod environment;
pub mod error;
pub mod exit;
pub mod expiration;
pub mod metadata;
pub mod parse;
pub mod plan;
pub mod pycall;
pub mod query;
pub mod recipe_provider;
pub mod recipes;
pub mod records;
pub mod runtime;
pub mod store;
pub mod value;

use crate::query::*;

/// The native extension, `liquers._native`; the `liquers` package (`python/liquers/`) re-exports it.
///
/// Declared safe without the GIL, so a free-threaded interpreter keeps the GIL off when it imports
/// `liquers` (PyO3's default since 0.28, stated here so it is a decision rather than an
/// inheritance). The audit behind it is in `specs/design/liquers-py-async/phase2-architecture.md`;
/// `gil_used = true` is the fallback if a free-threading defect is found.
#[pymodule(gil_used = false)]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Stored `py.Object` values are read back through this decoder; an environment with the
    // pickle codec off lists no formats for the type, so the read path never reaches it there.
    liquers_lib::value::foreign::register_foreign_decoder(
        crate::value::PY_OBJECT_TYPE_IDENTIFIER,
        crate::value::decode_pickle,
    );
    m.add_class::<store::StoreArgumentInfo>()?;
    m.add_class::<store::handle::Store>()?;
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
    m.add_class::<crate::recipe_provider::RecipeProviders>()?;

    m.add_class::<crate::environment::Environment>()?;
    m.add_class::<crate::environment::Issue>()?;
    m.add_class::<crate::context::Context>()?;
    m.add_class::<crate::asset::State>()?;
    m.add_class::<crate::asset::AssetRef>()?;
    crate::records::register(m)?;

    // The Liquers version, in the form Python packaging uses (PEP 440); `liquers.__version__`.
    m.add("__version__", pep440(liquers_core::VERSION))?;

    // Registered first, so it runs after every environment's own `atexit` hooks.
    crate::exit::register(m)?;

    Ok(())
}

/// The PEP 440 form of a Cargo (SemVer) version, written the way maturin writes a wheel's version:
/// the pre-release suffixes `-alpha.N`, `-beta.N` and `-rc.N` become `aN`, `bN` and `rcN`. A
/// version without a pre-release (`0.1.0`) is returned unchanged; build metadata (`+…`) is kept.
pub fn pep440(cargo: &str) -> String {
    let (core, build) = match cargo.split_once('+') {
        Some((core, build)) => (core, Some(build)),
        None => (cargo, None),
    };
    let mut out = match core.split_once('-') {
        Some((release, pre)) => {
            let (label, number) = match pre.split_once('.') {
                Some((label, number)) => (label, number),
                None => (pre, "0"),
            };
            let short = match label {
                "alpha" | "a" => "a",
                "beta" | "b" => "b",
                "rc" => "rc",
                other => return format!("{release}.dev0+{other}.{number}"),
            };
            format!("{release}{short}{number}")
        }
        None => core.to_string(),
    };
    if let Some(build) = build {
        out.push('+');
        out.push_str(build);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::pep440;

    #[test]
    fn pep440_rewrites_prereleases() {
        assert_eq!(pep440("0.1.0"), "0.1.0");
        assert_eq!(pep440("0.2.0-alpha.1"), "0.2.0a1");
        assert_eq!(pep440("0.2.0-beta.3"), "0.2.0b3");
        assert_eq!(pep440("0.2.0-rc.1"), "0.2.0rc1");
        assert_eq!(pep440("0.2.0-rc"), "0.2.0rc0");
        assert_eq!(pep440("1.0.0+abc"), "1.0.0+abc");
    }
}
