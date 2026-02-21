# Phase 4: Implementation Plan - web-api-library

## Overview

**Feature:** Web API Library (liquers-axum rebuild)

**Architecture:** Composable library with builder pattern. Framework-agnostic core (ApiResponse, ErrorDetail, DataEntry, BinaryResponse) with Axum-specific handlers. Generic builders (QueryApiBuilder<E>, StoreApiBuilder<E>) create routers. Error mapping translates ErrorType to HTTP status codes. CBOR/bincode/JSON serialization for unified entry endpoints.

**Estimated complexity:** Medium-High
- 10+ new files, ~2000 lines of code
- Multiple integration points (Axum, liquers-core, liquers-store)
- Three serialization formats to support
- Comprehensive error handling across 19 ErrorType variants

**Estimated time:** 15-20 hours for experienced Rust developer
- Phase 1: Core infrastructure (3-4 hours)
- Phase 2-5: API implementation (8-10 hours)
- Phase 6-7: Testing and validation (4-6 hours)

**Prerequisites:**
- ✅ Phase 1, 2, 3 approved
- ✅ All open questions resolved (destructive GETs opt-in, no streaming, per-endpoint format)
- ✅ Dependencies identified: axum 0.8, ciborium 0.2, bincode 1.3, base64 0.22

## Implementation Steps

### Step 1: Create Module Structure and Cargo.toml

**File:** `liquers-axum/Cargo.toml`

**Action:**
- Add new dependencies for CBOR/bincode/JSON serialization
- Configure Axum features for multipart upload
- Add tower-http for CORS and tracing

**Code changes:**
```toml
# MODIFY: liquers-axum/Cargo.toml
[dependencies]
# Core liquers crates
liquers-core = { path = "../liquers-core" }
liquers-store = { path = "../liquers-store" }

# Web framework
axum = { version = "0.8.8", features = ["macros", "multipart"] }  # Use latest 0.8.x
tokio = { version = "1", features = ["full"] }
tower = { version = "0.5" }
tower-http = { version = "0.6", features = ["cors", "trace"] }

# Serialization (NEW)
serde = { version = "1", features = ["derive"] }
serde_json = "1"
ciborium = "0.2"      # CBOR support
bincode = "1.3"       # Bincode support
base64 = "0.22"       # Base64 for JSON data encoding

# Error handling
thiserror = "2"

# Logging
tracing = "0.1"

# Utilities
bytes = "1"

[features]
default = ["axum"]
axum = ["dep:axum"]  # Allow building without Axum for future framework support
```

**Validation:**
```bash
cargo check -p liquers-axum
# Expected: Compiles (may have warnings about unused dependencies)
```

**Rollback:**
```bash
git checkout liquers-axum/Cargo.toml
```

**Agent Specification:**
- **Model:** haiku
- **Skills:** rust-best-practices
- **Knowledge:** Phase 2 architecture doc (dependencies section)
- **Rationale:** Cargo.toml dependency updates follow standard pattern

---

### Step 2: Create Core Module Structure

**Files:**
- `liquers-axum/src/api_core/mod.rs` (new)
- `liquers-axum/src/api_core/response.rs` (new)
- `liquers-axum/src/api_core/error.rs` (new)
- `liquers-axum/src/api_core/format.rs` (new)
- `liquers-axum/src/query/mod.rs` (new)
- `liquers-axum/src/store/mod.rs` (new)

**Action:**
- Delete existing files (clean rebuild):
  - `src/core_handlers.rs`
  - `src/store_handlers.rs`
  - `src/environment.rs`
  - `src/utils.rs`
  - `src/value.rs`
- Create new directory structure
- Create empty module files with basic exports

