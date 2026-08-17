use crate::api_core::response::ErrorDetail;
use axum::http::StatusCode;
use liquers_core::error::{Error, ErrorType};

/// Map liquers_core::error::ErrorType to HTTP status code (per spec 3.3)
/// All ErrorType variants are explicitly handled
pub fn error_to_status_code(error_type: ErrorType) -> StatusCode {
    match error_type {
        ErrorType::KeyNotFound => StatusCode::NOT_FOUND,
        ErrorType::KeyNotSupported => StatusCode::NOT_FOUND,
        // 400, not 404: the caller supplied something that is not a store address at all, which is
        // a malformed request rather than a resource that happens to be absent.
        ErrorType::KeyNotAbsolute => StatusCode::BAD_REQUEST,
        ErrorType::ParseError => StatusCode::BAD_REQUEST,
        ErrorType::UnknownCommand => StatusCode::BAD_REQUEST,
        ErrorType::ParameterError => StatusCode::BAD_REQUEST,
        ErrorType::ArgumentMissing => StatusCode::BAD_REQUEST,
        ErrorType::TooManyParameters => StatusCode::BAD_REQUEST,
        ErrorType::ActionNotRegistered => StatusCode::BAD_REQUEST,
        ErrorType::ConversionError => StatusCode::UNPROCESSABLE_ENTITY,
        ErrorType::SerializationError => StatusCode::UNPROCESSABLE_ENTITY,
        ErrorType::KeyReadError => StatusCode::INTERNAL_SERVER_ERROR,
        ErrorType::KeyWriteError => StatusCode::INTERNAL_SERVER_ERROR,
        ErrorType::ExecutionError => StatusCode::INTERNAL_SERVER_ERROR,
        ErrorType::UnexpectedError => StatusCode::INTERNAL_SERVER_ERROR,
        ErrorType::General => StatusCode::INTERNAL_SERVER_ERROR,
        ErrorType::CommandAlreadyRegistered => StatusCode::CONFLICT,
        ErrorType::CacheNotSupported => StatusCode::NOT_IMPLEMENTED,
        ErrorType::NotSupported => StatusCode::NOT_IMPLEMENTED,
        ErrorType::NotAvailable => StatusCode::NOT_FOUND,
        ErrorType::DependencyVersionMismatch => StatusCode::CONFLICT,
        ErrorType::DependencyCycle => StatusCode::CONFLICT,
        // Asset processing was cancelled: 499 Client Closed Request (falls back to 503 if the
        // non-standard code is unavailable).
        ErrorType::Cancelled => {
            StatusCode::from_u16(499).unwrap_or(StatusCode::SERVICE_UNAVAILABLE)
        }
    }
}

/// Convert liquers Error to ErrorDetail
pub fn error_to_detail(error: &Error) -> ErrorDetail {
    ErrorDetail {
        error_type: format!("{:?}", error.error_type),
        message: error.message.clone(),
        query: error.query.clone(), // Already encoded in Error
        key: error.key.clone(),     // Already encoded in Error
        traceback: None,            // Error struct doesn't have traceback field
        metadata: None,
    }
}

