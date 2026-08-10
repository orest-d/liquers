//! Manager-parametric suite (async-wasm-refactor M-D).
//!
//! The same `AssetManager` trait contract is exercised over BOTH implementations —
//! `DefaultAssetManager` (via `SimpleEnvironment`, queued) and `ImmediateAssetManager` (via
//! `ImmediateEnvironment`, inline) — proving b1's manager is swappable behind the trait and
//! that `ImmediateAssetManager` evaluates correctly at runtime. Plus immediate-only checks:
//! concurrency dedup and the no-tokio-runtime proof (browser-readiness on native).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use liquers_core::{
    assets::AssetManager,
    command_metadata::CommandKey,
    context::{Environment, EnvRef, ImmediateEnvironment, SimpleEnvironment},
    error::Error,
    metadata::{Metadata, Status},
    parse::parse_key,
    query::{Key, Query, TryToQuery},
    recipes::DefaultRecipeProvider,
    state::State,
    store::{AsyncMemoryStore, AsyncStore},
    value::Value,
};

fn q(s: &str) -> Query {
    s.try_to_query().expect("query parse")
}

// --- generic scenario bodies (written once, run against both managers) ---

async fn scenario_basic_eval<E>(envref: EnvRef<E>) -> Result<(), Error>
where
    E: Environment<Value = Value>,
{
    let asset = envref.get_asset_manager().get_asset(&q("greet")).await?;
    let state = asset.get().await?;
    assert_eq!(state.status(), Status::Ready);
    assert_eq!(state.try_into_string()?, "hello");
    Ok(())
}

/// `eval_mode()` reports the manager's constant, and a second `get_asset` of the same query
/// returns a finished asset (cache path).
async fn scenario_cache_and_mode<E>(envref: EnvRef<E>) -> Result<(), Error>
where
    E: Environment<Value = Value>,
{
    let m = envref.get_asset_manager();
    let a1 = m.get_asset(&q("greet")).await?;
    assert!(a1.get().await?.status().is_finished());
    let a2 = m.get_asset(&q("greet")).await?;
    assert_eq!(a2.get().await?.try_into_string()?, "hello");
    Ok(())
}

fn register_greet<E>(cr: &mut liquers_core::commands::CommandRegistry<E>)
where
    E: Environment<Value = Value>,
{
    cr.register_command(
        CommandKey::new_name("greet"),
        |_state, _args, _ctx| -> Result<Value, Error> { Ok(Value::from("hello")) },
    )
    .expect("register greet");
}

// --- keyed scenarios (keyed-recipe-ownership) ---
//
// Every scenario above is non-keyed, which is exactly why
// `CORE-IMMEDIATE-MANAGER-KEYED-RECURSION` survived: a keyed query under the inline manager
// recursed until the stack was exhausted, and nothing here went down that path.

/// Store holding a `recipes.yaml` that maps `dash.txt` to `greet`.
async fn recipe_store() -> Result<AsyncMemoryStore, Error> {
    let store = AsyncMemoryStore::new(&Key::new());
    store
        .set(
            &parse_key("recipes.yaml")?,
            b"recipes:\n  - query: greet/dash.txt\n",
            &Metadata::new(),
        )
        .await?;
    Ok(store)
}

async fn stored_text_store(include_recipe: bool) -> Result<AsyncMemoryStore, Error> {
    let key = parse_key("stored.txt")?;
    let store = AsyncMemoryStore::new(&Key::new());
    store
        .set(
            &key,
            b"from store",
            &Metadata::MetadataRecord(
                liquers_core::metadata::MetadataRecord::new()
                    .with_key(key.clone())
                    .with_type_identifier("text".to_owned())
                    .with_status(Status::Source)
                    .clone(),
            ),
        )
        .await?;
    if include_recipe {
        store
            .set(
                &parse_key("recipes.yaml")?,
                b"recipes:\n  - query: counted/stored.txt\n",
                &Metadata::new(),
            )
            .await?;
    }
    Ok(store)
}

async fn scenario_stored_value<E>(envref: EnvRef<E>, calls: Arc<AtomicUsize>) -> Result<(), Error>
where
    E: Environment<Value = Value>,
{
    let asset = envref
        .get_asset_manager()
        .get(&parse_key("stored.txt")?)
        .await?;
    assert_eq!(asset.get().await?.try_into_string()?, "from store");
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "an eligible stored value must fast-track without running its recipe"
    );
    Ok(())
}

