# Phase 2: Solution & Architecture - Web API Library

## Overview

The Web API library is implemented as a composable, framework-agnostic core with Axum-specific bindings. Builder pattern (`QueryApiBuilder<E>`, `StoreApiBuilder<E>`) creates routers generic over Environment type. Response types (ApiResponse, ErrorDetail, DataEntry, BinaryResponse) are framework-agnostic structs with Axum `IntoResponse` implementations. Error mapping translates `liquers_core::error::ErrorType` to HTTP status codes. Serialization supports CBOR (default), bincode, and JSON for unified entry endpoints.

## Data Structures

### Core Response Types

#### ApiResponse<T>

```rust
use serde::{Serialize, Deserialize};

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
```

**Ownership rationale:**
- All fields owned - response is moved into HTTP response body
- `T` is generic and owned by the response
- No borrowing needed (data consumed by serialization)

**Serialization:**
- Derives: `Serialize, Deserialize`
- `#[serde(skip_serializing_if = "Option::is_none")]` for optional fields (cleaner JSON)

#### ResponseStatus

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum ResponseStatus {
    Ok,
    Error,
}
```

**Copy trait:** Small enum (1 byte), implements Copy for efficient passing.

**No default match arm:** Only 2 variants - all match statements will be explicit.

#### ErrorDetail

```rust
/// Error detail structure (per spec section 3.2)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorDetail {
    #[serde(rename = "type")]
    pub error_type: String,  // ErrorType as string
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
```

**Ownership rationale:**
- All owned strings (moved into response)
- `error_type` field serialized as "type" via `#[serde(rename = "type")]`

#### DataEntry

```rust
use liquers_core::metadata::Metadata;

/// Unified data/metadata structure (per spec sections 4.1.13-14)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataEntry {
    pub metadata: Metadata,  // Metadata enum (MetadataRecord or LegacyMetadata)
    #[serde(with = "base64")]
    pub data: Vec<u8>,
}
```

**Ownership rationale:**
- `data: Vec<u8>` owned - can be large, but only one copy per request/response
- `metadata: Metadata` - enum wrapping MetadataRecord or LegacyMetadata (serde_json::Value)

**Serialization:**
- Base64 encoding for `data` field when serialized to JSON
- CBOR/bincode serialize `data` as raw bytes (no base64)
- Custom `base64` serde module for JSON format only

#### BinaryResponse

```rust
use liquers_core::metadata::Metadata;

/// Binary response with metadata in headers
pub struct BinaryResponse {
    pub data: Vec<u8>,
    pub metadata: Metadata,
}
```

**Ownership rationale:**
- `data: Vec<u8>` owned - moved into HTTP response body
- `metadata: Metadata` owned - used to set HTTP headers

**No serialization:** Directly converted to Axum Response (not serde).

#### SerializationFormat

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SerializationFormat {
    Cbor,      // application/cbor
    Bincode,   // application/x-bincode
    Json,      // application/json
}
```

**Copy trait:** Small enum (1 byte), passed by value.

**No default match arm:** Only 3 variants - all match statements explicit.

### Builder Structures

#### QueryApiBuilder<E>

```rust
use std::marker::PhantomData;
use liquers_core::context::Environment;

pub struct QueryApiBuilder<E>
where
    E: Environment,
{
    base_path: String,
    _phantom: PhantomData<E>,
}
```

**Generic bound rationale:**
- `E: Environment` - minimal bound, exactly what's needed
- `PhantomData<E>` - zero-sized marker for generic type (E is not stored)
- Builder pattern: configuration only, no state

#### StoreApiBuilder<E>

```rust
pub struct StoreApiBuilder<E>
where
    E: Environment,
{
    base_path: String,
    allow_destructive_gets: bool,  // Opt-in per Phase 1 decision
    _phantom: PhantomData<E>,
}
```

**Generic bound rationale:**
- Same as QueryApiBuilder
- `allow_destructive_gets` enables GET-based DELETE/PUT operations (Phase 1 decision: opt-in)

### Axum State Type

```rust
use liquers_core::context::EnvRef;

