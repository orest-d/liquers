//! Integration tests for payload inheritance in nested evaluation.
//!
//! Covers the behaviour introduced for `specs/archive/2026-08-08-issues.md` (PAYLOAD-NESTED-EVALUATION-INHERITANCE, resolved):
//! PAYLOAD-NESTED-EVALUATION-INHERITANCE — a nested query whose plan requires a payload
//! inherits the parent evaluation's payload, together with the boundaries that limit it.

use std::collections::HashMap;
use std::sync::Arc;

use liquers_core::{
    command_metadata::PayloadRequirement,
    commands::{InjectedFromContext, PayloadType},
    context::{
        Context, Environment, ImmediateEnvironmentWithPayload, SimpleEnvironmentWithPayload,
    },
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
/// Verified three ways: through plan resolution, through asset introspection, and through
/// `evaluate("-R/<key>")` — the path a caller actually takes. The evaluation check was
/// disabled while `VOLATILE-KEYED-RECIPE-SELF-DELEGATION` was open, because
/// `payload: required` implies `volatile` and a volatile keyed recipe failed with a spurious
/// dependency cycle before any recipe check ran.
#[tokio::test]
async fn test_keyed_recipe_requiring_payload_is_rejected() -> Result<(), Box<dyn std::error::Error>>
{
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
    let info = envref
        .get_recipe_provider()
        .get_asset_info(&key, envref.clone())
        .await?;
    assert!(
        info.is_error,
        "asset info for an invalid keyed recipe should be marked as an error"
    );
    assert!(
        info.message.contains("keyed recipes cannot receive one"),
        "unexpected asset-info message: {}",
        info.message
    );

    // And the same rejection reaches evaluation, which is the path a caller actually takes.
    let asset = envref.evaluate("-R/dash.txt").await?;
    let err = asset
        .get()
        .await?
        .value_state()
        .expect_err("a keyed recipe requiring a payload must not evaluate");
    assert!(
        err.to_string().contains("keyed recipes cannot receive one"),
        "expected the payload-boundary rejection, got: {}",
        err
    );
    Ok(())
}

/// The mirror of the above: the same command evaluated directly (not through a key) is fine.
#[tokio::test]
async fn test_same_command_works_when_evaluated_directly() -> Result<(), Box<dyn std::error::Error>>
{
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

/// A keyed recipe whose command is `volatile: true` evaluates to its value. No payload is
/// involved anywhere in this test.
///
/// It belongs in this file because `payload: required` implies `volatile`, so every keyed
/// payload recipe runs through this path.
///
/// **This test used to assert the opposite.** A volatile key is deliberately never registered
/// in the manager's key map, so the old id-identity ownership test — which compared against
/// whatever `AssetManager::get` returned, and `get` mints a fresh asset for a volatile key on
/// every call — never matched, and the asset delegated to itself. The ownership question is
/// now asked with a non-evaluating map read, where "no registered owner" means "evaluate it
/// here". See `VOLATILE-KEYED-RECIPE-SELF-DELEGATION` and
/// `specs/design/keyed-recipe-ownership/`.
#[tokio::test]
async fn test_volatile_keyed_recipe_evaluates() -> Result<(), Box<dyn std::error::Error>> {
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
    let value = state
        .value_state()
        .map_err(|e| format!("volatile keyed recipe should evaluate, got: {}", e))?;
    assert_eq!(value.try_into_string()?, "vol");
    Ok(())
}

// ============================================================================
// I2: the other nested entry points
// ============================================================================

/// `Context::get_dependency_state` inherits the payload.
#[tokio::test]
async fn test_payload_inherited_via_get_dependency_state() -> Result<(), Box<dyn std::error::Error>>
{
    type CommandEnvironment = QueuedEnv;
    let mut env = QueuedEnv::new();

    async fn parent(_s: State<Value>, context: Context<QueuedEnv>) -> Result<Value, Error> {
        let q = parse_query("/-/child")?;
        let state = context.get_dependency_state(&q).await?;
        Ok(Value::from(format!(
            "via_state:{}",
            state.try_into_string()?
        )))
    }
    fn child(_s: &State<Value>, window_id: WindowId) -> Result<Value, Error> {
        Ok(Value::from(format!("window:{}", window_id.0)))
    }

    let cr = &mut env.command_registry;
    register_command!(cr, async fn parent(state, context) -> result payload: required)?;
    register_command!(cr, fn child(state, window_id: WindowId injected) -> result
        payload: required)?;

    let envref = env.to_ref();
    let asset = envref
        .evaluate_immediately("/-/parent", TestPayload::new("nina", 55))
        .await?;
    assert_eq!(asset.get().await?.try_into_string()?, "via_state:window:55");
    Ok(())
}

/// `Context::apply` inherits the payload.
#[tokio::test]
async fn test_payload_inherited_via_apply() -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = QueuedEnv;
    let mut env = QueuedEnv::new();

    async fn parent(_s: State<Value>, context: Context<QueuedEnv>) -> Result<Value, Error> {
        let q = parse_query("/-/applied")?;
        let input = State::new().with_data(Value::from("seed"));
        let asset = context.apply(&q, input).await?;
        Ok(Value::from(format!(
            "via_apply:{}",
            asset.get().await?.try_into_string()?
        )))
    }
    fn applied(state: &State<Value>, window_id: WindowId) -> Result<Value, Error> {
        Ok(Value::from(format!(
            "{}:{}",
            state.try_into_string()?,
            window_id.0
        )))
    }

    let cr = &mut env.command_registry;
    register_command!(cr, async fn parent(state, context) -> result payload: required)?;
    register_command!(cr, fn applied(state, window_id: WindowId injected) -> result
        payload: required)?;

    let envref = env.to_ref();
    let asset = envref
        .evaluate_immediately("/-/parent", TestPayload::new("omar", 66))
        .await?;
    assert_eq!(asset.get().await?.try_into_string()?, "via_apply:seed:66");
    Ok(())
}

// ============================================================================
// I3: a payload-free child stays a normal shared asset
// ============================================================================

/// A payload-free dependency of a payload-requiring parent goes through the asset manager
/// and is cached and reused, unaffected by the parent's payload.
#[tokio::test]
async fn test_payload_free_child_is_cached_and_shared() -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = QueuedEnv;
    let mut env = QueuedEnv::new();

    async fn parent(_s: State<Value>, context: Context<QueuedEnv>) -> Result<Value, Error> {
        let q = parse_query("/-/pure_child")?;
        let a = context.evaluate(&q).await?;
        let b = context.evaluate(&q).await?;
        // A cached, shared asset is the same instance on both requests.
        Ok(Value::from(format!("same:{}", a.id() == b.id())))
    }
    fn pure_child(_s: &State<Value>) -> Result<Value, Error> {
        Ok(Value::from("pure"))
    }

    let cr = &mut env.command_registry;
    register_command!(cr, async fn parent(state, context) -> result payload: required)?;
    register_command!(cr, fn pure_child(state) -> result)?;

    let envref = env.to_ref();
    let asset = envref
        .evaluate_immediately("/-/parent", TestPayload::new("pia", 7))
        .await?;
    assert_eq!(asset.get().await?.try_into_string()?, "same:true");
    Ok(())
}

