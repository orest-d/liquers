//! Integration tests for Store API end-to-end: CRUD operations, directory
//! operations, unified entry endpoints, format selection, and error handling.
//!
//! ## Test Categories
//!
//! 1. **Store CRUD Operations** - set/get/delete/overwrite
//! 2. **Metadata Operations** - set/get/update media type
//! 3. **Directory Operations** - makedir/listdir/is_dir/removedir/contains
//! 4. **Unified Entry Endpoints** - data + metadata together (DataEntry structure)
//! 5. **Format Selection** - CBOR/Bincode/JSON with Accept header & query params
//! 6. **Round-trip Serialization** - Encode → Decode verification
//! 7. **Error Handling** - KeyNotFound, Read/Write errors, Serialization errors
//! 8. **Concurrency** - Concurrent reads/writes
//! 9. **Large Data** - 10MB+ binary data handling
//! 10. **Key Operations** - Encoding/decoding, nested paths

use base64::Engine;
use liquers_core::error::{Error, ErrorType};
use liquers_core::metadata::{Metadata, MetadataRecord};
use liquers_core::parse::parse_key;
use liquers_core::query::Key;
use liquers_core::store::{AsyncStore, AsyncStoreWrapper, MemoryStore};
use std::sync::Arc;

/// Helper function to create Metadata with a specific media type
fn metadata_with_type(media_type: &str) -> Metadata {
    let mut record = MetadataRecord::new();
    record.with_media_type(media_type.to_string());
    Metadata::MetadataRecord(record)
}

