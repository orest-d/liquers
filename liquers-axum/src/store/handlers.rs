use crate::api_core::{error_to_detail, ApiResponse, BinaryResponse};
use crate::assets::common::metadata_json;
use axum::{
    body::Bytes,
    extract::{Path, Query as AxumQuery, State},
    response::{IntoResponse, Response},
    Json,
};
use liquers_core::{
    assets::AssetManager,
    context::{EnvRef, Environment},
    metadata::{Metadata, MetadataRecord},
    parse::parse_key,
    query::Key,
};
use std::collections::HashMap;

/// Tells the asset manager that `key` was written or removed through the store directly, so the
/// recipe provider drops directory-derived caches and registered listing versions refresh. The
/// write already succeeded; a failed refresh never undoes it.
async fn refresh_after_write<E: Environment>(env: &EnvRef<E>, key: &Key) {
    env.get_asset_manager()
        .refresh_listing_version(&key.parent())
        .await;
}

/// GET /api/store/data/{*key} - Retrieve raw data from store
pub async fn get_data_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(key_path): Path<String>,
) -> Response {
    // Parse key from path
    let key = match parse_key(&key_path) {
        Ok(k) => k,
        Err(e) => {
            let error_detail = error_to_detail(&e);
            let response: ApiResponse<()> = ApiResponse::error(error_detail, "Failed to parse key");
            return response.into_response();
        }
    };

    // Get store from environment
    let store = env.get_async_store();

    // Retrieve data from store
    match store.get_bytes(&key).await {
        Ok(data) => {
            // Also get metadata to include in response
            let metadata = store
                .get_metadata(&key)
                .await
                .unwrap_or_else(|_| Metadata::new());

            BinaryResponse { data, metadata }.into_response()
        }
        Err(e) => {
            let error_detail = error_to_detail(&e);
            let response: ApiResponse<()> =
                ApiResponse::error(error_detail, "Failed to retrieve data");
            response.into_response()
        }
    }
}

/// PUT /api/store/data/{*key} - Store raw data
pub async fn put_data_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(key_path): Path<String>,
    body: Bytes,
) -> Response {
    // Parse key from path
    let key = match parse_key(&key_path) {
        Ok(k) => k,
        Err(e) => {
            let error_detail = error_to_detail(&e);
            let response: ApiResponse<()> = ApiResponse::error(error_detail, "Failed to parse key");
            return response.into_response();
        }
    };

    // Get store from environment
    let store = env.get_async_store();

    // Get existing metadata for the key (or create new)
    let metadata = store
        .get_metadata(&key)
        .await
        .unwrap_or_else(|_| Metadata::new());

    // Store data with metadata
    match store.set(&key, &body, &metadata).await {
        Ok(()) => {
            refresh_after_write(&env, &key).await;
            let response: ApiResponse<String> =
                ApiResponse::ok(key.encode(), "Data stored successfully");
            response.into_response()
        }
        Err(e) => {
            let error_detail = error_to_detail(&e);
            let response: ApiResponse<()> =
                ApiResponse::error(error_detail, "Failed to store data");
            response.into_response()
        }
    }
}

/// DELETE /api/store/data/{*key} - Delete data from store
pub async fn delete_data_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(key_path): Path<String>,
) -> Response {
    // Parse key from path
    let key = match parse_key(&key_path) {
        Ok(k) => k,
        Err(e) => {
            let error_detail = error_to_detail(&e);
            let response: ApiResponse<()> = ApiResponse::error(error_detail, "Failed to parse key");
            return response.into_response();
        }
    };

    // Get store from environment
    let store = env.get_async_store();

    // Delete data
    match store.remove(&key).await {
        Ok(()) => {
            refresh_after_write(&env, &key).await;
            let response: ApiResponse<String> =
                ApiResponse::ok(key.encode(), "Data deleted successfully");
            response.into_response()
        }
        Err(e) => {
            let error_detail = error_to_detail(&e);
            let response: ApiResponse<()> =
                ApiResponse::error(error_detail, "Failed to delete data");
            response.into_response()
        }
    }
}

