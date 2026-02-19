# Test Patterns Reference

Complete test templates for the Liquers project organized by scenario.

## Table of Contents

1. [Sync Command Test](#1-sync-command-test)
2. [Async Command Test](#2-async-command-test)
3. [Command with Context](#3-command-with-context)
4. [Command with Default Parameters](#4-command-with-default-parameters)
5. [Payload Injection Test](#5-payload-injection-test)
6. [Store CRUD Test](#6-store-crud-test)
7. [Async Store Test](#7-async-store-test)
8. [Plan Builder Test](#8-plan-builder-test)
9. [Query/Key Parsing Test](#9-querykey-parsing-test)
10. [State Management Test](#10-state-management-test)
11. [Error Handling Test](#11-error-handling-test)
12. [Metadata Verification Test](#12-metadata-verification-test)
13. [End-to-End Evaluation Test](#13-end-to-end-evaluation-test)
14. [Generator Command Test (No State)](#14-generator-command-test)
15. [Value Type Conversion Test](#15-value-type-conversion-test)
16. [Polars DataFrame Test](#16-polars-dataframe-test)

---

## 1. Sync Command Test

```rust
#[tokio::test]
async fn test_sync_command() -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = SimpleEnvironment<Value>;
    let mut env = SimpleEnvironment::<Value>::new();

    fn greet(state: &State<Value>, greeting: String) -> Result<Value, Error> {
        let input = state.try_into_string()?;
        Ok(Value::from(format!("{}, {}!", greeting, input)))
    }

    let cr = &mut env.command_registry;
    register_command!(cr, fn greet(state, greeting: String) -> result)?;

    // Manual execution (unit-level)
    let state = State::new().with_string("world");
    let parameters = ResolvedParameterValues::new();
    let mut args = CommandArguments::new(parameters);
    args.set_value(0, Arc::new(Value::from("Hello")));
    let envref = env.to_ref();
    let assetref = envref.get_asset_manager().create_dummy_asset();
    let context = assetref.create_context().await;

    let key = CommandKey::new_name("greet");
    let result = cr.execute(&key, &state, args, context)?;
    assert_eq!(result.try_into_string()?, "Hello, world!");
    Ok(())
}
```

## 2. Async Command Test

```rust
#[tokio::test]
async fn test_async_command() -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = SimpleEnvironment<Value>;
    let mut env = SimpleEnvironment::<Value>::new();

    // NOTE: async commands take State by value (owned), not by reference
    async fn async_greet(state: State<Value>, greeting: String) -> Result<Value, Error> {
        let input = state.try_into_string()?;
        Ok(Value::from(format!("{}, {}!", greeting, input)))
    }

    let cr = &mut env.command_registry;
    register_command!(cr, async fn async_greet(state, greeting: String = "Hello") -> result)?;

    // Verify async metadata
    let key = CommandKey::new_name("async_greet");
    let metadata = cr.command_metadata_registry.get(key).unwrap();
    assert!(metadata.is_async, "should be marked async");

    Ok(())
}
```

## 3. Command with Context

```rust
#[tokio::test]
async fn test_command_with_context() -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = SimpleEnvironment<Value>;
    let mut env = SimpleEnvironment::<Value>::new();

    fn cmd_with_ctx<E: Environment>(
        state: &State<E::Value>,
        _context: Context<E>,
    ) -> Result<E::Value, Error> {
        state.try_into_string().map(|s| E::Value::new(&s))
    }

    let cr = &mut env.command_registry;
    register_command!(cr, fn cmd_with_ctx(state, context) -> result)?;

    Ok(())
}
```

## 4. Command with Default Parameters

```rust
#[tokio::test]
async fn test_default_params() -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = SimpleEnvironment<Value>;
    let mut env = SimpleEnvironment::<Value>::new();

    fn with_defaults(
        _state: &State<Value>,
        name: String,
        count: i32,
        flag: bool,
    ) -> Result<Value, Error> {
        Ok(Value::from(format!("{}-{}-{}", name, count, flag)))
    }

    let cr = &mut env.command_registry;
    register_command!(cr,
        fn with_defaults(state, name: String = "default", count: i32 = 5, flag: bool = true) -> result
    )?;

    // Verify default values in metadata
    let key = CommandKey::new_name("with_defaults");
    let metadata = cr.command_metadata_registry.get(key).unwrap();
    assert_eq!(metadata.arguments.len(), 3);

    Ok(())
}
```

## 5. Payload Injection Test

```rust
// Define payload and injection types
#[derive(Clone, Debug)]
pub struct TestPayload {
    pub user: String,
    pub window_id: u32,
}
impl PayloadType for TestPayload {}

#[derive(Debug, Clone)]
pub struct UserId(pub String);

impl ExtractFromPayload<TestPayload> for UserId {
    fn extract_from_payload(payload: &TestPayload) -> Result<Self, Error> {
        Ok(UserId(payload.user.clone()))
    }
}

// Implement InjectedFromContext for the environment
type TestEnvironment = SimpleEnvironmentWithPayload<Value, TestPayload>;

impl InjectedFromContext<TestEnvironment> for TestPayload {
    fn from_context(_name: &str, context: Context<TestEnvironment>) -> Result<Self, Error> {
        context.get_payload_clone()
            .ok_or(Error::general_error("No payload".to_string()))
    }
}

impl InjectedFromContext<TestEnvironment> for UserId {
    fn from_context(name: &str, context: Context<TestEnvironment>) -> Result<Self, Error> {
        let payload: TestPayload = InjectedFromContext::from_context(name, context)?;
        UserId::extract_from_payload(&payload)
    }
}

#[tokio::test]
async fn test_injection() -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = TestEnvironment;
    let mut env = TestEnvironment::new();

    fn get_user(_state: &State<Value>, user_id: UserId) -> Result<Value, Error> {
        Ok(Value::from(format!("user:{}", user_id.0)))
    }

    let cr = &mut env.command_registry;
    register_command!(cr, fn get_user(state, user_id: UserId injected) -> result)?;

    let envref = env.to_ref();
    let payload = TestPayload { user: "alice".into(), window_id: 42 };
    let asset = envref.evaluate_immediately("/-/get_user", payload).await?;
    let state = asset.get().await?;
    assert_eq!(state.try_into_string()?, "user:alice");
    Ok(())
}
```

## 6. Store CRUD Test

```rust
#[test]
fn test_memory_store_crud() -> Result<(), Box<dyn std::error::Error>> {
    let store = MemoryStore::new(&Key::new());
    let key = parse_key("data/test")?;
    let data = b"test content".to_vec();
    let metadata = Metadata::from(MetadataRecord::new());

    // Set
    store.set(&key, &data, &metadata)?;
    assert!(store.contains(&key)?);

    // Get
    let (retrieved_data, _retrieved_meta) = store.get(&key)?;
    assert_eq!(data, retrieved_data);

    // Get bytes only
    let bytes = store.get_bytes(&key)?;
    assert_eq!(data, bytes);

    // Remove
    store.remove(&key)?;
    assert!(!store.contains(&key)?);

    // Get after remove should fail
    assert!(store.get(&key).is_err());

    Ok(())
}

#[test]
fn test_memory_store_directory_ops() -> Result<(), Box<dyn std::error::Error>> {
    let store = MemoryStore::new(&Key::new());
    let key1 = parse_key("dir/file1")?;
    let key2 = parse_key("dir/file2")?;
    let dir_key = parse_key("dir")?;
    let metadata = Metadata::from(MetadataRecord::new());

    store.set(&key1, b"data1", &metadata)?;
    store.set(&key2, b"data2", &metadata)?;

    let listing = store.listdir(&dir_key)?;
    assert_eq!(listing.len(), 2);

    Ok(())
}
```

## 7. Async Store Test

```rust
#[tokio::test]
async fn test_async_store_wrapper() -> Result<(), Box<dyn std::error::Error>> {
    use liquers_core::store::AsyncStoreWrapper;

    let store = AsyncStoreWrapper(MemoryStore::new(&Key::new()));
    let key = parse_key("async/test")?;
    let data = b"async data".to_vec();
    let metadata = Metadata::from(MetadataRecord::new());

    store.set(&key, &data, &metadata).await?;
    assert!(store.contains(&key).await?);

    let (retrieved, _) = store.get(&key).await?;
    assert_eq!(data, retrieved);

    Ok(())
}
```

## 8. Plan Builder Test

```rust
#[test]
fn test_plan_building() -> Result<(), Box<dyn std::error::Error>> {
    let mut cr = CommandMetadataRegistry::new();
    cr.add_command(
        CommandMetadata::new("filter")
            .with_argument(ArgumentInfo::any_argument("column"))
    );

    let query = parse_query("filter-name")?;
    let plan = PlanBuilder::new(query, &cr).build()?;

    assert_eq!(plan.len(), 1);
    match &plan[0] {
        Step::Action { action_name, parameters, .. } => {
            assert_eq!(action_name, "filter");
            assert_eq!(parameters.0.len(), 1);
        }
        Step::GetResource(_) => panic!("Expected Action, got GetResource"),
        Step::Evaluate(_) => panic!("Expected Action, got Evaluate"),
        Step::Filename(_) => panic!("Expected Action, got Filename"),
        Step::UseQueryValue(_) => panic!("Expected Action, got UseQueryValue"),
    }

    Ok(())
}

#[test]
fn test_plan_with_placeholders() -> Result<(), Box<dyn std::error::Error>> {
    let mut cr = CommandMetadataRegistry::new();
    cr.add_command(
        CommandMetadata::new("cmd")
            .with_argument(ArgumentInfo::any_argument("arg"))
    );

    // Without placeholders — should fail (missing argument)
    assert!(PlanBuilder::new(parse_query("cmd")?, &cr).build().is_err());

    // With placeholders — should succeed
    let plan = PlanBuilder::new(parse_query("cmd")?, &cr)
        .with_placeholders_allowed()
        .build()?;
    assert_eq!(plan.len(), 1);

    Ok(())
}
```

## 9. Query/Key Parsing Test

```rust
#[test]
fn test_key_parsing_and_encoding() -> Result<(), Box<dyn std::error::Error>> {
    let key = parse_key("a/b/c")?;
    assert_eq!(key.encode(), "a/b/c");

    // Prefix checking
    let prefix = parse_key("a/b")?;
    assert!(key.has_key_prefix(&prefix));
    assert!(!prefix.has_key_prefix(&key));

    // Joining
    let base = parse_key("root")?;
    let child = base.join(&parse_key("child")?);
    assert_eq!(child.encode(), "root/child");

    Ok(())
}

#[test]
fn test_query_parsing() -> Result<(), Box<dyn std::error::Error>> {
    let query = parse_query("data/filter-x/sort")?;
    let encoded = query.encode();
    assert!(encoded.contains("filter"));

    // Round-trip
    let reparsed = parse_query(&encoded)?;
    assert_eq!(query.encode(), reparsed.encode());

    Ok(())
}
```

## 10. State Management Test

```rust
#[test]
fn test_state_creation_and_access() -> Result<(), Box<dyn std::error::Error>> {
    // Default state
    let state = State::<Value>::new();
    assert!(state.is_none());

    // String state
    let state = state.with_string("hello");
    assert_eq!(state.try_into_string()?, "hello");
    assert!(!state.is_none());

    // Error state
    let err_state = State::<Value>::from_error(Error::general_error("test error".to_string()));
    assert!(err_state.is_error()?);

    Ok(())
}

#[test]
fn test_state_metadata() -> Result<(), Box<dyn std::error::Error>> {
    let mut metadata = MetadataRecord::new();
    metadata.data_format = Some("json".to_string());
    metadata.with_type_identifier("text".to_string());

    let state = State::from_value_and_metadata(
        Value::from("data"),
        Arc::new(metadata.into()),
    );

    assert_eq!(state.get_data_format(), Some("json".to_string()));
    assert_eq!(state.type_identifier(), "text");

    Ok(())
}
```

## 11. Error Handling Test

```rust
#[test]
fn test_error_types() {
    // General error
    let err = Error::general_error("something failed".to_string());
    assert_eq!(err.error_type, ErrorType::General);
    assert!(err.to_string().contains("something failed"));

    // Key not found
    let key = parse_key("missing/key").unwrap();
    let err = Error::key_not_found(&key);
    assert_eq!(err.error_type, ErrorType::KeyNotFound);

    // Conversion error
    let err = Error::conversion_error("image", "text");
    assert_eq!(err.error_type, ErrorType::ConversionError);
}

#[test]
fn test_error_context() {
    let key = CommandKey::new("", "ns", "cmd");
    let err = Error::general_error("inner".to_string())
        .with_command_key(&key);

    let display = err.to_string();
    assert!(display.contains("ns-cmd"));
    assert!(display.contains("inner"));
}
```

## 12. Metadata Verification Test

```rust
#[tokio::test]
async fn test_command_metadata_registration() -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = SimpleEnvironment<Value>;
    let mut env = SimpleEnvironment::<Value>::new();

    fn labeled_cmd(_state: &State<Value>) -> Result<Value, Error> {
        Ok(Value::none())
    }

    let cr = &mut env.command_registry;
    register_command!(cr,
        fn labeled_cmd(state) -> result
        label: "My Command"
        doc: "Does something useful"
        namespace: "test_ns"
        filename: "output.txt"
    )?;

    let key = CommandKey::new("", "test_ns", "labeled_cmd");
    let metadata = cr.command_metadata_registry.get(key).unwrap();
    assert_eq!(metadata.label, "My Command");
    assert_eq!(metadata.doc, "Does something useful");
    assert!(!metadata.is_async);

    Ok(())
}
```

## 13. End-to-End Evaluation Test

```rust
use liquers_core::interpreter::evaluate;

#[tokio::test]
async fn test_end_to_end_pipeline() -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = SimpleEnvironment<Value>;
    let mut env = SimpleEnvironment::<Value>::new();

    fn world(_state: &State<Value>) -> Result<Value, Error> {
        Ok(Value::from("world"))
    }
    async fn greet(state: State<Value>, greet: String) -> Result<Value, Error> {
        let what = state.try_into_string()?;
        Ok(Value::from(format!("{greet}, {what}!")))
    }

    let cr = &mut env.command_registry;
    register_command!(cr, fn world(state) -> result)?;
    register_command!(cr, async fn greet(state, greet: String = "Hello") -> result)?;

    let envref = env.to_ref();

    // Pipeline: world → greet
    let state = evaluate(envref.clone(), "world/greet", None).await?;
    assert_eq!(state.try_into_string()?, "Hello, world!");

    // Pipeline with explicit parameter
    let state = evaluate(envref, "world/greet-Hi", None).await?;
    assert_eq!(state.try_into_string()?, "Hi, world!");

    Ok(())
}
```

## 14. Generator Command Test

```rust
#[tokio::test]
async fn test_generator_command() -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = SimpleEnvironment<Value>;
    let mut env = SimpleEnvironment::<Value>::new();

    // Generator: no state parameter
    fn generate_data(name: String) -> Result<Value, Error> {
        Ok(Value::from(format!("generated:{}", name)))
    }

    let cr = &mut env.command_registry;
    register_command!(cr, fn generate_data(name: String) -> result)?;

    let envref = env.to_ref();
    let state = evaluate(envref, "/-/generate_data-test", None).await?;
    assert_eq!(state.try_into_string()?, "generated:test");

    Ok(())
}
```

## 15. Value Type Conversion Test

```rust
#[test]
fn test_value_conversions() -> Result<(), Box<dyn std::error::Error>> {
    // String
    let v = Value::from("hello");
    assert_eq!(v.try_into_string()?, "hello");

    // Integer
    let v = Value::from(42);
    assert_eq!(v.try_into_string()?, "42");

    // None
    let v = Value::none();
    assert!(v.is_none());

    // From string reference
    let v = Value::new("test");
    assert_eq!(v.try_into_string()?, "test");

    Ok(())
}
```

## 16. Polars DataFrame Test

```rust
use liquers_lib::{
    environment::DefaultEnvironment,
    value::{Value, ExtValueInterface},
};

fn create_csv_state(csv_text: &str) -> State<Value> {
    let mut metadata = MetadataRecord::new();
    metadata.data_format = Some("csv".to_string());
    metadata.with_type_identifier("text".to_string());
    State {
        data: Arc::new(Value::from(csv_text.to_string())),
        metadata: Arc::new(metadata.into()),
    }
}

#[tokio::test]
async fn test_csv_to_dataframe() -> Result<(), Box<dyn std::error::Error>> {
    let csv_data = "name,age,city\nAlice,30,NYC\nBob,25,LA";
    let state = create_csv_state(csv_data);

    let df = liquers_lib::polars::util::try_to_polars_dataframe(&state)?;
    assert_eq!(df.height(), 2);
    assert_eq!(df.width(), 3);

    Ok(())
}
```
