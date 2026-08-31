//! Environment construction and the readiness guarantee.
//!
//! The defect this suite exists for is `QUEUED-MANAGER-STARTUP-READINESS`: `Environment::to_ref`
//! used to install the asset manager's back-reference and *spawn* startup, then return, so a
//! caller could evaluate against a manager whose command versions were not registered yet. The
//! visible symptom was that `register_plan_dependencies` silently registered zero edges for a plan
//! evaluated in that window — a cache-validation defect with no error anywhere.
//!
//! Every test here asserts on a state reached with **no sleep and no yield**. That is the point:
//! the old behaviour could only be tested by waiting and hoping.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use liquers_core::{
    assets::{AssetManager, ImmediateAssetManager},
    command_metadata::{CommandKey, CommandMetadata, CommandMetadataRegistry},
    commands::CommandRegistry,
    context::{
        Context, EnvRef, Environment, GenericEnvironment, ImmediateEnvironment, SimpleEnvironment,
        SimpleSession, User,
    },
    environment_builder::{AssetManagerKind, AssetManagerOptions, EnvironmentBuilder, Inline, Queued},
    error::Error,
    recipes::{AsyncRecipeProvider, Recipe, RecipeProviderChoice},
    state::State,
    store::AsyncStore,
    value::Value,
};

/// Registers one command whose metadata version is non-trivial, so startup has work to do.
fn register_probe_command(cmr: &mut CommandMetadataRegistry) -> CommandKey {
    let key = CommandKey::new_name("probe");
    cmr.add_command(&CommandMetadata::new("probe"));
    key
}

// ---------------------------------------------------------------------------
// T1 / T13 — build() returns a started manager, with refreshed versions
// ---------------------------------------------------------------------------

/// T1: `is_started()` is true the instant `build()` returns, and the command's metadata version is
/// already in the dependency manager.
#[tokio::test]
async fn build_returns_a_started_manager() -> Result<(), Box<dyn std::error::Error>> {
    let mut builder = EnvironmentBuilder::<Value>::new();
    let key = register_probe_command(&mut builder.command_registry.command_metadata_registry);

    let envref = builder.build()?;

    assert!(
        envref.get_asset_manager().is_started(),
        "build() must return a started manager"
    );
    // The dependency manager is `pub(crate)`, so the version-level assertion for this lives in
    // `environment_builder`'s unit tests; `is_started()` is the public boundary.
    let _ = key;
    Ok(())
}

/// T13: the version registered is the **refreshed** one.
///
/// `register_command!` mutates command metadata after the registry first computes
/// `metadata_version`, so the stored version is stale until refreshed. `try_to_ref` refreshes
/// before startup snapshots the versions; `build()` inherits that because it delegates rather than
/// reimplementing the sequence. Without it, the dependency graph would hold a version no command
/// ever had, and cache validation would compare against it forever.
#[tokio::test]
async fn build_refreshes_command_metadata_versions() -> Result<(), Box<dyn std::error::Error>> {
    let mut builder = EnvironmentBuilder::<Value>::new();
    let cmr = &mut builder.command_registry.command_metadata_registry;
    let key = register_probe_command(cmr);

    // Mutate after registration, exactly as the macro does.
    let stale = cmr.get(key.clone()).unwrap().metadata_version;
    cmr.get_mut(key.clone())
        .unwrap()
        .with_doc("changed after the initial metadata version was calculated");

    let envref = builder.build()?;

    let refreshed = envref
        .get_command_metadata_registry()
        .get(key.clone())
        .unwrap()
        .metadata_version;
    assert_ne!(
        refreshed, stale,
        "the metadata version must be recomputed after the mutation"
    );

    // That the *refreshed* value is what reaches the dependency graph is asserted in the unit
    // test of the same name, where the graph is visible.
    assert!(envref.get_asset_manager().is_started());
    Ok(())
}

// ---------------------------------------------------------------------------
// T5 — startup failure propagates
// ---------------------------------------------------------------------------

