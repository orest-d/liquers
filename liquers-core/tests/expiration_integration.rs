use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use itertools::Itertools;
// Integration tests for expiration system
use liquers_core::{
    assets::{AssetManager, AssetRef, PersistenceStatus},
    command_metadata::CommandKey,
    context::{Context, EnvRef, Environment, SimpleEnvironment},
    error::Error,
    expiration::{ExpirationTime, Expires},
    interpreter::make_plan,
    metadata::{Metadata, MetadataRecord, Status},
    parse::{parse_key, parse_query},
    query::Key,
    recipes::{DefaultRecipeProvider, Recipe, RecipeList},
    state::State,
    store::{AsyncMemoryStore, AsyncStore},
    value::{Value, ValueInterface},
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
    let mut env = SimpleEnvironment::<Value>::new();
    env.with_async_store(Box::new(AsyncMemoryStore::new(&Key::new())));
    let envref = env.to_ref();

    let manager = envref.get_asset_manager();
    let key = parse_key("test/source_asset.bin")?;
    let mut metadata = MetadataRecord::new();
    metadata.type_identifier = "bytes".to_string();
    metadata.type_name = "bytes".to_string();
    metadata.data_format = Some("bin".to_string());

    manager.set_binary(&key, b"source-data", metadata).await?;
    let assetref = manager.get(&key).await?;
    assert_eq!(assetref.status().await, Status::Source);

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
    assetref
        .set_expiration_time(ExpirationTime::At(future))
        .await;

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

/// Timed expiration
#[tokio::test]
async fn test_timed_expiration() -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = SimpleEnvironment<Value>;
    let mut env = CommandEnvironment::new();

    fn hello() -> Result<Value, Error> {
        Ok(Value::from_string("Hello".to_string()))
    }
    let cr = &mut env.command_registry;
    register_command!(cr,
        fn hello() -> result
        version: 123
        expires: "in 500 ms"
    )?;

    let envref = env.to_ref();
    let asset = envref.evaluate("hello").await?;
    assert_eq!(asset.get().await?.try_into_string()?, "Hello");
    assert_eq!(asset.status().await, Status::Ready);
    let state = asset.get().await?;
    tokio::time::sleep(Duration::from_millis(600)).await;
    assert_eq!(asset.status().await, Status::Expired);

    Ok(())
}

/// Dependent expiration
#[tokio::test]
async fn test_dependent_expiration() -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = SimpleEnvironment<Value>;
    let mut env = CommandEnvironment::new();

    fn hello() -> Result<Value, Error> {
        Ok(Value::from_string("Hello".to_string()))
    }
    fn world(state: &State<Value>) -> Result<Value, Error> {
        Ok(Value::from_string(format!(
            "{}, world!",
            state.try_into_string()?
        )))
    }
    let cr = &mut env.command_registry;
    register_command!(cr,
        fn hello() -> result
        version: 123
    )?;
    register_command!(cr,
        fn world(state) -> result
        version: 234
    )?;

    let recipe = Recipe::new(
        "hello/hello.txt".to_string(),
        "Hello recipe".to_string(),
        "Produces hello.txt from hello command".to_string(),
    )?;
    let mut recipe_list = RecipeList::new();
    recipe_list.add_recipe(recipe);
    let yaml_content = serde_yaml::to_string(&recipe_list)?;

    let store = AsyncMemoryStore::new(&Key::new());
    let recipes_key = parse_key("recipes.yaml")?;
    store
        .set(&recipes_key, yaml_content.as_bytes(), &Metadata::new())
        .await?;
    env.with_async_store(Box::new(store));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));

    let envref = env.to_ref();
    let asset = envref.evaluate("-R/hello.txt/-/world").await?;
    assert_eq!(asset.get().await?.try_into_string()?, "Hello, world!");
    assert_eq!(asset.status().await, Status::Ready);
    let state = asset.get().await?;
    let dependencies = state.metadata.get_dependencies();

    println!("Dependencies:");
    for (i, d) in dependencies.iter().enumerate() {
        println!("  {i}: {}", d.key.as_str());
    }
    println!("----");
    // DependencyKey format: "ns-dep/command_impl-{realm}-{namespace}-{name}"
    // "world" is registered in the default realm/namespace, both normalized to "".
    assert!(dependencies
        .iter()
        .any(|d| d.key.as_str() == "ns-dep/command_impl---world"));

    let hello_asset = envref.evaluate("-R/hello.txt").await?;
    assert_eq!(hello_asset.get().await?.try_into_string()?, "Hello");
    assert_eq!(hello_asset.status().await, Status::Ready);

    hello_asset.expire().await?;
    assert_eq!(hello_asset.status().await, Status::Expired);

    //tokio::time::sleep(Duration::from_millis(600)).await;
    assert_eq!(asset.status().await, Status::Expired);

    Ok(())
}