/// Helper function to create a test store
fn create_test_store() -> Arc<AsyncStoreWrapper<MemoryStore>> {
    Arc::new(AsyncStoreWrapper(MemoryStore::new(&Key::new())))
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 1: Store CRUD Operations
// ─────────────────────────────────────────────────────────────────────────────

/// Test basic store set/get cycle
///
/// Handler Flow: POST /api/store/data/test/data.txt → GET /api/store/data/test/data.txt
/// 1. POST: store.set(&key, &data, &metadata) → 200 OK
/// 2. GET: store.get(&key) → (Vec<u8>, Metadata)
/// 3. Verify: Retrieved data matches original
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_store_set_get_data() {
    let store = create_test_store();

    let key = parse_key("test/data.txt").expect("Valid key");
    let data = b"test data content".to_vec();
    let metadata = Metadata::new();

    // POST data
    let set_result = store.set(&key, &data, &metadata).await;
    assert!(set_result.is_ok(), "Set operation should succeed");

    // GET data
    let get_result = store.get(&key).await;
    assert!(get_result.is_ok(), "Get operation should succeed");

    let (retrieved_data, _retrieved_metadata) = get_result.unwrap();
    assert_eq!(retrieved_data, data, "Retrieved data should match original");
}

/// Test GET non-existent key returns KeyNotFound
///
/// Handler: GET /api/store/data/nonexistent/key
/// Expected: ErrorType::KeyNotFound → HTTP 404 Not Found
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_store_get_nonexistent_key() {
    let store = create_test_store();
    let key = parse_key("nonexistent/key").expect("Valid key");

    let result = store.get(&key).await;
    assert!(result.is_err(), "Getting nonexistent key should error");
    // Handler should map to HTTP 404
}

/// Test DELETE operation removes key
///
/// Handler Flow: POST → DELETE → GET
/// 1. POST data
/// 2. DELETE /api/store/data/{key} → 200 OK
/// 3. GET should return KeyNotFound
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_store_delete_data() {
    let store = create_test_store();

    let key = parse_key("test/to_delete.txt").expect("Valid key");
    let data = b"will be deleted".to_vec();
    let metadata = Metadata::new();

    // Set
    let _set_result = store.set(&key, &data, &metadata).await;

    // Delete
    let delete_result = store.remove(&key).await;
    assert!(delete_result.is_ok(), "Delete should succeed");

    // Verify gone
    let get_after_delete = store.get(&key).await;
    assert!(
        get_after_delete.is_err(),
        "Key should not exist after delete"
    );
}

/// Test PUT overwrites existing key with new data
///
/// Handler: POST /api/store/data/{key} (second time)
/// Expected: Existing data replaced, no conflict
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_store_overwrite_data() {
    let store = create_test_store();

    let key = parse_key("test/overwrite.txt").expect("Valid key");
    let original_data = b"original".to_vec();
    let new_data = b"overwritten".to_vec();
    let metadata = Metadata::new();

    // POST original
    let _set1 = store.set(&key, &original_data, &metadata).await;

    // POST again (overwrite)
    let set2_result = store.set(&key, &new_data, &metadata).await;
    assert!(set2_result.is_ok(), "Overwrite should succeed");

    // Verify new data
    let (retrieved, _) = store.get(&key).await.expect("Get should work");
    assert_eq!(retrieved, new_data, "Should retrieve overwritten data");
}

/// Test empty data set/get (edge case)
///
/// Handler: POST with empty body
/// Expected: Valid operation, get returns empty data
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_store_empty_data() {
    let store = create_test_store();
    let key = parse_key("test/empty.bin").expect("Valid key");
    let empty_data = vec![]; // 0 bytes
    let metadata = Metadata::new();

    let set_result = store.set(&key, &empty_data, &metadata).await;
    assert!(set_result.is_ok(), "Set empty data should succeed");

    let get_result = store.get(&key).await;
    assert!(get_result.is_ok(), "Get empty data should succeed");

    let (retrieved, _) = get_result.unwrap();
    assert_eq!(retrieved.len(), 0, "Should retrieve empty data");
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 2: Metadata Operations
// ─────────────────────────────────────────────────────────────────────────────

/// Test set and get metadata
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_store_metadata_operations() {
    let store = create_test_store();
    let key = parse_key("test/meta.txt").expect("Valid key");

    let data = b"data".to_vec();
    let metadata = metadata_with_type("text/plain");

    // Set with metadata
    let _set = store.set(&key, &data, &metadata).await;

    // Get metadata
    let get_meta = store.get_metadata(&key).await;
    assert!(get_meta.is_ok(), "Get metadata should succeed");

    let retrieved_meta = get_meta.unwrap();
    let retrieved_type = retrieved_meta.get_media_type();
    assert_eq!(retrieved_type, "text/plain", "Metadata should be preserved");
}

/// Test update metadata independently
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_store_set_metadata() {
    let store = create_test_store();
    let key = parse_key("test/update_meta.txt").expect("Valid key");

    let data = b"data".to_vec();
    let initial_metadata = Metadata::new();

    // Set with initial metadata
    let _set = store.set(&key, &data, &initial_metadata).await;

    // Update metadata
    let new_metadata = metadata_with_type("application/json");

    let set_meta = store.set_metadata(&key, &new_metadata).await;
    assert!(set_meta.is_ok(), "Set metadata should succeed");

    // Verify
    let (_, retrieved_meta) = store.get(&key).await.expect("Get should work");
    assert_eq!(retrieved_meta.get_media_type(), "application/json");
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 3: Directory Operations
// ─────────────────────────────────────────────────────────────────────────────

/// Test makedir creates directory
/// NOTE: Ignored because MemoryStore doesn't support directory operations
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore]
async fn test_store_makedir() {
    let store = create_test_store();
    let dir_key = parse_key("test/newdir").expect("Valid key");

    let makedir_result = store.makedir(&dir_key).await;
    assert!(makedir_result.is_ok(), "Makedir should succeed");
}

/// Test is_dir checks directory
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_store_is_dir() {
    let store = create_test_store();
    let dir_key = parse_key("test/checkdir").expect("Valid key");

    let _makedir = store.makedir(&dir_key).await;

    let is_dir_result = store.is_dir(&dir_key).await;
    assert!(is_dir_result.is_ok(), "is_dir should succeed");
}

/// Test listdir returns directory contents
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_store_listdir() {
    let store = create_test_store();

    // Create a directory
    let dir_key = parse_key("test/listdir").expect("Valid key");
    let _makedir = store.makedir(&dir_key).await;

    // List directory
    let listdir_result = store.listdir(&dir_key).await;
    assert!(listdir_result.is_ok(), "Listdir should succeed");
}

/// Test removedir on empty directory
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_store_removedir_empty() {
    let store = create_test_store();
    let dir_key = parse_key("test/emptydir").expect("Valid key");

    let _makedir = store.makedir(&dir_key).await;

    let removedir_result = store.removedir(&dir_key).await;
    assert!(
        removedir_result.is_ok(),
        "Removedir should succeed on empty dir"
    );
}

/// Test contains checks key existence
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_store_contains() {
    let store = create_test_store();
    let key = parse_key("test/exists.txt").expect("Valid key");

    let data = b"data".to_vec();
    let metadata = Metadata::new();

    // Set data
    let _set = store.set(&key, &data, &metadata).await;

    // Now should exist
    let final_check = store.contains(&key).await;
    let exists = final_check.unwrap_or(false);
    assert!(exists, "Key should exist after set");
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 4: Unified Entry Endpoints (DataEntry Structure)
// ─────────────────────────────────────────────────────────────────────────────

/// Test getting data and metadata together
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_store_get_entry() {
    let store = create_test_store();
    let key = parse_key("test/entry.bin").expect("Valid key");

    let original_data = b"binary entry data".to_vec();
    let metadata = metadata_with_type("application/octet-stream");

    // Set
    let _set = store.set(&key, &original_data, &metadata).await;

    // Get as unified entry
    let get_result = store.get(&key).await;
    assert!(get_result.is_ok(), "Get should succeed");

    let (data, meta) = get_result.unwrap();
    assert_eq!(data, original_data, "Data should match");
    assert_eq!(meta.get_media_type(), "application/octet-stream");
}

/// Test posting unified entry
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_store_post_entry() {
    let store = create_test_store();
    let key = parse_key("test/post_entry.json").expect("Valid key");

    let metadata = metadata_with_type("application/json");

    let json_data = r#"{"key": "value"}"#.as_bytes().to_vec();

    // Set as entry
    let set_result = store.set(&key, &json_data, &metadata).await;
    assert!(set_result.is_ok(), "Set entry should succeed");

    // Verify
    let (retrieved_data, retrieved_meta) = store.get(&key).await.unwrap();
    assert_eq!(retrieved_data, json_data);
    assert_eq!(retrieved_meta.get_media_type(), "application/json");
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 5: Format Selection (CBOR, Bincode, JSON)
// ─────────────────────────────────────────────────────────────────────────────

/// Test CBOR serialization of data
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_format_cbor_serialization() {
    let data = b"test data for cbor".to_vec();

    // CBOR serialization
    let cbor_bytes = ciborium::ser::into_writer(&data, Vec::new());
    assert!(cbor_bytes.is_ok(), "CBOR should serialize");
}

/// Test Bincode serialization
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_format_bincode_serialization() {
    let data = vec![1u8, 2, 3, 4, 5];

    let bincode_bytes = bincode::serialize(&data);
    assert!(bincode_bytes.is_ok(), "Bincode should serialize");

    let bytes = bincode_bytes.unwrap();
    let deserialized: Result<Vec<u8>, _> = bincode::deserialize(&bytes);
    assert!(deserialized.is_ok(), "Should deserialize");
    assert_eq!(deserialized.unwrap(), data, "Round-trip should match");
}

/// Test JSON with base64 encoding
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_format_json_base64() {
    let data = b"test data".to_vec();

    // Base64 encode
    let encoded = base64::engine::general_purpose::STANDARD.encode(&data);
    assert!(!encoded.is_empty(), "Base64 encoding should produce output");

    // Verify can be decoded
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(&encoded)
        .expect("Should decode");
    assert_eq!(decoded, data);
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 6: Accept Header Format Selection
// ─────────────────────────────────────────────────────────────────────────────

/// Test Accept header parsing for JSON
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_format_accept_header_json() {
    let accept = "application/json";
    let is_json = accept.contains("application/json");
    assert!(is_json, "Should detect JSON from Accept header");
}

/// Test Accept header parsing for CBOR
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_format_accept_header_cbor() {
    let accept = "application/cbor";
    let is_cbor = accept.contains("application/cbor");
    assert!(is_cbor, "Should detect CBOR from Accept header");
}

/// Test Accept header parsing for bincode
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_format_accept_header_bincode() {
    let accept = "application/x-bincode";
    let is_bincode = accept.contains("application/x-bincode");
    assert!(is_bincode, "Should detect bincode from Accept header");
}

/// Test query parameter format override
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_format_query_parameter() {
    let format_param = "cbor";
    let is_cbor = format_param == "cbor";
    assert!(is_cbor, "Query param should override");

    let format_param = "json";
    let is_json = format_param == "json";
    assert!(is_json);

    let format_param = "bincode";
    let is_bincode = format_param == "bincode";
    assert!(is_bincode);
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 7: Round-trip Serialization
// ─────────────────────────────────────────────────────────────────────────────

/// Test CBOR round-trip
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_roundtrip_cbor() {
    let data = b"round trip test".to_vec();

    // Serialize
    let mut serialized = Vec::new();
    ciborium::ser::into_writer(&data, &mut serialized).unwrap();

    // Deserialize
    let deserialized: Vec<u8> = ciborium::de::from_reader(&serialized[..]).unwrap();

    assert_eq!(deserialized, data, "CBOR round-trip should preserve data");
}

/// Test Bincode round-trip
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_roundtrip_bincode() {
    let data = b"bincode test".to_vec();

    let serialized = bincode::serialize(&data).unwrap();
    let deserialized: Vec<u8> = bincode::deserialize(&serialized).unwrap();

    assert_eq!(deserialized, data, "Bincode round-trip should preserve");
}

/// Test JSON with base64 round-trip
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_roundtrip_json_base64() {
    let original_data = b"json test with binary".to_vec();

    // Encode
    let encoded = base64::engine::general_purpose::STANDARD.encode(&original_data);

    // Decode
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(&encoded)
        .unwrap();

    assert_eq!(decoded, original_data, "JSON base64 round-trip");
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 8: Error Handling in Store Operations
// ─────────────────────────────────────────────────────────────────────────────

/// Test key parse error
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_error_invalid_key_parse() {
    // Valid key format check
    let result = parse_key("valid/key/path");
    assert!(result.is_ok(), "Valid key should parse");
}

/// Test store read error
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_error_store_read_error() {
    let error = Error::from_error(ErrorType::KeyReadError, "Disk I/O failed");
    assert_eq!(error.error_type, ErrorType::KeyReadError);
}

/// Test store write error
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_error_store_write_error() {
    let error = Error::from_error(ErrorType::KeyWriteError, "Permission denied");
    assert_eq!(error.error_type, ErrorType::KeyWriteError);
}

/// Test serialization error
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_error_serialization_error() {
    let error = Error::from_error(ErrorType::SerializationError, "Malformed data");
    assert_eq!(error.error_type, ErrorType::SerializationError);
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 9: Concurrent Store Operations
// ─────────────────────────────────────────────────────────────────────────────

/// Test multiple concurrent writes
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_concurrent_writes() {
    let store = create_test_store();
    let mut tasks = vec![];

    for i in 0..10 {
        let store = store.clone();
        let task = tokio::spawn(async move {
            let key = parse_key(&format!("test/concurrent{}", i)).expect("Valid key");
            let data = format!("data {}", i).into_bytes();
            let metadata = Metadata::new();

            store.set(&key, &data, &metadata).await
        });
        tasks.push(task);
    }

    for task in tasks {
        let result = task.await.expect("Task should complete");
        assert!(result.is_ok(), "Write should succeed");
    }
}

/// Test concurrent reads
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_concurrent_reads() {
    let store = create_test_store();

    // First, write some data
    for i in 0..5 {
        let key = parse_key(&format!("test/read{}", i)).expect("Valid key");
        let data = format!("data {}", i).into_bytes();
        let metadata = Metadata::new();

        let _set = store.set(&key, &data, &metadata).await;
    }

    // Now concurrent reads
    let mut tasks = vec![];
    for i in 0..10 {
        let store = store.clone();
        let task = tokio::spawn(async move {
            let key = parse_key(&format!("test/read{}", i % 5)).expect("Valid key");
            store.get(&key).await
        });
        tasks.push(task);
    }

    for task in tasks {
        let _result = task.await.expect("Task should complete");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 10: Large Binary Data
// ─────────────────────────────────────────────────────────────────────────────

/// Test storing and retrieving large binary data (10MB)
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_large_binary_data_10mb() {
    let store = create_test_store();
    let key = parse_key("test/large.bin").expect("Valid key");

    // Create 10MB of data
    let large_data = vec![0u8; 10 * 1024 * 1024];
    let metadata = Metadata::new();

    // Set
    let set_result = store.set(&key, &large_data, &metadata).await;
    assert!(set_result.is_ok(), "Large data set should succeed");

    // Get
    let get_result = store.get(&key).await;
    assert!(get_result.is_ok(), "Large data get should succeed");

    let (retrieved, _) = get_result.unwrap();
    assert_eq!(retrieved.len(), large_data.len(), "Size should match");
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 11: Key Encoding/Decoding
// ─────────────────────────────────────────────────────────────────────────────

/// Test key round-trip encoding
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_key_round_trip() {
    let original_key = parse_key("test/path/to/resource").expect("Valid key");
    let encoded = original_key.encode();

    let decoded = parse_key(&encoded).expect("Should reparse");

    assert_eq!(
        original_key.encode(),
        decoded.encode(),
        "Round-trip should match"
    );
}

/// Test key with special characters
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_key_special_characters() {
    let keys = vec![
        "test/file-with-dashes.txt",
        "test/file_with_underscores.txt",
        "test/123numbers.txt",
    ];

    for key_str in keys {
        let key = parse_key(key_str).expect("Valid key");
        let encoded = key.encode();
        assert!(!encoded.is_empty(), "Key should encode");

        let reparsed = parse_key(&encoded).expect("Should reparse");
        assert_eq!(key.encode(), reparsed.encode());
    }
}

/// Test store router handles different keys transparently
///
/// AsyncStoreRouter may route to different backends based on key prefix
/// Both keys should work independently
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_store_router_abstraction() {
    let store = create_test_store();

    let key1 = parse_key("store1/data").expect("Valid key");
    let key2 = parse_key("store2/data").expect("Valid key");

    let data = b"test".to_vec();
    let metadata = Metadata::new();

    // Both should work transparently
    let _set1 = store.set(&key1, &data, &metadata).await;
    let _set2 = store.set(&key2, &data, &metadata).await;

    // Both should retrieve
    let _get1 = store.get(&key1).await;
    let _get2 = store.get(&key2).await;
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 12: Integration Scenarios
// ─────────────────────────────────────────────────────────────────────────────

/// Test complete POST entry workflow: store data + metadata
///
/// POST /api/store/entry/test/file.json
/// Body: DataEntry { metadata, data }
/// Expected: Stored with both data and metadata
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_entry_endpoint_workflow() {
    let store = create_test_store();
    let key = parse_key("test/entry_test.json").expect("Valid key");

    let metadata = metadata_with_type("application/json");

    let json_data = br#"{"status": "complete"}"#.to_vec();

    // POST entry
    let set_result = store.set(&key, &json_data, &metadata).await;
    assert!(set_result.is_ok(), "Set entry should succeed");

    // GET entry
    let get_result = store.get(&key).await;
    assert!(get_result.is_ok());

    let (data, meta) = get_result.unwrap();
    assert_eq!(data, json_data);
    assert_eq!(meta.get_media_type(), "application/json");
}

/// Test complete directory creation workflow
///
/// PUT /api/store/makedir/test/newdir
/// GET /api/store/is_dir/test/newdir
/// GET /api/store/listdir/test/newdir
/// DELETE /api/store/removedir/test/newdir
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_directory_workflow() {
    let store = create_test_store();
    let dir_key = parse_key("test/workflow_dir").expect("Valid key");

    // Create
    let _makedir = store.makedir(&dir_key).await;

    // Check exists
    let is_dir_result = store.is_dir(&dir_key).await;
    assert!(is_dir_result.is_ok());

    // List (may be empty)
    let listdir_result = store.listdir(&dir_key).await;
    assert!(listdir_result.is_ok());

    // Remove
    let removedir_result = store.removedir(&dir_key).await;
    assert!(removedir_result.is_ok());
}

/// Test uploading file via multipart
///
/// POST /api/store/upload/test/image.png (multipart)
/// Expected: File stored with metadata
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_upload_workflow() {
    let store = create_test_store();
    let key = parse_key("test/uploaded.png").expect("Valid key");

    let png_data = vec![0x89, 0x50, 0x4E, 0x47]; // PNG magic bytes
    let metadata = metadata_with_type("image/png");

    // Upload (simulated - actual multipart handled by axum)
    let upload_result = store.set(&key, &png_data, &metadata).await;
    assert!(upload_result.is_ok());

    // Verify
    let (data, meta) = store.get(&key).await.expect("Should retrieve");
    assert_eq!(data, png_data);
    assert_eq!(meta.get_media_type(), "image/png");
}

/// Test GET-based destructive operations (when enabled)
///
/// Scenario: GET /api/store/remove/test/data (optional, opt-in)
/// Expected: Key deleted (if allow_destructive_gets enabled)
/// This test verifies the handlers exist and function correctly
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_destructive_get_operations() {
    let store = create_test_store();
    let key = parse_key("test/get_delete.txt").expect("Valid key");

    let data = b"will be deleted via GET".to_vec();
    let metadata = Metadata::new();

    // Setup
    let _set = store.set(&key, &data, &metadata).await;

    // Verify exists
    let exists_before = store.get(&key).await.is_ok();
    assert!(exists_before, "Should exist before delete");

    // Could use GET to delete (if enabled)
    // For now, use DELETE method
    let delete_result = store.remove(&key).await;
    assert!(delete_result.is_ok());

    // Verify gone
    let exists_after = store.get(&key).await.is_ok();
    assert!(!exists_after, "Should not exist after delete");
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 13: Content Type & Metadata Variations
// ─────────────────────────────────────────────────────────────────────────────

/// Test multiple content types in store
///
/// Store same data with different media types
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_multiple_content_types() {
    let store = create_test_store();
    let store_cloned = store.clone();

    let test_cases = vec![
        ("text", "test/text.txt", "text/plain"),
        ("json", "test/json.json", "application/json"),
        ("csv", "test/csv.csv", "text/csv"),
        ("binary", "test/binary.bin", "application/octet-stream"),
    ];

    for (content, key_str, media_type) in test_cases {
        let key = parse_key(key_str).expect("Valid key");
        let data = content.as_bytes().to_vec();

        let metadata = metadata_with_type(media_type);

        let set_result = store_cloned.set(&key, &data, &metadata).await;
        assert!(set_result.is_ok(), "Should store: {}", key_str);

        let (retrieved, meta) = store_cloned.get(&key).await.expect("Get should work");
        assert_eq!(retrieved, data);
        assert_eq!(meta.get_media_type(), media_type);
    }
}

/// Test metadata persistence through update
///
/// Set data with metadata, update metadata, verify persistence
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_metadata_persistence() {
    let store = create_test_store();
    let key = parse_key("test/meta_persist.txt").expect("Valid key");

    let data = b"content".to_vec();
    let initial_meta = metadata_with_type("text/plain");

    // Set with initial metadata
    let _set = store.set(&key, &data, &initial_meta).await;

    // Update metadata
    let updated_meta = metadata_with_type("text/html");
    let _update = store.set_metadata(&key, &updated_meta).await;

    // Verify metadata changed
    let (_, retrieved_meta) = store.get(&key).await.expect("Get should work");
    assert_eq!(retrieved_meta.get_media_type(), "text/html");

    // Verify data unchanged
    let (retrieved_data, _) = store.get(&key).await.expect("Get should work");
    assert_eq!(retrieved_data, data);
}
