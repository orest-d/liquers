//! Environment construction: asset-manager kinds, and the builder that assembles an environment.
//!
//! # Why a builder
//!
//! An [`Environment`] owns its [`AssetManager`], and the manager needs an [`EnvRef`] back to that
//! environment — a construction cycle. Breaking it requires one deferred slot somewhere, and the
//! question is only who is allowed to observe the environment while that slot is empty. Before
//! this module the answer was "anybody": `to_ref` installed the back-reference and *spawned* an
//! async startup task, then returned, so a caller could evaluate against a manager whose command
//! versions had not been registered yet (`QUEUED-MANAGER-STARTUP-READINESS`).
//!
//! [`Environment::try_to_ref`] now owns the whole sequence and nothing escapes it early. This
//! module adds the configuration surface on top: [`EnvironmentBuilder`] collects services, builds
//! a [`GenericEnvironment`], and delegates to that same sequence — so there is one readiness
//! guarantee with one implementation, reached through two supported doors.
//!
//! # Choosing a manager
//!
//! [`AssetManagerKind`] selects the execution model as a type parameter rather than a value. It
//! has to be a *marker* rather than the manager type itself: the manager is parameterized by the
//! environment (`DefaultAssetManager<E>` where `E` owns an `Arc<DefaultAssetManager<E>>`), so
//! naming it directly produces an infinitely recursive type. A kind that is not parameterized by
//! `E` breaks the recursion and carries the manager as a generic associated type.

use std::marker::PhantomData;
use std::sync::Arc;

use crate::assets::AssetManager;
use crate::commands::{CommandRegistry, PayloadType};
use crate::context::{EnvRef, Environment, GenericEnvironment};
use crate::error::Error;
use crate::recipes::{AsyncRecipeProvider, RecipeProviderChoice};
use crate::store::AsyncStore;
use crate::store_config::StoreRouterConfig;
use crate::store_factory::{StoreFactory, StoreRouterBuilder};
use crate::type_system::TypeRegistry;
use crate::value::ValueInterface;

/// Per-manager construction settings.
///
/// Every field is optional, and a kind returns an error for a field it cannot honour rather than
/// dropping it: `job_capacity` set against an inline environment is a configuration mistake, not a
/// no-op, and silently ignoring it would make the mistake invisible.
///
/// Serde-able so [`crate::environment_config::EnvironmentConfig`] can carry it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetManagerOptions {
    /// Job-queue capacity for a queued kind. Defaults to the manager's own default (four).
    ///
    /// Must be at least 1. Zero is rejected at build time rather than treated as "no limit":
    /// the queue starts work only while `running_count < capacity`, so zero would accept
    /// evaluations and never run them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_capacity: Option<usize>,
}

impl AssetManagerOptions {
    /// Sets the job-queue capacity.
    pub fn with_job_capacity(mut self, capacity: usize) -> Self {
        self.job_capacity = Some(capacity);
        self
    }
}

/// Selects an asset-manager implementation without naming the environment it will serve.
///
/// A compile-time selector, deliberately **not** object-safe: it has a generic method and a
/// generic associated type, and is never used as `dyn`. See the module documentation for why the
/// manager type itself cannot be the parameter.
pub trait AssetManagerKind: 'static {
    /// The manager this kind produces for environment `E`.
    type Manager<E: Environment>: AssetManager<E>;

    /// Constructs the manager for an environment that already exists.
    ///
    /// Called from [`Environment::init_with_envref`], after the [`EnvRef`] is created and before
    /// anything else can observe it — so on both the builder path and the [`Environment::to_ref`]
    /// path. Synchronous: no store is touched, and the only work is in-memory.
    fn build<E: Environment>(
        envref: EnvRef<E>,
        options: &AssetManagerOptions,
    ) -> Result<Arc<Self::Manager<E>>, Error>;

    /// The recipe provider an environment of this kind installs when none is configured.
    ///
    /// This is on the *kind* rather than being a single global default because the kind is the
    /// only thing that distinguishes one built-in environment from another once they share
    /// [`GenericEnvironment`]. `SimpleEnvironment<V>` and `liquers_lib::DefaultEnvironment<V>`
    /// are otherwise the same type natively, and they have always disagreed here: the core
    /// environments resolve no recipes, while the library environment reads them through the
    /// store. Collapsing that would make every `-R/` query in an application relying on the
    /// library default fail with `KeyNotFound` — silently, since nothing stops compiling.
    ///
    /// Defaults to [`RecipeProviderChoice::Trivial`], which is the core behaviour; a kind
    /// overrides it only to carry a different crate's default.
    fn default_recipe_provider<E: Environment>() -> Arc<dyn AsyncRecipeProvider<E>> {
        RecipeProviderChoice::Trivial.provider()
    }
}

