use std::{collections::BTreeSet, sync::Arc};

use async_trait::async_trait;
use scc;
use tokio::sync::{broadcast, RwLock};

use crate::{
    context2::{EnvRef, Environment},
    error::Error,
    interpreter2,
    metadata::{Metadata, Status},
    query::{Key, Query},
    recipes2::{AsyncRecipeProvider, DefaultRecipeProvider, Recipe},
    state::State,
    store::AsyncStore,
    value::DefaultValueSerializer,
};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum AssetMessage {
    StatusChanged(Status),
}

pub struct AssetData<E: Environment> {
    pub query: Query,
    rx: broadcast::Receiver<AssetMessage>,
    tx: broadcast::Sender<AssetMessage>,

    /// This is used to store the data in the asset if available.
    data: Option<Arc<E::Value>>,

    /// This is used to store the binary representation of the data in the asset if available.
    /// If both data and binary is available, they will represent the same data and can be used interchangeably.
    binary: Option<Arc<Vec<u8>>>,

    metadata: Option<Arc<Metadata>>,

    recipe: Option<Recipe>,

    _marker: std::marker::PhantomData<E>,
}

impl<E: Environment> AssetData<E> {
    pub fn new(query: Query) -> Self {
        let (tx, rx) = broadcast::channel(100);
        AssetData {
            query,
            rx,
            tx,
            data: None,
            binary: None,
            metadata: None,
            _marker: std::marker::PhantomData,
            recipe: None,
        }
    }

    pub fn get_query(&self) -> Query {
        self.query.clone()
    }
}

pub struct AssetRef<E: Environment> {
    pub data: Arc<RwLock<AssetData<E>>>,
}

impl<E: Environment> Clone for AssetRef<E> {
    fn clone(&self) -> Self {
        AssetRef {
            data: self.data.clone(),
        }
    }
}
impl<E: Environment> AssetRef<E> {
    pub fn new(data: AssetData<E>) -> Self {
        AssetRef {
            data: Arc::new(RwLock::new(data)),
        }
    }
    pub fn new_from_query(query: Query) -> Self {
        AssetRef {
            data: Arc::new(RwLock::new(AssetData::new(query))),
        }
    }

    /// Deserialize the binary data into the asset's data field.
    /// Returns true if the deserialization was successful.
    async fn deserialize_from_binary(&self) -> Result<bool, Error> {
        let mut lock = self.data.write().await;
        let value = {
            if let (Some(binary), Some(metadata)) = (&lock.binary, &lock.metadata) {
                let type_identifier = metadata.as_ref().type_identifier()?;
                let extension = metadata.extension().unwrap_or("bin".to_string());
                E::Value::deserialize_from_bytes(binary, &type_identifier, &extension)
            } else {
                return Ok(false);
            }
        }?;

        lock.data = Some(Arc::new(value));
        Ok(true)
    }