/// GET /api/store/metadata/{*key} - Retrieve metadata as JSON
pub async fn get_metadata_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(key_path): Path<String>,
) -> Response {
    // Parse key from path
    let key = match parse_key(&key_path) {
        Ok(k) => k,
        Err(e) => {
            let error_detail = error_to_detail(&e);
            let response: ApiResponse<()> = ApiResponse::error(error_detail, "Failed to parse key");
            return response.into_response();
        }
    };

    // Get store from environment
    let store = env.get_async_store();

    // Retrieve metadata
    match store.get_metadata(&key).await {
        Ok(metadata) => {
            // A legacy document is served as stored, as the Assets API does.
            let response: ApiResponse<serde_json::Value> = ApiResponse::ok(
                metadata_json(&metadata),
                "Metadata retrieved successfully",
            );
            response.into_response()
        }
        Err(e) => {
            let error_detail = error_to_detail(&e);
            let response: ApiResponse<()> =
                ApiResponse::error(error_detail, "Failed to retrieve metadata");
            response.into_response()
        }
    }
}

/// PUT /api/store/metadata/{*key} - Update metadata from JSON body
pub async fn put_metadata_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(key_path): Path<String>,
    Json(metadata_json): Json<serde_json::Value>,
) -> Response {
    // Parse key from path
    let key = match parse_key(&key_path) {
        Ok(k) => k,
        Err(e) => {
            let error_detail = error_to_detail(&e);
            let response: ApiResponse<()> = ApiResponse::error(error_detail, "Failed to parse key");
            return response.into_response();
        }
    };

    // Parse metadata from JSON
    let metadata = match Metadata::from_json_value(metadata_json) {
        Ok(m) => m,
        Err(e) => {
            let error_detail = crate::api_core::ErrorDetail {
                error_type: "SerializationError".to_string(),
                message: format!("Failed to parse metadata JSON: {}", e),
                query: None,
                key: Some(key.encode()),
                traceback: None,
                metadata: None,
            };
            let response: ApiResponse<()> =
                ApiResponse::error(error_detail, "Invalid metadata JSON");
            return response.into_response();
        }
    };

    // Get store from environment
    let store = env.get_async_store();

    // Update metadata in store
    match store.set_metadata(&key, &metadata).await {
        Ok(()) => {
            refresh_after_write(&env, &key).await;
            let response: ApiResponse<String> =
                ApiResponse::ok(key.encode(), "Metadata updated successfully");
            response.into_response()
        }
        Err(e) => {
            let error_detail = error_to_detail(&e);
            let response: ApiResponse<()> =
                ApiResponse::error(error_detail, "Failed to update metadata");
            response.into_response()
        }
    }
}

// ============================================================================
// Directory Operations (Task #26)
// ============================================================================

/// GET /api/store/listdir/{*key} - List directory contents
pub async fn listdir_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(key_path): Path<String>,
) -> Response {
    let key = match parse_key(&key_path) {
        Ok(k) => k,
        Err(e) => {
            let error_detail = error_to_detail(&e);
            let response: ApiResponse<()> = ApiResponse::error(error_detail, "Failed to parse key");
            return response.into_response();
        }
    };

    let store = env.get_async_store();

    match store.listdir_keys(&key).await {
        Ok(keys) => {
            let key_strings: Vec<String> = keys.iter().map(|k| k.encode()).collect();
            let response: ApiResponse<Vec<String>> =
                ApiResponse::ok(key_strings, "Directory listed successfully");
            response.into_response()
        }
        Err(e) => {
            let error_detail = error_to_detail(&e);
            let response: ApiResponse<()> =
                ApiResponse::error(error_detail, "Failed to list directory");
            response.into_response()
        }
    }
}

