// Integration tests for expiration system
use liquers_core::{
    assets::AssetRef,
    commands::CommandRegistry,
    context::{Context, Environment, SimpleEnvironment},
    error::Error,
    expiration::{ExpirationTime, Expires},
    interpreter::make_plan,
    metadata::Status,
    parse::parse_query,
    state::State,
    value::Value,
};
use liquers_macro::register_command;

/// Test that expiring command marks plan with expires
#[tokio::test]
async fn test_expiring_command_marks_plan() -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = SimpleEnvironment<Value>;

    let mut env = SimpleEnvironment::<Value>::new();

    fn test_expiring_cmd(_state: &State<Value>) -> Result<Value, Error> {
        Ok(Value::from("expiring result"))
    }

    let cr = &mut env.command_registry;
    register_command!(cr,
        fn test_expiring_cmd(state) -> result
        namespace: "test"
        expires: "in 5 min"
    )?;

    let envref = env.to_ref();

    let query = parse_query("ns-test/test_expiring_cmd")?;
    let plan = make_plan(envref, &query).await?;

    // Plan should have expires set
    assert_eq!(
        plan.expires,
        Expires::InDuration(std::time::Duration::from_secs(300))
    );
    // Expiring commands should NOT be volatile
    assert!(!plan.is_volatile);

    Ok(())
}

/// Test that immediately-expiring command marks plan volatile
#[tokio::test]
async fn test_immediately_expiring_command_is_volatile() -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = SimpleEnvironment<Value>;

    let mut env = SimpleEnvironment::<Value>::new();

    fn test_immediate_cmd(_state: &State<Value>) -> Result<Value, Error> {
        Ok(Value::from("immediate result"))
    }

    let cr = &mut env.command_registry;
    register_command!(cr,
        fn test_immediate_cmd(state) -> result
        namespace: "test"
        expires: "immediately"
    )?;

    let envref = env.to_ref();

    let query = parse_query("ns-test/test_immediate_cmd")?;
    let plan = make_plan(envref, &query).await?;

    // Immediately expiring is treated as volatile
    assert_eq!(plan.expires, Expires::Immediately);
    assert!(plan.is_volatile, "Immediately expiring should be volatile");

    Ok(())
}

/// Test that non-expiring command has Never expires
#[tokio::test]
async fn test_non_expiring_command_plan() -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = SimpleEnvironment<Value>;

    let mut env = SimpleEnvironment::<Value>::new();

    fn test_normal_cmd(_state: &State<Value>) -> Result<Value, Error> {
        Ok(Value::from("normal result"))
    }

    let cr = &mut env.command_registry;
    register_command!(cr,
        fn test_normal_cmd(state) -> result
        namespace: "test"
    )?;

    let envref = env.to_ref();

    let query = parse_query("ns-test/test_normal_cmd")?;
    let plan = make_plan(envref, &query).await?;

    assert_eq!(plan.expires, Expires::Never);
    assert!(!plan.is_volatile);

    Ok(())
}

/// Test AssetRef expire() transitions Ready to Expired
#[tokio::test]
async fn test_asset_ref_expire_from_ready() -> Result<(), Box<dyn std::error::Error>> {
    let env = SimpleEnvironment::<Value>::new();
    let envref = env.to_ref();

    let assetref = AssetRef::new_temporary(envref);
    // Transition to Override first (Override can expire)
    assetref.to_override().await?;

    // Expire the asset
    assetref.expire().await?;
    assert_eq!(assetref.status().await, Status::Expired);

    // Expiring again should be idempotent
    assetref.expire().await?;
    assert_eq!(assetref.status().await, Status::Expired);

    Ok(())
}

/// Test AssetRef expire() on Source returns error
#[tokio::test]
async fn test_asset_ref_expire_from_source_errors() -> Result<(), Box<dyn std::error::Error>> {
    let env = SimpleEnvironment::<Value>::new();
    let envref = env.to_ref();

    let assetref = AssetRef::new_temporary(envref);
    // Set status to Source
    assetref.set_status(Status::Source).await?;

    // Expiring a Source asset should fail
    let result = assetref.expire().await;
    assert!(result.is_err());

    Ok(())
}

/// Test ExpirationTime accessors on AssetRef
#[tokio::test]
async fn test_asset_ref_expiration_time() -> Result<(), Box<dyn std::error::Error>> {
    let env = SimpleEnvironment::<Value>::new();
    let envref = env.to_ref();

    let assetref = AssetRef::new_temporary(envref);

    // Default is Never
    let exp_time = assetref.expiration_time().await;
    assert_eq!(exp_time, ExpirationTime::Never);

    // Set expiration time
    let future = chrono::Utc::now() + chrono::Duration::hours(1);
    assetref.set_expiration_time(ExpirationTime::At(future)).await;

    let exp_time = assetref.expiration_time().await;
    assert_eq!(exp_time, ExpirationTime::At(future));
    assert!(!assetref.is_expired().await);

    Ok(())
}

/// Test that register_command! with expires: creates valid metadata
/// (verified indirectly through plan building)
#[tokio::test]
async fn test_register_command_expires_in_plan() -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = SimpleEnvironment<Value>;

    let mut env = SimpleEnvironment::<Value>::new();

    fn expiring_cmd(_state: &State<Value>) -> Result<Value, Error> {
        Ok(Value::from("result"))
    }

    let cr = &mut env.command_registry;
    register_command!(cr,
        fn expiring_cmd(state) -> result
        namespace: "test"
        expires: "in 10 min"
    )?;

    let envref = env.to_ref();

    // Build plan and verify expires propagated from command metadata
    let query = parse_query("ns-test/expiring_cmd")?;
    let plan = make_plan(envref, &query).await?;
    assert_eq!(
        plan.expires,
        Expires::InDuration(std::time::Duration::from_secs(600))
    );

    Ok(())
}

/// Test plan serialization preserves expires
#[tokio::test]
async fn test_plan_expires_serialization_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = SimpleEnvironment<Value>;

    let mut env = SimpleEnvironment::<Value>::new();

    fn expiring_cmd2(_state: &State<Value>) -> Result<Value, Error> {
        Ok(Value::from("result"))
    }

    let cr = &mut env.command_registry;
    register_command!(cr,
        fn expiring_cmd2(state) -> result
        namespace: "test"
        expires: "in 1 hours"
    )?;

    let envref = env.to_ref();

    let query = parse_query("ns-test/expiring_cmd2")?;
    let plan = make_plan(envref, &query).await?;

    // Serialize and deserialize
    let json = serde_json::to_string(&plan)?;
    let plan2: liquers_core::plan::Plan = serde_json::from_str(&json)?;

    assert_eq!(plan2.expires, plan.expires);
    assert_eq!(plan2.is_volatile, plan.is_volatile);

    Ok(())
}
