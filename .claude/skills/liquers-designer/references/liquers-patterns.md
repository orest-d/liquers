# Liquers-Specific Design Patterns

This document consolidates established patterns from the Liquers codebase. Follow these patterns when designing new features to ensure consistency and compatibility.

**Sources:** CLAUDE.md, PROJECT_OVERVIEW.md, MEMORY.md, existing codebase

## Crate Dependencies

### One-Way Dependency Flow

```
liquers-core ← liquers-macro ← liquers-store ← liquers-lib ← liquers-axum ← liquers-py
```

**Rules:**
- liquers-core has NO dependencies on other liquers crates (only external crates)
- liquers-lib can use liquers-core, liquers-macro, liquers-store
- liquers-axum can use all preceding crates
- NO circular dependencies

**Where to put new code:**
- Core abstractions (Query, Command, Store traits): `liquers-core`
- Storage backends: `liquers-store`
- Rich value types (DataFrames, Images, UI): `liquers-lib`
- HTTP endpoints: `liquers-axum`

**Example violation:**
```rust
// BAD: liquers-core importing from liquers-lib
use liquers_lib::polars::DataFrame;  // ❌ Breaks dependency flow
```

**Correct approach:**
```rust
// GOOD: liquers-lib importing from liquers-core
use liquers_core::value::Value;  // ✅ Follows dependency flow
```

## Value Extension Pattern

### Adding New Value Types (ExtValue Variants)

**Location:** Always in `liquers-lib/src/value/mod.rs`

**Pattern:**
```rust
// In liquers-lib/src/value/mod.rs
#[derive(Debug, Clone)]
pub enum ExtValue {
    // Existing variants
    DataFrame { df: Arc<polars::prelude::DataFrame> },
    Image { image: Arc<image::DynamicImage> },

    // NEW: Your variant
    NewType { value: Arc<YourType> },
}
```

**Rules:**
- ExtValue derives only `Debug + Clone` (no `Serialize`)
- Use `Arc<T>` for shared ownership (cheap cloning)
- Implement `ExtValueInterface` trait for conversions
- Use `DefaultValueSerializer` for byte conversion

**Example: Adding a ParquetMetadata type**

```rust
#[derive(Debug, Clone)]
pub struct ParquetMetadata {
    pub schema: String,
    pub row_count: usize,
}

pub enum ExtValue {
    // ...existing variants
    ParquetMetadata { metadata: Arc<ParquetMetadata> },
}

impl ExtValueInterface for ExtValue {
    fn try_as_parquet_metadata(&self) -> Result<&ParquetMetadata, Error> {
        match self {
            ExtValue::ParquetMetadata { metadata } => Ok(metadata.as_ref()),
            _ => Err(Error::type_error("Expected ParquetMetadata")),
        }
    }
}
```

**ValueExtension Trait Requirements:**
```rust
trait ValueExtension: Debug + Clone + Sized + DefaultValueSerializer + Send + Sync + 'static
```

**Do NOT:**
- Add new top-level enums for values (use ExtValue variants)
- Implement Serialize directly on ExtValue (use DefaultValueSerializer)
- Use `Box<T>` instead of `Arc<T>` (Arc allows cheap cloning)

## Command Registration Pattern

### Using register_command! Macro

**Location:** Commands registered in `liquers-lib/src/commands.rs`

**Basic pattern:**
```rust
use liquers_macro::register_command;

// 1. Define function separately
fn my_command(state: &State<Value>, param: String) -> Result<Value, Error> {
    // Implementation
}

// 2. Register using macro DSL
let cr = env.get_mut_command_registry();
register_command!(cr, fn my_command(state, param: String) -> result)?;
```

**With metadata:**
```rust
register_command!(cr,
    fn my_command(state, param: String) -> result
    namespace: "my_namespace"
    label: "My Command"
    doc: "Description of what this command does"
    filename: "output.txt"
)?;
```

**Async commands:**
```rust
// Function takes OWNED State<Value>
async fn my_async_command(state: State<Value>, param: String) -> Result<Value, Error> {
    // Implementation
}

// Register as async
register_command!(cr, async fn my_async_command(state, param: String) -> result)?;
```

