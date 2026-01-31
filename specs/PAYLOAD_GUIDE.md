# Payload Usage Guide

## Quick Start

```rust
// 1. Define payload type
#[derive(Clone)]
pub struct MyPayload {
    pub user_id: String,
    pub window_id: u64,
}

// 2. Define newtype for specific field
pub struct UserId(pub String);

// 3. Implement InjectedFromContext
impl<E> InjectedFromContext<E> for UserId
where E: Environment<Payload = MyPayload>
{
    fn from_context(_name: &str, context: Context<E>) -> Result<Self, Error> {
        let payload = context.get_payload_clone()
            .ok_or_else(|| Error::general_error("No payload".to_string()))?;
        Ok(UserId(payload.user_id))
    }
}

// 4. Use in commands with 'injected' keyword
fn my_command(state: &State<Value>, user_id: UserId) -> Result<Value, Error> {
    println!("User: {}", user_id.0);
    Ok(Value::none())
}

register_command!(cr, fn my_command(state, user_id: UserId injected) -> result)?;

// 5. Provide payload when evaluating
let payload = MyPayload { user_id: "alice".to_string(), window_id: 42 };
envref.evaluate_immediately("/-/my_command", payload).await?;
```

## Overview

**Payload** is an optional, ad-hoc data structure passed through `Context` during query evaluation. It enables commands to access execution-specific information that is not part of the query itself (e.g., UI window handles, HTTP request context, user session data).

**Key mechanism**: The `InjectedFromContext<E>` trait enables automatic extraction of data from payload into command parameters using the `injected` keyword.

## Key Characteristics

**Optional** - Payload is NOT available in all execution contexts:
- ✅ Available: Immediate query evaluation (e.g., UI interactions, API requests)
- ❌ Not available: Background/async asset evaluation, store-triggered evaluations

**Environment-specific** - Each environment defines ONE payload type:
```rust
impl Environment for MyEnvironment {
    type Payload = MyPayloadType;  // Single type for entire environment
    // ...
}
```

**Type constraints** - Payload must implement:
```rust
type Payload: Clone + Send + Sync + 'static;
```

**Inheritance** - Sub-queries inherit the parent's payload:
- When a command evaluates a nested query/link, the same payload is passed through
- Enables deep command chains to access the same context

## When to Use Payload

### ✅ Good Use Cases

1. **UI Context**: Window/element handles for UI commands
   ```rust
   type Payload = UiContext;

   struct UiContext {
       current_window_id: u64,
       active_element_handle: String,
   }
   ```

2. **HTTP Request Context**: Request metadata in web servers
   ```rust
   type Payload = RequestContext;

   struct RequestContext {
       request_id: String,
       client_ip: String,
       headers: HashMap<String, String>,
   }
   ```

3. **User Session Data**: Per-query user information
   ```rust
   type Payload = UserSession;

   struct UserSession {
       user_id: String,
       permissions: HashSet<String>,
       preferences: serde_json::Value,
   }
   ```

### ❌ When NOT to Use Payload

1. **Data that should be persistent** - Use Store instead
2. **Command parameters** - Put them in the query
3. **Global configuration** - Put it in the Environment
4. **Cacheable data** - Payload is not available for cached results

## Implementation Guide

### Step 1: Define Your Payload Type

```rust
use std::sync::Arc;

#[derive(Clone)]
pub struct MyPayload {
    pub user_id: String,
    pub session_id: String,
    pub extra_data: Arc<HashMap<String, String>>,  // Arc for efficient cloning
}
```

**Important**: Keep payload lightweight and cheap to clone (use `Arc` for large data).

### Step 2: Choose Environment Type

**Option A: Use SimpleEnvironmentWithPayload** (recommended for most cases)

```rust
use liquers_core::context::SimpleEnvironmentWithPayload;
use liquers_core::value::Value;

type MyEnvironment = SimpleEnvironmentWithPayload<Value, MyPayload>;

fn main() {
    let mut env = MyEnvironment::new();
    // Register commands...
}
```

**Option B: Custom Environment Implementation**

