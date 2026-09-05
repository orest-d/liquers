---
title: Command Registration Guide
kind: guide
audience: internal
area: [core/commands, macro]
reviewed: 2026-09-05
---
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

See `specs/reference/REGISTER_COMMAND_FSD.md` for the complete DSL specification including:
- State parameter variations (state, value, text)
- Parameter types and defaults
- Injected parameters
- Metadata statements (label, doc, namespace, realm, etc.)

### Passing the working directory (or any relative query) into a command

A command cannot read the working directory from its `Context`. `get_cwd_key` and
`set_cwd_key` are crate-private, and `Context::evaluate`, `apply` and
`get_dependency_state` refuse a query with a CWD-relative operand:

```
Query '-R/./data.csv' is relative and cannot be evaluated from a command.
Take the current directory as a link argument (`-R-key/.`) and build an absolute
query from it.
```

This is deliberate. A command that varies its result by directory produces a value
its query does not describe, so two directories share one query text and one cache
entry for results that legitimately differ. The fix is to make the directory part of
the query, where it belongs.

**Declare it as a default link argument.** `-R-key/<key>` evaluates to the key
itself as a `Value::Key`, and `.` resolves to the working directory:

```rust
use liquers_core::query::Key;

fn where_am_i(state: &State<Value>, dir: Key) -> Result<Value, Error> {
    Ok(Value::from(format!("dir={}", dir.encode())))
}

register_command!(cr, fn where_am_i(state, dir: Key = query "-R-key/.") -> result)?;
```

Evaluated under `-R-cwd/proj/a/-/where_am_i`, `dir` arrives as `proj/a`.

**Build absolute queries from it** when the command needs to reach a sibling:

```rust
async fn read_sibling(
    _state: State<Value>,
    dir: Key,
    context: Context<CommandEnvironment>,
) -> Result<Value, Error> {
    // `dir.join("hello.txt")` is absolute, so this is accepted.
    let asset = context.evaluate(&Query::from(dir.join("hello.txt"))).await?;
    asset.get().await?.try_into_string().map(Value::from)
}
```

**Any relative query works the same way**, not just the directory. A default link is
an ordinary query, so `dir: Key = query "-R-key/./config"` yields
`<cwd>/config`, and a non-key default such as
`settings: String = query "-R/./settings.json/-/to_text"` reads a file beside the
recipe. Freezing resolves the relative part against the entry CWD before the link is
evaluated (see `DOC_08_RECIPES_PLANS.md`, "Freezing").

Three properties follow, and they are the reason to prefer this over ambient state:

- **Explicit.** The dependency appears in the plan and in the asset's dependency
  records, so invalidation and cycle detection see it.
- **Overridable.** A caller can supply the argument to point the command somewhere
  else — `where_am_i-~X~-R-key/other/place~E` — or a recipe can override it by name.
- **Identifying.** Once promoted into the query, the resolved directory is part of
  what names the result, so two directories get two cache entries and two callers in
  the same directory share one.

**Argument types.** A key-valued link arrives through `TryFrom<Value> for Key`; a
literal `Key` written in the query text is parsed by `FromParameterValue`. Both
exist, so `dir: Key` is declarable either way. Other argument types convert as
usual — a link delivering `Value::Text` binds to a `String` parameter.

**Pitfall — argument order.** A relative default is promoted into the query only
when every earlier argument slot is already written. If an earlier argument is also
omitted, promotion is skipped for that action rather than binding the link to the
wrong slot. Declare a relative-default argument **first**, or supply the arguments
before it explicitly, if you want the promoted form.

### Accepting a variable number of parameters

A command's declared arity is binding: a query that supplies more parameters than the command
declares fails at plan build with a positioned error. To accept a variable-length list, mark the
argument `multiple`. It consumes every remaining parameter.

```rust
fn select_columns(state: &State<Value>, columns: Vec<String>) -> Result<Value, Error> { /* … */ }

register_command!(cr,
    fn select_columns(state, columns: Vec<String> multiple) -> result
    namespace: "pl"
)?;
```

```
ns-pl/select_columns-date-amount-status     three columns
ns-pl/select_columns                        no columns - an empty Vec, not an error
```

