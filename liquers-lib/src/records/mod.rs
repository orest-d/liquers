//! Record streams in `liquers-lib`: the glue between the `liquers-records` crate and this crate's
//! `Value` — the value variants, the conversions every record command accepts, and the `ns-rec`
//! commands.
//!
//! Everything in `liquers-records` is re-exported here, so a crate above `liquers-lib` names
//! `liquers_lib::records::RecordBatch` without depending on `liquers-records` itself. The
//! re-export lives inside this module rather than as a crate-root alias, which would clash with
//! the module's name (E0255).

pub use liquers_records::*;
