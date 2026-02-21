use crate::api_core::response::ErrorDetail;
use axum::http::StatusCode;
use liquers_core::error::{Error, ErrorType};

/// Map liquers_core::error::ErrorType to HTTP status code (per spec 3.3)
/// All ErrorType variants are explicitly handled
pub fn error_to_status_code(error_type: ErrorType) -> StatusCode {
    match error_type {
        ErrorType::KeyNotFound => StatusCode::NOT_FOUND,
        ErrorType::KeyNotSupported => StatusCode::NOT_FOUND,
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

/// Parse ErrorType from string (for deserialization)
pub fn parse_error_type(type_str: &str) -> Result<ErrorType, String> {
    match type_str {
        "KeyNotFound" => Ok(ErrorType::KeyNotFound),
        "KeyNotSupported" => Ok(ErrorType::KeyNotSupported),
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
        _ => Err(format!("Unknown error type: {}", type_str)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        ];

        for error_type in all_types {
            let status = error_to_status_code(error_type);
            // Just verify it returns a valid status code
            assert!((400..600).contains(&status.as_u16()));
        }
    }
}