```rust
use liquers_core::context::Environment;

pub struct CustomEnvironment {
    // ... fields
}

impl Environment for CustomEnvironment {
    type Value = Value;
    type CommandExecutor = CommandRegistry<Self>;
    type SessionType = SimpleSession;
    type Payload = MyPayload;  // Your payload type

    // ... implement required methods
}
```

### Step 3: Access Payload in Commands

Commands can access payload through the `injected` keyword, which requires the parameter type to implement `InjectedFromContext<E>` trait.

#### Understanding InjectedFromContext

The `InjectedFromContext` trait enables automatic injection of context-specific data:

```rust
pub trait InjectedFromContext<E: Environment>: Sized {
    fn from_context(name: &str, context: Context<E>) -> Result<Self, Error>;
}
```

**Key point**: The payload type (`E::Payload`) **automatically** implements `InjectedFromContext`, so the entire payload can always be injected.

#### Pattern 1: Direct Payload Injection

Inject the entire payload into a command:

```rust
use liquers_macro::register_command;

fn my_command(state: &State<Value>, payload: MyPayload) -> Result<Value, Error> {
    // Full payload available
    Ok(Value::from(format!("User: {}, Window: {}",
        payload.user_id, payload.window_id)))
}

// Register with 'injected' keyword
let cr = env.get_mut_command_registry();
register_command!(cr, fn my_command(state, payload: MyPayload injected) -> result)?;
```

**Note**: The parameter name doesn't matter - it's the type that determines what gets injected.

#### Pattern 2: Newtype for Specific Fields (Recommended)

Use the **newtype idiom** to extract specific fields from the payload. This is the recommended approach for clean, focused command signatures.

```rust
use liquers_core::commands::InjectedFromContext;
use liquers_core::context::Context;
use liquers_core::error::Error;

// Define newtypes for specific payload fields
pub struct UserId(pub String);
pub struct WindowId(pub u64);
pub struct SessionId(pub String);

// Implement InjectedFromContext for each newtype
impl<E> InjectedFromContext<E> for UserId
where E: Environment<Payload = MyPayload>
{
    fn from_context(_name: &str, context: Context<E>) -> Result<Self, Error> {
        let payload = context.get_payload_clone()
            .ok_or_else(|| Error::general_error("No payload available".to_string()))?;
        Ok(UserId(payload.user_id))
    }
}

impl<E> InjectedFromContext<E> for WindowId
where E: Environment<Payload = MyPayload>
{
    fn from_context(_name: &str, context: Context<E>) -> Result<Self, Error> {
        let payload = context.get_payload_clone()
            .ok_or_else(|| Error::general_error("No payload available".to_string()))?;
        Ok(WindowId(payload.window_id))
    }
}

impl<E> InjectedFromContext<E> for SessionId
where E: Environment<Payload = MyPayload>
{
    fn from_context(_name: &str, context: Context<E>) -> Result<Self, Error> {
        let payload = context.get_payload_clone()
            .ok_or_else(|| Error::general_error("No payload available".to_string()))?;
        Ok(SessionId(payload.session_id))
    }
}

// Now use newtypes in commands - clean and focused!
fn get_user_data(state: &State<Value>, user_id: UserId) -> Result<Value, Error> {
    let data = format!("Data for user: {}", user_id.0);
    Ok(Value::from(data))
}

fn update_window(state: &State<Value>, window_id: WindowId, user_id: UserId) -> Result<Value, Error> {
    let info = format!("User {} updated window {}", user_id.0, window_id.0);
    Ok(Value::from(info))
}

// Register with injected parameters
register_command!(cr, fn get_user_data(state, user_id: UserId injected) -> result)?;
register_command!(cr, fn update_window(state, window_id: WindowId injected, user_id: UserId injected) -> result)?;
```

**Benefits of newtypes**:
- ✅ Clear command signatures showing exactly what data is needed
- ✅ Type safety - can't accidentally swap parameters
- ✅ Reusable across multiple commands
- ✅ Self-documenting code
- ✅ Can inject multiple fields independently

#### Pattern 3: Manual Payload Access

For more complex scenarios, access payload directly via `Context`:

```rust
fn complex_command<E>(
    state: &State<E::Value>,
    context: &Context<E>
) -> Result<E::Value, Error>
where E: Environment<Payload = MyPayload>
{
    match context.get_payload_clone() {
        Some(payload) => {
            // Complex logic using multiple fields
            let result = compute_something(&payload.user_id, payload.window_id);
            Ok(E::Value::from(result))
        }
        None => {
            // Gracefully handle missing payload
            Err(Error::general_error("Payload required but not available".to_string()))
        }
    }
}

register_command!(cr, fn complex_command(state, context) -> result)?;
```

#### Pattern 4: Generic Commands with Optional Payload

Write commands that work with or without payload:

```rust
fn adaptive_command<E: Environment>(
    state: &State<E::Value>,
    context: &Context<E>,
    default_user: String
) -> Result<E::Value, Error> {
    let user = context.get_payload_clone()
        .and_then(|p| {
            // Try to extract user_id if payload has this field
            // This requires custom logic per payload type
            Some(extract_user_id(p))
        })
        .unwrap_or(default_user);

    Ok(E::Value::from(user))
}
```

#### Pattern Comparison

| Pattern | Use When | Pros | Cons |
|---------|----------|------|------|
| **Direct Payload** | Need full payload | Simple, one parameter | Clutters signature if only need one field |
| **Newtype** (recommended) | Need specific fields | Clean, focused, type-safe | Requires newtype boilerplate |
| **Manual Context** | Complex logic, optional payload | Maximum flexibility | More verbose |
| **Generic** | Library commands, payload-agnostic | Works across environments | Limited payload access |

### Step 4: Provide Payload When Evaluating

**Immediate evaluation with payload**:

```rust
use liquers_core::context::EnvRef;

async fn evaluate_with_context(envref: &EnvRef<MyEnvironment>) {
    let payload = MyPayload {
        user_id: "alice".to_string(),
        session_id: "session-123".to_string(),
        extra_data: Arc::new(HashMap::new()),
    };

    let result = envref.evaluate_immediately(
        "/data/report.csv/-/filter-active",
        payload
    ).await?;

    // Access result...
}
```

**AssetManager direct application**:

```rust
use liquers_core::assets::AssetManager;

async fn apply_query(asset_manager: &AssetManager<MyEnvironment>) {
    let payload = MyPayload { /* ... */ };

    let asset_ref = asset_manager.apply_immediately(
        Query::parse("/data/input.csv/-/process")?,
        Value::none(),
        Some(payload)  // Wrapped in Option
    ).await?;

    // Asset evaluated with payload available
}
```

## Complete Example

### Define Environment

```rust
// my_app/src/environment.rs
use liquers_core::context::SimpleEnvironmentWithPayload;
use liquers_core::value::Value;
use std::sync::Arc;
use std::collections::HashMap;

#[derive(Clone)]
pub struct AppPayload {
    pub user_id: String,
    pub window_id: u64,
    pub context_data: Arc<HashMap<String, String>>,
}

pub type AppEnvironment = SimpleEnvironmentWithPayload<Value, AppPayload>;
```

### Define Newtypes for Payload Fields

```rust
// my_app/src/payload_types.rs
use liquers_core::commands::InjectedFromContext;
use liquers_core::context::{Context, Environment};
use liquers_core::error::Error;
use crate::environment::AppPayload;

// Newtype wrappers for payload fields
pub struct UserId(pub String);
pub struct WindowId(pub u64);
pub struct ContextData(pub Arc<HashMap<String, String>>);

// Implement InjectedFromContext for each newtype
impl<E> InjectedFromContext<E> for UserId
where E: Environment<Payload = AppPayload>
{
    fn from_context(_name: &str, context: Context<E>) -> Result<Self, Error> {
        let payload = context.get_payload_clone()
            .ok_or_else(|| Error::general_error("No payload available".to_string()))?;
        Ok(UserId(payload.user_id))
    }
}

impl<E> InjectedFromContext<E> for WindowId
where E: Environment<Payload = AppPayload>
{
    fn from_context(_name: &str, context: Context<E>) -> Result<Self, Error> {
        let payload = context.get_payload_clone()
            .ok_or_else(|| Error::general_error("No payload available".to_string()))?;
        Ok(WindowId(payload.window_id))
    }
}

impl<E> InjectedFromContext<E> for ContextData
where E: Environment<Payload = AppPayload>
{
    fn from_context(_name: &str, context: Context<E>) -> Result<Self, Error> {
        let payload = context.get_payload_clone()
            .ok_or_else(|| Error::general_error("No payload available".to_string()))?;
        Ok(ContextData(payload.context_data.clone()))
    }
}
```