// ============================================================================
// I6: cycle guard
// ============================================================================

/// Two payload-requiring commands calling each other must be detected. Neither end is a
/// dependency-graph node, so this relies on the evaluation-path guard on `Context`.
#[tokio::test]
async fn test_payload_cycle_is_detected() -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = QueuedEnv;
    let mut env = QueuedEnv::new();

    async fn cyc_a(_s: State<Value>, context: Context<QueuedEnv>) -> Result<Value, Error> {
        let q = parse_query("/-/cyc_b")?;
        match context.evaluate(&q).await {
            Ok(asset) => match asset.get().await {
                Ok(state) => match state.value_state() {
                    Ok(vs) => Ok(Value::from(
                        vs.try_into_string().unwrap_or_else(|_| "err".to_string()),
                    )),
                    Err(e) => Ok(Value::from(format!("INNER:{}", e))),
                },
                Err(e) => Ok(Value::from(format!("INNER:{}", e))),
            },
            Err(e) => Ok(Value::from(format!("CYCLE:{}", e))),
        }
    }
    async fn cyc_b(_s: State<Value>, context: Context<QueuedEnv>) -> Result<Value, Error> {
        let q = parse_query("/-/cyc_a")?;
        match context.evaluate(&q).await {
            Ok(asset) => match asset.get().await {
                Ok(state) => match state.value_state() {
                    Ok(vs) => Ok(Value::from(
                        vs.try_into_string().unwrap_or_else(|_| "err".to_string()),
                    )),
                    Err(e) => Ok(Value::from(format!("INNER:{}", e))),
                },
                Err(e) => Ok(Value::from(format!("INNER:{}", e))),
            },
            Err(e) => Ok(Value::from(format!("CYCLE:{}", e))),
        }
    }

    let cr = &mut env.command_registry;
    register_command!(cr, async fn cyc_a(state, context) -> result payload: required)?;
    register_command!(cr, async fn cyc_b(state, context) -> result payload: required)?;

    let envref = env.to_ref();
    let asset = envref
        .evaluate_immediately("/-/cyc_a", TestPayload::new("quinn", 8))
        .await?;
    let result = asset.get().await?.try_into_string()?;

    // The cycle must be reported, not hang and not silently succeed.
    assert!(
        result.contains("cycle") || result.contains("Cycle"),
        "expected a cycle to be reported, got: {}",
        result
    );
    Ok(())
}