**Default parameter values:**
```rust
register_command!(cr,
    fn my_command(state, param1: String = "default", param2: i64 = 42) -> result
)?;
```

**Context parameter (must be LAST):**
```rust
fn my_command(state: &State<Value>, param: String, context: &Context) -> Result<Value, Error> {
    // Implementation
}

register_command!(cr, fn my_command(state, param: String, context) -> result)?;
```

**Function naming:**
- Function names do NOT include namespace prefix
- Namespace set in metadata
- Example: `fn to_parquet(...)` with `namespace: "polars"` → command is `polars/to_parquet`

**Rules:**
- Commands registered via macro (not manually via CommandMetadata)
- Async command functions take **owned** `State<Value>` (not `&State<Value>`)
- Sync command functions take `&State<Value>`
- `context` parameter must be last (workaround for parameter index bug - see ISSUES.md)
- State parameter can be `state`, `value`, `text`, or omitted
- Return type: `-> result` (returns `Result<V, Error>`) or `-> value` (returns `V`)

## Store Backend Pattern

### Implementing AsyncStore Trait

**Location:** New store backends in `liquers-store/src/`

**Pattern:**
```rust
use async_trait::async_trait;
use liquers_core::store::AsyncStore;
use liquers_core::query::Key;
use liquers_core::error::Error;

pub struct MyStore {
    // Store-specific fields
}

#[async_trait]
impl AsyncStore for MyStore {
    async fn get(&self, key: &Key) -> Result<Vec<u8>, Error> {
        // Implementation
    }

    async fn set(&self, key: &Key, value: &[u8]) -> Result<(), Error> {
        // Implementation
    }

    async fn remove(&self, key: &Key) -> Result<(), Error> {
        // Implementation
    }

    async fn contains(&self, key: &Key) -> Result<bool, Error> {
        // Implementation
    }
}
```

**Rules:**
- Default to async (use `AsyncStore` trait)
- Sync wrappers (`AsyncStoreWrapper`) only when needed (e.g., Python bindings)
- Use `#[async_trait]` macro for async trait methods
- Return `Result<T, Error>` (not custom error types)

**Configuration support:**

Add to `liquers-store/src/config.rs`:
```rust
#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum StoreConfig {
    Memory,
    FileSystem { path: String },
    MyStore { config_param: String },  // NEW
}
```

Add to `liquers-store/src/store_builder.rs`:
```rust
impl StoreBuilder {
    pub async fn build_from_config(config: &StoreConfig) -> Result<Arc<dyn AsyncStore>, Error> {
        match config {
            StoreConfig::Memory => { /* ... */ },
            StoreConfig::MyStore { config_param } => {
                Ok(Arc::new(MyStore::new(config_param)?))
            },
            // ...
        }
    }
}
```

## UI Element Pattern (Phase 1 Established)

### Implementing UIElement Trait

**Location:** UI elements in `liquers-lib/src/ui/elements/`

**Pattern:**
```rust
use liquers_core::error::Error;
use crate::ui::{UIElement, UIContext, UIHandle, AppState};

#[derive(Clone, Debug)]
pub struct MyUIElement {
    handle: Option<UIHandle>,
    title: String,
    // Element-specific fields
}

impl UIElement for MyUIElement {
    fn type_name(&self) -> &'static str {
        "MyUIElement"
    }

    fn handle(&self) -> Option<UIHandle> {
        self.handle
    }

    fn set_handle(&mut self, handle: Option<UIHandle>) {
        self.handle = handle;
    }

    fn title(&self) -> String {
        self.title.clone()
    }

    fn set_title(&mut self, title: String) {
        self.title = title;
    }

    fn clone_boxed(&self) -> Box<dyn UIElement> {
        Box::new(self.clone())
    }

    fn init(&mut self, handle: UIHandle, ui_context: &UIContext) {
        self.set_handle(Some(handle));
        // Initialization logic (e.g., submit query)
    }

    fn update(&mut self, msg: &str, ui_context: &UIContext) {
        // Message handling
    }

    fn show_in_egui(&mut self, ui: &mut egui::Ui, app_state: &mut dyn AppState) {
        // Rendering logic
    }
}
```