### Register Commands

```rust
// my_app/src/commands.rs
use liquers_macro::register_command;
use liquers_core::{state::State, error::Error, context::Context, value::Value};
use crate::environment::AppEnvironment;
use crate::payload_types::{UserId, WindowId, ContextData};

// Command with single injected field (using newtype)
fn get_user_data(state: &State<Value>, user_id: UserId) -> Result<Value, Error> {
    let data = format!("Data for user: {}", user_id.0);
    Ok(Value::from(data))
}

// Command with multiple injected fields
fn get_window_info(
    state: &State<Value>,
    window_id: WindowId,
    user_id: UserId
) -> Result<Value, Error> {
    let info = format!("Window: {}, User: {}", window_id.0, user_id.0);
    Ok(Value::from(info))
}

// Command that injects the entire payload
fn process_with_full_payload(
    state: &State<Value>,
    payload: AppPayload
) -> Result<Value, Error> {
    // Access all payload fields
    let data = payload.context_data.get("key").map(|s| s.as_str()).unwrap_or("default");
    let result = format!("User: {}, Window: {}, Data: {}",
        payload.user_id, payload.window_id, data);
    Ok(Value::from(result))
}

// Command with manual context access (for complex cases)
fn complex_operation(
    state: &State<Value>,
    context: &Context<AppEnvironment>
) -> Result<Value, Error> {
    let payload = context.get_payload_clone()
        .ok_or_else(|| Error::general_error("No payload available".to_string()))?;

    // Complex logic using payload
    if payload.window_id > 100 {
        Ok(Value::from("High window ID"))
    } else {
        Ok(Value::from("Low window ID"))
    }
}

pub fn register_app_commands(env: &mut AppEnvironment) -> Result<(), Error> {
    let cr = env.get_mut_command_registry();

    // Register commands with newtype injection
    register_command!(cr, fn get_user_data(state, user_id: UserId injected) -> result)?;

    register_command!(cr, fn get_window_info(
        state,
        window_id: WindowId injected,
        user_id: UserId injected
    ) -> result)?;

    // Register command with full payload injection
    register_command!(cr, fn process_with_full_payload(
        state,
        payload: AppPayload injected
    ) -> result)?;

    // Register command with manual context access
    register_command!(cr, fn complex_operation(state, context) -> result)?;

    Ok(())
}
```

### Use in Application

```rust
// my_app/src/main.rs
use liquers_core::context::EnvRef;
use crate::environment::{AppEnvironment, AppPayload};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Setup environment
    let mut env = AppEnvironment::new();
    register_app_commands(&mut env)?;

    let envref = env.to_ref();

    // Create payload for this evaluation
    let payload = AppPayload {
        user_id: "alice".to_string(),
        window_id: 42,
        context_data: Arc::new(HashMap::new()),
    };

    // Evaluate query with payload
    let result = envref.evaluate_immediately(
        "/-/get_user_data",
        payload.clone()
    ).await?;

    println!("Result: {:?}", result);

    Ok(())
}
```

## Implementing InjectedFromContext

### Basic Implementation Template

```rust
use liquers_core::commands::InjectedFromContext;
use liquers_core::context::{Context, Environment};
use liquers_core::error::Error;

// 1. Define newtype
pub struct MyField(pub FieldType);

// 2. Implement InjectedFromContext
impl<E> InjectedFromContext<E> for MyField
where E: Environment<Payload = YourPayload>
{
    fn from_context(_name: &str, context: Context<E>) -> Result<Self, Error> {
        let payload = context.get_payload_clone()
            .ok_or_else(|| Error::general_error("No payload available".to_string()))?;

        // Extract field from payload
        Ok(MyField(payload.field_name.clone()))
    }
}
```

### Best Practices for InjectedFromContext

