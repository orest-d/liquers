# register_command! Macro Functional Specification

## Overview

The `register_command!` macro in `liquers-macro` provides a domain-specific language (DSL) for registering commands with the Liquers command execution framework. It generates wrapper code that bridges user-defined functions to the command registry system.

**Key Characteristics**:
- **Function-like macro** (not an attribute macro)
- Requires the target function to be **defined separately** before macro invocation
- Uses a **custom DSL** inspired by but not compatible with Rust function syntax
- Generates type-safe wrapper functions and metadata registration

## Basic Usage Pattern

```rust
use liquers_macro::register_command;

// 1. Define the command environment type alias (REQUIRED)
type CommandEnvironment = DefaultEnvironment<Value>;

// 2. Define the actual function
fn my_command(state: &State<Value>, arg: String) -> Result<Value, Error> {
    // implementation
}

// 3. Register using the macro
let cr = env.get_mut_command_registry();
register_command!(cr, fn my_command(state, arg: String) -> result)?;
```

## Macro Syntax

```
register_command!(
    <registry>,
    [async] fn <name>(<state_param>, <params...>) -> <return_type>
    [<metadata_statements...>]
)
```

### Components

| Component | Required | Description |
|-----------|----------|-------------|
| `<registry>` | Yes | Identifier of `CommandRegistry<E>` instance |
| `async` | No | Makes the command async |
| `<name>` | Yes | Function name (must match defined function) |
| `<state_param>` | No | How to pass input state to function |
| `<params>` | No | Command parameters |
| `<return_type>` | Yes | Either `result` or `value` |
| `<metadata_statements>` | No | Command metadata (label, doc, etc.) |

---

## State Parameter

The first parameter position (before the comma) specifies how the input state is passed to the command function.

| DSL Keyword | Function Receives | Use Case |
|-------------|-------------------|----------|
| `state` | `&State<V>` (sync) or `State<V>` (async) | Full state with metadata access |
| `value` | `V` (cloned from state.data) | When only the value is needed |
| `text` | `&str` (converted via `try_into_string()`) | Text processing commands |
| *(omitted)* | Nothing | First commands that generate data |

### Examples

```rust
// State parameter
fn cmd(state: &State<Value>) -> Result<Value, Error> { ... }
register_command!(cr, fn cmd(state) -> result)?;

// Value parameter
fn cmd(value: Value) -> Result<Value, Error> { ... }
register_command!(cr, fn cmd(value) -> result)?;

// Text parameter
fn cmd(text: &str) -> Result<Value, Error> { ... }
register_command!(cr, fn cmd(text) -> result)?;

// No state (first command)
fn cmd() -> Result<Value, Error> { ... }
register_command!(cr, fn cmd() -> result)?;
```

---

## Command Parameters

Parameters are specified after the state parameter, separated by commas.

### Syntax

```
<name>: <Type> [injected] [= <default_value>] [(label: "...", gui: ...)]
```

| Part | Required | Description |
|------|----------|-------------|
| `<name>` | Yes | Parameter name (no leading `_`, no `__`) |
| `<Type>` | Yes | Rust type |
| `injected` | No | Mark as injected from context |
| `= <default_value>` | No | Default value |
| `(...)` | No | Parameter metadata |

### Supported Types

The macro recognizes these types for metadata generation:

| Type | ArgumentType Generated |
|------|----------------------|
| `i8`, `i16`, `i32`, `i64`, `isize` | `Integer` |
| `u8`, `u16`, `u32`, `u64`, `usize` | `Integer` |
| `Option<i32>`, `Option<i64>`, etc. | `IntegerOption` |
| `f32`, `f64` | `Float` |
| `Option<f32>`, `Option<f64>` | `FloatOpt` |
| `bool` | `Boolean` |
| `String` | `String` |
| `Value`, `Any`, `CommandValue` | `Any` |
| Other types | `Any` |

### Default Values

| Syntax | Example | Description |
|--------|---------|-------------|
| String literal | `= "default"` | String default |
| Boolean | `= true` / `= false` | Boolean default |
| Integer | `= 42` | Integer default |
| Float | `= 3.14` | Float default |
| Query | `= query "path/to/query"` | Query that resolves at runtime |

### Injected Parameters

Injected parameters are extracted from the `Context` rather than from action arguments. The type must implement `InjectedFromContext<E>`.

```rust
fn cmd(state: &State<Value>, payload: MyPayload) -> Result<Value, Error> { ... }
register_command!(cr, fn cmd(state, payload: MyPayload injected) -> result)?;
```

