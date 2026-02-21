# Phase 3: Unit Tests Implementation Guide

## Overview

This guide provides step-by-step instructions for implementing the 106 unit tests specified in `PHASE3-UNIT-TESTS.md` across three core modules of the Web API Library.

## Module Files to Create/Modify

### 1. `liquers-axum/src/core/response.rs`

**Purpose:** Test response types and their serialization/deserialization.

**Test Count:** 30 tests

**Tests by Category:**

#### Happy Path (18 tests)

| Test | Purpose |
|------|---------|
| `test_ok_response_with_string_result` | Verify ok_response() creates correct ApiResponse with string |
| `test_ok_response_with_integer_result` | Verify ok_response() with integer result type |
| `test_ok_response_with_json_value` | Verify ok_response() with serde_json::Value |
| `test_ok_response_serialization_json` | Verify JSON serialization includes status, result, message |
| `test_api_response_skips_none_fields` | Verify #[serde(skip_serializing_if)] omits None fields |
| `test_error_detail_serialization` | Verify ErrorDetail serialization with all fields |
| `test_error_detail_type_field_renamed` | Verify "type" field name in JSON (not error_type) |
| `test_data_entry_construction` | Verify DataEntry creation with metadata and data |
| `test_data_entry_with_large_data` | Verify DataEntry handles 1MB data |
| `test_binary_response_construction` | Verify BinaryResponse creation |
| `test_response_status_ok_serialization` | Verify ResponseStatus::Ok → "OK" |
| `test_response_status_error_serialization` | Verify ResponseStatus::Error → "ERROR" |
| `test_error_to_detail_key_not_found` | Verify error_to_detail() with KeyNotFound |
| `test_error_to_detail_parse_error` | Verify error_to_detail() with ParseError |
| `test_error_response_structure` | Verify error_response() builds correct structure |
| `test_error_response_serialization` | Verify error response JSON structure |
| `test_ok_response_into_response` | Verify IntoResponse for ApiResponse returns StatusCode::OK |
| `test_binary_response_into_response` | Verify IntoResponse for BinaryResponse |

#### Error Path (5 tests)

| Test | Purpose |
|------|---------|
| `test_error_response_with_large_error` | Verify error_response() handles large messages |
| `test_api_response_with_invalid_utf8` | Verify error handling for non-UTF8 data |
| `test_data_entry_serialization_failure` | Verify graceful handling of serialization failures |
| (2 more specific error scenarios) | Based on actual error handling in functions |

#### Edge Cases (7 tests)

| Test | Purpose |
|------|---------|
| `test_ok_response_with_empty_string` | Empty string result |
| `test_ok_response_with_unicode_characters` | Unicode (日本語, emoji) in results |
| `test_data_entry_with_empty_data` | Empty data vector |
| `test_data_entry_with_null_bytes` | Null bytes in binary data (0x00, 0xFF) |
| `test_error_detail_with_all_fields` | ErrorDetail with traceback and metadata |
| `test_response_status_copy_trait` | Copy semantics for ResponseStatus |
| `test_api_response_clone` | Clone implementation for ApiResponse |
| `test_api_response_deserialization` | Round-trip JSON deserialization |
| `test_data_entry_round_trip_json` | DataEntry JSON serialization/deserialization |

**Implementation Checklist:**

- [ ] Create `response.rs` with all struct definitions
- [ ] Implement `ok_response<T>()` function
- [ ] Implement `error_response<T>()` function
- [ ] Implement `error_to_detail()` function
- [ ] Implement IntoResponse for ApiResponse<T>
- [ ] Implement IntoResponse for BinaryResponse
- [ ] Implement IntoResponse for DataEntry
- [ ] Create base64 serde module with human_readable check
- [ ] Add all 30 tests in #[cfg(test)] mod tests
- [ ] Run tests: `cargo test --lib core::response`
- [ ] Verify all tests pass ✓

---

### 2. `liquers-axum/src/core/error.rs`

**Purpose:** Test error type mapping to HTTP status codes.

**Test Count:** 38 tests

**Tests by Category:**

#### Happy Path (22 tests)

