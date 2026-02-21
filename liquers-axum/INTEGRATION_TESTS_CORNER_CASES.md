# Integration Tests & Corner Cases - Web API Library Phase 3

## Overview

This document outlines comprehensive integration tests and corner cases for the Liquers Web API (liquers-axum rebuild). Tests are organized by category and include both positive/negative scenarios and stress conditions.

## Integration Test Files

### 1. `liquers-axum/tests/query_api_integration.rs`

End-to-end tests for Query API (GET/POST execution).

**Test Categories:**
- Query Parsing (valid/invalid/encoded)
- Handler State & Metadata
- Error Handling & HTTP Status Mapping
- POST Handler JSON Body Parsing
- Query Encoding/Decoding Round-trip
- Value Serialization for HTTP Response
- State and Metadata for Response Construction
- Large Value Serialization (1MB+)
- Error Context and Response Construction
- Concurrent Query Execution
- Key Operations in Query Context
- End-to-End Query Execution Flow

**Key Test Patterns:**

```rust
// Handler simulation: GET /liquer/q/text-hello
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_full_query_flow() {
    // Step 1: Extract from path → "text-hello"
    let query_str = "text-hello";

    // Step 2: Parse query
    let query = parse_query(query_str).expect("Should parse");

    // Step 3: Evaluate (simulated)
    // let asset_ref = env.evaluate(&query).await;
    // let state = asset_ref.wait().await;

    // Step 4: Serialize response
    let value = Value::from("hello result");
    let bytes = value.to_bytes();

    // Step 5: Return Response with metadata
}

// Handler simulation: POST /liquer/q/text-hello
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_post_json_body_parsing() {
    let json_str = r#"{"greeting": "Hello", "count": 42}"#;
    let parsed: serde_json::Value = serde_json::from_str(json_str)
        .expect("Valid JSON should parse");

    // Merge args into query evaluation
    // Return response
}
```

### 2. `liquers-axum/tests/store_api_integration.rs`

End-to-end tests for Store API (CRUD, directory, unified entry endpoints).

**Test Categories:**
- Store CRUD Operations (set/get/delete/overwrite)
- Metadata Operations
- Directory Operations (makedir/listdir/is_dir/removedir)
- Unified Entry Endpoints (data + metadata together)
- Format Selection (CBOR/Bincode/JSON)
- Accept Header Format Selection
- Round-trip Serialization (all formats)
- Error Handling in Store Operations
- Concurrent Store Operations
- Large Binary Data (10MB+)
- Key Encoding/Decoding

**Key Test Patterns:**

```rust
// Store CRUD: POST /api/store/data/test/data.txt
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_store_set_get_data() {
    let store = Arc::new(AsyncStoreRouter::new());
    let key = Key::from_string("test/data.txt").expect("Valid key");
    let data = b"test data content".to_vec();
    let metadata = Metadata::new();

    // POST operation
    store.set(&key, &data, &metadata).await.expect("Set");

    // GET operation
    let (retrieved, meta) = store.get(&key).await.expect("Get");
    assert_eq!(retrieved, data);
}

// Format selection: GET /api/store/entry/test/data?format=json
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_format_json_serialization() {
    let data = b"test data".to_vec();

    // Query param: format=json
    // Accept header: application/json
    // Serialize with base64 encoding for binary data
    let encoded = base64::engine::general_purpose::STANDARD.encode(&data);

    // Response: DataEntry { metadata, data }
}
```

---

## Corner Cases & Stress Scenarios

### 1. Memory Management

#### 1.1 Large Data Handling

**Scenario:** Query/Store operations with 1MB+ values

```markdown
- **Query Large Value:** GET /liquer/q/text-largefile (1MB response)
- **Store Large Data:** POST /api/store/data/large.bin (10MB upload)
- **Concurrent Large Values:** 5 simultaneous 1MB serializations
```