/// Native queued execution: [`crate::assets::DefaultAssetManager`], with a job queue and an
/// expiration monitor.
///
/// Construction spawns two Tokio tasks, so this kind requires an active runtime — synchronous
/// construction does not mean runtime-free. Use [`Inline`] where that matters.
#[cfg(not(target_arch = "wasm32"))]
pub struct Queued;

#[cfg(not(target_arch = "wasm32"))]
impl AssetManagerKind for Queued {
    type Manager<E: Environment> = crate::assets::DefaultAssetManager<E>;

    fn build<E: Environment>(
        envref: EnvRef<E>,
        options: &AssetManagerOptions,
    ) -> Result<Arc<Self::Manager<E>>, Error> {
        Ok(Arc::new(match options.job_capacity {
            // A zero capacity is not "no limit", it is a deadlock: the job queue starts an asset
            // only while `running_count < capacity`, so with zero nothing ever starts and every
            // submitted evaluation parks forever. The caller gets a hang with no error, which is
            // strictly worse than a rejected configuration — and this is now reachable from a
            // configuration document, not just from a deliberate `with_capacity(0)`.
            Some(0) => {
                return Err(Error::general_error(
                    "job_capacity is 0; the queued asset manager would accept work and never run                      it. Use at least 1, or leave it unset for the default."
                        .to_string(),
                ))
            }
            Some(capacity) => crate::assets::DefaultAssetManager::with_capacity(envref, capacity),
            None => crate::assets::DefaultAssetManager::new(envref),
        }))
    }
}

/// Spawn-free inline execution: [`crate::assets::ImmediateAssetManager`].
///
/// Needs no Tokio runtime and starts no background task, which is what a browser requires. It is
/// the only kind available on wasm, and is also useful natively for deterministic tests.
pub struct Inline;

impl AssetManagerKind for Inline {
    type Manager<E: Environment> = crate::assets::ImmediateAssetManager<E>;

    fn build<E: Environment>(
        envref: EnvRef<E>,
        options: &AssetManagerOptions,
    ) -> Result<Arc<Self::Manager<E>>, Error> {
        if options.job_capacity.is_some() {
            return Err(Error::general_error(
                "job_capacity is set, but the inline asset manager has no job queue".to_string(),
            ));
        }
        Ok(Arc::new(crate::assets::ImmediateAssetManager::new(envref)))
    }
}

/// The kind used when none is named: [`Queued`] natively, [`Inline`] on wasm.
///
/// The default type parameter of both [`EnvironmentBuilder`] and [`GenericEnvironment`], so
/// `EnvironmentBuilder::<Value>::new()` is correct on every target.
#[cfg(not(target_arch = "wasm32"))]
pub type DefaultKind = Queued;
/// The kind used when none is named. On wasm only [`Inline`] exists.
#[cfg(target_arch = "wasm32")]
pub type DefaultKind = Inline;