#### 1. Use Descriptive Newtype Names

```rust
// GOOD - Clear what it represents
pub struct UserId(pub String);
pub struct WindowHandle(pub u64);
pub struct RequestId(pub uuid::Uuid);

// BAD - Generic, unclear
pub struct Id(pub String);
pub struct Data(pub String);
```

#### 2. Handle Missing Payload Gracefully

```rust
impl<E> InjectedFromContext<E> for UserId
where E: Environment<Payload = AppPayload>
{
    fn from_context(_name: &str, context: Context<E>) -> Result<Self, Error> {
        let payload = context.get_payload_clone()
            .ok_or_else(|| Error::general_error(
                "UserId injection requires payload".to_string()
            ))?;
        Ok(UserId(payload.user_id))
    }
}
```

#### 3. Support Multiple Environment Types (Optional)

If your newtype should work with different payload structures:

```rust
// Define a trait that your payload types implement
pub trait HasUserId {
    fn user_id(&self) -> &str;
}

impl HasUserId for AppPayload {
    fn user_id(&self) -> &str {
        &self.user_id
    }
}

// Implement InjectedFromContext generically
impl<E> InjectedFromContext<E> for UserId
where
    E: Environment,
    E::Payload: HasUserId,
{
    fn from_context(_name: &str, context: Context<E>) -> Result<Self, Error> {
        let payload = context.get_payload_clone()
            .ok_or_else(|| Error::general_error("No payload available".to_string()))?;
        Ok(UserId(payload.user_id().to_string()))
    }
}
```

#### 4. Add Validation in from_context

```rust
pub struct ValidatedUserId(pub String);

impl<E> InjectedFromContext<E> for ValidatedUserId
where E: Environment<Payload = AppPayload>
{
    fn from_context(_name: &str, context: Context<E>) -> Result<Self, Error> {
        let payload = context.get_payload_clone()
            .ok_or_else(|| Error::general_error("No payload available".to_string()))?;

        // Validate user_id
        if payload.user_id.is_empty() {
            return Err(Error::general_error("User ID cannot be empty".to_string()));
        }

        if !payload.user_id.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return Err(Error::general_error("Invalid user ID format".to_string()));
        }

        Ok(ValidatedUserId(payload.user_id))
    }
}
```

#### 5. Derive Common Traits

```rust
// Make newtypes ergonomic
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct UserId(pub String);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WindowId(pub u64);

// Add Display for better error messages
use std::fmt;

impl fmt::Display for UserId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
```

### Common Newtype Patterns

#### Pattern 1: Simple Field Extraction

```rust
pub struct SessionId(pub String);

impl<E> InjectedFromContext<E> for SessionId
where E: Environment<Payload = AppPayload>
{
    fn from_context(_name: &str, context: Context<E>) -> Result<Self, Error> {
        let payload = context.get_payload_clone()
            .ok_or_else(|| Error::general_error("No payload".to_string()))?;
        Ok(SessionId(payload.session_id))
    }
}
```

#### Pattern 2: Computed Value from Payload

```rust
pub struct IsAdminUser(pub bool);

impl<E> InjectedFromContext<E> for IsAdminUser
where E: Environment<Payload = AppPayload>
{
    fn from_context(_name: &str, context: Context<E>) -> Result<Self, Error> {
        let payload = context.get_payload_clone()
            .ok_or_else(|| Error::general_error("No payload".to_string()))?;

        // Compute value from payload
        let is_admin = payload.user_id == "admin" ||
                       payload.context_data.contains_key("admin_override");

        Ok(IsAdminUser(is_admin))
    }
}
```

#### Pattern 3: Nested Field Access

```rust
#[derive(Clone)]
pub struct AppPayload {
    pub user: UserInfo,
    // ... other fields
}

#[derive(Clone)]
pub struct UserInfo {
    pub id: String,
    pub email: String,
    pub role: String,
}

pub struct UserEmail(pub String);

impl<E> InjectedFromContext<E> for UserEmail
where E: Environment<Payload = AppPayload>
{
    fn from_context(_name: &str, context: Context<E>) -> Result<Self, Error> {
        let payload = context.get_payload_clone()
            .ok_or_else(|| Error::general_error("No payload".to_string()))?;

        // Access nested field
        Ok(UserEmail(payload.user.email.clone()))
    }
}
```

