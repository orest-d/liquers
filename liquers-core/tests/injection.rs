use std::collections::HashMap;
use std::sync::Arc;

use liquers_core::{
    commands::{ExtractFromPayload, InjectedFromContext, PayloadType},
    context::{Context, Environment, SimpleEnvironmentWithPayload},
    error::Error,
    state::State,
    value::Value,
};
use liquers_macro::*;

// ============================================================================
// Test Payload Definition
// ============================================================================

#[derive(Clone, Debug)]
pub struct TestPayload {
    pub user_id: String,
    pub window_id: u64,
    pub session_id: String,
    pub context_data: Arc<HashMap<String, String>>,
}

// Mark TestPayload as a payload type
impl PayloadType for TestPayload {}

// Implement InjectedFromContext for TestPayload to enable direct injection
impl<E> InjectedFromContext<E> for TestPayload
where
    E: Environment<Payload = TestPayload>,
{
    fn from_context(name: &str, context: Context<E>) -> Result<Self, Error> {
        context.get_payload_clone().ok_or(Error::general_error(format!(
            "No payload in context for injected parameter {}", name
        )))
    }
}

impl TestPayload {
    pub fn new(user_id: &str, window_id: u64) -> Self {
        Self {
            user_id: user_id.to_string(),
            window_id,
            session_id: format!("session-{}", user_id),
            context_data: Arc::new(HashMap::new()),
        }
    }

    pub fn with_context_data(mut self, key: &str, value: &str) -> Self {
        Arc::make_mut(&mut self.context_data).insert(key.to_string(), value.to_string());
        self
    }
}

type TestEnvironment = SimpleEnvironmentWithPayload<Value, TestPayload>;
type CommandEnvironment = TestEnvironment; // Alias for macro compatibility

// ============================================================================
// Newtype Definitions with ExtractFromPayload
// ============================================================================

/// Newtype for user ID
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserId(pub String);

impl ExtractFromPayload<TestPayload> for UserId {
    fn extract_from_payload(payload: &TestPayload) -> Result<Self, Error> {
        Ok(UserId(payload.user_id.clone()))
    }
}

impl InjectedFromContext<TestEnvironment> for UserId {
    fn from_context(_name: &str, context: Context<TestEnvironment>) -> Result<Self, Error> {
        let payload = context.get_payload_clone()
            .ok_or_else(|| Error::general_error("No payload for UserId".to_string()))?;
        UserId::extract_from_payload(&payload)
    }
}

/// Newtype for window ID
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowId(pub u64);

impl ExtractFromPayload<TestPayload> for WindowId {
    fn extract_from_payload(payload: &TestPayload) -> Result<Self, Error> {
        Ok(WindowId(payload.window_id))
    }
}

impl InjectedFromContext<TestEnvironment> for WindowId {
    fn from_context(_name: &str, context: Context<TestEnvironment>) -> Result<Self, Error> {
        let payload = context.get_payload_clone()
            .ok_or_else(|| Error::general_error("No payload for WindowId".to_string()))?;
        WindowId::extract_from_payload(&payload)
    }
}

/// Newtype for session ID
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionId(pub String);

impl ExtractFromPayload<TestPayload> for SessionId {
    fn extract_from_payload(payload: &TestPayload) -> Result<Self, Error> {
        Ok(SessionId(payload.session_id.clone()))
    }
}

impl InjectedFromContext<TestEnvironment> for SessionId {
    fn from_context(_name: &str, context: Context<TestEnvironment>) -> Result<Self, Error> {
        let payload = context.get_payload_clone()
            .ok_or_else(|| Error::general_error("No payload for SessionId".to_string()))?;
        SessionId::extract_from_payload(&payload)
    }
}

/// Newtype with validation
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedUserId(pub String);

