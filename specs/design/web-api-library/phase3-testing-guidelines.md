# Phase 3: Testing Guidelines & Best Practices - Web API Library

## Test Organization

### File Structure

```
liquers-axum/
├── src/
│   ├── lib.rs
│   ├── core/
│   │   ├── response.rs
│   │   ├── error.rs
│   │   └── format.rs
│   ├── query/
│   │   ├── builder.rs
│   │   └── handlers.rs
│   └── store/
│       ├── builder.rs
│       └── handlers.rs
├── tests/                          # Integration tests (not unit tests)
│   ├── query_api_integration.rs    # Query API end-to-end tests (27 tests)
│   ├── store_api_integration.rs    # Store API end-to-end tests (31 tests)
│   └── common/                     # Shared helpers (if needed)
│       └── mod.rs
└── examples/                       # Runnable examples
    └── basic_server.rs
```

**Rationale:**
- Integration tests in `tests/` directory (compiled separately, test framework only)
- Unit tests (if any) stay in `src/` with `#[cfg(test)]` modules
- Common test helpers in `tests/common/mod.rs`

### Test Naming Convention

**Pattern:** `test_<module>_<scenario>_<expected_outcome>`

**Examples:**
```rust
test_query_parsing_valid_queries()           // Module: query, Scenario: parsing, Outcome: valid
test_store_set_get_data()                    // Module: store, Scenario: CRUD, Outcome: round-trip success
test_concurrent_writes()                     // Module: concurrent, Scenario: writes, Outcome: completion
test_error_key_not_found_error()             // Module: error, Scenario: not found, Outcome: error correct
```

**Grouping:** Tests organized by capability:
1. Basic functionality
2. Error handling
3. Data type handling
4. Concurrency
5. Edge cases

---

## Test Framework Configuration

### Tokio Runtime Setup

All tests use async with tokio:

```rust
#[tokio::test]
async fn test_basic() {
    // Single-threaded, uses current task
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_concurrent() {
    // Multi-threaded, 2 workers
    // Used for concurrency tests only
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_heavy_concurrency() {
    // More workers for high-concurrency scenarios
}
```

**Why tokio::test over #[test]:**
- Async/await support required for all handlers
- Implicit runtime creation and cleanup
- Simpler than manual `tokio::block_on()`

### Common Test Setup

For tests requiring shared infrastructure:

```rust
// At module level
fn setup_env() -> TestEnvironment {
    TestEnvironment::new()
}

fn setup_store() -> Arc<AsyncStoreRouter> {
    Arc::new(AsyncStoreRouter::new())
}

// In individual tests
#[tokio::test]
async fn test_example() {
    let env = setup_env();
    let store = setup_store();
    // test code
}
```

---

## Error Handling in Tests

### DO: Explicit Result Handling

```rust
#[tokio::test]
async fn test_store_operation() {
    let store = Arc::new(AsyncStoreRouter::new());
    let key = Key::from_string("test/key").expect("Valid key");
    let data = b"test".to_vec();
    let metadata = Metadata::new();

    // ✅ GOOD: Explicit expectation message
    let set_result = store.set(&key, &data, &metadata).await;
    assert!(set_result.is_ok(), "Set operation should succeed");

    // ✅ GOOD: Pattern match with specific assertion
    let get_result = store.get(&key).await;
    match get_result {
        Ok((retrieved, _)) => assert_eq!(retrieved, data),
        Err(e) => panic!("Get should succeed, but got: {:?}", e),
    }
}
```

### DON'T: Unwrap in Tests

```rust
#[tokio::test]
async fn test_bad_example() {
    let store = Arc::new(AsyncStoreRouter::new());

    // ❌ BAD: Unwrap without context
    let (data, _) = store.get(&key).await.unwrap();  // No message if fails!

    // ❌ BAD: Expect also unhelpful
    let key = Key::from_string("test/key").expect("Key parsing failed");  // Too generic
}
```

