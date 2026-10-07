//! Manager-parametric suite (async-wasm-refactor M-D).
//!
//! The same `AssetManager` trait contract is exercised over both built-in implementations —
//! `DefaultAssetManager` (via `SimpleEnvironment`, queued) and `ImmediateAssetManager` (via
//! `ImmediateEnvironment`, inline) — proving b1's manager is swappable behind the trait and
//! that `ImmediateAssetManager` evaluates correctly at runtime. Plus immediate-only checks:
//! concurrency dedup and the no-tokio-runtime proof (browser-readiness on native). The shared
//! scenarios also run against a manager written outside core, in `external_asset_manager.rs`.

mod common;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use liquers_core::{
    assets::AssetManager,
    command_metadata::CommandKey,
    context::{EnvRef, Environment, ImmediateEnvironment, SimpleEnvironment},
    error::Error,
    parse::parse_key,
    query::{Query, TryToQuery},
    recipes::DefaultRecipeProvider,
    value::Value,
};

use common::manager_scenarios::{
    lazy_expiry_chain_store, register_lazy_expiry_chain, scenario_lazy_deadline_expiry_cascade,
    scenario_lazy_dependent_read_first,
    listing_store, provenance_store, register_index_files, register_provenance_commands, scenario_listing_dependency, scenario_every_expired_asset_has_reason_and_log_line,
    scenario_audit_after_restart, scenario_expiry_reason_cascade,
    register_gate_command, scenario_stale_dependency, stale_dependency_store, StaleGate,
    counting_recipe_store, register_counted, register_dependent, register_greet, register_vol_cmd,
    recipe_store, scenario_adhoc_apply_is_not_keyed, scenario_basic_eval, scenario_cache_and_mode,
    scenario_concurrent_first_evaluations, scenario_entry_point_equivalence,
    scenario_keyed_asset_records_its_key, scenario_keyed_delegation, scenario_keyed_eval,
    scenario_persist_apply_writes_nothing, scenario_persist_keyed_nonvolatile,
    scenario_persist_keyed_volatile, scenario_persist_query_writes_nothing, scenario_ready_on_return,
    scenario_stored_value, scenario_volatile_keyed_eval, stored_text_store,
    volatile_recipe_store,
};