**Serialization:**
```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MyUIElement {
    handle: Option<UIHandle>,
    title: String,

    #[serde(skip)]  // Non-serializable fields
    runtime_state: Option<SomeType>,
}
```

**Rules:**
- Implement all UIElement trait methods
- Use `#[serde(skip)]` for non-serializable fields (Arc<dyn Trait>, runtime state)
- `show_in_egui` takes `&mut dyn AppState` for recursive rendering
- `update` takes `&UIContext` for headless query submission
- `init` called by AppRunner (not AppState.set_element())
- No `as_any()` downcasting in Phase 1 (deferred to Phase 2)

**AssetViewElement spawned-task pattern:**

For async updates (e.g., polling asset status):
```rust
pub struct AssetViewElement {
    value: Arc<std::sync::RwLock<Option<Value>>>,
    progress: Arc<std::sync::RwLock<f64>>,
    error: Arc<std::sync::RwLock<Option<Error>>>,
}

impl AssetViewElement {
    pub fn from_asset_ref<E: Environment>(asset_ref: AssetRef<E>) -> Self {
        let element = Self {
            value: Arc::new(RwLock::new(None)),
            progress: Arc::new(RwLock::new(0.0)),
            error: Arc::new(RwLock::new(None)),
        };

        // Spawn background task for polling
        let value_clone = element.value.clone();
        let progress_clone = element.progress.clone();
        tokio::spawn(async move {
            // Poll asset_ref status, update fields
        });

        element
    }
}
```

Use `Arc<std::sync::RwLock<T>>` (not tokio::sync::RwLock) for fields accessed from egui render thread.

## Error Handling Pattern

### Using Typed Error Constructors

**Location:** All error handling uses `liquers_core::error::Error`

**Pattern:**
```rust
use liquers_core::error::{Error, ErrorType};

// ✅ GOOD: Use typed constructors
Error::key_not_found(&key)
Error::general_error("Invalid input".to_string())
Error::from_error(ErrorType::General, external_error)

// ❌ BAD: Direct Error::new (avoid)
Error::new(ErrorType::ParseError, "message")  // Don't do this
```

**Available constructors:**
```rust
Error::key_not_found(key: &Key) -> Error
Error::general_error(message: String) -> Error
Error::type_error(message: &str) -> Error
Error::parse_error(message: String) -> Error
Error::from_error(error_type: ErrorType, source: impl std::error::Error + 'static) -> Error
```

**Error propagation:**
```rust
// Use ? operator
let value = some_function()?;

// Wrap external errors
external_crate::function()
    .map_err(|e| Error::from_error(ErrorType::General, e))?;
```

**Rules:**
- NO custom error types (use `liquers_core::error::Error` only)
- NO `unwrap()` or `expect()` in library code (only in tests)
- Return `Result<T, Error>` from all fallible functions
- Use `?` operator for error propagation

## Async Pattern

### Default to Async, Sync Wrappers When Needed

**Pattern:**
```rust
// ✅ GOOD: Async implementation
#[async_trait]
pub trait AsyncStore {
    async fn get(&self, key: &Key) -> Result<Vec<u8>, Error>;
}

// ✅ GOOD: Sync wrapper (only when needed, e.g., Python bindings)
pub struct AsyncStoreWrapper {
    inner: Arc<dyn AsyncStore>,
    runtime: tokio::runtime::Runtime,
}

impl AsyncStoreWrapper {
    pub fn get(&self, key: &Key) -> Result<Vec<u8>, Error> {
        self.runtime.block_on(self.inner.get(key))
    }
}
```

**When to use async:**
- I/O operations (file, network, database)
- Functions called from async contexts
- Default choice unless there's a reason not to

**When to use sync:**
- Pure computation (no I/O)
- Functions called from sync contexts (e.g., Python bindings, egui render)
- When wrapping async with `block_on` or `blocking_lock()`

**Tokio runtime:**
- Use `tokio::runtime::Runtime` with `sync`, `rt`, `macros`, `time` features
- For eframe apps: create Runtime in app struct, wrap env setup in `runtime.block_on(async { ... })`
- Store runtime in app struct (`_runtime` field) to keep it alive

