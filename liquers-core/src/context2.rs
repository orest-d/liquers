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
use std::
    sync::{Arc, Mutex}
;

use crate::{
    assets2::AssetStore,
    cache::Cache,
    command_metadata::CommandMetadataRegistry,
    commands2::{CommandExecutor, CommandRegistry},
    error::Error,
    metadata::MetadataRecord,
    query::{Key, TryToQuery},
    state::State,
    store::{NoStore, Store},
    value::ValueInterface,
};

pub trait Environment: Sized + Sync + Send + 'static {
    type Value: ValueInterface;
    type CommandExecutor: CommandExecutor<EnvRef<Self>, Self::Value, Context<Self>>;
    type AssetStore: AssetStore<Self>;

    fn get_command_metadata_registry(&self) -> &CommandMetadataRegistry;
    fn get_command_executor(&self) -> &Self::CommandExecutor;
    #[cfg(feature = "async_store")]
    fn get_async_store(&self) -> Arc<Box<dyn crate::store::AsyncStore>>;

    fn get_asset_store(
        &self,
    ) -> Arc<
        Box<
            dyn AssetStore<
                Self,
                Asset = <Self::AssetStore as crate::assets2::AssetStore<Self>>::Asset,
            >,
        >,
    >;
}

// TODO: Define Session and User; Session connects multiple actions of a single user.
// TODO: Session could be "SystemSession" for automated tasks or recipes.
// TODO: Remove rwlock
// TODO: Improve interface in envref
pub struct EnvRef<E: Environment>(pub Arc<tokio::sync::RwLock<E>>);

impl<E: Environment> EnvRef<E> {
    pub fn new(env: E) -> Self {
        EnvRef(Arc::new(tokio::sync::RwLock::new(env)))
    }
    #[cfg(feature = "async_store")]
    pub async fn get_async_store(&self) -> Arc<Box<dyn crate::store::AsyncStore>> {
        self.0.read().await.get_async_store()
    }
}

impl<E: Environment> Clone for EnvRef<E> {
    fn clone(&self) -> Self {
        EnvRef(self.0.clone())
    }
}

// TODO: Do we need both Context and ActionContext?
// TODO: There should be an asset reference
pub struct Context<E: Environment> {
    envref: EnvRef<E>,
    metadata: Arc<Mutex<MetadataRecord>>,  // TODO: Decide whether Asset or Context is the Metadata owner
    cwd_key: Arc<Mutex<Option<Key>>>,      // TODO: CWD should be owned by the context or maybe it should be in the Metadata
}

impl<E: Environment> Context<E> {
    pub async fn new(env: EnvRef<E>) -> Self {
        Context {
            envref: env,
            metadata: Arc::new(Mutex::new(MetadataRecord::new())),
            cwd_key: Arc::new(Mutex::new(None)),
        }
    }
}

// TODO: It should be enough to have E as a parameter
impl<E: Environment> ActionContext<EnvRef<E>, E::Value> for Context<E> {
    fn borrow_payload(&self) -> &EnvRef<E> {
        &self.envref
    }
    fn clone_payload(&self) -> EnvRef<E> {
        EnvRef(self.envref.0.clone())
    }
    fn evaluate_dependency<Q: TryToQuery>(&self, query: Q) -> Result<State<E::Value>, Error> {
        todo!("implement evaluate_dependency")
    }
    fn get_metadata(&self) -> MetadataRecord {
        self.metadata.lock().unwrap().clone()
    }
    fn set_filename(&self, filename: String) {
        self.metadata.lock().unwrap().with_filename(filename);
    }
    fn debug(&self, message: &str) {
        self.metadata.lock().unwrap().debug(message);
    }
    fn info(&self, message: &str) {
        self.metadata.lock().unwrap().info(message);
    }
    fn warning(&self, message: &str) {
        self.metadata.lock().unwrap().warning(message);
    }
    fn error(&self, message: &str) {
        self.metadata.lock().unwrap().error(message);
    }
    fn clone_context(&self) -> Self {
        Context {
            envref: self.clone_payload(),
            metadata: self.metadata.clone(),
            cwd_key: self.cwd_key.clone(),
        }
    }
    fn get_cwd_key(&self) -> Option<Key> {
        self.cwd_key.lock().unwrap().clone()
    }

