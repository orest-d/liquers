// Integration tests for volatility system
use liquers_core::{
    assets::AssetManager,
    command_metadata::{ArgumentInfo, CommandMetadata, CommandMetadataRegistry},
    commands::{CommandArguments, CommandRegistry},
    context::{Context, EnvRef, Environment, SimpleEnvironment},
    error::Error,
    interpreter::make_plan,
    metadata::Status,
    parse::parse_query,
    state::State,
    store::{AsyncStore, AsyncStoreWrapper, MemoryStore},
    value::Value,
};
use liquers_macro::register_command;
use std::sync::Arc;

/// Test that volatile query creates plan with is_volatile = true
#[tokio::test]
async fn test_volatile_query_v_instruction() -> Result<(), Box<dyn std::error::Error>> {
    // Setup environment
    let env = SimpleEnvironment::<Value>::new();
    let envref = env.to_ref();

    // Build plan for volatile query (v instruction)
    let query = parse_query("v")?;
    let plan = make_plan(envref, &query).await?;

    // Verify plan is marked volatile
    assert!(plan.is_volatile, "Plan with 'v' instruction should be volatile");

    Ok(())
}

/// Test that volatile command marks plan volatile
#[tokio::test]
async fn test_volatile_command_marks_plan() -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = SimpleEnvironment<Value>;
    // Setup environment with volatile command
    let mut env = SimpleEnvironment::<Value>::new();

    // Register a volatile command
    fn test_volatile_cmd(_state: &State<Value>) -> Result<Value, Error> {
        Ok(Value::from("volatile result"))
    }

    let cr = &mut env.command_registry;
    register_command!(cr,
        fn test_volatile_cmd(state) -> result
        namespace: "test"
        volatile: true
    )?;

    let envref = env.to_ref();

    // Build plan for volatile command
    let query = parse_query("ns-test/test_volatile_cmd")?;
    let plan = make_plan(envref, &query).await?;

    // Verify plan is marked volatile
    assert!(plan.is_volatile, "Plan with volatile command should be volatile");

    Ok(())
}

/// Test that non-volatile query creates plan with is_volatile = false
#[tokio::test]
async fn test_non_volatile_query() -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = SimpleEnvironment<Value>;
    // Setup environment
    let mut env = SimpleEnvironment::<Value>::new();

    // Register a non-volatile command
    fn test_cmd(_state: &State<Value>) -> Result<Value, Error> {
        Ok(Value::from("result"))
    }

    let cr = &mut env.command_registry;
    register_command!(cr,
        fn test_cmd(state) -> result
        namespace: "test"
    )?;

    let envref = env.to_ref();

    // Build plan for non-volatile command
    let query = parse_query("ns-test/test_cmd")?;
    let plan = make_plan(envref, &query).await?;

    // Verify plan is NOT marked volatile
    assert!(!plan.is_volatile, "Plan without volatile elements should not be volatile");

    Ok(())
}

/// Test that AssetManager doesn't cache volatile assets
#[tokio::test]
async fn test_asset_manager_volatile_no_cache() -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = SimpleEnvironment<Value>;
    // Setup environment with volatile command
    let mut env = SimpleEnvironment::<Value>::new();

    fn volatile_cmd(_state: &State<Value>) -> Result<Value, Error> {
        Ok(Value::from("volatile"))
    }

    let cr = &mut env.command_registry;
    register_command!(cr,
        fn volatile_cmd(state) -> result
        namespace: "test"
        volatile: true
    )?;

    let envref = env.to_ref();
    let manager = envref.get_asset_manager();

    // Request same volatile query 3 times
    let query = parse_query("ns-test/volatile_cmd")?;
    let asset1 = manager.get_asset(&query).await?;
    let asset2 = manager.get_asset(&query).await?;
    let asset3 = manager.get_asset(&query).await?;

    // All should have different IDs (no caching)
    let id1 = asset1.id();
    let id2 = asset2.id();
    let id3 = asset3.id();

    assert_ne!(id1, id2, "Volatile assets should not be cached (id1 != id2)");
    assert_ne!(id2, id3, "Volatile assets should not be cached (id2 != id3)");
    assert_ne!(id1, id3, "Volatile assets should not be cached (id1 != id3)");

    Ok(())
}

