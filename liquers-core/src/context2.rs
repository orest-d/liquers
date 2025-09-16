//! This defines Environment and Context.
//!
//! * [Environment] is a global object that holds configuration and services like command executor, asset store, etc.
//! * [Session] connects multiple actions of a single user.
//! * [User] represents an individual user interacting with the system.
//! * [Context] is a per-action object that holds e.g. the environment reference, metadata, current working directory.
//!
//! This builds a natural hierarchy. The most specific structure is the [Context],
//! which provides access to thhe [Session] and [Environment].
//! [ActionContext] is a public interface to the Context.

use core::panic;
use std::sync::{Arc, Mutex};

use crate::{
    assets2::{AssetRef, DefaultAssetManager}, cache::Cache, command_metadata::CommandMetadataRegistry, commands2::{CommandExecutor, CommandRegistry}, error::Error, metadata::{LogEntry, MetadataRecord}, query::Key, store::{NoStore, Store}, value::ValueInterface
};

pub trait Environment: Sized + Sync + Send + 'static {
    type Value: ValueInterface;
    type CommandExecutor: CommandExecutor<Self>;

    fn get_command_metadata_registry(&self) -> &CommandMetadataRegistry;
    fn get_command_executor(&self) -> &Self::CommandExecutor;
    #[cfg(feature = "async_store")]
    fn get_async_store(&self) -> Arc<Box<dyn crate::store::AsyncStore>>;

    fn get_asset_manager(
        &self,
    ) -> Arc<Box<DefaultAssetManager<Self>>>;
}

// TODO: Define Session and User; Session connects multiple actions of a single user.
// TODO: Session could be "SystemSession" for automated tasks or recipes.
pub struct EnvRef<E: Environment>(pub Arc<E>);

impl<E: Environment> EnvRef<E> {
    pub fn new(env: E) -> Self {
        EnvRef(Arc::new(env))
    }
    #[cfg(feature = "async_store")]
    pub fn get_async_store(&self) -> Arc<Box<dyn crate::store::AsyncStore>> {
        self.0.get_async_store()
    }
    pub fn get_command_metadata_registry(&self) -> &CommandMetadataRegistry {
        self.0.get_command_metadata_registry()
    }
    pub fn get_command_executor(&self) -> &E::CommandExecutor {
        self.0.get_command_executor()
    }

    pub fn get_asset_manager(
        &self,
    ) -> Arc<Box<DefaultAssetManager<E>>> {
        self.0.get_asset_manager()
    }

}

impl<E: Environment> Clone for EnvRef<E> {
    fn clone(&self) -> Self {
        EnvRef(self.0.clone())
    }
}

// TODO: There should be an asset reference
pub struct Context<E: Environment> {
    assetref: AssetRef<E>,
    cwd_key: Arc<Mutex<Option<Key>>>, // TODO: CWD should be owned by the context or maybe it should be in the Metadata
}

impl<E: Environment> Context<E> {
    pub async fn new(assetref:AssetRef<E>) -> Self {
        Context {
            assetref,
            cwd_key: Arc::new(Mutex::new(None)),
        }
    }
    pub async fn get_metadata(&self) -> Result<MetadataRecord, Error> {
        self.assetref.data.read().await.metadata.metadata_record().ok_or(
            Error::unexpected_error(format!("{} has legacy metadata", self.assetref.asset_reference().await))
        )
    }
    pub async fn set_filename(&self, filename: &str) -> Result<(), Error> {
        self.assetref.data.write().await.metadata.set_filename(filename).map(|_| ())
    }
    pub async fn debug(&self, message: &str) {
        self.assetref.data.write().await.metadata.add_log_entry(LogEntry::debug(message.to_string()));
    }
    pub async fn info(&self, message: &str) {
        self.assetref.data.write().await.metadata.add_log_entry(LogEntry::info(message.to_string()));
    }
    pub async fn warning(&self, message: &str) {
        self.assetref.data.write().await.metadata.add_log_entry(LogEntry::warning(message.to_string()));
    }
    pub async fn error(&self, message: &str) {
        self.assetref.data.write().await.metadata.add_log_entry(LogEntry::error(message.to_string()));
    }
    pub fn clone_context(&self) -> Self {
        Context {
            //asset: self.asset.clone(),
            assetref: self.assetref.clone(),
            cwd_key: self.cwd_key.clone(),
        }
    }
    pub fn get_cwd_key(&self) -> Option<Key> {
        self.cwd_key.lock().unwrap().clone()
    }

