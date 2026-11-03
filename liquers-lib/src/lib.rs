pub mod commands;
#[cfg(feature = "egui")]
pub mod egui;
pub mod environment;
pub mod image;
#[cfg(feature = "markdown")]
pub mod markdown;
#[cfg(feature = "mem")]
pub mod mem;
#[cfg(feature = "polars")]
pub mod polars;
#[cfg(feature = "records")]
pub mod records;
#[cfg(feature = "records")]
pub mod search;
#[cfg(feature = "records")]
pub mod store_config;
#[cfg(feature = "templates")]
pub mod template;
pub mod ui;
pub mod utils;
pub mod value;

/// Whether this build of the crate has the `records` feature, so `ExtValue::RecordView` and
/// `RecordSource` exist. A dependent crate that matches on `ExtValue` and forwards the feature
/// (`liquers-web`) asserts against it: feature unification can turn `records` on here without
/// turning it on there, and the assertion names the fix where a bare non-exhaustive match would not.
pub const RECORDS_ENABLED: bool = cfg!(feature = "records");
