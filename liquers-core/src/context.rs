use std::{cell::RefCell, marker::PhantomData, rc::Rc, sync::{Arc, Mutex}};

use crate::{
    cache::{Cache, NoCache}, command_metadata::CommandMetadataRegistry, commands::{CommandExecutor, CommandRegistry}, error::Error, metadata::{self, MetadataRecord}, query::{Key, Query}, state::State, store::{NoStore, Store}, value::ValueInterface
};

pub trait Environment: Sized{
    type Value: ValueInterface;
    type EnvironmentReference: EnvRef<Self>;
    type CommandExecutor: CommandExecutor<Self::EnvironmentReference, Self, Self::Value>;

    fn evaluate(&mut self, _query: &Query) -> Result<State<Self::Value>, Error> {
        Err(Error::not_supported("evaluate not implemented".to_string()))
    }
    fn get_command_metadata_registry(&self) -> &CommandMetadataRegistry;
    fn get_mut_command_metadata_registry(&mut self) -> &mut CommandMetadataRegistry;
    fn get_command_executor(&self) -> &Self::CommandExecutor;
    fn get_mut_command_executor(&mut self) -> &mut Self::CommandExecutor;
    fn get_store(&self) -> Arc<Mutex<Box<dyn Store>>>;
    fn get_cache(&self) -> Arc<Mutex<Box<dyn Cache<Self::Value>>>>;
}

pub trait EnvRef<E: Environment>: Sized {
    fn get(&self) -> &E;
    fn get_ref(&self) -> Self;
    fn get_store(&self) -> Arc<Mutex<Box<dyn Store>>>{
        self.get().get_store()
    }
    fn new_context(&self) -> Context<Self, E> {
        Context::new(self.get_ref())
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

pub struct Context<ER:EnvRef<E>, E: Environment> {
    envref: ER,
    metadata: Rc<RefCell<MetadataRecord>>,
    environment: PhantomData<E>
}

impl <ER:EnvRef<E>, E: Environment> Context<ER, E> {
    pub fn new(environment: ER) -> Self {
        Context {
            envref: environment,
            metadata: Rc::new(RefCell::new(MetadataRecord::new())),
            environment: PhantomData::default(),
        }
    }
    pub fn get_environment(&self) -> &E {
        self.envref.get()
    }
    pub fn get_command_metadata_registry(&self) -> &CommandMetadataRegistry {
        self.envref.get().get_command_metadata_registry()
    }
    pub fn get_command_executor(&self) -> &E::CommandExecutor {
        self.envref.get().get_command_executor()
    }
    pub fn get_store(&self) -> Arc<Mutex<Box<dyn Store>>> {
        self.envref.get().get_store()
    }
    pub fn get_metadata(&self) -> MetadataRecord {
        self.metadata.borrow().clone()
    }
    pub fn set_filename(&self, filename: String) {
        self.metadata.borrow_mut().with_filename(filename);
    }
    pub fn debug(&self, message:&str){
        self.metadata.borrow_mut().debug(message);
    }
    pub fn info(&self, message:&str){
        self.metadata.borrow_mut().info(message);
    }
    pub fn warning(&self, message:&str){
        self.metadata.borrow_mut().warning(message);
    }
    pub fn error(&self, message:&str){
        self.metadata.borrow_mut().error(message);
    }
    pub fn clone_context(&self) -> Self {
        Context {
            envref: self.envref.get_ref(),
            metadata: self.metadata.clone(),
            environment: PhantomData::default(),
        }
    }
}

/// Simple environment with configurable store and cache
/// CommandRegistry is used as command executor as well as it is providing the command metadata registry.
pub struct SimpleEnvironment<V: ValueInterface> {
    store: Arc<Mutex<Box<dyn Store>>>,
    cache: Arc<Mutex<Box<dyn Cache<V>>>>,
    command_registry: CommandRegistry<ArcEnvRef<Self>,Self,V>
}

impl<V: ValueInterface+'static> SimpleEnvironment<V> {
    pub fn new() -> Self {
        SimpleEnvironment {
            store: Arc::new(Mutex::new(Box::new(NoStore))),
            command_registry: CommandRegistry::new(),
            cache: Arc::new(Mutex::new(Box::new(NoCache::new()))),
        }
    }
    pub fn with_store(&mut self, store: Box<dyn Store>) -> &mut Self {
        self.store = Arc::new(Mutex::new(store));
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
    type Value=V;
    type CommandExecutor=CommandRegistry<Self::EnvironmentReference, Self,V>;
    type EnvironmentReference=ArcEnvRef<Self>;

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
    fn get_store(&self) -> Arc<Mutex<Box<dyn Store>>>{
        self.store.clone()
    }
    
    fn get_cache(&self) -> Arc<Mutex<Box<dyn Cache<Self::Value>>>> {
        self.cache.clone()
    }
}