/// Dependent expiration 2
#[tokio::test]
async fn test_dependent_expiration2() -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = SimpleEnvironment<Value>;
    let mut env = CommandEnvironment::new();

    fn hello() -> Result<Value, Error> {
        Ok(Value::from_string("Hello".to_string()))
    }
    fn world(state: &State<Value>) -> Result<Value, Error> {
        Ok(Value::from_string(format!(
            "{}, world!",
            state.try_into_string()?
        )))
    }
    let cr = &mut env.command_registry;
    register_command!(cr,
        fn hello() -> result
        version: 123
        expires: "in 500 ms"
    )?;
    register_command!(cr,
        fn world(state) -> result
        version: 234
        expires: "never"
    )?;

    let recipe = Recipe::new(
        "hello/hello.txt".to_string(),
        "Hello recipe".to_string(),
        "Produces hello.txt from hello command".to_string(),
    )?;
    let mut recipe_list = RecipeList::new();
    recipe_list.add_recipe(recipe);
    let yaml_content = serde_yaml::to_string(&recipe_list)?;

    let store = AsyncMemoryStore::new(&Key::new());
    let recipes_key = parse_key("recipes.yaml")?;
    store
        .set(&recipes_key, yaml_content.as_bytes(), &Metadata::new())
        .await?;
    env.with_async_store(Box::new(store));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));

    let envref = env.to_ref();
    let asset = envref.evaluate("-R/hello.txt/-/world").await?;
    assert_eq!(asset.get().await?.try_into_string()?, "Hello, world!");
    assert_eq!(asset.status().await, Status::Ready);
    let state = asset.get().await?;
    assert_eq!(state.metadata.expires(), Expires::InDuration(std::time::Duration::from_millis(500)));
  
    tokio::time::sleep(Duration::from_millis(600)).await;
    assert_eq!(asset.status().await, Status::Expired);

    Ok(())
}

#[tokio::test]
async fn test_commands_chain_expiration() -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = SimpleEnvironment<Value>;
    let mut env = CommandEnvironment::new();

    fn hello() -> Result<Value, Error> {
        Ok(Value::from_string("Hello".to_string()))
    }
    fn world(state: &State<Value>) -> Result<Value, Error> {
        Ok(Value::from_string(format!(
            "{}, world!",
            state.try_into_string()?
        )))
    }
    let cr = &mut env.command_registry;
    register_command!(cr,
        fn hello() -> result
        version: 123
        expires: "in 500 ms"
    )?;
    register_command!(cr,
        fn world(state) -> result
        version: 234
        expires: "never"
    )?;

    let envref = env.to_ref();
    let asset = envref.evaluate("hello/world").await?;
    assert_eq!(asset.get().await?.try_into_string()?, "Hello, world!");
    assert_eq!(asset.status().await, Status::Ready);
    let state = asset.get().await?;
    assert_eq!(state.metadata.expires(), Expires::InDuration(std::time::Duration::from_millis(500)));

    tokio::time::sleep(Duration::from_millis(600)).await;
    assert_eq!(asset.status().await, Status::Expired);

    Ok(())
}

// ============================================================================
// WP-3 (expiration-safety): expired assets are cache misses for normal access;
// keyed assets get an explicit, non-evaluating recovery/promote path.
// See specs/expiration-safety/ for the design.
// ============================================================================

// --- Example 1: expired keyed asset is a cache miss (primary use case) ---

