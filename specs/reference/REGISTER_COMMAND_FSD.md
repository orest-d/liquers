---
title: register_command! Macro Functional Specification
kind: reference
audience: internal
area: [macro, core/commands]
reviewed: 2026-10-10
---
# register_command! Macro Functional Specification

## Overview

The `register_command!` macro in `liquers-macro` provides a domain-specific language (DSL) for registering commands with the Liquers command execution framework. It generates wrapper code that bridges user-defined functions to the command registry system.

**Key Characteristics**:
- **Function-like macro** (not an attribute macro)
- Requires the target function to be **defined separately** before macro invocation
- Uses a **custom DSL** inspired by but not compatible with Rust function syntax
- Generates type-safe wrapper functions and metadata registration

## The runtime counterpart

`register_command!` is the **compile-time** way to say that a function is a command. Its runtime
counterpart is the [Command Declaration Format](COMMAND_DECLARATION.md), which a dynamic host —
JavaScript, Python — or a plain `commands.yaml` uses to produce the same `CommandMetadata` without
a macro. The two agree by construction and by test: `int02` in
`liquers-core/tests/command_declaration.rs` registers one command both ways and compares the
result, `metadata_version` included.

Where they *deliberately* differ is the default label. This macro derives `foo bar` from `foo_bar`;
the declaration path derives `Foo bar`, and `liquers-web` keeps the name verbatim. Unifying them is
a behaviour change that would move existing commands' `metadata_version`, so it has been left
alone — see `specs/design/command-declaration/phase2-architecture.md` open question 2.

An argument with no explicit `gui:` statement uses `command_metadata::DEFAULT_GUI`. The same
constant supplies the serde default and declaration path, so every registration route uses the
same `TextField(40)` hint.

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
    [async] fn <name>([context,] <state_param>, <params...>) -> <return_type>
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
| `<params>` | No | Command parameters, and `context` at any position (§Context Parameter) |
| `<return_type>` | Yes | Either `result` or `value` |
| `<metadata_statements>` | No | Command metadata (label, doc, etc.) |

---

## State Parameter

The state keyword specifies how the input state is passed to the command function. It comes
before every argument; only `context` may precede it (§Context Parameter).

