//! Record streams in `liquers-lib`: the glue between the `liquers-records` crate and this crate's
//! `Value` — the value variants, the conversions every record command accepts, and the `ns-rec`
//! commands.
//!
//! Everything in `liquers-records` is re-exported here, so a crate above `liquers-lib` names
//! `liquers_lib::records::RecordBatch` without depending on `liquers-records` itself. The
//! re-export lives inside this module rather than as a crate-root alias, which would clash with
//! the module's name (E0255).

pub use liquers_records::*;

use std::sync::Arc;

use liquers_core::error::Error;

use crate::value::{ExtValueInterface, Value};

pub mod commands;
pub mod convert;
#[cfg(feature = "polars")]
pub mod polars;

pub use convert::{to_record, to_record_source, ToRecordOptions};

/// Reads Parquet bytes into a [`RecordBatch`]. `liquers-records` itself never reads Parquet — see
/// `specs/design/record-streams/phase2-architecture.md` §"Tier 3 — Parquet: writing is cheap,
/// reading is not". With the `polars` feature, this goes through [`polars::read_parquet_record_batch`]
/// to polars' own reader; without it, this refuses, naming the `polars` feature. Shared by
/// `value/mod.rs`'s `deserialize_from_bytes` (the `RecordView`/`"parquet"` case) and
/// `convert::to_record`'s byte path, so both go through the same rule.
pub fn read_parquet_record_batch(bytes: &[u8], schema: ReadSchema<'_>) -> Result<RecordBatch, Error> {
    #[cfg(feature = "polars")]
    {
        self::polars::read_parquet_record_batch(bytes, schema)
    }
    #[cfg(not(feature = "polars"))]
    {
        let _ = (bytes, schema);
        Err(Error::not_supported(
            "Parquet reading needs liquers-lib's 'polars' feature: a RecordView reads Parquet \
             through the polars bridge (records::polars), not liquers-records itself"
                .to_string(),
        ))
    }
}

/// How the records crate reads and builds a `Value` without knowing its concrete type
/// (`liquers-records` cannot name `liquers-lib`'s `Value`, which sits above it). Delegates to the
/// `ExtValueInterface` accessors Step 5.2 added to `ExtValue`/`Value`
/// (`liquers-lib/src/value/mod.rs`), converting their typed `Result` into the `Option` this
/// trait's contract asks for — a conversion failure here just means "not a record value", which
/// is exactly what `None` says.
impl RecordValue for Value {
    fn as_record_view(&self) -> Option<Arc<dyn RecordView>> {
        ExtValueInterface::as_record_view(self).ok()
    }
    fn as_record_source(&self) -> Option<Arc<dyn RecordSource>> {
        ExtValueInterface::as_record_source(self).ok()
    }
    fn from_record_view(view: Arc<dyn RecordView>) -> Self {
        <Value as ExtValueInterface>::from_record_view(view)
    }
    fn from_record_source(source: Arc<dyn RecordSource>) -> Self {
        <Value as ExtValueInterface>::from_record_source(source)
    }
}
