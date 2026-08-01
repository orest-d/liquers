//! Integration tests for payload inheritance in nested evaluation.
//!
//! Covers the behaviour introduced for `specs/ISSUES.md`:
//! PAYLOAD-NESTED-EVALUATION-INHERITANCE — a nested query whose plan requires a payload
//! inherits the parent evaluation's payload, together with the boundaries that limit it.

use std::collections::HashMap;
use std::sync::Arc;

use liquers_core::{
    command_metadata::PayloadRequirement,
    commands::{InjectedFromContext, PayloadType},
    context::{Context, Environment, ImmediateEnvironmentWithPayload, SimpleEnvironmentWithPayload},
    error::Error,
    metadata::Metadata,
    parse::{parse_key, parse_query},
    query::Key,
    recipes::{DefaultRecipeProvider, Recipe, RecipeList},
    state::State,
    store::{AsyncMemoryStore, AsyncStore},
    value::Value,
};
use liquers_macro::register_command;

// ============================================================================
// Shared payload and injection newtypes
// ============================================================================

#[derive(Clone, Debug)]
pub struct TestPayload {
    pub user_id: String,
    pub window_id: u64,
    pub log: Arc<std::sync::Mutex<Vec<String>>>,
}

impl PayloadType for TestPayload {}

impl TestPayload {
    fn new(user_id: &str, window_id: u64) -> Self {
        Self {
            user_id: user_id.to_string(),
            window_id,
            log: Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }
}

type QueuedEnv = SimpleEnvironmentWithPayload<Value, TestPayload>;
type InlineEnv = ImmediateEnvironmentWithPayload<Value, TestPayload>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserId(pub String);

impl<E> InjectedFromContext<E> for UserId
where
    E: Environment<Payload = TestPayload>,
{
    fn from_context(_name: &str, context: Context<E>) -> Result<Self, Error> {
        Ok(UserId(
            context
                .get_payload_clone()
                .ok_or_else(|| Error::general_error("No payload for UserId".to_string()))?
                .user_id,
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowId(pub u64);

impl<E> InjectedFromContext<E> for WindowId
where
    E: Environment<Payload = TestPayload>,
{
    fn from_context(_name: &str, context: Context<E>) -> Result<Self, Error> {
        Ok(WindowId(
            context
                .get_payload_clone()
                .ok_or_else(|| Error::general_error("No payload for WindowId".to_string()))?
                .window_id,
        ))
    }
}

// ============================================================================
// I5: keyed recipes are a payload boundary
// ============================================================================

/// A recipe stored at a key may not require a payload: a key names one shared asset while a
/// payload is supplied per evaluation, so there is no payload that could satisfy it.
///
/// NOTE: this is verified through recipe/plan resolution rather than through
/// `evaluate("-R/<key>")`. A keyed recipe whose command is volatile — which
/// `payload: required` implies — currently fails with a spurious dependency cycle before any
/// recipe check runs. That is a **pre-existing** defect, reproducible with a plain
/// `volatile: true` command and no payload involvement whatsoever — see
/// `test_volatile_keyed_recipe_cycles_preexisting_defect` below and
/// `specs/ISSUES.md`: VOLATILE-KEYED-RECIPE-SELF-DELEGATION.
#[tokio::test]
async fn test_keyed_recipe_requiring_payload_is_rejected(
) -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = QueuedEnv;
    let mut env = QueuedEnv::new();

    fn needs_payload(window_id: WindowId) -> Result<Value, Error> {
        Ok(Value::from(format!("window:{}", window_id.0)))
    }

    let cr = &mut env.command_registry;
    register_command!(cr, fn needs_payload(window_id: WindowId injected) -> result
        payload: required
    )?;

    let recipe = Recipe::new(
        "needs_payload/dash.txt".to_string(),
        "Dashboard".to_string(),
        "Recipe that (invalidly) requires a payload".to_string(),
    )?;
    let mut recipe_list = RecipeList::new();
    recipe_list.add_recipe(recipe.clone());

    let store = AsyncMemoryStore::new(&Key::new());
    store
        .set(
            &parse_key("recipes.yaml")?,
            serde_yaml::to_string(&recipe_list)?.as_bytes(),
            &Metadata::new(),
        )
        .await?;
    env.with_async_store(Box::new(store));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));

    let envref = env.to_ref();
    let key = parse_key("dash.txt")?;

    // The recipe's plan does require a payload ...
    let plan = recipe.to_plan(envref.get_command_metadata_registry())?;
    assert_eq!(plan.payload_required, PayloadRequirement::Required);

    // ... and building it *for a key* is therefore rejected.
    let err = recipe
        .to_plan_for_key(envref.get_command_metadata_registry(), &key)
        .expect_err("keyed recipe requiring a payload must be rejected");
    assert!(
        err.to_string().contains("keyed recipes cannot receive one"),
        "unexpected message: {}",
        err
    );

    // The same rejection reaches asset introspection, which is a user-visible surface.
    let info = envref.get_recipe_provider().get_asset_info(&key, envref.clone()).await?;
    assert!(
        info.is_error,
        "asset info for an invalid keyed recipe should be marked as an error"
    );
    assert!(
        info.message.contains("keyed recipes cannot receive one"),
        "unexpected asset-info message: {}",
        info.message
    );
    Ok(())
}

/// The mirror of the above: the same command evaluated directly (not through a key) is fine.
#[tokio::test]
async fn test_same_command_works_when_evaluated_directly(
) -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = QueuedEnv;
    let mut env = QueuedEnv::new();

    fn needs_payload(_state: &State<Value>, window_id: WindowId) -> Result<Value, Error> {
        Ok(Value::from(format!("window:{}", window_id.0)))
    }

    let cr = &mut env.command_registry;
    register_command!(cr, fn needs_payload(state, window_id: WindowId injected) -> result
        payload: required
    )?;

    let envref = env.to_ref();
    let asset = envref
        .evaluate_immediately("/-/needs_payload", TestPayload::new("alice", 42))
        .await?;
    assert_eq!(asset.get().await?.try_into_string()?, "window:42");
    Ok(())
}

/// Documents a **pre-existing defect**, unrelated to payload: a keyed recipe whose command is
/// merely `volatile: true` fails with a spurious dependency cycle. No payload is involved
/// anywhere in this test.
///
/// It matters here only because `payload: required` implies `volatile`, so every keyed payload
/// recipe hits this path — which is why the keyed-payload rejection above is verified through
/// recipe resolution rather than through `evaluate("-R/<key>")`.
///
/// This test asserts the *current* broken behaviour so that fixing the defect fails loudly here
/// and this test can be inverted at that point.
#[tokio::test]
async fn test_volatile_keyed_recipe_cycles_preexisting_defect(
) -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = QueuedEnv;
    let mut env = QueuedEnv::new();

