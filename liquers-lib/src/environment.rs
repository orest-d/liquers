use std::sync::Arc;

use futures::FutureExt;
use liquers_core::{assets::DefaultAssetManager, command_metadata::CommandMetadataRegistry, commands::CommandRegistry, context::{Context, EnvRef, Environment, SimpleSession, User}, error::Error, recipes::{AsyncRecipeProvider, Recipe}, state::State, store::{AsyncStore, NoAsyncStore}, value::ValueInterface};

pub trait CommandRegistryAccess: Environment {
    fn get_mut_command_registry(&mut self) -> &mut CommandRegistry<Self>;
}


/// Simple environment with configurable store and cache
/// CommandRegistry is used as command executor as well as it is providing the command metadata registry.
pub struct DefaultEnvironment<V: ValueInterface> {
    async_store: Arc<Box<dyn AsyncStore>>,
    pub command_registry: CommandRegistry<Self>,
    asset_store: Arc<Box<DefaultAssetManager<Self>>>,
    recipe_provider: Option<Arc<Box<dyn AsyncRecipeProvider<Self>>>>,
}

impl<V: ValueInterface> Default for DefaultEnvironment<V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V: ValueInterface> CommandRegistryAccess for DefaultEnvironment<V> {
    fn get_mut_command_registry(&mut self) -> &mut CommandRegistry<Self> {
        &mut self.command_registry
    }
}

impl<V: ValueInterface> DefaultEnvironment<V> {
    pub fn new() -> Self {
        DefaultEnvironment {
            command_registry: CommandRegistry::new(),
            async_store: Arc::new(Box::new(NoAsyncStore)),
            asset_store: Arc::new(Box::new(DefaultAssetManager::new())),
            recipe_provider: None,
        }
    }
    pub fn with_async_store(&mut self, store: Box<dyn AsyncStore>) -> &mut Self {
        self.async_store = Arc::new(store);
        self
    }

    pub fn with_recipe_provider(
        &mut self,
        provider: Arc<Box<dyn AsyncRecipeProvider<Self>>>,
    ) -> &mut Self {
        self.recipe_provider = Some(provider);
        self
    }

    pub fn with_default_recipe_provider(
        &mut self
    ) -> &mut Self {
        
        let provider: Arc<Box<dyn AsyncRecipeProvider<Self>>> = Arc::new(Box::new(liquers_core::recipes::DefaultRecipeProvider));
        self.recipe_provider = Some(provider);
        self
    }
    pub fn with_trivial_recipe_provider(
        &mut self
    ) -> &mut Self {

        let provider: Arc<Box<dyn AsyncRecipeProvider<Self>>> = Arc::new(Box::new(liquers_core::recipes::TrivialRecipeProvider));
        self.recipe_provider = Some(provider);
        self
    }

}

// Specialized impl for DefaultEnvironment<Value> to add polars command registration
impl DefaultEnvironment<crate::value::Value> {
    /// Register polars commands (only available for DefaultEnvironment<Value>)
    pub fn register_polars_commands(&mut self) -> Result<(), Error> {
        crate::polars::register_commands(self)
    }
}

impl<V: ValueInterface> Environment for DefaultEnvironment<V> {
    type Value = V;
    type CommandExecutor = CommandRegistry<Self>;
    type SessionType = SimpleSession;
    type Payload = ();

    fn get_command_metadata_registry(&self) -> &CommandMetadataRegistry {
        &self.command_registry.command_metadata_registry
    }

    fn get_command_executor(&self) -> &Self::CommandExecutor {
        &self.command_registry
    }

    fn get_async_store(&self) -> Arc<Box<dyn AsyncStore>> {
        self.async_store.clone()
    }

    fn get_asset_manager(&self) -> Arc<Box<DefaultAssetManager<Self>>> {
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
    ) -> std::pin::Pin<
        Box<dyn core::future::Future<Output = Result<Arc<Self::Value>, Error>> + Send + 'static>,
    > {
        use liquers_core::interpreter::apply_plan;

        async move {
            let plan = {
                let cmr = envref.0.get_command_metadata_registry();
                recipe.to_plan(cmr)?
            };
            let res = apply_plan(plan, input_state, context, envref).await?;

            Ok(res)
        }
        .boxed()
    }
    
    fn get_recipe_provider(&self) -> Arc<Box<dyn AsyncRecipeProvider<Self>>> {
        if let Some(provider) = &self.recipe_provider {
            return provider.clone();
        }
        panic!("No recipe provider configured in SimpleEnvironment");
    }
    
    fn init_with_envref(&self, envref: EnvRef<Self>) {
        self.get_asset_manager().set_envref(envref.clone());
    }
}