**Code changes:**
```rust
// NEW: liquers-axum/src/api_core/mod.rs
pub mod response;
pub mod error;
pub mod format;

pub use response::{ApiResponse, ResponseStatus, ErrorDetail, DataEntry, BinaryResponse};
pub use error::{error_to_status_code, error_to_detail, parse_error_type};
pub use format::{SerializationFormat, select_format, serialize_data_entry, deserialize_data_entry};

// NEW: liquers-axum/src/query/mod.rs
pub mod builder;
pub mod handlers;

pub use builder::QueryApiBuilder;

// NEW: liquers-axum/src/store/mod.rs
pub mod builder;
pub mod handlers;

pub use builder::StoreApiBuilder;

// MODIFY: liquers-axum/src/lib.rs (replace entire contents)
pub mod api_core;
pub mod query;
pub mod store;

// Re-exports for convenience
pub use api_core::{ApiResponse, ErrorDetail, DataEntry, BinaryResponse, SerializationFormat};
pub use query::QueryApiBuilder;
pub use store::StoreApiBuilder;

// Re-export EnvRef from liquers-core
pub use liquers_core::context::EnvRef;
```

**Validation:**
```bash
cargo check -p liquers-axum
# Expected: Errors about missing modules (response.rs, error.rs, etc.) - expected
```

**Rollback:**
```bash
rm -rf liquers-axum/src/api_core liquers-axum/src/query liquers-axum/src/store
git checkout liquers-axum/src/lib.rs
# Restore deleted files if needed
git checkout liquers-axum/src/core_handlers.rs liquers-axum/src/store_handlers.rs liquers-axum/src/environment.rs liquers-axum/src/utils.rs liquers-axum/src/value.rs
```

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices
- **Knowledge:** Phase 2 architecture doc (module structure section), existing liquers-axum structure
- **Rationale:** New module architecture requires architectural judgment

---

### Step 3: Implement Core Response Types

**File:** `liquers-axum/src/api_core/response.rs`

**Action:**
- Define ApiResponse<T>, ResponseStatus, ErrorDetail, DataEntry, BinaryResponse
- Add serde derives and annotations
- Implement helper functions (ok_response, error_response)

**Code changes:**
```rust
// NEW: liquers-axum/src/api_core/response.rs
use serde::{Serialize, Deserialize};
use liquers_core::error::Error;
use liquers_core::metadata::Metadata;

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
    #[serde(with = "base64_serde")]
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
    use crate::core::error::error_to_detail;

    ApiResponse {
        status: ResponseStatus::Error,
        result: None,
        message: error.message.clone(),
        query: error.query.clone(),
        key: error.key.clone(),
        error: Some(error_to_detail(error)),
    }
}

// Base64 serde module for DataEntry.data field
// Note: Renamed from 'base64' to 'base64_serde' to avoid shadowing the base64 crate
mod base64_serde {
    use serde::{Serialize, Deserialize, Serializer, Deserializer};

    pub fn serialize<S>(bytes: &Vec<u8>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if serializer.is_human_readable() {
            // JSON: base64 encode
            use base64::Engine;
            let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
            serializer.serialize_str(&encoded)
        } else {
            // CBOR/bincode: raw bytes
            serializer.serialize_bytes(bytes)
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        if deserializer.is_human_readable() {
            // JSON: base64 decode
            use base64::Engine;
            let s = String::deserialize(deserializer)?;
            base64::engine::general_purpose::STANDARD
                .decode(s)
                .map_err(serde::de::Error::custom)
        } else {
            // CBOR/bincode: raw bytes
            Vec::<u8>::deserialize(deserializer)
        }
    }
}
```

**Validation:**
```bash
cargo check -p liquers-axum
# Expected: Compiles with no errors
```

**Rollback:**
```bash
rm liquers-axum/src/api_core/response.rs
```

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices
- **Knowledge:** Phase 2 architecture doc (data structures section), serde documentation
- **Rationale:** Complex serde annotations and custom base64 module require careful implementation

---

### Step 4: Implement Error Mapping

**File:** `liquers-axum/src/api_core/error.rs`