    fn vol_cmd() -> Result<Value, Error> {
        Ok(Value::from("vol"))
    }

    let cr = &mut env.command_registry;
    register_command!(cr, fn vol_cmd() -> result volatile: true)?;

    let recipe = Recipe::new(
        "vol_cmd/dash.txt".to_string(),
        "Dashboard".to_string(),
        "Volatile recipe, no payload".to_string(),
    )?;
    let mut recipe_list = RecipeList::new();
    recipe_list.add_recipe(recipe);

    let store = AsyncMemoryStore::new(&Key::new());
    store
        .set(
            &parse_key("recipes.yaml")?,
            serde_yaml::to_string(&recipe_list)?.as_bytes(),
            &Metadata::new(),
        )
        .await?;
    env.with_async_store(Box::new(store));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));

    let envref = env.to_ref();
    let asset = envref.evaluate("-R/dash.txt").await?;
    let state = asset.get().await?;
    let outcome = state.value_state();
    match outcome {
        Ok(_) => panic!(
            "volatile keyed recipe unexpectedly succeeded - the pre-existing defect may have \
             been fixed; invert this test and re-enable the evaluate() path in \
             test_keyed_recipe_requiring_payload_is_rejected"
        ),
        Err(e) => assert!(
            e.to_string().contains("Dependency cycle"),
            "expected the known spurious cycle, got: {}",
            e
        ),
    }
    Ok(())
}
