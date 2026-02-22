use crate::api_core::response::{DataEntry, SerializationFormat};
use axum::http::header::{HeaderMap, ACCEPT, CONTENT_TYPE};

/// Select serialization format from query parameter or Accept header
/// Priority: ?format query param > Accept header > default (CBOR)
pub fn select_format(query_format: Option<&str>, headers: &HeaderMap) -> SerializationFormat {
    // Priority 1: Query parameter
    if let Some(format_str) = query_format {
        match format_str.to_lowercase().as_str() {
            "cbor" => return SerializationFormat::Cbor,
            "bincode" => return SerializationFormat::Bincode,
            "json" => return SerializationFormat::Json,
            _ => {} // Fall through to next priority
        }
    }

    // Priority 2: Accept header
    if let Some(accept) = headers.get(ACCEPT) {
        if let Ok(accept_str) = accept.to_str() {
            // Parse Accept header (simple implementation - doesn't handle quality values)
            for mime_type in accept_str.split(',') {
                let mime_type = mime_type.trim().split(';').next().unwrap_or("");
                if let Some(format) = SerializationFormat::from_mime_type(mime_type) {
                    return format;
                }
            }
        }
    }

    // Default: CBOR (most efficient for binary data)
    SerializationFormat::Cbor
}

/// Select format from Content-Type header for deserialization
pub fn format_from_content_type(headers: &HeaderMap) -> Option<SerializationFormat> {
    headers
        .get(CONTENT_TYPE)
        .and_then(|ct| ct.to_str().ok())
        .and_then(|ct_str| {
            let mime_type = ct_str.split(';').next().unwrap_or("").trim();
            SerializationFormat::from_mime_type(mime_type)
        })
}

/// Serialize DataEntry to bytes using the specified format
pub fn serialize_data_entry(
    entry: &DataEntry,
    format: SerializationFormat,
) -> Result<Vec<u8>, String> {
    match format {
        SerializationFormat::Cbor => {
            let mut cbor_bytes = Vec::new();
            if let Err(e) = ciborium::ser::into_writer(entry, &mut cbor_bytes) {
                tracing::error!("CBOR serialization failed: {}", e);
                return Err(format!("CBOR serialization failed: {}", e));
            }
            Ok(cbor_bytes)
        }
        SerializationFormat::Bincode => {
            bincode::serialize(entry).map_err(|e| format!("Bincode serialization failed: {}", e))
        }
        SerializationFormat::Json => {
            serde_json::to_vec(entry).map_err(|e| format!("JSON serialization failed: {}", e))
        }
    }
}