fn q(s: &str) -> Query {
    s.try_to_query().expect("query parse")
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

// --- expiry provenance (dependency-audit-and-expiry-provenance, Step 4) ---

#[tokio::test]
async fn expiry_reason_cascade_default() -> Result<(), Error> {
    let mut env = SimpleEnvironment::<Value>::new();
    register_provenance_commands(&mut env.command_registry);
    env.with_async_store(Box::new(provenance_store(false).await?));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    scenario_expiry_reason_cascade(env.to_ref()).await
}

#[tokio::test]
async fn expiry_reason_cascade_immediate() -> Result<(), Error> {
    let mut env = ImmediateEnvironment::<Value>::new();
    register_provenance_commands(&mut env.command_registry);
    env.with_async_store(Box::new(provenance_store(false).await?));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    scenario_expiry_reason_cascade(env.to_ref()).await
}

#[tokio::test]
async fn every_expired_asset_has_reason_and_log_line_default() -> Result<(), Error> {
    let mut env = SimpleEnvironment::<Value>::new();
    register_provenance_commands(&mut env.command_registry);
    env.with_async_store(Box::new(provenance_store(true).await?));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    scenario_every_expired_asset_has_reason_and_log_line(env.to_ref()).await
}

#[tokio::test]
async fn every_expired_asset_has_reason_and_log_line_immediate() -> Result<(), Error> {
    let mut env = ImmediateEnvironment::<Value>::new();
    register_provenance_commands(&mut env.command_registry);
    env.with_async_store(Box::new(provenance_store(true).await?));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    scenario_every_expired_asset_has_reason_and_log_line(env.to_ref()).await
}

#[tokio::test]
async fn audit_after_restart_default() -> Result<(), Error> {
    scenario_audit_after_restart(|store| {
        let mut env = SimpleEnvironment::<Value>::new();
        register_provenance_commands(&mut env.command_registry);
        env.with_async_store(Box::new(store));
        env.with_recipe_provider(Box::new(DefaultRecipeProvider));
        env.to_ref()
    })
    .await
}

#[tokio::test]
async fn audit_after_restart_immediate() -> Result<(), Error> {
    scenario_audit_after_restart(|store| {
        let mut env = ImmediateEnvironment::<Value>::new();
        register_provenance_commands(&mut env.command_registry);
        env.with_async_store(Box::new(store));
        env.with_recipe_provider(Box::new(DefaultRecipeProvider));
        env.to_ref()
    })
    .await
}

// --- folder-listing dependencies (dependency-audit-and-expiry-provenance, Step 8) ---

#[tokio::test]
async fn listing_dependency_default() -> Result<(), Error> {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut env = SimpleEnvironment::<Value>::new();
    register_provenance_commands(&mut env.command_registry);
    register_index_files(&mut env.command_registry, calls.clone());
    env.with_async_store(Box::new(listing_store().await?));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    scenario_listing_dependency(env.to_ref(), calls).await
}

#[tokio::test]
async fn listing_dependency_immediate() -> Result<(), Error> {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut env = ImmediateEnvironment::<Value>::new();
    register_provenance_commands(&mut env.command_registry);
    register_index_files(&mut env.command_registry, calls.clone());
    env.with_async_store(Box::new(listing_store().await?));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    scenario_listing_dependency(env.to_ref(), calls).await
}

// --- stale dependency, end to end (dependency-audit-and-expiry-provenance, Step 10) ---

#[tokio::test]
async fn stale_dependency_default() -> Result<(), Error> {
    let gate = StaleGate::new();
    let mut env = SimpleEnvironment::<Value>::new();
    register_provenance_commands(&mut env.command_registry);
    register_gate_command(&mut env.command_registry, gate.clone());
    env.with_async_store(Box::new(stale_dependency_store().await?));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    scenario_stale_dependency(env.to_ref(), gate).await
}

#[tokio::test]
async fn stale_dependency_immediate() -> Result<(), Error> {
    let gate = StaleGate::new();
    let mut env = ImmediateEnvironment::<Value>::new();
    register_provenance_commands(&mut env.command_registry);
    register_gate_command(&mut env.command_registry, gate.clone());
    env.with_async_store(Box::new(stale_dependency_store().await?));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    scenario_stale_dependency(env.to_ref(), gate).await
}

#[tokio::test]
async fn lazy_deadline_expiry_cascades_default() -> Result<(), Error> {
    let mut env = SimpleEnvironment::<Value>::new();
    let calls = register_lazy_expiry_chain(&mut env.command_registry)?;
    env.with_async_store(Box::new(lazy_expiry_chain_store().await?));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    scenario_lazy_deadline_expiry_cascade(env.to_ref(), calls).await
}

#[tokio::test]
async fn lazy_deadline_expiry_cascades_immediate() -> Result<(), Error> {
    let mut env = ImmediateEnvironment::<Value>::new();
    let calls = register_lazy_expiry_chain(&mut env.command_registry)?;
    env.with_async_store(Box::new(lazy_expiry_chain_store().await?));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    scenario_lazy_deadline_expiry_cascade(env.to_ref(), calls).await
}

#[tokio::test]
async fn lazy_dependent_read_first_immediate() -> Result<(), Error> {
    let mut env = ImmediateEnvironment::<Value>::new();
    let calls = register_lazy_expiry_chain(&mut env.command_registry)?;
    env.with_async_store(Box::new(lazy_expiry_chain_store().await?));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider));
    scenario_lazy_dependent_read_first(env.to_ref(), calls).await
}
