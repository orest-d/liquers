# Command Registration Guide

This guide covers defining and registering new commands in Liquers. It covers both the `register_command!` macro approach and manual registration.

## Quick Reference

| Approach | Use Case | Complexity |
|----------|----------|-----------|
| `register_command!` macro | Standard commands with metadata | Low |
| Manual registration | Fine-grained control, closures, tests | Medium |
| Generic Environment | Library commands for any environment | High |

---

## 1. Using register_command! Macro (Recommended)

The `register_command!` macro is the standard way to register commands. It provides a DSL for defining commands with metadata and parameter validation.

### Basic Pattern

```rust
use liquers_macro::register_command;
use liquers_core::{error::Error, state::State, context::Context};
use liquers_lib::value::Value;
use liquers_lib::environment::DefaultEnvironment;

// 1. Define the function separately
fn my_command(state: &State<Value>, name: String) -> Result<Value, Error> {
    let input = state.try_into_string()?;
    Ok(Value::from(format!("Hello, {}!", name)))
}

// 2. Register it in a registration function
pub fn register_commands(mut env: DefaultEnvironment<Value>) -> Result<DefaultEnvironment<Value>, Error> {
    let cr = env.get_mut_command_registry();

    type CommandEnvironment = DefaultEnvironment<Value>;
    register_command!(cr, fn my_command(state, name: String) -> result)?;

    Ok(env)
}

// 3. Call the registration function when initializing your environment
let env = DefaultEnvironment::<Value>::new();
let env = register_commands(env)?;
```

### Type Alias Requirement

The macro requires a `type CommandEnvironment` definition that matches your environment type:

```rust
type CommandEnvironment = DefaultEnvironment<Value>;
```

This type alias is used by the macro to generate the correct wrapper code.

### Macro DSL Syntax

**Full signature:**
```
register_command!(
    <registry>,
    [async] fn <name>(<state_param>, <param1>, <param2>, ...) -> <return_type>
    [metadata statements]
)
```

See `specs/REGISTER_COMMAND_FSD.md` for the complete DSL specification including:
- State parameter variations (state, value, text)
- Parameter types and defaults
- Injected parameters
- Metadata statements (label, doc, namespace, realm, etc.)

### Common Examples

**Sync command with state and parameter:**
```rust
fn greet(state: &State<Value>, greeting: String) -> Result<Value, Error> {
    let input = state.try_into_string()?;
    Ok(Value::from(format!("{}, {}!", greeting, input)))
}

register_command!(cr,
    fn greet(state, greeting: String = "Hello") -> result
    label: "Greet"
    doc: "Greet the input with a customizable greeting"
)?;
```

**Async command:**
```rust
async fn fetch_data(state: State<Value>, url: String) -> Result<Value, Error> {
    // async implementation
    Ok(Value::from("data"))
}

register_command!(cr, async fn fetch_data(state, url: String) -> result)?;
```

**Command with context:**
```rust
fn log_info(state: &State<Value>, context: Context<DefaultEnvironment<Value>>) -> Result<Value, Error> {
    context.info("Processing data")?;
    Ok(state.data.clone())
}

register_command!(cr,
    fn log_info(state, context) -> result
    doc: "Log info and pass through input"
)?;
```

**Generator command (no input state):**
```rust
fn create_empty() -> Result<Value, Error> {
    Ok(Value::from(""))
}

register_command!(cr, fn create_empty() -> result)?;
```

---

## 2. Manual Registration

Use manual registration when you need more control, such as:
- Registering closures or lambda functions
- Fine-tuning parameter handling
- Complex metadata configuration
- Testing specific scenarios

### CommandRegistry Methods

**Synchronous command:**
```rust
pub fn register_command<K, F>(&mut self, key: K, f: F) -> Result<&mut CommandMetadata, Error>
where
    K: Into<CommandKey>,
    F: (Fn(&State<E::Value>, CommandArguments<E>, Context<E>) -> Result<E::Value, Error>) + Sync + Send + 'static,
```

**Asynchronous command:**
```rust
pub fn register_async_command<K, F>(
    &mut self,
    key: K,
    f: F,
) -> Result<&mut CommandMetadata, Error>
where
    K: Into<CommandKey>,
    F: (Fn(
            State<E::Value>,
            CommandArguments<E>,
            Context<E>,
        ) -> std::pin::Pin<Box<dyn Future<Output = Result<E::Value, Error>> + Send + 'static>>,
    ) + Sync + Send + 'static,
```

### Sync Command Example

