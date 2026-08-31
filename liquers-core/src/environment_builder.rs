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
    /// The unconfigured recipe provider resolves to [`RecipeProviderChoice::Trivial`]. That is the
    /// *core* default and is deliberately not the same as the document default — see
    /// [`RecipeProviderChoice`] — nor the same as `liquers-lib`'s, which reads recipes through the
    /// store.
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
            .unwrap_or_else(|| RecipeProviderChoice::Trivial.provider());

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