**Rules:**
- Default to async
- Use `#[async_trait]` for async trait methods
- Sync wrappers only for compatibility (Python, egui render)
- Commands can be async: use `register_command!(cr, async fn ...)`

## Phased Design Pattern

### UI Features Phased Evolution

**Example:** UI Element framework

**Phase 1:** Basic infrastructure
- UIElement trait with core methods (type_name, handle, show_in_egui)
- Simple elements (DisplayElement, AssetViewElement)
- No advanced features (no downcasting, no container composition)

**Phase 1a:** Architecture refinement
- UIContext as primary state holder
- UIPayload for interface to UIContext
- Simplified command patterns

**Phase 1b:** Execution model
- AppRunner pattern (generic over Environment)
- Lazy evaluation (None → Progress → Ready/Error)
- Message-based interaction

**Phase 1c:** Abstraction
- AppState trait (non-generic)
- InsertionPoint-based insertion
- StateViewElement for plain values

**Phase 2:** Advanced features (future)
- Container composition (TabContainer, SplitPane)
- Downcasting (as_any pattern)
- Advanced UI patterns

**Lessons:**
- Start simple, iterate based on real usage
- Each phase has clear deliverables
- Avoid over-engineering early (YAGNI)
- Document design decisions for future phases

**Applying to new features:**
- If a feature is complex, split into phases
- Phase 1: Minimal viable functionality
- Later phases: Advanced features based on feedback
- Document phase plan in `specs/<feature>/` folder

## Match Statements

### No Default Match Arms

**Pattern:**
```rust
// ✅ GOOD: Explicit variants
match status {
    Status::Pending => { /* handle */ },
    Status::InProgress => { /* handle */ },
    Status::Completed => { /* handle */ },
}

// ❌ BAD: Default match arm
match status {
    Status::Pending => { /* handle */ },
    _ => { /* catch-all */ },  // Don't do this
}
```

**Rationale:**
- Future enum variants will trigger compile errors
- Forces handling of all cases explicitly
- Prevents silent bugs when enums are extended

**Exceptions:**
- Matching on external enums (not owned by liquers)
- Intentional catch-all for extensibility (document why)

## Serialization

### Serde Patterns

**Pattern:**
```rust
use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize)]
pub struct MyStruct {
    pub field1: String,

    #[serde(skip)]  // Not serializable
    pub runtime_data: Arc<dyn Trait>,

    #[serde(default)]  // Optional field with default
    pub optional_field: Option<String>,
}
```

**ExtValue special case:**
```rust
// ExtValue derives Debug + Clone only (no Serialize)
#[derive(Debug, Clone)]
pub enum ExtValue {
    DataFrame { df: Arc<polars::prelude::DataFrame> },
    // ...
}

// Serialization via DefaultValueSerializer trait
impl DefaultValueSerializer for ExtValue {
    fn to_bytes(&self) -> Result<Vec<u8>, Error> {
        // Custom serialization logic
    }

    fn from_bytes(bytes: &[u8]) -> Result<Self, Error> {
        // Custom deserialization logic
    }
}
```

**Rules:**
- Use `#[serde(skip)]` for non-serializable fields
- Use `#[serde(default)]` for optional fields with defaults
- ExtValue uses `DefaultValueSerializer` (not `Serialize` derive)
- Config files support JSON, YAML, TOML
- Environment variables: `${VAR_NAME}` syntax

## State Construction

### No From<Value> for State

**Pattern:**
```rust
// ❌ BAD: State does not have From<Value>
let state: State = value.into();  // Doesn't compile

// ✅ GOOD: Explicit construction
use std::sync::Arc;
use liquers_core::state::State;
use liquers_core::metadata::Metadata;

let state = State {
    data: Arc::new(value),
    metadata: Arc::new(Metadata::new()),
};

// ✅ GOOD: Or use helper methods if available
let state = State::from_value(value);  // If this method exists
```

**Rationale:**
- State requires both data and metadata
- Explicit construction prevents accidental loss of metadata
- Makes the Arc wrapping explicit

## Testing Patterns

### Unit Tests

**Location:** Same file as code, in `#[cfg(test)] mod tests { ... }`

