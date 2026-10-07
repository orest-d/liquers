//! An `AssetManager` implemented outside `liquers-core`
//! (`specs/design/dependency-audit-and-expiry-provenance/`, Part F, Phase 3 I3).
//!
//! An integration test is its own crate, so everything here is built from the public API. If this
//! file compiles and passes, the public surface of `AssetManager` is enough to implement a
//! manager. `MinimalInlineAssetManager` lives in `common/minimal_manager.rs` because
//! `expiry_provenance_integration.rs` needs the same manager and one test file cannot import
//! another.

mod common;
mod fixtures;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use liquers_core::{
    assets::{AssetManager, EvalMode},
    command_metadata::CommandKey,
    context::{EnvRef, Environment},
    environment_builder::{AssetManagerOptions, DependencyAuditPolicy, EnvironmentBuilder},
    error::Error,
    metadata::{DependencyKey, ExpiryCause, ExpiryReason, LogEntryKind, Metadata, Status, Version},
    parse::parse_key,
    query::Key,
    recipes::DefaultRecipeProvider,
    state::State,
    store::{AsyncMemoryStore, AsyncStore},
    value::Value,
};

use common::manager_scenarios::{
    counted_recipe_store, counting_recipe_store, listing_store, provenance_evaluate_chain,
    provenance_store, provenance_text_metadata, recipe_store, register_counted,
    register_dependent, register_gate_command, register_greet, register_index_files,
    register_provenance_commands, register_vol_cmd, scenario_adhoc_apply_is_not_keyed,
    scenario_audit_after_restart, scenario_basic_eval, scenario_cache_and_mode,
    scenario_concurrent_first_evaluations, scenario_entry_point_equivalence,
    scenario_every_expired_asset_has_reason_and_log_line, scenario_expiry_reason_cascade,
    scenario_keyed_asset_records_its_key, scenario_keyed_delegation, scenario_keyed_eval,
    scenario_listing_dependency, scenario_persist_apply_writes_nothing,
    scenario_persist_keyed_nonvolatile, scenario_persist_keyed_volatile,
    scenario_persist_query_writes_nothing, scenario_ready_on_return, scenario_stale_dependency,
    scenario_stored_value, scenario_volatile_keyed_eval, stale_dependency_store,
    stored_text_store, volatile_recipe_store, StaleGate, lazy_expiry_chain_store,
    register_lazy_expiry_chain, scenario_lazy_deadline_expiry_cascade,
};
use common::minimal_manager::{
    MinimalEnv, MinimalInlineAssetManager, MinimalKind, AUDIT_TRAIL_PREFIX,
};
use fixtures::StoreSnapshot;

type TestResult = Result<(), Box<dyn std::error::Error>>;

/// Fail fast instead of hanging.
async fn within<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::time::timeout(std::time::Duration::from_secs(20), future)
        .await
        .expect("call did not finish within 20 s")
}

/// A fresh minimal environment over `store`, with recipes read through the store.
fn env_over(
    store: AsyncMemoryStore,
    register: impl FnOnce(&mut liquers_core::commands::CommandRegistry<MinimalEnv>),
) -> EnvRef<MinimalEnv> {
    let mut env = MinimalEnv::new();
    register(&mut env.command_registry);
    env.with_async_store(Box::new(store));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    env.to_ref()
}

fn manager(envref: &EnvRef<MinimalEnv>) -> Arc<MinimalInlineAssetManager<MinimalEnv>> {
    envref.get_asset_manager()
}

/// Names the scenario that failed, so a red run says which contract broke.
async fn scenario(
    name: &str,
    run: impl std::future::Future<Output = Result<(), Error>>,
) -> TestResult {
    within(run)
        .await
        .map_err(|e| format!("scenario '{name}' failed on the external manager: {e}").into())
}

