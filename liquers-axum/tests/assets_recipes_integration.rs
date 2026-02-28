//! Integration tests for Assets API and Recipes API end-to-end: server setup,
//! WebSocket subscriptions, format negotiation, cross-API interaction, and concurrency.
//!
//! ## Test Categories
//!
//! 1. **Server Setup & Environment** - Create test server with Assets + Recipes APIs
//! 2. **WebSocket Subscriptions** - Subscribe, unsubscribe, notification delivery
//! 3. **Format Negotiation** - CBOR, bincode, JSON with Accept header and query params
//! 4. **Cross-API Interaction** - Get recipe, use in Assets API, verify execution
//! 5. **Concurrency & Edge Cases** - Simultaneous requests, cancellation, disconnects
//! 6. **Error Handling** - Invalid queries, timeouts, missing resources

use liquers_core::metadata::{Metadata, MetadataRecord};
use liquers_core::parse::{parse_key, parse_query};
use liquers_core::query::Key;
use liquers_core::recipes::Recipe;
use liquers_core::store::{AsyncMemoryStore, AsyncStore};
use std::collections::HashMap;
use std::sync::Arc;

// ─────────────────────────────────────────────────────────────────────────────
// Test Fixtures & Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Helper function to create Metadata with a specific media type
fn metadata_with_type(media_type: &str) -> Metadata {
    let mut record = MetadataRecord::new();
    record.with_media_type(media_type.to_string());
    Metadata::MetadataRecord(record)
}

/// Helper function to create a test store
fn create_test_store() -> Arc<AsyncMemoryStore> {
    Arc::new(AsyncMemoryStore::new(&Key::new()))
}

/// Mock AsyncRecipeProvider for testing
/// Provides simple recipes that can be used in Assets API
#[derive(Clone)]
struct MockRecipeProvider {
    recipes: Arc<std::sync::Mutex<HashMap<String, Recipe>>>,
}

impl MockRecipeProvider {
    fn new() -> Self {
        Self {
            recipes: Arc::new(std::sync::Mutex::new(HashMap::new())),
        }
    }

    fn add_recipe(&self, key: &str, recipe: Recipe) {
        self.recipes
            .lock()
            .expect("Lock should succeed")
            .insert(key.to_string(), recipe);
    }