// ============================================================================
// I8 / C3: inline manager parity and deep nesting
// ============================================================================

/// The inline (Wasm-compatible) manager inherits payload with the same semantics.
/// This is what `ImmediateEnvironmentWithPayload` exists for.
#[tokio::test]
async fn test_inline_manager_payload_inheritance() -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = InlineEnv;
    let mut env = InlineEnv::new();

    async fn parent(
        _s: State<Value>,
        user_id: UserId,
        context: Context<InlineEnv>,
    ) -> Result<Value, Error> {
        let q = parse_query("/-/child")?;
        let asset = context.evaluate(&q).await?;
        Ok(Value::from(format!(
            "parent:{}|child:{}",
            user_id.0,
            asset.get().await?.try_into_string()?
        )))
    }
    fn child(_s: &State<Value>, window_id: WindowId) -> Result<Value, Error> {
        Ok(Value::from(format!("window:{}", window_id.0)))
    }

    let cr = &mut env.command_registry;
    register_command!(cr, async fn parent(state, user_id: UserId injected, context) -> result
        payload: required)?;
    register_command!(cr, fn child(state, window_id: WindowId injected) -> result
        payload: required)?;

    let envref = env.to_ref();
    let asset = envref
        .evaluate_immediately("/-/parent", TestPayload::new("rosa", 99))
        .await?;
    assert_eq!(
        asset.get().await?.try_into_string()?,
        "parent:rosa|child:window:99"
    );
    Ok(())
}

/// Payload survives three levels of nesting unchanged.
#[tokio::test]
async fn test_deep_nesting_payload_propagation() -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = QueuedEnv;
    let mut env = QueuedEnv::new();

    async fn l1(_s: State<Value>, context: Context<QueuedEnv>) -> Result<Value, Error> {
        let a = context.evaluate(&parse_query("/-/l2")?).await?;
        Ok(Value::from(format!(
            "l1>{}",
            a.get().await?.try_into_string()?
        )))
    }
    async fn l2(_s: State<Value>, context: Context<QueuedEnv>) -> Result<Value, Error> {
        let a = context.evaluate(&parse_query("/-/l3")?).await?;
        Ok(Value::from(format!(
            "l2>{}",
            a.get().await?.try_into_string()?
        )))
    }
    fn l3(_s: &State<Value>, user_id: UserId, window_id: WindowId) -> Result<Value, Error> {
        Ok(Value::from(format!("l3:{}:{}", user_id.0, window_id.0)))
    }

    let cr = &mut env.command_registry;
    register_command!(cr, async fn l1(state, context) -> result payload: required)?;
    register_command!(cr, async fn l2(state, context) -> result payload: required)?;
    register_command!(cr, fn l3(state, user_id: UserId injected, window_id: WindowId injected)
        -> result payload: required)?;

    let envref = env.to_ref();
    let asset = envref
        .evaluate_immediately("/-/l1", TestPayload::new("sven", 3))
        .await?;
    assert_eq!(asset.get().await?.try_into_string()?, "l1>l2>l3:sven:3");
    Ok(())
}

