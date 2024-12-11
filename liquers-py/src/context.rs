use std::sync::{Arc, Mutex};


use liquers_core::{cache::{Cache, NoCache}, command_metadata::CommandMetadataRegistry, commands::CommandRegistry, context::{ArcEnvRef, ContextInterface}, store::{AsyncStore, NoAsyncStore, Store}};
use once_cell::sync::{Lazy, OnceCell};
use pyo3::{exceptions::PyException, prelude::*};

use crate::value::Value;

pub type EnvRefDef = liquers_core::context::ArcEnvRef<Environment>;
pub type ContextType = liquers_core::context::Context<EnvRefDef, Environment>;

/*
#[pyclass]
pub struct EnvRef(pub EnvRefDef);

#[pymethods]
impl EnvRef{
    #[new]
    fn new()->Self{
        let envref = liquers_core::context::ArcEnvRef(Arc::new(Environment::new()));
        EnvRef(envref)
    }
}
*/

#[pyclass]
pub struct Environment {
    pub store: Arc<Box<dyn Store>>,
    pub cache: Arc<Mutex<Box<dyn Cache<Value>>>>,
    pub command_registry: CommandRegistry<EnvRefDef, Self, Value>,
    //#[cfg(feature = "async_store")]
    //async_store: Arc<Mutex<Box<dyn AsyncStore>>>,
}

#[pymethods]
impl Environment {
    #[new]
    pub fn new() -> Self {
        Environment {
            store: Arc::new(Box::new(liquers_core::store::NoStore)),
            command_registry: CommandRegistry::new(),
            cache: Arc::new(Mutex::new(Box::new(NoCache::new()))),
            //#[cfg(feature = "async_store")]
            //async_store: Arc::new(Mutex::new(Box::new(NoAsyncStore))),
        }
    }

    #[getter]
    pub fn get_cmr(&self) -> crate::command_metadata::CommandMetadataRegistry {
        crate::command_metadata::CommandMetadataRegistry(self.command_registry.command_metadata_registry.clone())
    }

    #[setter]
    pub fn set_cmr(&mut self, cmr:&crate::command_metadata::CommandMetadataRegistry){
        self.command_registry.command_metadata_registry = cmr.0.clone();
    }

    /*
    pub fn with_store(&mut self, store: Box<dyn Store>) -> &mut Self {
        self.store = Arc::new(Mutex::new(store));
        self
    }
    #[cfg(feature = "async_store")]
    pub fn with_async_store(&mut self, store:Box<dyn AsyncStore>) -> &mut Self {
        self.async_store = Arc::new(Mutex::new(store));
        self
    }
    pub fn with_cache(&mut self, cache: Box<dyn Cache<Value>>) -> &mut Self {
        self.cache = Arc::new(Mutex::new(cache));
        self
    }
    pub fn to_ref(self)->EnvRef{
        liquers_core::context::ArcEnvRef(Arc::new(self))
    }
    */
}


impl liquers_core::context::Environment for Environment {
    type Value = Value;
    type CommandExecutor = CommandRegistry<Self::EnvironmentReference, Self, Self::Value>;
    type EnvironmentReference = EnvRefDef;
    type Context = liquers_core::context::Context<Self::EnvironmentReference, Self>;

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
    fn get_async_store(&self) -> Arc<Box<dyn liquers_core::store::AsyncStore>> {
        //self.async_store.clone()
        Arc::new(Box::new(NoAsyncStore))
    }
    
}


#[pyclass(unsendable)]
pub struct Context(pub liquers_core::context::Context<EnvRefDef, Environment>);

#[pymethods]
impl Context {
    fn info(&self, message:&str) {
        self.0.info(message);
    }
}


/*
static ENVREF:Lazy<EnvRef> = Lazy::new(||{
    liquers_core::context::ArcEnvRef(Arc::new(Environment::new()))
});
*/

/*
fn get_envref() -> Arc<Mutex<EnvRef>> {
    static INSTANCE: OnceCell<Arc<Mutex<EnvRef>>> = OnceCell::new();
    let envref = INSTANCE.get_or_init(|| {
        Arc::new(
            Mutex::new(
                liquers_core::context::ArcEnvRef(Arc::new(Environment::new()))
            )
        )
    });
    envref.clone()
}
*/

/*
pub struct PyEnvRef<E: Environment>(pub Rc<E>);

impl<E: Environment> EnvRef<E> for RcEnvRef<E> {
    fn get(&self) -> &E {
        &*self.0
    }
    fn get_ref(&self) -> Self {
        RcEnvRef(self.0.clone())
    }
}
*/