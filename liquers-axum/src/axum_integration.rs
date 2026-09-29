use crate::api_core::{
    error_to_status_code, parse_error_type, serialize_data_entry, ApiResponse, BinaryResponse,
    DataEntry, ResponseStatus, SerializationFormat,
};
use axum::{
    body::Body,
    http::{header, response::Builder, HeaderValue, Response, StatusCode},
    response::IntoResponse,
};
use serde::Serialize;

/// Finish a response builder, or answer 500 if the builder rejected a part of it (an invalid
/// header value, typically a media type taken from metadata). Library code never panics on a
/// response it cannot build.
pub(crate) fn build_or_500(builder: Builder, body: Body) -> Response<Body> {
    match builder.body(body) {
        Ok(response) => response,
        Err(e) => {
            tracing::error!("Failed to build response: {}", e);
            let mut response = Response::new(Body::from(
                r#"{"status":"ERROR","message":"Failed to build response"}"#,
            ));
            *response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
            response.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/json"),
            );
            response
        }
    }
}

/// Implement IntoResponse for ApiResponse<T>
impl<T: Serialize> IntoResponse for ApiResponse<T> {
    fn into_response(self) -> Response<Body> {
        let status = match self.status {
            ResponseStatus::Ok => StatusCode::OK,
            ResponseStatus::Error => {
                // Extract status from error detail
                self.error
                    .as_ref()
                    .and_then(|e| parse_error_type(&e.error_type).ok())
                    .map(error_to_status_code)
                    .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
            }
        };

        let json = match serde_json::to_string(&self) {
            Ok(json) => json,
            Err(e) => {
                tracing::error!("Failed to serialize ApiResponse: {}", e);
                return build_or_500(
                    Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .header(header::CONTENT_TYPE, "application/json"),
                    Body::from(r#"{"status":"ERROR","message":"Serialization failed"}"#),
                );
            }
        };

        build_or_500(
            Response::builder()
                .status(status)
                .header(header::CONTENT_TYPE, "application/json"),
            Body::from(json),
        )
    }
}

/// Implement IntoResponse for BinaryResponse
impl IntoResponse for BinaryResponse {
    fn into_response(self) -> Response<Body> {
        // `get_media_type` resolves the seeding cascade: a declared level-3 override verbatim,
        // otherwise the type derived from the effective data format. Shaping this header is an
        // intended capability, which is why an override reaches here unchanged — the guard is on
        // the *shape* of the string, applied when it is stored.
        let media_type = self.metadata.get_media_type();
        let media_type = if media_type.is_empty() {
            "application/octet-stream"
        } else {
            &media_type
        };

        let mut response = Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, media_type);

        // Add metadata status header
        response = response.header("X-Liquers-Status", format!("{:?}", self.metadata.status()));

        build_or_500(response, Body::from(self.data))
    }
}

/// Implement IntoResponse for DataEntry (unified entry endpoint)
/// Serializes using the format specified in the response headers or defaults to CBOR
impl IntoResponse for DataEntry {
    fn into_response(self) -> Response<Body> {
        // Default to CBOR for DataEntry (most efficient for binary data)
        let format = SerializationFormat::Cbor;

        match serialize_data_entry(&self, format) {
            Ok(bytes) => build_or_500(
                Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, format.mime_type()),
                Body::from(bytes),
            ),
            Err(e) => {
                tracing::error!("Failed to serialize DataEntry: {}", e);
                build_or_500(
                    Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .header(header::CONTENT_TYPE, "application/json"),
                    Body::from(format!(
                        r#"{{"status":"ERROR","message":"Serialization failed: {}"}}"#,
                        e
                    )),
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_core::ErrorDetail;
    use liquers_core::metadata::Metadata;

    #[test]
    fn test_api_response_ok_into_response() {
        let response: ApiResponse<String> = ApiResponse::ok("test result".to_string(), "Success");
        let axum_response = response.into_response();

        assert_eq!(axum_response.status(), StatusCode::OK);
        assert_eq!(
            axum_response.headers().get(header::CONTENT_TYPE).unwrap(),
            "application/json"
        );
    }

    #[test]
    fn test_api_response_error_into_response() {
        let error_detail = ErrorDetail {
            error_type: "ParseError".to_string(),
            message: "Invalid syntax".to_string(),
            query: Some("test/query".to_string()),
            key: None,
            traceback: None,
            metadata: None,
        };
        let response: ApiResponse<String> = ApiResponse::error(error_detail, "Failed");
        let axum_response = response.into_response();

        assert_eq!(axum_response.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_binary_response_into_response() {
        // Create metadata with media type set
        let metadata_json = serde_json::json!({
            "media_type": "text/plain"
        });
        let metadata = Metadata::from_json_value(metadata_json).unwrap();

        let response = BinaryResponse {
            data: b"Hello, world!".to_vec(),
            metadata,
        };
        let axum_response = response.into_response();

        assert_eq!(axum_response.status(), StatusCode::OK);
        // The media type from metadata may be quoted in JSON serialization
        let content_type = axum_response
            .headers()
            .get(header::CONTENT_TYPE)
            .unwrap()
            .to_str()
            .unwrap();
        // Accept either quoted or unquoted
        assert!(
            content_type == "text/plain" || content_type == "\"text/plain\"",
            "Expected text/plain but got: {}",
            content_type
        );
    }

    #[test]
    fn test_binary_response_default_content_type() {
        let metadata = Metadata::new(); // No media type set

        let response = BinaryResponse {
            data: vec![1, 2, 3, 4, 5],
            metadata,
        };
        let axum_response = response.into_response();

        assert_eq!(
            axum_response.headers().get(header::CONTENT_TYPE).unwrap(),
            "application/octet-stream"
        );
    }

    #[test]
    fn test_data_entry_into_response() {
        let entry = DataEntry {
            metadata: serde_json::json!({"key": "value"}),
            data: vec![1, 2, 3, 4, 5],
        };
        let axum_response = entry.into_response();

        assert_eq!(axum_response.status(), StatusCode::OK);
        assert_eq!(
            axum_response.headers().get(header::CONTENT_TYPE).unwrap(),
            "application/cbor"
        );
    }
}