impl ExtractFromPayload<TestPayload> for ValidatedUserId {
    fn extract_from_payload(payload: &TestPayload) -> Result<Self, Error> {
        // Validate user_id
        if payload.user_id.is_empty() {
            return Err(Error::general_error("User ID cannot be empty".to_string()));
        }

        if !payload
            .user_id
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_')
        {
            return Err(Error::general_error(
                "Invalid user ID format (alphanumeric and underscore only)".to_string(),
            ));
        }

        Ok(ValidatedUserId(payload.user_id.clone()))
    }
}

impl InjectedFromContext<TestEnvironment> for ValidatedUserId {
    fn from_context(_name: &str, context: Context<TestEnvironment>) -> Result<Self, Error> {
        let payload = context.get_payload_clone()
            .ok_or_else(|| Error::general_error("No payload for ValidatedUserId".to_string()))?;
        ValidatedUserId::extract_from_payload(&payload)
    }
}

/// Newtype with default fallback (Note: can't provide default when no payload - will fail)
/// For optional payload, use manual Context access instead
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserIdWithDefault(pub String);

impl ExtractFromPayload<TestPayload> for UserIdWithDefault {
    fn extract_from_payload(payload: &TestPayload) -> Result<Self, Error> {
        Ok(UserIdWithDefault(payload.user_id.clone()))
    }
}

impl InjectedFromContext<TestEnvironment> for UserIdWithDefault {
    fn from_context(_name: &str, context: Context<TestEnvironment>) -> Result<Self, Error> {
        let payload = context.get_payload_clone()
            .ok_or_else(|| Error::general_error("No payload for UserIdWithDefault".to_string()))?;
        UserIdWithDefault::extract_from_payload(&payload)
    }
}

/// Newtype for computed value
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IsHighWindowId(pub bool);

impl ExtractFromPayload<TestPayload> for IsHighWindowId {
    fn extract_from_payload(payload: &TestPayload) -> Result<Self, Error> {
        // Compute value from payload
        Ok(IsHighWindowId(payload.window_id > 100))
    }
}

impl InjectedFromContext<TestEnvironment> for IsHighWindowId {
    fn from_context(_name: &str, context: Context<TestEnvironment>) -> Result<Self, Error> {
        let payload = context.get_payload_clone()
            .ok_or_else(|| Error::general_error("No payload for IsHighWindowId".to_string()))?;
        IsHighWindowId::extract_from_payload(&payload)
    }
}

// ============================================================================
// Test Commands
// ============================================================================

// Pattern 1: Direct payload injection
fn get_full_payload(_state: &State<Value>, payload: TestPayload) -> Result<Value, Error> {
    let result = format!(
        "user:{},window:{},session:{}",
        payload.user_id, payload.window_id, payload.session_id
    );
    Ok(Value::from(result))
}

// Pattern 2: Single newtype injection
fn get_user_id(_state: &State<Value>, user_id: UserId) -> Result<Value, Error> {
    Ok(Value::from(format!("user:{}", user_id.0)))
}

// Pattern 3: Multiple newtype injections
fn get_user_and_window(
    _state: &State<Value>,
    user_id: UserId,
    window_id: WindowId,
) -> Result<Value, Error> {
    Ok(Value::from(format!("user:{},window:{}", user_id.0, window_id.0)))
}

// Pattern 4: All three newtypes
fn get_full_info(
    _state: &State<Value>,
    user_id: UserId,
    window_id: WindowId,
    session_id: SessionId,
) -> Result<Value, Error> {
    Ok(Value::from(format!(
        "user:{},window:{},session:{}",
        user_id.0, window_id.0, session_id.0
    )))
}

// Pattern 5: Manual context access
fn get_context_data(_state: &State<Value>, context: Context<TestEnvironment>) -> Result<Value, Error> {
    let payload = context.get_payload_clone().ok_or_else(|| {
        Error::general_error("No payload available".to_string())
    })?;

    let data = payload
        .context_data
        .get("key")
        .map(|s| s.as_str())
        .unwrap_or("default");

    Ok(Value::from(format!("data:{}", data)))
}

