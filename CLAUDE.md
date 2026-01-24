# Liquers Development Guide

## Project Structure

```
liquers-core/     # Core abstractions (Query, Key, Store, Assets, Commands)
liquers-macro/    # register_command! function-like proc-macro
liquers-store/    # Storage backends (OpenDAL integration, config)
liquers-lib/      # Command library, Rich value types (Polars DataFrames, egui UI, images)
liquers-axum/     # HTTP REST API server
liquers-py/       # Python bindings (PyO3)
specs/            # Specifications and design documents
```

**Dependency flow**: `liquers-core` ← `liquers-macro` ← `liquers-store` ← `liquers-lib` ← `liquers-axum`

**Key specs**: See `specs/PROJECT_OVERVIEW.md` for architecture, `specs/REGISTER_COMMAND_FSD.md` for macro details, `specs/ASSETS.md` for asset lifecycle.

## Architecture Rules

### Where Code Goes
- Query language, parsing, plans: `liquers-core/src/query.rs`, `liquers-core/src/parse.rs`, `liquers-core/src/plan.rs`
- Storage traits and implementations: `liquers-core/src/store.rs`, `liquers-store/src/`
- Command execution framework: `liquers-core/src/commands.rs`, `liquers-core/src/command_metadata.rs`
- Asset lifecycle: `liquers-core/src/assets.rs`
- New value types (DataFrames, images): `liquers-lib/src/value/`
- New storage backends: `liquers-store/src/`
- New commands: `liquers-lib/src/commands.rs`
- Polars DataFrame operations: `liquers-lib/src/polars/` (see `specs/POLARS_COMMAND_LIBRARY.md`)

### Key Types
- `Query`, `Key`, `ActionRequest` - query DSL (`liquers-core/src/query.rs`)
- `Value` (layer 1) → `State<V>` (layer 2) → `Asset` (layer 3) - value encapsulation
- `Environment` - global services (store, assets, commands)
- `Context` - per-command execution context
- `Error` with `ErrorType` - all errors use `liquers_core::error::Error`

## Code Conventions

### Match Statements

Match statements of enums should be explicit; avoid the default match arm (`_ =>`).
This ensures future changes (new `Status` variants, `Step` types, channel messages) trigger compile errors.

### Error Handling
```rust
use liquers_core::error::{Error, ErrorType};

// DO: Use typed error constructors
Error::key_not_found(&key)
Error::general_error("message".to_string())
Error::from_error(ErrorType::General, external_error)

// DON'T: Use Error::new directly
// Error::new(ErrorType::ParseError, "...")  // Avoid this
```

### Async Patterns
- Default to async (`AsyncStore`, `AsyncStoreRouter`)
- Use `#[async_trait]` for async trait methods
- Tokio runtime with `sync`, `rt`, `macros`, `time` features
- Sync wrappers (`AsyncStoreWrapper`) only for Python compatibility

### Naming
- Traits: `ValueInterface`, `ExtValueInterface`, `AsyncStore`, `CommandExecutor`
- Async variants: prefix with `Async` (e.g., `AsyncStoreRouter`)
- Builders: `StoreRouterBuilder`, `PlanBuilder`
- Test modules: `#[cfg(test)] mod tests { ... }` at end of file

### Serialization
- Use `serde` with `Serialize, Deserialize` derives
- First-class support for JSON, YAML, and TOML
- Environment variables: `${VAR_NAME}` syntax in configs

## Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_functionality() { ... }

    #[tokio::test]
    async fn test_async_functionality() { ... }
}
```

- Unit tests in same file, integration tests in `tests/`
- Use `parse_key()`, `parse_query()` helpers for test setup
- Memory stores for testing: `MemoryStore::new(&Key::new())`, wrapped via `AsyncStoreWrapper`
- See `liquers-core/tests/async_hellow_world.rs` for full flow: Environment with memory store, RecipeProvider, command registration, query evaluation

## Constraints

### Do NOT
- Use `unwrap()` or `expect()` in library code (only in tests)
- Create new error types outside `liquers_core::error`
- Use `Error::new` directly
- Use blocking I/O in async contexts
- Add sync Store implementations (async only, sync via wrapper)
- Modify Query/Key encoding without updating `specs/PROJECT_OVERVIEW.md`

### Performance-Sensitive Areas
- Query parsing (`liquers-core/src/parse.rs`) - used on every request
- Key encoding/decoding (`liquers-core/src/query.rs`) - frequent operations
- Asset lookups in `AssetManager` - use `scc` concurrent map

## Modifying Existing Code

### Before Changing APIs
1. Check if type is used in `liquers-py` (Python bindings break easily)
2. Check `register_command!` macro usage in `liquers-lib`
3. Update `specs/PROJECT_OVERVIEW.md` if core concepts change

### Refactoring Guidelines
- Prefer extending traits over modifying them
- Add new methods with default implementations when possible
- Keep `liquers-core` minimal; rich features go in `liquers-lib`

## Common Tasks

### Adding a Command

The `register_command!` macro is a **function-like macro** (not an attribute macro) with a custom DSL.
The actual function must be defined SEPARATELY, then registered via the macro.

**See `specs/COMMAND_REGISTRATION_GUIDE.md` for comprehensive guidelines** covering:
- Using the `register_command!` macro (recommended)
- Manual registration (fine-grained control)
- Generic Environment commands (library reusability)
- Best practices and examples

For macro syntax details, see `specs/REGISTER_COMMAND_FSD.md`.

```rust
use liquers_macro::register_command;
use liquers_core::{state::State, error::Error, context::Context};

// 1. Define the function separately
fn greet(state: &State<Value>, greeting: String) -> Result<Value, Error> {
    let input = state.try_into_string()?;
    Ok(Value::from(format!("{}, {}!", greeting, input)))
}

// 2. Register using the macro DSL
let cr = env.get_mut_command_registry();
register_command!(cr, fn greet(state, greeting: String) -> result)?;

// Async command with default value
register_command!(cr, async fn async_greet(state, greeting: String = "Hello") -> result)?;

// With metadata
register_command!(cr,
    fn to_text(state, context) -> result
    label: "To text"
    doc: "Convert input state to string"
    filename: "text.txt"
)?;
```

**DSL Syntax Reference**:
- State parameter (first): `state`, `value`, `text`, or omit entirely
- `context` - special parameter for execution context
- Parameters: `name: Type`, optionally `injected`, optionally `= default_value`
- Default value types: string `"foo"`, bool `true`, int `42`, float `3.14`, query `query "path/to/query"`
- Return: `-> result` (returns `Result<V, Error>`) or `-> value` (returns `V`)
- Metadata: `label:`, `doc:`, `namespace:`, `realm:`, `preset:`, `next:`, `filename:`, `volatile:`

See examples in `liquers-lib/src/commands.rs` and `liquers-core/tests/async_hellow_world.rs`.

### Adding a Store Backend
1. Implement `AsyncStore` trait in `liquers-store/src/`
2. Add config support in `liquers-store/src/config.rs` and `liquers-store/src/store_builder.rs`
3. Update `OPENDAL_STORE_TYPES` in `liquers-store/src/config.rs` if OpenDAL-based
4. See `specs/STORE_CONFIG_FSD.md` for configuration format

### Adding a Value Type
1. Extend `ExtValue` enum in `liquers-lib/src/value/mod.rs`
2. Implement conversions in `ExtValueInterface` trait
3. Add serialization support