/// GET /api/store/is_dir/{*key} - Check if key is a directory
pub async fn is_dir_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(key_path): Path<String>,
) -> Response {
    let key = match parse_key(&key_path) {
        Ok(k) => k,
        Err(e) => {
            let error_detail = error_to_detail(&e);
            let response: ApiResponse<()> = ApiResponse::error(error_detail, "Failed to parse key");
            return response.into_response();
        }
    };

    let store = env.get_async_store();

    match store.is_dir(&key).await {
        Ok(is_directory) => {
            let response: ApiResponse<bool> =
                ApiResponse::ok(is_directory, "Directory check completed");
            response.into_response()
        }
        Err(e) => {
            let error_detail = error_to_detail(&e);
            let response: ApiResponse<()> =
                ApiResponse::error(error_detail, "Failed to check directory status");
            response.into_response()
        }
    }
}

/// GET /api/store/contains/{*key} - Check if key exists in store
pub async fn contains_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(key_path): Path<String>,
) -> Response {
    let key = match parse_key(&key_path) {
        Ok(k) => k,
        Err(e) => {
            let error_detail = error_to_detail(&e);
            let response: ApiResponse<()> = ApiResponse::error(error_detail, "Failed to parse key");
            return response.into_response();
        }
    };

    let store = env.get_async_store();

    match store.contains(&key).await {
        Ok(exists) => {
            let response: ApiResponse<bool> = ApiResponse::ok(exists, "Contains check completed");
            response.into_response()
        }
        Err(e) => {
            let error_detail = error_to_detail(&e);
            let response: ApiResponse<()> =
                ApiResponse::error(error_detail, "Failed to check if key exists");
            response.into_response()
        }
    }
}

/// GET /api/store/keys?prefix={prefix} - Every key under `prefix` (default: the whole store), at
/// any depth, directories included as `AsyncStore::listdir_keys_deep` reports them. For the keys
/// directly in a directory, use `listdir`.
pub async fn keys_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    AxumQuery(params): AxumQuery<HashMap<String, String>>,
) -> Response {
    let store = env.get_async_store();

    // Get optional prefix parameter
    let prefix_key = if let Some(prefix_str) = params.get("prefix") {
        match parse_key(prefix_str) {
            Ok(k) => Some(k),
            Err(e) => {
                let error_detail = error_to_detail(&e);
                let response: ApiResponse<()> =
                    ApiResponse::error(error_detail, "Failed to parse prefix key");
                return response.into_response();
            }
        }
    } else {
        None
    };

    let prefix = prefix_key.unwrap_or_else(Key::new);
    let result = store.listdir_keys_deep(&prefix).await;

    match result {
        Ok(keys) => {
            let key_strings: Vec<String> = keys.iter().map(|k| k.encode()).collect();
            let response: ApiResponse<Vec<String>> =
                ApiResponse::ok(key_strings, "Keys listed successfully");
            response.into_response()
        }
        Err(e) => {
            let error_detail = error_to_detail(&e);
            let response: ApiResponse<()> = ApiResponse::error(error_detail, "Failed to list keys");
            response.into_response()
        }
    }
}

/// PUT /api/store/makedir/{*key} - Create directory
pub async fn makedir_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(key_path): Path<String>,
) -> Response {
    let key = match parse_key(&key_path) {
        Ok(k) => k,
        Err(e) => {
            let error_detail = error_to_detail(&e);
            let response: ApiResponse<()> = ApiResponse::error(error_detail, "Failed to parse key");
            return response.into_response();
        }
    };

    let store = env.get_async_store();

    match store.makedir(&key).await {
        Ok(()) => {
            refresh_after_write(&env, &key).await;
            let response: ApiResponse<String> =
                ApiResponse::ok(key.encode(), "Directory created successfully");
            response.into_response()
        }
        Err(e) => {
            let error_detail = error_to_detail(&e);
            let response: ApiResponse<()> =
                ApiResponse::error(error_detail, "Failed to create directory");
            response.into_response()
        }
    }
}