/// Every scenario of `common/manager_scenarios.rs`, each in a fresh environment, against a
/// manager that shares no code with the built-in ones.
#[tokio::test]
async fn external_manager_passes_shared_scenarios() -> TestResult {
    // --- non-keyed ---
    let mut env = MinimalEnv::new();
    register_greet(&mut env.command_registry);
    scenario("basic_eval", scenario_basic_eval(env.to_ref())).await?;

    let mut env = MinimalEnv::new();
    register_greet(&mut env.command_registry);
    let envref = env.to_ref();
    assert_eq!(envref.get_asset_manager().eval_mode(), EvalMode::Inline);
    scenario("cache_and_mode", scenario_cache_and_mode(envref)).await?;

    let mut env = MinimalEnv::new();
    register_greet(&mut env.command_registry);
    scenario("ready_on_return", scenario_ready_on_return(env.to_ref())).await?;

    let mut env = MinimalEnv::new();
    register_greet(&mut env.command_registry);
    scenario(
        "concurrent_first_evaluations",
        scenario_concurrent_first_evaluations(env.to_ref()),
    )
    .await?;

    // --- keyed ---
    let keyed = |store: AsyncMemoryStore| {
        env_over(store, |cr| {
            register_greet(cr);
            register_dependent(cr);
            register_vol_cmd(cr);
        })
    };
    scenario("keyed_eval", scenario_keyed_eval(keyed(recipe_store().await?))).await?;
    scenario(
        "keyed_asset_records_its_key",
        scenario_keyed_asset_records_its_key(keyed(recipe_store().await?)),
    )
    .await?;
    scenario(
        "adhoc_apply_is_not_keyed",
        scenario_adhoc_apply_is_not_keyed(keyed(recipe_store().await?)),
    )
    .await?;
    scenario(
        "entry_point_equivalence",
        scenario_entry_point_equivalence(keyed(recipe_store().await?)),
    )
    .await?;
    scenario(
        "volatile_keyed_eval",
        scenario_volatile_keyed_eval(keyed(volatile_recipe_store().await?)),
    )
    .await?;

    for with_recipe in [true, false] {
        let calls = Arc::new(AtomicUsize::new(0));
        let envref = env_over(stored_text_store(with_recipe).await?, |cr| {
            register_counted(cr, calls.clone())
        });
        scenario("stored_value", scenario_stored_value(envref, calls)).await?;
    }

    let calls = Arc::new(AtomicUsize::new(0));
    let (store, value_writes) = counting_recipe_store().await?;
    let envref = {
        let mut env = MinimalEnv::new();
        register_counted(&mut env.command_registry, calls.clone());
        env.with_async_store(Box::new(store));
        env.with_recipe_provider(Box::new(DefaultRecipeProvider));
        env.to_ref()
    };
    scenario("keyed_delegation", scenario_keyed_delegation(envref, calls)).await?;
    assert_eq!(value_writes.load(Ordering::SeqCst), 1);

    // --- persistence ---
    scenario(
        "persist_keyed_nonvolatile",
        scenario_persist_keyed_nonvolatile(keyed(recipe_store().await?)),
    )
    .await?;
    scenario(
        "persist_keyed_volatile",
        scenario_persist_keyed_volatile(keyed(volatile_recipe_store().await?)),
    )
    .await?;
    scenario(
        "persist_query_writes_nothing",
        scenario_persist_query_writes_nothing(keyed(recipe_store().await?)),
    )
    .await?;
    scenario(
        "persist_apply_writes_nothing",
        scenario_persist_apply_writes_nothing(keyed(recipe_store().await?)),
    )
    .await?;

    // --- expiry provenance, audits, listings, stale dependencies ---
    let provenance = |store: AsyncMemoryStore| env_over(store, register_provenance_commands);
    scenario(
        "expiry_reason_cascade",
        scenario_expiry_reason_cascade(provenance(provenance_store(false).await?)),
    )
    .await?;
    scenario(
        "every_expired_asset_has_reason_and_log_line",
        scenario_every_expired_asset_has_reason_and_log_line(provenance(
            provenance_store(true).await?,
        )),
    )
    .await?;
    scenario(
        "audit_after_restart",
        scenario_audit_after_restart(provenance),
    )
    .await?;

    let calls = Arc::new(AtomicUsize::new(0));
    let envref = env_over(listing_store().await?, |cr| {
        register_provenance_commands(cr);
        register_index_files(cr, calls.clone());
    });
    scenario("listing_dependency", scenario_listing_dependency(envref, calls)).await?;

    let gate = StaleGate::new();
    let envref = env_over(stale_dependency_store().await?, |cr| {
        register_provenance_commands(cr);
        register_gate_command(cr, gate.clone());
    });
    scenario("stale_dependency", scenario_stale_dependency(envref, gate)).await?;
    Ok(())
}

