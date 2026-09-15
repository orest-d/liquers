---
title: Unit Testing Guide
kind: guide
audience: internal
area: [build, core/assets]
reviewed: 2026-09-15
---
# Liquers Unit Testing Guide

This guide explains how to write comprehensive unit tests for the Liquers query evaluation flow, covering environment setup, command registration, query evaluation, and result verification.

## Table of Contents

1. [Basic Test Structure](#basic-test-structure)
2. [Environment Setup](#environment-setup)
3. [Store Configuration](#store-configuration)
4. [Recipe Providers](#recipe-providers)
5. [Command Registration](#command-registration)
6. [Context Usage](#context-usage)
7. [Query Evaluation](#query-evaluation)
8. [Result Extraction and Testing](#result-extraction-and-testing)
9. [Complete Examples](#complete-examples)
10. [Testing Assets](#testing-assets)

---

## Basic Test Structure

All async tests use the `tokio::test` attribute and return `Result<(), Box<dyn std::error::Error>>`:

```rust
use liquers_core::{
    context::{Context, Environment, SimpleEnvironment},
    error::Error,
    interpreter::evaluate,
    state::State,
    value::Value,
};
use liquers_macro::register_command;

#[tokio::test]
async fn test_example() -> Result<(), Box<dyn std::error::Error>> {
    // Test code here
    Ok(())
}
```

For synchronous tests (e.g., query parsing only):

```rust
#[test]
fn test_query_parsing() {
    // Test code here
}
```

---

## Environment Setup

### Using SimpleEnvironment (for simple tests)

```rust
use liquers_core::context::SimpleEnvironment;
use liquers_core::value::Value;

let mut env = SimpleEnvironment::<Value>::new();
```

### Using DefaultEnvironment (for full-featured tests with stores)

```rust
use liquers_lib::environment::DefaultEnvironment;
use liquers_lib::value::Value;

let mut env = DefaultEnvironment::<Value>::new();
```

### Converting Environment to EnvRef

The `EnvRef` is required for query evaluation:

```rust
let envref = env.to_ref();
```

**Important:** `to_ref()` consumes the environment, so call it after all setup is complete.

**Note:** `to_ref()` refreshes command metadata versions before the environment is shared, then
initializes the asset manager. For queued managers, startup loads command versions into the
dependency manager. Register commands and customize their metadata before `to_ref()`; commands
registered later will not be included in either lifecycle step automatically.

---

## Store Configuration

### Memory Store (for testing)

```rust
use liquers_core::{
    metadata::Metadata,
    parse::parse_key,
    query::Key,
    store::{AsyncMemoryStore, AsyncStore},
};

// Create memory store at root
let memory_store = AsyncMemoryStore::new(&Key::new());

// Add data to the store
let key = parse_key("data/test.txt")?;
memory_store
    .set(&key, b"test content", &Metadata::new())
    .await?;

// Add to environment (DefaultEnvironment only)
env.with_async_store(Box::new(memory_store));
```

### Accessing Store Data in Tests

```rust
use liquers_core::context::Environment;

let store = env.get_async_store();
let key = Key::parse("data/test.txt")?;
let value = store.get(&key).await?;
```

---

## Recipe Providers

### Default Recipe Provider

Automatically looks for recipes in `<store>/-R/recipes.yaml`:

```rust
env.with_default_recipe_provider();
```

### Trivial Recipe Provider

Treats resource paths as literal queries:

```rust
env.with_trivial_recipe_provider();
```

### Custom Recipe Provider

For storing recipes in memory:

```rust
use liquers_core::recipes::{RecipeList, Recipe};

// Create recipe list
let mut recipe_list = RecipeList::new();
recipe_list.add_recipe(
    Recipe::new(
        "-R/hello/test.txt".to_string(),
        "Test Recipe".to_string(),
        "A test recipe query".to_string(),
    )?
)?;

// Serialize to YAML and store
let yaml_content = serde_yaml::to_string(&recipe_list)?;
let recipes_key = Key::parse("-R/recipes.yaml")?;
memory_store.set(&recipes_key, Value::from(yaml_content))?;

// Use default provider (will read from store)
env.with_default_recipe_provider();
```

---

## Command Registration

### Manual Registration (Fine-grained control)

```rust
use liquers_core::command_metadata::CommandKey;

let key = CommandKey::new_name("my_command");
env.command_registry.register_command(
    key,
    |state, _args, _ctx| {
        Ok(Value::from("result"))
    }
)?;
```

### Using register_command! Macro (Recommended)

The macro provides automatic parameter handling, metadata, and type safety.

#### Basic Command

```rust
type CommandEnvironment = SimpleEnvironment<Value>;

fn my_command(state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from("result"))
}

let cr = &mut env.command_registry;
register_command!(cr, fn my_command(state) -> result)?;
```

#### Command with Parameters

```rust
fn greet(state: &State<Value>, name: String) -> Result<Value, Error> {
    let data = state.try_into_string()?;
    Ok(Value::from(format!("Hello, {name}! Data: {data}")))
}

register_command!(cr, fn greet(state, name: String) -> result)?;
```

#### Command with Default Values

```rust
fn greet(state: &State<Value>, greeting: String) -> Result<Value, Error> {
    let data = state.try_into_string()?;
    Ok(Value::from(format!("{greeting}, {data}!")))
}

register_command!(cr,
    fn greet(state, greeting: String = "Hello") -> result
)?;
```

#### Async Command

```rust
async fn async_fetch(state: State<Value>, url: String) -> Result<Value, Error> {
    // Async operations
    Ok(Value::from("fetched data"))
}

register_command!(cr,
    async fn async_fetch(state, url: String) -> result
)?;
```

#### Command with Context

```rust
fn logged_command<E: Environment>(
    state: &State<E::Value>,
    context: Context<E>,
) -> Result<Value, Error> {
    context.info("Processing command")?;
    Ok(Value::from("result"))
}

register_command!(cr,
    fn logged_command(state, context) -> result
)?;
```

#### Command with Metadata

```rust
register_command!(cr,
    fn to_text(state, context) -> result
    namespace: "convert"
    label: "To Text"
    doc: "Convert state to text representation"
    filename: "output.txt"
)?;
```

#### Generic Command (Library-reusable)

```rust
fn generic_cmd<E: Environment>(
    state: &State<E::Value>,
    param: String,
    context: Context<E>,
) -> Result<E::Value, Error>
where
    E::Value: From<String>
{
    context.info(&format!("Processing: {param}"))?;
    Ok(E::Value::from("result".to_string()))
}

// Type alias required for macro
type CommandEnvironment = SimpleEnvironment<Value>;

register_command!(cr,
    fn generic_cmd(state, param: String, context) -> result
)?;
```

---

## Context Usage

The Context provides access to the environment during command execution:

### Logging

```rust
fn my_command<E: Environment>(
    state: &State<E::Value>,
    context: Context<E>,
) -> Result<Value, Error> {
    context.info("Starting operation")?;
    context.warning("This is a warning")?;
    context.error("This is an error")?;

    Ok(Value::from("result"))
}
```

### Accessing Environment Services

```rust
fn my_command<E: Environment>(
    state: &State<E::Value>,
    context: Context<E>,
) -> Result<Value, Error> {
    // Access command metadata registry
    let metadata_registry = context.envref.0.get_command_metadata_registry();

    // Access store
    let store = context.envref.0.get_async_store();

    // Access asset manager
    let assets = context.envref.0.get_asset_manager();

    Ok(Value::from("result"))
}
```

---

## Query Evaluation

### Basic Evaluation

```rust
let envref = env.to_ref();
let state = evaluate(envref.clone(), "command1/command2-arg", None).await?;
```

### Query Syntax Rules

**Important:** Queries cannot contain whitespace or newlines. For testing with complex data:

```rust
// ❌ WRONG - will fail to parse
let query = "txt-hello world/process";

// ✅ CORRECT - use dashes or underscores
let query = "txt-hello_world/process";

// ✅ CORRECT - for multi-line data, create State directly
let mut metadata = liquers_core::metadata::Metadata::new();
let state = State {
    data: Arc::new(Value::from("multi\nline\ndata")),
    metadata: Arc::new(metadata),
};
```

### Evaluation with Context

```rust
use liquers_core::context::{Context, SimpleSession, User};

let session = SimpleSession {
    user: User::new("test_user"),
};
let context = Context::new(envref.clone(), session, ());

let state = evaluate(envref.clone(), "my_query", Some(context)).await?;
```

---

## Result Extraction and Testing

### Testing String Results

```rust
let state = evaluate(envref, "world/greet", None).await?;
let value = state.try_into_string()?;
assert_eq!(value, "Hello, world!");
```

### Testing Typed Results

```rust
// Integer
let value = state.try_into_i32()?;
assert_eq!(value, 42);

// Float
let value = state.try_into_f64()?;
assert_eq!(value, 3.14);

// Boolean
let value = state.try_into_bool()?;
assert_eq!(value, true);
```

### Testing Metadata

```rust
let metadata = &state.metadata;
assert_eq!(metadata.get_data_format(), "json");
assert_eq!(metadata.type_identifier(), "application/json");
assert_eq!(metadata.filename(), "output.json");
```

### Testing Custom Value Types

For extended value types (e.g., DataFrames, Images):

```rust
use liquers_lib::value::ExtValueInterface;

// Test if state contains a DataFrame
assert!(state.data.as_polars_dataframe().is_ok());

let df = state.data.as_polars_dataframe()?;
assert_eq!(df.height(), 3);
assert_eq!(df.width(), 2);
```

### Testing Errors

```rust
use liquers_core::error::ErrorType;

let result = evaluate(envref, "nonexistent_command", None).await;
assert!(result.is_err());

if let Err(error) = result {
    assert_eq!(error.error_type, ErrorType::CommandNotFound);
    assert!(error.message.contains("nonexistent_command"));
}
```

---

## Complete Examples

### Example 1: Simple Command Test

```rust
#[tokio::test]
async fn test_simple_command() -> Result<(), Box<dyn std::error::Error>> {
    use liquers_core::context::SimpleEnvironment;
    use liquers_core::value::Value;

    type CommandEnvironment = SimpleEnvironment<Value>;
    let mut env = SimpleEnvironment::<Value>::new();

    // Register command
    fn hello(state: &State<Value>) -> Result<Value, Error> {
        Ok(Value::from("Hello, world!"))
    }

    let cr = &mut env.command_registry;
    register_command!(cr, fn hello(state) -> result)?;

    // Evaluate
    let envref = env.to_ref();
    let state = evaluate(envref, "hello", None).await?;

    // Test
    assert_eq!(state.try_into_string()?, "Hello, world!");
    Ok(())
}
```

### Example 2: Chained Commands with Parameters

```rust
#[tokio::test]
async fn test_chained_commands() -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = SimpleEnvironment<Value>;
    let mut env = SimpleEnvironment::<Value>::new();

    // First command: generates data
    fn data(_state: &State<Value>) -> Result<Value, Error> {
        Ok(Value::from("world"))
    }

    // Second command: processes data
    fn greet(state: &State<Value>, greeting: String) -> Result<Value, Error> {
        let what = state.try_into_string()?;
        Ok(Value::from(format!("{greeting}, {what}!")))
    }

    let cr = &mut env.command_registry;
    register_command!(cr, fn data(state) -> result)?;
    register_command!(cr, fn greet(state, greeting: String = "Hello") -> result)?;

    // Test with default parameter
    let envref = env.to_ref();
    let state = evaluate(envref.clone(), "data/greet", None).await?;
    assert_eq!(state.try_into_string()?, "Hello, world!");

    // Test with custom parameter
    let state = evaluate(envref, "data/greet-Hi", None).await?;
    assert_eq!(state.try_into_string()?, "Hi, world!");

    Ok(())
}
```

### Example 3: Using Memory Store and Recipes

```rust
#[tokio::test]
async fn test_with_store_and_recipes() -> Result<(), Box<dyn std::error::Error>> {
    use liquers_core::{
        metadata::Metadata,
        parse::parse_key,
        query::Key,
        store::{AsyncMemoryStore, AsyncStore},
    };
    use liquers_core::recipes::{RecipeList, Recipe};
    use liquers_lib::environment::DefaultEnvironment;
    use liquers_lib::value::Value;

    // Create memory store
    let memory_store = AsyncMemoryStore::new(&Key::new());

    // Add data to store
    let data_key = parse_key("data/input.txt")?;
    memory_store
        .set(&data_key, b"test data", &Metadata::new())
        .await?;

    // Create recipes
    let mut recipe_list = RecipeList::new();
    recipe_list.add_recipe(
        Recipe::new(
            "-R/processed/output.txt".to_string(),
            "Process Output".to_string(),
            "data-input.txt/uppercase".to_string(),
        )?
    )?;

    // Store recipes
    let recipes_key = parse_key("recipes.yaml")?;
    let yaml_content = serde_yaml::to_string(&recipe_list)?;
    memory_store
        .set(&recipes_key, yaml_content.as_bytes(), &Metadata::new())
        .await?;

    // Setup environment
    let mut env = DefaultEnvironment::<Value>::new();
    env.with_async_store(Box::new(memory_store));
    env.with_default_recipe_provider();

    // Register command
    fn uppercase(state: &State<Value>) -> Result<Value, Error> {
        let text = state.try_into_string()?;
        Ok(Value::from(text.to_uppercase()))
    }

    let cr = env.get_mut_command_registry();
    register_command!(cr, fn uppercase(state) -> result)?;

    // Evaluate recipe
    let envref = env.to_ref();
    let state = evaluate(envref, "-R/processed/output.txt", None).await?;

    // Test
    assert_eq!(state.try_into_string()?, "TEST DATA");
    Ok(())
}
```

### Example 4: Testing with Context

```rust
#[tokio::test]
async fn test_with_context() -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = SimpleEnvironment<Value>;
    let mut env = SimpleEnvironment::<Value>::new();

    // Command that uses context
    fn logged_operation<E: Environment>(
        state: &State<E::Value>,
        operation: String,
        context: Context<E>,
    ) -> Result<Value, Error> {
        context.info(&format!("Performing operation: {operation}"))?;
        let data = state.try_into_string()?;
        context.info(&format!("Processing data: {data}"))?;
        Ok(Value::from(format!("{operation}: {data}")))
    }

    let cr = &mut env.command_registry;
    register_command!(cr,
        fn logged_operation(state, operation: String, context) -> result
    )?;

    // Create initial state
    fn initial(_state: &State<Value>) -> Result<Value, Error> {
        Ok(Value::from("input"))
    }
    register_command!(cr, fn initial(state) -> result)?;

    // Evaluate
    let envref = env.to_ref();
    let state = evaluate(envref, "initial/logged_operation-process", None).await?;

    // Test
    assert_eq!(state.try_into_string()?, "process: input");
    Ok(())
}
```

### Example 5: Testing Polars DataFrame Commands

```rust
#[tokio::test]
async fn test_polars_dataframe() -> Result<(), Box<dyn std::error::Error>> {
    use liquers_lib::environment::DefaultEnvironment;
    use liquers_lib::value::{Value, ExtValueInterface};
    use liquers_core::metadata::Metadata;
    use std::sync::Arc;

    // Create environment
    let mut env = DefaultEnvironment::<Value>::new();
    env.with_default_recipe_provider();
    env.register_polars_commands()?;

    // Create CSV state directly (not via query to avoid newlines)
    let csv_data = "name,age\nAlice,30\nBob,25";
    let mut metadata = Metadata::new();

    let state = State {
        data: Arc::new(Value::from(csv_data.to_string())),
        metadata: Arc::new(metadata),
    };

    // Use try_to_polars_dataframe utility
    let df = liquers_lib::polars::util::try_to_polars_dataframe(&state)?;

    // Test DataFrame
    assert_eq!(df.height(), 2);
    assert_eq!(df.width(), 2);

    Ok(())
}
```

---

## Testing Assets

Asset tests fail for a small number of recurring reasons that have nothing to do with the behaviour
under test. Each rule below cost at least one debugging session, and each is stated because the
mistake it prevents looks like a passing test or like a bug in the code.

### `get().await` before `status()`

```rust
let asset = envref.evaluate("-R/b.txt").await?;
let _ = asset.get().await?;             // wait for a terminal status
assert_eq!(asset.status().await, Status::Ready);
```

`evaluate()` returns a **handle**, not a finished asset. On the queued path it commonly returns
while the asset is still `Processing`, so reading `status()` first reports a transient value.
Worse, it does not merely read wrong — `expire()` refuses a `Processing` asset, so the test fails
several lines later with an error about expiry and nothing pointing at the real cause.

`get()` is the wait. Discard the state if the test does not need it; the point is the `await`.

### `set_value` and `set_state` write to the store

They are *installs*, not inert test setup. For a keyed asset they persist, and they persist at
statuses the normal read gate hides. A test that uses one to "just put something in memory" has
also written the store, and a later assertion about what the store holds will be measuring the
setup rather than the behaviour.

### Simulate a restart by re-hydrating, never by sharing a store

Two environments over one live store is not a supported configuration, and a fixture for it would
exercise a coordination point the system does not have: the asset manager is the synchronization
mechanism, and the store is not equipped for that (see
[`reference/ASSETS.md`](../reference/ASSETS.md) §Who decides status).

Re-hydration is both the cheaper fixture and the more faithful one. A second process does not share
a live store object either — it reads persisted bytes:

```rust
let snapshot = {
    let envref = chain_env(...).await?;      // first environment, dropped at the end of this block
    let b = envref.evaluate("-R/b.txt").await?;
    let _ = b.get().await?;
    StoreSnapshot::capture(&envref.get_async_store(), &[recipes, a_key, b_key]).await?
};
let envref2 = rehydrated_env(&snapshot, ...).await?;   // fresh manager, fresh dependency manager
```

`StoreSnapshot` lives in `liquers-core/tests/fixtures/`. Capture *before* the state you want to
preserve is disturbed — expiring one asset cascades and rewrites its dependents — and use
`absorb` to merge entries captured at different moments.

Register **the same commands** in both environments. A command registered with a different
implementation version is a different command, and everything in the store will be invalidated for
a reason that has nothing to do with the test.

### A cache test asserts an evaluation counter, not a value

If a command is deterministic, a test that checks the value cannot tell a cache hit from a
recomputation — both produce the same string. Register a counting command, or count through an
`Arc<AtomicUsize>` the command closes over, and assert on the count. The same applies in reverse:
a test that asserts a *value* after a fast track is asserting that deserialization worked, which is
worth having, but it is not a test that the fast track happened.

### Size a store wrapper by compiling it

`AsyncStore` has two required methods. That number is misleading: the other twenty defaults are
**error stubs, not forwarding defaults** — `set`'s default is `Err(key_not_supported)`. A wrapper
that overrides only the required pair compiles cleanly and then fails every write at runtime.

Write the wrapper, compile the test that uses it, and add the forwards the failures name. Never
estimate the size of one from the trait's required-method count. See
[`STORE_IMPLEMENTATION_GUIDE.md`](./STORE_IMPLEMENTATION_GUIDE.md) §1.

### Assert a delta, not an absolute count

A counting fixture is counting environment construction as well as the code under test. Take a
baseline immediately before the call and assert on the difference, so the test does not break when
recipe loading changes for unrelated reasons.

```rust
let before = store.reads();
assert!(reloaded.try_fast_track().await?);
assert_eq!(store.reads() - before, 0, "the manager answers for a live dependency");
```

### Assert that the success path can succeed

Before adding a check that can *refuse* something, make sure a test asserts the un-refused path.
`try_fast_track` had no such test: the only test naming it asserted `false`. A check that failed
closed on everything would have left the whole suite green — every result still correct, merely
recomputed — and surfaced months later as a complaint about speed. Where one direction of a mistake
is invisible to the suite, write the test that makes it visible *first*, on unmodified code, so you
have seen it pass for the right reason.

---

## Best Practices

1. **Use `SimpleEnvironment` for simple tests** - faster and less setup
2. **Use `DefaultEnvironment` when you need stores or assets**
3. **Always declare `type CommandEnvironment`** before using `register_command!` macro
4. **Call `env.to_ref()` after all setup** - it consumes the environment
5. **Test both success and error cases** - verify error types and messages
6. **Avoid whitespace in queries** - use dashes or create State directly
7. **Use descriptive test names** - `test_command_with_invalid_input` not `test1`
8. **Test command chains** - verify data flows correctly through pipeline
9. **Clean up resources** - though Rust handles this automatically
10. **Use `#[tokio::test]` for async** - required for `evaluate()` calls

---

## Common Patterns

### Testing Data Transformations

```rust
#[tokio::test]
async fn test_data_transformation() -> Result<(), Box<dyn std::error::Error>> {
    // Setup
    let mut env = SimpleEnvironment::<Value>::new();

    // Register transform command
    fn double(state: &State<Value>) -> Result<Value, Error> {
        let n = state.try_into_i32()?;
        Ok(Value::from(n * 2))
    }

    fn initial(_state: &State<Value>) -> Result<Value, Error> {
        Ok(Value::from(21))
    }

    let cr = &mut env.command_registry;
    register_command!(cr, fn initial(state) -> result)?;
    register_command!(cr, fn double(state) -> result)?;

    // Test
    let envref = env.to_ref();
    let state = evaluate(envref, "initial/double", None).await?;
    assert_eq!(state.try_into_i32()?, 42);

    Ok(())
}
```

### Testing Error Handling

```rust
#[tokio::test]
async fn test_error_handling() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = SimpleEnvironment::<Value>::new();

    fn failing_command(state: &State<Value>) -> Result<Value, Error> {
        Err(Error::general_error("Intentional failure".to_string()))
    }

    let cr = &mut env.command_registry;
    register_command!(cr, fn failing_command(state) -> result)?;

    // Test that error is propagated
    let envref = env.to_ref();
    let result = evaluate(envref, "failing_command", None).await;

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.message.contains("Intentional failure"));

    Ok(())
}
```

---

## Troubleshooting

### "Can't parse query completely"
- Check for whitespace, newlines, or special characters in query
- Use dashes or underscores instead of spaces
- Create State directly for complex data

### "Command not found"
- Verify command was registered before calling `env.to_ref()`
- Check command name matches exactly (case-sensitive)
- Ensure `type CommandEnvironment` is declared

### "Type mismatch" errors
- Ensure Value type matches between environment and commands
- Use correct type alias for `CommandEnvironment`
- Check generic constraints on Environment trait

### Recipe not found
- Verify recipe provider was added before `to_ref()`
- Check recipe path format (should start with `-R/`)
- Ensure recipes.yaml exists in store at `-R/recipes.yaml`

---

## See Also

- [Command Registration Guide](./COMMAND_REGISTRATION_GUIDE.md)
- [Project Overview](../reference/PROJECT_OVERVIEW.md)
- [Polars Command Library](../reference/POLARS_COMMAND_LIBRARY.md)
- [Claude Development Guide](../../CLAUDE.md)

## History

| Date | Change | Source |
|---|---|---|
| 2026-09-15 | Added §Testing Assets: wait with `get().await` before reading `status()`; `set_value`/`set_state` persist; simulate a restart by re-hydrating rather than sharing a store, with `StoreSnapshot`; assert an evaluation counter rather than a value; size a store wrapper by compiling it; assert deltas; assert that the success path can succeed. Promoted from `CROSS-PROCESS-RELOAD-IS-UNTESTED`, whose own condition for promotion was a third recurrence. | `stale-dependency-status-finalization` |
| 2026-09-04 | Replaced the removed synchronous `AsyncStoreWrapper` example with direct `AsyncMemoryStore` setup and async byte writes. | `DOCS-ASYNC-STORE-WRAPPER-NO-LONGER-EXISTS` |
| 2026-09-01 | Corrected repository-relative links in the See Also section. | `DOCS-DEAD-LINKS-OUTSIDE-README` |
| 2026-08-31 | Updated `to_ref()` setup guidance to include command metadata-version refresh before environment sharing. | `design/refresh-command-metadata-versions/phase-5` |
| 2026-03-02 | Present at repository import; content unchanged since. Not reviewed against the implementation. | migration |
