use crate::api_core::{error_to_detail, ApiResponse, BinaryResponse};
use axum::{
    extract::{Path, State},
    response::{IntoResponse, Response},
    Json,
};
use liquers_core::{
    context::{EnvRef, Environment},
    metadata::Status,
    parse::parse_query,
};
use serde_json::Value as JsonValue;

/// GET /q/{*query} - Execute query and return result
pub async fn get_query_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(query_path): Path<String>,
) -> Response {
    // Parse query from path
    let query = match parse_query(&query_path) {
        Ok(q) => q,
        Err(e) => {
            let error_detail = error_to_detail(&e);
            let response: ApiResponse<()> =
                ApiResponse::error(error_detail, "Failed to parse query");
            return response.into_response();
        }
    };

    // Evaluate query
    let asset_ref = match env.evaluate(&query).await {
        Ok(asset) => asset,
        Err(e) => {
            let error_detail = error_to_detail(&e);
            let response: ApiResponse<()> =
                ApiResponse::error(error_detail, "Query evaluation failed");
            return response.into_response();
        }
    };

    // Poll for result with 30-second timeout
    let timeout = tokio::time::Duration::from_secs(30);
    let start = tokio::time::Instant::now();

    loop {
        if start.elapsed() > timeout {
            let error_detail = crate::api_core::ErrorDetail {
                error_type: "ExecutionError".to_string(),
                message: "Query execution timed out after 30 seconds".to_string(),
                query: Some(query.encode()),
                key: None,
                traceback: None,
                metadata: None,
            };
            let response: ApiResponse<()> =
                ApiResponse::error(error_detail, "Query execution timeout");
            return response.into_response();
        }

        // Check if binary data is ready
        if let Some((data_arc, metadata_arc)) = asset_ref.poll_binary().await {
            let data = (*data_arc).clone();
            let metadata = (*metadata_arc).clone();
            return BinaryResponse { data, metadata }.into_response();
        }

        // Check status for errors
        let status = asset_ref.status().await;
        match status {
            Status::Error => {
                // Try to get error details via get_binary() which should fail
                if let Err(e) = asset_ref.get_binary().await {
                    let error_detail = error_to_detail(&e);
                    let response: ApiResponse<()> =
                        ApiResponse::error(error_detail, "Query execution failed");
                    return response.into_response();
                } else {
                    // Status is error but get_binary succeeded - shouldn't happen
                    let error_detail = crate::api_core::ErrorDetail {
                        error_type: "ExecutionError".to_string(),
                        message: "Query execution failed".to_string(),
                        query: Some(query.encode()),
                        key: None,
                        traceback: None,
                        metadata: None,
                    };
                    let response: ApiResponse<()> =
                        ApiResponse::error(error_detail, "Query execution failed");
                    return response.into_response();
                }
            }
            Status::Ready => {
                // Data should be available, but poll_binary returned None above
                // This is a race condition - loop again
            }
            Status::Cancelled => {
                let error_detail = crate::api_core::ErrorDetail {
                    error_type: "ExecutionError".to_string(),
                    message: "Query execution was cancelled".to_string(),
                    query: Some(query.encode()),
                    key: None,
                    traceback: None,
                    metadata: None,
                };
                let response: ApiResponse<()> =
                    ApiResponse::error(error_detail, "Query execution cancelled");
                return response.into_response();
            }
            _ => {
                // Still processing, wait and retry
            }
        }

        // Not ready yet, wait a bit before polling again
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    }
}

/// POST /q/{*query} - Execute query with optional JSON body
/// Body format: {"parameters": ["arg1", "arg2", ...]} (optional, for future use)
/// Currently behaves the same as GET - body arguments are logged but not used
/// TODO: Implement parameter passing mechanism once query modification API is finalized
pub async fn post_query_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(query_path): Path<String>,
    Json(body): Json<Option<JsonValue>>,
) -> Response {
    // Log body if present (for future implementation)
    if let Some(args) = body {
        tracing::debug!("POST query received with body: {:?}", args);
        // TODO: Implement query parameter modification once API is designed
    }

    // Parse query from path (same as GET for now)
    let query = match parse_query(&query_path) {
        Ok(q) => q,
        Err(e) => {
            let error_detail = error_to_detail(&e);
            let response: ApiResponse<()> =
                ApiResponse::error(error_detail, "Failed to parse query");
            return response.into_response();
        }
    };

    // Evaluate query using the same logic as GET handler
    let asset_ref = match env.evaluate(&query).await {
        Ok(asset) => asset,
        Err(e) => {
            let error_detail = error_to_detail(&e);
            let response: ApiResponse<()> =
                ApiResponse::error(error_detail, "Query evaluation failed");
            return response.into_response();
        }
    };

    // Poll for result with 30-second timeout (same as GET)
    let timeout = tokio::time::Duration::from_secs(30);
    let start = tokio::time::Instant::now();

    loop {
        if start.elapsed() > timeout {
            let error_detail = crate::api_core::ErrorDetail {
                error_type: "ExecutionError".to_string(),
                message: "Query execution timed out after 30 seconds".to_string(),
                query: Some(query.encode()),
                key: None,
                traceback: None,
                metadata: None,
            };
            let response: ApiResponse<()> =
                ApiResponse::error(error_detail, "Query execution timeout");
            return response.into_response();
        }

        if let Some((data_arc, metadata_arc)) = asset_ref.poll_binary().await {
            let data = (*data_arc).clone();
            let metadata = (*metadata_arc).clone();
            return BinaryResponse { data, metadata }.into_response();
        }

        let status = asset_ref.status().await;
        match status {
            Status::Error => {
                if let Err(e) = asset_ref.get_binary().await {
                    let error_detail = error_to_detail(&e);
                    let response: ApiResponse<()> =
                        ApiResponse::error(error_detail, "Query execution failed");
                    return response.into_response();
                } else {
                    let error_detail = crate::api_core::ErrorDetail {
                        error_type: "ExecutionError".to_string(),
                        message: "Query execution failed".to_string(),
                        query: Some(query.encode()),
                        key: None,
                        traceback: None,
                        metadata: None,
                    };
                    let response: ApiResponse<()> =
                        ApiResponse::error(error_detail, "Query execution failed");
                    return response.into_response();
                }
            }
            Status::Cancelled => {
                let error_detail = crate::api_core::ErrorDetail {
                    error_type: "ExecutionError".to_string(),
                    message: "Query execution was cancelled".to_string(),
                    query: Some(query.encode()),
                    key: None,
                    traceback: None,
                    metadata: None,
                };
                let response: ApiResponse<()> =
                    ApiResponse::error(error_detail, "Query execution cancelled");
                return response.into_response();
            }
            _ => {}
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    }
}