/// Assembles and starts an [`Environment`].
///
/// The recommended way to construct an environment. Services are configured with the `with_*`
/// setters, commands are registered into the public [`Self::command_registry`] field, and
/// [`Self::build`] returns an [`EnvRef`] that is ready to evaluate — the manager is constructed,
/// installed and started before it returns.
///
/// The split between the setters and the registry field is not accidental: services can be
/// described by a configuration document, commands cannot (they are Rust functions, and no
/// document can name one). See [`crate::environment_config::EnvironmentConfig`].
///
/// ```ignore
/// let mut builder = EnvironmentBuilder::<Value>::new().with_async_store(store);
/// register_command!(&mut builder.command_registry, fn greet(state) -> result)?;
/// let envref = builder.build()?;
/// ```
pub struct EnvironmentBuilder<V: ValueInterface, P: PayloadType = (), K: AssetManagerKind = DefaultKind>
{
    type_registry: TypeRegistry,
    async_store: Option<Arc<dyn AsyncStore>>,
    store_config: Option<(StoreRouterConfig, Box<dyn StoreFactory>, bool)>,
    /// Commands are registered here before [`Self::build`] freezes the environment.
    ///
    /// A public field rather than an accessor: `register_command!` needs a `&mut CommandRegistry`,
    /// which cannot be threaded through a by-value setter chain.
    pub command_registry: CommandRegistry<GenericEnvironment<V, P, K>>,
    recipe_provider: Option<Arc<dyn AsyncRecipeProvider<GenericEnvironment<V, P, K>>>>,
    manager_options: AssetManagerOptions,
    _payload: PhantomData<P>,
    _kind: PhantomData<K>,
}

impl<V: ValueInterface, P: PayloadType, K: AssetManagerKind> Default
    for EnvironmentBuilder<V, P, K>
{
    fn default() -> Self {
        Self::new()
    }
}

impl<V: ValueInterface, P: PayloadType, K: AssetManagerKind> EnvironmentBuilder<V, P, K> {
    /// A builder with a type registry from `V`, no store, and no recipe provider.
    ///
    /// The unconfigured recipe provider resolves to [`AssetManagerKind::default_recipe_provider`],
    /// which is [`RecipeProviderChoice::Trivial`] for the core kinds. That is deliberately not the
    /// same as the *document* default — see [`RecipeProviderChoice`] — nor the same as
    /// `liquers-lib`'s kind, which reads recipes through the store.
    pub fn new() -> Self {
        EnvironmentBuilder {
            type_registry: TypeRegistry::from_value_type::<V>(),
            async_store: None,
            store_config: None,
            command_registry: CommandRegistry::new(),
            recipe_provider: None,
            manager_options: AssetManagerOptions::default(),
            _payload: PhantomData,
            _kind: PhantomData,
        }
    }

    /// Supplies a type registry, for an integration adding a type `V` cannot describe statically.
    ///
    /// **Extend** [`TypeRegistry::from_value_type`]: starting from [`TypeRegistry::new`] loses
    /// every type the build already had, including the `error` pseudo-type that even a failed
    /// asset needs.
    pub fn with_type_registry(mut self, registry: TypeRegistry) -> Self {
        self.type_registry = registry;
        self
    }

    /// Supplies an already-constructed asynchronous store.
    pub fn with_async_store(mut self, store: Arc<dyn AsyncStore>) -> Self {
        self.async_store = Some(store);
        self
    }

    /// Builds the store from a configuration document and a factory chain.
    ///
    /// Construction is deferred to [`Self::build`], which is what keeps this setter infallible and
    /// chainable; a malformed configuration, an unset `${VAR}` reference or a store type no
    /// factory claims surfaces there.
    pub fn with_store_config(
        mut self,
        config: StoreRouterConfig,
        factory: Box<dyn StoreFactory>,
    ) -> Self {
        self.store_config = Some((config, factory, true));
        self
    }

    /// [`Self::with_store_config`] without `${VAR}` expansion.
    ///
    /// For an environment that has none — a browser page — or where expansion was already done.
    /// Mirrors [`StoreRouterBuilder::build_without_env_expansion`].
    pub fn with_store_config_unexpanded(
        mut self,
        config: StoreRouterConfig,
        factory: Box<dyn StoreFactory>,
    ) -> Self {
        self.store_config = Some((config, factory, false));
        self
    }

    /// Supplies a recipe provider.
    pub fn with_recipe_provider(
        mut self,
        provider: Arc<dyn AsyncRecipeProvider<GenericEnvironment<V, P, K>>>,
    ) -> Self {
        self.recipe_provider = Some(provider);
        self
    }