A keyword is recognised by its **form**: a bare `state`, `value` or `text` is the state keyword,
while the same word followed by `:` names an argument. `fn cmd(value: String)` is a command with no
state and one argument called `value`; `fn cmd(value)` takes the state's value. Placing the state
after an argument, or declaring it twice, is a compile-time error.

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
<name>: <Type> [injected | multiple] [= <default_value>] [(label: "...", gui: ..., enum: ..., enum_ref: ...)]
```

| Part | Required | Description |
|------|----------|-------------|
| `<name>` | Yes | Parameter name (no leading `_`, no `__`) |
| `<Type>` | Yes | Rust type |
| `injected` | No | Mark as injected from context. Consumes no query parameter |
| `multiple` | No | Mark as variadic: consumes every remaining action parameter. Requires a container type; mutually exclusive with `injected`; see [Variadic Parameters](#variadic-parameters) |
| `= <default_value>` | No | Default value. Not permitted on a `multiple` parameter |
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
| `Vec<T>` with `multiple` | whatever `T` maps to — see [Variadic Parameters](#variadic-parameters) |
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

Built-in injectable: `E::Payload` (the environment's payload type). A command that injects the
payload, or anything extracted from it, should also declare `payload: required` (§Metadata
Statements); otherwise it receives no payload when evaluated as a nested dependency
([`PAYLOAD_GUIDE.md`](PAYLOAD_GUIDE.md), "Declare it, or lose it").

### Variadic Parameters

A parameter marked `multiple` consumes **every remaining action parameter**. It is the only way a
command accepts a variable-length list; declared arity is otherwise binding.

```rust
fn select_columns(state: &State<Value>, columns: Vec<String>) -> Result<Value, Error> { ... }
register_command!(cr, fn select_columns(state, columns: Vec<String> multiple) -> result)?;
```

```
select_columns-a-b-c    ->  ParameterValue::MultipleParameters with three elements
select_columns-a~_b     ->  one element, the string "a-b"
select_columns          ->  MultipleParameters with no elements
```

**Container type.** The declared type must be a container; `Vec<T>` is what is recognised. The
macro's `variadic_element_type` is the single place another container would be added.

**Argument type comes from the element.** `Vec<String>` generates `ArgumentType::String`,
`Vec<i64>` generates `Integer`, and so on — not `Any`, which is where an unflagged `Vec<T>` falls.
This is load-bearing rather than cosmetic: `ParameterValue::from_string` parses each action
parameter through `ArgumentType`, so an `Any` element type would deliver every element as a JSON
string and a `Vec<i64>` parameter would fail at retrieval.

**No default.** A variadic parameter defaults to the empty list and may not declare another
default. `ParameterValue::from_arginfo` maps `CommandParameterValue::None` on a variadic argument to
an empty `MultipleParameters`, so an empty list reaches the command as `Ok(vec![])` rather than as
an error — a command requiring at least one element enforces that itself.

**Position.** A `multiple` parameter must be the last one that consumes a query parameter.
Parameters marked `injected` and the `context` parameter may follow it, because neither consumes
one.

**Retrieval.** Generated code emits `arguments.get_multiple(i, name)?`, not `arguments.get`.
`get_multiple` is bounded only by `FromParameterValue`; `get`'s additional
`TryFrom<E::Value>` bound serves a pre-materialised fast path that a variadic argument never takes.

**Compile-time rejections.**

| Declaration | Error |
|---|---|
| `c: String multiple` | `` a `multiple` argument must have a container type; `String` is not one. Expected `Vec<String>` `` |
| `c: Vec<String> multipel` | `` unknown argument flag `multipel`; expected `injected` or `multiple` `` |
| `c: Vec<String> injected multiple` | `` an argument cannot be both `injected` and `multiple` `` |
| `c: Vec<String> multiple multiple` | `` duplicate argument flag `multiple` `` |
| `c: Vec<String> multiple = "x"` | `` a `multiple` argument cannot have a default value `` |
| an argument after a `multiple` one | `` argument `b` follows the `multiple` argument `a` and can never receive a value `` |

Unknown flags are rejected rather than ignored, so a misspelled flag is a build error.

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
| `enum: <EnumSpec>` | Inline enum alternatives and mapping |
| `enum_ref: "..."` | Reference a global enum by name |

`hint key: "..."` is rejected with *argument hints are not supported; `hint` would be ignored*.
It used to be parsed and silently dropped. Argument hints are planned in
`design/command-metadata-descriptions-and-hints/`.

Enum metadata syntax:

```text
enum: ["a", "b", "c"]
enum: {"alias" => "value", "hq" => query "path/to/query"}
enum(type: int): {"low" => 1, "high" => 3}
enum(type: string, others: true): ["red", "green", "blue"]
enum_ref: "img.resize_method"
```

Enum type keywords supported in `enum(type: ...)`:
- `string`
- `int`
- `int_opt`
- `float`
- `float_opt`
- `bool`
- `any`

Validation rules:
- `enum` and `enum_ref` are mutually exclusive.
- aliases must be unique.
- default value must match alias unless `others: true`.
- explicit `type` must match mapped literal types.

Runtime rules:
- alias values are expanded to mapped values.
- when `others: false`, unknown alias is rejected.
- when `others: true`, fallback value is accepted only if compatible with enum type.

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

When enum metadata is present and `gui:` is omitted, defaults are:
- up to 3 alternatives: `VerticalRadioEnum`
- 4+ alternatives or `enum_ref`: `EnumSelector`

---

## Context Parameter

The special `context` keyword (or `Context`) passes the execution context to the function. It is
not a command argument: it takes no type, consumes no query parameter and occupies no argument slot,
so the argument numbering in the metadata and in `specs/command_registry.yaml` skips it.

**Position.** `context` may appear anywhere in the list — before the state, between arguments, or
last — and the generated wrapper passes it to the function in the declared position:

```rust
fn cmd(state: &State<Value>, n: i64, context: Context<E>) -> Result<Value, Error> {
    context.info("Processing...")?;
    // ...
}
register_command!(cr, fn cmd(state, n: i64, context) -> result)?;
```

**Recommended position:** last, or immediately before a `multiple` argument when there is one:

```rust
register_command!(cr, fn join(state, context, items: Vec<String> multiple) -> result)?;
```

The reason is portability: a Python signature cannot take a positional parameter after `*args`
(`def join(state, context, *items)`), so this order maps directly to the Python bindings.
`context` first (`fn cmd(context, state, n: i64)`) is allowed but not recommended.

**Compile-time errors**, each reported at the offending token:

| Signature | Error |
|---|---|
| `context` declared twice (either spelling) | `` `context` is declared twice `` |
| `context: T` | `` `context` is reserved for the execution context and takes no type `` |

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
#[liquers_macro::command_version] // needed by `version: auto` below
fn my_cmd(state: &State<Value>, arg: String) -> Result<Value, Error> { /* … */ }

register_command!(cr,
    fn my_cmd(state, arg: String) -> result
    label: "My Command"
    doc: "Does something useful"
    namespace: "utils"
    realm: "backend"
    filename: "output.txt"
    volatile: true
    payload: required
    expires: "in 5 min"
    version: auto
    preset: "my_cmd-default" (label: "Default", description: "Run with defaults")
    next: "another_cmd"
)?;
```

Most statements take a literal. Three do not: `payload:` takes a bare identifier, `expires:` a
string that is checked only when the command is registered, and `version:` one of four forms.

