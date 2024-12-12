use std::{
    cell::RefCell,
    marker::PhantomData,
    rc::Rc,
    sync::{Arc, Mutex},
};

use crate::{
    cache::{Cache, NoCache},
    command_metadata::CommandMetadataRegistry,
    commands::{CommandExecutor, CommandRegistry},
    error::Error,
    metadata::MetadataRecord,
    query::{Query, TryToQuery},
    state::State,
    store::{NoStore, Store},
    value::ValueInterface,
};

pub trait Environment: Sized + Sync + Send {
    type Value: ValueInterface;
    type EnvironmentReference: EnvRef<Self>;
    type CommandExecutor: CommandExecutor<Self::EnvironmentReference, Self, Self::Value>;
    type Context: ContextInterface<Self>;

    fn evaluate(&mut self, _query: &Query) -> Result<State<Self::Value>, Error> {
        Err(Error::not_supported("evaluate not implemented".to_string()))
    }
    fn get_command_metadata_registry(&self) -> &CommandMetadataRegistry;
    fn get_mut_command_metadata_registry(&mut self) -> &mut CommandMetadataRegistry;
    fn get_command_executor(&self) -> &Self::CommandExecutor;
    fn get_mut_command_executor(&mut self) -> &mut Self::CommandExecutor;
    fn get_store(&self) -> Arc<Box<dyn Store>>;
    fn get_cache(&self) -> Arc<Mutex<Box<dyn Cache<Self::Value>>>>;
    #[cfg(feature = "async_store")]
    fn get_async_store(&self) -> Arc<Box<dyn crate::store::AsyncStore>>;
}

pub trait EnvRef<E: Environment>: Sized {
    fn get(&self) -> &E;
    fn get_ref(&self) -> Self;
    fn get_store(&self) -> Arc<Box<dyn Store>> {
        self.get().get_store()
    }
    fn new_context(&self) -> Context<Self, E> {
        Context::new(self.get_ref())
    }
    #[cfg(feature = "async_store")]
    fn get_async_store(&self) -> Arc<Box<dyn crate::store::AsyncStore>>{
        self.get().get_async_store()    
    }
}

impl<E: Environment> Clone for ArcEnvRef<E> {
    fn clone(&self) -> Self {
        self.get_ref()
    }
}

impl<E: Environment> Clone for RcEnvRef<E> {
    fn clone(&self) -> Self {
        self.get_ref()
    }
}

impl<E: Environment> Clone for StatEnvRef<E> {
    fn clone(&self) -> Self {
        self.get_ref()
    }
}

pub struct StatEnvRef<E: Environment + 'static>(pub &'static E);

impl<E: Environment> EnvRef<E> for StatEnvRef<E> {
    fn get(&self) -> &E {
        self.0
    }
    fn get_ref(&self) -> Self {
        StatEnvRef(self.0)
    }
}

pub struct RcEnvRef<E: Environment>(pub Rc<E>);

impl<E: Environment> EnvRef<E> for RcEnvRef<E> {
    fn get(&self) -> &E {
        &*self.0
    }
    fn get_ref(&self) -> Self {
        RcEnvRef(self.0.clone())
    }
}

pub struct ArcEnvRef<E: Environment>(pub Arc<E>);

impl<E: Environment> EnvRef<E> for ArcEnvRef<E> {
    fn get(&self) -> &E {
        &*self.0
    }
    fn get_ref(&self) -> Self {
        ArcEnvRef(self.0.clone())
    }
}

pub struct Context<ER: EnvRef<E>, E: Environment> {
    envref: ER,
    metadata: Rc<RefCell<MetadataRecord>>,
    environment: PhantomData<E>,
}

pub trait ContextInterface<E: Environment>{
    fn evaluate_dependency<Q:TryToQuery>(&self, query: Q) -> Result<State<<E as Environment>::Value>, Error> {
        crate::interpreter::PlanInterpreter::new(self.get_envref()).evaluate(query)
    }
    fn get_envref(&self) -> E::EnvironmentReference;
    fn get_environment(&self) -> &E;
    fn get_command_metadata_registry(&self) -> &CommandMetadataRegistry;
    fn get_command_executor(&self) -> &E::CommandExecutor;
    fn get_store(&self) -> Arc<Box<dyn Store>>;
    fn get_metadata(&self) -> MetadataRecord;
    fn set_filename(&self, filename: String);
    fn debug(&self, message: &str);
    fn info(&self, message: &str);
    fn warning(&self, message: &str);
    fn error(&self, message: &str);
    fn clone_context(&self) -> Self;
}