    /// Load the binary data from store if not already loaded.
    /// Only works for assets with query being a key without realm.
    /// Returns true if the binary data was present or loaded.
    async fn try_load_binary_if_necessary(&self, envref: EnvRef<E>) -> Result<bool, Error> {
        let mut lock = self.data.write().await;
        if lock.binary.is_some() {
            return Ok(true);
        }
        if let Some(key) = lock.query.key() {
            let store = envref.get_async_store();
            let (data, metadata) = store.get(&key).await?;
            lock.binary = Some(Arc::new(data));
            lock.metadata = Some(Arc::new(metadata));
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Try to create data from a recipe
    async fn try_create_from_recipe(&self, envref: EnvRef<E>) -> Result<bool, Error> {
        let mut lock = self.data.write().await;
        if lock.recipe.is_none() {
            return Ok(false);
        }
        let recipe = lock.recipe.as_ref().unwrap();

        let envref1 = envref.clone();
        let plan = {
            let cmr = envref1.0.get_command_metadata_registry();
            recipe.to_plan(cmr)
        }?;

        let cwd_key = lock.query.key().map_or_else(|| None, |k| Some(k.parent()));

        let res = interpreter2::ngi::evaluate_plan(plan, envref, cwd_key).await?;
        lock.data = Some(res.data.clone());
        lock.metadata = Some(res.metadata.clone());
        lock.binary = None;
        Ok(true)
    }

    pub async fn get_state_if_available(&self) -> Result<Option<State<E::Value>>, Error> {
        let lock = self.data.read().await;
        if let (Some(data), Some(metadata)) = (&lock.data, &lock.metadata) {
            return Ok(Some(State {
                data: data.clone(),
                metadata: metadata.clone(),
            }));
        } else if let (Some(binary), Some(metadata)) = (&lock.binary, &lock.metadata) {
            todo!("Implement conversion from binary to State");
        }
        Ok(None)
    }

    pub async fn get_state(&self, envref: EnvRef<E>) -> Result<State<E::Value>, Error> {
        if let Some(state) = self.get_state_if_available().await? {
            Ok(state)
        } else {
            if self.try_load_binary_if_necessary(envref.clone()).await? && self.deserialize_from_binary().await? {
                if let Some(state) = self.get_state_if_available().await? {
                    // TODO: Dispose binary if too long
                    return Ok(state);
                }
            }
            if self.try_create_from_recipe(envref.clone()).await? {
                if let Some(state) = self.get_state_if_available().await? {
                    return Ok(state);
                }
            }
            let mut lock = self.data.write().await;
            let query = lock.get_query();
            let plan = interpreter2::ngi::make_plan(envref.clone(), query.clone())?;
            let res = interpreter2::ngi::evaluate_plan(plan, envref.clone(), None).await?;
            lock.data = Some(res.data.clone());
            lock.metadata = Some(res.metadata.clone());
            lock.binary = None;
            Ok(res)
        }
    }
}

#[async_trait]
pub trait AssetInterface<E: Environment>: Send + Sync {
    async fn get_query(&self) -> Query;
    async fn message_receiver(&self) -> broadcast::Receiver<AssetMessage>;
    async fn get_state(&self, envref: EnvRef<E>) -> Result<State<E::Value>, Error>;
}

#[async_trait]
impl<E: Environment> AssetInterface<E> for AssetRef<E> {
    async fn get_query(&self) -> Query {
        let lock = self.data.read();
        lock.await.query.clone()
    }
    async fn message_receiver(&self) -> broadcast::Receiver<AssetMessage> {
        let lock = self.data.read().await;
        lock.tx.subscribe()
    }
    async fn get_state(&self, envref: EnvRef<E>) -> Result<State<E::Value>, Error> {
        self.get_state(envref).await
    }
}

#[async_trait]
pub trait AssetManager<E: Environment>: Send + Sync {
    type Asset: AssetInterface<E>;
    async fn get_asset_if_exists(&self, query: &Query) -> Result<Self::Asset, Error>;
    async fn get_asset(&self, query: Query) -> Result<Self::Asset, Error>;
    async fn assets_list(&self) -> Result<Vec<Query>, Error>;
    async fn contains_asset(&self, query: &Query) -> Result<bool, Error>;
}

#[async_trait]
pub trait AssetStore<E: Environment>: Send + Sync {
    type Asset: AssetInterface<E>;
    async fn get_asset(&self, query: &Query) -> Result<Self::Asset, Error>;
    async fn get(&self, key: &Key) -> Result<Self::Asset, Error>;
    async fn create(&self, key: &Key) -> Result<Self::Asset, Error>;
    async fn remove(&self, key: &Key) -> Result<(), Error>;
    /// Returns true if store contains the key.
    async fn contains(&self, key: &Key) -> Result<bool, Error>;

    /// List or iterator of all keys
    async fn keys(&self) -> Result<Vec<Key>, Error>;

    /// Return names inside a directory specified by key.
    /// To get a key, names need to be joined with the key (key/name).
    /// Complete keys can be obtained with the listdir_keys method.
    async fn listdir(&self, _key: &Key) -> Result<Vec<String>, Error>;

    /// Return keys inside a directory specified by key.
    /// Only keys present directly in the directory are returned,
    /// subdirectories are not traversed.
    async fn listdir_keys(&self, key: &Key) -> Result<Vec<Key>, Error>;

    /// Return asset info of assets inside a directory specified by key.
    /// Return keys inside a directory specified by key.
    /// Keys directly in the directory are returned,
    /// as well as in all the subdirectories.
    async fn listdir_keys_deep(&self, key: &Key) -> Result<Vec<Key>, Error>;

    /// Make a directory
    async fn makedir(&self, key: &Key) -> Result<Self::Asset, Error>;
}

pub struct DefaultAssetStore<E: Environment> {
    envref: std::sync::OnceLock<EnvRef<E>>,
    assets: scc::HashMap<Key, AssetRef<E>>,
    query_assets: scc::HashMap<Query, AssetRef<E>>,
    recipe_provider: std::sync::OnceLock<DefaultRecipeProvider<E>>,
}

impl<E: Environment> Default for DefaultAssetStore<E> {
    fn default() -> Self {
        Self::new()
    }
}

impl<E: Environment> DefaultAssetStore<E> {
    pub fn new() -> Self {
        DefaultAssetStore {
            envref: std::sync::OnceLock::new(),
            assets: scc::HashMap::new(),
            query_assets: scc::HashMap::new(),
            recipe_provider: std::sync::OnceLock::new(),
        }
    }
    pub fn get_envref(&self) -> EnvRef<E> {
        self.envref.get().expect("Environment not set in AssetStore").clone()
    }

    pub fn set_envref(&self, envref: EnvRef<E>) {
        if self.envref.set(envref.clone()).is_err() {
            panic!("Environment already set in AssetStore");
        }
        self.recipe_provider.set(DefaultRecipeProvider::new(envref));
    }

    pub fn get_recipe_provider(&self) -> &DefaultRecipeProvider<E> {
        self.recipe_provider.get().expect("Recipe provider not set in AssetStore")
    }
}

#[async_trait]
impl<E: Environment> AssetStore<E> for DefaultAssetStore<E> {
    type Asset = AssetRef<E>;
    
    async fn get_asset(&self, query: &Query) -> Result<Self::Asset, Error> {
        if let Some(key) = query.key() {
            self.get(&key).await
        } else {
            let entry = self
                .query_assets
                .entry_async(query.clone())
                .await
                .or_insert_with(|| AssetRef::<E>::new_from_query(query.clone()));
            Ok(entry.get().clone())
        }
    }

    async fn get(&self, key: &Key) -> Result<Self::Asset, Error> {
        let entry = self
            .assets
            .entry_async(key.clone())
            .await
            .or_insert_with(|| AssetRef::<E>::new_from_query(key.clone().into()));

        let asset_ref = entry.get().clone();

        // Try to get a recipe from the recipe provider and set it in the asset if available
        if let Ok(recipe) = self
            .get_recipe_provider()
            .recipe(key)
            .await
        {
            let mut lock = asset_ref.data.write().await;
            lock.recipe = Some(recipe);
        }

        Ok(asset_ref)
    }

    async fn create(&self, key: &Key) -> Result<Self::Asset, Error> {
        self.get(key).await
    }

    async fn remove(&self, _key: &Key) -> Result<(), Error> { // TODO: Does nothing??
        Ok(())
    }

    async fn contains(&self, key: &Key) -> Result<bool, Error> {
        let store = self.get_envref().get_async_store();
        if store.contains(key).await? {
            return Ok(true);
        }
        self.get_recipe_provider().contains(key).await
    }

    async fn keys(&self) -> Result<Vec<Key>, Error> {
        self.listdir_keys_deep(&Key::new()).await
    }

    async fn listdir(&self, key: &Key) -> Result<Vec<String>, Error> {
        let store = self.get_envref().get_async_store();
        let mut names = self
            .get_recipe_provider()
            .assets_with_recipes(key)
            .await?
            .into_iter()
            .map(|resourcename| resourcename.name)
            .collect::<BTreeSet<String>>();
        store.listdir(key).await?.into_iter().for_each(|name| {
            names.insert(name);
        });

        Ok(names.into_iter().collect())
    }

    async fn listdir_keys(&self, key: &Key) -> Result<Vec<Key>, Error> {
        Ok(self
            .listdir(key)
            .await?
            .into_iter()
            .map(|name| key.join(name))
            .collect::<Vec<Key>>())
    }

    async fn listdir_keys_deep(&self, key: &Key) -> Result<Vec<Key>, Error> {
        let store = self.get_envref().get_async_store();

        let mut keys = store
            .listdir_keys_deep(key)
            .await?
            .into_iter()
            .collect::<BTreeSet<Key>>();
        let mut folders = vec![];
        for k in keys.iter() {
            if store.is_dir(k).await? {
                folders.push(k.clone());
            }
        }

        for subkey in folders {
            if store.is_dir(&subkey).await? {
                let recipes = self.get_recipe_provider().assets_with_recipes(&subkey).await?;
                for resourcename in recipes {
                    keys.insert(subkey.join(resourcename.name));
                }
            }
        }

        Ok(keys.into_iter().collect())
    }

    async fn makedir(&self, key: &Key) -> Result<Self::Asset, Error> {
        let store = self.get_envref().get_async_store();
        let _sink = store.makedir(key).await?;
        let asset = self.get(key).await?;
        Ok(asset)
    }
}
