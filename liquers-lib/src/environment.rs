use std::sync::Arc;

use liquers_core::{
    assets::AssetManager,
    command_metadata::CommandMetadataRegistry,
    commands::{CommandRegistry, PayloadType},
    context::{Context, EnvRef, Environment, SimpleSession, User},
    error::Error,
    maybe_send::MaybeBoxed,
    recipes::{AsyncRecipeProvider, Recipe},
    state::State,
    store::{AsyncStore, NoAsyncStore},
    value::ValueInterface,
};

// The asset manager is selected by target: the threaded `DefaultAssetManager` natively, the
// spawn-free `ImmediateAssetManager` on wasm (the browser has no tokio runtime). This is what
// lets `ui_spec_demo` keep using `DefaultEnvironment` unchanged and run in the browser.
#[cfg(not(target_arch = "wasm32"))]
use liquers_core::assets::DefaultAssetManager as SelectedAssetManager;
#[cfg(target_arch = "wasm32")]
use liquers_core::assets::ImmediateAssetManager as SelectedAssetManager;

pub trait CommandRegistryAccess: Environment {
    fn get_mut_command_registry(&mut self) -> &mut CommandRegistry<Self>;
}

/// Simple environment with configurable store and cache
/// CommandRegistry is used as command executor as well as it is providing the command metadata registry.
pub struct DefaultEnvironment<V: ValueInterface, P: PayloadType = ()> {
    async_store: Arc<dyn AsyncStore>,
    pub command_registry: CommandRegistry<Self>,
    asset_store: Arc<SelectedAssetManager<Self>>,
    recipe_provider: Option<Arc<dyn AsyncRecipeProvider<Self>>>,
    _payload: std::marker::PhantomData<P>,
}

impl<V: ValueInterface, P: PayloadType> Default for DefaultEnvironment<V, P> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V: ValueInterface, P: PayloadType> CommandRegistryAccess for DefaultEnvironment<V, P> {
    fn get_mut_command_registry(&mut self) -> &mut CommandRegistry<Self> {
        &mut self.command_registry
    }
}

impl<V: ValueInterface, P: PayloadType> DefaultEnvironment<V, P> {
    pub fn new() -> Self {
        DefaultEnvironment {
            command_registry: CommandRegistry::new(),
            async_store: Arc::new(NoAsyncStore),
            asset_store: Arc::new(SelectedAssetManager::new()),
            recipe_provider: None,
            _payload: std::marker::PhantomData,
        }
    }
    pub fn with_async_store(&mut self, store: Box<dyn AsyncStore>) -> &mut Self {
        self.async_store = Arc::from(store);
        self
    }

    pub fn with_recipe_provider(
        &mut self,
        provider: Arc<dyn AsyncRecipeProvider<Self>>,
    ) -> &mut Self {
        self.recipe_provider = Some(provider);
        self
    }

    pub fn with_default_recipe_provider(&mut self) -> &mut Self {
        let provider: Arc<dyn AsyncRecipeProvider<Self>> =
            Arc::new(liquers_core::recipes::DefaultRecipeProvider);
        self.recipe_provider = Some(provider);
        self
    }
    pub fn with_trivial_recipe_provider(&mut self) -> &mut Self {
        let provider: Arc<dyn AsyncRecipeProvider<Self>> =
            Arc::new(liquers_core::recipes::TrivialRecipeProvider);
        self.recipe_provider = Some(provider);
        self
    }
}

// Specialized impl for DefaultEnvironment<Value> to add polars command registration
#[cfg(feature = "polars")]
impl DefaultEnvironment<crate::value::Value> {
    /// Register polars commands (only available for DefaultEnvironment<Value>)
    pub fn register_polars_commands(&mut self) -> Result<(), Error> {
        crate::polars::register_commands(self)
    }
}

impl<V: ValueInterface, P: PayloadType> Environment for DefaultEnvironment<V, P> {
    type Value = V;
    type CommandExecutor = CommandRegistry<Self>;
    type SessionType = SimpleSession;
    type Payload = P;
    type AssetManager = SelectedAssetManager<Self>;

    fn get_command_metadata_registry(&self) -> &CommandMetadataRegistry {
        &self.command_registry.command_metadata_registry
    }

    fn get_command_executor(&self) -> &Self::CommandExecutor {
        &self.command_registry
    }

    fn get_async_store(&self) -> Arc<dyn AsyncStore> {
        self.async_store.clone()
    }

    fn get_asset_manager(&self) -> Arc<SelectedAssetManager<Self>> {
        self.asset_store.clone()
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

        async move {
            let recipe_expires = recipe.expires.clone();
            let mut plan = {
                let cmr = envref.0.get_command_metadata_registry();
                recipe.to_plan(cmr)?
            };

            finalize_plan(envref.clone(), &mut plan, &context).await?;
            let combined_expires = plan.expires.clone() | recipe_expires;
            context.set_expires(combined_expires).await?;

            let res = apply_plan(plan, input_state, context, envref).await?;

            Ok(res)
        }
        .maybe_boxed()
    }

    fn get_recipe_provider(&self) -> Arc<dyn AsyncRecipeProvider<Self>> {
        if let Some(provider) = &self.recipe_provider {
            return provider.clone();
        }
        panic!("No recipe provider configured in DefaultEnvironment");
    }

    fn init_with_envref(&self, envref: EnvRef<Self>) {
        self.get_asset_manager().set_envref(envref.clone());
        // Native: eagerly load command versions in a background task. Wasm/immediate: the
        // manager's `start()` runs lazily on first use, so nothing is spawned here.
        #[cfg(not(target_arch = "wasm32"))]
        {
            let am = self.get_asset_manager();
            tokio::spawn(async move {
                use liquers_core::assets::AssetManager;
                am.start().await;
            });
        }
    }
}