#### Pattern 4: Optional Field with Default

```rust
pub struct Theme(pub String);

impl<E> InjectedFromContext<E> for Theme
where E: Environment<Payload = AppPayload>
{
    fn from_context(_name: &str, context: Context<E>) -> Result<Self, Error> {
        let payload = context.get_payload_clone()
            .ok_or_else(|| Error::general_error("No payload".to_string()))?;

        // Provide default if field is missing
        let theme = payload.context_data
            .get("theme")
            .map(|s| s.clone())
            .unwrap_or_else(|| "default".to_string());

        Ok(Theme(theme))
    }
}
```

## Advanced Patterns

### Mutable Payload with Interior Mutability

Payload is cloned when passed to sub-queries, but you can use interior mutability for shared state:

```rust
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct MutablePayload {
    pub user_id: String,
    pub accumulated_logs: Arc<Mutex<Vec<String>>>,  // Shared mutable state
}

fn logging_command(
    state: &State<Value>,
    context: &Context<MyEnvironment>
) -> Result<Value, Error> {
    if let Some(payload) = context.get_payload_clone() {
        let mut logs = payload.accumulated_logs.lock().unwrap();
        logs.push("Command executed".to_string());
    }
    Ok(Value::none())
}
```

**Warning**: Use with caution - can make debugging difficult.

### Payload-Aware Generic Commands

Write commands that work with or without payload:

```rust
fn adaptive_command<E: Environment>(
    state: &State<E::Value>,
    context: &Context<E>,
    default_value: String
) -> Result<E::Value, Error> {
    // Try to get specific data from payload
    let value = context.get_payload_clone()
        .and_then(|p| extract_value_from_payload(p))  // Custom extraction
        .unwrap_or(default_value);

    // Use value...
    Ok(E::Value::from(value))
}
```

### Conditional Command Registration

Register different commands based on payload availability:

```rust
pub fn register_commands<E: Environment>(env: &mut E) -> Result<(), Error> {
    // Always register core commands
    register_core_commands(env)?;

    // Check if environment supports payload
    if std::any::TypeId::of::<E::Payload>() != std::any::TypeId::of::<()>() {
        // Register payload-dependent commands
        register_payload_commands(env)?;
    }

    Ok(())
}
```

## Troubleshooting

### Problem: "Payload required but not available"

**Cause**: Command expects payload but query evaluated without it.

**Solutions**:
1. Use `evaluate_immediately()` instead of `evaluate()`
2. Provide payload when calling `apply_immediately()`
3. Make command handle missing payload gracefully

**Example fix**:
```rust
// BAD - evaluate() doesn't support payload
let result = envref.evaluate("/data/-/my_command").await?;

// GOOD - evaluate_immediately() accepts payload
let payload = MyPayload { /* ... */ };
let result = envref.evaluate_immediately("/data/-/my_command", payload).await?;
```

### Problem: "No payload in context for injected parameter"

**Cause**: Used `injected` keyword but payload is `None` at runtime.

**Solution**: This error comes from the default `InjectedFromContext` implementation. Either:
1. Always provide payload when evaluating
2. Implement custom `from_context` that provides a fallback
3. Don't use `injected` - access via `Context` instead

**Example with fallback**:
```rust
pub struct UserIdWithDefault(pub String);

impl<E> InjectedFromContext<E> for UserIdWithDefault
where E: Environment<Payload = AppPayload>
{
    fn from_context(_name: &str, context: Context<E>) -> Result<Self, Error> {
        match context.get_payload_clone() {
            Some(payload) => Ok(UserIdWithDefault(payload.user_id)),
            None => Ok(UserIdWithDefault("anonymous".to_string())), // Fallback
        }
    }
}
```

### Problem: Type doesn't implement InjectedFromContext

**Cause**: Used `injected` with a type that doesn't implement the trait.

**Error message**:
```
error[E0277]: the trait bound `MyType: InjectedFromContext<_>` is not satisfied
```

**Solution**: Implement `InjectedFromContext` for your type:

```rust
impl<E> InjectedFromContext<E> for MyType
where E: Environment<Payload = MyPayload>
{
    fn from_context(_name: &str, context: Context<E>) -> Result<Self, Error> {
        let payload = context.get_payload_clone()
            .ok_or_else(|| Error::general_error("No payload".to_string()))?;
        // Extract/construct MyType from payload
        Ok(MyType { /* ... */ })
    }
}
```

### Problem: Type mismatch with payload

**Cause**: Command expects different payload type than environment provides.

**Solution**: Ensure command is generic over Environment or uses correct concrete type:

```rust
// BAD: Hardcoded wrong payload type
fn my_command(context: &Context<SomeOtherEnvironment>) -> Result<Value, Error>

// GOOD: Generic
fn my_command<E: Environment>(context: &Context<E>) -> Result<E::Value, Error>

// GOOD: Correct concrete type
fn my_command(context: &Context<AppEnvironment>) -> Result<Value, Error>
```

### Problem: InjectedFromContext not working with generic Environment

**Cause**: Type constraint doesn't match the environment's payload type.

**Solution**: Use trait bounds to constrain the payload type:

```rust
// Option 1: Require specific payload type
impl<E> InjectedFromContext<E> for UserId
where E: Environment<Payload = AppPayload>
{
    // ...
}

// Option 2: Use trait bound on payload
pub trait HasUserId {
    fn user_id(&self) -> &str;
}

impl<E> InjectedFromContext<E> for UserId
where
    E: Environment,
    E::Payload: HasUserId,
{
    fn from_context(_name: &str, context: Context<E>) -> Result<Self, Error> {
        let payload = context.get_payload_clone()
            .ok_or_else(|| Error::general_error("No payload".to_string()))?;
        Ok(UserId(payload.user_id().to_string()))
    }
}
```

### Problem: Payload not passed to nested queries

**Cause**: Using `context.evaluate()` instead of inheriting automatically.

**Solution**: Payload inheritance is automatic when using the standard evaluation flow. If you need to evaluate a sub-query manually, clone the context:

```rust
async fn parent_command<E: Environment>(
    state: &State<E::Value>,
    context: &Context<E>
) -> Result<E::Value, Error> {
    // Nested evaluation inherits payload automatically
    let child_result = context.evaluate(&parse_query("/-/child_command")?).await?;

    // Access child result
    Ok(E::Value::none())
}
```

### Problem: Newtype field is private/inaccessible

**Cause**: Newtype field not marked `pub`.

**Solution**: Always make newtype fields public:

```rust
// BAD
pub struct UserId(String);  // Field is private!

// GOOD
pub struct UserId(pub String);  // Field is public
```

## Best Practices

### Payload Design

1. **Keep payload small** - Use `Arc` for large data structures
   ```rust
   // GOOD - Arc for shared large data
   pub struct AppPayload {
       pub user_id: String,  // Small, cloned cheaply
       pub large_data: Arc<HashMap<String, Vec<u8>>>,  // Large, Arc-wrapped
   }
   ```

2. **Handle missing payload gracefully** - Not all execution paths provide it
   ```rust
   // Provide defaults in InjectedFromContext
   fn from_context(_name: &str, context: Context<E>) -> Result<Self, Error> {
       match context.get_payload_clone() {
           Some(p) => Ok(Self(p.user_id)),
           None => Ok(Self("anonymous".to_string())),  // Fallback
       }
   }
   ```

3. **Don't rely on payload for persistence** - It's request-scoped only
   - ❌ Don't: Store important state only in payload
   - ✅ Do: Use payload for request context, Store for persistence

### Command Design

4. **Prefer newtype pattern** - Use newtypes instead of raw payload access
   ```rust
   // BETTER - Clear, type-safe
   fn my_command(state: &State<Value>, user_id: UserId) -> Result<Value, Error>

   // WORSE - Requires full payload knowledge
   fn my_command(state: &State<Value>, payload: AppPayload) -> Result<Value, Error>
   ```

5. **Use descriptive newtype names** - Make intent clear
   ```rust
   // GOOD
   pub struct UserId(pub String);
   pub struct WindowHandle(pub u64);
   pub struct RequestId(pub uuid::Uuid);

   // BAD
   pub struct Data(pub String);
   pub struct Id(pub u64);
   ```

