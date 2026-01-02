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
use std::{
    sync::{Arc, Mutex},
};

use futures::FutureExt;

use crate::{
    assets::{
        AssetManager, AssetRef, AssetServiceMessage, DefaultAssetManager,
    },
    cache::Cache,
    command_metadata::CommandMetadataRegistry,
    commands::{CommandExecutor, CommandRegistry},
    error::Error,
    metadata::{LogEntry, MetadataRecord, ProgressEntry},
    query::{Key, Query, TryToQuery},
    recipes::{AsyncRecipeProvider, Recipe},
    state::State,
    store::{NoStore, Store},
    value::ValueInterface,
};

pub enum User {
    System,
    Anonymous,
    Named(String),
}

pub trait Session {
    fn get_user(&self) -> &User;
}

pub trait Environment: Sized + Sync + Send + 'static {
    type Value: ValueInterface;
    type CommandExecutor: CommandExecutor<Self>;
    type SessionType: Session;
    type Payload: Clone +  Send + Sync + 'static;

    fn get_command_metadata_registry(&self) -> &CommandMetadataRegistry;
    fn get_command_executor(&self) -> &Self::CommandExecutor;
    #[cfg(feature = "async_store")]
    fn get_async_store(&self) -> Arc<Box<dyn crate::store::AsyncStore>>;

    fn get_asset_manager(&self) -> Arc<Box<DefaultAssetManager<Self>>>;

    fn get_recipe_provider(&self) -> Arc<Box<dyn AsyncRecipeProvider<Self>>>;

    fn create_session(&self, user: User) -> Self::SessionType;

    fn apply_recipe(
        envref: EnvRef<Self>,
        input_state: State<Self::Value>,
        recipe: Recipe,
        context: Context<Self>,
    ) -> std::pin::Pin<
        Box<dyn core::future::Future<Output = Result<Arc<Self::Value>, Error>> + Send + 'static>,
    >;

    fn init_with_envref(&self, envref: EnvRef<Self>);

    fn to_ref(self) -> EnvRef<Self> {
        let envref = EnvRef::new(self);
        envref.0.init_with_envref(envref.clone());
        envref
    }
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

    pub fn get_asset_manager(&self) -> Arc<Box<DefaultAssetManager<E>>> {
        self.0.get_asset_manager()
    }

    pub fn get_recipe_provider(&self) -> Arc<Box<dyn AsyncRecipeProvider<E>>> {
        self.0.get_recipe_provider()
    }

    pub fn apply_recipe(
        &self,
        input_state: State<E::Value>,
        recipe: Recipe,
        context: Context<E>,
    ) -> std::pin::Pin<
        Box<dyn core::future::Future<Output = Result<Arc<E::Value>, Error>> + Send + 'static>,
    > {
        Box::pin(E::apply_recipe(self.clone(), input_state, recipe, context))
    }

    pub fn evaluate<Q:TryToQuery>(
        &self,
        query: Q,
    ) -> std::pin::Pin<
        Box<dyn core::future::Future<Output = Result<AssetRef<E>, Error>> + Send + 'static>,
    > {
        let envref = self.clone();
        let rquery = query.try_to_query();
        
        async move {
            let asset_manager = envref.get_asset_manager();
            asset_manager.get_asset(&rquery?).await
        }
        .boxed()
    }

    pub fn evaluate_immediately<Q:TryToQuery>(
        &self,
        query: Q,
        payload: E::Payload,
    ) -> std::pin::Pin<
        Box<dyn core::future::Future<Output = Result<AssetRef<E>, Error>> + Send + 'static>,
    > {
        let envref = self.clone();
        let rquery = query.try_to_query();
        
        async move {
            let asset_manager = envref.get_asset_manager();
            let query = rquery?;
            asset_manager.apply_immediately(query.into(), E::Value::none(), Some(payload)).await
        }
        .boxed()
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
    envref: EnvRef<E>,
    cwd_key: Arc<Mutex<Option<Key>>>, // TODO: CWD should be owned by the context or maybe it should be in the Metadata
    service_tx: tokio::sync::mpsc::UnboundedSender<AssetServiceMessage>,
    pub payload: Option<E::Payload>,
}

impl<E: Environment> Context<E> {
    pub async fn new(assetref: AssetRef<E>) -> Self {
        let service_tx = assetref.service_sender().await;
        let envref = assetref.get_envref().await;
        Context {
            assetref,
            envref,
            cwd_key: Arc::new(Mutex::new(None)),
            service_tx,
            payload: None,
        }
    }