/// DELETE /api/store/removedir/{*key} - Remove directory
pub async fn removedir_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(key_path): Path<String>,
) -> Response {
    let key = match parse_key(&key_path) {
        Ok(k) => k,
        Err(e) => {
            let error_detail = error_to_detail(&e);
            let response: ApiResponse<()> = ApiResponse::error(error_detail, "Failed to parse key");
            return response.into_response();
        }
    };

    let store = env.get_async_store();

    match store.removedir(&key).await {
        Ok(()) => {
            refresh_after_write(&env, &key).await;
            let response: ApiResponse<String> =
                ApiResponse::ok(key.encode(), "Directory removed successfully");
            response.into_response()
        }
        Err(e) => {
            let error_detail = error_to_detail(&e);
            let response: ApiResponse<()> =
                ApiResponse::error(error_detail, "Failed to remove directory");
            response.into_response()
        }
    }
}

// ============================================================================
// Unified Entry Endpoints (Task #27)
// ============================================================================

/// GET /api/store/entry/{*key} - Retrieve data and metadata as unified entry
/// Supports CBOR, bincode, and JSON formats via ?format= or Accept header
pub async fn get_entry_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(key_path): Path<String>,
    headers: axum::http::HeaderMap,
    AxumQuery(params): AxumQuery<HashMap<String, String>>,
) -> Response {
    use crate::api_core::{select_format, serialize_data_entry, DataEntry};

    let key = match parse_key(&key_path) {
        Ok(k) => k,
        Err(e) => {
            let error_detail = error_to_detail(&e);
            let response: ApiResponse<()> = ApiResponse::error(error_detail, "Failed to parse key");
            return response.into_response();
        }
    };

    let store = env.get_async_store();

    // Get both data and metadata
    match store.get(&key).await {
        Ok((data, metadata)) => {
            // A legacy document is served as stored, as the Assets API does.
            let entry = DataEntry {
                metadata: metadata_json(&metadata),
                data,
            };

            // Select format based on query param or Accept header
            let format = select_format(params.get("format").map(|s| s.as_str()), &headers);

            // Serialize and return
            match serialize_data_entry(&entry, format) {
                Ok(bytes) => crate::axum_integration::build_or_500(
                    axum::http::Response::builder()
                        .status(axum::http::StatusCode::OK)
                        .header(axum::http::header::CONTENT_TYPE, format.mime_type()),
                    axum::body::Body::from(bytes),
                ),
                Err(e) => {
                    let error_detail = crate::api_core::ErrorDetail {
                        error_type: "SerializationError".to_string(),
                        message: format!("Failed to serialize entry: {}", e),
                        query: None,
                        key: Some(key.encode()),
                        traceback: None,
                        metadata: None,
                    };
                    let response: ApiResponse<()> =
                        ApiResponse::error(error_detail, "Serialization failed");
                    response.into_response()
                }
            }
        }
        Err(e) => {
            let error_detail = error_to_detail(&e);
            let response: ApiResponse<()> =
                ApiResponse::error(error_detail, "Failed to retrieve entry");
            response.into_response()
        }
    }
}