    fn set_cwd_key(&self, key: Option<Key>) {
        let mut guard = self.cwd_key.lock().unwrap();
        *guard = key;
    }
}

// TODO: Think about the Payload. EnvRef and Session should always be available.
// TODO: Add reference to Session
// TODO: Add EnvRef
// TODO: Add progress reporting
// TODO: Should action parameters be in context?
// TODO: There should be a reference to input_state_query
// TODO: There should be a reference to query including the current action
pub trait ActionContext<P, V: ValueInterface> {
    fn borrow_payload(&self) -> &P;
    fn clone_payload(&self) -> P;
    // TODO: evaluate should probably be async
    fn evaluate_dependency<Q: TryToQuery>(&self, query: Q) -> Result<State<V>, Error>;
    fn get_metadata(&self) -> MetadataRecord;
    fn set_filename(&self, filename: String);

    // TODO: There should be a general log entry access 
    fn debug(&self, message: &str);
    fn info(&self, message: &str);
    fn warning(&self, message: &str);
    fn error(&self, message: &str);
    fn clone_context(&self) -> Self; // TODO: clone_context may not need to be available for the action
    fn get_cwd_key(&self) -> Option<Key>;
    fn set_cwd_key(&self, key: Option<Key>); // TODO: set_cwd_key may not need to be available for the action 
}


/// Simple environment with configurable store and cache
/// CommandRegistry is used as command executor as well as it is providing the command metadata registry.
pub struct SimpleNGEnvironment<V: ValueInterface> {
    store: Arc<Box<dyn Store>>,
    #[cfg(feature = "async_store")]
    async_store: Arc<Box<dyn crate::store::AsyncStore>>,
    //cache: Arc<tokio::sync::RwLock<Box<dyn Cache<V>>>>,
    command_registry: CommandRegistry<EnvRef<Self>, V, Context<Self>>,
    asset_store: Option<
        Arc<
            Box<
                (dyn AssetStore<
                    SimpleNGEnvironment<V>,
                    Asset = crate::assets2::AssetRef<SimpleNGEnvironment<V>>,
                > + 'static),
            >,
        >,
    >,
}

impl<V: ValueInterface> SimpleNGEnvironment<V> {
    pub fn new() -> Self {
        SimpleNGEnvironment {
            store: Arc::new(Box::new(NoStore)),
            command_registry: CommandRegistry::new(),
            //            cache: Arc::new(tokio::sync::RwLock::new(Box::new(NoCache::<V>::new()))),
            #[cfg(feature = "async_store")]
            async_store: Arc::new(Box::new(crate::store::NoAsyncStore)),
            asset_store: None,
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
        panic!("SimpleNGEnvironment does not support cache for now");
    }
    pub fn to_ref(self) -> EnvRef<Self> {
        EnvRef::new(self)
    }
    pub async fn ref_with_default_asset_store(mut self) -> EnvRef<Self> {
        let envref = self.to_ref();
        let envref_copy1 = envref.clone();
        let envref_copy2 = envref.clone();
        {
            let mut lock = envref.0.write().await;
            (*lock).asset_store = Some(Arc::new(Box::new(crate::assets2::EnvAssetStore::new(
                envref_copy1,
                crate::recipes2::DefaultRecipeProvider::new(envref_copy2),
            ))));
        }
        envref
    }
}

impl<V: ValueInterface> Environment for SimpleNGEnvironment<V> {
    type Value = V;
    type CommandExecutor = CommandRegistry<EnvRef<Self>, V, Context<Self>>;
    type AssetStore = crate::assets2::EnvAssetStore<Self, crate::recipes2::DefaultRecipeProvider<Self>>;

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

    fn get_asset_store(
        &self,
    ) -> Arc<
        Box<
            (dyn AssetStore<
                SimpleNGEnvironment<V>,
                Asset = crate::assets2::AssetRef<SimpleNGEnvironment<V>>,
            > + 'static),
        >,
    > {
        if let Some(store) = &self.asset_store {
            store.clone()
        } else {
            panic!("Asset store is not set for SimpleNGEnvironment");
        }
    }
}