/// Keyed evaluation through a stored recipe.
///
/// Under `ImmediateAssetManager` this is the recursion reproducer: `evaluate_recipe` used to
/// ask `AssetManager::get` who owned `dash.txt` while it *was* that asset, and `get` runs an
/// unfinished asset inline.
async fn scenario_keyed_eval<E>(envref: EnvRef<E>) -> Result<(), Error>
where
    E: Environment<Value = Value>,
{
    let asset = envref
        .get_asset_manager()
        .get(&parse_key("dash.txt")?)
        .await?;
    let state = asset.get().await?;
    assert_eq!(state.try_into_string()?, "hello");
    Ok(())
}

/// An asset holding a key recipe it does not own takes the **delegation** branch rather than
/// evaluating the recipe itself.
///
/// This pins branch *selection*, which is what the ownership test controls: a fix that turned
/// every case into self-evaluation would silently double-evaluate shared keys, and this test
/// is what catches that. The counter proves it: the recipe runs once, for the owner.
///
/// **The branch's outcome is a separate, pre-existing defect.** Delegation always fails with
/// a spurious `DependencyCycle`, and cannot do otherwise: `owned_key_asset` is queried with
/// the key from *this* asset's own recipe, so the delegate is always registered under this
/// asset's own key, and `record_dependency_on_asset` compares those two keys and sees a
/// self-edge. See `ASSET-KEYED-DELEGATION-ALWAYS-CYCLES`. This test asserts the current broken
/// outcome so that fixing it fails loudly here — the same arrangement that
/// `test_volatile_keyed_recipe_cycles_preexisting_defect` used before it was inverted.
///
/// `apply` builds the untracked asset: it constructs one from the recipe it is given and runs
/// it without registering it, so a bare key recipe reaches `evaluate_recipe` with an id the
/// key map does not hold.
async fn scenario_keyed_delegation<E>(
    envref: EnvRef<E>,
    calls: Arc<AtomicUsize>,
) -> Result<(), Error>
where
    E: Environment<Value = Value>,
{
    let key = parse_key("dash.txt")?;
    let manager = envref.get_asset_manager();

    let owner = manager.get(&key).await?;
    assert_eq!(owner.get().await?.try_into_string()?, "counted");
    assert_eq!(calls.load(Ordering::SeqCst), 1, "precondition: evaluated once");

    // The failure surfaces at different points on the two managers, and both are correct:
    // the inline manager runs the asset during `apply` and returns the error there, while the
    // queued manager submits and the error appears when the value is read.
    let err = match manager.apply((&key).into(), State::new()).await {
        Ok(adhoc) => {
            assert_ne!(adhoc.id(), owner.id(), "precondition: a different asset");
            match adhoc.get().await {
                Ok(state) => match state.value_state() {
                    Ok(_) => panic!(
                        "delegation unexpectedly produced a value - \
                         ASSET-KEYED-DELEGATION-ALWAYS-CYCLES may have been fixed; invert this \
                         test to assert the owner's value and a still-unchanged call count"
                    ),
                    Err(e) => e,
                },
                Err(e) => e,
            }
        }
        Err(e) => e,
    };
    assert!(
        err.to_string().contains("Dependency cycle"),
        "expected the known self-edge cycle from the delegation branch, got: {}",
        err
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "whatever delegation does with the value, it must not re-run the recipe"
    );
    Ok(())
}

/// Environment with the `dash.txt` recipe mapped to a counting command.
async fn counted_recipe_store() -> Result<AsyncMemoryStore, Error> {
    let store = AsyncMemoryStore::new(&Key::new());
    store
        .set(
            &parse_key("recipes.yaml")?,
            b"recipes:\n  - query: counted/dash.txt\n",
            &Metadata::new(),
        )
        .await?;
    Ok(store)
}

fn register_counted<E>(
    cr: &mut liquers_core::commands::CommandRegistry<E>,
    calls: Arc<AtomicUsize>,
) where
    E: Environment<Value = Value>,
{
    cr.register_command(
        CommandKey::new_name("counted"),
        move |_state, _args, _ctx| -> Result<Value, Error> {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(Value::from("counted"))
        },
    )
    .expect("register counted");
}