/// PUT /api/store/entry/{*key} - Store data and metadata as unified entry
/// Supports CBOR, bincode, and JSON formats via ?format= or Content-Type header
pub async fn put_entry_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(key_path): Path<String>,
    headers: axum::http::HeaderMap,
    AxumQuery(params): AxumQuery<HashMap<String, String>>,
    body: Bytes,
) -> Response {
    use crate::api_core::{deserialize_data_entry, format_from_content_type};

    let key = match parse_key(&key_path) {
        Ok(k) => k,
        Err(e) => {
            let error_detail = error_to_detail(&e);
            let response: ApiResponse<()> = ApiResponse::error(error_detail, "Failed to parse key");
            return response.into_response();
        }
    };

    // Determine format from query param or Content-Type header
    let format = if let Some(format_str) = params.get("format") {
        match format_str.to_lowercase().as_str() {
            "cbor" => crate::api_core::SerializationFormat::Cbor,
            "bincode" => crate::api_core::SerializationFormat::Bincode,
            "json" => crate::api_core::SerializationFormat::Json,
            _ => format_from_content_type(&headers)
                .unwrap_or(crate::api_core::SerializationFormat::Cbor),
        }
    } else {
        format_from_content_type(&headers).unwrap_or(crate::api_core::SerializationFormat::Cbor)
    };

    // Deserialize DataEntry
    let entry = match deserialize_data_entry(&body, format) {
        Ok(e) => e,
        Err(err) => {
            let error_detail = crate::api_core::ErrorDetail {
                error_type: "SerializationError".to_string(),
                message: format!("Failed to deserialize entry: {}", err),
                query: None,
                key: Some(key.encode()),
                traceback: None,
                metadata: None,
            };
            let response: ApiResponse<()> =
                ApiResponse::error(error_detail, "Deserialization failed");
            return response.into_response();
        }
    };

    // Parse metadata from JSON
    let metadata = match Metadata::from_json_value(entry.metadata) {
        Ok(m) => m,
        Err(e) => {
            let error_detail = crate::api_core::ErrorDetail {
                error_type: "SerializationError".to_string(),
                message: format!("Failed to parse metadata: {}", e),
                query: None,
                key: Some(key.encode()),
                traceback: None,
                metadata: None,
            };
            let response: ApiResponse<()> = ApiResponse::error(error_detail, "Invalid metadata");
            return response.into_response();
        }
    };

    let store = env.get_async_store();

    // Store data and metadata
    match store.set(&key, &entry.data, &metadata).await {
        Ok(()) => {
            refresh_after_write(&env, &key).await;
            let response: ApiResponse<String> =
                ApiResponse::ok(key.encode(), "Entry stored successfully");
            response.into_response()
        }
        Err(e) => {
            let error_detail = error_to_detail(&e);
            let response: ApiResponse<()> =
                ApiResponse::error(error_detail, "Failed to store entry");
            response.into_response()
        }
    }
}

/// DELETE /api/store/entry/{*key} - Delete entry (same as DELETE /data)
pub async fn delete_entry_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(key_path): Path<String>,
) -> Response {
    // Delegate to delete_data_handler (same behavior)
    delete_data_handler(State(env), Path(key_path)).await
}

// ============================================================================
// Optional GET-based Destructive Operations (Task #28)
// ============================================================================

/// GET /api/store/remove/{*key} - Delete data via GET (opt-in for legacy compatibility)
pub async fn get_remove_handler<E: Environment>(
    state: State<EnvRef<E>>,
    path: Path<String>,
) -> Response {
    // Delegate to DELETE /data handler
    delete_data_handler(state, path).await
}

/// GET /api/store/removedir/{*key} - Remove directory via GET (opt-in for legacy compatibility)
pub async fn get_removedir_handler<E: Environment>(
    state: State<EnvRef<E>>,
    path: Path<String>,
) -> Response {
    // Delegate to DELETE /removedir handler
    removedir_handler(state, path).await
}

/// GET /api/store/makedir/{*key} - Create directory via GET (opt-in for legacy compatibility)
pub async fn get_makedir_handler<E: Environment>(
    state: State<EnvRef<E>>,
    path: Path<String>,
) -> Response {
    // Delegate to PUT /makedir handler
    makedir_handler(state, path).await
}

// ============================================================================
// Multipart Upload Endpoint (Task #29)
// ============================================================================

/// The media type an upload declares: the part's `Content-Type` when it differs from `derived`,
/// the type the filename already implies. `MetadataRecord::media_type` is a declared override, not
/// a cache of the derived type, so an upload whose type matches its filename declares nothing. An
/// absent, empty or `application/octet-stream` type (a browser's default for an unknown file)
/// declares nothing either.
fn declared_media_type(derived: &str, content_type: Option<&str>) -> Option<String> {
    let content_type = content_type?.trim();
    if content_type.is_empty() || content_type == "application/octet-stream" || content_type == derived
    {
        None
    } else {
        Some(content_type.to_string())
    }
}