// ============================================================================
// C1: concurrent evaluations with distinct payloads must not share
// ============================================================================

/// Because a payload requirement implies volatility, each evaluation resolves to a fresh,
/// unshared asset. Two concurrent evaluations of the same query with different payloads must
/// each see their own.
#[tokio::test]
async fn test_concurrent_payloads_do_not_share() -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = QueuedEnv;
    let mut env = QueuedEnv::new();

    fn who(_s: &State<Value>, user_id: UserId, window_id: WindowId) -> Result<Value, Error> {
        Ok(Value::from(format!("{}:{}", user_id.0, window_id.0)))
    }

    let cr = &mut env.command_registry;
    register_command!(cr, fn who(state, user_id: UserId injected, window_id: WindowId injected)
        -> result payload: required)?;

    let envref = env.to_ref();
    let (a, b) = tokio::join!(
        envref.evaluate_immediately("/-/who", TestPayload::new("tom", 1)),
        envref.evaluate_immediately("/-/who", TestPayload::new("uma", 2)),
    );
    assert_eq!(a?.get().await?.try_into_string()?, "tom:1");
    assert_eq!(b?.get().await?.try_into_string()?, "uma:2");
    Ok(())
}

// ============================================================================
// C2: payload cloning shares interior state
// ============================================================================

/// The payload is cloned per action, so large data belongs behind an `Arc`. All actions in a
/// chain must observe the same underlying allocation.
#[tokio::test]
async fn test_payload_clone_shares_interior_state() -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = QueuedEnv;
    let mut env = QueuedEnv::new();

    fn note(_s: &State<Value>, context: Context<QueuedEnv>) -> Result<Value, Error> {
        let payload = context
            .get_payload_clone()
            .ok_or_else(|| Error::general_error("no payload".to_string()))?;
        let mut log = payload
            .log
            .lock()
            .map_err(|e| Error::general_error(format!("lock poisoned: {e}")))?;
        log.push("noted".to_string());
        Ok(Value::from(log.len() as i64))
    }

    let cr = &mut env.command_registry;
    register_command!(cr, fn note(state, context) -> result payload: required)?;

    let envref = env.to_ref();
    let payload = TestPayload::new("vic", 0);
    let shared = payload.log.clone();
    envref
        .evaluate_immediately("/-/note", payload)
        .await?
        .get()
        .await?;

    let count = shared
        .lock()
        .map_err(|e| Error::general_error(format!("lock poisoned: {e}")))?
        .len();
    assert_eq!(
        count, 1,
        "the caller's Arc must observe the command's write"
    );
    Ok(())
}

// ============================================================================
// Plan-level enforcement of `payload: required` (PR #14 review, comment 3)
// ============================================================================

/// A command declared `payload: required` must not run without a payload, even when nothing
/// forces the issue: no injected argument, and the body tolerates a missing payload.
///
/// The per-entry-point checks in `Context` only cover nested scheduling, so before the
/// plan-level gate this command ran happily through the top-level payload-free
/// `EnvRef::evaluate`, contradicting its own declaration.
#[tokio::test]
async fn test_toplevel_required_payload_is_enforced() -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = QueuedEnv;
    let mut env = QueuedEnv::new();

    // No injected argument, and absence of a payload is handled rather than propagated.
    fn tolerant(_s: &State<Value>, context: Context<QueuedEnv>) -> Result<Value, Error> {
        match context.get_payload_clone() {
            Some(p) => Ok(Value::from(format!("payload:{}", p.window_id))),
            None => Ok(Value::from("RAN_WITHOUT_PAYLOAD")),
        }
    }

    let cr = &mut env.command_registry;
    register_command!(cr, fn tolerant(state, context) -> result payload: required)?;

    let envref = env.to_ref();
    let asset = envref.evaluate("/-/tolerant").await?;
    let state = asset.get().await?;
    let err = state.value_state().err().ok_or_else(|| {
        Error::general_error(
            "a payload-required command must not run without a payload".to_string(),
        )
    })?;

    assert!(
        err.to_string().contains("requires an evaluation payload"),
        "expected the payload requirement to be enforced, got: {}",
        err
    );
    Ok(())
}