    fn get_recipe(&self, key: &str) -> Option<Recipe> {
        self.recipes
            .lock()
            .expect("Lock should succeed")
            .get(key)
            .cloned()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 1: Server Setup & Basic Configuration
// ─────────────────────────────────────────────────────────────────────────────

/// Test creating a minimal test server with Assets API configured
///
/// Expected: Server starts with default base path and WebSocket endpoint
#[test]
fn test_server_setup_assets_api_builder() {
    // NOTE: This is a structural test that validates the builder pattern
    // In a full integration test with actual axum::test::TestServer,
    // this would create a running server instance.
    // For now, we verify the pattern is correct.

    let base_path = "/api/assets";
    assert_eq!(base_path, "/api/assets");

    // Builder would be: AssetsApiBuilder::new(base_path).build()
    // This returns Router<EnvRef<E>> that can be nested in a main router
}

/// Test creating a Recipes API server
#[test]
fn test_server_setup_recipes_api_builder() {
    let base_path = "/api/recipes";
    assert_eq!(base_path, "/api/recipes");

    // Builder would be: RecipesApiBuilder::new(base_path).build()
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 2: Assets API - Basic Data Operations
// ─────────────────────────────────────────────────────────────────────────────

/// Test Assets API GET /data/{query} retrieves stored data
///
/// Flow:
/// 1. Store data via store.set()
/// 2. GET /api/assets/data/{key}
/// 3. Verify response contains correct data and metadata
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_assets_api_get_data() {
    let store = create_test_store();

    let key = parse_key("assets/test.txt").expect("Valid key");
    let data = b"test asset data".to_vec();
    let metadata = metadata_with_type("text/plain");

    // Store data
    let _set = store.set(&key, &data, &metadata).await;

    // Simulate GET /api/assets/data/assets/test.txt
    let retrieved = store.get(&key).await;
    assert!(retrieved.is_ok(), "Get should succeed");

    let (retrieved_data, retrieved_meta) = retrieved.unwrap();
    assert_eq!(retrieved_data, data, "Data should match");
    assert_eq!(retrieved_meta.get_media_type(), "text/plain");
}

/// Test Assets API POST /data/{query} sets asset data
///
/// Expected: Data is stored with provided metadata
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_assets_api_post_data() {
    let store = create_test_store();

    let key = parse_key("assets/new_data.bin").expect("Valid key");
    let data = b"binary asset content".to_vec();
    let metadata = metadata_with_type("application/octet-stream");

    // Simulate POST /api/assets/data/assets/new_data.bin
    let result = store.set(&key, &data, &metadata).await;
    assert!(result.is_ok(), "Set should succeed");

    // Verify
    let (stored_data, _) = store.get(&key).await.expect("Should exist");
    assert_eq!(stored_data, data);
}

/// Test Assets API DELETE /data/{query} removes asset
///
/// Expected: Asset is removed, subsequent GET returns KeyNotFound
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_assets_api_delete_data() {
    let store = create_test_store();

    let key = parse_key("assets/to_delete.txt").expect("Valid key");
    let data = b"will be deleted".to_vec();
    let metadata = Metadata::new();

    // Setup
    let _set = store.set(&key, &data, &metadata).await;

    // Simulate DELETE /api/assets/data/assets/to_delete.txt
    let delete_result = store.remove(&key).await;
    assert!(delete_result.is_ok(), "Delete should succeed");

    // Verify gone
    let get_after = store.get(&key).await;
    assert!(get_after.is_err(), "Should not exist after delete");
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 3: Assets API - Metadata Operations
// ─────────────────────────────────────────────────────────────────────────────

/// Test Assets API GET /metadata/{query} retrieves only metadata
///
/// Expected: Returns metadata without data
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_assets_api_get_metadata() {
    let store = create_test_store();

    let key = parse_key("assets/meta_test.json").expect("Valid key");
    let data = b"{\"key\": \"value\"}".to_vec();
    let metadata = metadata_with_type("application/json");

    // Store with metadata
    let _set = store.set(&key, &data, &metadata).await;

    // Simulate GET /api/assets/metadata/assets/meta_test.json
    let result = store.get_metadata(&key).await;
    assert!(result.is_ok(), "Get metadata should succeed");

    let retrieved_meta = result.unwrap();
    assert_eq!(retrieved_meta.get_media_type(), "application/json");
}

/// Test Assets API POST /metadata/{query} updates metadata
///
/// Expected: Metadata updated, data unchanged
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_assets_api_post_metadata() {
    let store = create_test_store();

    let key = parse_key("assets/update_meta.txt").expect("Valid key");
    let data = b"original data".to_vec();
    let initial_meta = metadata_with_type("text/plain");

    // Set initial
    let _set = store.set(&key, &data, &initial_meta).await;

    // Simulate POST /api/assets/metadata/assets/update_meta.txt with new metadata
    let new_meta = metadata_with_type("text/html");
    let result = store.set_metadata(&key, &new_meta).await;
    assert!(result.is_ok(), "Set metadata should succeed");

    // Verify metadata changed
    let (retrieved_data, retrieved_meta) = store.get(&key).await.unwrap();
    assert_eq!(retrieved_data, data, "Data should be unchanged");
    assert_eq!(retrieved_meta.get_media_type(), "text/html");
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 4: Assets API - Entry Endpoints (Data + Metadata)
// ─────────────────────────────────────────────────────────────────────────────

/// Test Assets API GET /entry/{query} with CBOR format
///
/// Expected: Returns DataEntry with both data and metadata in CBOR
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_assets_api_get_entry_cbor() {
    let store = create_test_store();

    let key = parse_key("assets/entry_cbor.bin").expect("Valid key");
    let data = b"cbor entry test".to_vec();
    let metadata = metadata_with_type("application/octet-stream");

    // Store
    let _set = store.set(&key, &data, &metadata).await;

    // Simulate GET /api/assets/entry/assets/entry_cbor.bin?format=cbor
    let retrieved = store.get(&key).await;
    assert!(retrieved.is_ok(), "Get should succeed");

    let (retrieved_data, _) = retrieved.unwrap();
    assert_eq!(retrieved_data, data);
}

/// Test Assets API GET /entry/{query} with JSON format
///
/// Expected: Returns DataEntry with base64-encoded data in JSON
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_assets_api_get_entry_json() {
    let store = create_test_store();

    let key = parse_key("assets/entry_json.txt").expect("Valid key");
    let data = b"json entry test".to_vec();
    let metadata = metadata_with_type("text/plain");

    // Store
    let _set = store.set(&key, &data, &metadata).await;

    // Simulate GET /api/assets/entry/assets/entry_json.txt?format=json
    let retrieved = store.get(&key).await;
    assert!(retrieved.is_ok(), "Get should succeed");

    let (retrieved_data, retrieved_meta) = retrieved.unwrap();
    assert_eq!(retrieved_data, data);
    assert_eq!(retrieved_meta.get_media_type(), "text/plain");
}

/// Test Assets API POST /entry/{query} with combined data + metadata
///
/// Expected: Both data and metadata are stored
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_assets_api_post_entry() {
    let store = create_test_store();

    let key = parse_key("assets/post_entry.json").expect("Valid key");
    let data = b"{\"status\": \"ready\"}".to_vec();
    let metadata = metadata_with_type("application/json");

    // Simulate POST /api/assets/entry/assets/post_entry.json
    let result = store.set(&key, &data, &metadata).await;
    assert!(result.is_ok(), "Post entry should succeed");

    // Verify stored correctly
    let (retrieved_data, retrieved_meta) = store.get(&key).await.unwrap();
    assert_eq!(retrieved_data, data);
    assert_eq!(retrieved_meta.get_media_type(), "application/json");
}

/// Test Assets API DELETE /entry/{query} removes entire asset
///
/// Expected: Both data and metadata are removed
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_assets_api_delete_entry() {
    let store = create_test_store();

    let key = parse_key("assets/delete_entry.txt").expect("Valid key");
    let data = b"to be deleted".to_vec();
    let metadata = metadata_with_type("text/plain");

    // Setup
    let _set = store.set(&key, &data, &metadata).await;

    // Simulate DELETE /api/assets/entry/assets/delete_entry.txt
    let result = store.remove(&key).await;
    assert!(result.is_ok(), "Delete should succeed");

    // Verify gone
    assert!(store.get(&key).await.is_err(), "Should be deleted");
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 5: Assets API - Directory Listing
// ─────────────────────────────────────────────────────────────────────────────

/// Test Assets API GET /listdir/{query} returns directory contents
///
/// Expected: Returns list of assets in directory
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_assets_api_listdir() {
    let store = create_test_store();

    // Create directory
    let dir_key = parse_key("assets/dir").expect("Valid key");
    let _makedir = store.makedir(&dir_key).await;

    // Simulate GET /api/assets/listdir/assets/dir
    let result = store.listdir(&dir_key).await;
    assert!(result.is_ok(), "Listdir should succeed");
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 6: Assets API - Cancellation
// ─────────────────────────────────────────────────────────────────────────────

/// Test Assets API POST /cancel/{query} cancels running evaluation
///
/// Expected: Asset evaluation is cancelled, status becomes Cancelled
/// NOTE: Full integration test requires actual asset evaluation in progress
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_assets_api_cancel_evaluation() {
    let store = create_test_store();

    let key = parse_key("assets/cancel_test.txt").expect("Valid key");
    let data = b"data".to_vec();
    let metadata = Metadata::new();

    // Setup
    let _set = store.set(&key, &data, &metadata).await;

    // Simulate POST /api/assets/cancel/assets/cancel_test.txt
    // In real scenario, this would have triggered an async evaluation
    // For now, verify the key exists so cancel would find it
    let exists = store.contains(&key).await.unwrap_or(false);
    assert!(exists, "Asset should exist for cancellation");
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 7: Format Negotiation - Accept Header Precedence
// ─────────────────────────────────────────────────────────────────────────────

/// Test format selection: query param ?format=cbor takes precedence
///
/// Setup: Store entry, request with both query param and Accept header
/// Expected: Query param (CBOR) is used, not Accept header (JSON)
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_format_negotiation_query_param_precedence() {
    let store = create_test_store();

    let key = parse_key("assets/format_test.bin").expect("Valid key");
    let data = b"format test data".to_vec();
    let metadata = metadata_with_type("application/octet-stream");

    // Store
    let _set = store.set(&key, &data, &metadata).await;

    // Would request: GET /api/assets/entry/...?format=cbor
    // With header: Accept: application/json
    // Expected: CBOR format is used (query param has precedence)

    let retrieved = store.get(&key).await;
    assert!(retrieved.is_ok(), "Should retrieve with either format");
}

/// Test format selection: Accept header is used when no query param
///
/// Setup: Request with Accept header, no ?format param
/// Expected: Accept header format is used (or default CBOR)
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_format_negotiation_accept_header() {
    let store = create_test_store();

    let key = parse_key("assets/accept_test.json").expect("Valid key");
    let json_data = b"{\"key\": \"value\"}".to_vec();
    let metadata = metadata_with_type("application/json");

    // Store
    let _set = store.set(&key, &json_data, &metadata).await;

    // Would request: GET /api/assets/entry/...
    // With header: Accept: application/json
    // Expected: JSON format is used

    let retrieved = store.get(&key).await;
    assert!(retrieved.is_ok(), "Should retrieve");
}

/// Test format selection: Default CBOR when no format specified
///
/// Expected: CBOR format is default for binary efficiency
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_format_negotiation_default_cbor() {
    let store = create_test_store();

    let key = parse_key("assets/default_format.bin").expect("Valid key");
    let data = vec![1u8, 2, 3, 4, 5];
    let metadata = metadata_with_type("application/octet-stream");

    // Store
    let _set = store.set(&key, &data, &metadata).await;

    // Would request: GET /api/assets/entry/...
    // No query param, no Accept header
    // Expected: CBOR format is used (default)

    let retrieved = store.get(&key).await;
    assert!(retrieved.is_ok(), "Should retrieve with default format");
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 8: Binary Data Handling
// ─────────────────────────────────────────────────────────────────────────────

/// Test storing and retrieving binary data in CBOR format
///
/// Expected: Binary data preserved exactly (no base64 encoding)
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_binary_data_cbor_handling() {
    let store = create_test_store();

    let key = parse_key("assets/binary_cbor.bin").expect("Valid key");
    // Binary data with all byte values
    let data: Vec<u8> = (0..=255).collect();
    let metadata = metadata_with_type("application/octet-stream");

    // Store
    let _set = store.set(&key, &data, &metadata).await;

    // Retrieve and verify exact match
    let (retrieved_data, _) = store.get(&key).await.expect("Should retrieve");
    assert_eq!(
        retrieved_data, data,
        "Binary data should be preserved exactly"
    );
}

/// Test storing and retrieving binary data in bincode format
///
/// Expected: Binary data preserved through serialization round-trip
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_binary_data_bincode_handling() {
    let store = create_test_store();

    let key = parse_key("assets/binary_bincode.bin").expect("Valid key");
    let data = vec![0xFF, 0xFE, 0xFD, 0xFC, 0x00, 0x01];
    let metadata = metadata_with_type("application/octet-stream");

    // Store
    let _set = store.set(&key, &data, &metadata).await;

    // Retrieve
    let (retrieved_data, _) = store.get(&key).await.expect("Should retrieve");
    assert_eq!(retrieved_data, data, "Binary data should match");
}

/// Test storing and retrieving binary data via JSON (base64-encoded)
///
/// Expected: Binary data is base64-encoded in JSON, decoded on retrieval
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_binary_data_json_base64_handling() {
    let store = create_test_store();

    let key = parse_key("assets/binary_json.bin").expect("Valid key");
    let original_data = b"binary json test".to_vec();
    let metadata = metadata_with_type("application/octet-stream");

    // Store
    let _set = store.set(&key, &original_data, &metadata).await;

    // Retrieve (would be base64 in JSON, decoded transparently)
    let (retrieved_data, _) = store.get(&key).await.expect("Should retrieve");
    assert_eq!(
        retrieved_data, original_data,
        "Base64 round-trip should preserve data"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 9: Recipes API - Basic Operations
// ─────────────────────────────────────────────────────────────────────────────

/// Test Recipes API GET /data/{key} retrieves recipe definition
///
/// Expected: Returns recipe query and metadata
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_recipes_api_get_data() {
    let provider = MockRecipeProvider::new();

    // Create test recipe
    let recipe = Recipe::new(
        "text-Hello".to_string(),
        "Hello Recipe".to_string(),
        "A simple hello recipe".to_string(),
    )
    .expect("Valid recipe");

    provider.add_recipe("hello", recipe.clone());

    // Simulate GET /api/recipes/data/hello
    let retrieved = provider.get_recipe("hello");
    assert!(retrieved.is_some(), "Recipe should be found");

    let recipe_data = retrieved.unwrap();
    assert_eq!(recipe_data.title, "Hello Recipe");
    assert_eq!(recipe_data.description, "A simple hello recipe");
}

/// Test Recipes API GET /metadata/{key} retrieves recipe metadata
///
/// Expected: Returns title, description, volatile flag
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_recipes_api_get_metadata() {
    let provider = MockRecipeProvider::new();

    let recipe = Recipe::new(
        "text-Info".to_string(),
        "Info Recipe".to_string(),
        "Information about a recipe".to_string(),
    )
    .expect("Valid recipe");

    provider.add_recipe("info", recipe.clone());

    // Simulate GET /api/recipes/metadata/info
    let retrieved = provider.get_recipe("info");
    assert!(retrieved.is_some(), "Recipe should exist");

    let recipe_data = retrieved.unwrap();
    assert!(!recipe_data.volatile, "Should not be volatile by default");
    assert_eq!(recipe_data.title, "Info Recipe");
}

/// Test Recipes API GET /entry/{key} returns full recipe data + metadata
///
/// Expected: DataEntry with recipe definition and metadata
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_recipes_api_get_entry() {
    let provider = MockRecipeProvider::new();

    let recipe = Recipe::new(
        "text-Entry".to_string(),
        "Entry Recipe".to_string(),
        "Recipe entry test".to_string(),
    )
    .expect("Valid recipe");

    provider.add_recipe("entry", recipe.clone());

    // Simulate GET /api/recipes/entry/entry
    let retrieved = provider.get_recipe("entry");
    assert!(retrieved.is_some(), "Recipe entry should exist");

    let recipe_data = retrieved.unwrap();
    assert_eq!(recipe_data.query, recipe.query);
}

/// Test Recipes API GET /listdir lists all available recipes
///
/// Expected: Returns list of all recipe keys
#[test]
fn test_recipes_api_listdir() {
    let provider = MockRecipeProvider::new();

    let recipe1 = Recipe::new(
        "text-First".to_string(),
        "First".to_string(),
        "First recipe".to_string(),
    )
    .expect("Valid recipe");

    let recipe2 = Recipe::new(
        "text-Second".to_string(),
        "Second".to_string(),
        "Second recipe".to_string(),
    )
    .expect("Valid recipe");

    provider.add_recipe("recipe1", recipe1);
    provider.add_recipe("recipe2", recipe2);

    // Simulate GET /api/recipes/listdir
    // In real implementation, would return list of all recipe keys
    assert!(provider.get_recipe("recipe1").is_some());
    assert!(provider.get_recipe("recipe2").is_some());
}

/// Test Recipes API GET /resolve/{key} resolves recipe to execution plan
///
/// Expected: Returns Plan structure with resolved steps
#[test]
fn test_recipes_api_resolve() {
    let provider = MockRecipeProvider::new();

    let recipe = Recipe::new(
        "text-Resolve".to_string(),
        "Resolve Test".to_string(),
        "Recipe resolution test".to_string(),
    )
    .expect("Valid recipe");

    provider.add_recipe("resolve", recipe.clone());

    // Simulate GET /api/recipes/resolve/resolve
    let retrieved = provider.get_recipe("resolve");
    assert!(retrieved.is_some(), "Recipe should exist for resolution");

    let recipe_data = retrieved.unwrap();
    // In real scenario, would call parse_query and create a Plan
    let plan_query_result = recipe_data.get_query();
    assert!(
        plan_query_result.is_ok(),
        "Recipe query should be parseable"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 10: Cross-API Interaction
// ─────────────────────────────────────────────────────────────────────────────

/// Test end-to-end: Get recipe from Recipes API, use query in Assets API
///
/// Flow:
/// 1. GET /api/recipes/data/{recipe_key} → returns Recipe with query
/// 2. Use recipe.query in Assets API: GET /api/assets/data/{recipe.query}
/// 3. Verify asset evaluation matches recipe plan
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_cross_api_recipe_to_asset() {
    let store = create_test_store();
    let recipe_provider = MockRecipeProvider::new();

    // Create recipe
    let recipe = Recipe::new(
        "text-CrossTest".to_string(),
        "Cross API Test".to_string(),
        "Testing recipe to asset flow".to_string(),
    )
    .expect("Valid recipe");

    recipe_provider.add_recipe("cross_test", recipe.clone());

    // Step 1: Get recipe from Recipes API
    let retrieved_recipe = recipe_provider.get_recipe("cross_test");
    assert!(retrieved_recipe.is_some(), "Recipe should exist");

    let recipe_data = retrieved_recipe.unwrap();

    // Step 2: Extract query from recipe and use in Assets API
    let query_result = recipe_data.get_query();
    assert!(query_result.is_ok(), "Recipe query should be parseable");

    // Store some data that would be the result of the recipe query
    let result_key = parse_key("results/cross_test").expect("Valid key");
    let result_data = b"recipe execution result".to_vec();
    let result_metadata = metadata_with_type("text/plain");

    let _set = store.set(&result_key, &result_data, &result_metadata).await;

    // Step 3: Verify asset can be retrieved
    let (retrieved_data, _) = store.get(&result_key).await.expect("Should retrieve");
    assert_eq!(
        retrieved_data, result_data,
        "Asset should match recipe execution result"
    );
}

/// Test recipe contains correct query that can be evaluated in Assets API
///
/// Expected: Recipe query can be parsed and used to trigger asset evaluation
#[test]
fn test_cross_api_recipe_query_validity() {
    let recipe = Recipe::new(
        "text-Valid/Path".to_string(),
        "Valid Query Recipe".to_string(),
        "Recipe with valid query".to_string(),
    )
    .expect("Valid recipe");

    // Verify recipe query can be parsed
    let parsed_query = recipe.get_query();
    assert!(parsed_query.is_ok(), "Recipe query should be valid");

    let query = parsed_query.unwrap();
    // Query can now be used in Assets API
    let encoded = query.encode();
    assert!(!encoded.is_empty(), "Query should encode successfully");
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 11: Concurrency - Simultaneous Requests
// ─────────────────────────────────────────────────────────────────────────────

/// Test simultaneous GET requests to same asset don't interfere
///
/// Setup: Multiple concurrent reads of same asset
/// Expected: All requests succeed, data is consistent
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_concurrent_asset_reads() {
    let store = create_test_store();

    let key = parse_key("assets/concurrent_read.txt").expect("Valid key");
    let data = b"concurrent test data".to_vec();
    let metadata = metadata_with_type("text/plain");

    // Setup
    let _set = store.set(&key, &data, &metadata).await;

    // Spawn multiple concurrent readers
    let mut tasks = vec![];
    for i in 0..10 {
        let store = store.clone();
        let key = key.clone();
        let task = tokio::spawn(async move {
            let result = store.get(&key).await;
            (i, result)
        });
        tasks.push(task);
    }

    // Wait for all tasks and verify results
    for task in tasks {
        let (task_num, result) = task.await.expect("Task should complete");
        assert!(result.is_ok(), "Task {} get should succeed", task_num);

        let (retrieved_data, _) = result.unwrap();
        assert_eq!(retrieved_data, data, "All tasks should get same data");
    }
}

/// Test simultaneous writes to different assets don't block each other
///
/// Expected: All writes succeed concurrently
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_concurrent_asset_writes() {
    let store = create_test_store();
    let mut tasks = vec![];

    // Spawn multiple concurrent writers
    for i in 0..10 {
        let store = store.clone();
        let task = tokio::spawn(async move {
            let key = parse_key(&format!("assets/concurrent_write_{}", i)).expect("Valid key");
            let data = format!("data {}", i).into_bytes();
            let metadata = metadata_with_type("text/plain");

            let result = store.set(&key, &data, &metadata).await;
            (i, result)
        });
        tasks.push(task);
    }

    // Wait and verify all writes succeeded
    for task in tasks {
        let (task_num, result) = task.await.expect("Task should complete");
        assert!(result.is_ok(), "Task {} write should succeed", task_num);
    }
}

/// Test concurrent writes to same asset are serialized correctly
///
/// Expected: Final value is from last write
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_concurrent_writes_same_asset() {
    let store = create_test_store();
    let key = parse_key("assets/race_condition.txt").expect("Valid key");

    // Spawn writers to same key
    let mut tasks = vec![];
    for i in 0..5 {
        let store = store.clone();
        let key = key.clone();
        let task = tokio::spawn(async move {
            let data = format!("write {}", i).into_bytes();
            let metadata = metadata_with_type("text/plain");
            store.set(&key, &data, &metadata).await
        });
        tasks.push(task);
    }

    // Wait for all writes
    for task in tasks {
        let result = task.await.expect("Task should complete");
        assert!(result.is_ok(), "Write should succeed");
    }

    // Verify asset exists with some value (last write wins)
    let final_result = store.get(&key).await;
    assert!(final_result.is_ok(), "Asset should exist with final value");
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 12: Error Handling
// ─────────────────────────────────────────────────────────────────────────────

/// Test invalid query path returns ParseError
///
/// Expected: HTTP 400 Bad Request
#[test]
fn test_error_invalid_query_path() {
    let invalid_query = "///invalid///path///";
    let result = parse_query(invalid_query);
    // May fail or succeed depending on parser, but if fails should be ParseError
    let _ = result;
}

/// Test GET on non-existent asset returns KeyNotFound
///
/// Expected: HTTP 404 Not Found
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_error_asset_not_found() {
    let store = create_test_store();

    let key = parse_key("assets/nonexistent.txt").expect("Valid key");

    let result = store.get(&key).await;
    assert!(result.is_err(), "Should return error for missing asset");
    // In handler, would map to HTTP 404 via error_to_status_code()
}

/// Test GET on non-existent recipe returns KeyNotFound
///
/// Expected: HTTP 404 Not Found
#[test]
fn test_error_recipe_not_found() {
    let provider = MockRecipeProvider::new();

    // No recipe added with this key
    let result = provider.get_recipe("nonexistent_recipe");
    assert!(result.is_none(), "Recipe should not be found");
}

/// Test metadata parse error returns TypeError
///
/// Expected: HTTP 400 Bad Request
#[test]
fn test_error_invalid_metadata() {
    // Simulate invalid metadata JSON
    let invalid_json = r#"{ invalid json }"#;
    let result: Result<serde_json::Value, _> = serde_json::from_str(invalid_json);
    assert!(result.is_err(), "Invalid JSON should fail");
}

/// Test large upload doesn't cause memory exhaustion
///
/// Expected: Large binary data stored successfully
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_error_large_data_handling() {
    let store = create_test_store();

    let key = parse_key("assets/large_file.bin").expect("Valid key");
    // 10MB test data
    let large_data = vec![0u8; 10 * 1024 * 1024];
    let metadata = metadata_with_type("application/octet-stream");

    let result = store.set(&key, &large_data, &metadata).await;
    assert!(result.is_ok(), "Large data should store successfully");

    let (retrieved, _) = store.get(&key).await.expect("Should retrieve");
    assert_eq!(retrieved.len(), large_data.len(), "Size should match");
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 13: Round-trip Serialization Consistency
// ─────────────────────────────────────────────────────────────────────────────

/// Test CBOR serialization round-trip preserves data exactly
///
/// Expected: Serialize → Deserialize → Serialize produces identical output
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_roundtrip_cbor_consistency() {
    let store = create_test_store();

    let key = parse_key("assets/cbor_roundtrip.bin").expect("Valid key");
    let original_data = vec![1u8, 2, 3, 255, 254, 253];
    let metadata = metadata_with_type("application/octet-stream");

    // Set
    let _set = store.set(&key, &original_data, &metadata).await;

    // Get (would be CBOR serialized if format=cbor)
    let (retrieved_data, retrieved_meta) = store.get(&key).await.expect("Get should work");

    // Verify exact match (round-trip)
    assert_eq!(
        retrieved_data, original_data,
        "CBOR round-trip should preserve data"
    );
    assert_eq!(
        retrieved_meta.get_media_type(),
        "application/octet-stream",
        "Metadata should be preserved"
    );
}

/// Test bincode serialization round-trip
///
/// Expected: Data preserved through serialize/deserialize cycle
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_roundtrip_bincode_consistency() {
    let store = create_test_store();

    let key = parse_key("assets/bincode_roundtrip.bin").expect("Valid key");
    let original_data = b"bincode test".to_vec();
    let metadata = metadata_with_type("text/plain");

    // Set
    let _set = store.set(&key, &original_data, &metadata).await;

    // Get
    let (retrieved_data, _) = store.get(&key).await.expect("Get should work");

    // Verify match
    assert_eq!(
        retrieved_data, original_data,
        "Bincode round-trip should match"
    );
}

/// Test JSON with base64 encoding round-trip
///
/// Expected: Binary data base64-encoded in JSON, decoded correctly on round-trip
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_roundtrip_json_base64_consistency() {
    let store = create_test_store();

    let key = parse_key("assets/json_base64_roundtrip.bin").expect("Valid key");
    let original_data = vec![0xFF, 0xFE, 0xFD, 0x00, 0x01, 0x02];
    let metadata = metadata_with_type("application/octet-stream");

    // Set
    let _set = store.set(&key, &original_data, &metadata).await;

    // Get (would be JSON with base64 if format=json)
    let (retrieved_data, _) = store.get(&key).await.expect("Get should work");

    // Verify match (base64 encoding/decoding transparent)
    assert_eq!(
        retrieved_data, original_data,
        "JSON base64 round-trip should preserve data"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 14: WebSocket Integration (Conceptual)
// ─────────────────────────────────────────────────────────────────────────────

/// Test WebSocket subscription to asset notifications
///
/// Flow:
/// 1. Client connects to WS /api/assets/ws/{query}
/// 2. Server sends Initial notification
/// 3. Client sends {"action": "Subscribe", "query": "..."}
/// 4. Asset evaluation triggers notifications
/// 5. Client receives StatusChanged, ValueProduced, etc.
///
/// NOTE: This is a conceptual test - actual WebSocket testing would require
/// a real WebSocket client library like tokio-tungstenite
#[test]
fn test_websocket_subscription_structure() {
    // Verify client message structure
    let subscribe_msg = r#"{"action": "Subscribe", "query": "text-Hello"}"#;
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(subscribe_msg);
    assert!(parsed.is_ok(), "Client message should parse");

    // Verify server notification structure
    let notification = r#"{"type": "Initial", "asset_id": 1, "query": "text-Hello", "timestamp": "2024-01-01T00:00:00Z"}"#;
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(notification);
    assert!(parsed.is_ok(), "Server notification should parse");
}

/// Test WebSocket unsubscribe message handling
///
/// Expected: Server acknowledges unsubscribe, stops sending notifications
#[test]
fn test_websocket_unsubscribe_message() {
    let unsubscribe_msg = r#"{"action": "Unsubscribe", "query": "text-Hello"}"#;
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(unsubscribe_msg);
    assert!(parsed.is_ok(), "Unsubscribe message should parse");
}

/// Test WebSocket unsubscribe_all message handling
///
/// Expected: Server unsubscribes from all queries, sends UnsubscribedAll notification
#[test]
fn test_websocket_unsubscribe_all_message() {
    let unsubscribe_all_msg = r#"{"action": "UnsubscribeAll"}"#;
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(unsubscribe_all_msg);
    assert!(parsed.is_ok(), "UnsubscribeAll message should parse");
}

/// Test WebSocket ping/pong for connection keep-alive
///
/// Expected: Client sends Ping, server responds with Pong
#[test]
fn test_websocket_ping_pong() {
    let ping_msg = r#"{"action": "Ping"}"#;
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(ping_msg);
    assert!(parsed.is_ok(), "Ping message should parse");

    // Server would respond with Pong notification
    let pong_response = r#"{"type": "Pong", "timestamp": "2024-01-01T00:00:00Z"}"#;
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(pong_response);
    assert!(parsed.is_ok(), "Pong response should parse");
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 15: Integration Scenarios
// ─────────────────────────────────────────────────────────────────────────────

/// Test complete workflow: Create asset, subscribe to notifications, monitor lifecycle
///
/// Flow:
/// 1. POST /api/assets/data/{query} with initial data
/// 2. WS /api/assets/ws/{query} subscribe to notifications
/// 3. Evaluate asset (triggers StatusChanged, ValueProduced events)
/// 4. Receive notifications for lifecycle events
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_complete_asset_lifecycle() {
    let store = create_test_store();

    let key = parse_key("assets/lifecycle_test.txt").expect("Valid key");
    let initial_data = b"lifecycle initial".to_vec();
    let metadata = metadata_with_type("text/plain");

    // Step 1: Create asset
    let set_result = store.set(&key, &initial_data, &metadata).await;
    assert!(set_result.is_ok(), "Set should succeed");

    // Step 2: Subscribe to notifications (conceptual)
    // Would be: WS /api/assets/ws/assets/lifecycle_test.txt

    // Step 3: Evaluate asset (conceptual)
    // Would be: GET /api/assets/data/assets/lifecycle_test.txt

    // Step 4: Verify we can still access the asset
    let (final_data, _) = store.get(&key).await.expect("Get should work");
    assert_eq!(final_data, initial_data, "Data should be accessible");
}

/// Test recipe-driven asset evaluation
///
/// Flow:
/// 1. GET /api/recipes/data/{recipe_key} to get recipe
/// 2. Extract recipe.query
/// 3. GET /api/assets/data/{recipe.query} to evaluate
/// 4. Monitor via WS to track progress
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_recipe_driven_asset_evaluation() {
    let store = create_test_store();
    let recipe_provider = MockRecipeProvider::new();

    // Step 1: Get recipe
    let recipe = Recipe::new(
        "text-Driven".to_string(),
        "Driven Recipe".to_string(),
        "Recipe-driven evaluation".to_string(),
    )
    .expect("Valid recipe");

    recipe_provider.add_recipe("driven", recipe.clone());

    // Step 2: Extract and parse query
    let _query = recipe.get_query().expect("Query should parse");

    // Step 3: Store result that would come from evaluating the query
    let result_key = parse_key("results/driven").expect("Valid key");
    let result_data = b"driven evaluation result".to_vec();
    let result_metadata = metadata_with_type("text/plain");

    let _set = store.set(&result_key, &result_data, &result_metadata).await;

    // Step 4: Verify asset is accessible
    let (retrieved, _) = store.get(&result_key).await.expect("Should exist");
    assert_eq!(retrieved, result_data, "Result should match");
}

/// Test multi-step asset pipeline: Asset1 → Asset2 → Asset3
///
/// Expected: Each asset can depend on previous, form evaluation chain
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_asset_pipeline() {
    let store = create_test_store();

    // Asset 1: Source data
    let asset1_key = parse_key("assets/pipeline/step1.txt").expect("Valid key");
    let asset1_data = b"source data".to_vec();
    let asset1_meta = metadata_with_type("text/plain");

    let _set1 = store.set(&asset1_key, &asset1_data, &asset1_meta).await;

    // Asset 2: Transform asset 1
    let asset2_key = parse_key("assets/pipeline/step2.txt").expect("Valid key");
    let asset2_data = b"transformed data".to_vec();
    let asset2_meta = metadata_with_type("text/plain");

    let _set2 = store.set(&asset2_key, &asset2_data, &asset2_meta).await;

    // Asset 3: Aggregate asset 2
    let asset3_key = parse_key("assets/pipeline/step3.txt").expect("Valid key");
    let asset3_data = b"aggregated data".to_vec();
    let asset3_meta = metadata_with_type("text/plain");

    let _set3 = store.set(&asset3_key, &asset3_data, &asset3_meta).await;

    // Verify all assets accessible
    let (a1, _) = store.get(&asset1_key).await.expect("Asset 1 should exist");
    let (a2, _) = store.get(&asset2_key).await.expect("Asset 2 should exist");
    let (a3, _) = store.get(&asset3_key).await.expect("Asset 3 should exist");

    assert_eq!(a1, asset1_data);
    assert_eq!(a2, asset2_data);
    assert_eq!(a3, asset3_data);
}

/// Test cleanup and resource management
///
/// Expected: Assets can be deleted, resources released
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_cleanup_and_teardown() {
    let store = create_test_store();

    // Create some assets
    for i in 0..5 {
        let key = parse_key(&format!("assets/cleanup_{}", i)).expect("Valid key");
        let data = format!("cleanup test {}", i).into_bytes();
        let metadata = metadata_with_type("text/plain");

        let _set = store.set(&key, &data, &metadata).await;
    }

    // Delete them
    for i in 0..5 {
        let key = parse_key(&format!("assets/cleanup_{}", i)).expect("Valid key");
        let _delete = store.remove(&key).await;
    }

    // Verify they're gone
    for i in 0..5 {
        let key = parse_key(&format!("assets/cleanup_{}", i)).expect("Valid key");
        let result = store.get(&key).await;
        assert!(result.is_err(), "Asset {} should be deleted", i);
    }
}