| Statement | Type | Description |
|-----------|------|-------------|
| `label: "..."` | String | Human-readable command name |
| `doc: "..."` | String | Documentation/description |
| `namespace: "..."` or `ns: "..."` | String | Command namespace |
| `realm: "..."` | String | Command realm |
| `filename: "..."` | String | Default output filename |
| `volatile: true/false` | Bool | Mark command as volatile |
| `payload: required` / `payload: none` | Identifier | `required` marks the command as needing the evaluation payload (`PayloadRequirement::Required`) and **also sets `volatile`**. The requirement propagates to the plan, so nested evaluation forwards the payload. `none` is the default and emits nothing. A string or bool (`payload: "required"`, `payload: true`) is a compile error. See [`PAYLOAD_GUIDE.md`](PAYLOAD_GUIDE.md). |
| `expires: "..."` | String | Default expiration of the command's results, e.g. `"in 5 min"`, `"immediately"`, `"never"`. Parsed when the command is **registered**, not at compile time: an invalid spec makes `register_command!` return `Err`. Grammar: [`DOC_08_RECIPES_PLANS.md`](api/DOC_08_RECIPES_PLANS.md) §Finalization and expiration. |
| `version: auto` / `now` / `"..."` / integer | Identifier, String or Integer | The command's implementation version (`impl_version`), which feeds dependency freshness. `auto`: a hash of the function's source; requires `#[liquers_macro::command_version]` on the function (see §Implementation versions). `now`: the registration time, so it **changes on every start** and every dependent re-evaluates after a restart. A string: its BLAKE3 hash, computed at compile time. An integer: used as is; bump it by hand. Omitted: the command is unversioned. Any other identifier is a compile error. |
| `preset: "action" (...)` | Preset | Predefined action configuration |
| `next: "action" (...)` | Preset | Suggested follow-up action |

### Implementation versions

`#[liquers_macro::command_version]` on a function definition generates a companion
`<fn>__VERSION_() -> u128`, a hash of the item's tokens, so any edit to the function changes it.
`version: auto` registers that hash as the command's `impl_version`. Without the attribute,
`version: auto` fails to compile with ``cannot find function `<fn>__VERSION_` ``. The version
feeds dependency freshness (`COMMAND_DECLARATION.md`, `impl_version`), so editing the function makes
results computed by the old code stale.

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

An async command that takes `context` must accept exactly the `Context<CommandEnvironment>` the
generated wrapper passes — normally by naming the `CommandEnvironment` alias (§Type Requirements) in
its own signature, so the alias has to be in scope where the function is defined, not only at the
`register_command!` call. See `fetch_remote` in §Complete Example.

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
            multiple: false,   // `true` when the parameter is declared `multiple`
            injected: false,
            gui_info: DEFAULT_GUI.clone(),
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

Common compile-time errors:
- Parameter name starts with `_` or contains `__`
- `context` declared twice or given a type; the state keyword declared twice or after an argument
- An argument `hint` option (not supported)
- Unknown metadata statement
- Invalid default value type
- Type mismatch between function and DSL — including parameter *order*: the wrapper calls the
  function with its parameters in the declared order

At run time, an argument error names the argument by its 1-based position in the command's argument
list and by name, e.g. `Missing argument #1 'limit'`.

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

- Implementation: `liquers-macro/src/registration.rs` (entry point `liquers-macro/src/lib.rs`)
- Usage examples: `liquers-lib/src/commands.rs`, `liquers-core/tests/async_hellow_world.rs`
- Command framework: `liquers-core/src/commands.rs`
- Command metadata: `liquers-core/src/command_metadata.rs`

---

*Last updated: 2025-01-18*

## History

| Date | Change | Source |
|---|---|---|
| 2026-10-10 | §Context Parameter rewritten: `context` at any position, the recommended position (last or before `multiple`, for Python `*args` parity), its compile-time errors; §State Parameter: keywords recognised by form, state before every argument; §Parameter Metadata: argument `hint` rejected; §Error Handling: new diagnostics, 1-based argument numbers; §References: implementation file. | phase-5, `design/context-param-order/` |
| 2026-10-07 | §Metadata Statements: rows for `payload:`, `expires:` and `version:`, the example block shows them, new §Implementation versions (`#[command_version]`); §Injected Parameters links `payload: required`. | phase-5, `design/register-command-payload-docs/` |
| 2026-09-27 | Reviewed against `design/record-streams/` Phase 5 (its Phase 4 plan added this document to the set): §Async Commands states that an async command taking `context` needs the `CommandEnvironment` alias in scope for its own signature. | phase-5 |
| 2026-09-04 | Made omitted argument `gui_info` use the shared `command_metadata::DEFAULT_GUI` (`TextField(40)`) in macro, declaration, and serde paths. | `ARGUMENT-GUI-INFO-HAS-THREE-DEFAULTS` |
| 2026-08-30 | Added §The runtime counterpart, pointing at the new `COMMAND_DECLARATION.md` and naming the one deliberate divergence (the default label rule) and the test that holds the rest in agreement. | `design/command-declaration/` |
| 2026-03-02 | Present at repository import; content unchanged since. Not reviewed against the implementation. | migration |
| 2026-08-25 | Documented the `multiple` argument flag: grammar slot shared with `injected`, the container-type requirement, element-derived `ArgumentType`, the empty-list default, the last-argument rule, `get_multiple` retrieval, and the six compile-time rejections. | design/variadic-arguments-declaration |
