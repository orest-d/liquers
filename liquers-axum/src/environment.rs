use std::sync::{Arc, Mutex};

use liquers_core::{
    cache::{Cache, NoCache}, command_metadata::CommandMetadataRegistry, commands::CommandRegistry, context::{ArcEnvRef, Context, Environment}, error::Error, interpreter::AsyncPlanInterpreter, query::{self, TryToQuery}, state::State, store::{AsyncStore, NoAsyncStore, NoStore, Store}, value::ValueInterface
};
use tokio::sync::RwLock;

/// Simple environment with configurable store and cache
/// CommandRegistry is used as command executor as well as it is providing the command metadata registry.
pub struct ServerEnvironment<V: ValueInterface> {
    store: Arc<Box<dyn Store>>,
    #[cfg(feature = "async_store")]
    async_store: Arc<Box<dyn AsyncStore>>,
    cache: Arc<Mutex<Box<dyn Cache<V>>>>,
    command_registry: CommandRegistry<ArcEnvRef<Self>, Self, V>,
}

pub type ServerValue = liquers_core::value::Value;
pub type ServerEnvironmentType = ServerEnvironment<ServerValue>;
pub type SharedEnvironment = Arc<RwLock<ServerEnvironment<ServerValue>>>;

impl<V: ValueInterface + 'static> ServerEnvironment<V> {
    pub fn new() -> Self {
        ServerEnvironment {
            store: Arc::new(Box::new(NoStore)),
            command_registry: CommandRegistry::new(),
            cache: Arc::new(Mutex::new(Box::new(NoCache::new()))),
            #[cfg(feature = "async_store")]
            async_store: Arc::new(Box::new(NoAsyncStore)),
        }
    }
    pub fn with_store(&mut self, store: Box<dyn Store>) -> &mut Self {
        self.store = Arc::new(store);
        self
    }
    #[cfg(feature = "async_store")]
    pub fn with_async_store(&mut self, store: Box<dyn AsyncStore>) -> &mut Self {
        self.async_store = Arc::new(store);
        self
    }
    pub fn with_cache(&mut self, cache: Box<dyn Cache<V>>) -> &mut Self {
        self.cache = Arc::new(Mutex::new(cache));
        self
    }
    pub fn to_ref(self) -> ArcEnvRef<Self> {
        ArcEnvRef(Arc::new(self))
    }
}

pub async fn async_evaluate<E: Environment, Q: TryToQuery>(
    envref: <E as Environment>::EnvironmentReference,
    query: Q,
) -> Result<liquers_core::state::State<<E as Environment>::Value>, liquers_core::error::Error> {
    let mut pi = AsyncPlanInterpreter::new(envref);
    let query = query.try_to_query()?;
    pi.with_query(&query)?;
    //println!("{:?}", pi.plan);
    /*
    println!(
        "############################ PLAN ############################\n{}\n",
        serde_yaml::to_string(pi.plan.as_ref().unwrap()).unwrap()
    );
    */
    pi.run().await?;
    let state = pi.state.take();
    if let Some(state) = state {
        Ok(state)
    }
    else{
        Err(Error::general_error("No state returned".to_owned()).with_query(&query))
    }
}

impl<V: ValueInterface> Environment for ServerEnvironment<V> {
    type Value = V;
    type CommandExecutor = CommandRegistry<Self::EnvironmentReference, Self, V>;
    type EnvironmentReference = ArcEnvRef<Self>;
    type Context = Context<Self::EnvironmentReference, Self>;

    fn get_mut_command_metadata_registry(&mut self) -> &mut CommandMetadataRegistry {
        &mut self.command_registry.command_metadata_registry
    }

    fn get_command_metadata_registry(&self) -> &CommandMetadataRegistry {
        &self.command_registry.command_metadata_registry
    }

    fn get_command_executor(&self) -> &Self::CommandExecutor {
        &self.command_registry
    }
    fn get_mut_command_executor(&mut self) -> &mut Self::CommandExecutor {
        &mut self.command_registry
    }
    fn get_store(&self) -> Arc<Box<dyn Store>> {
        self.store.clone()
    }

    fn get_cache(&self) -> Arc<Mutex<Box<dyn Cache<Self::Value>>>> {
        self.cache.clone()
    }
    #[cfg(feature = "async_store")]
    fn get_async_store(&self) -> Arc<Box<dyn AsyncStore>> {
        self.async_store.clone()
    }
}