    pub fn set_payload(&mut self, payload: E::Payload) {
        self.payload = Some(payload);
    }

    pub fn get_payload_clone(&self) -> Option<E::Payload> {
        self.payload.clone()
    }

    pub async fn evaluate(&self, query: &Query) -> Result<AssetRef<E>, Error> {
        let envref = self.assetref.get_envref().await;
        envref.get_asset_manager().get_asset(query).await
    }

    pub async fn apply(&self, query: &Query, to: E::Value) -> Result<AssetRef<E>, Error> {
        let envref = self.assetref.get_envref().await;
        envref.get_asset_manager().apply(query.into(), to).await
    }

    pub async fn get_metadata(&self) -> Result<MetadataRecord, Error> {
        self.assetref
            .data
            .read()
            .await
            .metadata
            .metadata_record()
            .ok_or(Error::unexpected_error(format!(
                "{} has legacy metadata",
                self.assetref.asset_reference().await
            )))
    }
    pub fn progress(&self, progress: ProgressEntry) -> Result<(), Error> {
        self.service_tx
            .send(AssetServiceMessage::UpdatePrimaryProgress(progress))
            .map_err(|e| Error::general_error(format!("Failed to send progress message: {}", e)))
    }
    pub fn secondary_progress(&self, progress: ProgressEntry) -> Result<(), Error> {
        self.service_tx
            .send(AssetServiceMessage::UpdateSecondaryProgress(progress))
            .map_err(|e| {
                Error::general_error(format!("Failed to send secondary progress message: {}", e))
            })
    }
    pub async fn set_filename(&self, filename: &str) -> Result<(), Error> {
        self.assetref
            .data
            .write()
            .await
            .metadata
            .set_filename(filename)
            .map(|_| ())
    }
    pub fn add_log_entry(&self, entry: LogEntry) -> Result<(), Error> {
        self.service_tx
            .send(AssetServiceMessage::LogMessage(entry))
            .map_err(|e| Error::general_error(format!("Failed to send log message: {}", e)))
    }
    pub fn debug(&self, message: &str) -> Result<(), Error> {
        eprintln!("DEBUG:   {}", message);
        self.add_log_entry(LogEntry::debug(message.to_string()))
    }
    pub fn info(&self, message: &str) -> Result<(), Error> {
        eprintln!("INFO:    {}", message);
        self.add_log_entry(LogEntry::info(message.to_string()))
    }
    pub fn warning(&self, message: &str) -> Result<(), Error> {
        eprintln!("WARNING: {}", message);
        self.add_log_entry(LogEntry::warning(message.to_string()))
    }
    pub fn error(&self, message: &str) -> Result<(), Error> {
        eprintln!("ERROR:   {}", message);
        self.add_log_entry(LogEntry::error(message.to_string()))
    }
    pub async fn clone_context(&self) -> Self {
        Context {
            assetref: self.assetref.clone(),
            envref: self.envref.clone(),
            cwd_key: self.cwd_key.clone(),
            service_tx: self.service_tx.clone(),
            payload: self.payload.clone(),
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

    pub fn get_envref(&self) -> EnvRef<E> {
        self.envref.clone()
    }

    pub(crate) async fn set_value(&self, value: E::Value) -> Result<(), Error> {
        self.assetref.set_value(value).await
    }

    pub(crate) async fn set_metadata_value(&self, metadata: MetadataRecord) -> Result<(), Error> {
        self.assetref
            .set_value(E::Value::from_metadata(metadata))
            .await
    }

    pub(crate) async fn set_state(&self, state: State<E::Value>) -> Result<(), Error> {
        self.assetref.set_state(state).await
    }

    pub async fn set_error(&self, error: Error) -> Result<(), Error> {
        self.assetref.set_error(error).await
    }
}

impl<E: Environment> Clone for Context<E> {
    fn clone(&self) -> Self {
        Context {
            assetref: self.assetref.clone(),
            envref: self.envref.clone(),
            cwd_key: self.cwd_key.clone(),
            service_tx: self.service_tx.clone(),
            payload: self.payload.clone(),
        }
    }
}
// TODO: There should be a reference to input_state_query
// TODO: There should be a reference to query including the current action

pub struct SimpleSession {
    pub user: User,
}
impl Session for SimpleSession {
    fn get_user(&self) -> &User {
        &self.user
    }
}

/// Simple environment with configurable store and cache
/// CommandRegistry is used as command executor as well as it is providing the command metadata registry.
pub struct SimpleEnvironment<V: ValueInterface> {
    store: Arc<Box<dyn Store>>,
    #[cfg(feature = "async_store")]
    async_store: Arc<Box<dyn crate::store::AsyncStore>>,
    //cache: Arc<tokio::sync::RwLock<Box<dyn Cache<V>>>>,
    pub command_registry: CommandRegistry<Self>,
    asset_store: Arc<Box<DefaultAssetManager<Self>>>,
    recipe_provider: Option<Arc<Box<dyn AsyncRecipeProvider<Self>>>>,
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
            asset_store: Arc::new(Box::new(crate::assets::DefaultAssetManager::new())),
            recipe_provider: None,
        }
    }
    pub fn with_store(&mut self, store: Box<dyn Store>) -> &mut Self {
        self.store = Arc::new(store);
        self
    }
    pub fn with_recipe_provider(&mut self, provider: Box<dyn AsyncRecipeProvider<Self>>) -> &mut Self {
        self.recipe_provider = Some(Arc::new(provider));
        self
    }
    #[cfg(feature = "async_store")]
    pub fn with_async_store(&mut self, store: Box<dyn crate::store::AsyncStore>) -> &mut Self {
        self.async_store = Arc::new(store);
        self
    }
    pub fn with_cache(&mut self, _cache: Box<dyn Cache<V>>) -> &mut Self {
        panic!("SimpleEnvironment does not support cache for now");
    }
}

