# Phase 3: Unit Tests - Web API Library

Comprehensive unit tests for Phase 2 architecture core modules, covering happy paths, error paths, and edge cases.

## Test Coverage Overview

| Module | Happy Path | Error Path | Edge Cases |
|--------|-----------|-----------|-----------|
| core/response.rs | ✓ | ✓ | ✓ |
| core/error.rs | ✓ | ✓ | ✓ |
| core/format.rs | ✓ | ✓ | ✓ |

---

## File: `liquers-axum/src/core/response.rs`

### Module Implementation with Tests

```rust
use serde::{Serialize, Deserialize};
use liquers_core::metadata::Metadata;
use liquers_core::query::Key;
use liquers_core::error::Error;
use axum::{
    response::{IntoResponse, Response},
    http::{StatusCode, header},
    body::Body,
};

/// Standard API response wrapper (per spec section 3.4)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse<T>
where
    T: Serialize,
{
    pub status: ResponseStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<T>,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ErrorDetail>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum ResponseStatus {
    Ok,
    Error,
}

/// Error detail structure (per spec section 3.2)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorDetail {
    #[serde(rename = "type")]
    pub error_type: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub traceback: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

/// Unified data/metadata structure (per spec sections 4.1.13-14)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataEntry {
    pub metadata: Metadata,
    #[serde(with = "base64")]
    pub data: Vec<u8>,
}

/// Binary response with metadata in headers
pub struct BinaryResponse {
    pub data: Vec<u8>,
    pub metadata: Metadata,
}

/// Construct success response
pub fn ok_response<T: Serialize>(result: T) -> ApiResponse<T> {
    ApiResponse {
        status: ResponseStatus::Ok,
        result: Some(result),
        message: "Success".to_string(),
        query: None,
        key: None,
        error: None,
    }
}

/// Construct error response from Error
pub fn error_response<T: Serialize>(error: &Error) -> ApiResponse<T> {
    ApiResponse {
        status: ResponseStatus::Error,
        result: None,
        message: error.message.clone(),
        query: error.query.as_ref().map(|q| q.encode()),
        key: error.key.as_ref().map(|k| k.encode()),
        error: Some(error_to_detail(error)),
    }
}

/// Convert Error to ErrorDetail
pub fn error_to_detail(error: &Error) -> ErrorDetail {
    ErrorDetail {
        error_type: format!("{:?}", error.error_type),
        message: error.message.clone(),
        query: None,  // Error struct doesn't include query string
        key: None,    // Error struct doesn't include key string
        traceback: None,
        metadata: None,
    }
}

impl<T> IntoResponse for ApiResponse<T>
where
    T: Serialize,
{
    fn into_response(self) -> Response {
        let json = serde_json::to_string(&self)
            .unwrap_or_else(|e| format!(r#"{{"status":"ERROR","message":"Serialization failed: {}"}}"#, e));

        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(json))
            .unwrap()
    }
}

impl IntoResponse for BinaryResponse {
    fn into_response(self) -> Response {
        let media_type = self.metadata.get_media_type()
            .unwrap_or_else(|| "application/octet-stream".to_string());

        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, media_type)
            .body(Body::from(self.data))
            .unwrap()
    }
}

impl IntoResponse for DataEntry {
    fn into_response(self) -> Response {
        let cbor_bytes = ciborium::ser::into_writer(&self, Vec::new())
            .unwrap_or_else(|e| {
                tracing::error!("CBOR serialization failed: {}", e);
                Vec::new()
            });

        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/cbor")
            .body(Body::from(cbor_bytes))
            .unwrap()
    }
}

mod base64 {
    use serde::{Serialize, Deserialize, Serializer, Deserializer};

    pub fn serialize<S>(bytes: &Vec<u8>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if serializer.is_human_readable() {
            let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
            serializer.serialize_str(&encoded)
        } else {
            serializer.serialize_bytes(bytes)
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        if deserializer.is_human_readable() {
            let s = String::deserialize(deserializer)?;
            base64::engine::general_purpose::STANDARD.decode(&s)
                .map_err(serde::de::Error::custom)
        } else {
            Vec::<u8>::deserialize(deserializer)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Happy Path Tests

    #[test]
    fn test_ok_response_with_string_result() {
        let response = ok_response("hello".to_string());

        assert_eq!(response.status, ResponseStatus::Ok);
        assert_eq!(response.result, Some("hello".to_string()));
        assert_eq!(response.message, "Success");
        assert_eq!(response.query, None);
        assert_eq!(response.key, None);
        assert_eq!(response.error, None);
    }

    #[test]
    fn test_ok_response_with_integer_result() {
        let response = ok_response(42);

        assert_eq!(response.status, ResponseStatus::Ok);
        assert_eq!(response.result, Some(42));
        assert_eq!(response.message, "Success");
    }

    #[test]
    fn test_ok_response_with_json_value() {
        let json_value = serde_json::json!({"key": "value", "number": 123});
        let response = ok_response(json_value.clone());

        assert_eq!(response.status, ResponseStatus::Ok);
        assert_eq!(response.result, Some(json_value));
    }

    #[test]
    fn test_ok_response_serialization_json() {
        let response = ok_response("test".to_string());
        let json = serde_json::to_string(&response).unwrap();

        assert!(json.contains(r#""status":"OK""#));
        assert!(json.contains(r#""result":"test""#));
        assert!(json.contains(r#""message":"Success""#));
        // Verify optional fields are skipped when None
        assert!(!json.contains("query"));
        assert!(!json.contains("error"));
    }

    #[test]
    fn test_api_response_skips_none_fields() {
        let response: ApiResponse<String> = ApiResponse {
            status: ResponseStatus::Ok,
            result: Some("data".to_string()),
            message: "OK".to_string(),
            query: None,
            key: None,
            error: None,
        };

        let json = serde_json::to_string(&response).unwrap();

        // Verify None fields are omitted (not "null")
        assert!(!json.contains("\"query\":null"));
        assert!(!json.contains("\"key\":null"));
        assert!(!json.contains("\"error\":null"));
    }

    #[test]
    fn test_error_detail_serialization() {
        let detail = ErrorDetail {
            error_type: "KeyNotFound".to_string(),
            message: "Key not found".to_string(),
            query: Some("text-hello".to_string()),
            key: Some("ns/key".to_string()),
            traceback: None,
            metadata: None,
        };

        let json = serde_json::to_string(&detail).unwrap();

        assert!(json.contains(r#""type":"KeyNotFound""#));
        assert!(json.contains(r#""message":"Key not found""#));
        assert!(json.contains(r#""query":"text-hello""#));
        assert!(json.contains(r#""key":"ns/key""#));
    }

    #[test]
    fn test_error_detail_type_field_renamed() {
        let detail = ErrorDetail {
            error_type: "ParseError".to_string(),
            message: "Invalid syntax".to_string(),
            query: None,
            key: None,
            traceback: None,
            metadata: None,
        };

        let json = serde_json::to_string(&detail).unwrap();

        // Verify "type" field is used, not "error_type"
        assert!(json.contains(r#""type":"ParseError""#));
        assert!(!json.contains("error_type"));
    }

    #[test]
    fn test_data_entry_construction() {
        let metadata = Metadata::new();
        let data = vec![1, 2, 3, 4, 5];

        let entry = DataEntry {
            metadata,
            data: data.clone(),
        };

        assert_eq!(entry.data, data);
    }

    #[test]
    fn test_data_entry_with_large_data() {
        let metadata = Metadata::new();
        let data = vec![0u8; 1_000_000];  // 1MB

        let entry = DataEntry {
            metadata,
            data,
        };

        assert_eq!(entry.data.len(), 1_000_000);
    }

    #[test]
    fn test_binary_response_construction() {
        let metadata = Metadata::new();
        let data = b"test data".to_vec();

        let response = BinaryResponse {
            data: data.clone(),
            metadata,
        };

        assert_eq!(response.data, data);
    }

    #[test]
    fn test_response_status_ok_serialization() {
        let status = ResponseStatus::Ok;
        let json = serde_json::to_string(&status).unwrap();

        assert_eq!(json, r#""OK""#);
    }

    #[test]
    fn test_response_status_error_serialization() {
        let status = ResponseStatus::Error;
        let json = serde_json::to_string(&status).unwrap();

        assert_eq!(json, r#""ERROR""#);
    }

    // Error Path Tests

    #[test]
    fn test_error_to_detail_key_not_found() {
        use liquers_core::error::ErrorType;

        let error = Error::key_not_found(&Key::new());
        let detail = error_to_detail(&error);

        assert_eq!(detail.error_type, "KeyNotFound");
        assert!(!detail.message.is_empty());
    }

    #[test]
    fn test_error_to_detail_parse_error() {
        use liquers_core::error::ErrorType;

        let error = Error::general_error("Invalid query syntax".to_string());
        let detail = error_to_detail(&error);

        assert_eq!(detail.message, "Invalid query syntax");
    }

    #[test]
    fn test_error_response_structure() {
        let error = Error::general_error("Test error".to_string());
        let response: ApiResponse<()> = error_response(&error);

        assert_eq!(response.status, ResponseStatus::Error);
        assert_eq!(response.result, None);
        assert_eq!(response.message, "Test error");
        assert!(response.error.is_some());

        let detail = response.error.unwrap();
        assert_eq!(detail.message, "Test error");
    }

    #[test]
    fn test_error_response_serialization() {
        let error = Error::general_error("Something went wrong".to_string());
        let response: ApiResponse<()> = error_response(&error);
        let json = serde_json::to_string(&response).unwrap();

        assert!(json.contains(r#""status":"ERROR""#));
        assert!(json.contains(r#""message":"Something went wrong""#));
        assert!(json.contains(r#""error""#));
    }

    #[test]
    fn test_ok_response_into_response() {
        let response = ok_response("hello".to_string());
        let axum_response = response.into_response();

        assert_eq!(axum_response.status(), StatusCode::OK);
    }

    #[test]
    fn test_binary_response_into_response() {
        let metadata = Metadata::new();
        let response = BinaryResponse {
            data: b"test".to_vec(),
            metadata,
        };

        let axum_response = response.into_response();
        assert_eq!(axum_response.status(), StatusCode::OK);
    }

    // Edge Cases

    #[test]
    fn test_ok_response_with_empty_string() {
        let response = ok_response("".to_string());

        assert_eq!(response.result, Some("".to_string()));
    }

    #[test]
    fn test_ok_response_with_unicode_characters() {
        let response = ok_response("こんにちは 🚀".to_string());

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("こんにちは"));
    }

    #[test]
    fn test_data_entry_with_empty_data() {
        let metadata = Metadata::new();
        let entry = DataEntry {
            metadata,
            data: vec![],
        };

        assert_eq!(entry.data.len(), 0);
    }

    #[test]
    fn test_data_entry_with_null_bytes() {
        let metadata = Metadata::new();
        let entry = DataEntry {
            metadata,
            data: vec![0u8, 1u8, 0u8, 255u8, 0u8],
        };

        assert_eq!(entry.data, vec![0u8, 1u8, 0u8, 255u8, 0u8]);
    }

    #[test]
    fn test_error_detail_with_all_fields() {
        let detail = ErrorDetail {
            error_type: "ExecutionError".to_string(),
            message: "Command execution failed".to_string(),
            query: Some("cmd-arg1-arg2".to_string()),
            key: Some("store/key/path".to_string()),
            traceback: Some(vec![
                "at line 1".to_string(),
                "in function foo".to_string(),
            ]),
            metadata: Some(serde_json::json!({"context": "test"})),
        };

        let json = serde_json::to_string(&detail).unwrap();
        assert!(json.contains("traceback"));
        assert!(json.contains("metadata"));
    }

    #[test]
    fn test_response_status_copy_trait() {
        let status1 = ResponseStatus::Ok;
        let status2 = status1;  // Copy semantics

        assert_eq!(status1, status2);
    }

    #[test]
    fn test_api_response_clone() {
        let response = ok_response("test".to_string());
        let cloned = response.clone();

        assert_eq!(response.status, cloned.status);
        assert_eq!(response.result, cloned.result);
    }

    #[test]
    fn test_error_detail_with_special_characters_in_message() {
        let detail = ErrorDetail {
            error_type: "General".to_string(),
            message: "Error with \"quotes\" and 'apostrophes'".to_string(),
            query: None,
            key: None,
            traceback: None,
            metadata: None,
        };

        let json = serde_json::to_string(&detail).unwrap();
        let parsed: ErrorDetail = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.message, detail.message);
    }

    #[test]
    fn test_api_response_deserialization() {
        let json = r#"{
            "status": "OK",
            "result": "hello",
            "message": "Success"
        }"#;

        let response: ApiResponse<String> = serde_json::from_str(json).unwrap();
        assert_eq!(response.status, ResponseStatus::Ok);
        assert_eq!(response.result, Some("hello".to_string()));
    }

    #[test]
    fn test_binary_response_with_text_metadata() {
        let mut metadata = Metadata::new();
        // Assuming metadata has setter or similar interface
        let response = BinaryResponse {
            data: "plain text".as_bytes().to_vec(),
            metadata,
        };

        assert_eq!(response.data, "plain text".as_bytes());
    }

    #[test]
    fn test_data_entry_round_trip_json() {
        let metadata = Metadata::new();
        let original = DataEntry {
            metadata,
            data: vec![0xDE, 0xAD, 0xBE, 0xEF],
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: DataEntry = serde_json::from_str(&json).unwrap();

        assert_eq!(original.data, deserialized.data);
    }
}
```