/// Parses the string form written by [`error_to_detail`] back into an [`ErrorType`].
///
/// **This table is not checked by the compiler.** It matches on `&str`, so the `_` arm is
/// unavoidable and a newly added `ErrorType` variant compiles fine while silently failing to parse
/// — after which [`crate::axum_integration`] falls back to 500 and the variant's entry in
/// [`error_to_status_code`] is never reached. `keyabs15b` is the guard: it round-trips every
/// variant and is kept exhaustive by [`tests::each_error_type`].
pub fn parse_error_type(type_str: &str) -> Result<ErrorType, String> {
    match type_str {
        "KeyNotFound" => Ok(ErrorType::KeyNotFound),
        "KeyNotSupported" => Ok(ErrorType::KeyNotSupported),
        "KeyNotAbsolute" => Ok(ErrorType::KeyNotAbsolute),
        "ParseError" => Ok(ErrorType::ParseError),
        "UnknownCommand" => Ok(ErrorType::UnknownCommand),
        "ParameterError" => Ok(ErrorType::ParameterError),
        "ArgumentMissing" => Ok(ErrorType::ArgumentMissing),
        "TooManyParameters" => Ok(ErrorType::TooManyParameters),
        "ConversionError" => Ok(ErrorType::ConversionError),
        "SerializationError" => Ok(ErrorType::SerializationError),
        "KeyReadError" => Ok(ErrorType::KeyReadError),
        "KeyWriteError" => Ok(ErrorType::KeyWriteError),
        "ExecutionError" => Ok(ErrorType::ExecutionError),
        "UnexpectedError" => Ok(ErrorType::UnexpectedError),
        "General" => Ok(ErrorType::General),
        "CommandAlreadyRegistered" => Ok(ErrorType::CommandAlreadyRegistered),
        "ActionNotRegistered" => Ok(ErrorType::ActionNotRegistered),
        "CacheNotSupported" => Ok(ErrorType::CacheNotSupported),
        "NotSupported" => Ok(ErrorType::NotSupported),
        "NotAvailable" => Ok(ErrorType::NotAvailable),
        "DependencyVersionMismatch" => Ok(ErrorType::DependencyVersionMismatch),
        "DependencyCycle" => Ok(ErrorType::DependencyCycle),
        "Cancelled" => Ok(ErrorType::Cancelled),
        _ => Err(format!("Unknown error type: {}", type_str)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every `ErrorType`, kept complete by the compiler.
    ///
    /// The `match` is what does the work: it has no `_` arm, so adding a variant to `ErrorType`
    /// fails to compile here until the variant is added to the list as well. Without it, `keyabs15b`
    /// would silently stop covering new variants — which is exactly how `KeyNotAbsolute` reached
    /// `error_to_status_code` while `parse_error_type` had no arm for it.
    fn each_error_type() -> Vec<ErrorType> {
        let all = vec![
            ErrorType::ArgumentMissing,
            ErrorType::ActionNotRegistered,
            ErrorType::CommandAlreadyRegistered,
            ErrorType::ParseError,
            ErrorType::ParameterError,
            ErrorType::TooManyParameters,
            ErrorType::ConversionError,
            ErrorType::SerializationError,
            ErrorType::General,
            ErrorType::CacheNotSupported,
            ErrorType::UnknownCommand,
            ErrorType::NotSupported,
            ErrorType::NotAvailable,
            ErrorType::KeyNotFound,
            ErrorType::KeyNotSupported,
            ErrorType::KeyNotAbsolute,
            ErrorType::KeyReadError,
            ErrorType::KeyWriteError,
            ErrorType::UnexpectedError,
            ErrorType::ExecutionError,
            ErrorType::DependencyVersionMismatch,
            ErrorType::DependencyCycle,
            ErrorType::Cancelled,
        ];
        for error_type in &all {
            match error_type {
                ErrorType::ArgumentMissing
                | ErrorType::ActionNotRegistered
                | ErrorType::CommandAlreadyRegistered
                | ErrorType::ParseError
                | ErrorType::ParameterError
                | ErrorType::TooManyParameters
                | ErrorType::ConversionError
                | ErrorType::SerializationError
                | ErrorType::General
                | ErrorType::CacheNotSupported
                | ErrorType::UnknownCommand
                | ErrorType::NotSupported
                | ErrorType::NotAvailable
                | ErrorType::KeyNotFound
                | ErrorType::KeyNotSupported
                | ErrorType::KeyNotAbsolute
                | ErrorType::KeyReadError
                | ErrorType::KeyWriteError
                | ErrorType::UnexpectedError
                | ErrorType::ExecutionError
                | ErrorType::DependencyVersionMismatch
                | ErrorType::DependencyCycle
                | ErrorType::Cancelled => {}
            }
        }
        all
    }

    /// `keyabs15b` — every error type survives the round trip an HTTP response actually makes.
    ///
    /// `error_to_detail` writes `format!("{:?}")` and `ApiResponse::into_response` reads it back
    /// with `parse_error_type` before consulting `error_to_status_code`. A variant missing from
    /// that table parses as `Err`, is discarded by `.ok()`, and the response falls back to 500 —
    /// so the status mapping is right and unreachable at the same time. `keyabs15` alone did not
    /// catch that, because it called `error_to_status_code` directly.
    #[test]
    fn keyabs15b_every_error_type_round_trips_through_the_response_path() {
        for error_type in each_error_type() {
            let serialized = format!("{:?}", error_type);
            let parsed = parse_error_type(&serialized).unwrap_or_else(|e| {
                panic!("{serialized} does not round trip: {e} — the response would fall back to 500")
            });
            assert_eq!(parsed, error_type, "{serialized}");
        }
    }

    /// `keyabs15` — a relative key is a bad request, not a missing resource.
    ///
    /// The contrast with `KeyNotSupported` is the point of the separate error type — one says the
    /// address is malformed, the other that no store serves it.
    ///
    /// This asserts the mapping; `keyabs15b` asserts that the mapping is reachable. Both are
    /// needed, and `AXUM-HANDLER-TEST-COVERAGE` is why neither goes through a real handler.
    #[test]
    fn keyabs15_key_not_absolute_is_bad_request() {
        assert_eq!(
            error_to_status_code(ErrorType::KeyNotAbsolute),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            error_to_status_code(ErrorType::KeyNotSupported),
            StatusCode::NOT_FOUND
        );
    }

    #[test]
    fn test_error_to_status_code_not_found() {
        assert_eq!(
            error_to_status_code(ErrorType::KeyNotFound),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            error_to_status_code(ErrorType::KeyNotSupported),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            error_to_status_code(ErrorType::NotAvailable),
            StatusCode::NOT_FOUND
        );
    }

    #[test]
    fn test_error_to_status_code_bad_request() {
        assert_eq!(
            error_to_status_code(ErrorType::ParseError),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            error_to_status_code(ErrorType::UnknownCommand),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            error_to_status_code(ErrorType::ParameterError),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            error_to_status_code(ErrorType::TooManyParameters),
            StatusCode::BAD_REQUEST
        );
    }

    #[test]
    fn test_error_to_status_code_unprocessable() {
        assert_eq!(
            error_to_status_code(ErrorType::ConversionError),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            error_to_status_code(ErrorType::SerializationError),
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }

    #[test]
    fn test_error_to_status_code_server_error() {
        assert_eq!(
            error_to_status_code(ErrorType::ExecutionError),
            StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(
            error_to_status_code(ErrorType::UnexpectedError),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test]
    fn test_error_to_status_code_not_implemented() {
        assert_eq!(
            error_to_status_code(ErrorType::CacheNotSupported),
            StatusCode::NOT_IMPLEMENTED
        );
        assert_eq!(
            error_to_status_code(ErrorType::NotSupported),
            StatusCode::NOT_IMPLEMENTED
        );
    }

    #[test]
    fn test_error_to_status_code_conflict() {
        assert_eq!(
            error_to_status_code(ErrorType::CommandAlreadyRegistered),
            StatusCode::CONFLICT
        );
    }

    #[test]
    fn test_error_to_detail() {
        let error = Error::general_error("Test error".to_string());

        let detail = error_to_detail(&error);
        assert_eq!(detail.error_type, "General");
        assert_eq!(detail.message, "Test error");
        assert_eq!(detail.query, None);
        assert_eq!(detail.key, None);
    }

    #[test]
    fn test_parse_error_type() {
        assert_eq!(
            parse_error_type("KeyNotFound").unwrap(),
            ErrorType::KeyNotFound
        );
        assert_eq!(
            parse_error_type("ParseError").unwrap(),
            ErrorType::ParseError
        );
        assert_eq!(
            parse_error_type("NotAvailable").unwrap(),
            ErrorType::NotAvailable
        );
        assert!(parse_error_type("InvalidType").is_err());
    }

    #[test]
    fn test_all_error_types_mapped() {
        // Test that all ErrorType variants have explicit status code mappings
        let all_types = vec![
            ErrorType::KeyNotFound,
            ErrorType::KeyNotSupported,
            ErrorType::ParseError,
            ErrorType::UnknownCommand,
            ErrorType::ParameterError,
            ErrorType::ArgumentMissing,
            ErrorType::TooManyParameters,
            ErrorType::ConversionError,
            ErrorType::SerializationError,
            ErrorType::KeyReadError,
            ErrorType::KeyWriteError,
            ErrorType::ExecutionError,
            ErrorType::UnexpectedError,
            ErrorType::General,
            ErrorType::CommandAlreadyRegistered,
            ErrorType::ActionNotRegistered,
            ErrorType::CacheNotSupported,
            ErrorType::NotSupported,
            ErrorType::NotAvailable,
            ErrorType::DependencyVersionMismatch,
            ErrorType::DependencyCycle,
        ];

        for error_type in all_types {
            let status = error_to_status_code(error_type);
            // Just verify it returns a valid status code
            assert!((400..600).contains(&status.as_u16()));
        }
    }
}
