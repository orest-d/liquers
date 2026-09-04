//! Manager-parametric suite (async-wasm-refactor M-D).
//!
//! The same `AssetManager` trait contract is exercised over BOTH implementations —
//! `DefaultAssetManager` (via `SimpleEnvironment`, queued) and `ImmediateAssetManager` (via
//! `ImmediateEnvironment`, inline) — proving b1's manager is swappable behind the trait and
//! that `ImmediateAssetManager` evaluates correctly at runtime. Plus immediate-only checks:
//! concurrency dedup and the no-tokio-runtime proof (browser-readiness on native).

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use liquers_core::{
    assets::AssetManager,
    command_metadata::CommandKey,
    context::{EnvRef, Environment, ImmediateEnvironment, SimpleEnvironment},
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
                    .with_type_identifier("Text".to_owned())
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
/// evaluating the recipe itself, and that branch hands it the owner's value.
///
/// Two contracts are pinned here, and both assertions are load-bearing:
///
/// - **Branch selection**, which the ownership test controls (`specs/design/keyed-recipe-ownership/`).
///   A change that turned every case into self-evaluation would still produce `"counted"` — the
///   recipe genuinely computes it — so the *counter* is what catches it. A shared key must be
///   computed once, not once per reader.
/// - **The hand-off itself** (`specs/design/keyed-delegation-hand-off/`). Delegation used to fail
///   unconditionally with a spurious `DependencyCycle`: `owned_key_asset` is queried with the key
///   from *this* asset's own recipe, so the delegate is always registered under this asset's own
///   key, and `record_dependency_on_asset` saw a self-edge. Two assets sharing a key are one
///   dependency-graph node, so nothing is recorded and the wait proceeds
///   (`ASSET-KEYED-DELEGATION-ALWAYS-CYCLES`).
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
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "precondition: evaluated once"
    );

    let adhoc = manager.apply((&key).into(), State::new(), None).await?;
    // Without this the test could pass trivially: were `apply` ever to return the registered
    // owner, the delegation branch would never be entered at all.
    assert_ne!(adhoc.id(), owner.id(), "precondition: a different asset");

    assert_eq!(
        adhoc.get().await?.try_into_string()?,
        "counted",
        "delegation must hand the owner's value to the delegating asset"
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "the hand-off takes the owner's value; it must not re-run the recipe"
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

struct CountingStore {
    inner: AsyncMemoryStore,
    value_writes: Arc<AtomicUsize>,
}

#[async_trait]
impl AsyncStore for CountingStore {
    fn store_name(&self) -> String {
        self.inner.store_name()
    }

    fn key_prefix(&self) -> Key {
        self.inner.key_prefix()
    }

    async fn get(&self, key: &Key) -> Result<(Vec<u8>, Metadata), Error> {
        self.inner.get(key).await
    }

    async fn set(&self, key: &Key, data: &[u8], metadata: &Metadata) -> Result<(), Error> {
        self.value_writes.fetch_add(1, Ordering::SeqCst);
        self.inner.set(key, data, metadata).await
    }

    async fn set_metadata(&self, key: &Key, metadata: &Metadata) -> Result<(), Error> {
        self.inner.set_metadata(key, metadata).await
    }

    async fn contains(&self, key: &Key) -> Result<bool, Error> {
        self.inner.contains(key).await
    }

    async fn is_dir(&self, key: &Key) -> Result<bool, Error> {
        self.inner.is_dir(key).await
    }

    async fn listdir(&self, key: &Key) -> Result<Vec<String>, Error> {
        self.inner.listdir(key).await
    }

    fn is_supported(&self, key: &Key) -> bool {
        self.inner.is_supported(key)
    }
}

async fn counting_recipe_store() -> Result<(CountingStore, Arc<AtomicUsize>), Error> {
    let value_writes = Arc::new(AtomicUsize::new(0));
    Ok((
        CountingStore {
            inner: counted_recipe_store().await?,
            value_writes: value_writes.clone(),
        },
        value_writes,
    ))
}

fn register_counted<E>(cr: &mut liquers_core::commands::CommandRegistry<E>, calls: Arc<AtomicUsize>)
where
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
    let (store, value_writes) = counting_recipe_store().await?;
    let mut env = SimpleEnvironment::<Value>::new();
    register_counted(&mut env.command_registry, calls.clone());
    env.with_async_store(Box::new(store));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    scenario_keyed_delegation(env.to_ref(), calls).await?;
    assert_eq!(value_writes.load(Ordering::SeqCst), 1);
    Ok(())
}