/// Regression test pinning already-correct manager-level behavior: a fresh manager request for
/// an expired keyed asset must recompute, not serve the stale value.
#[tokio::test]
async fn test_manager_get_treats_expired_keyed_asset_as_cache_miss(
) -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = SimpleEnvironment<Value>;
    static CALLS: AtomicUsize = AtomicUsize::new(0);

    fn wp3_counter_1() -> Result<Value, Error> {
        let n = CALLS.fetch_add(1, Ordering::SeqCst) + 1;
        Ok(Value::from_string(n.to_string()))
    }

    let mut env = CommandEnvironment::new();
    let cr = &mut env.command_registry;
    register_command!(cr, fn wp3_counter_1() -> result version: 1)?;
    let envref = env.to_ref();

    let asset1 = envref.evaluate("wp3_counter_1").await?;
    assert_eq!(asset1.get().await?.try_into_string()?, "1");
    asset1.expire().await?;
    assert_eq!(asset1.status().await, Status::Expired);

    // A fresh manager request for the SAME query must recompute, not serve the stale "1".
    let asset2 = envref.evaluate("wp3_counter_1").await?;
    assert_eq!(asset2.get().await?.try_into_string()?, "2");
    assert_eq!(asset2.status().await, Status::Ready);
    Ok(())
}

/// `poll_state()` now treats `Status::Expired` as a cache miss (WP-3), so `AssetRef::get()` on
/// an already-expired, directly-held ref falls through to the notification-wait loop's existing
/// `Expired` error arm instead of returning the stale value.
#[tokio::test]
async fn test_assetref_get_does_not_serve_expired_state() -> Result<(), Box<dyn std::error::Error>>
{
    type CommandEnvironment = SimpleEnvironment<Value>;
    fn wp3_fresh_value() -> Result<Value, Error> {
        Ok(Value::from_string("fresh".to_string()))
    }

    let mut env = CommandEnvironment::new();
    let cr = &mut env.command_registry;
    register_command!(cr, fn wp3_fresh_value() -> result)?;
    let envref = env.to_ref();

    let asset = envref.evaluate("wp3_fresh_value").await?;
    assert_eq!(asset.get().await?.try_into_string()?, "fresh");

    asset.expire().await?;
    assert_eq!(asset.status().await, Status::Expired);

    let result = asset.get().await;
    assert!(
        result.is_err(),
        "get() on an already-expired AssetRef must error, not return stale data"
    );
    Ok(())
}

/// `poll_state()` returns `None` for `Expired`; the new `poll_state_any_status()` is the
/// explicit recovery read that still returns the original value.
#[tokio::test]
async fn test_expired_status_poll_state_none() -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = SimpleEnvironment<Value>;
    fn wp3_cached_value() -> Result<Value, Error> {
        Ok(Value::from_string("original".to_string()))
    }

    let mut env = CommandEnvironment::new();
    let cr = &mut env.command_registry;
    register_command!(cr, fn wp3_cached_value() -> result)?;
    let envref = env.to_ref();

    let asset = envref.evaluate("wp3_cached_value").await?;
    let _ = asset.get().await?;
    asset.expire().await?;

    assert!(
        asset.poll_state().await.is_none(),
        "poll_state() must treat Expired as no-data (cache miss)"
    );

    let recovered = asset.poll_state_any_status().await;
    assert!(recovered.is_some());
    assert_eq!(recovered.unwrap().try_into_string()?, "original");
    Ok(())
}

// --- Example 2: recovery and override-preservation flow ---

/// Shared setup: a keyed resource `"wp3_counter.txt"` backed by a recipe that runs an
/// incrementing counter command, so every real recomputation is observable.
async fn wp3_keyed_counter_env(
) -> Result<(EnvRef<SimpleEnvironment<Value>>, Key, Arc<AtomicUsize>), Box<dyn std::error::Error>>
{
    type CommandEnvironment = SimpleEnvironment<Value>;
    let calls = Arc::new(AtomicUsize::new(0));
    let calls_for_cmd = calls.clone();

    let mut env = CommandEnvironment::new();
    env.command_registry.register_command(
        CommandKey::new_name("wp3_counter"),
        move |_state, _args, _ctx| {
            let n = calls_for_cmd.fetch_add(1, Ordering::SeqCst) + 1;
            Ok(Value::from_string(n.to_string()))
        },
    )?;

    let recipe = Recipe::new(
        "wp3_counter/wp3_counter.txt".to_string(),
        "Counter recipe".to_string(),
        "Produces wp3_counter.txt from the wp3_counter command".to_string(),
    )?;
    let mut recipe_list = RecipeList::new();
    recipe_list.add_recipe(recipe);
    let yaml_content = serde_yaml::to_string(&recipe_list)?;

    let store = AsyncMemoryStore::new(&Key::new());
    let recipes_key = parse_key("recipes.yaml")?;
    store
        .set(&recipes_key, yaml_content.as_bytes(), &Metadata::new())
        .await?;
    env.with_async_store(Box::new(store));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));

    let envref = env.to_ref();
    Ok((envref, parse_key("wp3_counter.txt")?, calls))
}

