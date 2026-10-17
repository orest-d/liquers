pub mod error;
pub mod format;
pub mod overrides;
pub mod response;

pub use error::{error_to_detail, error_to_status_code, parse_error_type};
pub use format::{
    deserialize_data_entry, entry_format_param, format_from_content_type, negotiate_format,
    parse_entry_format, select_format, serialize_data_entry, ENTRY_FORMAT_OPTION,
};
pub use overrides::{OverrideInput, OverrideRequest, RouteOptions, LINK_PREFIX};
pub use response::{
    ApiResponse, BinaryResponse, DataEntry, ErrorDetail, ResponseStatus, SerializationFormat,
};

/// Join a builder's base path and a route below it with exactly one `/`, so `"/api/x"` and
/// `"/api/x/"` register the same routes (`"/api/x/data/{*key}"`) and `""` or `"/"` registers
/// `"/data/{*key}"`. Design: `specs/design/axum-query-api-base-slash/`.
pub(crate) fn join_base(base: &str, rest: &str) -> String {
    format!("{}/{}", base.trim_end_matches('/'), rest)
}