// Pattern 6: Validated injection
fn get_validated_user(_state: &State<Value>, user_id: ValidatedUserId) -> Result<Value, Error> {
    Ok(Value::from(format!("validated:{}", user_id.0)))
}

// Pattern 7: Injection with default
fn get_user_with_default(_state: &State<Value>, user_id: UserIdWithDefault) -> Result<Value, Error> {
    Ok(Value::from(format!("user:{}", user_id.0)))
}

// Pattern 8: Computed value injection
fn check_high_window(_state: &State<Value>, is_high: IsHighWindowId) -> Result<Value, Error> {
    Ok(Value::from(format!("high:{}", is_high.0)))
}

// Pattern 9: Mixed regular and injected parameters
fn greet_user(
    _state: &State<Value>,
    user_id: UserId,
    greeting: String,
) -> Result<Value, Error> {
    Ok(Value::from(format!("{}, {}!", greeting, user_id.0)))
}

// Pattern 10: Async command with injection
async fn async_get_user(_state: State<Value>, user_id: UserId) -> Result<Value, Error> {
    // Simulate async work
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
    Ok(Value::from(format!("async:user:{}", user_id.0)))
}

// ============================================================================
// Tests
// ============================================================================

#[tokio::test]
async fn test_direct_payload_injection() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = TestEnvironment::new();
    let cr = &mut env.command_registry;

    register_command!(cr, fn get_full_payload(state, payload: TestPayload injected) -> result)?;

    let envref = env.to_ref();
    let payload = TestPayload::new("alice", 42);

    let asset = envref
        .evaluate_immediately("/-/get_full_payload", payload)
        .await?;
    let state = asset.get().await?;
    let result = state.try_into_string()?;

    assert_eq!(result, "user:alice,window:42,session:session-alice");
    Ok(())
}

#[tokio::test]
async fn test_single_newtype_injection() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = TestEnvironment::new();
    let cr = &mut env.command_registry;

    register_command!(cr, fn get_user_id(state, user_id: UserId injected) -> result)?;

    let envref = env.to_ref();
    let payload = TestPayload::new("bob", 99);

    let asset = envref.evaluate_immediately("/-/get_user_id", payload).await?;
    let state = asset.get().await?;
    let result = state.try_into_string()?;

    assert_eq!(result, "user:bob");
    Ok(())
}

#[tokio::test]
async fn test_multiple_newtype_injections() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = TestEnvironment::new();
    let cr = &mut env.command_registry;

    register_command!(cr, fn get_user_and_window(
        state,
        user_id: UserId injected,
        window_id: WindowId injected
    ) -> result)?;

    let envref = env.to_ref();
    let payload = TestPayload::new("charlie", 123);

    let asset = envref
        .evaluate_immediately("/-/get_user_and_window", payload)
        .await?;
    let state = asset.get().await?;
    let result = state.try_into_string()?;

    assert_eq!(result, "user:charlie,window:123");
    Ok(())
}

#[tokio::test]
async fn test_all_newtypes_injection() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = TestEnvironment::new();
    let cr = &mut env.command_registry;

    register_command!(cr, fn get_full_info(
        state,
        user_id: UserId injected,
        window_id: WindowId injected,
        session_id: SessionId injected
    ) -> result)?;

    let envref = env.to_ref();
    let payload = TestPayload::new("diana", 456);

    let asset = envref
        .evaluate_immediately("/-/get_full_info", payload)
        .await?;
    let state = asset.get().await?;
    let result = state.try_into_string()?;

    assert_eq!(result, "user:diana,window:456,session:session-diana");
    Ok(())
}

#[tokio::test]
async fn test_manual_context_access() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = TestEnvironment::new();
    let cr = &mut env.command_registry;

    register_command!(cr, fn get_context_data(state, context) -> result)?;

    let envref = env.to_ref();
    let payload = TestPayload::new("eve", 789).with_context_data("key", "custom_value");

    let asset = envref
        .evaluate_immediately("/-/get_context_data", payload)
        .await?;
    let state = asset.get().await?;
    let result = state.try_into_string()?;

    assert_eq!(result, "data:custom_value");
    Ok(())
}