**Action:**
- Implement error_to_status_code() with explicit match for all 19 ErrorType variants
- Implement error_to_detail() to convert Error to ErrorDetail
- Implement parse_error_type() for reverse mapping

**Code changes:**
```rust
// NEW: liquers-axum/src/api_core/error.rs
use liquers_core::error::{Error, ErrorType};
use axum::http::StatusCode;
use crate::api_core::response::ErrorDetail;

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

/// Convert Error to ErrorDetail
pub fn error_to_detail(error: &Error) -> ErrorDetail {
    ErrorDetail {
        error_type: format!("{:?}", error.error_type),
        message: error.message.clone(),
        query: error.query.clone(),
        key: error.key.clone(),
        traceback: None,
        metadata: None,
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
```

**Validation:**
```bash
cargo check -p liquers-axum
# Expected: Compiles with no errors
```

**Rollback:**
```bash
rm liquers-axum/src/api_core/error.rs
```

**Agent Specification:**
- **Model:** haiku
- **Skills:** rust-best-practices
- **Knowledge:** Phase 2 architecture doc (error mapping section), liquers-core/src/error.rs
- **Rationale:** Straightforward match statements following established pattern

---

### Step 5: Implement Serialization Format Selection

**File:** `liquers-axum/src/api_core/format.rs`

**Action:**
- Define SerializationFormat enum
- Implement select_format() for query param/Accept header
- Implement serialize_data_entry() and deserialize_data_entry()

**Code changes:**
```rust
// NEW: liquers-axum/src/api_core/format.rs
use axum::http::HeaderMap;
use liquers_core::error::{Error, ErrorType};
use crate::api_core::response::DataEntry;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SerializationFormat {
    Cbor,      // application/cbor
    Bincode,   // application/x-bincode
    Json,      // application/json
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

    // Default to CBOR per spec
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
```

**Validation:**
```bash
cargo check -p liquers-axum
# Expected: Compiles with no errors
```

**Rollback:**
```bash
rm liquers-axum/src/api_core/format.rs
```

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices
- **Knowledge:** Phase 2 architecture doc (format selection section), ciborium/bincode/serde_json docs
- **Rationale:** Multiple serialization libraries require careful error handling

---

### Step 6: Implement Axum IntoResponse for Response Types

**File:** `liquers-axum/src/axum_integration.rs` (new)

**Action:**
- Implement IntoResponse for ApiResponse<T>, BinaryResponse, DataEntry
- Handle Content-Type headers appropriately

**Code changes:**
```rust
// NEW: liquers-axum/src/axum_integration.rs
use axum::{
    response::{IntoResponse, Response},
    http::{StatusCode, header},
    body::Body,
};
use serde::Serialize;

use crate::api_core::response::{ApiResponse, ResponseStatus, BinaryResponse, DataEntry};
use crate::api_core::error::{error_to_status_code, parse_error_type};

impl<T> IntoResponse for ApiResponse<T>
where
    T: Serialize,
{
    fn into_response(self) -> Response {
        let status = match self.status {
            ResponseStatus::Ok => StatusCode::OK,
            ResponseStatus::Error => {
                self.error.as_ref()
                    .and_then(|e| parse_error_type(&e.error_type))
                    .map(error_to_status_code)
                    .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
            }
        };

        let json = serde_json::to_string(&self)
            .unwrap_or_else(|e| format!(r#"{{"status":"ERROR","message":"Serialization failed: {}"}}"#, e));

        Response::builder()
            .status(status)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(json))
            .expect("Failed to build response")
    }
}

impl IntoResponse for BinaryResponse {
    fn into_response(self) -> Response {
        // NOTE: Metadata::get_media_type() returns String (not Option<String>)
        // It defaults to "application/octet-stream" internally
        let media_type = self.metadata.get_media_type();

        let mut builder = Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, media_type);

        // NOTE: Metadata::status() returns Status (not Option<Status>)
        let status = self.metadata.status();
        builder = builder.header("X-Liquers-Status", format!("{:?}", status));

        builder
            .body(Body::from(self.data))
            .expect("Failed to build response")
    }
}

impl IntoResponse for DataEntry {
    fn into_response(self) -> Response {
        // Default to CBOR
        let mut cbor_bytes = Vec::new();
        if let Err(e) = ciborium::ser::into_writer(&self, &mut cbor_bytes) {
            tracing::error!("CBOR serialization failed: {}", e);
            cbor_bytes.clear();
        }

        Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "application/cbor")
            .body(Body::from(cbor_bytes))
            .expect("Failed to build response")
    }
}

// MODIFY: liquers-axum/src/lib.rs - add axum_integration module
pub mod axum_integration;
```