```rust
use liquers_core::commands::{CommandArguments, CommandRegistry};
use liquers_core::command_metadata::CommandKey;
use liquers_core::context::SimpleEnvironment;
use liquers_core::value::Value;

let mut registry = CommandRegistry::<SimpleEnvironment<Value>>::new();

// Register a simple command returning a constant value
let key = CommandKey::new_name("answer");
registry.register_command(key, |_state, _args, _context| {
    Ok(Value::from(42))
})?;

// Register a command using state and parameters
let key = CommandKey::new_name("greet");
registry.register_command(key, |state, args, _context| {
    let input = state.try_into_string()?;
    let greeting: String = args.get(0, "greeting")?;
    Ok(Value::from(format!("{}, {}!", greeting, input)))
})?;
```

### Async Command Example

```rust
let key = CommandKey::new_name("async_task");
registry.register_async_command(key, |state, _args, _context| {
    Box::pin(async move {
        // Async implementation
        Ok(Value::from("done"))
    })
})?;
```

### Parameter Extraction from CommandArguments

```rust
// Get parameter by position
let name: String = args.get(0, "name")?;

// Get parameter with default handling
let count: i32 = args.get(1, "count").unwrap_or(10);

// Get raw parameter value
let param = args.get_parameter(0, "name")?;
let value = param.value();
```

### Metadata Configuration

After registration, customize the command metadata:

```rust
let metadata = registry.register_command(key, |_, _, _| Ok(Value::from(42)))?;
metadata
    .with_label("The Answer")
    .with_doc("Returns the ultimate answer to everything");
```

---

## 3. Generic Environment Commands (Library Commands)

Generic commands work with any `Environment` type, enabling a rich library of commands that users can employ with their custom environments and value types.

### Purpose

Generic environment commands provide:
- **Reusability**: Same command works with different environments and value types
- **Type safety**: Generic constraints ensure compatibility
- **User extensibility**: Users can define custom environments and still use library commands
- **Rich ecosystem**: Users inherit a library of production-ready commands

### Requirements for Generic Commands

```rust
use liquers_core::context::Environment;
use liquers_core::error::Error;
use liquers_core::state::State;

// Function signature with generic Environment
pub fn my_command<E: Environment>(state: &State<E::Value>) -> Result<E::Value, Error>
where
    E::Value: SomeRequiredTrait, // If needed
{
    // Implementation using only E::Value and Environment trait methods
    Ok(E::Value::from_string("result".to_string()))
}
```

### Key Principles

1. **Use `E::Value` not concrete types**: This makes the command environment-agnostic
2. **Restrict traits only if necessary**: Minimize trait bounds to maximize compatibility
3. **Access context through `Context<E>`**: Get environment services through context

### Example: Generic Text Conversion

```rust
/// Generic command trying to convert any value to text representation.
pub fn to_text<E: Environment>(state: &State<E::Value>) -> Result<E::Value, Error> {
    Ok(E::Value::from_string(state.try_into_string()?))
}
```

This command:
- Works with any `Environment` E
- Uses `E::Value::from_string()` instead of `Value::from()`
- Works for users with custom value types that implement the required conversion

### Example: Conditional Trait Bounds

```rust
pub fn label<E: Environment>(text: String, _context: Context<E>) -> Result<E::Value, Error>
where
    E::Value: UIValueExtension,  // Only works with UI-capable values
{
    Ok(E::Value::from_ui(move |ui| {
        ui.label(&text);
        Ok(())
    }))
}
```

This command:
- Only works with environments where `Value` implements `UIValueExtension`
- Maintains type safety at compile time
- Users without `UIValueExtension` cannot accidentally use this command

### Registration Pattern for Generic Commands

```rust
pub fn register_commands(
    mut env: DefaultEnvironment<Value>,
) -> Result<DefaultEnvironment<Value>, Error> {
    let cr = env.get_mut_command_registry();

    type CommandEnvironment = DefaultEnvironment<Value>;

    // Register generic commands
    register_command!(cr,
        fn to_text(state) -> result
        label: "To text"
        doc: "Convert input to string representation"
    )?;

    // Register commands with trait bounds
    register_command!(cr,
        fn label(text: String, context) -> result
        label: "Label"
        doc: "Display text as a UI label"
    )?;

    Ok(env)
}
```

**Important**: The `type CommandEnvironment` must still be the concrete environment type (e.g., `DefaultEnvironment<Value>`), but the command functions themselves are generic.

### When to Use Generic Commands

Use generic commands when:
- The command doesn't depend on specific value type features
- The command should be available in a library for any user environment
- The command converts between basic types (string, metadata, etc.)
- The command applies GUI operations (when trait bounds allow)

Don't use generic commands when:
- The command requires specific value types (Polars, Images, etc.)
- The command accesses specialized environment features
- The command is environment-specific (desktop, web, etc.)

### Testing Generic Commands

```rust
#[tokio::test]
async fn test_generic_command() {
    type CommandEnvironment = SimpleEnvironment<Value>;
    let mut env = SimpleEnvironment::<Value>::new();

    // Commands registered via macro work generically
    let cr = &mut env.command_registry;
    register_command!(cr, fn to_text(state) -> result)?;

    // Can test with any compatible environment
}
```