| Test | Purpose |
|------|---------|
| `test_key_not_found_maps_to_404` | ErrorType::KeyNotFound → 404 |
| `test_parse_error_maps_to_400` | ErrorType::ParseError → 400 |
| `test_unknown_command_maps_to_400` | ErrorType::UnknownCommand → 400 |
| `test_parameter_error_maps_to_400` | ErrorType::ParameterError → 400 |
| `test_argument_missing_maps_to_400` | ErrorType::ArgumentMissing → 400 |
| `test_action_not_registered_maps_to_400` | ErrorType::ActionNotRegistered → 400 |
| `test_command_already_registered_maps_to_409` | ErrorType::CommandAlreadyRegistered → 409 |
| `test_too_many_parameters_maps_to_400` | ErrorType::TooManyParameters → 400 |
| `test_conversion_error_maps_to_422` | ErrorType::ConversionError → 422 |
| `test_serialization_error_maps_to_422` | ErrorType::SerializationError → 422 |
| `test_key_read_error_maps_to_500` | ErrorType::KeyReadError → 500 |
| `test_key_write_error_maps_to_500` | ErrorType::KeyWriteError → 500 |
| `test_execution_error_maps_to_500` | ErrorType::ExecutionError → 500 |
| `test_unexpected_error_maps_to_500` | ErrorType::UnexpectedError → 500 |
| `test_general_error_maps_to_500` | ErrorType::General → 500 |
| `test_cache_not_supported_maps_to_501` | ErrorType::CacheNotSupported → 501 |
| `test_not_supported_maps_to_501` | ErrorType::NotSupported → 501 |
| `test_not_available_maps_to_404` | ErrorType::NotAvailable → 404 |
| `test_key_not_supported_maps_to_404` | ErrorType::KeyNotSupported → 404 |
| `test_parse_error_type_key_not_found` | parse_error_type("KeyNotFound") → Some(...) |
| `test_parse_error_type_parse_error` | parse_error_type("ParseError") → Some(...) |
| `test_parse_error_type_all_variants` | All 19 variants parse correctly |

#### Error Path (6 tests)

| Test | Purpose |
|------|---------|
| `test_parse_error_type_unknown_variant` | Unknown string returns None |
| `test_parse_error_type_empty_string` | Empty string returns None |
| `test_parse_error_type_wrong_case` | Case sensitivity ("parseerror" fails) |
| `test_parse_error_type_with_leading_whitespace` | Whitespace validation |
| `test_parse_error_type_with_trailing_whitespace` | Trailing whitespace fails |
| (1 more edge case) | Special characters or invalid formats |

#### Edge Cases (10 tests)

| Test | Purpose |
|------|---------|
| `test_status_code_values_are_correct` | Verify numeric HTTP status codes |
| `test_all_error_types_have_mapping` | All variants explicitly mapped (no default) |
| `test_parse_and_map_roundtrip` | Parse error type, then map to status |
| `test_bad_request_errors_are_grouped` | All 400 errors grouped together |
| `test_server_error_types_are_grouped` | All 500 errors grouped together |
| `test_not_found_errors_are_grouped` | All 404 errors grouped together |
| `test_conflict_error_unique` | CommandAlreadyRegistered → 409 (unique) |
| `test_unprocessable_errors_grouped` | All 422 errors grouped |
| `test_not_implemented_grouped` | All 501 errors grouped |
| (1 more semantic test) | Error type semantics validation |

**Implementation Checklist:**

- [ ] Create `error.rs` with error_to_status_code() function
- [ ] Implement error_to_status_code() with explicit match for all 19 ErrorType variants
- [ ] Implement parse_error_type() with explicit match for all 19 string variants
- [ ] Verify no default match arm (`_ =>`) - compile should enforce this
- [ ] Add all 38 tests in #[cfg(test)] mod tests
- [ ] Run tests: `cargo test --lib core::error`
- [ ] Verify all tests pass ✓

---

### 3. `liquers-axum/src/core/format.rs`

**Purpose:** Test format selection and serialization/deserialization.

**Test Count:** 38 tests

**Tests by Category:**

#### Happy Path (25 tests)

