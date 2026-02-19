# Testable Components Reference

What to test for each Liquers component, including edge cases and error conditions.

## Table of Contents

1. [Query and Key (liquers-core)](#1-query-and-key)
2. [Parser (liquers-core)](#2-parser)
3. [Plan Builder (liquers-core)](#3-plan-builder)
4. [State (liquers-core)](#4-state)
5. [Store (liquers-core)](#5-store)
6. [Commands (liquers-core)](#6-commands)
7. [Command Metadata (liquers-core)](#7-command-metadata)
8. [Assets (liquers-core)](#8-assets)
9. [Error System (liquers-core)](#9-error-system)
10. [Interpreter (liquers-core)](#10-interpreter)
11. [register_command! Macro (liquers-macro)](#11-register_command-macro)
12. [Store Builder (liquers-store)](#12-store-builder)
13. [Value Types (liquers-lib)](#13-value-types)
14. [Polars Commands (liquers-lib)](#14-polars-commands)
15. [Image Commands (liquers-lib)](#15-image-commands)
16. [UI Elements (liquers-lib)](#16-ui-elements)

---

## 1. Query and Key

**File**: `liquers-core/src/query.rs`

### What to test
- Key construction from segments
- Key encoding/decoding round-trips
- Key prefix operations (`has_key_prefix`, `strip_prefix`)
- Key joining (`join`)
- Filename extraction from keys
- Query encoding/decoding round-trips
- ActionRequest parameter handling
- TryToQuery implementations for &str and String

### Edge cases
- Empty key
- Root key (single segment)
- Keys with special characters (URL encoding)
- Very long key paths
- Keys with trailing slashes
- Query with no actions (just resource path)
- Query with only actions (starts with `/-/`)

### Error conditions
- Invalid key encoding
- Malformed query strings

---

## 2. Parser

**File**: `liquers-core/src/parse.rs`

### What to test
- `parse_key()` with valid keys
- `parse_query()` with valid queries
- Position tracking through parsing
- Resource path extraction
- Action parameter parsing (hyphen-separated)
- Filename extraction (`.ext` suffix)
- Tilde escaping in parameters

### Edge cases
- Queries with no resource: `/-/action`
- Queries with resource only: `path/to/data`
- Queries with filename: `data/action.json`
- Parameters with tilde-escaped hyphens
- Multiple chained actions: `data/action1-arg/action2-arg`
- `q` instruction (UseQueryValue): `data/action/q/next_action`

### Error conditions
- Empty string input
- Invalid characters
- Malformed action syntax

---

## 3. Plan Builder

**File**: `liquers-core/src/plan.rs`

### What to test
- Plan creation from query + command metadata registry
- Step types: Action, GetResource, Evaluate, Filename, UseQueryValue
- Parameter resolution in plan steps
- Plan override values
- Plan serialization (JSON, YAML)
- Placeholder handling (with/without `with_placeholders_allowed`)
- Volatile command detection in plans

### Edge cases
- Plan with single step
- Plan with many chained steps
- Plan with resource + actions
- Plan with filename step
- Plan with q instruction (UseQueryValue)
- Plans with default parameter values

### Error conditions
- Unknown command in query
- Missing required parameters (no placeholder mode)
- Invalid parameter types

---

## 4. State

**File**: `liquers-core/src/state.rs`

### What to test
- `State::new()` creates none state
- `with_string()` / `with_data()` / `with_metadata()` builders
- `try_into_string()` conversion
- `as_bytes()` serialization with various formats
- `is_none()` / `is_error()` checks
- `State::from_error()` creation
- `set_status()` mutation
- `extension()` / `type_identifier()` / `get_data_format()` accessors
- Clone preserves data and metadata

### Edge cases
- State with None value
- State with binary data
- Status transitions
- Metadata format vs data default extension

### Error conditions
- `try_into_string()` on non-string value
- `as_bytes()` with unsupported format
- `error_result()` on error state

---

## 5. Store

**File**: `liquers-core/src/store.rs`

### What to test
- **MemoryStore**: get, set, remove, contains, listdir, makedir, is_dir
- **FileStore**: same operations with filesystem persistence
- **StoreRouter**: delegation to correct store based on key prefix
- **AsyncStoreWrapper**: wraps sync store for async interface
- **NoStore/NoAsyncStore**: returns appropriate errors
- Key prefix isolation between stores in router
- Metadata finalization on set

### Edge cases
- Set to same key twice (overwrite)
- Remove non-existent key
- List empty directory
- Nested directory creation
- Keys at store boundary (prefix matching)
- `listdir_keys_deep` recursion

### Error conditions
- Get non-existent key → KeyNotFound
- Operations on unsupported keys
- Directory operations on files / file operations on directories

---

## 6. Commands

**File**: `liquers-core/src/commands.rs`

### What to test
- `CommandRegistry::new()` creation
- `register_command()` sync registration
- `register_async_command()` async registration
- `execute()` sync execution
- `execute_async()` async execution
- `CommandArguments` parameter extraction with types (i32, String, bool, f64, etc.)
- `FromParameterValue` conversions for all supported types
- Injected parameter resolution via `InjectedFromContext`
- Payload type system (`PayloadType`, `ExtractFromPayload`)

### Edge cases
- Command with no parameters
- Command with all optional parameters
- Command with mixed regular and injected parameters
- Same command name in different namespaces
- Same command name in different realms

### Error conditions
- Execute unregistered command
- Wrong parameter types
- Missing required parameters
- Injection failure (no payload)

---

## 7. Command Metadata

**File**: `liquers-core/src/command_metadata.rs`

### What to test
- `CommandMetadata::new()` creation
- Builder methods: `with_argument()`, `with_async()`, `with_label()`, etc.
- `CommandMetadataRegistry` add/get operations
- `CommandKey` creation and comparison
- `ArgumentInfo` type information
- Serialization/deserialization (JSON, YAML)

### Edge cases
- Metadata with no arguments
- Multiple arguments with defaults
- Preset and next action metadata

---

## 8. Assets

**File**: `liquers-core/src/assets.rs`

### What to test
- `AssetServiceMessage` variants (all message types)
- `AssetNotificationMessage` variants
- Asset lifecycle: create → submit → process → ready
- Progress tracking via service channel
- Error propagation through asset
- `AssetRef` creation and access
- `DefaultAssetManager` operations
- `create_dummy_asset()` for testing

### Edge cases
- Concurrent asset access
- Asset cancellation during processing
- Partial status (preview/checkpoint)
- Expired status after invalidation

### Status transitions to test
```
None → Recipe/Source/Override
Recipe → Submitted → Dependencies/Processing
Processing → Partial/Storing/Ready/Error/Cancelled
Ready → Expired
```

---

## 9. Error System

**File**: `liquers-core/src/error.rs`

### What to test
- All `ErrorType` variants construction
- Error constructor methods: `general_error()`, `key_not_found()`, `conversion_error()`
- `from_error()` wrapping external errors
- Error context chaining: `with_command_key()`, `with_query()`
- `Display` formatting
- Position information in errors
- Error type matching

### Edge cases
- Nested error chains
- Errors with position info
- Errors with command key context
- Empty error messages

---

## 10. Interpreter

**File**: `liquers-core/src/interpreter.rs`

### What to test
- `evaluate()` end-to-end pipeline execution
- `evaluate_immediately()` with payload
- `apply_plan()` step-by-step execution
- `do_step()` for each Step variant
- Pipeline chaining (output of one command → input of next)
- Resource fetching from store
- Nested evaluation (command triggers sub-evaluation)

### Edge cases
- Empty pipeline
- Single-step pipeline
- Pipeline with resource + multiple actions
- Pipeline with filename step
- Pipeline with q instruction

### Error conditions
- Command execution failure mid-pipeline
- Resource not found in store
- Type mismatch between pipeline steps

---

## 11. register_command! Macro

**File**: `liquers-macro/src/lib.rs`

### What to test
- Sync function registration
- Async function registration
- All state parameter variants: `state`, `value`, `text`, omitted
- All return types: `-> result`, `-> value`
- Default values: string, int, float, bool, query
- Injected parameters
- Context parameter
- All metadata statements: label, doc, namespace/ns, realm, filename, volatile, preset, next
- Generated wrapper function correctness
- Generated metadata correctness

### Edge cases
- Multiple commands registered on same registry
- Commands with many parameters
- Commands with all default parameters
- Mixed regular and injected parameters
- Metadata with multiple presets/next actions

### Error conditions (compile-time)
- Missing `type CommandEnvironment` alias
- Type mismatch between function signature and DSL
- Invalid parameter names

---

## 12. Store Builder

**File**: `liquers-store/src/store_builder.rs`

### What to test
- `StoreRouterBuilder::from_yaml()` / `from_json()` parsing
- `build()` creating store router
- Memory store creation from config
- Filesystem store creation from config
- Multiple stores in router
- Store prefix configuration

### Edge cases
- Empty configuration
- Single store
- Multiple stores with overlapping prefixes
- Environment variable expansion in configs: `${VAR_NAME}`

---

## 13. Value Types

**File**: `liquers-lib/src/value/mod.rs`

### What to test
- `ExtValue` enum variant creation (Image, PolarsDataFrame, Widget, UIElement)
- `ExtValueInterface` conversions: `from_image()`, `as_image()`, `from_polars_dataframe()`, etc.
- `ValueInterface` implementation: `none()`, `new()`, `identifier()`, `try_into_string()`
- `DefaultValueSerializer`: `as_bytes()` / `deserialize_from_bytes()` round-trips
- Type identifier reporting for each variant
- Default extension/filename/media-type for each variant

### Edge cases
- Large binary values
- Non-serializable value types (Image, UIElement)
- Value from JSON parsing

---

## 14. Polars Commands

**File**: `liquers-lib/src/polars/`

### What to test
- CSV to DataFrame conversion (`try_to_polars_dataframe`)
- DataFrame operations: head, tail, slice, select, drop, filter, sort
- Aggregation operations: sum, mean, count, min, max
- Group-by operations
- Date/boolean/separator parsing utilities
- Chained operations (filter → select → sort)

### Helpers
```rust
fn create_csv_state(csv_text: &str) -> State<Value> {
    let mut metadata = MetadataRecord::new();
    metadata.data_format = Some("csv".to_string());
    metadata.with_type_identifier("text".to_string());
    State {
        data: Arc::new(Value::from(csv_text.to_string())),
        metadata: Arc::new(metadata.into()),
    }
}
```

---

## 15. Image Commands

**File**: `liquers-lib/src/image/`

### What to test per module
- **io.rs**: Image loading/saving, format conversion
- **color.rs**: Color space conversions, channel operations
- **geometric.rs**: Resize, crop, rotate, flip
- **filtering.rs**: Blur, sharpen, edge detection
- **drawing.rs**: Primitives, text rendering
- **morphology.rs**: Erosion, dilation
- **edges.rs**: Edge detection algorithms
- **format.rs**: Format conversion
- **info.rs**: Image dimensions, metadata
- **util.rs**: Utility functions

---

## 16. UI Elements

**File**: `liquers-lib/src/ui/`

### What to test per module
- **element.rs**: UIElement trait implementations, clone_boxed, type_name
- **handle.rs**: Handle creation, comparison, serialization
- **app_state.rs**: AppState operations, node management, evaluate_pending
- **resolve.rs**: Element resolution from assets

### Key patterns
- Extract-render-replace: `take_element` → `show_in_egui` → `put_element`
- Lazy evaluation: element=None until rendering triggers eval
- Handle-based node identification

---

## Coverage Priority

When deciding what tests to write, prioritize:

1. **Public API surface** — any `pub fn` or `pub trait` method
2. **Error paths** — verify specific ErrorType, not just is_err()
3. **Serialization round-trips** — JSON/YAML encode then decode
4. **State transitions** — Status enum progressions
5. **Edge cases from specs** — documented in specs/*.md files
6. **Integration paths** — command registration → evaluation → result

## Files with Existing Tests (for reference)

| File | Test Count | Focus |
|------|-----------|-------|
| `liquers-core/src/parse.rs` | Many | Query/Key parsing |
| `liquers-core/src/query.rs` | Several | Key operations |
| `liquers-core/src/plan.rs` | 3+ | Plan building |
| `liquers-core/src/commands.rs` | 3+ | Command execution |
| `liquers-core/src/store.rs` | Several | MemoryStore |
| `liquers-core/src/error.rs` | Several | Error formatting |
| `liquers-core/src/value.rs` | Several | Value conversions |
| `liquers-core/tests/async_hellow_world.rs` | 3+ | End-to-end evaluation |
| `liquers-core/tests/injection.rs` | 5+ | Payload injection |
| `liquers-lib/tests/polars_commands.rs` | 12+ | Polars operations |
| `liquers-lib/src/image/*.rs` | Many | Image operations |
| `liquers-lib/src/ui/*.rs` | Several | UI elements |
| `liquers-store/src/config.rs` | Several | Store config |
| `liquers-store/src/store_builder.rs` | Several | Store building |
| `liquers-macro/src/lib.rs` | 1+ | Macro internals |