**Tests:**
- `test_large_value_serialization_1mb` - 1MB string value
- `test_large_value_serialization_10mb` - 10MB string value
- `test_large_binary_data_10mb` - 10MB binary in store
- `test_concurrent_value_serialization` - Multiple concurrent serializations

**Corner Cases:**
- Handler must serialize 10MB without OOM
- Arc cloning for large values avoids duplication
- Streaming response for truly large data (not implemented in Phase 2)
- Memory cleanup after response sent

**Mitigation:**
- Use Arc<Value> for sharing (already in State)
- Test with realistic data sizes (1MB, 10MB)
- Monitor Arc clone count in concurrent tests
- Consider streaming for >100MB (future enhancement)

#### 1.2 Metadata Size

**Scenario:** Custom metadata with large metadata payloads

```markdown
- Metadata with custom fields: 1MB metadata object
- Serialization of metadata-heavy responses
```

**Tests:**
- `test_metadata_media_types` - Various media type strings
- Round-trip tests with metadata preservation

**Corner Cases:**
- Metadata serialization in JSON/CBOR/Bincode
- Query error includes metadata (may be large)
- Response header size constraints (HTTP headers ~8KB limit)

**Mitigation:**
- Metadata should be small (<1KB typical)
- X-Liquers-* headers have size limits
- Error detail metadata is optional

---

### 2. Concurrency & Thread Safety

#### 2.1 Multiple Concurrent Requests

**Scenario:** 5+ simultaneous GET/POST handlers

```markdown
- GET /liquer/q/text-hello × 5 concurrent
- POST /liquer/q/text-hello (with JSON body) × 5 concurrent
- Mixed GET and POST on same query
```

**Tests:**
- `test_concurrent_get_requests` - 5 simultaneous GET parses
- `test_concurrent_post_requests` - 5 simultaneous POST body parses
- `test_concurrent_value_serialization` - Serialize same value concurrently
- `test_concurrent_writes` - Store: 10 concurrent set operations
- `test_concurrent_reads` - Store: 10 concurrent get operations

**Corner Cases:**
- Arc<Store> and Arc<Environment> shared across handler tasks
- Value serialization of same Arc<Value> multiple times
- State cloning in concurrent contexts
- No race conditions in AsyncStoreRouter

**Mitigation:**
- Use Arc for shared resources (Store, Environment, large Values)
- Tokio multi-threaded executor (worker_threads=4)
- No mutable state without internal synchronization
- AsyncStore trait methods are Send + Sync

#### 2.2 Concurrent Store Operations

**Scenario:** Multiple concurrent writes to different keys

```markdown
- 10 concurrent POST /api/store/data/test/file{i}.txt
- 10 concurrent reads from written keys
- Mixed read/write operations
```

**Tests:**
- `test_concurrent_writes` - 10 tasks set different keys
- `test_concurrent_reads` - 10 tasks read 5 shared keys
- (Store API tests file covers these)

**Corner Cases:**
- Key collision if not enough namespace separation
- Store memory growth with many concurrent writes
- AsyncStoreRouter routing between multiple backends
- Metadata consistency across concurrent writes

**Mitigation:**
- Use unique key prefixes per test (test/concurrent{i})
- Store implementations handle locking internally
- Verify metadata preserved after concurrent ops

---

### 3. Serialization & Format Selection

#### 3.1 Format Round-trip (CBOR/Bincode/JSON)

**Scenario:** Data survives encode/decode in all formats

```markdown
- CBOR: binary data → CBOR → binary (lossless)
- Bincode: Vec<u8> → Bincode → Vec<u8> (lossless)
- JSON: Vec<u8> → base64 → JSON → base64 → Vec<u8> (lossless)
```

**Tests:**
- `test_roundtrip_cbor` - CBOR serialization
- `test_roundtrip_bincode` - Bincode serialization
- `test_roundtrip_json_base64` - JSON with base64 encoding
- (Store API tests file has comprehensive roundtrips)

**Corner Cases:**
- Base64 encoding/decoding overhead (~33% larger)
- CBOR/Bincode handle binary cleanly, JSON requires base64
- Metadata serialization in each format
- Empty data round-trip
- Large data round-trip (10MB+)