**Validation:**
```bash
cargo check -p liquers-axum --features axum
# Expected: Compiles with no errors
```

**Rollback:**
```bash
rm liquers-axum/src/axum_integration.rs
git checkout liquers-axum/src/lib.rs
```

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices
- **Knowledge:** Phase 2 architecture doc (Axum integration section), Axum IntoResponse trait docs
- **Rationale:** Axum trait implementation requires framework-specific knowledge

---

### Step 7: Implement QueryApiBuilder

**File:** `liquers-axum/src/query/builder.rs` (new)

**Action:**
- Define QueryApiBuilder<E> struct with PhantomData
- Implement new() and build_axum() methods

**Code changes:**
```rust
// NEW: liquers-axum/src/query/builder.rs
use std::marker::PhantomData;
use liquers_core::context::Environment;

pub struct QueryApiBuilder<E>
where
    E: Environment,
{
    base_path: String,
    _phantom: PhantomData<E>,
}

impl<E> QueryApiBuilder<E>
where
    E: Environment + Send + Sync + 'static,
{
    pub fn new(base_path: impl Into<String>) -> Self {
        Self {
            base_path: base_path.into(),
            _phantom: PhantomData,
        }
    }

    #[cfg(feature = "axum")]
    pub fn build_axum(self) -> axum::Router {
        use axum::routing::{get, post};
        use crate::query::handlers::{get_query_handler, post_query_handler};

        axum::Router::new()
            .route(&format!("{}/*query", self.base_path), get(get_query_handler::<E>))
            .route(&format!("{}/*query", self.base_path), post(post_query_handler::<E>))
    }
}
```

**Validation:**
```bash
cargo check -p liquers-axum
# Expected: Error about missing handlers module - expected
```

**Rollback:**
```bash
rm liquers-axum/src/query/builder.rs
```

**Agent Specification:**
- **Model:** haiku
- **Skills:** rust-best-practices
- **Knowledge:** Phase 2 architecture doc (builder pattern section)
- **Rationale:** Builder pattern follows template from Phase 2

---

### Step 8: Implement Query API Handlers

**File:** `liquers-axum/src/query/handlers.rs` (new)

**Action:**
- Implement get_query_handler() and post_query_handler()
- Parse query, evaluate, poll asset, return response