### Pattern: Assert with Context

```rust
#[tokio::test]
async fn test_with_context() {
    let result = some_operation().await;

    assert!(
        result.is_ok(),
        "Operation should succeed for key={:?}, got error: {:?}",
        expected_key,
        result.err()
    );
}
```

---

## Handling Optional/Error Cases

### Testing Error Paths Explicitly

```rust
#[tokio::test]
async fn test_error_path() {
    let store = Arc::new(AsyncStoreRouter::new());
    let nonexistent_key = Key::from_string("does/not/exist").unwrap();

    let result = store.get(&nonexistent_key).await;

    // ✅ GOOD: Verify error occurred
    assert!(result.is_err(), "Getting nonexistent key should fail");

    // ✅ GOOD: Verify error type (if extractable)
    if let Err(e) = result {
        assert_eq!(e.error_type, ErrorType::KeyNotFound);
    }
}
```

### Testing Success Paths After Setup

```rust
#[tokio::test]
async fn test_success_path() {
    let store = Arc::new(AsyncStoreRouter::new());
    let key = Key::from_string("test/key").expect("Valid key");
    let data = b"test".to_vec();
    let metadata = Metadata::new();

    // Setup (may fail, but test will abort)
    let _setup = store.set(&key, &data, &metadata)
        .await
        .expect("Setup: store.set should succeed");

    // Actual test
    let result = store.get(&key).await;
    assert!(result.is_ok());
    let (retrieved, _) = result.unwrap();
    assert_eq!(retrieved, data);
}
```

### When unwrap() is Acceptable

1. **Setup phase:** Creating test data, if it fails, test should fail
   ```rust
   let key = Key::from_string("test/key").expect("Valid test key syntax");
   ```

2. **Unambiguous test requirements:** If operation should never fail
   ```rust
   let (data, _) = store.get(&existing_key).await.expect("Known key should exist");
   ```

3. **Temporary debugging:** With explicit comment
   ```rust
   let result = operation().await.expect("DEBUG: fix this");  // TODO: proper error handling
   ```

**NEVER:** Unwrap external results without context. Always use `expect()` with message.

---

## Concurrency Testing

### Multi-threaded Test Template

```rust
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_concurrent_operations() {
    let shared = Arc::new(SomeSharedState::new());
    let mut tasks = vec![];

    // Spawn multiple tasks
    for i in 0..10 {
        let shared = shared.clone();
        let task = tokio::spawn(async move {
            // Each task does independent work
            operation(&shared, i).await
        });
        tasks.push(task);
    }

    // Wait for all tasks to complete
    for task in tasks {
        let result = task.await.expect("Task should complete");
        // Optionally verify result
        assert!(result.is_ok());
    }
}
```

### Key Patterns

1. **Arc for shared state:** `Arc<Mutex<State>>` or `Arc<State>` if interior mutability used
2. **tokio::spawn for async tasks:** Allows true concurrency
3. **Collect tasks in Vec:** For joining all before assertions
4. **Each task is independent:** No shared mutable references between tasks
5. **Assertions after tasks complete:** Verify correctness after all are done

### Deadlock Prevention

```rust
// ✅ GOOD: No locks held across spawn boundaries
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_no_deadlock() {
    let shared = Arc::new(tokio::sync::Mutex::new(State::new()));

    let task1 = {
        let shared = shared.clone();
        tokio::spawn(async move {
            let mut state = shared.lock().await;  // Lock released after block
            operation(&mut state).await
            // Lock drops here, not held across yield point
        })
    };

    let task2 = {
        let shared = shared.clone();
        tokio::spawn(async move {
            let mut state = shared.lock().await;
            operation(&mut state).await
        })
    };

    task1.await.expect("Task 1");
    task2.await.expect("Task 2");
}
```

### Race Condition Testing