#[tokio::test]
async fn keyed_delegation_immediate() -> Result<(), Error> {
    let calls = Arc::new(AtomicUsize::new(0));
    let (store, value_writes) = counting_recipe_store().await?;
    let mut env = ImmediateEnvironment::<Value>::new();
    register_counted(&mut env.command_registry, calls.clone());
    env.with_async_store(Box::new(store));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    scenario_keyed_delegation(env.to_ref(), calls).await?;
    assert_eq!(value_writes.load(Ordering::SeqCst), 1);
    Ok(())
}

#[tokio::test]
async fn volatile_keyed_eval_immediate() -> Result<(), Error> {
    let mut env = ImmediateEnvironment::<Value>::new();
    register_vol_cmd(&mut env.command_registry);
    env.with_async_store(Box::new(volatile_recipe_store().await?));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    scenario_volatile_keyed_eval(env.to_ref()).await
}

// ============================================================================
// The recorded key: is this a keyed asset?
// (evaluate-path-consolidation Step 2)
//
// A keyed asset is an asset associated with a key, and it knows so from the moment it is
// constructed. Everything that used to re-derive the answer — where to write, what to
// invalidate, whether two assets are the same node — reads this one field instead.
// ============================================================================

/// A keyed asset records its key; a query asset records none. The keyed case includes the
/// **volatile** one, which the manager deliberately never registers — a map-derived predicate
/// reports `None` for it and would silently stop it being stored.
async fn scenario_keyed_asset_records_its_key<E>(envref: EnvRef<E>) -> Result<(), Error>
where
    E: Environment<Value = Value>,
{
    let m = envref.get_asset_manager();

    let key = parse_key("dash.txt")?;
    let keyed = m.get(&key).await?;
    keyed.get().await?;
    assert_eq!(
        keyed.key().await,
        Some(key.clone()),
        "an asset created for a key must record it"
    );

    let query_asset = m.get_asset(&q("greet")).await?;
    query_asset.get().await?;
    assert_eq!(
        query_asset.key().await,
        None,
        "a non-keyed query asset owns no key and must never be stored"
    );

    // The distinction must be visible in metadata too: a keyed asset and a non-keyed query asset
    // built from the same query are not the same thing, and their states must differ.
    let info = keyed.get_asset_info().await?;
    assert_eq!(info.key, Some(key), "the key must reach AssetInfo");
    Ok(())
}

/// An ad-hoc `apply` asset is not keyed, even when its recipe is shaped like a key. This is the
/// durable half of `CONTEXT-APPLY-BARE-KEY-ILL-DEFINED`: not keyed means it can never be stored.
async fn scenario_adhoc_apply_is_not_keyed<E>(envref: EnvRef<E>) -> Result<(), Error>
where
    E: Environment<Value = Value>,
{
    let m = envref.get_asset_manager();
    let bare_key_recipe: liquers_core::recipes::Recipe = parse_key("dash.txt")?.into();
    let applied = m
        .apply(bare_key_recipe, State::new().with_data("ignored".into()), None)
        .await?;
    let _ = applied.get().await;
    assert_eq!(
        applied.key().await,
        None,
        "an ad-hoc apply asset owns nothing, even with a key-shaped recipe"
    );
    Ok(())
}

#[tokio::test]
async fn keyed_asset_records_its_key_default() -> Result<(), Error> {
    let mut env = SimpleEnvironment::<Value>::new();
    register_greet(&mut env.command_registry);
    env.with_async_store(Box::new(recipe_store().await?));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    scenario_keyed_asset_records_its_key(env.to_ref()).await
}

#[tokio::test]
async fn keyed_asset_records_its_key_immediate() -> Result<(), Error> {
    let mut env = ImmediateEnvironment::<Value>::new();
    register_greet(&mut env.command_registry);
    env.with_async_store(Box::new(recipe_store().await?));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    scenario_keyed_asset_records_its_key(env.to_ref()).await
}

#[tokio::test]
async fn volatile_keyed_asset_records_its_key_default() -> Result<(), Error> {
    let mut env = SimpleEnvironment::<Value>::new();
    register_vol_cmd(&mut env.command_registry);
    env.with_async_store(Box::new(volatile_recipe_store().await?));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    let envref = env.to_ref();
    let key = parse_key("vol.txt")?;
    let asset = envref.get_asset_manager().get(&key).await?;
    asset.get().await?;
    assert_eq!(
        asset.key().await,
        Some(key),
        "a volatile keyed asset is keyed — it is merely never registered, which is a \
         caching decision, not an identity one"
    );
    Ok(())
}