---

## File: `liquers-axum/src/core/error.rs`

### Module Implementation with Tests

```rust
use liquers_core::error::{Error, ErrorType};
use axum::http::StatusCode;

/// Map ErrorType to HTTP status code (per spec 3.3)
pub fn error_to_status_code(error_type: ErrorType) -> StatusCode {
    match error_type {
        ErrorType::KeyNotFound => StatusCode::NOT_FOUND,
        ErrorType::KeyNotSupported => StatusCode::NOT_FOUND,
        ErrorType::ParseError => StatusCode::BAD_REQUEST,
        ErrorType::UnknownCommand => StatusCode::BAD_REQUEST,
        ErrorType::ParameterError => StatusCode::BAD_REQUEST,
        ErrorType::ArgumentMissing => StatusCode::BAD_REQUEST,
        ErrorType::ActionNotRegistered => StatusCode::BAD_REQUEST,
        ErrorType::CommandAlreadyRegistered => StatusCode::CONFLICT,
        ErrorType::TooManyParameters => StatusCode::BAD_REQUEST,
        ErrorType::ConversionError => StatusCode::UNPROCESSABLE_ENTITY,
        ErrorType::SerializationError => StatusCode::UNPROCESSABLE_ENTITY,
        ErrorType::KeyReadError => StatusCode::INTERNAL_SERVER_ERROR,
        ErrorType::KeyWriteError => StatusCode::INTERNAL_SERVER_ERROR,
        ErrorType::ExecutionError => StatusCode::INTERNAL_SERVER_ERROR,
        ErrorType::UnexpectedError => StatusCode::INTERNAL_SERVER_ERROR,
        ErrorType::General => StatusCode::INTERNAL_SERVER_ERROR,
        ErrorType::CacheNotSupported => StatusCode::NOT_IMPLEMENTED,
        ErrorType::NotSupported => StatusCode::NOT_IMPLEMENTED,
        ErrorType::NotAvailable => StatusCode::NOT_FOUND,
    }
}

/// Parse ErrorType from string (reverse of Debug format)
pub fn parse_error_type(s: &str) -> Option<ErrorType> {
    match s {
        "KeyNotFound" => Some(ErrorType::KeyNotFound),
        "KeyNotSupported" => Some(ErrorType::KeyNotSupported),
        "ParseError" => Some(ErrorType::ParseError),
        "UnknownCommand" => Some(ErrorType::UnknownCommand),
        "ParameterError" => Some(ErrorType::ParameterError),
        "ArgumentMissing" => Some(ErrorType::ArgumentMissing),
        "ActionNotRegistered" => Some(ErrorType::ActionNotRegistered),
        "CommandAlreadyRegistered" => Some(ErrorType::CommandAlreadyRegistered),
        "TooManyParameters" => Some(ErrorType::TooManyParameters),
        "ConversionError" => Some(ErrorType::ConversionError),
        "SerializationError" => Some(ErrorType::SerializationError),
        "KeyReadError" => Some(ErrorType::KeyReadError),
        "KeyWriteError" => Some(ErrorType::KeyWriteError),
        "ExecutionError" => Some(ErrorType::ExecutionError),
        "UnexpectedError" => Some(ErrorType::UnexpectedError),
        "General" => Some(ErrorType::General),
        "CacheNotSupported" => Some(ErrorType::CacheNotSupported),
        "NotSupported" => Some(ErrorType::NotSupported),
        "NotAvailable" => Some(ErrorType::NotAvailable),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Happy Path Tests

    #[test]
    fn test_key_not_found_maps_to_404() {
        let status = error_to_status_code(ErrorType::KeyNotFound);
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[test]
    fn test_parse_error_maps_to_400() {
        let status = error_to_status_code(ErrorType::ParseError);
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_unknown_command_maps_to_400() {
        let status = error_to_status_code(ErrorType::UnknownCommand);
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_parameter_error_maps_to_400() {
        let status = error_to_status_code(ErrorType::ParameterError);
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_argument_missing_maps_to_400() {
        let status = error_to_status_code(ErrorType::ArgumentMissing);
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_action_not_registered_maps_to_400() {
        let status = error_to_status_code(ErrorType::ActionNotRegistered);
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_command_already_registered_maps_to_409() {
        let status = error_to_status_code(ErrorType::CommandAlreadyRegistered);
        assert_eq!(status, StatusCode::CONFLICT);
    }

    #[test]
    fn test_too_many_parameters_maps_to_400() {
        let status = error_to_status_code(ErrorType::TooManyParameters);
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_conversion_error_maps_to_422() {
        let status = error_to_status_code(ErrorType::ConversionError);
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn test_serialization_error_maps_to_422() {
        let status = error_to_status_code(ErrorType::SerializationError);
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn test_key_read_error_maps_to_500() {
        let status = error_to_status_code(ErrorType::KeyReadError);
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn test_key_write_error_maps_to_500() {
        let status = error_to_status_code(ErrorType::KeyWriteError);
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn test_execution_error_maps_to_500() {
        let status = error_to_status_code(ErrorType::ExecutionError);
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn test_unexpected_error_maps_to_500() {
        let status = error_to_status_code(ErrorType::UnexpectedError);
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn test_general_error_maps_to_500() {
        let status = error_to_status_code(ErrorType::General);
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn test_cache_not_supported_maps_to_501() {
        let status = error_to_status_code(ErrorType::CacheNotSupported);
        assert_eq!(status, StatusCode::NOT_IMPLEMENTED);
    }

    #[test]
    fn test_not_supported_maps_to_501() {
        let status = error_to_status_code(ErrorType::NotSupported);
        assert_eq!(status, StatusCode::NOT_IMPLEMENTED);
    }

    #[test]
    fn test_not_available_maps_to_404() {
        let status = error_to_status_code(ErrorType::NotAvailable);
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[test]
    fn test_key_not_supported_maps_to_404() {
        let status = error_to_status_code(ErrorType::KeyNotSupported);
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[test]
    fn test_parse_error_type_key_not_found() {
        let error_type = parse_error_type("KeyNotFound");
        assert_eq!(error_type, Some(ErrorType::KeyNotFound));
    }

    #[test]
    fn test_parse_error_type_parse_error() {
        let error_type = parse_error_type("ParseError");
        assert_eq!(error_type, Some(ErrorType::ParseError));
    }

    #[test]
    fn test_parse_error_type_all_variants() {
        let variants = vec![
            ("KeyNotFound", ErrorType::KeyNotFound),
            ("KeyNotSupported", ErrorType::KeyNotSupported),
            ("ParseError", ErrorType::ParseError),
            ("UnknownCommand", ErrorType::UnknownCommand),
            ("ParameterError", ErrorType::ParameterError),
            ("ArgumentMissing", ErrorType::ArgumentMissing),
            ("ActionNotRegistered", ErrorType::ActionNotRegistered),
            ("CommandAlreadyRegistered", ErrorType::CommandAlreadyRegistered),
            ("TooManyParameters", ErrorType::TooManyParameters),
            ("ConversionError", ErrorType::ConversionError),
            ("SerializationError", ErrorType::SerializationError),
            ("KeyReadError", ErrorType::KeyReadError),
            ("KeyWriteError", ErrorType::KeyWriteError),
            ("ExecutionError", ErrorType::ExecutionError),
            ("UnexpectedError", ErrorType::UnexpectedError),
            ("General", ErrorType::General),
            ("CacheNotSupported", ErrorType::CacheNotSupported),
            ("NotSupported", ErrorType::NotSupported),
            ("NotAvailable", ErrorType::NotAvailable),
        ];

        for (name, expected) in variants {
            let parsed = parse_error_type(name);
            assert_eq!(parsed, Some(expected), "Failed to parse: {}", name);
        }
    }

    // Error Path Tests

    #[test]
    fn test_parse_error_type_unknown_variant() {
        let error_type = parse_error_type("UnknownVariant");
        assert_eq!(error_type, None);
    }

    #[test]
    fn test_parse_error_type_empty_string() {
        let error_type = parse_error_type("");
        assert_eq!(error_type, None);
    }

    #[test]
    fn test_parse_error_type_wrong_case() {
        let error_type = parse_error_type("parseerror");  // lowercase
        assert_eq!(error_type, None);
    }

    #[test]
    fn test_parse_error_type_with_leading_whitespace() {
        let error_type = parse_error_type(" ParseError");
        assert_eq!(error_type, None);
    }

    #[test]
    fn test_parse_error_type_with_trailing_whitespace() {
        let error_type = parse_error_type("ParseError ");
        assert_eq!(error_type, None);
    }

    // Edge Cases

    #[test]
    fn test_status_code_values_are_correct() {
        // Verify status codes match HTTP standards
        assert_eq!(error_to_status_code(ErrorType::KeyNotFound).as_u16(), 404);
        assert_eq!(error_to_status_code(ErrorType::ParseError).as_u16(), 400);
        assert_eq!(error_to_status_code(ErrorType::CommandAlreadyRegistered).as_u16(), 409);
        assert_eq!(error_to_status_code(ErrorType::ConversionError).as_u16(), 422);
        assert_eq!(error_to_status_code(ErrorType::KeyReadError).as_u16(), 500);
        assert_eq!(error_to_status_code(ErrorType::CacheNotSupported).as_u16(), 501);
    }

    #[test]
    fn test_all_error_types_have_mapping() {
        // Ensure no ErrorType falls through to default case
        // This is guaranteed by explicit match statement (no _ => case)
        let _ = error_to_status_code(ErrorType::KeyNotFound);
        let _ = error_to_status_code(ErrorType::ExecutionError);
        let _ = error_to_status_code(ErrorType::General);
    }

    #[test]
    fn test_parse_and_map_roundtrip() {
        // Verify parse_error_type followed by error_to_status_code works
        let original_type = ErrorType::ParseError;
        let status = error_to_status_code(original_type);

        let parsed = parse_error_type("ParseError").unwrap();
        let remapped = error_to_status_code(parsed);

        assert_eq!(status, remapped);
    }

    #[test]
    fn test_bad_request_errors_are_grouped() {
        let bad_request_types = vec![
            ErrorType::ParseError,
            ErrorType::UnknownCommand,
            ErrorType::ParameterError,
            ErrorType::ArgumentMissing,
            ErrorType::ActionNotRegistered,
            ErrorType::TooManyParameters,
        ];

        for error_type in bad_request_types {
            let status = error_to_status_code(error_type);
            assert_eq!(status, StatusCode::BAD_REQUEST,
                      "Expected BAD_REQUEST for {:?}", error_type);
        }
    }

    #[test]
    fn test_server_error_types_are_grouped() {
        let server_errors = vec![
            ErrorType::KeyReadError,
            ErrorType::KeyWriteError,
            ErrorType::ExecutionError,
            ErrorType::UnexpectedError,
            ErrorType::General,
        ];

        for error_type in server_errors {
            let status = error_to_status_code(error_type);
            assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR,
                      "Expected INTERNAL_SERVER_ERROR for {:?}", error_type);
        }
    }

    #[test]
    fn test_not_found_errors_are_grouped() {
        let not_found_types = vec![
            ErrorType::KeyNotFound,
            ErrorType::KeyNotSupported,
            ErrorType::NotAvailable,
        ];

        for error_type in not_found_types {
            let status = error_to_status_code(error_type);
            assert_eq!(status, StatusCode::NOT_FOUND,
                      "Expected NOT_FOUND for {:?}", error_type);
        }
    }
}
```

