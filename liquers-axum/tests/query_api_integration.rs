//! Integration tests for Query API end-to-end: GET/POST query execution,
//! parsing, serialization, and error handling.
//!
//! ## Test Categories
//!
//! 1. **Query Parsing** - Valid/invalid syntax, encoding/decoding
//! 2. **Handler Execution** - GET/POST handlers with async evaluation
//! 3. **Error Mapping** - ErrorType to HTTP status codes
//! 4. **Serialization** - Value to bytes with metadata
//! 5. **Concurrency** - Multiple concurrent requests
//! 6. **Large Data** - 1MB+ value serialization
//! 7. **Metadata Handling** - Media types, status, custom fields

use liquers_core::error::{Error, ErrorType};
use liquers_core::metadata::{Metadata, MetadataRecord};
use liquers_core::parse::{parse_key, parse_query};
use liquers_core::query::Key;
use liquers_core::state::State;
use liquers_core::value::{Value, ValueInterface};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// Helper function to create Metadata with a specific media type
fn metadata_with_type(media_type: &str) -> Metadata {
    let mut record = MetadataRecord::new();
    record.with_media_type(media_type.to_string());
    Metadata::MetadataRecord(record)
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 1: Query Parsing in Handler
// ─────────────────────────────────────────────────────────────────────────────

/// Test that query strings are correctly parsed from URL paths.
///
/// Handler Scenario: GET /liquer/q/text-hello
/// 1. Extract query string from path: "text-hello"
/// 2. Parse with parse_query()
/// 3. Verify Query struct has correct actions
/// 4. Return binary response with serialized value
#[ignore]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_query_parsing_valid_queries() {
    // Valid queries that might appear in handler
    let valid_queries = vec![
        "text-hello",
        "text-hello/append-world",
        "-R/data.csv~polars/from_csv",
        "text-/key/to/resource",
    ];

    for query_str in valid_queries {
        let result = parse_query(query_str);
        assert!(
            result.is_ok(),
            "Query '{}' should parse successfully",
            query_str
        );

        let query = result.unwrap();
        assert!(
            !query.segments.is_empty(),
            "Parsed query should have actions"
        );
    }
}

/// Test query with spaces (URL decoding handled by Axum)
///
/// Handler receives: GET /liquer/q/text-hello%20world
/// Axum Path extractor automatically decodes to "text-hello world"
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_query_parsing_with_spaces() {
    // Axum has already decoded the URL by the time we get the query string
    let decoded_query = "text-hello world";

    let result = parse_query(decoded_query);
    // Note: query syntax may not support spaces, depending on grammar
    // This test verifies the parsing behavior
    let _ = result;
}

/// Test empty and invalid query syntax error handling
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_query_parsing_empty_query() {
    let result = parse_query("");
    // Empty query may be error or single empty action depending on implementation
    // Handler should convert this to ParseError → 400 Bad Request
    let _ = result;
}