```rust
// ✅ GOOD: Tests last-write-wins semantics
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_concurrent_writes_race() {
    let store = Arc::new(AsyncStoreRouter::new());
    let key = Key::from_string("test/race").unwrap();

    let mut tasks = vec![];
    for i in 0..5 {
        let store = store.clone();
        let task = tokio::spawn(async move {
            let data = format!("value{}", i).into_bytes();
            store.set(&key, &data, &Metadata::new()).await
        });
        tasks.push((i, task));
    }

    for (i, task) in tasks {
        let result = task.await.expect("Set task");
        assert!(result.is_ok(), "Write {} should succeed", i);
    }

    // One of the 5 writes will be final (last-write-wins)
    // Verification: store has exactly one value (length check)
    let (retrieved, _) = store.get(&key).await.expect("Get should work");
    assert!(retrieved.len() > 0);
    // Could parse to verify which write won, but implementation-dependent
}
```

---

## Data Type Testing

### Comprehensive Value Coverage

```rust
#[tokio::test]
async fn test_value_string() {
    let v = Value::from("test string");
    assert_eq!(v.to_string(), "test string");
    let _bytes = v.to_bytes();
}

#[tokio::test]
async fn test_value_numeric() {
    let v = Value::from(42i32);
    let _bytes = v.to_bytes();
    // Verify numeric serialization
}

#[tokio::test]
async fn test_value_empty() {
    let v = Value::from("");
    let _bytes = v.to_bytes();
    // Empty string should serialize
}

#[tokio::test]
async fn test_value_large() {
    let large = "x".repeat(1024 * 1024);
    let v = Value::from(large);
    let bytes = v.to_bytes();
    assert!(bytes.len() > 100_000);
}
```

### Metadata Testing

```rust
#[tokio::test]
async fn test_metadata_empty() {
    let m = Metadata::new();
    assert_eq!(m.get_media_type(), None);  // Default empty
}

#[tokio::test]
async fn test_metadata_set_get() {
    let mut m = Metadata::new();
    m.set_media_type("application/json");
    assert_eq!(m.get_media_type(), Some("application/json".to_string()));
}

#[tokio::test]
async fn test_metadata_update() {
    let mut m = Metadata::new();
    m.set_media_type("text/plain");
    m.set_media_type("application/json");  // Update
    assert_eq!(m.get_media_type(), Some("application/json".to_string()));
}
```

---

## Serialization Round-trip Testing

### Format-Specific Round-trips

```rust
#[tokio::test]
async fn test_roundtrip_cbor() {
    let original = vec![1u8, 2, 3, 4, 5];

    // Serialize
    let serialized = ciborium::ser::into_writer(&original, Vec::new())
        .expect("CBOR serialize");

    // Deserialize
    let deserialized: Vec<u8> = ciborium::de::from_reader(&serialized[..])
        .expect("CBOR deserialize");

    // Compare
    assert_eq!(deserialized, original, "CBOR should preserve data exactly");
}
```

### Testing All Formats Together

```rust
#[tokio::test]
async fn test_all_formats_round_trip() {
    let metadata = Metadata::new();
    let data = b"test data for all formats".to_vec();

    for format in &[
        SerializationFormat::Cbor,
        SerializationFormat::Bincode,
        SerializationFormat::Json,
    ] {
        let serialized = serialize_data_entry(&entry, *format)
            .expect(&format!("Serialization for {:?}", format));

        let deserialized = deserialize_data_entry(&serialized, *format)
            .expect(&format!("Deserialization for {:?}", format));

        assert_eq!(
            deserialized.data, data,
            "Round-trip for {:?} should preserve data",
            format
        );
    }
}
```

---

## Error Type Coverage

### Complete ErrorType Testing