    /// Selects one of the built-in recipe providers by name.
    ///
    /// The data-expressible half of [`Self::with_recipe_provider`], so a configuration document
    /// and hand-written code spell the choice identically.
    pub fn with_recipe_provider_choice(self, choice: RecipeProviderChoice) -> Self {
        let provider = choice.provider::<GenericEnvironment<V, P, K>>();
        self.with_recipe_provider(provider)
    }

    /// Supplies per-manager construction settings.
    pub fn with_asset_manager_options(mut self, options: AssetManagerOptions) -> Self {
        self.manager_options = options;
        self
    }

    /// Constructs the environment, installs and starts its asset manager, and shares it.
    ///
    /// The returned reference is ready to evaluate: command metadata versions have been refreshed
    /// and registered into the dependency manager before it is handed back. Delegates to
    /// [`Environment::try_to_ref`] rather than reimplementing that sequence.
    pub fn build(self) -> Result<EnvRef<GenericEnvironment<V, P, K>>, Error> {
        let async_store: Arc<dyn AsyncStore> = match (self.async_store, self.store_config) {
            (Some(store), None) => store,
            (None, Some((config, factory, expand))) => {
                let builder = StoreRouterBuilder::new(config, factory);
                let router = if expand {
                    builder.build()?
                } else {
                    builder.build_without_env_expansion()?
                };
                Arc::new(router)
            }
            (Some(_), Some(_)) => {
                return Err(Error::general_error(
                    "both a store and a store configuration were supplied; use one or the other"
                        .to_string(),
                ))
            }
            (None, None) => Arc::new(crate::store::NoAsyncStore),
        };

        let recipe_provider = self
            .recipe_provider
            .unwrap_or_else(K::default_recipe_provider);

        let environment = GenericEnvironment::assemble(
            self.type_registry,
            async_store,
            self.command_registry,
            recipe_provider,
            self.manager_options,
        );
        environment.try_to_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command_metadata::{CommandKey, CommandMetadata};
    use crate::metadata::DependencyKey;
    use crate::value::Value;

    /// Registers one command so startup has a version to register.
    fn probe(builder: &mut EnvironmentBuilder<Value>) -> CommandKey {
        let key = CommandKey::new_name("probe");
        builder
            .command_registry
            .command_metadata_registry
            .add_command(&CommandMetadata::new("probe"));
        key
    }

    /// T1 (version half): the command's metadata version is in the dependency manager the instant
    /// `build()` returns.
    ///
    /// Before this work the same assertion after `to_ref()` found `None`: startup was a detached
    /// task. That is the whole defect, and this is the inverted reproduction of it — no sleep, no
    /// yield.
    #[tokio::test]
    async fn command_version_present_immediately_after_build() {
        let mut builder = EnvironmentBuilder::<Value>::new();
        let key = probe(&mut builder);

        let envref = builder.build().expect("build");

        let dep_key = DependencyKey::for_command_metadata(&key);
        assert!(
            envref
                .get_asset_manager()
                .dependency_manager()
                .get_version(&dep_key)
                .await
                .is_some(),
            "the metadata version must be registered before build() returns"
        );
        let impl_key = DependencyKey::for_command_implementation(&key);
        let _ = impl_key;
    }

    /// T13 (graph half): the version that reaches the dependency graph is the **refreshed** one.
    ///
    /// `register_command!` mutates metadata after the registry computes `metadata_version`. If the
    /// refresh did not precede startup, the graph would hold a version no command ever had, and
    /// every later comparison against it would be wrong — silently, since nothing errors.
    #[tokio::test]
    async fn build_registers_the_refreshed_metadata_version() {
        let mut builder = EnvironmentBuilder::<Value>::new();
        let key = probe(&mut builder);

        let cmr = &mut builder.command_registry.command_metadata_registry;
        let stale = cmr.get(key.clone()).unwrap().metadata_version;
        cmr.get_mut(key.clone())
            .unwrap()
            .with_doc("changed after the initial metadata version was calculated");

        let envref = builder.build().expect("build");

        let refreshed = envref
            .get_command_metadata_registry()
            .get(key.clone())
            .unwrap()
            .metadata_version;
        assert_ne!(refreshed, stale, "the refresh must recompute the version");

        let registered = envref
            .get_asset_manager()
            .dependency_manager()
            .get_version(&DependencyKey::for_command_metadata(&key))
            .await
            .expect("a version must be registered");
        assert_eq!(
            registered, refreshed,
            "the dependency graph must hold the refreshed version, not the stale one"
        );
    }

    /// T8: refreshing when nothing changed reports no work, however often it runs.
    #[tokio::test]
    async fn refresh_reports_nothing_when_nothing_changed() {
        let mut builder = EnvironmentBuilder::<Value>::new();
        probe(&mut builder);
        let envref = builder.build().expect("build");
        let manager = envref.get_asset_manager();

        assert!(manager.refresh_command_versions().expect("refresh").is_empty());
        assert!(manager.refresh_command_versions().expect("refresh").is_empty());
    }

    /// T7: a changed metadata version *is* reported by a refresh, which is what makes the barrier
    /// re-runnable rather than one-shot.
    ///
    /// This is the mechanism `POST-INIT-COMMAND-REGISTRATION` needs: the reported keys are exactly
    /// those whose dependents must be expired, and `refresh_command_versions_and_expire` applies
    /// the cascade for them.
    #[tokio::test]
    async fn refresh_reports_a_changed_version() {
        let mut builder = EnvironmentBuilder::<Value>::new();
        let key = probe(&mut builder);
        let envref = builder.build().expect("build");

        // Nothing changed yet.
        let manager = envref.get_asset_manager();
        assert!(manager.refresh_command_versions().expect("refresh").is_empty());

        // Register a *different* version for the same key, simulating a metadata edit that a
        // future dynamic-registration path would make.
        let dep_key = DependencyKey::for_command_metadata(&key);
        let bumped = crate::metadata::Version::new(9_999);
        let changed = manager
            .dependency_manager()
            .register_version_sync(&dep_key, bumped);
        assert!(
            changed,
            "overwriting an occupied entry with a different version must report the change"
        );

        // The next refresh puts the real version back, and reports that as a change.
        let reported = manager.refresh_command_versions().expect("refresh");
        assert!(
            reported.contains(&dep_key),
            "a version that differs from the registry's must be reported, got {reported:?}"
        );
    }

    /// T2 — **the original bug**, as a direct assertion.
    ///
    /// `AssetManager::register_plan_dependencies` reads a known version for each plan dependency.
    /// When startup had not run, that read returned `None`, the registration skipped, and the plan's
    /// dependency edges were **silently** not registered — no error, no log, just a dependency
    /// graph that would never invalidate anything. That is `QUEUED-MANAGER-STARTUP-READINESS`, and
    /// this asserts the inverse: an edge registered against a command key immediately after
    /// construction, with nothing awaited in between.
    #[tokio::test]
    async fn plan_dependencies_registered_immediately_after_build() {
        use crate::dependencies::{DependencyRelation, PlanDependency};

        let mut builder = EnvironmentBuilder::<Value>::new();
        let key = probe(&mut builder);
        let envref = builder.build().expect("build");
        let manager = envref.get_asset_manager();

        let command_key = DependencyKey::for_command_metadata(&key);
        let dependent = crate::parse::parse_key("report.txt").expect("key");

        manager
            .register_plan_dependencies(
                &dependent,
                &[PlanDependency::new(
                    command_key.clone(),
                    DependencyRelation::StateArgument,
                )],
            )
            .await
            .expect("register");

        // Expiring the command must now reach the dependent. Before this work the edge was never
        // created, so this returned nothing at all.
        let expired = manager.dependency_manager().expire(&command_key).await;
        let dependent_dep_key = DependencyKey::from(&dependent);
        assert!(
            expired.keys.contains(&dependent_dep_key),
            "expiring the command must cascade to the asset that depends on it; got {:?}",
            expired.keys
        );
    }

    /// The other half of T2: what the defect actually looked like.
    ///
    /// A dependency key with no registered version is skipped by `register_plan_dependencies`,
    /// silently, and no edge forms. Before this work *every* command key was in that state during
    /// the startup window, so this is not a hypothetical failure mode — it is the failure that was
    /// happening, reproduced here on purpose. It is also why the fix had to be a construction-time
    /// guarantee rather than a check: there is no error to notice.
    #[tokio::test]
    async fn an_unregistered_dependency_version_registers_no_edge() {
        use crate::dependencies::{DependencyRelation, PlanDependency};

        let mut builder = EnvironmentBuilder::<Value>::new();
        probe(&mut builder);
        let envref = builder.build().expect("build");
        let manager = envref.get_asset_manager();

        // A command that was never registered, so startup never gave it a version — exactly the
        // state every command was in before `build()` awaited startup.
        let unknown = DependencyKey::for_command_metadata(&CommandKey::new_name("never_declared"));
        let dependent = crate::parse::parse_key("report.txt").expect("key");

        manager
            .register_plan_dependencies(
                &dependent,
                &[PlanDependency::new(
                    unknown.clone(),
                    DependencyRelation::StateArgument,
                )],
            )
            .await
            .expect("register reports success even though it registered nothing");

        // `expire` reports the key itself alongside its dependents, so the assertion is about the
        // dependent: it is absent, because no edge was ever created for it.
        let expired = manager.dependency_manager().expire(&unknown).await;
        let dependent_dep_key = DependencyKey::from(&dependent);
        assert!(
            !expired.keys.contains(&dependent_dep_key),
            "no edge can exist for a version the manager never saw; got {:?}",
            expired.keys
        );
    }

    /// A zero job capacity is rejected rather than accepted into a deadlock.
    ///
    /// `JobQueue` starts an asset only while `running_count < capacity`, so capacity zero accepts
    /// every submission and runs none of them: the caller waits forever with no error. Since
    /// `job_capacity` is now reachable from a configuration document, that has to fail at build
    /// time — a hang is the one outcome a misconfiguration must not produce.
    #[tokio::test]
    async fn a_zero_job_capacity_is_rejected() {
        let result = EnvironmentBuilder::<Value>::new()
            .with_asset_manager_options(AssetManagerOptions::default().with_job_capacity(0))
            .build();

        match result {
            Ok(_) => panic!("job_capacity 0 would accept work and never run it"),
            Err(e) => {
                let message = e.to_string();
                assert!(
                    message.contains("job_capacity"),
                    "the error must name the offending option, got: {message}"
                );
            }
        }
    }

    /// A capacity of one is the smallest workable value and is accepted.
    #[tokio::test]
    async fn a_job_capacity_of_one_is_accepted() {
        let envref = EnvironmentBuilder::<Value>::new()
            .with_asset_manager_options(AssetManagerOptions::default().with_job_capacity(1))
            .build()
            .expect("capacity 1 is valid");
        assert!(envref.get_asset_manager().is_started());
    }

    /// The two per-crate recipe-provider defaults are distinct, and both are expressible as a
    /// `RecipeProviderChoice`.
    ///
    /// `liquers-core` defaults to `Trivial`; `liquers-lib`'s `default_environment_builder`
    /// configures `Default`. Collapsing them would silently stop `-R/` queries resolving for every
    /// application that relied on the library default, with no compile error anywhere.
    #[test]
    fn core_builder_defaults_to_the_trivial_provider() {
        let builder = EnvironmentBuilder::<Value, (), Inline>::new();
        assert!(
            builder.recipe_provider.is_none(),
            "an unconfigured builder holds no provider; build() resolves Trivial"
        );

        let configured =
            EnvironmentBuilder::<Value, (), Inline>::new()
                .with_recipe_provider_choice(RecipeProviderChoice::Default);
        assert!(configured.recipe_provider.is_some());
    }

    /// A store and a store configuration are mutually exclusive, and saying both is an error
    /// rather than a silent precedence rule.
    #[test]
    fn a_store_and_a_store_config_conflict() {
        let result = EnvironmentBuilder::<Value, (), Inline>::new()
            .with_async_store(Arc::new(crate::store::NoAsyncStore))
            .with_store_config(
                StoreRouterConfig::new(),
                Box::new(crate::store_factory::default_store_factory()),
            )
            .build();
        match result {
            Ok(_) => panic!("configuring both a store and a store configuration must fail"),
            Err(e) => assert!(e.to_string().contains("store configuration")),
        }
    }
}