**Code changes:**
```rust
// NEW: liquers-axum/src/query/handlers.rs
use axum::{
    extract::{Path, State},
    response::IntoResponse,
    body::Bytes,
};
use liquers_core::{
    context::{Environment, EnvRef},
    parse::parse_query,
    error::{Error, ErrorType},
};
use crate::api_core::response::{BinaryResponse, error_response};

/// GET /q/{*query} handler
pub async fn get_query_handler<E>(
    State(env): State<EnvRef<E>>,
    Path(query_str): Path<String>,
) -> impl IntoResponse
where
    E: Environment + Send + Sync + 'static,
{
    // Parse query
    let query = match parse_query(&query_str) {
        Ok(q) => q,
        Err(e) => return error_response::<()>(&e).into_response(),
    };

    // Evaluate query
    let asset_ref = match env.evaluate(&query).await {
        Ok(asset) => asset,
        Err(e) => return error_response::<()>(&e).into_response(),
    };

    // Poll asset to completion with 30-second timeout
    // NOTE: AssetRef has no wait() method. Use poll_binary() in a loop
    // or poll_state() to check for completion. The implementation must
    // poll until status is Ready or Error. See liquers-core/src/assets.rs
    // for AssetRef::poll_state() and AssetRef::poll_binary().
    //
    // Preferred approach: use poll_binary() which returns (Arc<Vec<u8>>, Arc<Metadata>)
    // directly, avoiding the need to serialize Value to bytes ourselves.
    let timeout = tokio::time::Duration::from_secs(30);
    let start = tokio::time::Instant::now();

    loop {
        // Check timeout
        if start.elapsed() > timeout {
            let error = Error::general_error("Query evaluation timeout (30s exceeded)".to_string());
            return error_response::<()>(&error).into_response();
        }
        if let Some((data_arc, metadata_arc)) = asset_ref.poll_binary().await {
            return BinaryResponse {
                data: (*data_arc).clone(),
                metadata: (*metadata_arc).clone(),
            }.into_response();
        }
        // Check for error status
        let status = asset_ref.status().await;
        match status {
            liquers_core::metadata::Status::Error => {
                let error = Error::general_error("Query evaluation failed".to_string());
                return error_response::<()>(&error).into_response();
            }
            liquers_core::metadata::Status::Ready => {
                // Ready but no binary - try poll_state and serialize
                if let Some(state) = asset_ref.poll_state().await {
                    let metadata = (*state.metadata).clone();
                    // Determine format from metadata (extension or media type)
                    let format = metadata.get_extension()
                        .unwrap_or_else(|| "json");  // Default to JSON if no extension
                    let data = match state.data.as_bytes(format) {
                        Ok(bytes) => bytes,
                        Err(e) => return error_response::<()>(&e).into_response(),
                    };
                    return BinaryResponse { data, metadata }.into_response();
                }
                let error = Error::general_error("Asset ready but no data available".to_string());
                return error_response::<()>(&error).into_response();
            }
            _ => {
                // Still processing, sleep briefly and retry
                tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
            }
        }
    }
}

/// POST /q/{*query} handler
pub async fn post_query_handler<E>(
    State(env): State<EnvRef<E>>,
    Path(query_str): Path<String>,
    body: Bytes,
) -> impl IntoResponse
where
    E: Environment + Send + Sync + 'static,
{
    use liquers_core::error::Error;

    // Parse JSON body for arguments
    let _args: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => return error_response::<()>(&Error::from_error(ErrorType::SerializationError, e)).into_response(),
    };

    // Parse query
    let query = match parse_query(&query_str) {
        Ok(q) => q,
        Err(e) => return error_response::<()>(&e).into_response(),
    };

    // TODO: Merge args into query (implementation detail for future work)

    // Evaluate query (same as GET for now)
    let asset_ref = match env.evaluate(&query).await {
        Ok(asset) => asset,
        Err(e) => return error_response::<()>(&e).into_response(),
    };

    // Same polling pattern as GET handler with 30s timeout
    let timeout = tokio::time::Duration::from_secs(30);
    let start = tokio::time::Instant::now();

    loop {
        if start.elapsed() > timeout {
            let error = Error::general_error("Query evaluation timeout (30s exceeded)".to_string());
            return error_response::<()>(&error).into_response();
        }
        if let Some((data_arc, metadata_arc)) = asset_ref.poll_binary().await {
            return BinaryResponse {
                data: (*data_arc).clone(),
                metadata: (*metadata_arc).clone(),
            }.into_response();
        }
        let status = asset_ref.status().await;
        match status {
            liquers_core::metadata::Status::Error => {
                let error = Error::general_error("Query evaluation failed".to_string());
                return error_response::<()>(&error).into_response();
            }
            liquers_core::metadata::Status::Ready => {
                if let Some(state) = asset_ref.poll_state().await {
                    let metadata = (*state.metadata).clone();
                    // Determine format from metadata
                    let format = metadata.get_extension()
                        .unwrap_or_else(|| "json");
                    let data = match state.data.as_bytes(format) {
                        Ok(bytes) => bytes,
                        Err(e) => return error_response::<()>(&e).into_response(),
                    };
                    return BinaryResponse { data, metadata }.into_response();
                }
                let error = Error::general_error("Asset ready but no data available".to_string());
                return error_response::<()>(&error).into_response();
            }
            _ => {
                tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
            }
        }
    }
}
```