// EnvRef is already defined in liquers-core as:
// pub struct EnvRef<E: Environment>(pub Arc<E>);
```

**Ownership rationale:**
- `EnvRef<E>` is a struct wrapping `Arc<E>` (defined in liquers-core)
- Provides shared ownership across handler tasks
- Environment is immutable (or internally synchronized if mutable)
- Cheap cloning for each request

## Trait Implementations

### Axum IntoResponse

**Implementor:** `ApiResponse<T>`

```rust
use axum::{
    response::{IntoResponse, Response},
    http::{StatusCode, header},
    body::Body,
};

impl<T> IntoResponse for ApiResponse<T>
where
    T: Serialize,
{
    fn into_response(self) -> Response {
        let status = match self.status {
            ResponseStatus::Ok => StatusCode::OK,
            ResponseStatus::Error => {
                // Extract status from error detail
                self.error.as_ref()
                    .and_then(|e| parse_error_type(&e.error_type))
                    .map(error_to_status_code)
                    .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
            }
        };

        // Serialize to JSON (always JSON for ApiResponse)
        let json = serde_json::to_string(&self)
            .unwrap_or_else(|e| format!(r#"{{"status":"ERROR","message":"Serialization failed: {}"}}"#, e));

        Response::builder()
            .status(status)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(json))
            .unwrap()  // Safe: header names are static
    }
}
```

**Bounds:** `T: Serialize` required for JSON serialization.

**Note:** `unwrap()` on `Response::builder()` is safe - headers are static strings.

**Implementor:** `BinaryResponse`

```rust
impl IntoResponse for BinaryResponse {
    fn into_response(self) -> Response {
        let media_type = self.metadata.get_media_type()
            .unwrap_or_else(|| "application/octet-stream".to_string());

        let mut builder = Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, media_type);

        // Add metadata as headers
        if let Some(status) = self.metadata.status() {
            builder = builder.header("X-Liquers-Status", format!("{:?}", status));
        }

        builder
            .body(Body::from(self.data))
            .unwrap()  // Safe: header names are static
    }
}
```

**Implementor:** `DataEntry`

```rust
impl IntoResponse for DataEntry {
    fn into_response(self) -> Response {
        // Default to CBOR (determined by Accept header in handler)
        // This implementation is for direct DataEntry responses
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
```

### No New Trait Definitions

All traits used (`IntoResponse`, `Environment`, `AsyncStore`) are existing traits from Axum and liquers-core.

## Generic Parameters & Bounds

### Generic Builder: QueryApiBuilder<E>

```rust
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
    pub fn build_axum(self) -> axum::Router<EnvRef<E>> {
        // Build router (see Integration Points section)
    }
}
```

**Bound justification:**
- `E: Environment` - builder creates routes that use Environment trait methods
- `Send + Sync` - Axum handlers are async and multi-threaded
- `'static` - Handlers are spawned as tasks, must outlive any references

### Generic Builder: StoreApiBuilder<E>

```rust
impl<E> StoreApiBuilder<E>
where
    E: Environment + Send + Sync + 'static,
{
    pub fn new(base_path: impl Into<String>) -> Self {
        Self {
            base_path: base_path.into(),
            allow_destructive_gets: false,  // Opt-in per Phase 1
            _phantom: PhantomData,
        }
    }

    pub fn with_destructive_gets(mut self) -> Self {
        self.allow_destructive_gets = true;
        self
    }

    #[cfg(feature = "axum")]
    pub fn build_axum(self) -> axum::Router<EnvRef<E>> {
        // Build router (see Integration Points section)
    }
}
```

**Same bounds as QueryApiBuilder.**

## Sync vs Async Decisions

| Function | Async? | Rationale |
|----------|--------|-----------|
| All Axum handlers | Yes | Axum handlers are async, call async Environment methods |
| `error_to_status_code` | No | Pure mapping function, no I/O |
| `error_to_detail` | No | Pure conversion function, no I/O |
| `error_response` | No | Constructs ApiResponse, no I/O |
| `parse_format_param` | No | Parses query parameter, no I/O |
| `serialize_data_entry` | No | CPU-bound serialization (CBOR/bincode/JSON) |
| `deserialize_data_entry` | No | CPU-bound deserialization |
| Builder methods (`new`, `with_*`) | No | Configuration only, no I/O |
| `build_axum` | No | Constructs router, no I/O |

**Pattern:** Only Axum handlers are async (they call async Environment methods internally).

## Function Signatures

### Module: liquers_axum::core::response

```rust
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
        query: error.query.clone(),  // Option<String> - already encoded
        key: error.key.clone(),  // Option<String> - already encoded
        traceback: None,  // Error struct has no detail/traceback field
        metadata: None,
    }
}
```

**Parameter choices:**
- `error: &Error` - borrowed to avoid cloning Error (can be large)
- Return `ApiResponse<T>` - owned, will be moved into HTTP response

### Module: liquers_axum::core::error

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
    // Match against Debug format strings
    match s {
        "KeyNotFound" => Some(ErrorType::KeyNotFound),
        "KeyNotSupported" => Some(ErrorType::KeyNotSupported),
        // ... (all variants)
        _ => None,
    }
}
```

**No default match arm:** All ErrorType variants explicitly handled.

### Module: liquers_axum::core::format

```rust
use axum::http::HeaderMap;