impl<V: ValueInterface> Environment for SimpleEnvironment<V> {
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

    #[cfg(feature = "async_store")]
    fn get_async_store(&self) -> Arc<Box<dyn crate::store::AsyncStore>> {
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
        use crate::interpreter::apply_plan;

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



/// Simple environment with payload and configurable store and cache
/// CommandRegistry is used as command executor as well as it is providing the command metadata registry.
pub struct SimpleEnvironmentWithPayload<V: ValueInterface,P: Clone +  Send + Sync + 'static> {
    store: Arc<Box<dyn Store>>,
    #[cfg(feature = "async_store")]
    async_store: Arc<Box<dyn crate::store::AsyncStore>>,
    //cache: Arc<tokio::sync::RwLock<Box<dyn Cache<V>>>>,
    pub command_registry: CommandRegistry<Self>,
    asset_store: Arc<Box<DefaultAssetManager<Self>>>,
    recipe_provider: Option<Arc<Box<dyn AsyncRecipeProvider<Self>>>>,
    _payload: std::marker::PhantomData<P>,
}

impl<V: ValueInterface,P: Clone +  Send + Sync + 'static> Default for SimpleEnvironmentWithPayload<V,P> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V: ValueInterface,P: Clone +  Send + Sync + 'static> SimpleEnvironmentWithPayload<V,P> {
    pub fn new() -> Self {
        SimpleEnvironmentWithPayload {
            store: Arc::new(Box::new(NoStore)),
            command_registry: CommandRegistry::new(),
            //            cache: Arc::new(tokio::sync::RwLock::new(Box::new(NoCache::<V>::new()))),
            _payload: std::marker::PhantomData::<P>::default(),
            #[cfg(feature = "async_store")]
            async_store: Arc::new(Box::new(crate::store::NoAsyncStore)),
            asset_store: Arc::new(Box::new(crate::assets::DefaultAssetManager::new())),
            recipe_provider: None,
        }
    }
    pub fn with_store(&mut self, store: Box<dyn Store>) -> &mut Self {
        self.store = Arc::new(store);
        self
    }
    pub fn with_recipe_provider(&mut self, provider: Box<dyn AsyncRecipeProvider<Self>>) -> &mut Self {
        self.recipe_provider = Some(Arc::new(provider));
        self
    }
    #[cfg(feature = "async_store")]
    pub fn with_async_store(&mut self, store: Box<dyn crate::store::AsyncStore>) -> &mut Self {
        self.async_store = Arc::new(store);
        self
    }
    pub fn with_cache(&mut self, _cache: Box<dyn Cache<V>>) -> &mut Self {
        panic!("SimpleEnvironment does not support cache for now");
    }
}

impl<V: ValueInterface,P: Clone +  Send + Sync + 'static> Environment for SimpleEnvironmentWithPayload<V,P> {
    type Value = V;
    type CommandExecutor = CommandRegistry<Self>;
    type SessionType = SimpleSession;
    type Payload = P;

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
        use crate::interpreter::apply_plan;

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