/// The same command with a payload supplied runs normally — the gate rejects only the
/// genuinely payload-free case.
#[tokio::test]
async fn test_toplevel_required_payload_runs_when_supplied(
) -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = QueuedEnv;
    let mut env = QueuedEnv::new();

    fn tolerant(_s: &State<Value>, context: Context<QueuedEnv>) -> Result<Value, Error> {
        match context.get_payload_clone() {
            Some(p) => Ok(Value::from(format!("payload:{}", p.window_id))),
            None => Ok(Value::from("RAN_WITHOUT_PAYLOAD")),
        }
    }

    let cr = &mut env.command_registry;
    register_command!(cr, fn tolerant(state, context) -> result payload: required)?;

    let envref = env.to_ref();
    let asset = envref
        .evaluate_immediately("/-/tolerant", TestPayload::new("wendy", 11))
        .await?;
    assert_eq!(asset.get().await?.try_into_string()?, "payload:11");
    Ok(())
}

/// Sibling evaluations of the same payload-requiring query are concurrent branches, not an
/// ancestor cycle (PR #14 review, comment 2). The path is copied per branch rather than
/// shared, so one sibling never sees the other's entry.
#[tokio::test]
async fn test_sibling_payload_evaluations_are_not_a_cycle() -> Result<(), Box<dyn std::error::Error>>
{
    type CommandEnvironment = QueuedEnv;
    let mut env = QueuedEnv::new();

    async fn parent(_s: State<Value>, context: Context<QueuedEnv>) -> Result<Value, Error> {
        let q = parse_query("/-/leaf")?;
        let a = context.evaluate(&q).await?;
        let b = context.evaluate(&q).await?;
        Ok(Value::from(format!(
            "a={} b={}",
            a.get().await?.try_into_string()?,
            b.get().await?.try_into_string()?
        )))
    }
    fn leaf(_s: &State<Value>, window_id: WindowId) -> Result<Value, Error> {
        Ok(Value::from(format!("w{}", window_id.0)))
    }

    let cr = &mut env.command_registry;
    register_command!(cr, async fn parent(state, context) -> result payload: required)?;
    register_command!(cr, fn leaf(state, window_id: WindowId injected) -> result
        payload: required)?;

    let envref = env.to_ref();
    let asset = envref
        .evaluate_immediately("/-/parent", TestPayload::new("xena", 5))
        .await?;
    assert_eq!(asset.get().await?.try_into_string()?, "a=w5 b=w5");
    Ok(())
}

// ============================================================================
// Payload requirement recorded on the evaluated asset
// (ASSET-PAYLOAD-REQUIREMENT-NOT-RECORDED, evaluate-path-consolidation Step 1)
// ============================================================================

/// The plan has known its payload requirement since `PlanBuilder` ran, but until this change
/// nothing carried it to the asset: every evaluated asset reported `None`, including one that
/// could not have run without a payload.
#[tokio::test]
async fn payload_requirement_is_recorded_in_metadata_and_asset_info(
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

    assert_eq!(
        asset.payload_required().await,
        PayloadRequirement::Required,
        "an asset whose plan declared `payload: required` must record it"
    );
    assert_eq!(
        asset.get_asset_info().await?.payload_required,
        PayloadRequirement::Required,
        "the requirement must reach AssetInfo, which is what a client sees"
    );
    Ok(())
}

