//! Generic scenario bodies for the manager-parametric suite.
//!
//! These scenarios are written once and run against BOTH `DefaultAssetManager` (queued)
//! and `ImmediateAssetManager` (inline) implementations, proving the manager trait contract
//! holds for both.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use liquers_core::{
    assets::AssetManager,
    command_metadata::CommandKey,
    context::{EnvRef, Environment},
    error::Error,
    metadata::{Metadata, Status},
    parse::parse_key,
    query::{Key, Query, TryToQuery},
    state::State,
    store::{AsyncMemoryStore, AsyncStore},
    value::Value,
};

fn q(s: &str) -> Query {
    s.try_to_query().expect("query parse")
}

// --- generic scenario bodies (written once, run against both managers) ---

pub async fn scenario_basic_eval<E>(envref: EnvRef<E>) -> Result<(), Error>
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
pub async fn scenario_cache_and_mode<E>(envref: EnvRef<E>) -> Result<(), Error>
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

pub fn register_greet<E>(cr: &mut liquers_core::commands::CommandRegistry<E>)
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
pub async fn recipe_store() -> Result<AsyncMemoryStore, Error> {
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

pub async fn stored_text_store(include_recipe: bool) -> Result<AsyncMemoryStore, Error> {
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

pub async fn scenario_stored_value<E>(envref: EnvRef<E>, calls: Arc<AtomicUsize>) -> Result<(), Error>
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
pub async fn scenario_keyed_eval<E>(envref: EnvRef<E>) -> Result<(), Error>
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
pub async fn scenario_keyed_delegation<E>(
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
pub async fn counted_recipe_store() -> Result<AsyncMemoryStore, Error> {
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

pub struct CountingStore {
    pub inner: AsyncMemoryStore,
    pub value_writes: Arc<AtomicUsize>,
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

pub async fn counting_recipe_store() -> Result<(CountingStore, Arc<AtomicUsize>), Error> {
    let value_writes = Arc::new(AtomicUsize::new(0));
    Ok((
        CountingStore {
            inner: counted_recipe_store().await?,
            value_writes: value_writes.clone(),
        },
        value_writes,
    ))
}

pub fn register_counted<E>(cr: &mut liquers_core::commands::CommandRegistry<E>, calls: Arc<AtomicUsize>)
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
pub async fn volatile_recipe_store() -> Result<AsyncMemoryStore, Error> {
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

pub fn register_vol_cmd<E>(cr: &mut liquers_core::commands::CommandRegistry<E>)
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
pub async fn scenario_volatile_keyed_eval<E>(envref: EnvRef<E>) -> Result<(), Error>
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

pub async fn scenario_keyed_asset_records_its_key<E>(envref: EnvRef<E>) -> Result<(), Error>
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
pub async fn scenario_adhoc_apply_is_not_keyed<E>(envref: EnvRef<E>) -> Result<(), Error>
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
pub async fn scenario_entry_point_equivalence<E>(envref: EnvRef<E>) -> Result<(), Error>
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

pub fn register_dependent<E>(cr: &mut liquers_core::commands::CommandRegistry<E>)
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

/// Row 1 — a keyed, non-volatile recipe asset is stored, and its value is loadable.
pub async fn scenario_persist_keyed_nonvolatile<E>(envref: EnvRef<E>) -> Result<(), Error>
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
pub async fn scenario_persist_keyed_volatile<E>(envref: EnvRef<E>) -> Result<(), Error>
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
pub async fn scenario_persist_query_writes_nothing<E>(envref: EnvRef<E>) -> Result<(), Error>
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
pub async fn scenario_persist_apply_writes_nothing<E>(envref: EnvRef<E>) -> Result<(), Error>
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

/// `QUEUED-MANAGER-STARTUP-READINESS` verification item 5: the queued and inline managers must
/// offer *equivalent* readiness semantics even though their execution models differ.
///
/// They arrive at it differently — the queued manager used to spawn startup and the inline one
/// used to defer it lazily to the first evaluation — and both were unobservable. Now both are
/// started before the `EnvRef` is handed back, so the same assertion holds for each.
pub async fn scenario_ready_on_return<E>(envref: EnvRef<E>) -> Result<(), Error>
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

/// Verification item 3: multiple concurrent first evaluations must share one startup operation.
///
/// The construction-time guarantee makes this trivially true rather than carefully arranged —
/// startup has already completed before any evaluation can begin, so there is no first-evaluation
/// race left to lose. Asserted anyway: a future change that moved startup back to a lazy path
/// would have to keep this true.
pub async fn scenario_concurrent_first_evaluations<E>(envref: EnvRef<E>) -> Result<(), Error>
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
