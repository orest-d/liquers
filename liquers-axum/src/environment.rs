use std::sync::Arc;

use futures::FutureExt;
use liquers_core::{
    assets::DefaultAssetManager,
    command_metadata::CommandMetadataRegistry,
    commands::CommandRegistry,
    context::{Context, EnvRef, Environment, Session, User},
    error::Error,
    query::Key,
    recipes::{AsyncRecipeProvider, Recipe},
    state::State,
    store::{AsyncStore, AsyncStoreWrapper, FileStore},
};
use tokio::sync::Mutex;

pub struct TrivialSession;
impl Session for TrivialSession {
    fn get_user(&self) -> &User {
        &User::Anonymous
    }
}

pub struct ServerEnvironment {
    async_store: Arc<Box<dyn AsyncStore>>,
    pub command_registry: CommandRegistry<Self>,
    asset_store: Arc<Box<DefaultAssetManager<Self>>>,
    recipe_provider: Option<Arc<Box<dyn AsyncRecipeProvider<Self>>>>,
}

pub type ServerEnvRef = EnvRef<ServerEnvironment>;

impl Default for ServerEnvironment {
    fn default() -> Self {
        Self::new()
    }
}

impl ServerEnvironment {
    pub fn new() -> Self {
        ServerEnvironment {
            command_registry: CommandRegistry::new(),
            async_store: Arc::new(Box::new(AsyncStoreWrapper(FileStore::new(
                ".",
                &Key::new(),
            )))),
            asset_store: Arc::new(Box::new(DefaultAssetManager::new())),
            recipe_provider: None,
        }
    }
    pub fn with_async_store(&mut self, store: Box<dyn AsyncStore>) -> &mut Self {
        self.async_store = Arc::new(store);
        self
    }
}

impl Environment for ServerEnvironment {
    type Value = liquers_core::value::Value;
    type CommandExecutor = CommandRegistry<Self>;
    type SessionType = TrivialSession;
    type Payload = ServerPayload;

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
    fn create_session(&self, _user: User) -> Self::SessionType {
        TrivialSession
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

pub struct ServerPayloadData {}

#[derive(Clone)]
pub struct ServerPayload(Arc<Mutex<ServerPayloadData>>);

impl ServerPayload {
    pub fn new() -> Self {
        ServerPayload(Arc::new(Mutex::new(ServerPayloadData {})))
    }
}