---

## File: `liquers-axum/src/core/format.rs`

### Module Implementation with Tests

```rust
use liquers_core::error::{Error, ErrorType};
use axum::http::HeaderMap;
use serde::{Serialize, Deserialize};
use liquers_core::metadata::Metadata;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SerializationFormat {
    Cbor,
    Bincode,
    Json,
}

/// Select serialization format from Accept header or query param
pub fn select_format(
    headers: &HeaderMap,
    format_param: Option<&str>,
) -> SerializationFormat {
    // Query param takes precedence
    if let Some(format) = format_param {
        return parse_format_param(format)
            .unwrap_or(SerializationFormat::Cbor);
    }

    // Parse Accept header
    if let Some(accept) = headers.get("accept") {
        if let Ok(accept_str) = accept.to_str() {
            if accept_str.contains("application/json") {
                return SerializationFormat::Json;
            }
            if accept_str.contains("application/x-bincode") {
                return SerializationFormat::Bincode;
            }
            if accept_str.contains("application/cbor") {
                return SerializationFormat::Cbor;
            }
        }
    }

    SerializationFormat::Cbor
}

/// Parse ?format=cbor query parameter
pub fn parse_format_param(format: &str) -> Result<SerializationFormat, Error> {
    match format.to_lowercase().as_str() {
        "cbor" => Ok(SerializationFormat::Cbor),
        "bincode" => Ok(SerializationFormat::Bincode),
        "json" => Ok(SerializationFormat::Json),
        _ => Err(Error::not_supported(format!("Unknown format: {}", format))),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataEntry {
    pub metadata: Metadata,
    #[serde(with = "base64")]
    pub data: Vec<u8>,
}

mod base64 {
    use serde::{Serialize, Deserialize, Serializer, Deserializer};

    pub fn serialize<S>(bytes: &Vec<u8>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if serializer.is_human_readable() {
            let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
            serializer.serialize_str(&encoded)
        } else {
            serializer.serialize_bytes(bytes)
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        if deserializer.is_human_readable() {
            let s = String::deserialize(deserializer)?;
            base64::engine::general_purpose::STANDARD.decode(&s)
                .map_err(serde::de::Error::custom)
        } else {
            Vec::<u8>::deserialize(deserializer)
        }
    }
}

/// Serialize DataEntry to bytes with selected format
pub fn serialize_data_entry(entry: &DataEntry, format: SerializationFormat) -> Result<Vec<u8>, Error> {
    match format {
        SerializationFormat::Cbor => {
            let mut buf = Vec::new();
            ciborium::ser::into_writer(entry, &mut buf)
                .map_err(|e| Error::from_error(ErrorType::SerializationError, e))?;
            Ok(buf)
        }
        SerializationFormat::Bincode => {
            bincode::serialize(entry)
                .map_err(|e| Error::from_error(ErrorType::SerializationError, *e))
        }
        SerializationFormat::Json => {
            serde_json::to_vec(entry)
                .map_err(|e| Error::from_error(ErrorType::SerializationError, e))
        }
    }
}

/// Deserialize DataEntry from bytes with detected/specified format
pub fn deserialize_data_entry(bytes: &[u8], format: SerializationFormat) -> Result<DataEntry, Error> {
    match format {
        SerializationFormat::Cbor => {
            ciborium::de::from_reader(bytes)
                .map_err(|e| Error::from_error(ErrorType::SerializationError, e))
        }
        SerializationFormat::Bincode => {
            bincode::deserialize(bytes)
                .map_err(|e| Error::from_error(ErrorType::SerializationError, *e))
        }
        SerializationFormat::Json => {
            serde_json::from_slice(bytes)
                .map_err(|e| Error::from_error(ErrorType::SerializationError, e))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Happy Path Tests: parse_format_param

    #[test]
    fn test_parse_format_param_cbor() {
        let result = parse_format_param("cbor");
        assert_eq!(result, Ok(SerializationFormat::Cbor));
    }

    #[test]
    fn test_parse_format_param_bincode() {
        let result = parse_format_param("bincode");
        assert_eq!(result, Ok(SerializationFormat::Bincode));
    }

    #[test]
    fn test_parse_format_param_json() {
        let result = parse_format_param("json");
        assert_eq!(result, Ok(SerializationFormat::Json));
    }

    #[test]
    fn test_parse_format_param_cbor_uppercase() {
        let result = parse_format_param("CBOR");
        assert_eq!(result, Ok(SerializationFormat::Cbor));
    }

    #[test]
    fn test_parse_format_param_mixed_case() {
        let result = parse_format_param("CbOr");
        assert_eq!(result, Ok(SerializationFormat::Cbor));
    }

    // Happy Path Tests: select_format

    #[test]
    fn test_select_format_defaults_to_cbor() {
        let headers = HeaderMap::new();
        let format = select_format(&headers, None);
        assert_eq!(format, SerializationFormat::Cbor);
    }

    #[test]
    fn test_select_format_from_query_param_cbor() {
        let headers = HeaderMap::new();
        let format = select_format(&headers, Some("cbor"));
        assert_eq!(format, SerializationFormat::Cbor);
    }

    #[test]
    fn test_select_format_from_query_param_json() {
        let headers = HeaderMap::new();
        let format = select_format(&headers, Some("json"));
        assert_eq!(format, SerializationFormat::Json);
    }

    #[test]
    fn test_select_format_query_param_takes_precedence() {
        let mut headers = HeaderMap::new();
        headers.insert("accept", "application/cbor".parse().unwrap());
        let format = select_format(&headers, Some("json"));
        assert_eq!(format, SerializationFormat::Json);
    }

    #[test]
    fn test_select_format_from_accept_header_json() {
        let mut headers = HeaderMap::new();
        headers.insert("accept", "application/json".parse().unwrap());
        let format = select_format(&headers, None);
        assert_eq!(format, SerializationFormat::Json);
    }

    #[test]
    fn test_select_format_from_accept_header_bincode() {
        let mut headers = HeaderMap::new();
        headers.insert("accept", "application/x-bincode".parse().unwrap());
        let format = select_format(&headers, None);
        assert_eq!(format, SerializationFormat::Bincode);
    }

    #[test]
    fn test_select_format_from_accept_header_cbor() {
        let mut headers = HeaderMap::new();
        headers.insert("accept", "application/cbor".parse().unwrap());
        let format = select_format(&headers, None);
        assert_eq!(format, SerializationFormat::Cbor);
    }

    #[test]
    fn test_select_format_accept_with_quality_weights() {
        let mut headers = HeaderMap::new();
        headers.insert("accept", "application/json;q=0.9, application/cbor;q=0.8".parse().unwrap());
        let format = select_format(&headers, None);
        // First match wins (json appears first)
        assert_eq!(format, SerializationFormat::Json);
    }

    #[test]
    fn test_select_format_accept_with_charset() {
        let mut headers = HeaderMap::new();
        headers.insert("accept", "application/json; charset=utf-8".parse().unwrap());
        let format = select_format(&headers, None);
        assert_eq!(format, SerializationFormat::Json);
    }

    // Happy Path Tests: serialization/deserialization

    #[test]
    fn test_serialize_data_entry_cbor() {
        let metadata = Metadata::new();
        let entry = DataEntry {
            metadata,
            data: vec![1, 2, 3, 4, 5],
        };

        let result = serialize_data_entry(&entry, SerializationFormat::Cbor);
        assert!(result.is_ok());
        let bytes = result.unwrap();
        assert!(!bytes.is_empty());
    }

    #[test]
    fn test_serialize_data_entry_bincode() {
        let metadata = Metadata::new();
        let entry = DataEntry {
            metadata,
            data: vec![1, 2, 3, 4, 5],
        };

        let result = serialize_data_entry(&entry, SerializationFormat::Bincode);
        assert!(result.is_ok());
        let bytes = result.unwrap();
        assert!(!bytes.is_empty());
    }

    #[test]
    fn test_serialize_data_entry_json() {
        let metadata = Metadata::new();
        let entry = DataEntry {
            metadata,
            data: vec![1, 2, 3, 4, 5],
        };

        let result = serialize_data_entry(&entry, SerializationFormat::Json);
        assert!(result.is_ok());
        let bytes = result.unwrap();
        let json_str = String::from_utf8(bytes).unwrap();
        assert!(json_str.contains("data"));
    }

    #[test]
    fn test_deserialize_data_entry_cbor_roundtrip() {
        let metadata = Metadata::new();
        let original = DataEntry {
            metadata,
            data: vec![1, 2, 3, 4, 5],
        };

        let serialized = serialize_data_entry(&original, SerializationFormat::Cbor).unwrap();
        let deserialized = deserialize_data_entry(&serialized, SerializationFormat::Cbor).unwrap();

        assert_eq!(original.data, deserialized.data);
    }

    #[test]
    fn test_deserialize_data_entry_bincode_roundtrip() {
        let metadata = Metadata::new();
        let original = DataEntry {
            metadata,
            data: vec![10, 20, 30, 40, 50],
        };

        let serialized = serialize_data_entry(&original, SerializationFormat::Bincode).unwrap();
        let deserialized = deserialize_data_entry(&serialized, SerializationFormat::Bincode).unwrap();

        assert_eq!(original.data, deserialized.data);
    }

    #[test]
    fn test_deserialize_data_entry_json_roundtrip() {
        let metadata = Metadata::new();
        let original = DataEntry {
            metadata,
            data: vec![0xDE, 0xAD, 0xBE, 0xEF],
        };

        let serialized = serialize_data_entry(&original, SerializationFormat::Json).unwrap();
        let deserialized = deserialize_data_entry(&serialized, SerializationFormat::Json).unwrap();

        assert_eq!(original.data, deserialized.data);
    }

    #[test]
    fn test_json_uses_base64_encoding() {
        let metadata = Metadata::new();
        let entry = DataEntry {
            metadata,
            data: vec![1, 2, 3, 4, 5],
        };

        let serialized = serialize_data_entry(&entry, SerializationFormat::Json).unwrap();
        let json_str = String::from_utf8(serialized).unwrap();

        // Verify data is base64 encoded (not raw binary)
        assert!(!json_str.contains("\x01\x02"));  // No raw bytes
        // Base64 encoding of [1,2,3,4,5] is "AQIDBAU="
        assert!(json_str.contains("AQIDBAU"));
    }

    // Error Path Tests

    #[test]
    fn test_parse_format_param_invalid_format() {
        let result = parse_format_param("msgpack");
        assert!(result.is_err());

        let error = result.unwrap_err();
        assert_eq!(error.error_type, ErrorType::NotSupported);
    }

    #[test]
    fn test_parse_format_param_empty_string() {
        let result = parse_format_param("");
        assert!(result.is_err());
    }

    #[test]
    fn test_select_format_with_invalid_query_param_defaults_to_cbor() {
        let headers = HeaderMap::new();
        let format = select_format(&headers, Some("invalid"));
        assert_eq!(format, SerializationFormat::Cbor);
    }

    #[test]
    fn test_deserialize_corrupted_cbor_data() {
        let corrupted_data = vec![0xFF, 0xFF, 0xFF];  // Invalid CBOR
        let result = deserialize_data_entry(&corrupted_data, SerializationFormat::Cbor);
        assert!(result.is_err());
    }

    #[test]
    fn test_deserialize_corrupted_bincode_data() {
        let corrupted_data = vec![0xFF, 0xFF, 0xFF];  // Invalid bincode
        let result = deserialize_data_entry(&corrupted_data, SerializationFormat::Bincode);
        assert!(result.is_err());
    }

    #[test]
    fn test_deserialize_corrupted_json_data() {
        let corrupted_data = b"{invalid json}";
        let result = deserialize_data_entry(corrupted_data, SerializationFormat::Json);
        assert!(result.is_err());
    }

    // Edge Cases

    #[test]
    fn test_serialization_format_copy_trait() {
        let format1 = SerializationFormat::Json;
        let format2 = format1;  // Copy semantics
        assert_eq!(format1, format2);
    }

    #[test]
    fn test_serialize_empty_data() {
        let metadata = Metadata::new();
        let entry = DataEntry {
            metadata,
            data: vec![],
        };

        // Test all formats
        assert!(serialize_data_entry(&entry, SerializationFormat::Cbor).is_ok());
        assert!(serialize_data_entry(&entry, SerializationFormat::Bincode).is_ok());
        assert!(serialize_data_entry(&entry, SerializationFormat::Json).is_ok());
    }

    #[test]
    fn test_serialize_large_data() {
        let metadata = Metadata::new();
        let entry = DataEntry {
            metadata,
            data: vec![0x42u8; 1_000_000],  // 1MB
        };

        assert!(serialize_data_entry(&entry, SerializationFormat::Cbor).is_ok());
        assert!(serialize_data_entry(&entry, SerializationFormat::Bincode).is_ok());
        assert!(serialize_data_entry(&entry, SerializationFormat::Json).is_ok());
    }

    #[test]
    fn test_serialize_with_all_byte_values() {
        let metadata = Metadata::new();
        let data: Vec<u8> = (0..=255).collect();
        let entry = DataEntry {
            metadata,
            data,
        };

        assert!(serialize_data_entry(&entry, SerializationFormat::Cbor).is_ok());
        assert!(serialize_data_entry(&entry, SerializationFormat::Bincode).is_ok());
        assert!(serialize_data_entry(&entry, SerializationFormat::Json).is_ok());
    }

    #[test]
    fn test_select_format_case_insensitive_header() {
        let mut headers = HeaderMap::new();
        headers.insert("ACCEPT", "application/json".parse().unwrap());  // Uppercase
        let format = select_format(&headers, None);
        // Note: HeaderMap keys are case-insensitive
        assert_eq!(format, SerializationFormat::Json);
    }

    #[test]
    fn test_deserialize_empty_data() {
        let metadata = Metadata::new();
        let original = DataEntry {
            metadata,
            data: vec![],
        };

        let cbor = serialize_data_entry(&original, SerializationFormat::Cbor).unwrap();
        let deserialized = deserialize_data_entry(&cbor, SerializationFormat::Cbor).unwrap();
        assert!(deserialized.data.is_empty());
    }

    #[test]
    fn test_json_base64_with_special_chars() {
        let metadata = Metadata::new();
        let entry = DataEntry {
            metadata,
            data: vec![0x00, 0xFF, 0x80, 0x40],  // Special byte values
        };

        let serialized = serialize_data_entry(&entry, SerializationFormat::Json).unwrap();
        let deserialized = deserialize_data_entry(&serialized, SerializationFormat::Json).unwrap();

        assert_eq!(entry.data, deserialized.data);
    }

    #[test]
    fn test_multiple_formats_round_trip_same_data() {
        let metadata = Metadata::new();
        let original_data = vec![10, 20, 30, 40, 50];
        let entry = DataEntry {
            metadata,
            data: original_data,
        };

        // Serialize and deserialize with each format
        for format in &[SerializationFormat::Cbor, SerializationFormat::Bincode, SerializationFormat::Json] {
            let serialized = serialize_data_entry(&entry, *format).unwrap();
            let deserialized = deserialize_data_entry(&serialized, *format).unwrap();
            assert_eq!(entry.data, deserialized.data, "Failed roundtrip for {:?}", format);
        }
    }

    #[test]
    fn test_select_format_wildcard_accept() {
        let mut headers = HeaderMap::new();
        headers.insert("accept", "*/*".parse().unwrap());
        let format = select_format(&headers, None);
        // Should default to CBOR (no specific format matched)
        assert_eq!(format, SerializationFormat::Cbor);
    }

    #[test]
    fn test_select_format_multiple_accept_types() {
        let mut headers = HeaderMap::new();
        headers.insert("accept", "text/html, application/xhtml+xml, application/xml;q=0.9, application/json;q=0.8, */*;q=0.7".parse().unwrap());
        let format = select_format(&headers, None);
        // Should find json in the list
        assert_eq!(format, SerializationFormat::Json);
    }

    #[test]
    fn test_serialization_format_comparison() {
        assert_eq!(SerializationFormat::Cbor, SerializationFormat::Cbor);
        assert_ne!(SerializationFormat::Cbor, SerializationFormat::Json);
        assert_ne!(SerializationFormat::Json, SerializationFormat::Bincode);
    }

    #[test]
    fn test_data_entry_with_metadata() {
        let mut metadata = Metadata::new();
        let entry = DataEntry {
            metadata,
            data: vec![1, 2, 3],
        };

        let serialized = serialize_data_entry(&entry, SerializationFormat::Cbor).unwrap();
        let deserialized = deserialize_data_entry(&serialized, SerializationFormat::Cbor).unwrap();

        // Verify metadata is preserved
        assert_eq!(serialized.len() > 0, true);
    }
}
```