6. **Document payload requirements** - Make it clear when commands need payload
   ```rust
   /// Get user-specific data
   ///
   /// Requires: UserId from payload
   fn get_user_data(state: &State<Value>, user_id: UserId) -> Result<Value, Error>
   ```

### Implementation

7. **Implement common traits for newtypes** - Make them ergonomic
   ```rust
   #[derive(Debug, Clone, PartialEq, Eq, Hash)]
   pub struct UserId(pub String);

   impl std::fmt::Display for UserId {
       fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
           write!(f, "{}", self.0)
       }
   }
   ```

8. **Add validation in InjectedFromContext** - Fail early with clear errors
   ```rust
   fn from_context(_name: &str, context: Context<E>) -> Result<Self, Error> {
       let payload = context.get_payload_clone()
           .ok_or_else(|| Error::general_error("No payload".to_string()))?;

       if payload.user_id.is_empty() {
           return Err(Error::general_error("User ID cannot be empty".to_string()));
       }

       Ok(UserId(payload.user_id))
   }
   ```

9. **Group related newtypes** - Keep them organized
   ```rust
   // my_app/src/payload_types.rs
   //
   // All payload-related newtypes in one module
   pub mod payload_types {
       pub struct UserId(pub String);
       pub struct WindowId(pub u64);
       pub struct SessionId(pub String);
       // ... implementations
   }
   ```

### Threading and Safety

10. **Avoid mutable shared state** - Don't mutate payload internals unless necessary
    ```rust
    // RISKY - shared mutable state
    pub struct AppPayload {
        pub logs: Arc<Mutex<Vec<String>>>,
    }

    // BETTER - immutable payload, use Context for logging
    pub struct AppPayload {
        pub user_id: String,
    }
    // Use context.info(), context.debug() for logging instead
    ```

11. **Prefer immutable data** - Makes reasoning about command behavior easier
    - Clone data when needed rather than sharing mutable references
    - Use message passing (via Context) for side effects

## Summary

| Aspect | Details |
|--------|---------|
| **Purpose** | Pass execution-specific context to commands |
| **Scope** | Single query evaluation (not persisted) |
| **Availability** | Only for immediate evaluation, not background/async |
| **Type** | One type per environment, must be `Clone + Send + Sync + 'static` |
| **Access** | Via `Context::get_payload_clone()` or `injected` parameters |
| **Injection** | Requires type to implement `InjectedFromContext<E>` trait |
| **Newtype Pattern** | **Recommended** - Use newtypes for specific payload fields |
| **Inheritance** | Automatically passed to nested queries |
| **Common uses** | UI handles, HTTP request context, user sessions |

### Quick Reference: Access Patterns

| Pattern | Syntax | When to Use |
|---------|--------|-------------|
| **Direct Payload** | `fn cmd(payload: MyPayload injected)` | Need full payload access |
| **Newtype** (recommended) | `fn cmd(user_id: UserId injected)` | Need specific field(s) only |
| **Manual Context** | `fn cmd(context: &Context<E>)` | Complex logic, optional payload |
| **Multiple Fields** | `fn cmd(user: UserId injected, win: WindowId injected)` | Need several specific fields |

### InjectedFromContext Implementation Checklist

When creating a newtype for injection:

- [ ] Define newtype struct with public field: `pub struct MyType(pub InnerType);`
- [ ] Implement `InjectedFromContext<E>` for the newtype
- [ ] Constrain `E::Payload` to match your payload type
- [ ] Handle missing payload (return error or provide default)
- [ ] Add validation if needed
- [ ] Derive common traits: `Debug, Clone, PartialEq, Display`
- [ ] Document what the newtype represents
- [ ] Test with and without payload present

---

*See also:*
- `liquers-core/src/context.rs` - Context and Environment definitions
- `liquers-core/src/assets.rs` - Asset evaluation with payload
- `specs/COMMAND_REGISTRATION_GUIDE.md` - Command registration patterns
- `specs/PROJECT_OVERVIEW.md` - Architecture overview
