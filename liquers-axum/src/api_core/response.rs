use liquers_core::metadata::Metadata;
use serde::{Deserialize, Serialize};

/// Standard API response wrapper (per spec section 3.4)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse<T: Serialize> {
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

impl<T: Serialize> ApiResponse<T> {
    /// Create a successful response with data
    pub fn ok(result: T, message: impl Into<String>) -> Self {
        Self {
            status: ResponseStatus::Ok,
            result: Some(result),
            message: message.into(),
            query: None,
            key: None,
            error: None,
        }
    }

    /// Create an error response
    pub fn error(error_detail: ErrorDetail, message: impl Into<String>) -> Self {
        Self {
            status: ResponseStatus::Error,
            result: None,
            message: message.into(),
            query: error_detail.query.clone(),
            key: error_detail.key.clone(),
            error: Some(error_detail),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum ResponseStatus {
    Ok,
    Error,
}

/// Error detail structure (per spec section 3.2)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorDetail {
    #[serde(rename = "type")]
    pub error_type: String, // ErrorType as string
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
/// Supports CBOR, bincode, and JSON serialization with automatic base64 encoding for JSON
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataEntry {
    pub metadata: serde_json::Value, // MetadataRecord as JSON
    #[serde(with = "base64_serde")]
    pub data: Vec<u8>,
}

/// Custom base64 serialization module for DataEntry.data field
/// When serializing to JSON, binary data is base64-encoded
/// When serializing to CBOR/bincode, binary data is stored directly
mod base64_serde {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S>(bytes: &Vec<u8>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if serializer.is_human_readable() {
            // JSON format - use base64 encoding
            use base64::prelude::*;
            serializer.serialize_str(&BASE64_STANDARD.encode(bytes))
        } else {
            // CBOR/bincode format - serialize bytes directly
            serializer.serialize_bytes(bytes)
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        if deserializer.is_human_readable() {
            // JSON format - decode base64
            use base64::prelude::*;
            let s = String::deserialize(deserializer)?;
            BASE64_STANDARD
                .decode(s)
                .map_err(|e| serde::de::Error::custom(format!("Invalid base64: {}", e)))
        } else {
            // CBOR/bincode format - deserialize bytes directly
            Vec::<u8>::deserialize(deserializer)
        }
    }
}

/// Binary response with metadata in headers (for /q and /api/store/data endpoints)
#[derive(Debug, Clone)]
pub struct BinaryResponse {
    pub data: Vec<u8>,
    pub metadata: Metadata,
}

/// Serialization format enum
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SerializationFormat {
    Cbor,    // application/cbor
    Bincode, // application/x-bincode
    Json,    // application/json
}

impl SerializationFormat {
    /// Get the MIME type for this format
    pub fn mime_type(&self) -> &'static str {
        match self {
            SerializationFormat::Cbor => "application/cbor",
            SerializationFormat::Bincode => "application/x-bincode",
            SerializationFormat::Json => "application/json",
        }
    }

    /// Parse format from MIME type string
    pub fn from_mime_type(mime: &str) -> Option<Self> {
        match mime.to_lowercase().as_str() {
            "application/cbor" => Some(SerializationFormat::Cbor),
            "application/x-bincode" => Some(SerializationFormat::Bincode),
            "application/json" => Some(SerializationFormat::Json),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_response_ok() {
        let response = ApiResponse::ok("test result", "Success");
        assert!(matches!(response.status, ResponseStatus::Ok));
        assert_eq!(response.result, Some("test result"));
        assert_eq!(response.message, "Success");
        assert!(response.error.is_none());
    }

    #[test]
    fn test_api_response_error() {
        let error_detail = ErrorDetail {
            error_type: "ParseError".to_string(),
            message: "Invalid syntax".to_string(),
            query: Some("test/query".to_string()),
            key: None,
            traceback: None,
            metadata: None,
        };
        let response: ApiResponse<String> = ApiResponse::error(error_detail.clone(), "Failed");
        assert!(matches!(response.status, ResponseStatus::Error));
        assert!(response.result.is_none());
        assert_eq!(response.message, "Failed");
        assert_eq!(response.query, Some("test/query".to_string()));
    }

    #[test]
    fn test_data_entry_json_serialization() {
        let entry = DataEntry {
            metadata: serde_json::json!({"key": "value"}),
            data: vec![1, 2, 3, 4, 5],
        };

        // Serialize to JSON - data should be base64 encoded
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("AQIDBAU=")); // Base64 of [1,2,3,4,5]

        // Deserialize from JSON
        let deserialized: DataEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.data, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_serialization_format_mime_types() {
        assert_eq!(SerializationFormat::Cbor.mime_type(), "application/cbor");
        assert_eq!(
            SerializationFormat::Bincode.mime_type(),
            "application/x-bincode"
        );
        assert_eq!(SerializationFormat::Json.mime_type(), "application/json");
    }

    #[test]
    fn test_serialization_format_parsing() {
        assert_eq!(
            SerializationFormat::from_mime_type("application/cbor"),
            Some(SerializationFormat::Cbor)
        );
        assert_eq!(
            SerializationFormat::from_mime_type("application/x-bincode"),
            Some(SerializationFormat::Bincode)
        );
        assert_eq!(
            SerializationFormat::from_mime_type("application/json"),
            Some(SerializationFormat::Json)
        );
        assert_eq!(SerializationFormat::from_mime_type("text/plain"), None);
    }
}