**Mitigation:**
- CBOR as default (efficient binary format)
- JSON for browser clients (base64 overhead acceptable)
- Bincode for Python bindings (compact binary)
- Test all formats with same data

#### 3.2 Accept Header vs Query Parameter

**Scenario:** Format negotiation with conflicting signals

```markdown
- GET /api/store/entry/data with:
  - Accept: application/json + ?format=cbor → CBOR wins (query param)
  - Accept: application/cbor (no param) → CBOR
  - No Accept header + no param → CBOR (default)
```

**Tests:**
- `test_format_accept_header_json`
- `test_format_accept_header_cbor`
- `test_format_accept_header_bincode`
- `test_format_query_parameter`

**Corner Cases:**
- Query parameter takes precedence over Accept header
- Invalid format param → NotSupported error (501)
- Unknown MIME type in Accept → falls back to CBOR
- Multiple values in Accept header (client preference order)

**Mitigation:**
- Explicit precedence: query param > Accept header > default CBOR
- Parser must handle multiple MIME types in Accept
- Invalid format returns clear error message

---

### 4. Error Handling

#### 4.1 Parse Errors (Query & Key)

**Scenario:** Invalid query/key syntax

```markdown
- GET /liquer/q/invalid//syntax → ParseError → 400 Bad Request
- GET /api/store/data/invalid/key → KeyParseError → 400 Bad Request
- GET /liquer/q/ (empty) → ParseError → 400 Bad Request
```

**Tests:**
- Query parsing tests in query_api file
- Key parsing tests in store_api file
- Error context preservation

**Corner Cases:**
- Empty query string (edge case)
- Very long query string (>10KB)
- Special characters in query
- URL encoding edge cases

**Mitigation:**
- Parse query at handler entry → fast 400 response
- Include query/key context in error for debugging
- Set reasonable string length limits

#### 4.2 Store Errors (KeyNotFound, Read/Write)

**Scenario:** Store I/O failures

```markdown
- GET /api/store/data/missing → KeyNotFound → 404 Not Found
- POST /api/store/data/readonly → KeyWriteError → 500 Internal Server
- GET /api/store/data/corrupted → KeyReadError → 500 Internal Server
```

**Tests:**
- `test_store_get_nonexistent_key` - 404
- `test_error_store_read_error` - 500
- `test_error_store_write_error` - 500
- `test_error_serialization_error` - 422

**Corner Cases:**
- Key exists but is directory (not file)
- Permission denied on store access
- Disk full on write
- Corrupted data on read
- Concurrent delete while reading

**Mitigation:**
- Clear error types for each scenario
- HTTP status mapping per ErrorType
- Error message includes context (key, operation)
- Client should handle 5xx errors with retry

#### 4.3 Serialization Errors

**Scenario:** Data incompatible with format

```markdown
- POST body with invalid JSON → SerializationError → 422
- CBOR data fails deserialization → SerializationError → 422
- Bincode size exceeds limits → SerializationError → 422
```

**Tests:**
- `test_post_invalid_json_body` - 422
- Format round-trip tests (implicit success tests)

**Corner Cases:**
- Malformed CBOR/Bincode binary
- JSON with invalid UTF-8
- Base64 decoding fails
- Data size exceeds handler limits

**Mitigation:**
- Validate JSON immediately on body receive
- CBOR/Bincode errors are library errors (propagate clearly)
- Set reasonable size limits (e.g., 100MB max)

---

### 5. Edge Cases & Boundary Conditions

#### 5.1 Empty & Null Data

**Scenario:** Empty values, null metadata, missing optional fields

```markdown
- GET /liquer/q/text-  (empty string param)
- POST /api/store/data/empty with empty bytes
- Response with no metadata
- Query with empty actions
```

**Tests:**
- `test_value_to_bytes_empty` - Empty string serialization
- `test_post_empty_json_object` - Empty JSON object
- `test_metadata_empty` - Default metadata
- `test_post_json_with_nulls` - Null values in JSON