/// Reproducibility follows the *requirement*, not the presence of a payload. A plain query
/// evaluated through `evaluate_immediately` has a payload in scope that no command consumes; it
/// must still report `None`, or every payload-carrying evaluation would look non-reproducible.
#[tokio::test]
async fn payload_supplied_but_not_required_records_none() -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = QueuedEnv;
    let mut env = QueuedEnv::new();

    fn plain(_state: &State<Value>) -> Result<Value, Error> {
        Ok(Value::from("no payload needed".to_string()))
    }

    let cr = &mut env.command_registry;
    register_command!(cr, fn plain(state) -> result)?;

    let envref = env.to_ref();
    let asset = envref
        .evaluate_immediately("/-/plain", TestPayload::new("bob", 7))
        .await?;
    assert_eq!(asset.get().await?.try_into_string()?, "no payload needed");

    assert_eq!(
        asset.payload_required().await,
        PayloadRequirement::None,
        "a plan that needs no payload records None even when a payload was supplied"
    );
    assert_eq!(
        asset.get_asset_info().await?.payload_required,
        PayloadRequirement::None
    );
    Ok(())
}

/// Keys are a payload boundary, and the boundary is now named rather than merely unreachable.
///
/// Resolving a key on the payload path would hand back the map-registered asset and run it with a
/// payload, leaving a payload-evaluated value in the key map for the next caller — who would
/// receive it without supplying one. Unreachable at HEAD, because a pure key query reports
/// `PayloadRequirement::None`, so the branch was dead code that silently contradicted the
/// invariant. It now returns an error that says why.
#[tokio::test]
async fn keyed_query_cannot_be_evaluated_with_a_payload() -> Result<(), Box<dyn std::error::Error>>
{
    use liquers_core::assets::AssetManager;

    type CommandEnvironment = QueuedEnv;
    let mut env = QueuedEnv::new();

    fn plain(_state: &State<Value>) -> Result<Value, Error> {
        Ok(Value::from("v".to_string()))
    }
    let cr = &mut env.command_registry;
    register_command!(cr, fn plain(state) -> result)?;

    let envref = env.to_ref();
    let manager = envref.get_asset_manager();
    let parent = manager
        .apply(Recipe::from(parse_query("plain")?), State::new(), None)
        .await?;

    let key_query = parse_query("-R/some/resource.txt")?;
    let err = match manager
        .get_dependency_asset_with_payload(
            &parent,
            &key_query,
            Some(TestPayload::new("carol", 3)),
            Vec::new(),
        )
        .await
    {
        Ok(_) => panic!("a keyed query must not be evaluated with a payload"),
        Err(e) => e,
    };

    assert!(
        err.to_string().contains("payload does not cross a key boundary"),
        "expected the boundary to be named, got: {err}"
    );
    Ok(())
}

/// Only `evaluate(None)` produces an asset the manager may hand out again. An asset evaluated
/// with a payload is in no map, so a second request cannot receive it.
///
/// Asserted against the maps rather than inferred from the volatility flag: the invariant is a
/// property of the payload, and the chain that currently guarantees it (payload implies volatile
/// implies unmapped) would break silently if a command were registered `payload: required`
/// without `volatile`.
#[tokio::test]
async fn payload_evaluated_asset_is_in_no_map() -> Result<(), Box<dyn std::error::Error>> {
    use liquers_core::assets::AssetManager;

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
    let query = parse_query("needs_payload")?;
    let asset = envref
        .evaluate_immediately("/-/needs_payload", TestPayload::new("dave", 9))
        .await?;
    assert_eq!(asset.get().await?.try_into_string()?, "window:9");

    let manager = envref.get_asset_manager();
    let first_id = asset.id();

    // Not in the key map — asserted directly.
    assert!(
        manager.lookup_key_asset(&parse_key("needs_payload")?).is_none(),
        "a payload-evaluated asset must not be reachable through the key map"
    );

    // Not reusable through the query map either — asserted by its observable consequence, since
    // there is no public query-map accessor to read: a second request must not be handed the
    // payload-evaluated asset. It gets a fresh one, which then fails for want of a payload.
    let second = envref.evaluate(&query).await?;
    assert_ne!(
        second.id(),
        first_id,
        "a payload-evaluated asset must never be handed out again"
    );
    assert!(
        second.get().await.and_then(|s| s.value_state()).is_err(),
        "the fresh asset requires a payload and must fail without one"
    );
    Ok(())
}