The flag goes in the same slot as `injected` — after the type, before any default or metadata
parentheses — and the two are mutually exclusive:

```
<name>: <Type> [injected | multiple] [= <default_value>] [(label: "...", gui: ..., ...)]
```

For hand-built or imported `CommandMetadata`, the same rule is checked by
`EnvironmentBuilder::build()`. Call `builder.validate()` before the consuming `build()` when a
GUI or custom logger needs the complete `IssueReport`; otherwise `build()` emits the full report
and returns a compact error when validation finds an error.

**The type must be a container.** `Vec<T>` is what is recognised, and the element type `T` is what
the argument's `ArgumentType` is derived from. That derivation is not cosmetic: each action
parameter is parsed through `ArgumentType`, so `rows: Vec<i64> multiple` gives you `i64` elements
and rejects `pick_rows-1-x-3` at plan build, pointing at `x`.

**A variadic argument takes no default; it defaults to the empty list**, as with Python's `*args`.
A command that needs at least one element must say so itself — the plan builder cannot know:

```rust
if columns.is_empty() {
    return Err(Error::general_error(
        "select_columns requires at least one column name".to_string(),
    ));
}
```

**It must be the last argument that consumes a query parameter.** Anything declared after it could
never receive a value, so the macro rejects the declaration. Arguments marked `injected` and the
`context` parameter may follow it, because neither consumes a query parameter.

#### Naming a value that contains the separator

`-` separates *parameters*, so a literal dash inside one parameter is escaped `~_`. With a variadic
argument the two spellings mean different things, and both are useful:

```
ns-pl/select_columns-a-b      two parameters   -> the columns "a" and "b"
ns-pl/select_columns-a~_b     one parameter    -> the single column "a-b"
```

This is why a command taking a list should declare it variadic rather than taking one `String` and
splitting it internally: a self-split cannot tell those two apart, and it mangles any value that
legitimately contains the separator.

#### If the declaration will not compile

The macro rejects malformed variadic declarations at compile time, naming the problem:

| Declaration | Error |
|---|---|
| `c: String multiple` | ``a `multiple` argument must have a container type; `String` is not one. Expected `Vec<String>` `` |
| `c: Vec<String> multipel` | ``unknown argument flag `multipel`; expected `injected` or `multiple` `` |
| `c: Vec<String> injected multiple` | ``an argument cannot be both `injected` and `multiple` `` |
| `c: Vec<String> multiple multiple` | ``duplicate argument flag `multiple` `` |
| `c: Vec<String> multiple = "x"` | ``a `multiple` argument cannot have a default value; it defaults to the empty list`` |
| `fn f(state, a: Vec<String> multiple, b: i32)` | ``argument `b` follows the `multiple` argument `a` and can never receive a value`` |

Note that unknown flags are rejected rather than ignored, so a misspelling of `multiple` or
`injected` is a build error rather than a silently scalar argument.

#### Retrieving the value manually

Generated code uses `CommandArguments::get_multiple`, not `get`:

```rust
let columns: Vec<String> = arguments.get_multiple(0, "columns")?;
```

`get` cannot serve a variadic argument — `Vec<T>` satisfies neither of its bounds — and a blanket
`FromParameterValue<Vec<T>>` impl would overlap the existing `Vec<V: ValueInterface>` one. Use
`get_multiple` when registering a variadic command manually.

### Return Type Requirement

**Important**: Command functions registered with the `register_command!` macro **must** return either:
- `Result<Value, Error>` for concrete environments (when using `-> result`)
- `Result<E::Value, Error>` for generic environments (when using `-> result`)
- `Value` for concrete environments (when using `-> value`)
- `E::Value` for generic environments (when using `-> value`)

The macro **does not** support automatic conversion from other types. For example, a function returning `Result<i32, Error>` will fail to compile:

```rust
// ❌ This will NOT compile
fn get_number(_state: &State<Value>) -> Result<i32, Error> {
    Ok(42)
}
register_command!(cr, fn get_number(state) -> result)?;
// Error: expected `Result<Value, Error>`, found `Result<i32, Error>`

// ✅ Instead, wrap the value explicitly
fn get_number(_state: &State<Value>) -> Result<Value, Error> {
    Ok(Value::from(42))
}
register_command!(cr, fn get_number(state) -> result)?;
```