#[tokio::test]
async fn volatile_keyed_asset_records_its_key_immediate() -> Result<(), Error> {
    let mut env = ImmediateEnvironment::<Value>::new();
    register_vol_cmd(&mut env.command_registry);
    env.with_async_store(Box::new(volatile_recipe_store().await?));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    let envref = env.to_ref();
    let key = parse_key("vol.txt")?;
    let asset = envref.get_asset_manager().get(&key).await?;
    asset.get().await?;
    assert_eq!(asset.key().await, Some(key));
    Ok(())
}

#[tokio::test]
async fn adhoc_apply_is_not_keyed_default() -> Result<(), Error> {
    let mut env = SimpleEnvironment::<Value>::new();
    register_greet(&mut env.command_registry);
    env.with_async_store(Box::new(recipe_store().await?));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    scenario_adhoc_apply_is_not_keyed(env.to_ref()).await
}

#[tokio::test]
async fn adhoc_apply_is_not_keyed_immediate() -> Result<(), Error> {
    let mut env = ImmediateEnvironment::<Value>::new();
    register_greet(&mut env.command_registry);
    env.with_async_store(Box::new(recipe_store().await?));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    scenario_adhoc_apply_is_not_keyed(env.to_ref()).await
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
    assert_eq!(
        COUNT.load(Ordering::SeqCst),
        1,
        "command body must run once"
    );
    Ok(())
}

// ============================================================================
// Entry-point equivalence
// (evaluate-path-consolidation — the point of the whole design)
// ============================================================================

/// The same recipe through three entry points produces the same facts.
///
/// What must match is everything `evaluate` produces: the value, the recorded dependencies, the
/// type identifier, and the payload requirement. What legitimately differs is decided at
/// *construction* — whether the asset is keyed, hence whether it is stored and reusable.
///
/// The literal **status sequence** is deliberately not asserted: a queued keyed asset passes
/// through `Submitted` and an inline one never does, because scheduling is manager policy, not a
/// property of the evaluation. Asserting it would produce a test that cannot pass, and "fixing"
/// that would mean weakening the real invariant.
async fn scenario_entry_point_equivalence<E>(envref: EnvRef<E>) -> Result<(), Error>
where
    E: Environment<Value = Value>,
{
    let m = envref.get_asset_manager();
    let query = q("dependent");

    // 1. through the query map
    let via_get = m.get_asset(&query).await?;
    let s1 = via_get.get().await?;

    // 2. as an ad-hoc apply, no payload
    let via_apply = m
        .apply(liquers_core::recipes::Recipe::from(query.clone()), State::new(), None)
        .await?;
    let s2 = via_apply.get().await?;

    assert_eq!(s1.try_into_string()?, s2.try_into_string()?, "same value");

    let md1 = via_get.get_metadata().await?;
    let md2 = via_apply.get_metadata().await?;
    let deps1 = md1.get_dependencies().to_vec();
    let deps2 = md2.get_dependencies().to_vec();
    assert_eq!(
        deps1.len(),
        deps2.len(),
        "dependency recording must not depend on the entry point — this is the asymmetry \
         CORE-EVALUATE-PATH-CONSOLIDATION names"
    );
    assert!(!deps1.is_empty(), "the fixture must actually record a dependency");
    assert_eq!(
        deps1.iter().map(|d| d.key.clone()).collect::<Vec<_>>(),
        deps2.iter().map(|d| d.key.clone()).collect::<Vec<_>>(),
        "the same dependencies, in the same order"
    );

    assert_eq!(md1.type_identifier()?, md2.type_identifier()?);
    assert_eq!(md1.payload_required(), md2.payload_required());

    // The legitimate difference: neither is keyed here (a plain query), so neither is stored.
    assert_eq!(via_get.key().await, None);
    assert_eq!(via_apply.key().await, None);
    assert!(via_get.status().await.is_finished());
    assert!(via_apply.status().await.is_finished());
    Ok(())
}