/// POST /api/store/upload/{*key} - Upload files via multipart/form-data
/// Uploads are stored under the provided key path, with filenames appended
/// Returns a list of successfully uploaded file keys
pub async fn upload_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(key_path): Path<String>,
    mut multipart: axum::extract::Multipart,
) -> Response {
    // Parse base key from path
    let base_key = match parse_key(&key_path) {
        Ok(k) => k,
        Err(e) => {
            let error_detail = error_to_detail(&e);
            let response: ApiResponse<()> = ApiResponse::error(error_detail, "Failed to parse key");
            return response.into_response();
        }
    };

    let store = env.get_async_store();
    let mut uploaded_files = Vec::new();
    let mut errors = Vec::new();

    // Process each field in the multipart data
    while let Ok(Some(field)) = multipart.next_field().await {
        let file_name = match field.file_name() {
            Some(name) => name.to_string(),
            None => {
                // Skip fields without filenames (likely form fields, not files)
                continue;
            }
        };
        // The filename lets the data format (and so the effective media type) be derived; the
        // part's Content-Type is declared only when the filename does not already imply it. Read
        // before the field is consumed by `bytes()`.
        let mut record = MetadataRecord::new();
        record.with_filename(file_name.clone());
        if let Some(media_type) = declared_media_type(&record.get_media_type(), field.content_type()) {
            record.with_media_type(media_type);
        }
        let metadata = Metadata::MetadataRecord(record);

        // Read file data
        let data = match field.bytes().await {
            Ok(bytes) => bytes.to_vec(),
            Err(e) => {
                errors.push(format!("Failed to read file '{}': {}", file_name, e));
                continue;
            }
        };

        // Construct storage key by appending filename to base key
        let file_key = if base_key.is_empty() {
            match parse_key(&file_name) {
                Ok(k) => k,
                Err(_) => {
                    errors.push(format!("Invalid filename: {}", file_name));
                    continue;
                }
            }
        } else {
            base_key.join(&file_name)
        };


        // Store the file
        match store.set(&file_key, &data, &metadata).await {
            Ok(()) => {
                refresh_after_write(&env, &file_key).await;
                uploaded_files.push(file_key.encode());
            }
            Err(e) => {
                errors.push(format!(
                    "Failed to store file '{}': {}",
                    file_name, e.message
                ));
            }
        }
    }

    // Return response with uploaded files and any errors
    if uploaded_files.is_empty() && !errors.is_empty() {
        // All uploads failed
        let error_detail = crate::api_core::ErrorDetail {
            error_type: "KeyWriteError".to_string(),
            message: format!("All uploads failed: {}", errors.join("; ")),
            query: None,
            key: Some(base_key.encode()),
            traceback: Some(errors),
            metadata: None,
        };
        let response: ApiResponse<()> = ApiResponse::error(error_detail, "Upload failed");
        response.into_response()
    } else {
        // At least some uploads succeeded
        #[derive(serde::Serialize)]
        struct UploadResult {
            uploaded: Vec<String>,
            #[serde(skip_serializing_if = "Vec::is_empty")]
            errors: Vec<String>,
        }

        let result = UploadResult {
            uploaded: uploaded_files,
            errors,
        };

        let response: ApiResponse<UploadResult> = ApiResponse::ok(result, "Upload completed");
        response.into_response()
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upload_matching_its_extension_declares_nothing() {
        assert_eq!(declared_media_type("text/csv", Some("text/csv")), None);
    }

    #[test]
    fn upload_differing_from_its_extension_declares_its_type() {
        assert_eq!(
            declared_media_type("application/octet-stream", Some("image/png")),
            Some("image/png".to_string())
        );
    }

    #[test]
    fn upload_with_default_or_absent_type_declares_nothing() {
        assert_eq!(declared_media_type("text/plain", Some("application/octet-stream")), None);
        assert_eq!(declared_media_type("text/plain", Some("")), None);
        assert_eq!(declared_media_type("text/plain", None), None);
    }
}