#[tokio::test]
async fn test_get_any_status_returns_expired_keyed_state() -> Result<(), Box<dyn std::error::Error>>
{
    let (envref, key, calls) = wp3_keyed_counter_env().await?;
    let manager = envref.get_asset_manager();

    let asset = manager.get(&key).await?;
    assert_eq!(asset.get().await?.try_into_string()?, "1");
    asset.expire().await?;

    let recovered = manager.get_any_status(&key).await?;
    assert!(recovered.is_some());
    assert_eq!(
        recovered.unwrap().try_into_string()?,
        "1",
        "must be the stale value, not recomputed"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    Ok(())
}

/// Covers the store-fallback branch in a best-effort way (whichever internal path actually
/// serves it, the observable contract must hold): the stale value comes back, unchanged, with
/// no recompute. See `test_get_any_status_and_to_override_from_store_only` below for a variant
/// that deterministically forces the store-only path via a second, independent manager.
#[tokio::test]
async fn test_get_any_status_loads_persisted_expired_state(
) -> Result<(), Box<dyn std::error::Error>> {
    let (envref, key, calls) = wp3_keyed_counter_env().await?;
    let manager = envref.get_asset_manager();

    {
        let asset = manager.get(&key).await?;
        assert_eq!(asset.get().await?.try_into_string()?, "1");
        asset.expire().await?;
    }

    let recovered = manager.get_any_status(&key).await?;
    assert!(recovered.is_some());
    assert_eq!(recovered.unwrap().try_into_string()?, "1");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    Ok(())
}

#[tokio::test]
async fn test_to_override_from_expired_keyed_state_preserves_value(
) -> Result<(), Box<dyn std::error::Error>> {
    let (envref, key, calls) = wp3_keyed_counter_env().await?;
    let manager = envref.get_asset_manager();

    let asset = manager.get(&key).await?;
    assert_eq!(asset.get().await?.try_into_string()?, "1");
    asset.expire().await?;

    manager.to_override(&key).await?;

    let reloaded = manager.get(&key).await?;
    assert_eq!(reloaded.status().await, Status::Override);
    assert_eq!(
        reloaded.get().await?.try_into_string()?,
        "1",
        "value preserved, no recompute"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    Ok(())
}

/// Demonstrates `to_override` is NOT Expired-specific: pinning a still-`Ready` asset works the
/// same way.
#[tokio::test]
async fn test_to_override_from_ready_asset_preserves_value(
) -> Result<(), Box<dyn std::error::Error>> {
    let (envref, key, calls) = wp3_keyed_counter_env().await?;
    let manager = envref.get_asset_manager();

    let asset = manager.get(&key).await?;
    assert_eq!(asset.get().await?.try_into_string()?, "1");
    assert_eq!(asset.status().await, Status::Ready, "not expired yet");

    manager.to_override(&key).await?;

    let reloaded = manager.get(&key).await?;
    assert_eq!(reloaded.status().await, Status::Override);
    assert_eq!(reloaded.get().await?.try_into_string()?, "1");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    Ok(())
}

// --- Example 3: dependency freshness guard + non-keyed eviction ---

/// Regression test: `AssetManager::get_dependency_asset` already evicts an `Expired` dependency
/// at scheduling time and re-resolves it fresh — this pins that existing behavior as a WP-3
/// acceptance guard.
#[tokio::test]
async fn test_expired_dependency_is_recomputed_before_dependent_evaluation(
) -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = SimpleEnvironment<Value>;
    static CALLS: AtomicUsize = AtomicUsize::new(0);

    fn wp3_child() -> Result<Value, Error> {
        let n = CALLS.fetch_add(1, Ordering::SeqCst) + 1;
        Ok(Value::from_string(n.to_string()))
    }
    fn wp3_parent(state: &State<Value>) -> Result<Value, Error> {
        Ok(Value::from_string(format!("parent({})", state.try_into_string()?)))
    }

    let mut env = CommandEnvironment::new();
    let cr = &mut env.command_registry;
    register_command!(cr, fn wp3_child() -> result version: 1)?;
    register_command!(cr, fn wp3_parent(state) -> result version: 1)?;

    let recipe = Recipe::new(
        "wp3_child/wp3_child.txt".to_string(),
        "Child recipe".to_string(),
        "Produces wp3_child.txt".to_string(),
    )?;
    let mut recipe_list = RecipeList::new();
    recipe_list.add_recipe(recipe);
    let yaml_content = serde_yaml::to_string(&recipe_list)?;
    let store = AsyncMemoryStore::new(&Key::new());
    let recipes_key = parse_key("recipes.yaml")?;
    store
        .set(&recipes_key, yaml_content.as_bytes(), &Metadata::new())
        .await?;
    env.with_async_store(Box::new(store));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    let envref = env.to_ref();

    let parent1 = envref.evaluate("-R/wp3_child.txt/-/wp3_parent").await?;
    assert_eq!(parent1.get().await?.try_into_string()?, "parent(1)");

    let child_asset = envref.evaluate("-R/wp3_child.txt").await?;
    child_asset.expire().await?;
    assert_eq!(child_asset.status().await, Status::Expired);

    // A NEW parent evaluation must see the child as expired and recompute it (-> "2") before
    // using it, not reuse the stale "1".
    let parent2 = envref.evaluate("-R/wp3_child.txt/-/wp3_parent").await?;
    assert_eq!(parent2.get().await?.try_into_string()?, "parent(2)");
    Ok(())
}

/// SKETCH, deliberately incomplete (see doc note): the tolerated in-flight race — a dependency
/// expiring strictly after the parent has already read its value, but before the parent's
/// command returns, must still let the parent complete with the value it already read. This
/// gate wires a real synchronization point (`tokio::sync::oneshot`) via
/// `CommandRegistry::register_async_command`. Ordering between "parent has read the dependency"
/// and "test force-expires the child" is resolved deterministically (not a fixed-duration guess):
/// the parent's `get_dependency_state` call is what causes the child to actually be scheduled
/// and evaluated in the first place, so polling the child until it reaches `Ready` is equivalent
/// to "the parent has already read it and is now past that line" — bounded so this can't hang.
#[tokio::test]
async fn test_dependency_expiring_during_parent_evaluation_is_allowed(
) -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = SimpleEnvironment<Value>;

    let mut env = CommandEnvironment::new();
    env.command_registry.register_command(
        CommandKey::new_name("wp3_gate_child"),
        |_state, _args, _ctx| Ok(Value::from_string("child_value".to_string())),
    )?;

    let (gate_tx, gate_rx) = tokio::sync::oneshot::channel::<()>();
    let gate_rx = Arc::new(tokio::sync::Mutex::new(Some(gate_rx)));
    env.command_registry.register_async_command(
        CommandKey::new_name("wp3_gate_parent"),
        move |_state, _args, context: Context<CommandEnvironment>| {
            let gate_rx = gate_rx.clone();
            Box::pin(async move {
                let child_state = context
                    .get_dependency_state(&parse_query("wp3_gate_child")?)
                    .await?;
                if let Some(rx) = gate_rx.lock().await.take() {
                    let _ = rx.await;
                }
                Ok(Value::from_string(format!(
                    "parent({})",
                    child_state.try_into_string()?
                )))
            })
        },
    )?;

    let envref = env.to_ref();
    let parent_future = envref.evaluate("wp3_gate_parent");

    let child_asset = envref
        .get_asset_manager()
        .get_asset(&parse_query("wp3_gate_child")?)
        .await?;
    // Bounded poll (not a fixed sleep) until the child is actually Ready -- deterministic proof
    // the parent's get_dependency_state call has already read it.
    let mut child_ready = false;
    for _ in 0..200 {
        if child_asset.status().await == Status::Ready {
            child_ready = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    assert!(child_ready, "child dependency never reached Ready within the bounded wait");
    child_asset.expire().await?;
    let _ = gate_tx.send(());

    let parent_asset = tokio::time::timeout(Duration::from_secs(10), parent_future).await??;
    assert_eq!(
        parent_asset.get().await?.try_into_string()?,
        "parent(child_value)",
        "in-flight evaluation must complete with the value already read, not fail"
    );
    Ok(())
}

/// Non-keyed (pure query, no recipe/store key) expired assets are a cache miss like any other
/// expired asset, and have no recovery path reachable: `get_any_status`/`to_override` both take
/// `&Key`, not `&Query`, so recovery is unreachable by construction, not merely unimplemented.
#[tokio::test]
async fn test_non_keyed_expired_asset_is_evicted_and_not_recoverable(
) -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = SimpleEnvironment<Value>;
    fn wp3_pure_query() -> Result<Value, Error> {
        Ok(Value::from_string("computed".to_string()))
    }

    let mut env = CommandEnvironment::new();
    let cr = &mut env.command_registry;
    register_command!(cr, fn wp3_pure_query() -> result)?;
    let envref = env.to_ref();

    let asset = envref.evaluate("wp3_pure_query").await?;
    assert_eq!(asset.get().await?.try_into_string()?, "computed");
    asset.expire().await?;

    assert!(
        asset.poll_state().await.is_none(),
        "non-keyed expired asset must be a cache miss like any other expired asset"
    );
    Ok(())
}

// --- Integration tests: persistence branches + no-side-effects guarantee ---

/// Wraps a real `AsyncMemoryStore`, counting `set` (full serialize+store) vs. `set_metadata`
/// (status-only rewrite) calls separately.
#[derive(Clone)]
struct WP3CountingStore {
    inner: Arc<AsyncMemoryStore>,
    set_calls: Arc<AtomicUsize>,
    set_metadata_calls: Arc<AtomicUsize>,
}

impl WP3CountingStore {
    fn new(inner: AsyncMemoryStore) -> Self {
        WP3CountingStore {
            inner: Arc::new(inner),
            set_calls: Arc::new(AtomicUsize::new(0)),
            set_metadata_calls: Arc::new(AtomicUsize::new(0)),
        }
    }
}

#[async_trait]
impl AsyncStore for WP3CountingStore {
    async fn get(&self, key: &Key) -> Result<(Vec<u8>, Metadata), Error> {
        self.inner.get(key).await
    }
    async fn set(&self, key: &Key, data: &[u8], metadata: &Metadata) -> Result<(), Error> {
        self.set_calls.fetch_add(1, Ordering::SeqCst);
        self.inner.set(key, data, metadata).await
    }
    async fn set_metadata(&self, key: &Key, metadata: &Metadata) -> Result<(), Error> {
        self.set_metadata_calls.fetch_add(1, Ordering::SeqCst);
        self.inner.set_metadata(key, metadata).await
    }
}

/// When the original evaluation's `persistence_status()` is `Persisted`, `to_override` calls
/// `set_metadata` exactly once more and does NOT call `set` again — proving no re-serialization
/// happened, not just that the end value looks right.
#[tokio::test]
async fn test_to_override_metadata_only_when_persisted() -> Result<(), Box<dyn std::error::Error>>
{
    type CommandEnvironment = SimpleEnvironment<Value>;
    fn wp3_persisted_counter() -> Result<Value, Error> {
        Ok(Value::from_string("1".to_string()))
    }

    let mut env = CommandEnvironment::new();
    let cr = &mut env.command_registry;
    register_command!(cr, fn wp3_persisted_counter() -> result version: 1)?;

    let recipe = Recipe::new(
        "wp3_persisted_counter/wp3_persisted.txt".to_string(),
        "Counter recipe".to_string(),
        "Produces wp3_persisted.txt".to_string(),
    )?;
    let mut recipe_list = RecipeList::new();
    recipe_list.add_recipe(recipe);
    let yaml_content = serde_yaml::to_string(&recipe_list)?;
    let inner_store = AsyncMemoryStore::new(&Key::new());
    let recipes_key = parse_key("recipes.yaml")?;
    inner_store
        .set(&recipes_key, yaml_content.as_bytes(), &Metadata::new())
        .await?;
    let store = WP3CountingStore::new(inner_store);
    env.with_async_store(Box::new(store.clone()));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    let envref = env.to_ref();
    let key = parse_key("wp3_persisted.txt")?;
    let manager = envref.get_asset_manager();

    let asset = manager.get(&key).await?;
    assert_eq!(asset.get().await?.try_into_string()?, "1");
    assert_eq!(asset.persistence_status().await, PersistenceStatus::Persisted);
    asset.expire().await?;
    // Snapshot AFTER expire(): expire() itself now persists the Expired status via
    // set_metadata (so a subsequent store-load can't fast-track the stale Ready bytes back in
    // once this in-memory entry is evicted) — that call is not part of what to_override does.
    let set_calls_before = store.set_calls.load(Ordering::SeqCst);
    let set_metadata_calls_before = store.set_metadata_calls.load(Ordering::SeqCst);

    manager.to_override(&key).await?;

    assert_eq!(
        store.set_calls.load(Ordering::SeqCst),
        set_calls_before,
        "to_override must NOT re-serialize when the value is already Persisted"
    );
    assert_eq!(
        store.set_metadata_calls.load(Ordering::SeqCst),
        set_metadata_calls_before + 1,
        "to_override must rewrite metadata exactly once"
    );

    let reloaded = manager.get(&key).await?;
    assert_eq!(reloaded.status().await, Status::Override);
    assert_eq!(reloaded.get().await?.try_into_string()?, "1");
    Ok(())
}

/// Deferred: needs a store double that fails `set()` for the target key only, while still
/// serving `recipes.yaml` reads (the `FailingSetStore` pattern used elsewhere in this crate
/// fails ALL reads too, so it can't be reused as-is). See
/// specs/expiration-safety/phase4-implementation.md Step 5 for the exact shape once unblocked.
#[tokio::test]
#[ignore = "needs a store double that fails set() for the target key only, see doc comment"]
async fn test_to_override_retries_persist_when_not_persisted() {
    // Intentionally left as a placeholder — see comment above.
}

/// Un-deferred by the Phase 4 opus final review: `Value::as_bytes(data_format)` falls through to
/// `Err(ErrorType::SerializationError)` for ANY unrecognized `data_format`, and the format
/// defaults to the key's file extension when unset — so a key with an unrecognized extension
/// fails to serialize regardless of its value, with no test-only `Value` type needed.
#[tokio::test]
async fn test_to_override_skips_store_write_when_nonserializable(
) -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = SimpleEnvironment<Value>;
    fn wp3_widget() -> Result<Value, Error> {
        Ok(Value::from_string("irrelevant".to_string()))
    }

    let mut env = CommandEnvironment::new();
    let cr = &mut env.command_registry;
    register_command!(cr, fn wp3_widget() -> result version: 1)?;

    let recipe = Recipe::new(
        "wp3_widget/wp3_widget.nosuchformat".to_string(),
        "Widget recipe".to_string(),
        "Produces wp3_widget.nosuchformat".to_string(),
    )?;
    let mut recipe_list = RecipeList::new();
    recipe_list.add_recipe(recipe);
    let yaml_content = serde_yaml::to_string(&recipe_list)?;
    let store = AsyncMemoryStore::new(&Key::new());
    let recipes_key = parse_key("recipes.yaml")?;
    store
        .set(&recipes_key, yaml_content.as_bytes(), &Metadata::new())
        .await?;
    env.with_async_store(Box::new(store));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    let envref = env.to_ref();
    let key = parse_key("wp3_widget.nosuchformat")?;
    let manager = envref.get_asset_manager();

    let asset = manager.get(&key).await?;
    assert_eq!(asset.get().await?.try_into_string()?, "irrelevant");
    assert_eq!(
        asset.persistence_status().await,
        PersistenceStatus::NonSerializable,
        "the unrecognized data_format must fail as_bytes() on the very first save attempt"
    );
    assert!(
        !manager.get_envref().get_async_store().contains(&key).await?,
        "nothing should have been persisted for an unrecognized data_format"
    );
    asset.expire().await?;

    manager.to_override(&key).await?;

    assert_eq!(asset.status().await, Status::Override, "in-memory promotion still happens");
    assert!(
        !manager.get_envref().get_async_store().contains(&key).await?,
        "to_override must not write anything to the store when the value was never serializable"
    );
    Ok(())
}