**Validation:**
```bash
cargo check -p liquers-axum
# Expected: Compiles with no errors
```

**Rollback:**
```bash
rm liquers-axum/src/query/handlers.rs
```

**Agent Specification:**
- **Model:** sonnet
- **Skills:** rust-best-practices
- **Knowledge:** Phase 2 architecture doc (query handlers section), liquers-core Environment trait
- **Rationale:** Complex async handler logic with error handling requires judgment

---

(Due to length constraints, I'll continue with the remaining steps in a structured format)

### Steps 9-15: Store API Implementation

**Step 9:** StoreApiBuilder (haiku, follows builder pattern)
**Step 10:** Store data endpoints (sonnet, async store operations)
**Step 11:** Store metadata endpoints (haiku, similar to data)
**Step 12:** Store directory operations (haiku, follows pattern)
**Step 13:** Unified entry endpoints (sonnet, CBOR/bincode/JSON logic)
**Step 14:** Optional GET-based destructive operations (haiku, conditional routes)
**Step 15:** Upload endpoint with multipart (sonnet, Axum multipart handling)

### Steps 16-20: Testing

**Step 16:** Core unit tests - response/error/format (sonnet, liquers-unittest)
**Step 17:** Query API integration tests (sonnet, liquers-unittest)
**Step 18:** Store API integration tests (sonnet, liquers-unittest)
**Step 19:** Example applications (haiku, follows template)
**Step 20:** Final validation and documentation (sonnet)

## Testing Plan

### Unit Tests

**When to run:** After Step 16

**Files:**
- `liquers-axum/src/api_core/response.rs` (#[cfg(test)] module)
- `liquers-axum/src/api_core/error.rs` (#[cfg(test)] module)
- `liquers-axum/src/api_core/format.rs` (#[cfg(test)] module)

**Command:**
```bash
cargo test -p liquers-axum --lib
```

**Expected:** 106 tests pass (30 response + 38 error + 38 format)

### Integration Tests

**When to run:** After Steps 17-18

**Files:**
- `liquers-axum/tests/query_api_tests.rs`
- `liquers-axum/tests/store_api_tests.rs`

**Command:**
```bash
cargo test -p liquers-axum --test query_api_tests
cargo test -p liquers-axum --test store_api_tests
```

**Expected:** 58 integration tests pass

### Manual Validation

**When to run:** After Step 19

**Commands:**
```bash
# 1. Start example server
cargo run -p liquers-axum --example basic_server
# Expected: Server starts on localhost:3000

# 2. Test query endpoint
curl http://localhost:3000/liquer/q/text-Hello
# Expected: 200 OK, "Hello" response

# 3. Test store data endpoint
echo "test data" | curl -X POST --data-binary @- http://localhost:3000/liquer/api/store/data/test.txt
curl http://localhost:3000/liquer/api/store/data/test.txt
# Expected: "test data" response

# 4. Test unified entry with CBOR
curl -H "Accept: application/cbor" http://localhost:3000/liquer/api/store/entry/test.txt > entry.cbor
# Expected: Binary CBOR file created

# 5. Test unified entry with JSON
curl -H "Accept: application/json" http://localhost:3000/liquer/api/store/entry/test.txt
# Expected: JSON with base64-encoded data field
```

## Agent Assignment Summary

| Step | Model | Skills | Rationale |
|------|-------|--------|-----------|
| 1 | haiku | rust-best-practices | Cargo.toml updates (standard pattern) |
| 2 | sonnet | rust-best-practices | Module architecture (requires judgment) |
| 3 | sonnet | rust-best-practices | Complex serde with custom base64 module |
| 4 | haiku | rust-best-practices | Error mapping (match statements) |
| 5 | sonnet | rust-best-practices | Multiple serialization formats |
| 6 | sonnet | rust-best-practices | Axum trait implementation |
| 7 | haiku | rust-best-practices | Builder pattern (template) |
| 8 | sonnet | rust-best-practices | Async handler logic with errors |
| 9 | haiku | rust-best-practices | Builder pattern (template) |
| 10 | sonnet | rust-best-practices | Async store operations |
| 11 | haiku | rust-best-practices | Similar to step 10 |
| 12 | haiku | rust-best-practices | Follows established pattern |
| 13 | sonnet | rust-best-practices | Format selection logic |
| 14 | haiku | rust-best-practices | Conditional routes |
| 15 | sonnet | rust-best-practices | Multipart file handling |
| 16 | sonnet | rust-best-practices, liquers-unittest | Unit test patterns |
| 17 | sonnet | rust-best-practices, liquers-unittest | Integration testing |
| 18 | sonnet | rust-best-practices, liquers-unittest | Integration testing |
| 19 | haiku | — | Example apps (straightforward) |
| 20 | sonnet | rust-best-practices | Final validation requires judgment |

## Rollback Plan

### Per-Step Rollback
Each step includes specific rollback commands (see individual steps above)

### Full Feature Rollback
```bash
git checkout main
git branch -D feature/web-api-library

# Delete new files:
rm -rf liquers-axum/src/api_core
rm -rf liquers-axum/src/query
rm -rf liquers-axum/src/store
rm liquers-axum/src/axum_integration.rs
rm liquers-axum/tests/query_api_tests.rs
rm liquers-axum/tests/store_api_tests.rs

# Restore modified files:
git checkout liquers-axum/Cargo.toml
git checkout liquers-axum/src/lib.rs

# Restore deleted files:
git checkout liquers-axum/src/core_handlers.rs
git checkout liquers-axum/src/store_handlers.rs
git checkout liquers-axum/src/environment.rs
git checkout liquers-axum/src/utils.rs
git checkout liquers-axum/src/value.rs
```

## Documentation Updates

### CLAUDE.md

**Update:** Add web API builder pattern example

**Section:** ## Common Tasks > Adding a Web API

**Add:**
```markdown
### Building Custom Web APIs

The `liquers-axum` crate provides composable builders for HTTP REST APIs:

\`\`\`rust
use liquers_axum::{QueryApiBuilder, StoreApiBuilder};
use liquers_core::context::SimpleEnvironment;

#[tokio::main]
async fn main() {
    let env = SimpleEnvironment::new().await;
    let env_ref = env.to_ref();

    let app = QueryApiBuilder::new("/liquer/q")
        .build_axum()
        .merge(
            StoreApiBuilder::new("/liquer/api/store")
                .with_destructive_gets()  // Optional: enable GET-based DELETE/PUT
                .build_axum()
        )
        .with_state(env_ref);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
\`\`\`

**Note:** Module renamed from `core` to `api_core` to avoid shadowing Rust's built-in `core` crate.
```

### PROJECT_OVERVIEW.md

**No updates needed** - Web API layer doesn't change core concepts

### specs/web-api-library/DESIGN.md

**Update:** Mark phases as complete

**Change:**
```markdown
- [x] Phase 1: High-Level Design
- [x] Phase 2: Solution & Architecture
- [x] Phase 3: Examples & Testing
- [x] Phase 4: Implementation Plan
- [ ] Implementation Complete
```

## Execution Options

(Will be presented after user approval - see Phase 4 template)