/// Store holding a `recipes.yaml` that maps `vol.txt` to a volatile command.
async fn volatile_recipe_store() -> Result<AsyncMemoryStore, Error> {
    let store = AsyncMemoryStore::new(&Key::new());
    store
        .set(
            &parse_key("recipes.yaml")?,
            b"recipes:\n  - query: vol_cmd/vol.txt\n",
            &Metadata::new(),
        )
        .await?;
    Ok(store)
}

fn register_vol_cmd<E>(cr: &mut liquers_core::commands::CommandRegistry<E>)
where
    E: Environment<Value = Value>,
{
    cr.register_command(
        CommandKey::new_name("vol_cmd"),
        |_state, _args, _ctx| -> Result<Value, Error> { Ok(Value::from("vol")) },
    )
    .expect("register vol_cmd")
    .volatile = true;
}

/// A keyed recipe whose command is `volatile: true` evaluates rather than delegating to
/// itself — `VOLATILE-KEYED-RECIPE-SELF-DELEGATION`, on the inline manager.
///
/// The queued counterpart lives in `payload_inheritance.rs`.
async fn scenario_volatile_keyed_eval<E>(envref: EnvRef<E>) -> Result<(), Error>
where
    E: Environment<Value = Value>,
{
    let asset = envref
        .get_asset_manager()
        .get(&parse_key("vol.txt")?)
        .await?;
    let state = asset.get().await?;
    assert_eq!(
        state
            .value_state()
            .map_err(|e| Error::general_error(format!(
                "volatile keyed recipe should evaluate, got: {e}"
            )))?
            .try_into_string()?,
        "vol"
    );
    Ok(())
}

// --- Default manager (queued) ---

#[tokio::test]
async fn basic_eval_default() -> Result<(), Error> {
    let mut env = SimpleEnvironment::<Value>::new();
    register_greet(&mut env.command_registry);
    scenario_basic_eval(env.to_ref()).await
}

#[tokio::test]
async fn cache_and_mode_default() -> Result<(), Error> {
    let mut env = SimpleEnvironment::<Value>::new();
    register_greet(&mut env.command_registry);
    let envref = env.to_ref();
    assert_eq!(
        envref.get_asset_manager().eval_mode(),
        liquers_core::assets::EvalMode::Queued
    );
    scenario_cache_and_mode(envref).await
}

// --- Immediate manager (inline) ---

#[tokio::test]
async fn basic_eval_immediate() -> Result<(), Error> {
    let mut env = ImmediateEnvironment::<Value>::new();
    register_greet(&mut env.command_registry);
    scenario_basic_eval(env.to_ref()).await
}

#[tokio::test]
async fn cache_and_mode_immediate() -> Result<(), Error> {
    let mut env = ImmediateEnvironment::<Value>::new();
    register_greet(&mut env.command_registry);
    let envref = env.to_ref();
    assert_eq!(
        envref.get_asset_manager().eval_mode(),
        liquers_core::assets::EvalMode::Inline
    );
    scenario_cache_and_mode(envref).await
}

// --- keyed, both managers (keyed-recipe-ownership) ---

#[tokio::test]
async fn keyed_eval_default() -> Result<(), Error> {
    let mut env = SimpleEnvironment::<Value>::new();
    register_greet(&mut env.command_registry);
    env.with_async_store(Box::new(recipe_store().await?));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    scenario_keyed_eval(env.to_ref()).await
}

#[tokio::test]
async fn keyed_eval_immediate() -> Result<(), Error> {
    let mut env = ImmediateEnvironment::<Value>::new();
    register_greet(&mut env.command_registry);
    env.with_async_store(Box::new(recipe_store().await?));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    scenario_keyed_eval(env.to_ref()).await
}

#[tokio::test]
async fn stored_value_precedes_recipe_default() -> Result<(), Error> {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut env = SimpleEnvironment::<Value>::new();
    register_counted(&mut env.command_registry, calls.clone());
    env.with_async_store(Box::new(stored_text_store(true).await?));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    scenario_stored_value(env.to_ref(), calls).await
}

#[tokio::test]
async fn stored_value_precedes_recipe_immediate() -> Result<(), Error> {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut env = ImmediateEnvironment::<Value>::new();
    register_counted(&mut env.command_registry, calls.clone());
    env.with_async_store(Box::new(stored_text_store(true).await?));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    scenario_stored_value(env.to_ref(), calls).await
}

#[tokio::test]
async fn plain_stored_value_default() -> Result<(), Error> {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut env = SimpleEnvironment::<Value>::new();
    env.with_async_store(Box::new(stored_text_store(false).await?));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    scenario_stored_value(env.to_ref(), calls).await
}