| Test | Purpose |
|------|---------|
| `test_parse_format_param_cbor` | parse_format_param("cbor") → Ok(Cbor) |
| `test_parse_format_param_bincode` | parse_format_param("bincode") → Ok(Bincode) |
| `test_parse_format_param_json` | parse_format_param("json") → Ok(Json) |
| `test_parse_format_param_cbor_uppercase` | Case insensitive: "CBOR" → Ok(Cbor) |
| `test_parse_format_param_mixed_case` | Case insensitive: "CbOr" → Ok(Cbor) |
| `test_select_format_defaults_to_cbor` | No header/param → Cbor (default) |
| `test_select_format_from_query_param_cbor` | ?format=cbor → Cbor |
| `test_select_format_from_query_param_json` | ?format=json → Json |
| `test_select_format_query_param_takes_precedence` | Query param overrides Accept header |
| `test_select_format_from_accept_header_json` | Accept: application/json → Json |
| `test_select_format_from_accept_header_bincode` | Accept: application/x-bincode → Bincode |
| `test_select_format_from_accept_header_cbor` | Accept: application/cbor → Cbor |
| `test_select_format_accept_with_quality_weights` | Parse q=0.9;q=0.8 correctly |
| `test_select_format_accept_with_charset` | application/json;charset=utf-8 works |
| `test_serialize_data_entry_cbor` | Serialize to CBOR produces non-empty bytes |
| `test_serialize_data_entry_bincode` | Serialize to bincode produces non-empty bytes |
| `test_serialize_data_entry_json` | Serialize to JSON produces valid JSON string |
| `test_deserialize_data_entry_cbor_roundtrip` | CBOR round-trip preserves data |
| `test_deserialize_data_entry_bincode_roundtrip` | bincode round-trip preserves data |
| `test_deserialize_data_entry_json_roundtrip` | JSON round-trip preserves data |
| `test_json_uses_base64_encoding` | JSON data encoded as base64 (AQIDBAU) |
| `test_select_format_case_insensitive_header` | HeaderMap handles case-insensitive keys |
| `test_multiple_formats_round_trip_same_data` | Same data round-trips in all formats |
| `test_select_format_wildcard_accept` | */* defaults to Cbor |
| `test_select_format_multiple_accept_types` | Complex Accept header parsed correctly |

#### Error Path (8 tests)

| Test | Purpose |
|------|---------|
| `test_parse_format_param_invalid_format` | Unknown format → Err(NotSupported) |
| `test_parse_format_param_empty_string` | "" → Err(...) |
| `test_select_format_with_invalid_query_param_defaults_to_cbor` | Bad ?format= → default Cbor |
| `test_deserialize_corrupted_cbor_data` | Invalid CBOR bytes → Err(...) |
| `test_deserialize_corrupted_bincode_data` | Invalid bincode bytes → Err(...) |
| `test_deserialize_corrupted_json_data` | Invalid JSON bytes → Err(...) |
| `test_serialize_invalid_metadata` | Invalid metadata → Err(...) |
| (1 more serialization error) | Edge case in serialization |

#### Edge Cases (15 tests)

| Test | Purpose |
|------|---------|
| `test_serialization_format_copy_trait` | Copy semantics for SerializationFormat |
| `test_serialize_empty_data` | Empty data vector in all 3 formats |
| `test_serialize_large_data` | 1MB data serialization (all formats) |
| `test_serialize_with_all_byte_values` | Bytes 0x00-0xFF preserve correctly |
| `test_deserialize_empty_data` | Empty data deserialization |
| `test_json_base64_with_special_chars` | Special bytes (0x00, 0xFF) in base64 |
| `test_select_format_wildcard_accept` | Accept: */* defaults to Cbor |
| `test_select_format_multiple_accept_types` | Parse complex Accept header |
| `test_serialization_format_comparison` | Equality/inequality operations |
| `test_data_entry_with_metadata` | Metadata preserved in roundtrip |
| `test_bincode_preserves_binary_data` | No base64 in bincode (raw bytes) |
| `test_cbor_preserves_binary_data` | No base64 in CBOR (raw bytes) |
| `test_base64_module_human_readable_check` | Serde is_human_readable() logic |
| `test_format_selection_precedence_order` | Query > Accept > Default |
| (1 more integration test) | Combined format + serialization scenario |

**Implementation Checklist:**

- [ ] Create `format.rs` with SerializationFormat enum
- [ ] Implement parse_format_param() with case-insensitive matching
- [ ] Implement select_format() with precedence logic
- [ ] Implement serialize_data_entry() for all 3 formats
- [ ] Implement deserialize_data_entry() for all 3 formats
- [ ] Create base64 serde module with human_readable detection
- [ ] DataEntry struct with Serialize/Deserialize derives
- [ ] Add all 38 tests in #[cfg(test)] mod tests
- [ ] Run tests: `cargo test --lib core::format`
- [ ] Verify all tests pass ✓

---

## Execution Plan

### Phase 1: Create Module Files

1. **Create `liquers-axum/src/core/response.rs`**
   - Add struct definitions
   - Add public functions
   - Add IntoResponse implementations
   - Copy test code from spec
   - Run: `cargo test --lib core::response::tests`

2. **Create `liquers-axum/src/core/error.rs`**
   - Add error_to_status_code() implementation
   - Add parse_error_type() implementation
   - Copy test code from spec
   - Run: `cargo test --lib core::error::tests`

3. **Create `liquers-axum/src/core/format.rs`**
   - Add SerializationFormat enum
   - Add format selection logic
   - Add serialization/deserialization functions
   - Add base64 serde module
   - Copy test code from spec
   - Run: `cargo test --lib core::format::tests`

### Phase 2: Module Organization

1. **Create `liquers-axum/src/core/mod.rs`**
   ```rust
   pub mod response;
   pub mod error;
   pub mod format;

   pub use response::{ApiResponse, ResponseStatus, ErrorDetail, DataEntry, BinaryResponse};
   pub use error::{error_to_status_code, parse_error_type};
   pub use format::{SerializationFormat, select_format, serialize_data_entry, deserialize_data_entry};
   ```

