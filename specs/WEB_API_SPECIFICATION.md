# Liquers Web API Specification

**Version:** 1.0.0
**Date:** 2026-01-19
**Status:** Draft

## Table of Contents

1. [Overview](#1-overview)
2. [Design Principles](#2-design-principles)
3. [Error Handling](#3-error-handling)
4. [Store API Module](#4-store-api-module)
5. [Assets API Module](#5-assets-api-module)
6. [Recipes API Module](#6-recipes-api-module)
7. [Query Execution API](#7-query-execution-api)
8. [Generic Library Design](#8-generic-library-design)
9. [Authentication & Authorization](#9-authentication--authorization)
10. [Axum Implementation](#10-axum-implementation)

---

## 1. Overview

The Liquers Web API provides HTTP and WebSocket interfaces to liquers services, enabling query execution, data storage, asset management, and recipe handling. The API is designed as an embeddable library that can be integrated into existing Rust applications.

### 1.1 Core Services

- **Store API** (`/api/store`): Persistent data storage with directory structure support
- **Assets API** (`/api/assets`): Asset lifecycle management with real-time notifications
- **Recipes API** (`/api/recipes`): Recipe definition and resolution
- **Query Execution** (`/q`): Synchronous query evaluation

### 1.2 Key Features

- **Modular Architecture**: Each service module can be used independently
- **Generic Environment**: Support for custom `Environment`, `Value`, and `Payload` types
- **First-Class Support**: Built-in optimizations for `liquers-lib` types
- **Framework Agnostic**: Core logic separates from web framework specifics
- **Real-time Updates**: WebSocket support for asset notifications

---

## 2. Design Principles

### 2.1 API Structure

All API endpoints are prefixed with a configurable base path (default: `/liquer`):

```
/liquer/
├── q/                    # Query execution
├── api/
│   ├── store/           # Store operations
│   ├── assets/          # Asset management
│   └── recipes/         # Recipe operations
└── ws/
    └── assets/          # Asset notification WebSocket
```

### 2.2 Path-Based Resource Identification

Resources are identified using path segments rather than query strings:

```
✅ GET /liquer/api/store/data/path/to/resource
❌ GET /liquer/api/store/data?path=path/to/resource
```

### 2.3 Content Negotiation

Response format is determined by:
1. **Accept header** (if specified)
2. **Metadata** (primary method, unless overriden by accept header) - see methods get_data_format and get_media_type.

Optionally (though it should not be necessary if metadata are available) the following methods may be used as fallback:
3. **File extension** in the path (e.g., `.json`, `.yaml`)
4. **Value type** (default media type for the value) - see `ValueInterface`.

### 2.4 Data and Metadata Access

Each resource type provides separate endpoints for data and metadata:

```
GET /liquer/api/store/data/{*key}      # Retrieve data only
GET /liquer/api/store/metadata/{*key}  # Retrieve metadata only
```

For efficient combined access (optimized for remote implementations), use unified entry endpoints:

```
GET /liquer/api/store/entry/{*key}      # Retrieve both data and metadata
POST /liquer/api/store/entry/{*key}     # Set both data and metadata
```

The entry endpoints use a common `DataEntry` structure (see sections 4.1.13 and 4.1.14) containing only `metadata` and `data` fields. GET requests return standard response format with `result` containing `DataEntry`. POST requests accept `DataEntry` directly in the body. Multiple serialization formats are supported: CBOR (default), bincode, and JSON. When using JSON format, binary data is base64-encoded for efficiency.

---

## 3. Error Handling

### 3.1 HTTP Status Codes

The API uses standard HTTP status codes to indicate error categories:

| Status Code | Usage |
|------------|-------|
| `200 OK` | Successful operation |
| `201 Created` | Resource created successfully |
| `204 No Content` | Successful operation with no response body |
| `400 Bad Request` | Invalid request syntax or parameters |
| `404 Not Found` | Resource not found |
| `409 Conflict` | Resource already exists or conflict with current state |
| `422 Unprocessable Entity` | Request syntax valid but semantically incorrect |
| `500 Internal Server Error` | Server-side error during processing |
| `503 Service Unavailable` | Service temporarily unavailable |

### 3.2 Error Response Format

All error responses include a JSON body with detailed error information:

```json
{
  "status": "ERROR",
  "error": {
    "type": "KeyNotFound",
    "message": "Key 'path/to/resource' not found in store",
    "query": "-R/path/to/resource",
    "key": "path/to/resource",
    "traceback": ["at line 42 in module foo", "..."],
    "metadata": {
      "store_type": "FileStore",
      "attempted_path": "/var/data/path/to/resource"
    }
  }
}
```

#### 3.2.1 Error Response Fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `status` | string | Yes | Always `"ERROR"` for error responses |
| `error.type` | string | Yes | Error type from `liquers_core::error::ErrorType` |
| `error.message` | string | Yes | Human-readable error description |
| `error.query` | string | No | The query that caused the error |
| `error.key` | string | No | The key that was being accessed |
| `error.traceback` | array | No | Stack trace for debugging |
| `error.metadata` | object | No | Additional context-specific information |

### 3.3 Error Type Mapping

Mapping from `liquers_core::error::ErrorType` to HTTP status codes:

| ErrorType | HTTP Status | Description |
|-----------|-------------|-------------|
| `General` | 500 | Generic server error |
| `KeyNotFound` | 404 | Resource not found in store |
| `KeyAlreadyExists` | 409 | Resource already exists |
| `ParseError` | 400 | Query parsing failed |
| `CommandNotFound` | 404 | Command not registered |
| `ArgumentError` | 400 | Invalid command arguments |
| `TypeError` | 422 | Type conversion or mismatch |
| `StoreError` | 500 | Store operation failed |
| `AssetError` | 500 | Asset management error |
| `RecipeError` | 422 | Recipe resolution error |
| `ExecutionError` | 500 | Command execution error |
| `NotImplemented` | 501 | Feature not implemented |
| `PermissionDenied` | 403 | Authorization failed |
| `Timeout` | 504 | Operation timeout |

A maintenance reminder: Keep the error mapping table (section 3.3) synchronized with the `ErrorType` enum in `liquers_core::error`. Any new error types added to the core library must be added to this mapping table with appropriate HTTP status codes. Implementation should include automated tests to catch mismatches.

### 3.4 Success Response Format

Successful operations return a JSON response with consistent structure:

```json
{
  "status": "OK",
  "result": { /* operation-specific data */ },
  "message": "Operation completed successfully",
  "query": "-R/path/to/resource",
  "key": "path/to/resource"
}
```

---

## 4. Store API Module

The Store API provides operations on the persistent data store.

### 4.1 Endpoints

#### 4.1.1 GET /api/store/data/{*key}

Retrieve raw data for a given key.

**Request:**
```
GET /liquer/api/store/data/path/to/resource
```

**Success Response (200):**
```
Content-Type: application/octet-stream
Content-Disposition: attachment; filename="resource"

[binary data]
```

**Error Response (404):**
```json
{
  "status": "ERROR",
  "error": {
    "type": "KeyNotFound",
    "message": "Key 'path/to/resource' not found",
    "key": "path/to/resource"
  }
}
```

---

#### 4.1.2 POST /api/store/data/{*key}

Store data at the given key.

**Request:**
```
POST /liquer/api/store/data/path/to/resource
Content-Type: application/octet-stream

[binary data]
```

**Success Response (201):**
```json
{
  "status": "OK",
  "result": {
    "key": "path/to/resource",
    "size": 1024,
    "stored": true
  },
  "message": "Data stored successfully",
  "key": "path/to/resource"
}
```

**Error Response (409):**
```json
{
  "status": "ERROR",
  "error": {
    "type": "KeyAlreadyExists",
    "message": "Key 'path/to/resource' already exists",
    "key": "path/to/resource"
  }
}
```

---

#### 4.1.3 GET /api/store/metadata/{*key}

Retrieve metadata for a given key.

**Request:**
```
GET /liquer/api/store/metadata/path/to/resource
```

**Success Response (200):**
```json
{
  "status": "OK",
  "result": {
    "key": "path/to/resource",
    "status": "Ready",
    "type_identifier": "application/json",
    "message": "Data ready",
    "timestamp": "2026-01-19T12:34:56Z",
    "query": null,
    "position": {},
    "traceback": null
  },
  "message": "Metadata retrieved",
  "key": "path/to/resource"
}
```

REVIEW NOTE: "result" will be a JSON-serialized version of `MetadataRecord` or legacy metadata JSON structure (typically an older version of MetadataRecord that failed to deserialize).

**CHANGE SUMMARY**: Clarified metadata response format:
- Primary format: JSON serialization of `liquers_core::metadata::MetadataRecord` structure
- Fallback: Legacy metadata JSON for backward compatibility with older metadata that can't be deserialized as `MetadataRecord`
- `MetadataRecord` fields include: `log`, `query`, `key`, `status`, `type_identifier`, `data_format`, `media_type`, `message`, `timestamp`, `position`, `traceback`, `primary_progress`, `secondary_progress`, `title`, `tags`, `icon`
- Implementation should attempt to deserialize as `MetadataRecord` first, fall back to raw JSON value on failure 

---

#### 4.1.4 POST /api/store/metadata/{*key}

Update metadata for a given key.

**Request:**
```
POST /liquer/api/store/metadata/path/to/resource
Content-Type: application/json

{
  "status": "Ready",
  "message": "Custom status message"
}
```

**Success Response (200):**
```json
{
  "status": "OK",
  "result": {
    "updated": true
  },
  "message": "Metadata updated",
  "key": "path/to/resource"
}
```

---

#### 4.1.5 DELETE /api/store/data/{*key}

Remove data at the given key.

**Request:**
```
DELETE /liquer/api/store/data/path/to/resource
```

REVIEW NOTE: All the store methods should be callable as REST GET. Thus this would be equivalent to GET /liquer/api/store/remove/path/to/resource

**CHANGE SUMMARY**: Added GET-based method endpoints as alternatives to RESTful HTTP verbs:
- All store operations can be invoked via `GET /api/store/{method}/{*key}` pattern for backward compatibility with Python liquer
- Examples: `GET /api/store/remove/{*key}`, `GET /api/store/removedir/{*key}`, `GET /api/store/makedir/{*key}`
- Destructive operations (remove, removedir, set) can be disabled via builder configuration for security
- Both RESTful (DELETE, PUT, POST) and GET-based methods are supported
- GET-based methods added to section 4.1 alongside RESTful equivalents
- Builder pattern allows `StoreApiBuilder::with_safe_methods_only()` to disable destructive GET operations


**Success Response (200):**
```json
{
  "status": "OK",
  "result": {
    "removed": true
  },
  "message": "Resource removed",
  "key": "path/to/resource"
}
```

**Error Response (404):**
```json
{
  "status": "ERROR",
  "error": {
    "type": "KeyNotFound",
    "message": "Key 'path/to/resource' not found",
    "key": "path/to/resource"
  }
}
```

---

#### 4.1.6 GET /api/store/contains/{*key}

Check if a key exists in the store.

**Request:**
```
GET /liquer/api/store/contains/path/to/resource
```

**Success Response (200):**
```json
{
  "status": "OK",
  "result": {
    "contains": true
  },
  "message": "Key exists",
  "key": "path/to/resource"
}
```

---

#### 4.1.7 GET /api/store/is_dir/{*key}

Check if a key represents a directory.

**Request:**
```
GET /liquer/api/store/is_dir/path/to/dir
```

**Success Response (200):**
```json
{
  "status": "OK",
  "result": {
    "is_dir": true
  },
  "message": "Key is a directory",
  "key": "path/to/dir"
}
```

---

#### 4.1.8 GET /api/store/listdir/{*key}

List contents of a directory.

**Request:**
```
GET /liquer/api/store/listdir/path/to/dir
```

**Success Response (200):**
```json
{
  "status": "OK",
  "result": {
    "keys": [
      "path/to/dir/file1.txt",
      "path/to/dir/file2.json",
      "path/to/dir/subdir"
    ]
  },
  "message": "Directory listing retrieved",
  "key": "path/to/dir"
}
```

---

#### 4.1.9 GET /api/store/keys

List all keys in the store (optionally with prefix filter).

**Request:**
```
GET /liquer/api/store/keys?prefix=path/to
```

**Query Parameters:**
- `prefix` (optional): Filter keys by prefix

**Success Response (200):**
```json
{
  "status": "OK",
  "result": {
    "keys": [
      "path/to/resource1",
      "path/to/resource2",
      "path/to/dir/file"
    ],
    "total": 3
  },
  "message": "Keys retrieved"
}
```

---

#### 4.1.10 PUT /api/store/makedir/{*key}

Create a directory at the given key.

**Request:**
```
PUT /liquer/api/store/makedir/path/to/newdir
```

**Success Response (201):**
```json
{
  "status": "OK",
  "result": {
    "created": true
  },
  "message": "Directory created",
  "key": "path/to/newdir"
}
```

---

#### 4.1.11 DELETE /api/store/removedir/{*key}

Remove a directory (must be empty or recursive flag set).

REVIEW NOTE: Equivalent to DELETE /api/store/data/{*key} if key is a directory.

**CHANGE SUMMARY**: Clarified that `removedir` is semantically equivalent to deleting data at a directory key:
- `DELETE /api/store/removedir/{*key}` is equivalent to `DELETE /api/store/data/{*key}` when key is a directory
- Also available as `GET /api/store/removedir/{*key}` for compatibility
- The dedicated `removedir` endpoint provides explicit directory handling with the `recursive` parameter
- Implementations should validate that the key represents a directory before removal
- If key is not a directory, return 400 Bad Request with appropriate error message

**Request:**
```
DELETE /liquer/api/store/removedir/path/to/dir?recursive=true
```

**Query Parameters:**
- `recursive` (optional, default: `false`): Remove directory recursively

**Success Response (200):**
```json
{
  "status": "OK",
  "result": {
    "removed": true
  },
  "message": "Directory removed",
  "key": "path/to/dir"
}
```

---

#### 4.1.12 POST /api/store/upload/{*key}

Upload file(s) to the store (multipart form data).

**Request:**
```
POST /liquer/api/store/upload/path/to/target
Content-Type: multipart/form-data; boundary=----WebKitFormBoundary

------WebKitFormBoundary
Content-Disposition: form-data; name="file"; filename="data.json"
Content-Type: application/json

{ "key": "value" }
------WebKitFormBoundary--
```

**Success Response (201):**
```json
{
  "status": "OK",
  "result": {
    "uploaded": [
      {
        "filename": "data.json",
        "key": "path/to/target/data.json",
        "size": 16
      }
    ]
  },
  "message": "Files uploaded successfully",
  "key": "path/to/target"
}
```

---

#### 4.1.13 GET /api/store/entry/{*key}

Retrieve both data and metadata in a single unified structure (optimized for remote store implementations).

**DataEntry Structure:**

The `result` field contains a `DataEntry` structure with the following fields:

```rust
pub struct DataEntry {
    pub metadata: serde_json::Value,   // MetadataRecord or LegacyMetadata serialized to JSON Value
    pub data: Vec<u8>,                 // Binary data (base64-encoded when serialized to JSON)
}
```

**Note:** When using JSON format, the `data` field is automatically base64-encoded for efficiency. CBOR and bincode formats handle binary data natively without encoding overhead.

**Request:**
```
GET /liquer/api/store/entry/path/to/resource?format=cbor
Accept: application/cbor
```

**Format Selection:**
The serialization format can be specified via:
1. URL query parameter: `?format=cbor` (takes precedence)
2. Accept header: `Accept: application/cbor`
3. Default: CBOR if neither is specified

**Supported Formats:**
- `cbor` → `application/cbor` (default, most efficient)
- `bincode` → `application/x-bincode`
- `json` → `application/json`

**Success Response (200) - CBOR format:**
```
HTTP/1.1 200 OK
Content-Type: application/cbor

[CBOR-encoded response with status, message, error, and result containing DataEntry]
```

**Success Response (200) - JSON format:**
```json
{
  "status": "OK",
  "message": "Data and metadata retrieved",
  "error": null,
  "key": "path/to/resource",
  "result": {
    "metadata": {
      "key": "path/to/resource",
      "status": "Ready",
      "type_identifier": "application/json",
      "data_format": "json",
      "timestamp": "2026-01-20T10:30:00Z"
    },
    "data": "eyJrZXkiOiAidmFsdWUifQ=="
  }
}
```

**Metadata Field:**
The `metadata` field is always a `serde_json::Value` containing:
- `MetadataRecord` serialized to JSON Value (preferred)
- `LegacyMetadata` as JSON Value (fallback for compatibility)

This ensures forward/backward compatibility when `MetadataRecord` structure changes between client and server versions.

**Error Response (404):**
```json
{
  "status": "ERROR",
  "message": "Key not found: path/to/missing",
  "error": {
    "status": "ERROR",
    "message": "Key not found: path/to/missing",
    "error_type": "KeyNotFound",
    "details": {},
    "timestamp": "2026-01-20T10:30:00Z"
  },
  "key": "path/to/missing",
  "result": null
}
```

---

#### 4.1.14 POST /api/store/entry/{*key}

Set both data and metadata in a single unified structure (optimized for remote store implementations).

**Request Body:**
The request body contains a `DataEntry` structure (see section 4.1.13) with only two fields:

```rust
pub struct DataEntry {
    pub metadata: serde_json::Value,   // MetadataRecord or LegacyMetadata serialized to JSON Value
    pub data: Vec<u8>,                 // Binary data (base64-encoded when serialized to JSON)
}
```

**Request (CBOR format):**
```
POST /liquer/api/store/entry/path/to/resource?format=cbor
Content-Type: application/cbor

[CBOR-encoded DataEntry structure]
```

**Request (JSON format):**
```
POST /liquer/api/store/entry/path/to/resource?format=json
Content-Type: application/json

{
  "metadata": {
    "status": "Ready",
    "type_identifier": "application/json",
    "data_format": "json"
  },
  "data": "eyJrZXkiOiAidmFsdWUifQ=="
}
```

**Format Support:**
- URL parameter `?format=cbor` or Content-Type header determines format
- Same format options as GET: cbor, bincode, json

**Success Response (201):**
```json
{
  "status": "OK",
  "message": "Data and metadata stored",
  "error": null,
  "key": "path/to/resource",
  "result": {
    "metadata": {
      "key": "path/to/resource",
      "status": "Ready",
      "type_identifier": "application/json",
      "data_format": "json",
      "timestamp": "2026-01-20T10:35:00Z"
    },
    "data": ""
  }
}
```

**Error Response (400):**
```json
{
  "status": "ERROR",
  "message": "Invalid metadata format",
  "error": {
    "status": "ERROR",
    "message": "Invalid metadata format",
    "error_type": "ValidationError",
    "details": {},
    "timestamp": "2026-01-20T10:35:00Z"
  },
  "key": "path/to/resource",
  "result": null
}
```

---

#### 4.1.15 DELETE /api/store/data/{*key}

Remove resource at the specified key (file or directory).

**Request:**
```
DELETE /liquer/api/store/data/path/to/resource
```

**Success Response (200):**
```json
{
  "status": "OK",
  "message": "Resource removed",
  "key": "path/to/resource",
  "result": {
    "removed": true
  }
}
```

---

#### 4.1.16 DELETE /api/store/entry/{*key}

Remove resource (equivalent to DELETE /api/store/data).

**Request:**
```
DELETE /liquer/api/store/entry/path/to/resource
```

**Success Response (200):**
```json
{
  "status": "OK",
  "message": "Resource removed",
  "key": "path/to/resource",
  "result": {
    "removed": true
  }
}
```

**Note:** This endpoint is semantically equivalent to `DELETE /api/store/data/{*key}`.

---

### 4.2 Store Module Trait

The Store API module is implemented as a generic builder that accepts any `Environment`:

```rust
pub struct StoreApiBuilder<E: Environment> {
    base_path: String,
    _phantom: PhantomData<E>,
}

impl<E: Environment> StoreApiBuilder<E> {
    pub fn new(base_path: impl Into<String>) -> Self;
    pub fn build<R>(self) -> R where R: Router<E>;
}
```

---

## 5. Assets API Module

The Assets API provides access to managed assets with lifecycle tracking and real-time notifications.

---

### 5.0 Comparison with Store API

The following table summarizes the key differences between the Assets API and Store API:

| Aspect | Store API | Assets API |
|--------|-----------|------------|
| **Resource Identifier** | Key (path-like string) | Query (may include commands) |
| **Example Path** | `/api/store/data/path/to/file.ext` | `/api/assets/data/-R/path/to/-/cmd-arg/file.ext` |
| **Data Source** | Pre-existing stored files | On-demand computation or cache |
| **Lifecycle** | Static (no state changes) | Dynamic (status transitions) |
| **Status Values** | Implicit (exists or not) | Explicit (None, Recipe, Processing, Ready, Error, etc.) |
| **Metadata Complexity** | Minimal (basic info) | Rich (logs, progress, traceback) |
| **Progress Tracking** | Not applicable | Primary & secondary progress |
| **Real-time Updates** | No notifications | WebSocket notifications |
| **Write Operations** | Yes (POST, DELETE) | Limited (POST entry to set Source/Override assets) |
| **Directory Support** | Yes | Yes (via listdir) |
| **Recipe Integration** | No | Yes (query resolution) |
| **Computation Trigger** | N/A | May trigger on access |
| **Caching** | Not applicable | Managed by AssetManager |
| **Concurrent Access** | File-level locking | Asset-level lifecycle management |

**When to use Store API:**
- Direct file storage and retrieval
- Static data that doesn't change frequently
- Simple key-based access patterns
- When you need write/delete operations

**When to use Assets API:**
- Computed/derived data from queries
- Data with complex processing pipelines
- When you need progress tracking
- Real-time status updates required
- Recipe-based data generation

---

### 5.0.1 Endpoint Comparison Table

The following table shows all endpoints across Store API, Assets API, and Recipes API, with analogous endpoints aligned in rows:

| Method | Store API | Assets API | Recipes API | Description |
|--------|-----------|------------|-------------|-------------|
| GET | `/api/store/data/{*key}` | `/api/assets/data/{*query}` | `/api/recipes/data/{*key}` | Retrieve resource data only |
| GET | `/api/store/metadata/{*key}` | `/api/assets/metadata/{*query}` | `/api/recipes/metadata/{*key}` | Retrieve resource metadata only |
| GET | `/api/store/listdir/{*key}` | `/api/assets/listdir/{*query}` | `/api/recipes/listdir` | List resources in directory/namespace |
| GET | `/api/store/entry/{*key}` | `/api/assets/entry/{*query}` | `/api/recipes/entry/{*key}` | Retrieve both data and metadata (efficient combined access) |
| POST | `/api/store/entry/{*key}` | `/api/assets/entry/{*query}` | — | Set both data and metadata (efficient combined write) |
| POST | `/api/store/data/{*key}` | `/api/assets/data/{*query}` | — | Store data at key/query |
| POST | `/api/store/metadata/{*key}` | `/api/assets/metadata/{*query}` | — | Store metadata at key/query |
| DELETE | `/api/store/data/{*key}` | `/api/assets/data/{*query}` | — | Remove resource (file/directory or cached asset value) |
| DELETE | `/api/store/entry/{*key}` | `/api/assets/entry/{*query}` | — | Remove resource (same as DELETE data) |
| GET | `/api/store/remove/{*key}` | `/api/assets/remove/{*query}` | — | Remove resource (GET-based alternative) |
| GET | `/api/store/makedir/{*key}` | — | — | Create directory |
| DELETE | `/api/store/removedir/{*key}` | — | — | Remove directory (explicit endpoint) |
| GET | `/api/store/removedir/{*key}` | — | — | Remove directory (GET-based alternative) |
| POST | `/api/store/upload/{*key}` | — | — | Upload files via multipart form data |
| GET | — | — | `/api/recipes/resolve/{*key}` | Resolve recipe to execution plan |
| WebSocket | — | `/ws/assets/{*query}` | — | Subscribe to real-time asset notifications |

**Notes:**
- Store API uses `{*key}` path parameter (path-like string)
- Assets API uses `{*query}` path parameter (may include commands)
- Recipes API uses `{*key}` path parameter (path-like string, same as Store API)
- GET-based write operations (remove, makedir, removedir) can be disabled via builder configuration
- WebSocket endpoint is unique to Assets API for real-time status updates
- **DELETE behavior for Assets API:** Deleting an asset with a recipe (via DELETE data or DELETE entry) removes only the cached value, not the recipe definition. The asset status returns to `Status::Recipe`, allowing re-computation on next access.
- **DELETE entry semantics:** DELETE entry is equivalent to DELETE data for both Store and Assets APIs

---

### 5.1 HTTP Endpoints

#### 5.1.1 GET /api/assets/data/{*query}

Retrieve asset data with embedded metadata.

**Request:**
```
GET /liquer/api/assets/data/path/to-text/some/query
```

**Success Response (200):**
```json
{
  "status": "OK",
  "result": {
    "data": "base64encodeddata...",
    "metadata": {
      "key": "path/to-text/some/query",
      "status": "Ready",
      "type_identifier": "text/plain",
      "message": "Asset ready",
      "timestamp": "2026-01-19T12:34:56Z"
    }
  },
  "message": "Asset retrieved",
  "query": "/path/to-text/some/query"
}
```

**Alternative Response (Binary):**
```
HTTP/1.1 200 OK
Content-Type: text/plain
X-Liquers-Status: Ready
X-Liquers-Key: path/to-text/some/query

[asset data]
```

---

#### 5.1.2 GET /api/assets/metadata/{*query}

Retrieve asset metadata only.

**Request:**
```
GET /liquer/api/assets/metadata/path/to/some/query
```

**Success Response (200):**
```json
{
  "status": "OK",
  "result": {
    "key": "path/to/some/query",
    "status": "Processing",
    "type_identifier": "application/json",
    "message": "Processing in progress",
    "timestamp": "2026-01-19T12:34:56Z",
    "primary_progress": {
      "message": "Processing items",
      "done": 42,
      "total": 100,
      "timestamp": "2026-01-19T12:34:55Z",
      "eta": "2026-01-19T12:35:30Z"
    },
    "secondary_progress": null
  },
  "message": "Metadata retrieved",
  "query": "/path/to/some/query"
}
```

---

#### 5.1.3 GET /api/assets/listdir/{*query}

List assets in a directory (query must resolve to directory).

**Request:**
```
GET /liquer/api/assets/listdir/path/to/dir
```

**Success Response (200):**
```json
{
  "status": "OK",
  "result": {
    "assets": [
      {
        "key": "path/to/dir/asset1",
        "status": "Ready"
      },
      {
        "key": "path/to/dir/asset2",
        "status": "Processing"
      }
    ]
  },
  "message": "Assets listed",
  "query": "/path/to/dir"
}
```

---

#### 5.1.4 POST /api/assets/data/{*query}

Store asset data directly (bypasses computation, sets asset as externally sourced).

**Request:**
```
POST /liquer/api/assets/data/path/to/query
Content-Type: application/json

{"result": "data"}
```

**Success Response (201):**
```json
{
  "status": "OK",
  "message": "Asset data stored",
  "query": "/path/to/query",
  "result": {
    "stored": true
  }
}
```

**Asset Status:**
When data is successfully stored, the asset status is changed to `Source` (externally set asset).

**Implementation Note:**
This endpoint requires `AssetManager::set()` or similar implementation. May return NotImplemented error until fully implemented.

---

#### 5.1.5 POST /api/assets/metadata/{*query}

Store asset metadata directly.

**Request:**
```
POST /liquer/api/assets/metadata/path/to/query
Content-Type: application/json

{
  "status": "Ready",
  "type_identifier": "application/json",
  "message": "Externally set metadata"
}
```

**Success Response (201):**
```json
{
  "status": "OK",
  "message": "Asset metadata stored",
  "query": "/path/to/query",
  "result": {
    "stored": true
  }
}
```

---

#### 5.1.6 DELETE /api/assets/data/{*query}

Remove cached asset value. If the asset has an associated recipe, the recipe is not deleted - only the cached value is removed, and the asset status returns to `Status::Recipe`.

**Request:**
```
DELETE /liquer/api/assets/data/path/to/query
```

**Success Response (200):**
```json
{
  "status": "OK",
  "message": "Asset value removed",
  "query": "/path/to/query",
  "result": {
    "removed": true,
    "new_status": "Recipe"
  }
}
```

**Behavior:**
- **Asset with recipe:** Cached value removed, status returns to `Status::Recipe`, asset can be recomputed on next access
- **Asset without recipe:** Asset removed entirely, status becomes `Status::None`

---

#### 5.1.7 DELETE /api/assets/entry/{*query}

Remove cached asset value (equivalent to DELETE /api/assets/data).

**Request:**
```
DELETE /liquer/api/assets/entry/path/to/query
```

**Success Response (200):**
```json
{
  "status": "OK",
  "message": "Asset value removed",
  "query": "/path/to/query",
  "result": {
    "removed": true,
    "new_status": "Recipe"
  }
}
```

**Note:** This endpoint is semantically equivalent to `DELETE /api/assets/data/{*query}`.

---

#### 5.1.8 GET /api/assets/entry/{*query}

Retrieve both asset data and metadata in a single unified structure (uses same `DataEntry` structure as Store API).

**DataEntry Structure:**

See section 4.1.13 for complete `DataEntry` structure definition. The structure is identical for both Store and Assets APIs.

**Request:**
```
GET /liquer/api/assets/entry/path/to-cmd/some/query?format=cbor
Accept: application/cbor
```

**Format Selection:**
Same as Store API (see section 4.1.13):
1. URL query parameter: `?format=cbor` (takes precedence)
2. Accept header: `Accept: application/cbor`
3. Default: CBOR if neither is specified

**Success Response (200) - JSON format:**
```json
{
  "status": "OK",
  "message": "Asset data and metadata retrieved",
  "error": null,
  "query": "/path/to-cmd/some/query",
  "result": {
    "metadata": {
      "key": "path/to-cmd/some/query",
      "query": "/path/to-cmd/some/query",
      "status": "Ready",
      "type_identifier": "application/json",
      "data_format": "json",
      "message": "Asset ready",
      "timestamp": "2026-01-20T10:30:00Z",
      "primary_progress": null,
      "secondary_progress": null
    },
    "data": "eyJyZXN1bHQiOiAiZGF0YSJ9"
  }
}
```

**Asset Status Integration:**
- If asset status is `Processing`, response is returned immediately with current metadata
- If asset status is `Error`, response has `status: "ERROR"` with error details in `error` field
- If asset status is `None` or `Dependencies`, asset computation may be triggered

**Error Response (when asset computation fails):**
```json
{
  "status": "ERROR",
  "message": "Asset computation failed",
  "error": {
    "status": "ERROR",
    "message": "Command 'cmd' failed: invalid argument",
    "error_type": "ExecutionError",
    "details": {
      "command": "cmd",
      "traceback": ["line 1", "line 2"]
    },
    "timestamp": "2026-01-20T10:30:00Z"
  },
  "query": "/path/to/query",
  "result": {
    "metadata": {
      "key": "path/to/query",
      "query": "/path/to/query",
      "status": "Error",
      "message": "Asset computation failed",
      "traceback": ["line 1", "line 2"],
      "timestamp": "2026-01-20T10:30:00Z"
    },
    "data": ""
  }
}
```

---

#### 5.1.9 POST /api/assets/entry/{*query}

Set asset data and metadata directly (bypasses computation, sets asset as externally sourced).

**Implementation Note:**
This endpoint requires `AssetManager::set()` implementation. Until fully implemented, this endpoint may return:
```json
{
  "status": "ERROR",
  "message": "AssetManager::set not yet implemented",
  "error": {
    "status": "ERROR",
    "message": "Direct asset setting not supported in this version",
    "error_type": "NotImplemented",
    "details": {},
    "timestamp": "2026-01-20T10:30:00Z"
  },
  "query": "/path/to/query",
  "result": null
}
```

**Request Body:**
The request body contains a `DataEntry` structure (see section 4.1.13) with only two fields:

```rust
pub struct DataEntry {
    pub metadata: serde_json::Value,   // MetadataRecord or LegacyMetadata serialized to JSON Value
    pub data: Vec<u8>,                 // Binary data (base64-encoded when serialized to JSON)
}
```

**Request (CBOR format):**
```
POST /liquer/api/assets/entry/path/to/query?format=cbor
Content-Type: application/cbor

[CBOR-encoded DataEntry structure]
```

**Request (JSON format):**
```
POST /liquer/api/assets/entry/path/to/query?format=json
Content-Type: application/json

{
  "metadata": {
    "status": "Ready",
    "type_identifier": "application/json",
    "data_format": "json",
    "message": "Externally set asset"
  },
  "data": "eyJzb3VyY2UiOiAiZXh0ZXJuYWwifQ=="
}
```

**Format Support:**
- URL parameter `?format=cbor` or Content-Type header determines format
- Same format options as GET: cbor, bincode, json

**Asset Status:**
When successfully set, the asset status is changed to one of:
- `Source`: Asset set as external source (preferred for user-provided data)
- `Override`: Asset set as override of computed value (future feature)

**Success Response (201):**
```json
{
  "status": "OK",
  "message": "Asset data and metadata set",
  "error": null,
  "query": "/path/to/query",
  "result": {
    "metadata": {
      "key": "path/to/query",
      "query": "/path/to/query",
      "status": "Source",
      "type_identifier": "application/json",
      "data_format": "json",
      "message": "Externally set asset",
      "timestamp": "2026-01-20T10:35:00Z"
    },
    "data": ""
  }
}
```

**Error Response (400):**
```json
{
  "status": "ERROR",
  "message": "Invalid metadata format",
  "error": {
    "status": "ERROR",
    "message": "Invalid metadata format",
    "error_type": "ValidationError",
    "details": {},
    "timestamp": "2026-01-20T10:35:00Z"
  },
  "query": "/path/to/query",
  "result": null
}
```

---

### 5.2 WebSocket API

#### 5.2.1 WS /ws/assets/{*query}

Subscribe to real-time asset notifications.

**Connection:**
```
ws://localhost:3000/liquer/ws/assets/path/to/some/query
```

**Initial Message (sent by server on connect):**
```json
{
  "type": "Initial",
  "asset_id": 12345,
  "query": "-R/path/to/some/query",
  "key": "path/to/some/query",
  "timestamp": "2026-01-19T12:34:56Z",
  "metadata": {
    "status": "Processing",
    "message": "Asset being computed"
  }
}
```

**Notification Messages:**

All messages from `liquers_core::assets::AssetNotificationMessage` are supported:

REVIEW NOTE: Message should contain serialized AssetNotificationMessage and asset id for identification.

**CHANGE SUMMARY**: Updated all WebSocket notification message formats to include `asset_id` field:
- Added `asset_id` (u64) field to all notification messages for client-side tracking
- Asset ID is obtained from `liquers_core::assets::AssetRef::id()` method
- Enables clients to correlate notifications with specific asset instances
- Particularly useful when subscribing to multiple assets simultaneously
- Asset ID remains constant for the lifetime of an asset instance
- All message examples in section 5.2.1 updated to include `"asset_id": 12345` field

##### Initial
```json
{
  "type": "Initial",
  "asset_id": 12345,
  "query": "-R/path/to/some/query",
  "timestamp": "2026-01-19T12:34:56Z"
}
```

##### JobSubmitted
```json
{
  "type": "JobSubmitted",
  "asset_id": 12345,
  "query": "-R/path/to/some/query",
  "timestamp": "2026-01-19T12:35:00Z"
}
```

##### JobStarted
```json
{
  "type": "JobStarted",
  "asset_id": 12345,
  "query": "-R/path/to/some/query",
  "timestamp": "2026-01-19T12:35:01Z"
}
```

##### StatusChanged
```json
{
  "type": "StatusChanged",
  "asset_id": 12345,
  "query": "-R/path/to/some/query",
  "status": "Processing",
  "timestamp": "2026-01-19T12:35:02Z"
}
```

Status values: `None`, `Directory`, `Recipe`, `Submitted`, `Dependencies`, `Processing`, `Partial`, `Error`, `Storing`, `Ready`, `External`, `Unavailable`

##### ValueProduced
```json
{
  "type": "ValueProduced",
  "asset_id": 12345,
  "query": "-R/path/to/some/query",
  "timestamp": "2026-01-19T12:35:10Z"
}
```

##### ErrorOccurred
```json
{
  "type": "ErrorOccurred",
  "asset_id": 12345,
  "query": "-R/path/to/some/query",
  "timestamp": "2026-01-19T12:35:15Z",
  "error": {
    "type": "ExecutionError",
    "message": "Command failed: division by zero",
    "traceback": ["..."]
  }
}
```

##### LogMessage
```json
{
  "type": "LogMessage",
  "asset_id": 12345,
  "query": "-R/path/to/some/query",
  "timestamp": "2026-01-19T12:35:05Z",
  "message": "Processing step 1 of 3"
}
```

##### PrimaryProgressUpdated
```json
{
  "type": "PrimaryProgressUpdated",
  "asset_id": 12345,
  "query": "-R/path/to/some/query",
  "timestamp": "2026-01-19T12:35:06Z",
  "progress": {
    "message": "Processing items",
    "done": 42,
    "total": 100,
    "timestamp": "2026-01-19T12:35:06Z",
    "eta": "2026-01-19T12:35:30Z"
  }
}
```

##### SecondaryProgressUpdated
```json
{
  "type": "SecondaryProgressUpdated",
  "asset_id": 12345,
  "query": "-R/path/to/some/query",
  "timestamp": "2026-01-19T12:35:07Z",
  "progress": {
    "message": "Downloading dependencies",
    "done": 5,
    "total": 10,
    "timestamp": "2026-01-19T12:35:07Z",
    "eta": null
  }
}
```

##### JobFinished
```json
{
  "type": "JobFinished",
  "asset_id": 12345,
  "query": "-R/path/to/some/query",
  "timestamp": "2026-01-19T12:35:20Z"
}
```

**Client Messages:**

Clients can send control messages to the server:

##### Subscribe to additional queries
```json
{
  "action": "subscribe",
  "query": "-R/another/query"
}
```

##### Unsubscribe from queries
```json
{
  "action": "unsubscribe",
  "query": "-R/path/to/some/query"
}
```

REVIEW NOTE: Should there be "unsubscribe all" too?

**CHANGE SUMMARY**: Added "unsubscribe all" WebSocket action:
- New action: `{"action": "unsubscribe_all"}` to clear all active subscriptions
- Useful for client cleanup without tracking individual subscriptions
- Server responds with confirmation message
- Added to section 5.2.1 WebSocket client messages
- See new subsection "Unsubscribe from all queries" after line 843

##### Unsubscribe from all queries
```json
{
  "action": "unsubscribe_all"
}
```

**Server Response:**
```json
{
  "type": "UnsubscribedAll",
  "timestamp": "2026-01-19T12:35:30Z",
  "message": "All subscriptions cleared"
}
```

##### Ping (keep-alive)
```json
{
  "action": "ping"
}
```

**Server Response to Ping:**
```json
{
  "type": "Pong",
  "timestamp": "2026-01-19T12:35:25Z"
}
```

---

### 5.3 Assets Module Trait

```rust
pub struct AssetsApiBuilder<E: Environment> {
    base_path: String,
    websocket_path: String,
    _phantom: PhantomData<E>,
}

impl<E: Environment> AssetsApiBuilder<E> {
    pub fn new(base_path: impl Into<String>) -> Self;
    pub fn with_websocket_path(self, path: impl Into<String>) -> Self;
    pub fn build<R>(self) -> R where R: Router<E>;
}
```

---

## 6. Recipes API Module

The Recipes API provides access to recipe definitions and resolution.

REVIEW NOTE: This API should be an interface to `AsyncRecipeProvider` trait defined in liquers_core::recipes.

**CHANGE SUMMARY**: Clarified that Recipes API is a direct HTTP interface to `AsyncRecipeProvider` trait methods:
- `GET /api/recipes/listdir` maps to `AsyncRecipeProvider::assets_with_recipes()`
- `GET /api/recipes/data/{*key}` maps to `AsyncRecipeProvider::recipe()` or `recipe_opt()`
- `GET /api/recipes/metadata/{*key}` maps to `AsyncRecipeProvider::recipe()` with metadata extraction
- `GET /api/recipes/entry/{*key}` maps to `AsyncRecipeProvider::recipe()` with combined data and metadata
- `GET /api/recipes/resolve/{*key}` maps to `AsyncRecipeProvider::recipe_plan()`
- Additional method `has_recipes()` not exposed directly but used internally
- `contains()` method not exposed as separate endpoint
- Implementation should delegate directly to the `AsyncRecipeProvider` obtained from `Environment::get_recipe_provider()`
- Error handling maps `AsyncRecipeProvider` errors to HTTP status codes
- Added clarification to section 6.0 overview and 6.2 implementation notes


### 6.1 Endpoints

#### 6.1.1 GET /api/recipes/listdir

List all available recipes.

**Request:**
```
GET /liquer/api/recipes/listdir
```

REVIEW NOTE: Filtering by namespace is not needed.

**CHANGE SUMMARY**: Removed namespace filtering from Recipes API:
- `namespace` query parameter removed from `GET /api/recipes/listdir` endpoint
- `namespace` query parameter removed from `GET /api/recipes/data/{*key}` endpoint
- Recipe keys are globally unique within an `AsyncRecipeProvider` instance
- Namespace concept not present in `AsyncRecipeProvider` trait
- Response examples updated to remove namespace field
- If namespace-like organization is needed, it should be encoded in the recipe key itself (e.g., "reports/summary")
- Updated sections 6.1.1 and 6.1.2 to reflect this change

**Success Response (200):**
```json
{
  "status": "OK",
  "result": {
    "recipes": [
      {
        "name": "data_pipeline",
        "title": "Standard Data Processing Pipeline",
        "description": "Loads CSV, filters rows, and aggregates data"
      },
      {
        "name": "reports/summary",
        "title": "Summary Report Generator",
        "description": "Generate summary reports from processed data"
      }
    ],
    "total": 2
  },
  "message": "Recipes listed"
}
```

---

#### 6.1.2 GET /api/recipes/data/{*key}

Retrieve a specific recipe definition (data only).

**Request:**
```
GET /liquer/api/recipes/data/data_pipeline
```

**Success Response (200):**
```json
{
  "status": "OK",
  "result": {
    "query": "/load-csv/filter-rows/aggregate",
    "title": "Standard Data Processing Pipeline",
    "description": "Loads CSV, filters rows, and aggregates data",
    "arguments": {
      "filename": "data.csv",
      "threshold": 100
    },
    "links": {
      "documentation": "/docs/pipelines/data_pipeline"
    },
    "cwd": null,
    "volatile": false
  },
  "message": "Recipe retrieved"
}
```

**Note:** The `result` field contains the direct JSON serialization of the `Recipe` struct. Fields with empty/default values may be omitted.

REVIEW NOTE: There is no recipe type field. Recipe is a JSON representation of `Recipe` structure defined in liquers_core::recipes.

**CHANGE SUMMARY**: Clarified recipe response format:
- The `recipe` field in responses is a direct JSON serialization of `liquers_core::recipes::Recipe` struct
- `Recipe` struct fields: `query` (String), `title` (String), `description` (String), `arguments` (HashMap<String, Value>), `links` (HashMap<String, String>), `cwd` (Option<String>), `volatile` (bool)
- No separate "type" discriminator field exists
- Response format should match the Serde serialization of the `Recipe` struct directly
- Example response updated in section 6.1.2 to show actual Recipe struct format
- Fields with empty/default values may be omitted due to `skip_serializing_if` attributes

**Error Response (404):**
```json
{
  "status": "ERROR",
  "error": {
    "type": "KeyNotFound",
    "message": "Recipe 'data_pipeline' not found"
  }
}
```

---

#### 6.1.3 GET /api/recipes/metadata/{*key}

Retrieve metadata for a specific recipe.

**Request:**
```
GET /liquer/api/recipes/metadata/data_pipeline
```

**Success Response (200):**
```json
{
  "status": "OK",
  "result": {
    "name": "data_pipeline",
    "title": "Standard Data Processing Pipeline",
    "description": "Loads CSV, filters rows, and aggregates data",
    "volatile": false
  },
  "message": "Recipe metadata retrieved"
}
```

**Note:** Metadata includes summary information from the Recipe struct (name, title, description, volatile flag).

---

#### 6.1.4 GET /api/recipes/entry/{*key}

Retrieve both recipe data and metadata in a single unified structure (uses same `DataEntry` structure as Store and Assets APIs).

**Request:**
```
GET /liquer/api/recipes/entry/data_pipeline?format=cbor
Accept: application/cbor
```

**Format Selection:**
Same as Store and Assets APIs (see section 4.1.13):
1. URL query parameter: `?format=cbor` (takes precedence)
2. Accept header: `Accept: application/cbor`
3. Default: CBOR if neither is specified

**Success Response (200) - JSON format:**
```json
{
  "status": "OK",
  "message": "Recipe retrieved",
  "result": {
    "metadata": {
      "name": "data_pipeline",
      "title": "Standard Data Processing Pipeline",
      "description": "Loads CSV, filters rows, and aggregates data",
      "volatile": false
    },
    "data": "eyJxdWVyeSI6ICIvbG9hZC1jc3YvZmlsdGVyLXJvd3MvYWdncmVnYXRlIiwgLi4ufQ=="
  }
}
```

**Note:** The `data` field contains the base64-encoded JSON serialization of the complete `Recipe` struct when using JSON format. For CBOR and bincode formats, the Recipe struct is encoded directly as binary.

---

#### 6.1.5 GET /api/recipes/resolve/{*key}

Resolve a recipe to an execution plan.

**Request:**
```
GET /liquer/api/recipes/resolve/data_pipeline
```

**Success Response (200):**
```json
{
  "status": "OK",
  "result": {
    "actions": [
      {
        "name": "load-csv",
        "arguments": {"filename": "data.csv"}
      },
      {
        "name": "filter-rows",
        "arguments": {"threshold": 100}
      },
      {
        "name": "aggregate",
        "arguments": {}
      }
    ],
    "query": "/load-csv/filter-rows/aggregate"
  },
  "message": "Recipe resolved to execution plan"
}
```

**Note:** This endpoint maps to `AsyncRecipeProvider::recipe_plan()`. The returned plan shows the sequence of actions that would be executed for this recipe.

**Error Response (404):**
```json
{
  "status": "ERROR",
  "error": {
    "type": "KeyNotFound",
    "message": "Recipe 'data_pipeline' not found"
  }
}
```

---

### 6.2 Recipes Module Trait

```rust
pub struct RecipesApiBuilder<E: Environment> {
    base_path: String,
    _phantom: PhantomData<E>,
}

impl<E: Environment> RecipesApiBuilder<E> {
    pub fn new(base_path: impl Into<String>) -> Self;
    pub fn build<R>(self) -> R where R: Router<E>;
}
```

---

## 7. Query Execution API

The Query Execution API evaluates liquers queries and returns results.

### 7.1 Endpoints

#### 7.1.1 GET /q/{*query}

Execute a query synchronously.

**Request:**
```
GET /liquer/q/path/to-text/some/query?arg1=value1&arg2=value2
```

**Query Parameters:**
- Any parameters are passed as arguments to the final command in the query

**Success Response (200):**

Content type determined by the result value type.

```
HTTP/1.1 200 OK
Content-Type: text/plain
X-Liquers-Query: /path/to-text/some/query
X-Liquers-Status: Ready

Hello, World!
```

**Error Response (500):**
```json
{
  "status": "ERROR",
  "error": {
    "type": "ExecutionError",
    "message": "Command 'to-text' failed: invalid input",
    "query": "/path/to-text/some/query",
    "traceback": ["at position 10", "..."]
  }
}
```

---

#### 7.1.2 POST /q/{*query}

Execute a query with arguments in JSON body.

**Request:**
```
POST /liquer/q/path/to-text/some/query
Content-Type: application/json

{
  "arg1": "value1",
  "arg2": 42,
  "arg3": [1, 2, 3]
}
```

**Success Response (200):**

Same as GET response.

---

### 7.2 Query Module Trait

```rust
pub struct QueryApiBuilder<E: Environment> {
    base_path: String,
    _phantom: PhantomData<E>,
}

impl<E: Environment> QueryApiBuilder<E> {
    pub fn new(base_path: impl Into<String>) -> Self;
    pub fn build<R>(self) -> R where R: Router<E>;
}
```

---

## 8. Generic Library Design

The web API library is designed to be generic over the `Environment` type, allowing embedding applications to use custom value types and configurations.

### 8.1 Core Abstractions

#### 8.1.1 Environment Trait

The API modules accept any type implementing `liquers_core::environment::Environment`:

```rust
pub trait Environment: Send + Sync + Clone + 'static {
    type Value: ValueInterface + Send + Sync + 'static;
    type Payload: PayloadInterface + Send + Sync + 'static;
    type Session: SessionInterface + Send + Sync + 'static;

    fn get_async_store(&self) -> Arc<Box<dyn AsyncStore>>;
    fn get_asset_manager(&self) -> Arc<Box<dyn AssetManager<Self>>>;
    fn get_command_registry(&self) -> &CommandRegistry<Self>;
    // ...
}
```

#### 8.1.2 Router Trait

Web framework integrations implement a generic `Router` trait:

```rust
pub trait Router<E: Environment> {
    fn add_route(&mut self, method: Method, path: &str, handler: Box<dyn Handler<E>>);
    fn merge(&mut self, other: Self);
}
```

---

### 8.2 Module Composition

API modules can be composed to create custom service combinations:

```rust
use liquers_web::{StoreApiBuilder, AssetsApiBuilder, QueryApiBuilder};

// Create a custom router with only store and query APIs
let router = AxumRouter::new()
    .merge(StoreApiBuilder::<MyEnvironment>::new("/api/store").build())
    .merge(QueryApiBuilder::<MyEnvironment>::new("/q").build());
```

---

### 8.3 First-Class Support for liquers-lib

When using `DefaultEnvironment` from `liquers-lib`, additional features are enabled:

- Automatic content negotiation for `ExtValue` types
- Built-in support for Polars DataFrames, images, egui UI components
- Optimized serialization paths for common types

```rust
use liquers_lib::environment::DefaultEnvironment;
use liquers_web::FullApiBuilder;

let app = FullApiBuilder::<DefaultEnvironment>::new()
    .with_base_path("/liquer")
    .with_store_api()
    .with_assets_api()
    .with_recipes_api()
    .with_query_api()
    .build();
```

---

### 8.4 Custom Value Type Example

```rust
use liquers_core::value::ValueInterface;

#[derive(Clone, Debug)]
pub enum MyValue {
    Text(String),
    Number(f64),
    Custom(MyCustomType),
}

impl ValueInterface for MyValue {
    // Implementation...
}

// Use with API modules
let store_api = StoreApiBuilder::<MyEnvironment>::new("/api/store").build();
```

---

## 9. Authentication & Authorization

### 9.1 Delegation Model

Authentication and authorization are delegated to the embedding application through middleware/layers:

```rust
// Application-provided middleware
let auth_layer = AuthLayer::new(my_auth_handler);

let app = FullApiBuilder::<MyEnvironment>::new()
    .with_base_path("/liquer")
    .with_store_api()
    .with_assets_api()
    .build()
    .layer(auth_layer);  // Framework-specific layer mechanism
```

---

### 9.2 Session Integration

The API extracts session information from the `Environment::Session` type:

```rust
// Custom session implementation
pub struct MySession {
    user_id: String,
    roles: Vec<String>,
}

impl SessionInterface for MySession {
    fn user(&self) -> String {
        self.user_id.clone()
    }
}
```

The embedding application's middleware populates the session before requests reach API handlers.

REVIEW NOTE: This is to be designed more precisely.

**CHANGE SUMMARY**: Session/authorization design requires additional specification:
- Current design delegates session management entirely to embedding application
- Need to specify:
  1. How session data flows from middleware to API handlers (via Environment? request extensions?)
  2. Whether `SessionInterface` should be extracted per-request or shared via Environment
  3. How session permissions map to Store/Assets operations (read/write/execute)
  4. Example middleware implementations for common auth patterns (JWT, OAuth, API keys)
  5. Integration with axum's state and extension systems
  6. How to pass session context to AssetManager and CommandExecutor
- TODO: Add section 9.3 "Session Flow and Integration Patterns" with:
  - Sequence diagram showing session extraction and propagation
  - Example middleware implementation
  - Per-request vs shared session tradeoffs
  - Permission checking integration points
- This remains open for future specification revision 

---

## 10. Axum Implementation

This section describes axum-specific implementation details.

### 10.1 Route Structure

```rust
use axum::{
    Router,
    routing::{get, post, put, delete},
    extract::{Path, State, Query},
    response::IntoResponse,
};

pub fn create_store_routes<E: Environment>() -> Router<EnvRef<E>> {
    Router::new()
        .route("/data/{*key}", get(get_data_handler::<E>))
        .route("/data/{*key}", post(post_data_handler::<E>))
        .route("/data/{*key}", delete(delete_data_handler::<E>))
        .route("/metadata/{*key}", get(get_metadata_handler::<E>))
        // ...
}
```

---

### 10.2 Handler Pattern

All handlers follow this pattern:

```rust
#[axum::debug_handler]
async fn get_data_handler<E: Environment>(
    Path(key): Path<String>,
    State(env): State<EnvRef<E>>,
) -> Result<impl IntoResponse, ApiError> {
    let key = parse_key(&key)?;
    let store = env.get_async_store();
    let data = store.get(&key).await?;

    Ok(DataResponse::new(data))
}
```

---

### 10.3 Error Handling

Errors implement `IntoResponse` for automatic conversion:

```rust
pub struct ApiError {
    status_code: StatusCode,
    error: liquers_core::error::Error,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = json!({
            "status": "ERROR",
            "error": {
                "type": format!("{:?}", self.error.error_type),
                "message": self.error.message,
            }
        });

        (self.status_code, Json(body)).into_response()
    }
}

impl From<liquers_core::error::Error> for ApiError {
    fn from(error: liquers_core::error::Error) -> Self {
        let status_code = match error.error_type {
            ErrorType::KeyNotFound => StatusCode::NOT_FOUND,
            ErrorType::ParseError => StatusCode::BAD_REQUEST,
            // ... see section 3.3
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };

        ApiError { status_code, error }
    }
}
```

---

### 10.4 WebSocket Implementation

```rust
use axum::extract::ws::{WebSocket, WebSocketUpgrade, Message};
use tokio::sync::broadcast;

async fn websocket_handler<E: Environment>(
    ws: WebSocketUpgrade,
    Path(query): Path<String>,
    State(env): State<EnvRef<E>>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket::<E>(socket, query, env))
}

async fn handle_socket<E: Environment>(
    mut socket: WebSocket,
    query: String,
    env: EnvRef<E>,
) {
    let query = parse_query(&query).unwrap();
    let asset_manager = env.get_asset_manager();

    // Subscribe to asset notifications
    let mut rx = asset_manager.subscribe(&query).await;

    // Send initial state
    if let Ok(metadata) = asset_manager.get_metadata(&query).await {
        let msg = serde_json::to_string(&NotificationMessage {
            r#type: "Initial",
            metadata: Some(metadata),
            ..Default::default()
        }).unwrap();

        socket.send(Message::Text(msg)).await.ok();
    }

    // Forward notifications to client
    while let Ok(notification) = rx.recv().await {
        let msg = serde_json::to_string(&notification).unwrap();
        if socket.send(Message::Text(msg)).await.is_err() {
            break;
        }
    }
}
```

---

### 10.5 State Management

```rust
pub type EnvRef<E> = Arc<E>;

pub async fn create_app<E: Environment>(env: E) -> Router {
    let env_ref = Arc::new(env);

    Router::new()
        .merge(create_store_routes::<E>())
        .merge(create_assets_routes::<E>())
        .merge(create_recipes_routes::<E>())
        .merge(create_query_routes::<E>())
        .with_state(env_ref)
}
```

---

## Appendix A: Complete Example

```rust
use liquers_lib::environment::DefaultEnvironment;
use liquers_web_axum::{FullApiBuilder, serve};

#[tokio::main]
async fn main() {
    // Create environment
    let env = DefaultEnvironment::new()
        .with_file_store("/var/data")
        .with_default_commands();

    // Build API
    let app = FullApiBuilder::new(env)
        .with_base_path("/liquer")
        .with_store_api()
        .with_assets_api()
        .with_recipes_api()
        .with_query_api()
        .build();

    // Serve
    serve(app, "0.0.0.0:3000").await;
}
```

---

## Appendix B: Migration from Python liquer

| Python liquer | Rust liquers Web API | Notes |
|--------------|---------------------|-------|
| `/liquer/q/QUERY` | `/liquer/q/{*query}` | Same path structure |
| `/liquer/api/store/data/KEY` | `/liquer/api/store/data/{*key}` | Same path structure |
| `/liquer/api/cache/get/KEY` | `/liquer/api/assets/data/{*query}` | Cache renamed to assets |
| Status in JSON body | HTTP status codes + JSON | Proper REST semantics |
| `/submit/QUERY` | Not included | Async execution via asset system |

---

## Appendix C: Future Extensions

Features not included in v1.0 but planned for future versions:

1. **Outbound Asset Messages**: WebSocket for sending messages TO assets (e.g., cancel, pause, resume)
2. **Batch Operations**: Bulk store operations for efficiency
3. **Streaming Responses**: For large datasets
4. **Server-Sent Events**: Alternative to WebSocket for unidirectional notifications
5. **GraphQL API**: Alternative query interface
6. **OpenAPI Specification**: Auto-generated API documentation
7. **Recipe CRUD**: POST/PUT/DELETE operations for recipes (currently read-only)
8. **CORS Configuration**: Built-in CORS middleware
9. **Rate Limiting**: Built-in rate limiting middleware

---

## Revision History

| Version | Date | Changes |
|---------|------|---------|
| 1.0.0 | 2026-01-19 | Initial specification |