/// Test that AssetManager caches non-volatile assets
#[tokio::test]
async fn test_asset_manager_non_volatile_caching() -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = SimpleEnvironment<Value>;
    // Setup environment with non-volatile command
    let mut env = SimpleEnvironment::<Value>::new();

    fn normal_cmd(_state: &State<Value>) -> Result<Value, Error> {
        Ok(Value::from("normal"))
    }

    let cr = &mut env.command_registry;
    register_command!(cr,
        fn normal_cmd(state) -> result
        namespace: "test"
    )?;

    let envref = env.to_ref();
    let manager = envref.get_asset_manager();

    // Request same non-volatile query 3 times
    let query = parse_query("ns-test/normal_cmd")?;
    let asset1 = manager.get_asset(&query).await?;
    let asset2 = manager.get_asset(&query).await?;
    let asset3 = manager.get_asset(&query).await?;

    // All should have same ID (cached)
    let id1 = asset1.id();
    let id2 = asset2.id();
    let id3 = asset3.id();

    assert_eq!(id1, id2, "Non-volatile assets should be cached (id1 == id2)");
    assert_eq!(id2, id3, "Non-volatile assets should be cached (id2 == id3)");

    Ok(())
}

/// Test v/q edge case: "v" is volatile, "v/q" is not
#[tokio::test]
async fn test_v_instruction_q_edge_case() -> Result<(), Box<dyn std::error::Error>> {
    let env = SimpleEnvironment::<Value>::new();
    let envref = env.to_ref();

    // Test 1: "v" should be volatile
    let query1 = parse_query("v")?;
    let plan1 = make_plan(envref.clone(), &query1).await?;
    assert!(plan1.is_volatile, "Query 'v' should be volatile");

    // Test 2: "v/q" should NOT be volatile (evaluates to Query("v") value)
    let query2 = parse_query("v/q")?;
    let plan2 = make_plan(envref, &query2).await?;
    assert!(!plan2.is_volatile, "Query 'v/q' should NOT be volatile");

    Ok(())
}

/// Test plan serialization preserves volatility
#[tokio::test]
async fn test_plan_serialization_round_trip() -> Result<(), Box<dyn std::error::Error>> {
    let env = SimpleEnvironment::<Value>::new();
    let envref = env.to_ref();

    // Create volatile plan
    let query = parse_query("v")?;
    let plan = make_plan(envref, &query).await?;
    assert!(plan.is_volatile);

    // Serialize to JSON
    let json = serde_json::to_string(&plan)?;

    // Deserialize
    let deserialized: liquers_core::plan::Plan = serde_json::from_str(&json)?;

    // Verify is_volatile field survived round-trip
    assert_eq!(deserialized.is_volatile, plan.is_volatile);

    Ok(())
}

/// Test Context is created with correct volatility
#[tokio::test]
async fn test_context_volatility_propagation() -> Result<(), Box<dyn std::error::Error>> {
    let env = SimpleEnvironment::<Value>::new();
    let envref = env.to_ref();

    // Create volatile plan
    let query = parse_query("v")?;
    let plan = make_plan(envref.clone(), &query).await?;
    assert!(plan.is_volatile);

    // Create context from plan volatility
    let assetref = liquers_core::assets::AssetRef::new_temporary(envref);
    let context = Context::new(assetref, plan.is_volatile).await;

    // Verify context has correct volatility
    assert!(context.is_volatile());

    Ok(())
}

/// Test AssetRef::to_override transitions correctly
#[tokio::test]
async fn test_asset_to_override() -> Result<(), Box<dyn std::error::Error>> {
    let env = SimpleEnvironment::<Value>::new();
    let envref = env.to_ref();

    // Create a temporary asset
    let assetref = liquers_core::assets::AssetRef::new_temporary(envref);

    // Set it to Override
    assetref.to_override().await?;

    // Verify status is Override
    let status = assetref.status().await;
    assert_eq!(status, Status::Override);

    // Calling again should be idempotent
    assetref.to_override().await?;
    let status = assetref.status().await;
    assert_eq!(status, Status::Override);

    Ok(())
}