/// Registration invariant (`ASSET-REGISTRATION-OWNERSHIP-CONTRACT`): at most one registered asset
/// per key, `lookup_key_asset` returns exactly that asset, a volatile asset is never registered.
#[tokio::test]
async fn external_manager_registers_one_asset_per_key() -> TestResult {
    let calls = Arc::new(AtomicUsize::new(0));
    let envref = env_over(counted_recipe_store().await?, |cr| {
        register_counted(cr, calls.clone())
    });
    let am = manager(&envref);
    let key = parse_key("dash.txt")?;
    assert!(am.lookup_key_asset(&key).is_none(), "nothing is registered before the first request");

    let assets = within(futures::future::join_all((0..8).map(|_| am.get(&key)))).await;
    let mut ids = Vec::new();
    for asset in assets {
        let asset = asset?;
        assert_eq!(asset.get().await?.try_into_string()?, "counted");
        ids.push(asset.id());
    }
    assert!(
        ids.iter().all(|id| *id == ids[0]),
        "eight concurrent requests for one key must converge on one asset: {ids:?}"
    );
    assert_eq!(am.registered_key_count(), 1);
    let registered = am.lookup_key_asset(&key).map(|asset| asset.id());
    assert_eq!(registered, Some(ids[0]), "lookup returns exactly the asset every caller got");
    assert_eq!(calls.load(Ordering::SeqCst), 1, "the command body ran once");

    // A later request reuses the registered asset.
    assert_eq!(am.get(&key).await?.id(), ids[0]);
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    // A volatile key is evaluated but never registered.
    let volatile_envref = env_over(volatile_recipe_store().await?, register_vol_cmd);
    let vam = manager(&volatile_envref);
    let vkey = parse_key("vol.txt")?;
    let first = vam.get(&vkey).await?;
    assert_eq!(first.get().await?.try_into_string()?, "vol");
    assert!(vam.lookup_key_asset(&vkey).is_none(), "a volatile asset is never registered");
    assert_eq!(vam.registered_key_count(), 0);
    let second = vam.get(&vkey).await?;
    assert_ne!(first.id(), second.id(), "each request for a volatile key gets its own asset");
    Ok(())
}

// --- the audit policy ---

/// `upper` that counts its runs; `b.txt` is `upper` over `a.txt`.
fn register_counting_upper(
    cr: &mut liquers_core::commands::CommandRegistry<MinimalEnv>,
    calls: Arc<AtomicUsize>,
) {
    cr.register_command(
        CommandKey::new_name("upper"),
        move |state: &State<Value>, _args, _ctx| -> Result<Value, Error> {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(Value::from(state.try_into_string()?.to_uppercase()))
        },
    )
    .expect("register upper");
}

fn minimal_env_with_policy(
    store: AsyncMemoryStore,
    policy: DependencyAuditPolicy,
    calls: Arc<AtomicUsize>,
) -> Result<EnvRef<MinimalEnv>, Error> {
    let mut builder = EnvironmentBuilder::<Value, (), MinimalKind>::new()
        .with_asset_manager_options(AssetManagerOptions::default().with_dependency_audit(policy))
        .with_async_store(Arc::new(store))
        .with_recipe_provider(Arc::new(DefaultRecipeProvider));
    register_counting_upper(&mut builder.command_registry, calls);
    builder.build()
}