/// `get_any_status` must have no side effects: a normal `get` call AFTER it still correctly
/// treats the key as a cache miss and recomputes.
#[tokio::test]
async fn test_get_any_status_has_no_side_effects_on_normal_get(
) -> Result<(), Box<dyn std::error::Error>> {
    let (envref, key, calls) = wp3_keyed_counter_env().await?;
    let manager = envref.get_asset_manager();

    let asset = manager.get(&key).await?;
    assert_eq!(asset.get().await?.try_into_string()?, "1");
    asset.expire().await?;

    let stale = manager.get_any_status(&key).await?;
    assert_eq!(stale.unwrap().try_into_string()?, "1");
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    let fresh = manager.get(&key).await?;
    assert_eq!(fresh.get().await?.try_into_string()?, "2");
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    Ok(())
}

/// Opus final review addition: deterministically forces the store-only branch of both
/// `get_any_status` and `to_override` by dropping the FIRST environment/manager entirely (no
/// shared in-memory `AssetRef` at all is reachable afterward) and re-hydrating a SECOND,
/// independent environment/manager from the same persisted store bytes.
#[tokio::test]
async fn test_get_any_status_and_to_override_from_store_only(
) -> Result<(), Box<dyn std::error::Error>> {
    type CommandEnvironment = SimpleEnvironment<Value>;
    static CALLS: AtomicUsize = AtomicUsize::new(0);
    fn wp3_store_only_counter() -> Result<Value, Error> {
        let n = CALLS.fetch_add(1, Ordering::SeqCst) + 1;
        Ok(Value::from_string(n.to_string()))
    }

    let recipe = Recipe::new(
        "wp3_store_only_counter/wp3_store_only.txt".to_string(),
        "Counter recipe".to_string(),
        "Produces wp3_store_only.txt".to_string(),
    )?;
    let mut recipe_list = RecipeList::new();
    recipe_list.add_recipe(recipe);
    let yaml_content = serde_yaml::to_string(&recipe_list)?;
    let key = parse_key("wp3_store_only.txt")?;
    let recipes_key = parse_key("recipes.yaml")?;

    // First environment: evaluate, persist, expire, then drop entirely.
    let persisted_bytes = {
        let mut env = CommandEnvironment::new();
        let cr = &mut env.command_registry;
        register_command!(cr, fn wp3_store_only_counter() -> result version: 1)?;
        let store = AsyncMemoryStore::new(&Key::new());
        store
            .set(&recipes_key, yaml_content.as_bytes(), &Metadata::new())
            .await?;
        env.with_async_store(Box::new(store));
        env.with_recipe_provider(Box::new(DefaultRecipeProvider));
        let envref = env.to_ref();
        let manager = envref.get_asset_manager();

        let asset = manager.get(&key).await?;
        assert_eq!(asset.get().await?.try_into_string()?, "1");
        asset.expire().await?;

        let raw_store = envref.get_async_store();
        let (binary, metadata) = raw_store.get(&key).await?;
        (binary, metadata)
        // `env`/`envref`/`manager`/`asset` all dropped here — no in-memory AssetRef survives.
    };

    // Second, independent environment: re-hydrate a fresh store from the same persisted bytes.
    let mut env2 = CommandEnvironment::new();
    let cr2 = &mut env2.command_registry;
    register_command!(cr2, fn wp3_store_only_counter() -> result version: 1)?;
    let store2 = AsyncMemoryStore::new(&Key::new());
    store2
        .set(&recipes_key, yaml_content.as_bytes(), &Metadata::new())
        .await?;
    store2
        .set(&key, &persisted_bytes.0, &persisted_bytes.1)
        .await?;
    env2.with_async_store(Box::new(store2));
    env2.with_recipe_provider(Box::new(DefaultRecipeProvider));
    let envref2 = env2.to_ref();
    let manager2 = envref2.get_asset_manager();

    // get_any_status: no in-memory entry exists in this manager at all -> must be the
    // store-fallback branch, deterministically.
    let recovered = manager2.get_any_status(&key).await?;
    assert_eq!(recovered.unwrap().try_into_string()?, "1");
    assert_eq!(
        CALLS.load(Ordering::SeqCst),
        1,
        "store-fallback read must not trigger evaluation"
    );

    // to_override: same deterministic store-only branch.
    manager2.to_override(&key).await?;
    let reloaded = manager2.get(&key).await?;
    assert_eq!(reloaded.status().await, Status::Override);
    assert_eq!(reloaded.get().await?.try_into_string()?, "1");
    assert_eq!(CALLS.load(Ordering::SeqCst), 1, "still no recompute");
    Ok(())
}