This restriction exists because the macro generates wrapper code that expects the exact return type. If you need to convert from other types, perform the conversion inside your function before returning.

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

### Enum Parameters

Use enum metadata to make allowed values explicit and avoid manual `String` validation in command code.

**Inline string enum:**
```rust
register_command!(cr,
    fn resize(state,
        method: String = "lanczos3" (
            enum: ["nearest", "triangle", "catmullrom", "gaussian", "lanczos3"]
        )
    ) -> result
)?;
```

**Alias to mapped value:**
```rust
register_command!(cr,
    fn rotate(state,
        method: String = "bilinear" (
            enum: {"linear" => "bilinear", "hq" => "lanczos3"}
        )
    ) -> result
)?;
```

**Typed enum values:**
```rust
register_command!(cr,
    fn quality(state,
        preset: i32 = 2 (
            enum(type: int): {"low" => 1, "med" => 2, "high" => 3}
        )
    ) -> result
)?;
```

**Allow values outside declared aliases (`others: true`):**
```rust
register_command!(cr,
    fn color(state,
        c: String (
            enum(type: string, others: true): ["red", "green", "blue"]
        )
    ) -> result
)?;
```

**Global enum reference:**
```rust
register_command!(cr,
    fn resize(state,
        method: String (enum_ref: "img.resize_method")
    ) -> result
)?;
```

Notes:
- `enum` and `enum_ref` cannot be used together.
- Unknown enum aliases fail unless `others: true`.
- If `gui:` is omitted, enum arguments default to `VerticalRadioEnum` (small sets) or `EnumSelector`.

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

Do metadata customization before converting the environment with `env.to_ref()`. The conversion
refreshes every command `metadata_version` before the registry is shared, so command authors do not
need to recompute versions manually. A version read directly from the registry before `to_ref()` may
still reflect an earlier registration skeleton if later metadata customization has run.

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

- `specs/reference/REGISTER_COMMAND_FSD.md` - Complete macro syntax specification
- `CLAUDE.md` - "Common Tasks > Adding a Command" section
- `liquers-core/src/commands.rs` - CommandRegistry implementation and tests
- `liquers-lib/src/commands.rs` - Example command library

### Example Projects

- **Core commands**: `liquers-lib/src/commands.rs` (generic commands)
- **GUI commands**: `liquers-lib/src/egui/commands.rs` (with trait bounds)
- **Tests**: `liquers-core/tests/async_hellow_world.rs` (complete example)
- **Manual registration**: `liquers-core/src/commands.rs` tests section

## History

| Date | Change | Source |
|---|---|---|
| 2026-09-05 | Documented builder-time validation for hand-built and imported metadata, including preflight access to the full report. | `design/variadic-metadata-tail-check` |
| 2026-08-31 | Documented that metadata customizations should happen before `env.to_ref()`, which refreshes command metadata versions before sharing. | `design/refresh-command-metadata-versions/phase-5` |
| 2026-03-02 | Present at repository import; content unchanged since. Not reviewed against the implementation. | migration |
| 2026-08-12 | Added "Accepting a variable number of parameters": declared arity is binding, `multiple` is the only variadic mechanism and is not yet declarable, and the `~_` escape is the interim spelling. | design/excess-action-parameters-error |
| 2026-08-16 | Added "Passing the working directory (or any relative query) into a command": `-R-key/.` as a default link argument, why the working key is not readable from `Context`, building absolute queries from it, and the argument-order pitfall. | PLAN-CWD-FREEZE |
| 2026-08-25 | Rewrote "Accepting a variable number of parameters": `multiple` is now declarable. Added the flag's grammar slot, the container-type and element-type rules, the empty-list default, the last-argument rule and its `injected`/`context` exemptions, the compile-time rejection table, the `a-b` vs `a~_b` distinction, and `get_multiple` for manual registration. | design/variadic-arguments-declaration |