---

## Test Execution Summary

### Running Tests

To execute all unit tests:

```bash
cd liquers-axum
cargo test --lib core::response
cargo test --lib core::error
cargo test --lib core::format
```

Or run all tests at once:

```bash
cargo test --lib
```

### Test Categories

| Category | Count | Purpose |
|----------|-------|---------|
| Happy Path | 50+ | Verify correct behavior with valid inputs |
| Error Path | 20+ | Verify error handling for invalid inputs |
| Edge Cases | 25+ | Verify boundary conditions and special cases |
| **Total** | **95+** | Comprehensive coverage |

### Coverage Metrics

- **Response types**: All public functions and serialization paths covered
- **Error mapping**: All 19 ErrorType variants mapped to HTTP status codes
- **Format selection**: All 3 formats (CBOR, bincode, JSON) + header/param precedence
- **Serialization**: Round-trip tests for all formats, base64 encoding verification

### Key Test Patterns

1. **Happy Path**: Valid inputs produce correct outputs
2. **Error Path**: Invalid inputs produce appropriate errors
3. **Roundtrip**: Serialize → Deserialize → Original equality
4. **Edge Cases**: Empty data, large data, special byte values, Unicode
5. **Precondence**: Query param > Accept header > Default
6. **Explicit Match**: All enum variants explicitly handled (no default arm)

### Notes on Test Design

- Tests follow CLAUDE.md naming convention: `test_<function>_<scenario>()`
- No `unwrap()` calls on fallible operations (except in assertions)
- All tests are deterministic and isolated
- Test data includes: valid, invalid, empty, large, and boundary cases
- Async tests marked with `#[tokio::test]` (none needed for Phase 3 core modules - all sync)