```rust
macro_rules! test_error_type {
    ($error_type:expr, $status:expr, $test_name:ident) => {
        #[tokio::test]
        async fn $test_name() {
            let error = Error::from_error($error_type, "test message");
            assert_eq!(error.error_type, $error_type);

            // Verify mapping (if testing error_to_status_code function)
            let status = error_to_status_code($error_type);
            assert_eq!(status, $status);
        }
    };
}

test_error_type!(ErrorType::KeyNotFound, StatusCode::NOT_FOUND, test_key_not_found);
test_error_type!(ErrorType::ParseError, StatusCode::BAD_REQUEST, test_parse_error);
test_error_type!(ErrorType::SerializationError, StatusCode::UNPROCESSABLE_ENTITY, test_serialization_error);
test_error_type!(ErrorType::KeyReadError, StatusCode::INTERNAL_SERVER_ERROR, test_read_error);
// ... etc for all variants
```

### Error Response Structure

```rust
#[tokio::test]
async fn test_error_response_structure() {
    let error = Error::key_not_found(&Key::new());
    let response = error_response::<()>(&error);

    assert_eq!(response.status, ResponseStatus::Error);
    assert!(response.result.is_none());
    assert!(response.error.is_some());

    let detail = response.error.unwrap();
    assert_eq!(detail.error_type, "KeyNotFound");
    assert!(!detail.message.is_empty());
}
```

---

## Large Data Testing

### Stress Testing Guidelines

```rust
#[tokio::test]
async fn test_large_data_1mb() {
    // 1MB is safe for in-memory stores
    let large_data = vec![0u8; 1024 * 1024];

    let store = Arc::new(AsyncStoreRouter::new());
    let key = Key::from_string("test/large").unwrap();
    let metadata = Metadata::new();

    // Should not hang or panic
    let set_result = store.set(&key, &large_data, &metadata).await;
    assert!(set_result.is_ok());

    let (retrieved, _) = store.get(&key).await.unwrap();
    assert_eq!(retrieved.len(), 1024 * 1024);
}

#[tokio::test]
async fn test_large_data_10mb() {
    // 10MB is upper practical limit without streaming
    // If this hangs or OOMs, need streaming implementation
    let large_data = vec![0u8; 10 * 1024 * 1024];

    // Same test structure as 1MB
}

// ⚠️ DO NOT TEST:
// #[tokio::test]
// async fn test_large_data_100mb() {
//     // Would require streaming support (Phase 3b)
//     // Single-pass loading unacceptable
//     let huge = vec![0u8; 100 * 1024 * 1024];
//     // This test would fail or hang
// }
```

---

## Network-layer Assumptions

### What's NOT Tested

Network-layer issues cannot be tested without actual HTTP server:

1. **TCP connection drops**
   - Would require real socket and test harness
   - Covered by integration tests with `axum::test::TestClient` (Phase 4)

2. **Timeout handling**
   - HTTP request timeout
   - Response body timeout
   - Requires tower middleware testing

3. **Partial uploads**
   - Multipart upload interruption
   - Requires network simulation

4. **Header size limits**
   - Large Cookie/Authorization headers
   - Out of scope for handler logic

### What IS Tested

Pure handler logic testable:

1. Handler parsing and logic ✅
2. Error propagation and mapping ✅
3. Data serialization and conversion ✅
4. Concurrency at task level ✅
5. Memory usage patterns ✅

---

## Test Isolation

### Avoiding Test Pollution

```rust
// ✅ GOOD: Each test has fresh state
#[tokio::test]
async fn test_independent_1() {
    let store = Arc::new(AsyncStoreRouter::new());  // Fresh store
    // test 1
}

#[tokio::test]
async fn test_independent_2() {
    let store = Arc::new(AsyncStoreRouter::new());  // Fresh store
    // test 2
}

// ❌ BAD: Shared state across tests (Rust doesn't allow this anyway)
static SHARED_STORE: Lazy<Arc<AsyncStoreRouter>> = Lazy::new(|| {
    Arc::new(AsyncStoreRouter::new())
});
```

Rust test framework runs each `#[tokio::test]` in separate task/thread, so isolation is natural.