#[tokio::test]
async fn plain_stored_value_immediate() -> Result<(), Error> {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut env = ImmediateEnvironment::<Value>::new();
    env.with_async_store(Box::new(stored_text_store(false).await?));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    scenario_stored_value(env.to_ref(), calls).await
}

#[tokio::test]
async fn keyed_delegation_default() -> Result<(), Error> {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut env = SimpleEnvironment::<Value>::new();
    register_counted(&mut env.command_registry, calls.clone());
    env.with_async_store(Box::new(counted_recipe_store().await?));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    scenario_keyed_delegation(env.to_ref(), calls).await
}

#[tokio::test]
async fn keyed_delegation_immediate() -> Result<(), Error> {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut env = ImmediateEnvironment::<Value>::new();
    register_counted(&mut env.command_registry, calls.clone());
    env.with_async_store(Box::new(counted_recipe_store().await?));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    scenario_keyed_delegation(env.to_ref(), calls).await
}

#[tokio::test]
async fn volatile_keyed_eval_immediate() -> Result<(), Error> {
    let mut env = ImmediateEnvironment::<Value>::new();
    register_vol_cmd(&mut env.command_registry);
    env.with_async_store(Box::new(volatile_recipe_store().await?));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    scenario_volatile_keyed_eval(env.to_ref()).await
}

// --- immediate-only ---

/// Two concurrent `get_asset` for the same query share one evaluation (the command body runs once).
#[tokio::test]
async fn immediate_concurrent_same_query_runs_once() -> Result<(), Error> {
    static COUNT: AtomicUsize = AtomicUsize::new(0);
    let mut env = ImmediateEnvironment::<Value>::new();
    env.command_registry
        .register_command(
            CommandKey::new_name("counted"),
            |_state, _args, _ctx| -> Result<Value, Error> {
                COUNT.fetch_add(1, Ordering::SeqCst);
                Ok(Value::from("x"))
            },
        )
        .expect("register");
    let envref = env.to_ref();
    let m = envref.get_asset_manager();
    let query = q("counted");
    let (a, b) = futures::join!(m.get_asset(&query), m.get_asset(&query));
    a?.get().await?;
    b?.get().await?;
    assert_eq!(COUNT.load(Ordering::SeqCst), 1, "command body must run once");
    Ok(())
}

/// **No-tokio-runtime proof.** The immediate path runs under `futures::executor::block_on`
/// with NO tokio runtime present. A reintroduced `tokio::spawn` on the inline path would panic
/// here ("no reactor running") — green means browser-ready. (Non-keyed query ⇒ no persistence.)
#[test]
fn immediate_runs_without_tokio_runtime() -> Result<(), Error> {
    let mut env = ImmediateEnvironment::<Value>::new();
    register_greet(&mut env.command_registry);
    let envref: EnvRef<ImmediateEnvironment<Value>> = env.to_ref();

    let text: String = futures::executor::block_on(async move {
        let asset = envref.get_asset_manager().get_asset(&q("greet")).await?;
        let state = asset.get().await?;
        state.try_into_string()
    })?;
    assert_eq!(text, "hello");
    Ok(())
}

/// The same proof for a **keyed** query, which the non-keyed one above cannot give.
///
/// A keyed asset persists, and persistence is where a `tokio::spawn` would most plausibly be
/// reintroduced — `persist_with_status_tracking` spawns for background saves and only stays
/// synchronous because it checks for `EvalMode::Inline`. Green here means that check still
/// holds; a regression panics with "no reactor running" rather than failing quietly in a
/// browser where there is no reactor to find.
#[test]
fn immediate_keyed_eval_without_tokio_runtime() -> Result<(), Error> {
    let mut env = ImmediateEnvironment::<Value>::new();
    register_greet(&mut env.command_registry);
    let store = futures::executor::block_on(recipe_store())?;
    env.with_async_store(Box::new(store));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    let envref: EnvRef<ImmediateEnvironment<Value>> = env.to_ref();

    let text: String = futures::executor::block_on(async move {
        let asset = envref
            .get_asset_manager()
            .get(&parse_key("dash.txt")?)
            .await?;
        let state = asset.get().await?;
        state.try_into_string()
    })?;
    assert_eq!(text, "hello");
    Ok(())
}