Built-in injectable: `E::Payload` (the environment's payload type).

### Parameter Metadata

Additional parameter configuration in parentheses:

```rust
register_command!(cr,
    fn cmd(state,
        width: i32 = 80 (label: "Width", gui: IntegerSlider(10, 200, 1))
    ) -> result
)?;
```

| Statement | Description |
|-----------|-------------|
| `label: "..."` | Human-readable label for UI |
| `gui: <GuiInfo>` | UI rendering hint |

### GUI Info Variants

| Variant | Syntax | Description |
|---------|--------|-------------|
| `TextField` | `TextField 20` | Text field with width hint |
| `CodeField` | `CodeField 40, "rust"` | Code editor with language |
| `TextArea` | `TextArea 80, 10` | Multi-line text (width, height) |
| `CodeArea` | `CodeArea 80, 20, "sql"` | Multi-line code editor |
| `IntegerField` | `IntegerField` | Integer input |
| `IntegerRange` | `IntegerRange(0, 100)` | Integer with min/max |
| `IntegerSlider` | `IntegerSlider(0, 100, 1)` | Slider (min, max, step) |
| `FloatField` | `FloatField` | Float input |
| `FloatSlider` | `FloatSlider(0.0, 1.0, 0.1)` | Float slider |
| `Checkbox` | `Checkbox` | Boolean checkbox |
| `RadioBoolean` | `RadioBoolean("Yes", "No")` | Boolean as radio buttons |
| `HorizontalRadioEnum` | `HorizontalRadioEnum` | Enum as horizontal radios |
| `VerticalRadioEnum` | `VerticalRadioEnum` | Enum as vertical radios |
| `EnumSelector` | `EnumSelector` | Enum dropdown |
| `ColorString` | `ColorString` | Color picker |
| `DateField` | `DateField 10` | Date input with width |
| `Hide` | `Hide` | Hidden parameter |
| `None` | `None` | No GUI info |

---

## Context Parameter

The special `context` keyword passes the execution context to the function:

```rust
fn cmd(state: &State<Value>, context: Context<E>) -> Result<Value, Error> {
    context.info("Processing...");
    // ...
}
register_command!(cr, fn cmd(state, context) -> result)?;
```

Context provides:
- `envref` - Reference to Environment
- `assetref` - Reference to current Asset
- `cwd_key` - Current working directory
- `service_tx` - Channel for progress/logging
- Logging methods: `info()`, `warning()`, `error()`

---

## Return Type

| DSL Keyword | Function Returns | Macro Generates |
|-------------|------------------|-----------------|
| `result` | `Result<V, Error>` | `res` (pass through) |
| `value` | `V` | `Ok(res)` (wrap in Ok) |

```rust
// Returns Result
fn cmd(state: &State<Value>) -> Result<Value, Error> { ... }
register_command!(cr, fn cmd(state) -> result)?;

// Returns Value directly
fn cmd(state: &State<Value>) -> Value { ... }
register_command!(cr, fn cmd(state) -> value)?;
```

---

## Metadata Statements

Metadata statements follow the function signature, one per line (no separators):

```rust
register_command!(cr,
    fn my_cmd(state, arg: String) -> result
    label: "My Command"
    doc: "Does something useful"
    namespace: "utils"
    realm: "backend"
    filename: "output.txt"
    volatile: true
    preset: "my_cmd-default" (label: "Default", description: "Run with defaults")
    next: "another_cmd"
)?;
```

| Statement | Type | Description |
|-----------|------|-------------|
| `label: "..."` | String | Human-readable command name |
| `doc: "..."` | String | Documentation/description |
| `namespace: "..."` or `ns: "..."` | String | Command namespace |
| `realm: "..."` | String | Command realm |
| `filename: "..."` | String | Default output filename |
| `volatile: true/false` | Bool | Mark command as volatile |
| `preset: "action" (...)` | Preset | Predefined action configuration |
| `next: "action" (...)` | Preset | Suggested follow-up action |

### Presets and Next

Presets define common invocations; next suggests follow-up commands:

```rust
preset: "filter-column-value"
preset: "filter-name-John" (label: "Filter by John", description: "Filter where name is John")
next: "to_json"
next: "save" (label: "Save Result", description: "Save to store")
```

---

## Async Commands

Prefix with `async` for async commands:

```rust
async fn fetch_data(state: State<Value>, url: String) -> Result<Value, Error> {
    // async implementation
}

register_command!(cr, async fn fetch_data(state, url: String) -> result)?;
```

**Differences from sync**:
- State is passed by value (`State<V>`) not reference
- Function must be `async fn`
- Uses `register_async_command()` internally
- Returns boxed future

---

## Generated Code

The macro generates:

1. **Wrapper function** (`<name>__CMD_`) that:
   - Extracts parameters from `CommandArguments`
   - Converts state according to state parameter type
   - Calls the original function
   - Handles result conversion

2. **Registration function** (`REGISTER__<name>`) that:
   - Creates the wrapper
   - Registers with `CommandRegistry`
   - Sets up `CommandMetadata` with arguments, label, doc, etc.

3. **Invocation** of the registration function

### Example Generated Code

For:
```rust
fn greet(state: &State<Value>, greeting: String) -> Result<Value, Error> { ... }
register_command!(cr, fn greet(state, greeting: String = "Hello") -> result
    label: "Greet"
)?;
```

Generates (simplified):
```rust
{
    use futures::FutureExt;

    #[allow(non_snake_case)]
    pub fn REGISTER__greet(
        registry: &mut CommandRegistry<CommandEnvironment>
    ) -> Result<&mut CommandMetadata, Error> {

        #[allow(non_snake_case)]
        fn greet__CMD_(
            state: &State<<CommandEnvironment as Environment>::Value>,
            arguments: CommandArguments<CommandEnvironment>,
            context: Context<CommandEnvironment>,
        ) -> Result<<CommandEnvironment as Environment>::Value, Error> {
            let greeting__par: String = arguments.get(0, "greeting")?;
            let res = greet(state, greeting__par);
            res
        }

        let mut cm = registry.register_command(
            CommandKey::new("", "", "greet"),
            greet__CMD_
        )?;
        cm.with_label("Greet");
        cm.arguments = vec![ArgumentInfo {
            name: "greeting".to_string(),
            label: "greeting".to_string(),
            default: CommandParameterValue::Value(Value::String("Hello".to_string())),
            argument_type: ArgumentType::String,
            multiple: false,
            injected: false,
            gui_info: ArgumentGUIInfo::TextField(20),
            ..Default::default()
        }];
        cm.with_filename("");
        Ok(cm)
    }

    REGISTER__greet(cr)
}
```

---

## Type Requirements

### CommandEnvironment Type Alias

The macro requires a type alias named `CommandEnvironment` in scope:

```rust
type CommandEnvironment = DefaultEnvironment<Value>;
// or
type CommandEnvironment = SimpleEnvironment<Value>;
// or your custom environment implementing Environment trait
```

### Context Import

The `Context` type must be in scope:

```rust
use liquers_core::context::Context;
```

### FutureExt for Async

Async commands require `futures::FutureExt` (automatically imported by macro):

```rust
// Added by macro: use futures::FutureExt;
```

---

## Error Handling

The macro returns `Result<&mut CommandMetadata, Error>`, so use `?` operator:

```rust
register_command!(cr, fn cmd(state) -> result)?;
// or
register_command!(cr, fn cmd(state) -> result).expect("registration failed");
```

Common errors:
- Parameter name starts with `_` or contains `__`
- Unknown metadata statement
- Invalid default value type
- Type mismatch between function and DSL

---

## Complete Example

```rust
use liquers_core::{
    context::{Context, Environment, DefaultEnvironment},
    error::Error,
    state::State,
    value::Value,
};
use liquers_macro::register_command;

type CommandEnvironment = DefaultEnvironment<Value>;

// Sync command with multiple parameters
fn filter_data(
    state: &State<Value>,
    column: String,
    value: String,
    case_sensitive: bool,
) -> Result<Value, Error> {
    // implementation
    Ok(state.data.clone())
}

// Async command with context
async fn fetch_remote(
    state: State<Value>,
    url: String,
    context: Context<CommandEnvironment>,
) -> Result<Value, Error> {
    context.info(&format!("Fetching from {}", url));
    // async implementation
    Ok(Value::none())
}

// First command (no state)
fn datetime() -> Result<Value, Error> {
    Ok(Value::from(chrono::Utc::now().to_rfc3339()))
}

pub fn register_commands(mut env: DefaultEnvironment<Value>) -> Result<DefaultEnvironment<Value>, Error> {
    let cr = env.get_mut_command_registry();

    register_command!(cr,
        fn filter_data(state,
            column: String (label: "Column Name"),
            value: String (label: "Filter Value"),
            case_sensitive: bool = false (gui: Checkbox)
        ) -> result
        label: "Filter Data"
        doc: "Filter rows where column matches value"
        namespace: "data"
        preset: "filter_data-name-John" (label: "Filter by John")
    )?;

    register_command!(cr,
        async fn fetch_remote(state, url: String, context) -> result
        label: "Fetch Remote"
        doc: "Fetch data from remote URL"
        volatile: true
    )?;

    register_command!(cr,
        fn datetime() -> result
        label: "Date/Time"
        doc: "Returns current date and time"
        volatile: true
        filename: "datetime.txt"
    )?;

    Ok(env)
}
```

---

## References

- Implementation: `liquers-macro/src/lib.rs`
- Usage examples: `liquers-lib/src/commands.rs`, `liquers-core/tests/async_hellow_world.rs`
- Command framework: `liquers-core/src/commands.rs`
- Command metadata: `liquers-core/src/command_metadata.rs`

---

*Last updated: 2025-01-18*