### Test-specific Keys

Always use unique keys to avoid collisions:

```rust
#[tokio::test]
async fn test_scenario_1() {
    let key = Key::from_string("test/scenario_1/data").unwrap();
    // Won't collide with other tests
}

#[tokio::test]
async fn test_scenario_2() {
    let key = Key::from_string("test/scenario_2/data").unwrap();
    // Won't collide with other tests
}
```

---

## Test Documentation

### Expected Failures

Document why certain scenarios can't be tested:

```rust
#[tokio::test]
async fn test_streaming_large_data() {
    // TODO: Requires Phase 3b streaming implementation
    // Currently returns 422 for >100MB results
    // When streaming is available, test:
    // 1. Request for 100MB+ data
    // 2. Response is chunked (Transfer-Encoding: chunked)
    // 3. Client receives all chunks correctly
}
```

### Test Preconditions

Document what each test assumes:

```rust
#[tokio::test]
async fn test_store_concurrent_writes() {
    // Assumes: AsyncStoreRouter uses key-level locking
    // If: No locking, may see data corruption
    // Verifies: 5 concurrent writes complete without errors

    // ... test code
}
```

---

## CI/CD Integration

### Running Tests Locally

```bash
# All tests
cargo test -p liquers-axum

# Specific test file
cargo test -p liquers-axum --test query_api_integration

# Specific test
cargo test -p liquers-axum test_store_set_get_data -- --exact

# With output
cargo test -p liquers-axum -- --nocapture --test-threads=1

# Performance profiling
cargo test -p liquers-axum --release
```

### Expected Test Duration

- Query API tests: ~2-5 seconds (27 tests)
- Store API tests: ~3-8 seconds (31 tests)
- Total: <15 seconds for full suite

If slower, likely issues:
- Timeouts not implemented efficiently
- Excessive allocations in hot paths
- Lock contention in concurrent tests

---

## Test Maintenance

### Adding New Tests

1. **Choose category:** Query API, Store API, or Integration
2. **Name clearly:** Follow `test_<module>_<scenario>` pattern
3. **Add to appropriate file:** `query_api_integration.rs` or `store_api_integration.rs`
4. **Document preconditions:** Comments explaining what's tested
5. **Handle errors explicitly:** No `unwrap()` without context

### Updating Tests for API Changes

If handlers change:
1. Update test expectations
2. Verify error mappings still correct
3. Add new tests for new behavior
4. Remove tests for removed features

Example:
```rust
// Old: handler returned 400 for ParseError
// New: handler now validates key format earlier
// Updated test:
#[tokio::test]
async fn test_query_key_validation() {
    let error = parse_query("invalid\x00key");
    assert!(error.is_err());
    // Error now happens in parse, not in handler
}
```

---

## Summary Checklist

Before committing tests:

- [ ] All tests have clear names (`test_<module>_<scenario>`)
- [ ] No `unwrap()` without `expect()` message
- [ ] Explicit error assertions (`.is_ok()`, `.is_err()`)
- [ ] Async tests use `#[tokio::test]`
- [ ] Concurrency tests use `worker_threads = 2+`
- [ ] Large data tests limit to <10MB
- [ ] Comments explain preconditions and expected behavior
- [ ] Error paths tested explicitly
- [ ] Round-trip serialization tested all formats
- [ ] Tests run in <15 seconds total
- [ ] No test pollution (isolated state per test)
- [ ] Documentation for NOT-testable scenarios

---

## References

- Test files: `liquers-axum/tests/query_api_integration.rs`, `liquers-axum/tests/store_api_integration.rs`
- Framework: `tokio::test` with `#[tokio::test]` macro
- Assertions: Standard Rust `assert!()`, `assert_eq!()`, `panic!()`
- Error types: `liquers_core::error::{Error, ErrorType}`
- Async: `tokio::spawn()` for concurrency, `.await` for async
