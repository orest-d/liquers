# Phase 1: High-Level Design - Web API Library

## Feature Name

Web API Library (liquers-axum rebuild)

## Purpose

Transform liquers-axum from a monolithic standalone server into a composable library that applications can embed and customize. Implements Query Execution API (`/q` endpoints) and Store API (`/api/store` endpoints) as reusable builders, generic over Environment, supporting CBOR/bincode/JSON serialization formats for atomic get/set of data+metadata in unified entry endpoints. Enables applications to compose web APIs with custom configuration while maintaining 100% compliance with specs/WEB_API_SPECIFICATION.md.

## Core Interactions

### Query System
Exposes query execution via HTTP: `GET /q/{*query}` and `POST /q/{*query}`.
Parses query from URL path, evaluates via `Environment::evaluate()`, returns results as binary or JSON based on metadata.

### Store System
Exposes AsyncStore operations via REST endpoints: GET/POST/DELETE for data, metadata, unified entry format.
Supports directory operations (listdir, makedir, removedir), multipart upload.
All operations delegate to `Environment::get_store()`.

### Command System
No new commands. API layer is pure HTTP interface over existing query/store functionality.

### Asset System
Consumes AssetRef results from query evaluation.
Polls asset status, serializes data/metadata for HTTP responses.

### Value Types
No new ExtValue variants. Generic over `Environment::Value`.
Works with any Value type that implements ValueInterface (liquers-core only).

### Web/API
Complete liquers-axum rebuild:
- Builder pattern: `QueryApiBuilder<E>`, `StoreApiBuilder<E>` compose into Axum routers
- Response types: `ApiResponse<T>`, `ErrorDetail`, `DataEntry`, `BinaryResponse` (framework-agnostic)
- Error mapping: `ErrorType` → HTTP status codes per spec section 3.3
- Format selection: CBOR (default), bincode, JSON via Accept header or `?format=` param

### UI
Not applicable (server-side web API).

## Crate Placement

**liquers-axum** - Complete rebuild of existing crate
- Rationale: Already the designated crate for HTTP/web functionality
- Dependencies: liquers-core (Environment, AsyncStore, Error), liquers-store (for Store trait), axum, serde, ciborium, bincode
- **No dependency on liquers-lib** - works with core Value type only (ExtValue support requires liquers-lib integration by application)

## Open Questions

1. ✅ **RESOLVED:** `with_destructive_gets()` will be **opt-in** (disabled by default) for security.

2. ✅ **RESOLVED:** Unified entry endpoints will **not support streaming** in this implementation (defer to future work).

3. ✅ **RESOLVED:** Response serialization format will be determined **per-endpoint** via Accept header and `?format=` query parameter (not global config).

## References

- specs/WEB_API_SPECIFICATION.md - Complete API specification (sections 3, 4, 7)
- liquers-core/src/context.rs - Environment trait
- liquers-core/src/store.rs - AsyncStore trait
- Current liquers-axum/src/ - Existing implementation (~30% complete, will be replaced)