    pub fn set_cwd_key(&self, key: Option<Key>) {
        let mut guard = self.cwd_key.lock().unwrap();
        *guard = key;
    }

    pub fn get_asset_ref(&self) -> AssetRef<E> {
        self.assetref.clone()
    }
}

// TODO: Think about the Payload. EnvRef and Session should always be available.
// TODO: Add reference to Session
// TODO: Add EnvRef
// TODO: Add progress reporting
// TODO: Should action parameters be in context?
// TODO: There should be a reference to input_state_query
// TODO: There should be a reference to query including the current action

/// Simple environment with configurable store and cache
/// CommandRegistry is used as command executor as well as it is providing the command metadata registry.
pub struct SimpleEnvironment<V: ValueInterface> {
    store: Arc<Box<dyn Store>>,
    #[cfg(feature = "async_store")]
    async_store: Arc<Box<dyn crate::store::AsyncStore>>,
    //cache: Arc<tokio::sync::RwLock<Box<dyn Cache<V>>>>,
    pub command_registry: CommandRegistry<Self>,
    asset_store: Arc<Box<DefaultAssetManager<Self>>>,
}

impl<V: ValueInterface> Default for SimpleEnvironment<V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V: ValueInterface> SimpleEnvironment<V> {
    pub fn new() -> Self {
        SimpleEnvironment {
            store: Arc::new(Box::new(NoStore)),
            command_registry: CommandRegistry::new(),
            //            cache: Arc::new(tokio::sync::RwLock::new(Box::new(NoCache::<V>::new()))),
            #[cfg(feature = "async_store")]
            async_store: Arc::new(Box::new(crate::store::NoAsyncStore)),
            asset_store: Arc::new(Box::new(crate::assets2::DefaultAssetManager::new()))
        }
    }
    pub fn with_store(&mut self, store: Box<dyn Store>) -> &mut Self {
        self.store = Arc::new(store);
        self
    }
    #[cfg(feature = "async_store")]
    pub fn with_async_store(&mut self, store: Box<dyn crate::store::AsyncStore>) -> &mut Self {
        self.async_store = Arc::new(store);
        self
    }
    pub fn with_cache(&mut self, cache: Box<dyn Cache<V>>) -> &mut Self {
        panic!("SimpleEnvironment does not support cache for now");
    }
    pub fn to_ref(self) -> EnvRef<Self> {
        let envref = EnvRef::new(self);
        let envref1 = envref.clone();
        envref1.0.get_asset_manager().set_envref(envref.clone());
        envref
    }
}

impl<V: ValueInterface> Environment for SimpleEnvironment<V> {
    type Value = V;
    type CommandExecutor = CommandRegistry<Self>;

    fn get_command_metadata_registry(&self) -> &CommandMetadataRegistry {
        &self.command_registry.command_metadata_registry
    }

    fn get_command_executor(&self) -> &Self::CommandExecutor {
        &self.command_registry
    }

    #[cfg(feature = "async_store")]
    fn get_async_store(&self) -> Arc<Box<dyn crate::store::AsyncStore>> {
        self.async_store.clone()
    }
    
    fn get_asset_manager(
        &self,
    ) -> Arc<Box<DefaultAssetManager<Self>>> {
        self.asset_store.clone()
    }

}