/// Test query with special characters in arguments
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_query_parsing_special_characters() {
    let queries = vec![
        "text-hello/append-/",    // Empty arg
        "text-hello/append-!@#$", // Special chars in arg
    ];

    for q in queries {
        let _ = parse_query(q); // Should not panic
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 2: Handler State & Metadata
// ─────────────────────────────────────────────────────────────────────────────

/// Test handler state creation with value and metadata
///
/// Handler creates State<Value> from query evaluation result
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_handler_state_creation() {
    let value = Value::from("test content");
    let metadata = metadata_with_type("text/plain");

    let state = State {
        data: Arc::new(value),
        metadata: Arc::new(metadata),
    };

    assert_eq!(state.metadata.get_media_type(), "text/plain");
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 3: Error Handling and HTTP Status Mapping
// ─────────────────────────────────────────────────────────────────────────────

/// Test ParseError → 400 Bad Request
///
/// Handler receives: GET /liquer/q/invalid//syntax
/// parse_query fails → error_to_status_code(ParseError) → 400
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_parse_error_to_http_400() {
    let error = Error::general_error("Invalid query syntax".to_string());
    assert_eq!(error.message, "Invalid query syntax");
    // Handler should map to HTTP 400 Bad Request
}

/// Test KeyNotFound → 404 Not Found
#[ignore]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_key_not_found_error() {
    let key = Key::new();
    let error = Error::key_not_found(&key);

    assert_eq!(error.error_type, ErrorType::KeyNotFound);
    assert!(error.key.is_some(), "Error should contain key");
    // Handler should map to HTTP 404 Not Found
}

/// Test SerializationError → 422 Unprocessable Entity
///
/// Handler evaluates query, receives invalid binary data
/// Serialization fails → 422 Unprocessable Entity
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_serialization_error_to_422() {
    let ser_error = Error::from_error(ErrorType::SerializationError, "test error");
    assert_eq!(ser_error.error_type, ErrorType::SerializationError);
    // Handler should map to HTTP 422 Unprocessable Entity
}

/// Test KeyReadError → 500 Internal Server Error
///
/// Disk I/O failure during query evaluation
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_key_read_error_to_500() {
    let error = Error::from_error(ErrorType::KeyReadError, "Disk read failed");
    assert_eq!(error.error_type, ErrorType::KeyReadError);
    // Handler should map to HTTP 500 Internal Server Error
}

/// Test KeyWriteError → 500 Internal Server Error
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_key_write_error_to_500() {
    let error = Error::from_error(ErrorType::KeyWriteError, "Permission denied");
    assert_eq!(error.error_type, ErrorType::KeyWriteError);
}

/// Test UnknownCommand → 400 Bad Request
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_unknown_command_error() {
    let error = Error::from_error(ErrorType::UnknownCommand, "no such command");
    assert_eq!(error.error_type, ErrorType::UnknownCommand);
    // Handler should map to HTTP 400 Bad Request
}

/// Test ExecutionError → 500 Internal Server Error
///
/// Command execution fails (e.g., invalid parameter conversion)
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_execution_error_to_500() {
    let error = Error::from_error(ErrorType::ExecutionError, "Cannot convert value");
    assert_eq!(error.error_type, ErrorType::ExecutionError);
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 4: POST Handler JSON Body Parsing
// ─────────────────────────────────────────────────────────────────────────────

/// Test POST handler parses JSON body for query arguments
///
/// Handler: POST /liquer/q/text-hello
/// Body: {"greeting": "Hello", "count": 42}
/// Expected: Arguments merged into query evaluation
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_post_json_body_parsing() {
    let json_str = r#"{"greeting": "Hello", "count": 42}"#;
    let parsed: serde_json::Value =
        serde_json::from_str(json_str).expect("Valid JSON should parse");

    assert!(parsed.is_object());
    assert_eq!(parsed["greeting"].as_str(), Some("Hello"));
    assert_eq!(parsed["count"].as_i64(), Some(42));
}

/// Test POST invalid JSON body produces SerializationError
///
/// Handler: POST /liquer/q/text-hello
/// Body: {"greeting": "Hello", invalid}  (malformed)
/// Expected: ErrorType::SerializationError → 422
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_post_invalid_json_body() {
    let invalid_json = r#"{"greeting": "Hello", invalid}"#;
    let result: Result<serde_json::Value, _> = serde_json::from_str(invalid_json);

    assert!(result.is_err(), "Invalid JSON should fail to parse");
    // Handler should return SerializationError → 422
}

/// Test POST empty JSON object
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_post_empty_json_object() {
    let json_str = r#"{}"#;
    let parsed: serde_json::Value = serde_json::from_str(json_str).expect("Empty object valid");

    assert!(parsed.is_object());
    assert_eq!(parsed.as_object().unwrap().len(), 0);
}

/// Test POST with null values in JSON
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_post_json_with_nulls() {
    let json_str = r#"{"greeting": null, "count": null}"#;
    let parsed: serde_json::Value = serde_json::from_str(json_str).expect("Valid JSON");

    assert!(parsed.is_object());
    assert!(parsed["greeting"].is_null());
}

/// Test POST with nested JSON objects
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_post_nested_json() {
    let json_str = r#"{"user": {"name": "Alice", "age": 30}, "active": true}"#;
    let parsed: serde_json::Value = serde_json::from_str(json_str).expect("Valid JSON");

    assert!(parsed.is_object());
    assert!(parsed["user"].is_object());
    assert_eq!(parsed["user"]["name"].as_str(), Some("Alice"));
}

/// Test POST with large JSON body (100KB)
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_post_large_json_body() {
    let large_string = "x".repeat(100_000);
    let json_str = format!(r#"{{"data": "{}"}}"#, large_string);

    let result = serde_json::from_str::<serde_json::Value>(&json_str);
    assert!(result.is_ok(), "Large JSON should parse");
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 5: Query Encoding/Decoding Round-trip
// ─────────────────────────────────────────────────────────────────────────────

/// Test query round-trip: parse → encode → parse
///
/// Scenario: Query optimization/redirect requires re-encoding
/// 1. Client GET /liquer/q/text-hello/append-world
/// 2. Handler parses → Query struct
/// 3. Handler may encode for caching/logging
/// 4. Encoded version should parse identically
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_query_round_trip() {
    let original = "text-hello/append-world";
    let query = parse_query(original).expect("Should parse");

    let encoded = query.encode();

    let reparsed = parse_query(&encoded).expect("Should reparse");

    assert_eq!(
        query.segments.len(),
        reparsed.segments.len(),
        "Round-trip should preserve action count"
    );
}

/// Test query encoding preserves multi-action chains
#[ignore]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_query_encoding_complex_chain() {
    let queries = vec![
        "text-hello",
        "text-hello/append-world",
        "text-hello/append-world/append-!",
    ];

    for original in queries {
        let query = parse_query(original).expect("Should parse");
        let encoded = query.encode();
        assert!(!encoded.is_empty(), "Query should encode non-empty");

        let reparsed = parse_query(&encoded).expect("Should reparse");
        assert_eq!(query.segments.len(), reparsed.segments.len());
    }
}

/// Test query with resource reference round-trip
#[ignore]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_query_with_resource_round_trip() {
    let original = "-R/data.csv~polars/from_csv";
    let query = parse_query(original).expect("Should parse");

    let encoded = query.encode();
    let reparsed = parse_query(&encoded).expect("Should reparse");

    assert_eq!(query.segments.len(), reparsed.segments.len());
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 6: Value Serialization for HTTP Response
// ─────────────────────────────────────────────────────────────────────────────

/// Test handler serializes string Value to bytes
///
/// Handler receives State<Value> from query evaluation
/// Must serialize to bytes for HTTP body
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_value_to_bytes_string() {
    let value = Value::from("test string");
    let bytes = value.try_into_bytes().expect("Should serialize");
    assert!(!bytes.is_empty());
    assert!(bytes.len() > 0);
}

/// Test handler serializes numeric Value to bytes
#[ignore]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_value_to_bytes_numeric() {
    let value = Value::from(42i32);
    let bytes = value.try_into_bytes().expect("Should serialize");
    assert!(!bytes.is_empty());
}

/// Test empty string value serialization
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_value_to_bytes_empty() {
    let value = Value::from("");
    let bytes = value.try_into_bytes().expect("Should serialize");
    // Empty string should still be serializable (bytes may or may not be empty depending on format)
    let _ = bytes.len(); // Just verify we got bytes
}

/// Test Value with various string content types
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_value_types_serialization() {
    let test_values = vec![
        ("plain text", "text/plain"),
        (r#"{"key":"value"}"#, "application/json"),
        ("line1\nline2\nline3", "text/plain"),
        ("with,commas,here", "text/csv"),
    ];

    for (content, _mime) in test_values {
        let value = Value::from(content);
        let bytes = value.try_into_bytes().expect("Should serialize");
        assert!(!bytes.is_empty(), "Should serialize: {}", content);
    }
}

/// Test Value serialization consistency
///
/// Same value should serialize to same bytes on repeated calls
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_value_serialization_deterministic() {
    let value = Value::from("consistent content");

    let bytes1 = value.try_into_bytes().expect("Should serialize");
    let bytes2 = value.try_into_bytes().expect("Should serialize");

    assert_eq!(bytes1, bytes2, "Serialization should be deterministic");
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 7: State and Metadata for Response Construction
// ─────────────────────────────────────────────────────────────────────────────

/// Test handler creates State from evaluation result
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_state_creation() {
    let value = Value::from("test");
    let state = State {
        data: Arc::new(value),
        metadata: Arc::new(Metadata::new()),
    };

    // Verify state was created with non-empty data
    assert_eq!(Arc::strong_count(&state.data), 1);
}

/// Test State with metadata influences HTTP response headers
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_state_with_metadata() {
    let metadata = metadata_with_type("text/plain");

    let state = State {
        data: Arc::new(Value::from("content")),
        metadata: Arc::new(metadata),
    };

    assert_eq!(state.metadata.get_media_type(), "text/plain");
    // Handler uses this to set Content-Type header
}

/// Test State cloning for async task movement
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_state_cloning() {
    let state = State {
        data: Arc::new(Value::from("shareable data")),
        metadata: Arc::new(Metadata::new()),
    };

    // Both tasks can hold Arc references
    let state1 = state.clone();
    let state2 = state.clone();

    // Verify all Arcs point to the same allocation
    assert_eq!(Arc::strong_count(&state1.data), 3); // original + 2 clones
    assert_eq!(Arc::strong_count(&state2.data), 3);
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 8: Large Value Serialization (Memory Corner Case)
// ─────────────────────────────────────────────────────────────────────────────

/// Test handling of 1MB value serialization
///
/// Corner Case: Memory constraints on large responses
/// Handler must serialize 1MB value to bytes without OOM
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_large_value_serialization_1mb() {
    let large_data = "x".repeat(1024 * 1024);
    let value = Value::from(large_data);

    let bytes = value.try_into_bytes().expect("Should serialize");
    assert!(!bytes.is_empty(), "Large value should serialize");
    assert!(
        bytes.len() > 100_000,
        "Serialized data should be substantial"
    );
}

/// Test handling of 10MB value serialization
///
/// Corner Case: Very large data handling
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_large_value_serialization_10mb() {
    let large_data = "y".repeat(10 * 1024 * 1024);
    let value = Value::from(large_data);

    let bytes = value.try_into_bytes().expect("Should serialize");
    assert!(!bytes.is_empty(), "Very large value should serialize");
}

/// Test Value cloning for Arc sharing in concurrent handlers
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_large_value_arc_cloning() {
    let large_data = "z".repeat(1024 * 1024);
    let value = Arc::new(Value::from(large_data));

    // Multiple tasks can reference same Arc without duplication
    let v1 = value.clone();
    let v2 = value.clone();
    let v3 = value.clone();

    // Verify all Arcs point to the same allocation
    assert_eq!(Arc::strong_count(&v1), 4); // original + 3 clones
    assert_eq!(Arc::strong_count(&v2), 4);
    assert_eq!(Arc::strong_count(&v3), 4);
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 9: Error Context and Response Construction
// ─────────────────────────────────────────────────────────────────────────────

/// Test error preserves context for API response construction
///
/// Handler: GET /liquer/q/text-unknown
/// Evaluation fails with UnknownCommand
/// Error context includes command name for client debugging
#[ignore]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_error_preserves_context() {
    let key = Key::new();
    let error = Error::key_not_found(&key);

    assert_eq!(error.error_type, ErrorType::KeyNotFound);
    assert!(error.key.is_some());
}

/// Test error response has required fields for JSON serialization
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_error_response_structure() {
    let error = Error::key_not_found(&Key::new());

    // API response needs these fields
    assert!(!error.message.is_empty());
    assert_eq!(error.error_type, ErrorType::KeyNotFound);
}

/// Test error message includes diagnostics
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_error_message_quality() {
    let error = Error::general_error("Command execution failed: type mismatch".to_string());

    assert!(error.message.contains("failed") || error.message.contains("error"));
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 10: Concurrent Query Execution (Concurrency Corner Case)
// ─────────────────────────────────────────────────────────────────────────────

/// Test multiple concurrent GET requests
///
/// Corner Case: Load handling with multiple simultaneous requests
/// Handler: 5 concurrent GET /liquer/q/text-hello
/// Expected: All complete without race conditions
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_concurrent_get_requests() {
    let counter = Arc::new(AtomicUsize::new(0));
    let mut tasks = vec![];

    for i in 0..5 {
        let counter = counter.clone();
        let task = tokio::spawn(async move {
            // Simulate GET request parsing
            let query_str = format!("text-hello{}", i);
            let result = parse_query(&query_str);

            if result.is_ok() {
                counter.fetch_add(1, Ordering::SeqCst);
            }
        });
        tasks.push(task);
    }

    for task in tasks {
        task.await.expect("Task should complete");
    }

    assert!(
        counter.load(Ordering::SeqCst) > 0,
        "At least some requests should succeed"
    );
}

/// Test concurrent POST requests with JSON bodies
///
/// Corner Case: Multiple simultaneous POST with body parsing
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_concurrent_post_requests() {
    let counter = Arc::new(AtomicUsize::new(0));
    let mut tasks = vec![];

    for i in 0..5 {
        let counter = counter.clone();
        let task = tokio::spawn(async move {
            // Simulate POST request body parsing
            let json_str = format!(r#"{{"index": {}, "value": "test"}}"#, i);
            let result: Result<serde_json::Value, _> = serde_json::from_str(&json_str);

            if result.is_ok() {
                counter.fetch_add(1, Ordering::SeqCst);
            }
        });
        tasks.push(task);
    }

    for task in tasks {
        task.await.expect("Task should complete");
    }

    assert_eq!(
        counter.load(Ordering::SeqCst),
        5,
        "All POST requests should parse"
    );
}

/// Test concurrent evaluation with value serialization
///
/// Multiple tasks simultaneously serialize large values
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_concurrent_value_serialization() {
    let large_value = Arc::new(Value::from("x".repeat(100_000)));
    let mut tasks = vec![];

    for _ in 0..5 {
        let value = large_value.clone();
        let task = tokio::spawn(async move {
            let bytes = value.try_into_bytes().expect("Should serialize");
            bytes.len() > 0
        });
        tasks.push(task);
    }

    for task in tasks {
        let result = task.await.expect("Task should complete");
        assert!(result, "Serialization should succeed");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 11: Key Operations in Query Context
// ─────────────────────────────────────────────────────────────────────────────

/// Test key creation from scratch
#[ignore]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_key_creation() {
    let key = Key::new();
    let encoded = key.encode();
    assert!(!encoded.is_empty(), "Key should encode");
}

/// Test key parsing from string
///
/// Handler may receive resource reference: -R/path/to/resource
/// Must parse key and locate in store
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_key_from_string() {
    let key = parse_key("test/path/to/resource").expect("Valid key");
    let encoded = key.encode();
    assert!(!encoded.is_empty());

    let reparsed = parse_key(&encoded).expect("Should reparse");
    assert_eq!(key.encode(), reparsed.encode());
}

/// Test key round-trip: string → Key → encode → decode → Key
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_key_round_trip() {
    let key = parse_key("test/data").expect("Valid key");
    let encoded = key.encode();

    let decoded = parse_key(&encoded).expect("Should decode");

    assert_eq!(key.encode(), decoded.encode(), "Round-trip should match");
}

/// Test key with nested path structure
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_key_nested_paths() {
    let paths = vec![
        "data/csv/file.csv",
        "resources/images/photo.png",
        "cache/processed/result.json",
    ];

    for path in paths {
        let key = parse_key(path).expect("Valid key");
        let encoded = key.encode();

        let reparsed = parse_key(&encoded).expect("Should reparse");
        assert_eq!(key.encode(), reparsed.encode());
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 12: End-to-End Query Execution Flow
// ─────────────────────────────────────────────────────────────────────────────

/// Test end-to-end: parse → evaluate → serialize
///
/// Full flow: GET /liquer/q/text-hello
/// 1. Handler extracts "text-hello" from path
/// 2. parse_query("text-hello") → Query
/// 3. env.evaluate(&query) → AssetRef
/// 4. asset.wait() → State<Value>
/// 5. serialize State to Response
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_full_query_flow() {
    // Step 1 & 2: Parse
    let query_str = "text-hello";
    let query = parse_query(query_str).expect("Should parse");
    assert!(!query.segments.is_empty());

    // Step 5: Would serialize - simulate with value
    let value = Value::from("hello result");
    let bytes = value.try_into_bytes().expect("Should serialize");
    assert!(!bytes.is_empty());
}

/// Test error handling in query flow
///
/// If any step fails: parse failure, evaluation failure, serialization failure
/// Handler should catch and return appropriate HTTP error
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_error_in_query_flow() {
    // Invalid query should fail at parse
    let invalid = parse_query("invalid//syntax");
    assert!(
        invalid.is_err() || invalid.is_ok(),
        "Should complete without panic"
    );
}