#[tokio::test]
async fn test_validated_injection_success() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = TestEnvironment::new();
    let cr = &mut env.command_registry;

    register_command!(cr, fn get_validated_user(state, user_id: ValidatedUserId injected) -> result)?;

    let envref = env.to_ref();
    let payload = TestPayload::new("valid_user_123", 1);

    let asset = envref
        .evaluate_immediately("/-/get_validated_user", payload)
        .await?;
    let state = asset.get().await?;
    let result = state.try_into_string()?;

    assert_eq!(result, "validated:valid_user_123");
    Ok(())
}

#[tokio::test]
async fn test_validated_injection_failure() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = TestEnvironment::new();
    let cr = &mut env.command_registry;

    register_command!(cr, fn get_validated_user(state, user_id: ValidatedUserId injected) -> result)?;

    let envref = env.to_ref();
    let payload = TestPayload::new("invalid-user!", 1); // Contains invalid character

    let asset_result = envref
        .evaluate_immediately("/-/get_validated_user", payload)
        .await;

    // Check if we got an asset (might have been created but failed)
    match asset_result {
        Ok(asset) => {
            // Asset was created, check if it has an error status
            let state_result = asset.get().await;
            assert!(
                state_result.is_err(),
                "Should fail validation - got: {:?}", state_result
            );
            if let Err(err) = state_result {
                assert!(
                    err.to_string().contains("Invalid user ID format"),
                    "Error message should mention validation failure, got: {}",
                    err
                );
            }
        }
        Err(err) => {
            // Failed before asset creation
            assert!(
                err.to_string().contains("Invalid user ID format"),
                "Error message should mention validation failure, got: {}",
                err
            );
        }
    }

    Ok(())
}

#[tokio::test]
async fn test_injection_with_default_with_payload() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = TestEnvironment::new();
    let cr = &mut env.command_registry;

    register_command!(cr, fn get_user_with_default(state, user_id: UserIdWithDefault injected) -> result)?;

    let envref = env.to_ref();
    let payload = TestPayload::new("frank", 2);

    let asset = envref
        .evaluate_immediately("/-/get_user_with_default", payload)
        .await?;
    let state = asset.get().await?;
    let result = state.try_into_string()?;

    assert_eq!(result, "user:frank");
    Ok(())
}

#[tokio::test]
async fn test_computed_value_injection() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = TestEnvironment::new();
    let cr = &mut env.command_registry;

    register_command!(cr, fn check_high_window(state, is_high: IsHighWindowId injected) -> result)?;

    let envref = env.to_ref();

    // Test with high window ID
    let payload_high = TestPayload::new("george", 200);
    let asset = envref
        .evaluate_immediately("/-/check_high_window", payload_high)
        .await?;
    let state = asset.get().await?;
    let result = state.try_into_string()?;
    assert_eq!(result, "high:true");

    // Test with low window ID
    let payload_low = TestPayload::new("helen", 50);
    let asset = envref
        .evaluate_immediately("/-/check_high_window", payload_low)
        .await?;
    let state = asset.get().await?;
    let result = state.try_into_string()?;
    assert_eq!(result, "high:false");

    Ok(())
}

#[tokio::test]
async fn test_mixed_regular_and_injected_parameters() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = TestEnvironment::new();
    let cr = &mut env.command_registry;

    register_command!(cr, fn greet_user(
        state,
        user_id: UserId injected,
        greeting: String
    ) -> result)?;

    let envref = env.to_ref();
    let payload = TestPayload::new("ian", 3);

    let asset = envref
        .evaluate_immediately("/-/greet_user-Hello", payload)
        .await?;
    let state = asset.get().await?;
    let result = state.try_into_string()?;

    assert_eq!(result, "Hello, ian!");
    Ok(())
}