impl <E: Environment> ContextInterface<E> for Context<<E as Environment>::EnvironmentReference, E>
{
    fn get_environment(&self) -> &E {
        self.envref.get()
    }
    fn get_envref(&self) -> <E as Environment>::EnvironmentReference {
        self.envref.get_ref()
    }    
    fn get_command_metadata_registry(&self) -> &CommandMetadataRegistry {
        self.envref.get().get_command_metadata_registry()
    }
    fn get_command_executor(&self) -> &E::CommandExecutor {
        self.envref.get().get_command_executor()
    }
    fn get_store(&self) -> Arc<Box<dyn Store>> {
        self.envref.get().get_store()
    }
    fn get_metadata(&self) -> MetadataRecord {
        self.metadata.borrow().clone()
    }
    fn set_filename(&self, filename: String) {
        self.metadata.borrow_mut().with_filename(filename);
    }
    fn debug(&self, message: &str) {
        self.metadata.borrow_mut().debug(message);
    }
    fn info(&self, message: &str) {
        self.metadata.borrow_mut().info(message);
    }
    fn warning(&self, message: &str) {
        self.metadata.borrow_mut().warning(message);
    }
    fn error(&self, message: &str) {
        self.metadata.borrow_mut().error(message);
    }
    fn clone_context(&self) -> Self {
        Context {
            envref: self.envref.get_ref(),
            metadata: self.metadata.clone(),
            environment: PhantomData::default(),
        }
    }    
}

impl<ER: EnvRef<E>, E: Environment> Context<ER, E>{
    pub fn new(environment: ER) -> Self {
        Context {
            envref: environment,
            metadata: Rc::new(RefCell::new(MetadataRecord::new())),
            environment: PhantomData::default(),
        }
    }
}

/// Simple environment with configurable store and cache
/// CommandRegistry is used as command executor as well as it is providing the command metadata registry.
pub struct SimpleEnvironment<V: ValueInterface> {
    store: Arc<Box<dyn Store>>,
    #[cfg(feature = "async_store")]
    async_store: Arc<Box<dyn crate::store::AsyncStore>>,
    cache: Arc<Mutex<Box<dyn Cache<V>>>>,
    command_registry: CommandRegistry<ArcEnvRef<Self>, Self, V>,
}

impl<V: ValueInterface + 'static> SimpleEnvironment<V> {
    pub fn new() -> Self {
        SimpleEnvironment {
            store: Arc::new(Box::new(NoStore)),
            command_registry: CommandRegistry::new(),
            cache: Arc::new(Mutex::new(Box::new(NoCache::new()))),
            #[cfg(feature = "async_store")]
            async_store: Arc::new(Box::new(crate::store::NoAsyncStore)),
        }
    }
    pub fn with_store(&mut self, store: Box<dyn Store>) -> &mut Self {
        self.store = Arc::new(store);
        self
    }
    #[cfg(feature = "async_store")]
    pub fn with_async_store(&mut self, store:Box<dyn crate::store::AsyncStore>) -> &mut Self {
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

impl<V: ValueInterface> Environment for SimpleEnvironment<V> {
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
    fn get_async_store(&self) -> Arc<Box<dyn crate::store::AsyncStore>> {
        self.async_store.clone()
    }
    
}

mod tests {
    use super::*;
    use crate::value::Value;
    use std::sync::Arc;

    #[test]
    fn test_context() {
        let env = SimpleEnvironment::<Value>::new().to_ref();
        let context = env.new_context();
        assert!(context.get_metadata().log.is_empty());
        context.info("test");
        assert_eq!(context.get_metadata().log.len(), 1);
        let cx = context.clone_context();
        cx.info("info");
        assert_eq!(context.get_metadata().log.len(), 2);
        serde_yaml::to_writer(std::io::stdout(), &context.get_metadata()).expect("yaml error");
    }
}