---

## 4. Organization and Structure

### File Layout in liquers-lib

```
liquers-lib/
├── src/
│   ├── commands.rs              # Core library commands
│   ├── environment.rs           # DefaultEnvironment definition
│   ├── egui/
│   │   ├── commands.rs          # GUI-specific commands
│   │   └── mod.rs
│   └── value/
│       ├── mod.rs               # Value and trait definitions
│       ├── simple.rs            # SimpleValue types
│       └── extended.rs          # ExtValue types
```

### Command Registration Functions

Follow this pattern for organizing commands:

```rust
// commands.rs - Core commands
pub fn register_commands(
    mut env: DefaultEnvironment<Value>,
) -> Result<DefaultEnvironment<Value>, Error> {
    // Register core commands
    let cr = env.get_mut_command_registry();
    type CommandEnvironment = DefaultEnvironment<Value>;

    register_command!(cr, fn to_text(state) -> result)?;
    // ... more commands

    Ok(env)
}

// egui/commands.rs - GUI commands
pub fn register_commands(
    mut env: DefaultEnvironment<Value>,
) -> Result<DefaultEnvironment<Value>, Error> {
    let cr = env.get_mut_command_registry();
    type CommandEnvironment = DefaultEnvironment<Value>;

    register_command!(cr, fn label(text: String, context) -> result)?;
    // ... more UI commands

    Ok(env)
}

// lib.rs - Combine all registrations
pub fn register_all_commands(
    mut env: DefaultEnvironment<Value>,
) -> Result<DefaultEnvironment<Value>, Error> {
    env = commands::register_commands(env)?;
    env = egui::commands::register_commands(env)?;
    Ok(env)
}
```

---

## 5. Best Practices

### Use register_command! by Default

```rust
// Preferred - uses macro
register_command!(cr, fn my_command(state, name: String) -> result)?;

// Only use manual registration when you need:
// - Closures
// - Precise control
// - Testing
registry.register_command(key, |state, args, _| {
    // Manual handling
})?;
```

### Provide Comprehensive Metadata

```rust
// Good - clear documentation
register_command!(cr,
    fn process_data(state, format: String = "json") -> result
    label: "Process data"
    doc: "Transform input data to the specified format (json, csv, yaml)"
    namespace: "data"
)?;

// Avoid - minimal metadata
register_command!(cr, fn process_data(state, format: String) -> result)?;
```

### Keep Command Functions Pure

```rust
// Good - function is deterministic
fn multiply(state: &State<Value>, factor: i32) -> Result<Value, Error> {
    let num = state.try_into_string()?.parse::<i32>()?;
    Ok(Value::from(num * factor))
}

// Avoid - side effects
fn multiply(state: &State<Value>, factor: i32) -> Result<Value, Error> {
    println!("Multiplying!"); // Side effect
    let num = state.try_into_string()?.parse::<i32>()?;
    Ok(Value::from(num * factor))
}
```

Use `context.info()` or `context.log()` for logging instead of `println!`.

### Error Handling

```rust
use liquers_core::error::Error;

// Good - specific error types
fn parse_number(state: &State<Value>) -> Result<Value, Error> {
    let text = state.try_into_string()?;
    let num = text.parse::<i32>()
        .map_err(|e| Error::general_error(format!("Invalid number: {}", e)))?;
    Ok(Value::from(num))
}

// Avoid - unwrap/expect in library code
fn parse_number(state: &State<Value>) -> Result<Value, Error> {
    let text = state.try_into_string()?;
    let num = text.parse::<i32>().unwrap(); // ❌ Never in library code
    Ok(Value::from(num))
}
```

### Naming Conventions

```rust
// Good - clear, descriptive names
fn convert_to_csv(...) -> Result<...> { ... }
fn extract_metadata(...) -> Result<...> { ... }
fn filter_by_name(...) -> Result<...> { ... }

// Avoid - vague names
fn process(...) -> Result<...> { ... }
fn transform(...) -> Result<...> { ... }
fn apply(...) -> Result<...> { ... }
```

---

## 6. Reference

### Related Documentation

- `specs/REGISTER_COMMAND_FSD.md` - Complete macro syntax specification
- `CLAUDE.md` - "Common Tasks > Adding a Command" section
- `liquers-core/src/commands.rs` - CommandRegistry implementation and tests
- `liquers-lib/src/commands.rs` - Example command library

### Example Projects

- **Core commands**: `liquers-lib/src/commands.rs` (generic commands)
- **GUI commands**: `liquers-lib/src/egui/commands.rs` (with trait bounds)
- **Tests**: `liquers-core/tests/async_hellow_world.rs` (complete example)
- **Manual registration**: `liquers-core/src/commands.rs` tests section