**Corner Cases:**
- Empty string is valid data
- Empty metadata is valid (no media type)
- Empty JSON object is valid
- Empty query string is invalid (parse error)

**Mitigation:**
- Distinguish empty data (valid) from missing data (error)
- Handle empty cases in serialization
- JSON skip_serializing_if for Option fields

#### 5.2 Special Characters & Encoding

**Scenario:** Query/key with special characters

```markdown
- GET /liquer/q/text-%20 (URL encoded space)
- GET /api/store/data/file%2Fname (encoded slash)
- Query with !@#$%^&* in arguments
```

**Tests:**
- `test_query_parsing_url_encoded` - Percent-encoded query
- `test_query_parsing_special_characters` - Special chars in args
- `test_key_special_characters` - Dashes, underscores, numbers in key

**Corner Cases:**
- URL decoding must happen before query parse
- Key encoding/decoding must be lossless
- Special characters in arguments need escaping
- Path traversal attempts (../../etc/passwd)

**Mitigation:**
- Axum Path extractor handles URL decoding automatically
- Key::from_string validates format
- Query parser validates syntax
- No directory traversal in keys (/ handled by key format)

#### 5.3 Resource Limits

**Scenario:** Very large requests approaching limits

```markdown
- Query string >10KB (very complex query chain)
- POST body >100MB (large data upload)
- 100+ concurrent requests (load spike)
- Query with 1000 actions
```

**Tests:**
- `test_large_value_serialization_*` - Large values
- `test_large_binary_data_*` - Large store data
- `test_concurrent_*` - Multiple concurrent tasks
- `test_post_large_json_body` - Large JSON body

**Corner Cases:**
- Axum body size limits (default 2GB)
- Memory exhaustion with many concurrent requests
- Query complexity explosion (exponential evaluation)
- Handler timeout on slow evaluations

**Mitigation:**
- Set reasonable limits (e.g., max 100MB per request)
- Limit query depth (e.g., max 100 actions)
- Timeout long-running queries (future enhancement)
- Monitor concurrent request count

---

### 6. Integration Scenarios

#### 6.1 Query Execution Full Flow

**Scenario:** Complete GET request workflow

```
GET /liquer/q/text-hello
├─ 1. Extract path segment: "text-hello"
├─ 2. parse_query("text-hello") → Query struct
├─ 3. env.evaluate(&query) → AssetRef<E>
├─ 4. asset_ref.wait() → Result<State<Value>>
├─ 5. Serialize State<Value> to bytes
├─ 6. Determine Content-Type from metadata
└─ 7. Response::builder().status(200).body(bytes)
```

**Tests:**
- `test_full_query_flow` - Simulates steps 1-5

**Possible Failures:**
- Step 2: ParseError → 400
- Step 3: UnknownCommand → 400, KeyNotFound → 404
- Step 4: ExecutionError → 500, KeyReadError → 500
- Step 5: SerializationError → 422

#### 6.2 Store Operations Full Flow

**Scenario:** Complete POST store data workflow

```
POST /api/store/data/test/file.txt
├─ 1. Extract path: "test/file.txt"
├─ 2. parse_key("test/file.txt") → Key
├─ 3. Extract body: Vec<u8>
├─ 4. Extract metadata (or use default)
├─ 5. store.set(&key, &data, &metadata)
└─ 6. Response::builder().status(200).body(ok_response())
```

**Tests:**
- `test_store_set_get_data` - Covers set step
- `test_store_metadata_operations` - Covers metadata handling

**Possible Failures:**
- Step 2: ParseError → 400
- Step 5: KeyWriteError → 500, NotSupported → 501

#### 6.3 Entry Endpoint (Unified Data + Metadata)

**Scenario:** GET entry with format negotiation