2. **Update `liquers-axum/src/lib.rs`**
   ```rust
   pub mod core;
   // ... existing modules
   ```

### Phase 3: Run All Tests

```bash
cd /home/orest/zlos/rust/liquers/liquers-axum

# Run all tests
cargo test --lib

# Run specific module tests
cargo test --lib core::response
cargo test --lib core::error
cargo test --lib core::format

# Run with verbose output
cargo test --lib -- --nocapture

# Check test count
cargo test --lib -- --list
```

### Phase 4: Verification

- [ ] All 106 tests pass
- [ ] No warnings or errors
- [ ] Code compiles with `cargo check`
- [ ] Tests run in reasonable time (<10s)
- [ ] No dependencies added (already in Cargo.toml)

---

## Test File Structure Example

Each module file should follow this pattern:

```rust
use serde::{Serialize, Deserialize};
use liquers_core::error::{Error, ErrorType};
// ... other imports

/// Public function 1
pub fn function1() { ... }

/// Public function 2
pub fn function2() { ... }

// Private helper module (if needed)
mod base64 { ... }

#[cfg(test)]
mod tests {
    use super::*;

    // Happy path tests
    #[test]
    fn test_function1_basic() { ... }

    // Error path tests
    #[test]
    fn test_function1_error() { ... }

    // Edge case tests
    #[test]
    fn test_function1_edge() { ... }

    // ... more tests
}
```

---

## Common Issues & Solutions

### Issue 1: Import Errors
**Problem:** `Error::not_supported()` not found
**Solution:** Use `Error::general_error()` or check liquers-core error API

### Issue 2: Serde Attribute Error
**Problem:** `#[serde(skip_serializing_if)]` not working
**Solution:** Ensure `serde` feature is enabled in Cargo.toml

### Issue 3: Test Discovery
**Problem:** Tests not running
**Solution:** Ensure files are in `src/` and have `#[cfg(test)]`

### Issue 4: Serialization Format Errors
**Problem:** CBOR/bincode serialization fails
**Solution:** Verify metadata and data types are properly Serializable

### Issue 5: Base64 Module
**Problem:** Serde is_human_readable() not recognized
**Solution:** Use feature gate: `use serde::Serializer`

---

## Dependencies Verification

All required crates already in `liquers-axum/Cargo.toml`:

- [x] `serde` 1.0+ with `derive` feature
- [x] `serde_json` 1.0+
- [x] `ciborium` 0.2+ (CBOR)
- [x] `bincode` 1.3+
- [x] `base64` 0.22+
- [x] `axum` 0.8+ (for IntoResponse)
- [x] `liquers-core` (for Error, ErrorType, Metadata)

No additional dependencies needed.

---

## Code Review Checklist

Before running tests, verify:

- [ ] No `unwrap()` in non-test code (except safe cases like Response::builder)
- [ ] All match statements explicit (no `_ =>` default arm)
- [ ] Error handling uses `Error::from_error()` or typed constructors
- [ ] Test names follow pattern: `test_<function>_<scenario>`
- [ ] Async tests use `#[tokio::test]` (none needed for Phase 3)
- [ ] No anyhow dependency used (library code only)
- [ ] Serde derives include `Serialize, Deserialize`
- [ ] Copy trait for small enums (ResponseStatus, SerializationFormat)
- [ ] Send + Sync bounds on generics (if needed)

---

## Expected Test Output

Running `cargo test --lib` should produce:

```
test core::response::tests::test_ok_response_with_string_result ... ok
test core::response::tests::test_ok_response_with_integer_result ... ok
[... 103 more tests ...]
test core::format::tests::test_serialize_with_all_byte_values ... ok

test result: ok. 106 passed; 0 failed; 0 ignored; 0 measured; X filtered out

   Finished test [unoptimized + debuginfo] target(s) in X.XXs
```

---

## Next Steps After Tests Pass

1. **Integration Tests** (`tests/` directory)
   - Test handlers (GET /q, POST /q, store endpoints)
   - Test error response formatting
   - Test HTTP status codes

2. **Handler Tests**
   - QueryApiBuilder::build_axum()
   - StoreApiBuilder::build_axum()
   - Test all endpoints

3. **End-to-End Tests**
   - Full HTTP request/response cycle
   - Real environment and store integration
   - Example applications

---

## References

- `PHASE3-UNIT-TESTS.md` - Full test specifications
- `PHASE3-UNIT-TESTS-SUMMARY.md` - Test overview and statistics
- `phase2-architecture.md` - Core data structures
- `CLAUDE.md` - Code conventions and patterns