**Pattern:**
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_function() {
        let result = my_function();
        assert_eq!(result, expected);
    }

    #[tokio::test]
    async fn test_async_function() {
        let result = my_async_function().await;
        assert!(result.is_ok());
    }
}
```

### Integration Tests

**Location:** `<crate>/tests/<test_name>.rs`

**Pattern:**
```rust
// liquers-lib/tests/my_integration.rs
use liquers_core::query::parse_query;
use liquers_lib::SimpleEnvironment;

#[tokio::test]
async fn test_full_workflow() {
    let env = SimpleEnvironment::new().await;
    let query = parse_query("/-/data.csv~command").unwrap();
    let result = env.evaluate(&query).await;
    assert!(result.is_ok());
}
```

**Test helpers:**
- `parse_key()`, `parse_query()` for setup
- `MemoryStore::new(&Key::new())` for in-memory testing
- `AsyncStoreWrapper` to wrap async stores for testing
- See `liquers-core/tests/async_hellow_world.rs` for full example

**Rules:**
- Unit tests in same file (fast, focused)
- Integration tests in `tests/` directory (end-to-end, cross-module)
- Use `#[tokio::test]` for async tests
- No `unwrap()` in test assertions (use `assert!(result.is_ok())` or `?`)

## Common Pitfalls

### Pitfall 1: Breaking Dependency Flow

**Bad:**
```rust
// In liquers-core
use liquers_lib::SomeType;  // ❌ Wrong direction
```

**Good:**
```rust
// In liquers-lib
use liquers_core::SomeType;  // ✅ Correct direction
```

### Pitfall 2: Using Error::new Directly

**Bad:**
```rust
Error::new(ErrorType::General, "message")  // ❌ Don't do this
```

**Good:**
```rust
Error::general_error("message".to_string())  // ✅ Use typed constructor
```

### Pitfall 3: Adding Default Match Arms

**Bad:**
```rust
match variant {
    Variant1 => { /* ... */ },
    _ => { /* catch all */ },  // ❌ Hides future variants
}
```

**Good:**
```rust
match variant {
    Variant1 => { /* ... */ },
    Variant2 => { /* ... */ },
    Variant3 => { /* ... */ },
}  // ✅ Explicit, compiler enforces completeness
```

### Pitfall 4: Sync When Async Is Appropriate

**Bad:**
```rust
pub fn read_file(path: &str) -> Result<Vec<u8>, Error> {
    std::fs::read(path)  // ❌ Blocking I/O
        .map_err(|e| Error::from_error(ErrorType::General, e))
}
```

**Good:**
```rust
pub async fn read_file(path: &str) -> Result<Vec<u8>, Error> {
    tokio::fs::read(path).await  // ✅ Async I/O
        .map_err(|e| Error::from_error(ErrorType::General, e))
}
```

### Pitfall 5: Unwrap/Expect in Library Code

**Bad:**
```rust
pub fn my_function(input: &str) -> String {
    input.parse::<i32>().unwrap()  // ❌ Panic on error
        .to_string()
}
```

**Good:**
```rust
pub fn my_function(input: &str) -> Result<String, Error> {
    let num = input.parse::<i32>()
        .map_err(|e| Error::general_error(format!("Parse error: {}", e)))?;
    Ok(num.to_string())
}  // ✅ Returns error instead of panicking
```

## Summary Checklist

When designing a new feature, ensure:

- [ ] Follows crate dependency flow (one-way, no cycles)
- [ ] New value types as ExtValue variants in liquers-lib
- [ ] Commands registered via `register_command!` macro
- [ ] Store backends implement AsyncStore trait
- [ ] UI elements implement UIElement trait (if applicable)
- [ ] Error handling uses typed constructors (no `Error::new`)
- [ ] Default to async (sync wrappers only when needed)
- [ ] Match statements are explicit (no `_ =>` default arm)
- [ ] No `unwrap()`/`expect()` in library code
- [ ] Serialization uses `#[serde(skip)]` for non-serializable fields
- [ ] Tests follow unit (inline) + integration (`tests/`) pattern
- [ ] Documentation updated (CLAUDE.md if new patterns, PROJECT_OVERVIEW.md if core concepts)

**If unsure, refer to existing code** in the same category (e.g., study existing commands for command patterns, existing stores for store patterns).