fn register_dependent<E>(cr: &mut liquers_core::commands::CommandRegistry<E>)
where
    E: Environment<Value = Value>,
{
    cr.register_async_command(CommandKey::new_name("dependent"), |_state, _args, ctx| {
        Box::pin(async move {
            let dep = ctx
                .get_dependency_state(&q("greet"))
                .await?
                .try_into_string()?;
            Ok(Value::from(format!("dependent:{dep}")))
        })
    })
    .expect("register dependent");
}

#[tokio::test]
async fn entry_point_equivalence_default() -> Result<(), Error> {
    let mut env = SimpleEnvironment::<Value>::new();
    register_greet(&mut env.command_registry);
    register_dependent(&mut env.command_registry);
    env.with_async_store(Box::new(recipe_store().await?));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    scenario_entry_point_equivalence(env.to_ref()).await
}

#[tokio::test]
async fn entry_point_equivalence_immediate() -> Result<(), Error> {
    let mut env = ImmediateEnvironment::<Value>::new();
    register_greet(&mut env.command_registry);
    register_dependent(&mut env.command_registry);
    env.with_async_store(Box::new(recipe_store().await?));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    scenario_entry_point_equivalence(env.to_ref()).await
}

/// Execute-once on the inline path, with a command that actually yields.
///
/// `immediate_concurrent_same_query_runs_once` above has the right shape but cannot expose the
/// gap: its command is a synchronous closure with no `.await`, so the first evaluation always
/// finishes before the second caller is polled. A command that yields opens the window that
/// `run_with_future_inline`'s `is_finished()`-only guard leaves — two callers both observe "not
/// finished" and both run the body (`INLINE-PATH-LACKS-EXECUTE-ONCE`).
///
/// Two `get_asset` calls, not two `apply` calls: each `apply` builds a separate ad-hoc asset and
/// would legitimately run twice. Execute-once is about two callers converging on one mapped asset.
#[tokio::test]
async fn immediate_concurrent_yielding_command_runs_once() -> Result<(), Error> {
    static YIELDING_COUNT: AtomicUsize = AtomicUsize::new(0);
    let mut env = ImmediateEnvironment::<Value>::new();
    env.command_registry
        .register_async_command(
            CommandKey::new_name("yielding"),
            |_state, _args, _ctx| {
                Box::pin(async move {
                    YIELDING_COUNT.fetch_add(1, Ordering::SeqCst);
                    // A real suspension point: this is what a JavaScript async command or any
                    // I/O does, and it is what the synchronous test command never did.
                    tokio::task::yield_now().await;
                    Ok(Value::from("y"))
                })
            },
        )
        .expect("register");
    let envref = env.to_ref();
    let m = envref.get_asset_manager();
    let query = q("yielding");
    let (a, b) = futures::join!(m.get_asset(&query), m.get_asset(&query));
    a?.get().await?;
    b?.get().await?;
    assert_eq!(
        YIELDING_COUNT.load(Ordering::SeqCst),
        1,
        "the command body must run once even when it yields mid-evaluation"
    );
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

// ---------------------------------------------------------------------------
// Readiness — the same guarantee under both managers (T6), and shared startup (T4)
// ---------------------------------------------------------------------------

/// `QUEUED-MANAGER-STARTUP-READINESS` verification item 5: the queued and inline managers must
/// offer *equivalent* readiness semantics even though their execution models differ.
///
/// They arrive at it differently — the queued manager used to spawn startup and the inline one
/// used to defer it lazily to the first evaluation — and both were unobservable. Now both are
/// started before the `EnvRef` is handed back, so the same assertion holds for each.
async fn scenario_ready_on_return<E>(envref: EnvRef<E>) -> Result<(), Error>
where
    E: Environment<Value = Value>,
{
    assert!(
        envref.get_asset_manager().is_started(),
        "the manager must be started before the EnvRef is observable"
    );
    // And it is usable immediately, with nothing awaited in between.
    let asset = envref.get_asset_manager().get_asset(&q("greet")).await?;
    assert_eq!(asset.get().await?.try_into_string()?, "hello");
    Ok(())
}

#[tokio::test]
async fn ready_on_return_default() -> Result<(), Error> {
    let mut env = SimpleEnvironment::<Value>::new();
    register_greet(&mut env.command_registry);
    scenario_ready_on_return(env.to_ref()).await
}

#[tokio::test]
async fn ready_on_return_immediate() -> Result<(), Error> {
    let mut env = ImmediateEnvironment::<Value>::new();
    register_greet(&mut env.command_registry);
    scenario_ready_on_return(env.to_ref()).await
}

/// Verification item 3: multiple concurrent first evaluations must share one startup operation.
///
/// The construction-time guarantee makes this trivially true rather than carefully arranged —
/// startup has already completed before any evaluation can begin, so there is no first-evaluation
/// race left to lose. Asserted anyway: a future change that moved startup back to a lazy path
/// would have to keep this true.
async fn scenario_concurrent_first_evaluations<E>(envref: EnvRef<E>) -> Result<(), Error>
where
    E: Environment<Value = Value>,
{
    assert!(envref.get_asset_manager().is_started());

    let mut handles = Vec::new();
    for _ in 0..8 {
        let envref = envref.clone();
        handles.push(async move {
            let asset = envref.get_asset_manager().get_asset(&q("greet")).await?;
            asset.get().await?.try_into_string()
        });
    }
    let results = futures::future::join_all(handles).await;
    for result in results {
        assert_eq!(result?, "hello");
    }
    Ok(())
}

#[tokio::test]
async fn concurrent_first_evaluations_default() -> Result<(), Error> {
    let mut env = SimpleEnvironment::<Value>::new();
    register_greet(&mut env.command_registry);
    scenario_concurrent_first_evaluations(env.to_ref()).await
}

#[tokio::test]
async fn concurrent_first_evaluations_immediate() -> Result<(), Error> {
    let mut env = ImmediateEnvironment::<Value>::new();
    register_greet(&mut env.command_registry);
    scenario_concurrent_first_evaluations(env.to_ref()).await
}

/// The no-tokio-runtime proof, extended from evaluation to **construction**.
///
/// `inline_builds_without_a_tokio_runtime` in `tests/environment_builder.rs` covers the builder
/// path; this covers `to_ref`, which is the door an ad-hoc environment uses. Both matter, because
/// the browser has no reactor for either to find.
#[test]
fn immediate_construction_without_tokio_runtime() -> Result<(), Error> {
    let mut env = ImmediateEnvironment::<Value>::new();
    register_greet(&mut env.command_registry);
    let envref: EnvRef<ImmediateEnvironment<Value>> = env.to_ref();
    assert!(
        envref.get_asset_manager().is_started(),
        "startup must complete during to_ref, with no runtime present"
    );
    Ok(())
}

// ============================================================================
// Persistence outcomes: only a keyed asset may be written to the store
// (evaluate-path-consolidation Step 3 — the durable-state change)
//
// The eight rows of the persistence table. Three of them narrow: a query asset resolving
// store_to_key, an apply with a bare-key recipe, and an apply whose recipe carries a filename
// all stop writing, because none of them is a keyed asset.
// ============================================================================

/// Row 1 — a keyed, non-volatile recipe asset is stored, and its value is loadable.
async fn scenario_persist_keyed_nonvolatile<E>(envref: EnvRef<E>) -> Result<(), Error>
where
    E: Environment<Value = Value>,
{
    let key = parse_key("dash.txt")?;
    let asset = envref.get_asset_manager().get(&key).await?;
    assert_eq!(asset.get().await?.try_into_string()?, "hello");
    let store = envref.get_async_store();
    assert!(
        store.contains(&key).await?,
        "a keyed non-volatile asset must be written to the store"
    );
    Ok(())
}

/// Row 2 — a **volatile** keyed asset is still stored. It is not persistent (its status is one
/// `try_fast_track` refuses), but the bytes land, which is what "stored but not loadable" means.
///
/// This is the regression guard for the whole design: a map-derived write predicate reports that
/// a volatile keyed asset owns nothing, and the existing `scenario_volatile_keyed_eval` asserts
/// only the produced value, so the loss would pass the suite unnoticed.
async fn scenario_persist_keyed_volatile<E>(envref: EnvRef<E>) -> Result<(), Error>
where
    E: Environment<Value = Value>,
{
    let key = parse_key("vol.txt")?;
    let asset = envref.get_asset_manager().get(&key).await?;
    let state = asset.get().await?;
    assert_eq!(state.value_state()?.try_into_string()?, "vol");
    let store = envref.get_async_store();
    assert!(
        store.contains(&key).await?,
        "a volatile keyed asset is keyed, so it is stored — it is merely not loadable"
    );
    Ok(())
}

/// Row 4 — a non-keyed query asset owns no place in the store and writes nothing.
async fn scenario_persist_query_writes_nothing<E>(envref: EnvRef<E>) -> Result<(), Error>
where
    E: Environment<Value = Value>,
{
    let asset = envref.get_asset_manager().get_asset(&q("greet")).await?;
    assert_eq!(asset.get().await?.try_into_string()?, "hello");
    assert_eq!(asset.key().await, None);
    let store = envref.get_async_store();
    assert!(
        !store.contains(&parse_key("dash.txt")?).await?,
        "a query asset must not write under any key"
    );
    Ok(())
}

/// Rows 6 and 7 — an ad-hoc `apply` writes nothing, even when its recipe is a bare key or
/// carries a filename. This is the durable half of `CONTEXT-APPLY-BARE-KEY-ILL-DEFINED`:
/// previously such an asset wrote its result under a key it did not own.
async fn scenario_persist_apply_writes_nothing<E>(envref: EnvRef<E>) -> Result<(), Error>
where
    E: Environment<Value = Value>,
{
    let m = envref.get_asset_manager();
    let store = envref.get_async_store();
    let target = parse_key("applied.txt")?;
    assert!(!store.contains(&target).await?, "precondition");

    // A recipe with cwd + filename, which `store_to_key()` resolves to `applied.txt`.
    let mut recipe: liquers_core::recipes::Recipe = q("greet/applied.txt").into();
    recipe.cwd = Some(String::new());
    let applied = m.apply(recipe, State::new(), None).await?;
    let _ = applied.get().await;

    assert_eq!(applied.key().await, None, "an apply asset is not keyed");
    assert!(
        !store.contains(&target).await?,
        "an ad-hoc apply owns no key and must not write to the store"
    );
    Ok(())
}

#[tokio::test]
async fn persist_keyed_nonvolatile_default() -> Result<(), Error> {
    let mut env = SimpleEnvironment::<Value>::new();
    register_greet(&mut env.command_registry);
    env.with_async_store(Box::new(recipe_store().await?));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    scenario_persist_keyed_nonvolatile(env.to_ref()).await
}

#[tokio::test]
async fn persist_keyed_nonvolatile_immediate() -> Result<(), Error> {
    let mut env = ImmediateEnvironment::<Value>::new();
    register_greet(&mut env.command_registry);
    env.with_async_store(Box::new(recipe_store().await?));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    scenario_persist_keyed_nonvolatile(env.to_ref()).await
}

#[tokio::test]
async fn persist_keyed_volatile_default() -> Result<(), Error> {
    let mut env = SimpleEnvironment::<Value>::new();
    register_vol_cmd(&mut env.command_registry);
    env.with_async_store(Box::new(volatile_recipe_store().await?));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    scenario_persist_keyed_volatile(env.to_ref()).await
}

#[tokio::test]
async fn persist_keyed_volatile_immediate() -> Result<(), Error> {
    let mut env = ImmediateEnvironment::<Value>::new();
    register_vol_cmd(&mut env.command_registry);
    env.with_async_store(Box::new(volatile_recipe_store().await?));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    scenario_persist_keyed_volatile(env.to_ref()).await
}

#[tokio::test]
async fn persist_query_writes_nothing_default() -> Result<(), Error> {
    let mut env = SimpleEnvironment::<Value>::new();
    register_greet(&mut env.command_registry);
    env.with_async_store(Box::new(recipe_store().await?));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    scenario_persist_query_writes_nothing(env.to_ref()).await
}

#[tokio::test]
async fn persist_query_writes_nothing_immediate() -> Result<(), Error> {
    let mut env = ImmediateEnvironment::<Value>::new();
    register_greet(&mut env.command_registry);
    env.with_async_store(Box::new(recipe_store().await?));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    scenario_persist_query_writes_nothing(env.to_ref()).await
}

#[tokio::test]
async fn persist_apply_writes_nothing_default() -> Result<(), Error> {
    let mut env = SimpleEnvironment::<Value>::new();
    register_greet(&mut env.command_registry);
    env.with_async_store(Box::new(recipe_store().await?));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    scenario_persist_apply_writes_nothing(env.to_ref()).await
}

#[tokio::test]
async fn persist_apply_writes_nothing_immediate() -> Result<(), Error> {
    let mut env = ImmediateEnvironment::<Value>::new();
    register_greet(&mut env.command_registry);
    env.with_async_store(Box::new(recipe_store().await?));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    scenario_persist_apply_writes_nothing(env.to_ref()).await
}