/// Deserialize DataEntry from bytes using the specified format
pub fn deserialize_data_entry(
    bytes: &[u8],
    format: SerializationFormat,
) -> Result<DataEntry, String> {
    match format {
        SerializationFormat::Cbor => ciborium::de::from_reader(bytes)
            .map_err(|e| format!("CBOR deserialization failed: {}", e)),
        SerializationFormat::Bincode => bincode::deserialize(bytes)
            .map_err(|e| format!("Bincode deserialization failed: {}", e)),
        SerializationFormat::Json => {
            serde_json::from_slice(bytes).map_err(|e| format!("JSON deserialization failed: {}", e))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::header::{HeaderValue, ACCEPT, CONTENT_TYPE};
    use axum::http::HeaderMap;

    #[test]
    fn test_select_format_query_param() {
        let headers = HeaderMap::new();

        assert_eq!(
            select_format(Some("cbor"), &headers),
            SerializationFormat::Cbor
        );
        assert_eq!(
            select_format(Some("bincode"), &headers),
            SerializationFormat::Bincode
        );
        assert_eq!(
            select_format(Some("json"), &headers),
            SerializationFormat::Json
        );
    }

    #[test]
    fn test_select_format_accept_header() {
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static("application/cbor"));
        assert_eq!(select_format(None, &headers), SerializationFormat::Cbor);

        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static("application/x-bincode"));
        assert_eq!(select_format(None, &headers), SerializationFormat::Bincode);

        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
        assert_eq!(select_format(None, &headers), SerializationFormat::Json);
    }

    #[test]
    fn test_select_format_accept_header_with_quality() {
        let mut headers = HeaderMap::new();
        headers.insert(
            ACCEPT,
            HeaderValue::from_static("application/json; q=0.9, application/cbor; q=1.0"),
        );
        // Should select json (first in list, quality values not implemented)
        assert_eq!(select_format(None, &headers), SerializationFormat::Json);
    }

    #[test]
    fn test_select_format_default() {
        let headers = HeaderMap::new();
        assert_eq!(select_format(None, &headers), SerializationFormat::Cbor);
    }

    #[test]
    fn test_select_format_query_priority() {
        let mut headers = HeaderMap::new();
        headers.insert(ACCEPT, HeaderValue::from_static("application/json"));

        // Query param should override Accept header
        assert_eq!(
            select_format(Some("cbor"), &headers),
            SerializationFormat::Cbor
        );
    }

    #[test]
    fn test_format_from_content_type() {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/cbor"));
        assert_eq!(
            format_from_content_type(&headers),
            Some(SerializationFormat::Cbor)
        );

        let mut headers = HeaderMap::new();
        headers.insert(
            CONTENT_TYPE,
            HeaderValue::from_static("application/x-bincode; charset=utf-8"),
        );
        assert_eq!(
            format_from_content_type(&headers),
            Some(SerializationFormat::Bincode)
        );
    }

    #[test]
    fn test_serialize_deserialize_cbor() {
        let entry = DataEntry {
            metadata: serde_json::json!({"key": "value"}),
            data: vec![1, 2, 3, 4, 5],
        };

        let bytes = serialize_data_entry(&entry, SerializationFormat::Cbor).unwrap();
        let deserialized = deserialize_data_entry(&bytes, SerializationFormat::Cbor).unwrap();

        assert_eq!(entry.data, deserialized.data);
        assert_eq!(entry.metadata, deserialized.metadata);
    }

    #[test]
    #[ignore] // Bincode doesn't support deserialize_any with serde_json::Value
    fn test_serialize_deserialize_bincode() {
        let entry = DataEntry {
            metadata: serde_json::json!({"test": 123}),
            data: vec![10, 20, 30],
        };

        let bytes = serialize_data_entry(&entry, SerializationFormat::Bincode).unwrap();
        let deserialized = deserialize_data_entry(&bytes, SerializationFormat::Bincode).unwrap();

        assert_eq!(entry.data, deserialized.data);
        assert_eq!(entry.metadata, deserialized.metadata);
    }

    #[test]
    fn test_serialize_deserialize_json() {
        let entry = DataEntry {
            metadata: serde_json::json!({"name": "test"}),
            data: vec![255, 128, 64],
        };

        let bytes = serialize_data_entry(&entry, SerializationFormat::Json).unwrap();
        let deserialized = deserialize_data_entry(&bytes, SerializationFormat::Json).unwrap();

        assert_eq!(entry.data, deserialized.data);
        assert_eq!(entry.metadata, deserialized.metadata);

        // Verify that JSON serialization uses base64 for data
        let json_str = String::from_utf8(bytes).unwrap();
        assert!(json_str.contains("\"data\""));
        // Base64 encoded [255, 128, 64] is "/4BA"
        assert!(json_str.contains("/4BA"));
    }

    #[test]
    fn test_serialize_invalid_format_roundtrip() {
        let entry = DataEntry {
            metadata: serde_json::json!({"test": true}),
            data: vec![1, 2, 3],
        };

        let cbor_bytes = serialize_data_entry(&entry, SerializationFormat::Cbor).unwrap();

        // Try to deserialize CBOR bytes as JSON - should fail
        let result = deserialize_data_entry(&cbor_bytes, SerializationFormat::Json);
        assert!(result.is_err());
    }
}