/// Select serialization format from Accept header or query param
pub fn select_format(
    headers: &HeaderMap,
    format_param: Option<&str>,
) -> SerializationFormat {
    // Query param takes precedence
    if let Some(format) = format_param {
        return parse_format_param(format)
            .unwrap_or(SerializationFormat::Cbor);  // Default to CBOR
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

**No default match arm:** All SerializationFormat variants explicitly handled.

### Module: liquers_axum::query::handlers

```rust
use axum::{
    extract::{Path, State},
    http::StatusCode,
};
use liquers_core::context::Environment;

/// GET /q/{*query} handler
pub async fn get_query_handler<E>(
    State(env): State<EnvRef<E>>,
    Path(query_str): Path<String>,
) -> impl IntoResponse
where
    E: Environment + Send + Sync + 'static,
{
    use liquers_core::parse::parse_query;

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

    // Poll asset to completion
    match asset_ref.wait().await {
        Ok(state) => {
            // Convert State<Value> to BinaryResponse or JSON based on metadata
            let metadata = state.metadata.as_ref();
            let data = state.data.as_ref();

            // Serialize data using metadata format hint
            // Return BinaryResponse with appropriate Content-Type
            BinaryResponse {
                data: serialize_value(data).unwrap_or_default(),
                metadata: (**metadata).clone(),
            }.into_response()
        }
        Err(e) => error_response::<()>(&e).into_response(),
    }
}

/// POST /q/{*query} handler
pub async fn post_query_handler<E>(
    State(env): State<EnvRef<E>>,
    Path(query_str): Path<String>,
    body: axum::body::Bytes,
) -> impl IntoResponse
where
    E: Environment + Send + Sync + 'static,
{
    use liquers_core::parse::parse_query;

    // Parse JSON body for arguments
    let args: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => return error_response::<()>(&Error::from_error(ErrorType::SerializationError, e)).into_response(),
    };

    // Merge args into query (implementation detail)
    // Then same logic as GET handler
    // ... (similar to get_query_handler)
}
```

**Parameter choices:**
- `State(env): State<EnvRef<E>>` - Axum state extractor for Arc<E>
- `Path(query_str): Path<String>` - Axum path extractor for catch-all path
- Return `impl IntoResponse` - allows returning different response types

**Bounds:** `E: Environment + Send + Sync + 'static` required for Axum handlers.

### Module: liquers_axum::store::handlers

```rust
/// GET /api/store/data/{*key} handler
pub async fn get_data_handler<E>(
    State(env): State<EnvRef<E>>,
    Path(key_str): Path<String>,
) -> impl IntoResponse
where
    E: Environment + Send + Sync + 'static,
{
    use liquers_core::parse::parse_key;

    let key = match parse_key(&key_str) {
        Ok(k) => k,
        Err(e) => return error_response::<()>(&e).into_response(),
    };

    let store = env.get_async_store();

    // AsyncStore::get returns (Vec<u8>, Metadata) tuple
    match store.get(&key).await {
        Ok((data, metadata)) => BinaryResponse {
            data,
            metadata,
        }.into_response(),
        Err(e) => error_response::<()>(&e).into_response(),
    }
}

/// POST /api/store/data/{*key} handler
pub async fn post_data_handler<E>(
    State(env): State<EnvRef<E>>,
    Path(key_str): Path<String>,
    body: axum::body::Bytes,
) -> impl IntoResponse
where
    E: Environment + Send + Sync + 'static,
{
    use liquers_core::parse::parse_key;

    let key = match parse_key(&key_str) {
        Ok(k) => k,
        Err(e) => return error_response::<()>(&e).into_response(),
    };

    let store = env.get_async_store();

    // AsyncStore::set requires metadata parameter
    let metadata = Metadata::new();
    match store.set(&key, &body, &metadata).await {
        Ok(()) => ok_response(()).into_response(),
        Err(e) => error_response::<()>(&e).into_response(),
    }
}

/// GET /api/store/metadata/{*key} handler
pub async fn get_metadata_handler<E>(
    State(env): State<EnvRef<E>>,
    Path(key_str): Path<String>,
) -> impl IntoResponse
where
    E: Environment + Send + Sync + 'static,
{
    use liquers_core::parse::parse_key;

    let key = match parse_key(&key_str) {
        Ok(k) => k,
        Err(e) => return error_response::<()>(&e).into_response(),
    };

    let store = env.get_async_store();

    match store.get_metadata(&key).await {
        Ok(metadata) => {
            // Metadata is enum - serialize to JSON for API response
            let metadata_json = metadata.to_json()
                .unwrap_or_else(|_| "null".to_string());
            ok_response(metadata_json).into_response()
        }
        Err(e) => error_response::<String>(&e).into_response(),
    }
}

/// GET /api/store/entry/{*key} handler (CBOR/bincode/JSON)
pub async fn get_entry_handler<E>(
    State(env): State<EnvRef<E>>,
    Path(key_str): Path<String>,
    headers: HeaderMap,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse
where
    E: Environment + Send + Sync + 'static,
{
    use liquers_core::parse::parse_key;

    let key = match parse_key(&key_str) {
        Ok(k) => k,
        Err(e) => return error_response::<()>(&e).into_response(),
    };

    let format = select_format(&headers, params.get("format").map(|s| s.as_str()));

    let store = env.get_async_store();

    // AsyncStore::get returns (Vec<u8>, Metadata) tuple in one call
    let (data, metadata) = match store.get(&key).await {
        Ok(tuple) => tuple,
        Err(e) => return error_response::<()>(&e).into_response(),
    };

    // Create DataEntry
    let entry = DataEntry {
        metadata,  // Metadata enum - directly use it
        data,
    };

    // Serialize with selected format
    let bytes = match serialize_data_entry(&entry, format) {
        Ok(b) => b,
        Err(e) => return error_response::<()>(&e).into_response(),
    };

    // Return with appropriate Content-Type
    let content_type = match format {
        SerializationFormat::Cbor => "application/cbor",
        SerializationFormat::Bincode => "application/x-bincode",
        SerializationFormat::Json => "application/json",
    };

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .body(Body::from(bytes))
        .unwrap()
        .into_response()
}

// Similar signatures for:
// - post_entry_handler (POST /entry)
// - delete_entry_handler (DELETE /entry)
// - listdir_handler (GET /listdir)
// - is_dir_handler (GET /is_dir)
// - contains_handler (GET /contains)
// - keys_handler (GET /keys)
// - makedir_handler (PUT /makedir)
// - removedir_handler (DELETE /removedir)
// - upload_handler (POST /upload with Multipart)
```

**All handlers follow same pattern:**
- Extract `State<EnvRef<E>>` for environment
- Extract `Path` for key/query
- Call async environment/store methods
- Return `impl IntoResponse` (ApiResponse or BinaryResponse)

## Integration Points

### Crate: liquers-axum

**Module structure:**
```
liquers-axum/src/
├── lib.rs                    # Public exports
├── core/
│   ├── mod.rs
│   ├── response.rs           # ApiResponse, ErrorDetail, DataEntry, BinaryResponse
│   ├── error.rs              # error_to_status_code, error_to_detail
│   └── format.rs             # SerializationFormat, select_format, serialize/deserialize
├── query/
│   ├── mod.rs
│   ├── builder.rs            # QueryApiBuilder<E>
│   └── handlers.rs           # get_query_handler, post_query_handler
├── store/
│   ├── mod.rs
│   ├── builder.rs            # StoreApiBuilder<E>
│   └── handlers.rs           # All store endpoint handlers
└── main.rs                   # Example standalone application
```

**Public exports in lib.rs:**
```rust
pub mod core;
pub mod query;
pub mod store;

// Re-exports for convenience
pub use core::{ApiResponse, ErrorDetail, DataEntry, BinaryResponse, SerializationFormat};
pub use query::QueryApiBuilder;
pub use store::StoreApiBuilder;

// Re-export EnvRef from liquers-core (struct wrapper around Arc)
pub use liquers_core::context::EnvRef;
```

### Builder Implementation

**File:** `liquers-axum/src/query/builder.rs`

```rust
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
    pub fn build_axum(self) -> axum::Router<EnvRef<E>> {
        use axum::routing::{get, post};
        use crate::query::handlers::{get_query_handler, post_query_handler};

        axum::Router::new()
            .route(&format!("{}/*query", self.base_path), get(get_query_handler::<E>))
            .route(&format!("{}/*query", self.base_path), post(post_query_handler::<E>))
    }
}
```

**File:** `liquers-axum/src/store/builder.rs`

```rust
impl<E> StoreApiBuilder<E>
where
    E: Environment + Send + Sync + 'static,
{
    pub fn new(base_path: impl Into<String>) -> Self {
        Self {
            base_path: base_path.into(),
            allow_destructive_gets: false,
            _phantom: PhantomData,
        }
    }

    pub fn with_destructive_gets(mut self) -> Self {
        self.allow_destructive_gets = true;
        self
    }

    #[cfg(feature = "axum")]
    pub fn build_axum(self) -> axum::Router<EnvRef<E>> {
        use axum::routing::{get, post, delete, put};
        use crate::store::handlers::*;

        let mut router = axum::Router::new()
            // Data endpoints
            .route(&format!("{}/data/*key", self.base_path), get(get_data_handler::<E>))
            .route(&format!("{}/data/*key", self.base_path), post(post_data_handler::<E>))
            .route(&format!("{}/data/*key", self.base_path), delete(delete_data_handler::<E>))
            // Metadata endpoints
            .route(&format!("{}/metadata/*key", self.base_path), get(get_metadata_handler::<E>))
            .route(&format!("{}/metadata/*key", self.base_path), post(post_metadata_handler::<E>))
            // Unified entry endpoints (CBOR/bincode/JSON)
            .route(&format!("{}/entry/*key", self.base_path), get(get_entry_handler::<E>))
            .route(&format!("{}/entry/*key", self.base_path), post(post_entry_handler::<E>))
            .route(&format!("{}/entry/*key", self.base_path), delete(delete_entry_handler::<E>))
            // Directory operations
            .route(&format!("{}/listdir/*key", self.base_path), get(listdir_handler::<E>))
            .route(&format!("{}/is_dir/*key", self.base_path), get(is_dir_handler::<E>))
            .route(&format!("{}/contains/*key", self.base_path), get(contains_handler::<E>))
            .route(&format!("{}/keys", self.base_path), get(keys_handler::<E>))
            .route(&format!("{}/makedir/*key", self.base_path), put(makedir_handler::<E>))
            .route(&format!("{}/removedir/*key", self.base_path), delete(removedir_handler::<E>))
            // Upload
            .route(&format!("{}/upload/*key", self.base_path), post(upload_handler::<E>));

        // Optional GET-based destructive operations (opt-in per Phase 1)
        if self.allow_destructive_gets {
            router = router
                .route(&format!("{}/remove/*key", self.base_path), get(get_remove_handler::<E>))
                .route(&format!("{}/removedir/*key", self.base_path), get(get_removedir_handler::<E>))
                .route(&format!("{}/makedir/*key", self.base_path), get(get_makedir_handler::<E>));
        }

        router
    }
}
```

### Dependencies

**Add to `liquers-axum/Cargo.toml`:**
```toml
[dependencies]
# Core liquers crates
liquers-core = { path = "../liquers-core" }
liquers-store = { path = "../liquers-store" }

# Web framework
axum = { version = "0.8", features = ["macros", "multipart"] }
tokio = { version = "1", features = ["full"] }
tower = { version = "0.5" }
tower-http = { version = "0.6", features = ["cors", "trace"] }

# Serialization
serde = { version = "1", features = ["derive"] }
serde_json = "1"
ciborium = "0.2"      # CBOR support
bincode = "1.3"       # Bincode support
base64 = "0.22"       # Base64 for JSON data encoding

# Error handling (no anyhow - library only)
thiserror = "2"

# Logging
tracing = "0.1"

# Utilities
bytes = "1"

[features]
default = ["axum"]
axum = ["dep:axum"]  # Allow building without Axum for future framework support
```

**Version rationale:**
- axum 0.8 - latest stable
- tokio 1 - required by Axum
- serde 1 - standard serialization
- ciborium 0.2, bincode 1.3 - mature serialization libraries

## Relevant Commands

### New Commands

**None.** This feature is a web API layer only, no new Liquers commands.

### Relevant Existing Namespaces

**None directly used.** The web API exposes:
- `/q` endpoints - execute ANY query (all command namespaces)
- `/api/store` endpoints - store operations (no commands involved)

The web API is namespace-agnostic and works with all existing command namespaces registered in the Environment.

## Web Endpoints

### Query Execution API

#### GET /q/{*query}

**Route:** `GET {base_path}/q/{*query}`

**Handler:** `get_query_handler<E>`

**Behavior:**
1. Parse query from path segment
2. Call `env.evaluate(&query).await`
3. Poll AssetRef to completion
4. Return binary response with Content-Type from metadata or JSON response

**Example:**
```
GET /liquer/q/text-hello
→ Response: "hello" (text/plain)

GET /liquer/q/-R/data.csv~polars/from_csv
→ Response: CSV data (application/csv)
```

#### POST /q/{*query}

**Route:** `POST {base_path}/q/{*query}`

**Handler:** `post_query_handler<E>`

**Behavior:**
1. Parse query from path segment
2. Parse JSON body for additional arguments
3. Merge arguments into query
4. Same evaluation as GET

**Example:**
```
POST /liquer/q/text-hello
Body: {"greeting": "Hi"}
→ Evaluate with merged arguments
```

### Store API

#### Data Endpoints

- `GET {base_path}/api/store/data/{*key}` - get_data_handler
- `POST {base_path}/api/store/data/{*key}` - post_data_handler
- `DELETE {base_path}/api/store/data/{*key}` - delete_data_handler

#### Metadata Endpoints

- `GET {base_path}/api/store/metadata/{*key}` - get_metadata_handler
- `POST {base_path}/api/store/metadata/{*key}` - post_metadata_handler

#### Unified Entry Endpoints (CBOR/bincode/JSON)

- `GET {base_path}/api/store/entry/{*key}?format=cbor` - get_entry_handler
- `POST {base_path}/api/store/entry/{*key}?format=json` - post_entry_handler
- `DELETE {base_path}/api/store/entry/{*key}` - delete_entry_handler

**Format selection (per-endpoint, Phase 1 decision):**
1. `?format=cbor|bincode|json` query parameter (if present)
2. Accept header (if present)
3. Default to CBOR (per spec)

#### Directory Operations

- `GET {base_path}/api/store/listdir/{*key}` - listdir_handler
- `GET {base_path}/api/store/is_dir/{*key}` - is_dir_handler
- `GET {base_path}/api/store/contains/{*key}` - contains_handler
- `GET {base_path}/api/store/keys?prefix=...` - keys_handler
- `PUT {base_path}/api/store/makedir/{*key}` - makedir_handler
- `DELETE {base_path}/api/store/removedir/{*key}` - removedir_handler

#### Upload

- `POST {base_path}/api/store/upload/{*key}` - upload_handler (multipart)

#### Optional GET-based Destructive Operations (opt-in)

**Only when `.with_destructive_gets()` is enabled:**
- `GET {base_path}/api/store/remove/{*key}` - get_remove_handler
- `GET {base_path}/api/store/removedir/{*key}` - get_removedir_handler
- `GET {base_path}/api/store/makedir/{*key}` - get_makedir_handler

**Default:** Disabled (opt-in per Phase 1 decision for security).

## Error Handling

### No New Error Types

**Using existing `liquers_core::error::Error` only.**

**Note:** Error struct fields are `error_type`, `message`, `position`, `query`, `key`, `command_key`. There is NO `detail` field.

### Error Constructors

All handlers use existing error constructors:

```rust
// Parsing errors (specific constructors)
Error::query_parse_error(&query_str, "Invalid syntax", &Position::unknown())
Error::key_parse_error(&key_str, "Invalid format", &Position::unknown())

// Not found errors
Error::key_not_found(&key)

// General errors
Error::general_error("message".to_string())
Error::not_supported("message".to_string())

// Wrapping external errors
Error::from_error(ErrorType::SerializationError, ciborium_error)
Error::from_error(ErrorType::General, axum_error)
```

**No `Error::new` usage** - all use typed constructors per liquers patterns.

### Error Scenarios

| Scenario | ErrorType | HTTP Status | Example |
|----------|-----------|-------------|---------|
| Query parse failure | `ParseError` | 400 Bad Request | Invalid query syntax |
| Key not found in store | `KeyNotFound` | 404 Not Found | Store key doesn't exist |
| CBOR serialization fails | `SerializationError` | 422 Unprocessable | Invalid CBOR data |
| Store I/O error | `KeyReadError` | 500 Internal Server | Disk read failure |
| Unknown command | `UnknownCommand` | 400 Bad Request | Command not registered |
| Parameter validation fails | `ParameterError` | 400 Bad Request | Invalid parameter type |
| Missing argument | `ArgumentMissing` | 400 Bad Request | Required parameter missing |
| Action not registered | `ActionNotRegistered` | 400 Bad Request | Action name unknown |
| Command already exists | `CommandAlreadyRegistered` | 409 Conflict | Duplicate registration |
| Too many parameters | `TooManyParameters` | 400 Bad Request | Excess arguments provided |
| Format not supported | `NotSupported` | 501 Not Implemented | Unknown serialization format |
| Data not available | `NotAvailable` | 404 Not Found | Resource temporarily unavailable |

**Mapping:** `error_to_status_code()` function maps all ErrorType variants explicitly (no default match arm).

## Serialization Strategy

### Serde Annotations

**ApiResponse<T>:**
```rust
#[derive(Serialize, Deserialize)]
pub struct ApiResponse<T: Serialize> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<T>,
    // ... (cleaner JSON, omit None fields)
}
```

**DataEntry:**
```rust
#[derive(Serialize, Deserialize)]
pub struct DataEntry {
    pub metadata: serde_json::Value,

    #[serde(with = "base64")]  // Only for JSON format
    pub data: Vec<u8>,
}
```

**Base64 module (custom serde):**
```rust
mod base64 {
    use serde::{Serialize, Deserialize, Serializer, Deserializer};

    pub fn serialize<S>(bytes: &Vec<u8>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Only base64 encode for JSON (detect via serializer format)
        // For CBOR/bincode, serialize as raw bytes
        if serializer.is_human_readable() {
            // JSON: base64 encode
            let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, bytes);
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
            let s = String::deserialize(deserializer)?;
            base64::Engine::decode(&base64::engine::general_purpose::STANDARD, s)
                .map_err(serde::de::Error::custom)
        } else {
            // CBOR/bincode: raw bytes
            Vec::<u8>::deserialize(deserializer)
        }
    }
}
```

### Round-trip Compatibility

**Test plan (Phase 3):**
- Serialize DataEntry → CBOR → Deserialize → Compare
- Serialize DataEntry → Bincode → Deserialize → Compare
- Serialize DataEntry → JSON → Deserialize → Compare
- Verify base64 encoding only in JSON, not CBOR/bincode

## Concurrency Considerations

### Thread Safety

**Environment sharing:**
- `EnvRef<E>` (struct wrapping `Arc<E>`) shared across Axum handler tasks
- Environment must be `Send + Sync + 'static`
- Environment methods are `&self` (immutable) or internally synchronized

**No additional locks needed:**
- Environment is either immutable or uses internal synchronization
- Each request is an independent async task
- No shared mutable state in handlers

**Axum state pattern:**
```rust
use liquers_core::context::{SimpleEnvironment, EnvRef};

// In main.rs or application code
let env = SimpleEnvironment::new();
let env_ref = env.to_ref();  // Creates EnvRef<SimpleEnvironment> wrapping Arc

let app = QueryApiBuilder::new("/liquer/q")
    .build_axum()
    .merge(
        StoreApiBuilder::new("/liquer/api/store")
            .build_axum()
    )
    .with_state(env_ref);  // EnvRef<E> shared across all handlers

axum::Server::bind(&addr)
    .serve(app.into_make_service())
    .await?;
```

**Send + Sync requirements:**
- All handler functions require `E: Send + Sync + 'static`
- Error types must be Send (for async propagation)
- Response types must be Send (moved into response tasks)

## Compilation Validation

**Expected to compile:** Yes (after dependencies added)

**Validation steps:**
1. `cargo check -p liquers-axum` - check core types
2. `cargo check -p liquers-axum --features axum` - check Axum integration
3. `cargo test -p liquers-axum` - compile tests

**Potential issues:**
- Axum version compatibility - ensure 0.8.x
- Send + Sync bounds - ensure all generic parameters have these bounds
- base64 serde module - test with all three formats

## References to liquers-patterns.md

- [x] **Crate dependencies**: liquers-axum depends on liquers-core + liquers-store (correct flow)
- [x] **No liquers-lib dependency**: Works with core Value type only
- [x] **No new commands**: Web API layer only
- [x] **Error handling**: Uses `Error::from_error()` and typed constructors (not `Error::new`)
- [x] **Async default**: All Axum handlers async
- [x] **No unwrap/expect**: All fallible operations return Result
- [x] **No default match arms**: All match statements on ErrorType and SerializationFormat explicit
- [x] **Send + Sync bounds**: All generic parameters include these for async/threading
- [x] **Arc for sharing**: EnvRef<E> = Arc<E> for sharing Environment across handlers
- [x] **No anyhow**: Library code uses only liquers_core::error::Error
