//! Assets API Handlers - HTTP request handlers for asset operations
//!
//! Part of the Assets API implementation.
//! See specs/axum-assets-recipes-api/phase2-architecture.md for specifications.

use axum::{
    body::Bytes,
    extract::{Path, Query as AxumQuery, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use liquers_core::{
    assets::AssetManager,
    context::{EnvRef, Environment},
    parse::parse_query,
    value::ValueInterface,
};
use std::collections::HashMap;

use crate::api_core::{
    error::error_to_detail, ApiResponse, BinaryResponse,
};

/// GET /data/{*query} - Retrieve asset data
pub async fn get_data_handler<E: Environment>(
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

    // Get AssetManager from environment
    let asset_manager = env.get_asset_manager();

    // Get asset for query
    let asset_ref = match (**asset_manager).get_asset(&query).await {
        Ok(asset) => asset,
        Err(e) => {
            let error_detail = error_to_detail(&e);
            let response: ApiResponse<()> =
                ApiResponse::error(error_detail, "Failed to get asset");
            return response.into_response();
        }
    };

    // Get state from asset (waits for asset to be ready)
    let state = match asset_ref.get().await {
        Ok(s) => s,
        Err(e) => {
            let error_detail = error_to_detail(&e);
            let response: ApiResponse<()> =
                ApiResponse::error(error_detail, "Failed to retrieve asset value");
            return response.into_response();
        }
    };

    // Serialize value to bytes
    let data = match state.data.try_into_bytes() {
        Ok(bytes) => bytes,
        Err(e) => {
            let error_detail = error_to_detail(&e);
            let response: ApiResponse<()> =
                ApiResponse::error(error_detail, "Failed to serialize asset value");
            return response.into_response();
        }
    };

    // Return binary response with data and metadata
    BinaryResponse {
        data,
        metadata: (*state.metadata).clone(),
    }
    .into_response()
}

/// POST /data/{*query} - Set asset data (not typically used - assets are computed)
pub async fn post_data_handler<E: Environment>(
    State(_env): State<EnvRef<E>>,
    Path(_query_path): Path<String>,
    _body: Bytes,
) -> Response {
    // Assets are typically computed from queries, not directly set
    // This endpoint exists for API completeness but may not be commonly used
    (
        StatusCode::NOT_IMPLEMENTED,
        "Direct asset data modification not supported",
    )
        .into_response()
}

/// DELETE /data/{*query} - Delete asset data
pub async fn delete_data_handler<E: Environment>(
    State(_env): State<EnvRef<E>>,
    Path(query_path): Path<String>,
) -> Response {
    // Parse query
    let _query = match parse_query(&query_path) {
        Ok(q) => q,
        Err(e) => {
            let error_detail = error_to_detail(&e);
            let response: ApiResponse<()> =
                ApiResponse::error(error_detail, "Failed to parse query");
            return response.into_response();
        }
    };

    // Asset deletion would require AssetManager.delete() method
    // which needs to be added to the AssetManager trait
    (
        StatusCode::NOT_IMPLEMENTED,
        "Asset deletion not implemented yet",
    )
        .into_response()
}

/// GET /metadata/{*query} - Retrieve asset metadata
pub async fn get_metadata_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(query_path): Path<String>,
) -> Response {
    // Parse query
    let query = match parse_query(&query_path) {
        Ok(q) => q,
        Err(e) => {
            let error_detail = error_to_detail(&e);
            let response: ApiResponse<()> =
                ApiResponse::error(error_detail, "Failed to parse query");
            return response.into_response();
        }
    };

    // Get AssetManager
    let asset_manager = env.get_asset_manager();

    // Get asset
    let asset_ref = match asset_manager.get_asset(&query).await {
        Ok(asset) => asset,
        Err(e) => {
            let error_detail = error_to_detail(&e);
            let response: ApiResponse<()> =
                ApiResponse::error(error_detail, "Failed to get asset");
            return response.into_response();
        }
    };

    // Get metadata from asset
    let metadata = match asset_ref.get_metadata().await {
        Ok(m) => m,
        Err(e) => {
            let error_detail = error_to_detail(&e);
            let response: ApiResponse<()> =
                ApiResponse::error(error_detail, "Failed to get metadata");
            return response.into_response();
        }
    };

    // Extract MetadataRecord and serialize to JSON
    if let Some(record) = metadata.metadata_record() {
        let metadata_json = serde_json::to_value(&record).unwrap_or(serde_json::json!({}));
        let response: ApiResponse<serde_json::Value> =
            ApiResponse::ok(metadata_json, "Asset metadata retrieved");
        response.into_response()
    } else {
        // Legacy metadata
        let response: ApiResponse<serde_json::Value> =
            ApiResponse::ok(serde_json::json!({}), "No metadata available");
        response.into_response()
    }
}

/// POST /metadata/{*query} - Set asset metadata
pub async fn post_metadata_handler<E: Environment>(
    State(_env): State<EnvRef<E>>,
    Path(_query_path): Path<String>,
) -> Response {
    // Metadata modification not typically supported for computed assets
    (
        StatusCode::NOT_IMPLEMENTED,
        "Asset metadata modification not supported",
    )
        .into_response()
}

/// GET /entry/{*query} - Retrieve asset entry (data + metadata)
pub async fn get_entry_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(query_path): Path<String>,
    AxumQuery(params): AxumQuery<HashMap<String, String>>,
) -> Response {
    // Parse query
    let query = match parse_query(&query_path) {
        Ok(q) => q,
        Err(e) => {
            let error_detail = error_to_detail(&e);
            let response: ApiResponse<()> =
                ApiResponse::error(error_detail, "Failed to parse query");
            return response.into_response();
        }
    };

    // Get AssetManager
    let asset_manager = env.get_asset_manager();

    // Get asset
    let asset_ref = match asset_manager.get_asset(&query).await {
        Ok(asset) => asset,
        Err(e) => {
            let error_detail = error_to_detail(&e);
            let response: ApiResponse<()> =
                ApiResponse::error(error_detail, "Failed to get asset");
            return response.into_response();
        }
    };

    // Get state
    let state = match asset_ref.get().await {
        Ok(s) => s,
        Err(e) => {
            let error_detail = error_to_detail(&e);
            let response: ApiResponse<()> =
                ApiResponse::error(error_detail, "Failed to retrieve asset value");
            return response.into_response();
        }
    };

    // Serialize value to bytes
    let data = match state.data.try_into_bytes() {
        Ok(bytes) => bytes,
        Err(e) => {
            let error_detail = error_to_detail(&e);
            let response: ApiResponse<()> =
                ApiResponse::error(error_detail, "Failed to serialize asset value");
            return response.into_response();
        }
    };

    // Create DataEntry
    use crate::api_core::response::DataEntry;

    // Extract MetadataRecord for serialization (Metadata enum doesn't implement Serialize)
    let metadata_json = if let Some(record) = state.metadata.metadata_record() {
        serde_json::to_value(&record).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    let entry = DataEntry {
        data,
        metadata: metadata_json,
    };

    // Select serialization format (query param > Accept header > CBOR default)
    use crate::api_core::format::{select_format, serialize_data_entry};
    use axum::http::header::HeaderMap;

    // Extract format from query param
    let format = select_format(params.get("format").map(|s| s.as_str()), &HeaderMap::new());

    // Serialize entry
    match serialize_data_entry(&entry, format) {
        Ok(bytes) => {
            // Return with appropriate Content-Type
            use axum::http::header::CONTENT_TYPE;
            let mut headers = HeaderMap::new();
            headers.insert(CONTENT_TYPE, format.mime_type().parse().unwrap());
            (headers, bytes).into_response()
        }
        Err(e) => {
            let error_detail = crate::api_core::response::ErrorDetail {
                error_type: "SerializationError".to_string(),
                message: e,
                query: None,
                key: None,
                traceback: None,
                metadata: None,
            };
            let response: ApiResponse<()> =
                ApiResponse::error(error_detail, "Failed to serialize entry");
            response.into_response()
        }
    }
}

/// POST /entry/{*query} - Set asset entry (not supported)
pub async fn post_entry_handler<E: Environment>(
    State(_env): State<EnvRef<E>>,
    Path(_query_path): Path<String>,
) -> Response {
    (
        StatusCode::NOT_IMPLEMENTED,
        "Asset entry modification not supported",
    )
        .into_response()
}

/// DELETE /entry/{*query} - Delete asset entry
pub async fn delete_entry_handler<E: Environment>(
    State(_env): State<EnvRef<E>>,
    Path(_query_path): Path<String>,
) -> Response {
    (
        StatusCode::NOT_IMPLEMENTED,
        "Asset deletion not implemented yet",
    )
        .into_response()
}

/// GET /listdir/{*query} - List assets in directory
pub async fn listdir_handler<E: Environment>(
    State(_env): State<EnvRef<E>>,
    Path(_query_path): Path<String>,
) -> Response {
    // Asset directory listing not implemented yet
    // Would require AssetManager.list_assets() method
    (
        StatusCode::NOT_IMPLEMENTED,
        "Asset directory listing not implemented yet",
    )
        .into_response()
}

/// POST /cancel/{*query} - Cancel asset evaluation
pub async fn cancel_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(query_path): Path<String>,
) -> Response {
    // Parse query
    let query = match parse_query(&query_path) {
        Ok(q) => q,
        Err(e) => {
            let error_detail = error_to_detail(&e);
            let response: ApiResponse<()> =
                ApiResponse::error(error_detail, "Failed to parse query");
            return response.into_response();
        }
    };

    // Get AssetManager
    let asset_manager = env.get_asset_manager();

    // Get asset (but don't wait for it to be ready)
    let asset_ref = match asset_manager.get_asset(&query).await {
        Ok(asset) => asset,
        Err(e) => {
            let error_detail = error_to_detail(&e);
            let response: ApiResponse<()> =
                ApiResponse::error(error_detail, "Failed to get asset");
            return response.into_response();
        }
    };

    // Cancel the asset
    let _ = asset_ref.cancel().await;

    // Return success
    ApiResponse::ok("Asset cancelled", "Cancel request sent").into_response()
}