/// The option reaches the external manager and changes what it does: after a restart in which a
/// dependency moved, `OnLoad` refuses the stale result and recomputes it, `Explicit` serves it.
#[tokio::test]
async fn external_manager_honours_audit_policy() -> TestResult {
    let a = parse_key("data/a.txt")?;
    let b = parse_key("data/b.txt")?;
    let keys: Vec<Key> = vec![parse_key("data/recipes.yaml")?, a.clone(), b.clone()];

    // Process one computes b.txt from a.txt and persists both.
    let snapshot = {
        let first_calls = Arc::new(AtomicUsize::new(0));
        let envref = minimal_env_with_policy(
            provenance_store(false).await?,
            DependencyAuditPolicy::Explicit,
            first_calls.clone(),
        )?;
        let am = envref.get_asset_manager();
        within(am.set_binary(&a, b"hello", provenance_text_metadata())).await?;
        let stored_b = within(am.get(&b)).await?;
        assert_eq!(stored_b.get().await?.try_into_string()?, "HELLO");
        assert_eq!(first_calls.load(Ordering::SeqCst), 1);
        StoreSnapshot::capture(&envref.get_async_store(), &keys).await?
    };

    // Process two replays the store after a batch job moved a.txt's version.
    let (snapshot_ref, a_ref) = (&snapshot, &a);
    let restart = |policy: DependencyAuditPolicy| async move {
        let (snapshot, a) = (snapshot_ref, a_ref);
        let store = AsyncMemoryStore::new(&Key::new());
        snapshot.replay_into(&store).await?;
        let mut metadata = store.get_metadata(a).await?;
        metadata.set_version(Some(Version::new(0xB0_0B)))?;
        store.set_metadata(a, &metadata).await?;
        let calls = Arc::new(AtomicUsize::new(0));
        let envref = minimal_env_with_policy(store, policy, calls.clone())?;
        Ok::<_, Error>((envref, calls))
    };

    let (strict, strict_calls) = restart(DependencyAuditPolicy::OnLoad).await?;
    assert_eq!(
        strict.get_asset_manager().dependency_audit_policy(),
        DependencyAuditPolicy::OnLoad,
        "the builder option reaches the manager"
    );
    let strict_b = within(strict.get_asset_manager().get(&b)).await?;
    assert_eq!(strict_b.get().await?.try_into_string()?, "HELLO");
    assert_eq!(
        strict_calls.load(Ordering::SeqCst),
        1,
        "OnLoad refuses the stale stored copy and recomputes it"
    );

    let (lax, lax_calls) = restart(DependencyAuditPolicy::Explicit).await?;
    assert_eq!(
        lax.get_asset_manager().dependency_audit_policy(),
        DependencyAuditPolicy::Explicit
    );
    let lax_b = within(lax.get_asset_manager().get(&b)).await?;
    assert_eq!(lax_b.get().await?.try_into_string()?, "HELLO");
    assert_eq!(
        lax_calls.load(Ordering::SeqCst),
        0,
        "Explicit serves the stored copy as it is"
    );
    Ok(())
}

/// `record_expiry` is the one place that decides what an expiry writes. A manager that overrides
/// it changes the wording of every route, and the default's behaviour is what it falls back to.
#[tokio::test]
async fn record_expiry_is_overridable_by_a_manager() -> TestResult {
    let envref = env_over(provenance_store(false).await?, register_provenance_commands);
    let am = manager(&envref);
    am.enable_audit_trail();
    let a = parse_key("data/a.txt")?;
    let b = parse_key("data/b.txt")?;
    within(am.set_binary(&a, b"hello", provenance_text_metadata())).await?;
    provenance_evaluate_chain(&envref).await?;

    within(am.set_binary(&a, b"changed", provenance_text_metadata())).await?;

    let root = DependencyKey::from(&a);
    let expected = ExpiryReason::Cascaded {
        cause: ExpiryCause::Updated {
            version: Version::from_content(b"changed"),
        },
        root: root.clone(),
        via: root,
    };
    let stored = envref.get_async_store().get_metadata(&b).await?;
    assert_eq!(stored.status(), Status::Expired);
    assert_eq!(
        stored.expiry_reason(),
        Some(expected.clone()),
        "the reason the default would set is still set"
    );
    let Metadata::MetadataRecord(record) = &stored else {
        panic!("data/b.txt must be stored with a MetadataRecord");
    };
    let default_line = expected.log_entry("data/b.txt");
    let Some(last) = record.log.last() else {
        panic!("data/b.txt has an empty log");
    };
    assert_eq!(last.kind, LogEntryKind::Info);
    assert_eq!(
        last.message,
        format!("{AUDIT_TRAIL_PREFIX}data/b.txt"),
        "the override's own line is the last one, naming the key"
    );
    let defaults = record
        .log
        .iter()
        .filter(|entry| entry.message == default_line.message && entry.kind == default_line.kind)
        .count();
    assert_eq!(defaults, 1, "the default line is still written once: {:?}", record.log);

    // The override saw the call that wrote the persisted entry.
    let calls = am.recorded_expiries();
    assert!(
        calls.iter().any(|(subject, reason)| subject == "data/b.txt" && *reason == expected),
        "record_expiry was called for data/b.txt: {calls:?}"
    );
    Ok(())
}

/// Lazy deadline expiry cascades on a manager written outside core, through public API only
/// (`expire_without_cascade` then `cascade_expire_dependents`).
#[tokio::test]
async fn external_manager_lazy_deadline_expiry_cascades() -> TestResult {
    let mut env = MinimalEnv::new();
    let calls = register_lazy_expiry_chain(&mut env.command_registry)?;
    env.with_async_store(Box::new(lazy_expiry_chain_store().await?));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    scenario(
        "lazy_deadline_expiry_cascade",
        scenario_lazy_deadline_expiry_cascade(env.to_ref(), calls),
    )
    .await
}