#[tokio::test]
async fn test_async_command_with_injection() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = TestEnvironment::new();
    let cr = &mut env.command_registry;

    register_command!(cr, async fn async_get_user(state, user_id: UserId injected) -> result)?;

    let envref = env.to_ref();
    let payload = TestPayload::new("julia", 4);

    let asset = envref
        .evaluate_immediately("/-/async_get_user", payload)
        .await?;
    let state = asset.get().await?;
    let result = state.try_into_string()?;

    assert_eq!(result, "async:user:julia");
    Ok(())
}

#[tokio::test]
async fn test_chained_commands_with_payload() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = TestEnvironment::new();
    let cr = &mut env.command_registry;

    // Command that uses payload
    fn first_cmd(_state: &State<Value>, user_id: UserId) -> Result<Value, Error> {
        Ok(Value::from(format!("step1:{}", user_id.0)))
    }

    // Command that transforms state (doesn't need payload)
    fn second_cmd(state: &State<Value>) -> Result<Value, Error> {
        let prev = state.try_into_string()?;
        Ok(Value::from(format!("{}->step2", prev)))
    }

    // Another command that uses payload
    fn third_cmd(state: &State<Value>, window_id: WindowId) -> Result<Value, Error> {
        let prev = state.try_into_string()?;
        Ok(Value::from(format!("{}->window:{}", prev, window_id.0)))
    }

    register_command!(cr, fn first_cmd(state, user_id: UserId injected) -> result)?;
    register_command!(cr, fn second_cmd(state) -> result)?;
    register_command!(cr, fn third_cmd(state, window_id: WindowId injected) -> result)?;

    let envref = env.to_ref();
    let payload = TestPayload::new("karen", 999);

    let asset = envref
        .evaluate_immediately("/-/first_cmd/second_cmd/third_cmd", payload)
        .await?;
    let state = asset.get().await?;
    let result = state.try_into_string()?;

    assert_eq!(result, "step1:karen->step2->window:999");
    Ok(())
}

#[tokio::test]
async fn test_payload_not_inherited_in_nested_evaluation() -> Result<(), Box<dyn std::error::Error>> {
    // NOTE: Currently payload is NOT inherited in nested evaluations via context.evaluate()
    // This is a known limitation. Nested queries go through AssetManager which doesn't
    // have access to the parent's payload.
    // TODO: Implement payload inheritance for nested evaluations

    let mut env = TestEnvironment::new();
    let cr = &mut env.command_registry;

    // Parent command that evaluates a nested query
    async fn parent_cmd(
        _state: State<Value>,
        user_id: UserId,
        context: Context<TestEnvironment>,
    ) -> Result<Value, Error> {
        // Nested evaluation - payload will NOT be available to child
        let nested_query = liquers_core::parse::parse_query("/-/child_cmd")?;
        let nested_result = match context.evaluate(&nested_query).await {
            Ok(asset) => {
                match asset.get().await {
                    Ok(state) => state.try_into_string().unwrap_or_else(|_| "error".to_string()),
                    Err(_) => "None".to_string(),
                }
            }
            Err(_) => "None".to_string(),
        };

        Ok(Value::from(format!("parent:{}|child:{}", user_id.0, nested_result)))
    }

    // Child command that requires payload (will fail when called from parent)
    fn child_cmd(_state: &State<Value>, window_id: WindowId) -> Result<Value, Error> {
        Ok(Value::from(format!("window:{}", window_id.0)))
    }

    register_command!(cr, async fn parent_cmd(
        state,
        user_id: UserId injected,
        context
    ) -> result)?;
    register_command!(cr, fn child_cmd(state, window_id: WindowId injected) -> result)?;

    let envref = env.to_ref();
    let payload = TestPayload::new("laura", 777);

    let asset = envref
        .evaluate_immediately("/-/parent_cmd", payload)
        .await?;
    let state = asset.get().await?;
    let result = state.try_into_string()?;

    // Child command fails because payload is not inherited
    assert_eq!(result, "parent:laura|child:None");
    Ok(())
}