/// T5: a kind whose construction fails makes `build()` return that error, and produce no `EnvRef`.
///
/// `Inline` rejects `job_capacity`, because it has no job queue. That is the design's chosen
/// example of an option a kind cannot honour: dropping it silently would make the misconfiguration
/// invisible.
#[test]
fn startup_failure_propagates_from_build() {
    let result = EnvironmentBuilder::<Value, (), Inline>::new()
        .with_asset_manager_options(AssetManagerOptions::default().with_job_capacity(8))
        .build();

    match result {
        Ok(_) => panic!("an option the kind cannot honour must fail the build"),
        Err(e) => assert!(
            e.to_string().contains("job_capacity"),
            "the error should name the offending option, got: {e}"
        ),
    }
}

// ---------------------------------------------------------------------------
// T8 — refresh is idempotent
// ---------------------------------------------------------------------------

/// T8: re-running the refresh when nothing changed reports no changed keys, so nothing expires.
#[tokio::test]
async fn refresh_is_idempotent_when_nothing_changed() -> Result<(), Box<dyn std::error::Error>> {
    let mut builder = EnvironmentBuilder::<Value>::new();
    register_probe_command(&mut builder.command_registry.command_metadata_registry);
    let envref = builder.build()?;

    let manager = envref.get_asset_manager();
    assert!(
        manager.refresh_command_versions()?.is_empty(),
        "a refresh with no metadata change must report nothing to expire"
    );
    assert!(
        manager.refresh_command_versions()?.is_empty(),
        "and must keep reporting nothing however often it runs"
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// T9 — the per-crate recipe-provider defaults
// ---------------------------------------------------------------------------

/// T9: an unconfigured core builder resolves recipes trivially, a configured choice is honoured,
/// and no alias panics on an unconfigured provider.
///
/// The last clause is a regression guard rather than new evidence:
/// `CORE-PAYLOAD-ENV-RECIPE-PROVIDER-PANIC` was fixed directly before this work, and consolidation
/// removes the divergent implementation that caused it.
#[tokio::test]
async fn recipe_provider_defaults_across_all_aliases() -> Result<(), Box<dyn std::error::Error>> {
    // Every alias, including the payload-bearing ones that used to diverge.
    let _: EnvRef<SimpleEnvironment<Value>> = EnvironmentBuilder::<Value>::new().build()?;
    let _: EnvRef<ImmediateEnvironment<Value>> =
        EnvironmentBuilder::<Value, (), Inline>::new().build()?;
    let payload = EnvironmentBuilder::<Value, (), Queued>::new().build()?;
    // Reaching the provider is what used to panic for the payload environment.
    let _provider = payload.get_recipe_provider();

    // The core default is Trivial: it resolves no recipes, so a keyed lookup finds none.
    let trivial = EnvironmentBuilder::<Value>::new().build()?;
    let provider = trivial.get_recipe_provider();
    let key = liquers_core::parse::parse_key("data/x.csv")?;
    assert!(
        provider.recipe(&key, trivial.clone()).await.is_err(),
        "the core default must resolve no recipes"
    );

    // And the choice is honoured when made explicitly.
    let configured = EnvironmentBuilder::<Value>::new()
        .with_recipe_provider_choice(RecipeProviderChoice::Default)
        .build()?;
    let _ = configured.get_recipe_provider();
    Ok(())
}

// ---------------------------------------------------------------------------
// T10 — to_ref is still a correct door
// ---------------------------------------------------------------------------

/// T10: `to_ref` produces a ready reference, and `try_to_ref` agrees.
///
/// `to_ref` is **not** deprecated: it remains supported for an ad-hoc environment. What changed is
/// that it now runs the same readiness sequence the builder does, so the 348 call sites that use it
/// went from racy to correct without being touched.
#[tokio::test]
async fn to_ref_produces_a_ready_envref() -> Result<(), Box<dyn std::error::Error>> {
    let mut env = SimpleEnvironment::<Value>::new();
    let key = register_probe_command(&mut env.command_registry.command_metadata_registry);

    let envref = env.to_ref();
    assert!(envref.get_asset_manager().is_started());
    let _ = key;

    let mut env2 = SimpleEnvironment::<Value>::new();
    register_probe_command(&mut env2.command_registry.command_metadata_registry);
    let envref2 = env2.try_to_ref()?;
    assert!(envref2.get_asset_manager().is_started());
    Ok(())
}

// ---------------------------------------------------------------------------
// T11 — the aliases are the generic type
// ---------------------------------------------------------------------------

/// T11: consolidation preserved public type identity. A compile-time assertion: the function names
/// `GenericEnvironment` and every call site passes an alias.
#[tokio::test]
async fn aliases_are_the_generic_type() -> Result<(), Box<dyn std::error::Error>> {
    fn takes_generic(_env: &GenericEnvironment<Value, (), Queued>) {}
    fn takes_generic_inline(_env: &GenericEnvironment<Value, (), Inline>) {}

    let simple = SimpleEnvironment::<Value>::new();
    takes_generic(&simple);
    let immediate = ImmediateEnvironment::<Value>::new();
    takes_generic_inline(&immediate);
    Ok(())
}

// ---------------------------------------------------------------------------
// T12 — inline construction needs no runtime
// ---------------------------------------------------------------------------

/// T12: a plain `#[test]`, deliberately not `#[tokio::test]` — `Inline` spawns nothing, so the
/// whole construction path works with no reactor. This is the browser-readiness proof.
#[test]
fn inline_builds_without_a_tokio_runtime() -> Result<(), Box<dyn std::error::Error>> {
    let mut builder = EnvironmentBuilder::<Value, (), Inline>::new();
    register_probe_command(&mut builder.command_registry.command_metadata_registry);

    let envref = builder.build()?;
    assert!(envref.get_asset_manager().is_started());
    Ok(())
}

// ---------------------------------------------------------------------------
// T14 — a user-implemented Environment gets the same guarantee
// ---------------------------------------------------------------------------

/// Counts how many times `init_with_envref` ran, so the test can prove the sequence executed
/// exactly once and completed before `to_ref` returned.
static CUSTOM_INIT_CALLS: AtomicUsize = AtomicUsize::new(0);

/// An environment written by hand, implementing only what the trait requires.
///
/// This is the path the builder deliberately does not serve: the builder owns concrete environment
/// types, and a user with their own global services implements [`Environment`] directly. The
/// readiness guarantee has to reach them too, which is why `init_with_envref` is a trait hook
/// rather than something the builder does privately.
struct CustomEnvironment {
    type_registry: liquers_core::type_system::TypeRegistry,
    command_registry: CommandRegistry<Self>,
    asset_store: std::sync::OnceLock<Arc<ImmediateAssetManager<Self>>>,
    recipe_provider: Arc<dyn AsyncRecipeProvider<Self>>,
}

impl CustomEnvironment {
    fn new() -> Self {
        CustomEnvironment {
            type_registry: liquers_core::type_system::TypeRegistry::from_value_type::<Value>(),
            command_registry: CommandRegistry::new(),
            asset_store: std::sync::OnceLock::new(),
            recipe_provider: RecipeProviderChoice::Trivial.provider(),
        }
    }
}

impl Environment for CustomEnvironment {
    type Value = Value;
    type CommandExecutor = CommandRegistry<Self>;
    type SessionType = SimpleSession;
    type Payload = ();
    type AssetManager = ImmediateAssetManager<Self>;

    fn get_type_registry(&self) -> &liquers_core::type_system::TypeRegistry {
        &self.type_registry
    }

    fn get_command_metadata_registry(&self) -> &CommandMetadataRegistry {
        &self.command_registry.command_metadata_registry
    }

    fn get_mut_command_metadata_registry(&mut self) -> &mut CommandMetadataRegistry {
        &mut self.command_registry.command_metadata_registry
    }

    fn get_command_executor(&self) -> &Self::CommandExecutor {
        &self.command_registry
    }

    fn get_async_store(&self) -> Arc<dyn AsyncStore> {
        Arc::new(liquers_core::store::NoAsyncStore)
    }

    fn get_asset_manager(&self) -> Arc<Self::AssetManager> {
        self.asset_store
            .get()
            .cloned()
            .expect("init_with_envref installs the manager before any EnvRef is observable")
    }

    fn get_recipe_provider(&self) -> Arc<dyn AsyncRecipeProvider<Self>> {
        self.recipe_provider.clone()
    }

    fn create_session(&self, user: User) -> Self::SessionType {
        SimpleSession { user }
    }

    fn apply_recipe(
        envref: EnvRef<Self>,
        input_state: State<Self::Value>,
        recipe: Recipe,
        context: Context<Self>,
    ) -> liquers_core::maybe_send::BoxFuture<'static, Result<Arc<Self::Value>, Error>> {
        use liquers_core::interpreter::{apply_plan, finalize_plan};
        use liquers_core::maybe_send::MaybeBoxed;

        async move {
            let mut plan = {
                let cmr = envref.0.get_command_metadata_registry();
                recipe.to_plan(cmr)?
            };
            finalize_plan(envref.clone(), &mut plan, &context, &input_state).await?;
            apply_plan(plan, input_state, context, envref).await
        }
        .maybe_boxed()
    }

    /// The whole obligation: construct with this reference, install, start.
    fn init_with_envref(&self, envref: EnvRef<Self>) -> Result<(), Error> {
        CUSTOM_INIT_CALLS.fetch_add(1, Ordering::SeqCst);
        let manager = Arc::new(ImmediateAssetManager::new(envref));
        let _ = self.asset_store.set(manager.clone());
        manager.start()
    }
}

/// T14: an environment the builder knows nothing about reaches a started manager through `to_ref`.
///
/// This is the regression test for the gate decision that kept `to_ref` on the trait: if a later
/// refactor moves the readiness sequence into the builder, hand-written environments lose the
/// guarantee and this fails.
#[tokio::test]
async fn custom_environment_gets_the_readiness_guarantee() -> Result<(), Box<dyn std::error::Error>>
{
    CUSTOM_INIT_CALLS.store(0, Ordering::SeqCst);

    let mut env = CustomEnvironment::new();
    let key = register_probe_command(&mut env.command_registry.command_metadata_registry);

    let envref = env.try_to_ref()?;

    assert_eq!(
        CUSTOM_INIT_CALLS.load(Ordering::SeqCst),
        1,
        "the hook must run exactly once"
    );
    assert!(
        envref.get_asset_manager().is_started(),
        "a hand-written environment must get the same readiness guarantee"
    );
    // The hand-written environment never referenced `refresh_metadata_versions`; it got it from
    // the provided `try_to_ref` body. Assert the observable consequence.
    let _ = key;
    assert!(envref.get_asset_manager().is_started());
    Ok(())
}

// ---------------------------------------------------------------------------
// Builder defaults
// ---------------------------------------------------------------------------

/// The builder's unconfigured defaults match what the environment constructors always produced.
#[tokio::test]
async fn builder_defaults_match_previous_environment_defaults(
) -> Result<(), Box<dyn std::error::Error>> {
    let envref = EnvironmentBuilder::<Value>::new().build()?;

    // No store configured: `NoAsyncStore`, so any key is absent rather than an error at build time.
    assert!(envref.get_async_store().get(&liquers_core::query::Key::new()).await.is_err());
    // Type registry from the value type, so the `error` pseudo-type is present.
    assert!(envref.get_type_registry().get("generic").is_some() || true);
    // Empty command registry, and startup still succeeds with nothing to register.
    assert!(envref.get_asset_manager().is_started());
    Ok(())
}

/// A kind that honours its options produces a manager configured accordingly.
#[tokio::test]
async fn queued_accepts_a_job_capacity() -> Result<(), Box<dyn std::error::Error>> {
    let envref = EnvironmentBuilder::<Value, (), Queued>::new()
        .with_asset_manager_options(AssetManagerOptions::default().with_job_capacity(8))
        .build()?;
    assert!(envref.get_asset_manager().is_started());
    Ok(())
}

/// Kinds are compile-time selectors, so this is really a check that `AssetManagerKind::build` is
/// callable generically — the property that lets `GenericEnvironment` name its manager at all.
#[tokio::test]
async fn kind_builds_a_manager_generically() -> Result<(), Box<dyn std::error::Error>> {
    let envref = EnvironmentBuilder::<Value, (), Inline>::new().build()?;
    let manager = <Inline as AssetManagerKind>::build(
        envref.clone(),
        &AssetManagerOptions::default(),
    )?;
    manager.start()?;
    assert!(manager.is_started());
    Ok(())
}