```
GET /api/store/entry/test/data.csv?format=json
├─ 1. Extract path: "test/data.csv"
├─ 2. parse_key(...)
├─ 3. Parse query param: ?format=json
├─ 4. store.get(&key) → (Vec<u8>, Metadata)
├─ 5. Create DataEntry { metadata, data }
├─ 6. Serialize DataEntry to JSON (base64 data)
├─ 7. Response with application/json
```

**Tests:**
- `test_store_get_entry` - Data + metadata together
- `test_format_*` - Format selection tests

**Format-Specific:**
- CBOR: Raw binary, efficient
- Bincode: Binary, compact
- JSON: Text, base64 for binary data

---

### 7. Serialization Deep Dive

#### 7.1 CBOR Serialization

**Characteristics:**
- Binary format, ~size of bincode
- Handles all Rust types naturally
- No base64 overhead
- Efficient for APIs

**Tests:**
- `test_format_cbor_serialization`
- `test_roundtrip_cbor`

**Corner Cases:**
- Metadata serialization in CBOR
- DataEntry structure in CBOR
- Binary data preservation

#### 7.2 Bincode Serialization

**Characteristics:**
- Binary format, very compact
- Schema-less (used by liquers-py)
- No base64 needed

**Tests:**
- `test_format_bincode_serialization`
- `test_roundtrip_bincode`

**Corner Cases:**
- Version compatibility (fixed format in liquers-py)
- Deserialize strict validation

#### 7.3 JSON Serialization

**Characteristics:**
- Text format, human-readable
- Binary data requires base64
- Metadata as nested objects
- Query/key strings encoded

**Tests:**
- `test_format_json_base64`
- `test_roundtrip_json_base64`
- `test_post_nested_json` - Complex JSON structures

**Base64 Encoding:**
- Input: `b"binary data"` → `"YmluYXJ5IGRhdGE="` (17 chars)
- Overhead: ~33% size increase
- Serde module handles transparently

---

## Test Execution Strategy

### Running All Tests

```bash
# Query API tests
cargo test -p liquers-axum --test query_api_integration -- --nocapture

# Store API tests
cargo test -p liquers-axum --test store_api_integration -- --nocapture

# All tests with output
cargo test -p liquers-axum -- --nocapture --test-threads=1

# Specific test
cargo test -p liquers-axum test_large_value_serialization_1mb -- --nocapture
```

### Test Isolation

- Each test creates independent resources (Store, Environment)
- No shared state between tests
- Multi-threaded executor: `flavor = "multi_thread"`
- Worker threads: 2-4 depending on test

### Performance Expectations

| Test Category | Expected Time | Notes |
|---------------|---------------|-------|
| Query parsing | <1ms | O(n) in query length |
| Value serialization (1MB) | 1-5ms | Network I/O bound |
| Large binary (10MB) | 10-50ms | Store backend dependent |
| Concurrent (5 tasks) | 5-20ms | Tokio runtime overhead |
| Format round-trip | <1ms | CPU-bound serialization |

---

## Future Enhancements

### Phase 3+ Corner Cases

1. **Streaming Responses** - For data >100MB
   - Server-Sent Events (SSE) for long queries
   - Chunked transfer encoding for large store data
   - Backpressure handling

2. **Request Timeouts**
   - Configurable timeout per handler
   - Graceful cancellation on timeout
   - Error response with timeout indicator

3. **Retry Logic**
   - Client-side retry for 5xx errors
   - Idempotent operation markers
   - Exponential backoff strategy

4. **Rate Limiting**
   - Per-client request rate limits
   - Burst allowances
   - 429 Too Many Requests response

5. **Compression**
   - gzip/brotli response compression
   - Content-Encoding negotiation
   - Transparent to client

6. **Caching**
   - ETags for immutable results
   - Cache-Control headers
   - Conditional GET (304 Not Modified)

---

## References

- `specs/web-api-library/phase2-architecture.md` - Phase 2 architecture
- `liquers-core/src/error.rs` - Error types and constructors
- `liquers-core/src/metadata.rs` - Metadata structure
- `CLAUDE.md` - Project conventions (async, error handling, match statements)
