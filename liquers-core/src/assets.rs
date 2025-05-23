use std::sync::Arc;

use async_trait::async_trait;
use nom::Err;
use scc;
use tokio::sync::{RwLock, broadcast};

use crate::{
    context::{NGEnvRef, NGEnvironment},
    error::Error,
    interpreter::NGPlanInterpreter,
    metadata::{Metadata, Status},
    query::{Key, Query},
    recipes::AsyncRecipeProvider,
    state::State,
    store::AsyncStore,
    value::{DefaultValueSerializer, ValueInterface},
};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum AssetMessage{
    StatusChanged(Status)
}

pub struct AssetData<E: NGEnvironment> {
    pub query: Query,
    rx: broadcast::Receiver<AssetMessage>,
    tx: broadcast::Sender<AssetMessage>,

    _marker: std::marker::PhantomData<E>,
}

impl<E: NGEnvironment> AssetData<E> {
    pub fn new(query: Query) -> Self {
        let (tx, rx) = broadcast::channel(100);
        AssetData {
            query,
            rx,
            tx,
            _marker: std::marker::PhantomData,
        }
    }

    pub fn get_query(&self) -> Query {
        self.query.clone()
    }
}

pub struct AssetRef<E: NGEnvironment> {
    pub data: Arc<RwLock<AssetData<E>>>,
}

impl<E:NGEnvironment> Clone for AssetRef<E> {
    fn clone(&self) -> Self {
        AssetRef {
            data: self.data.clone(),
        }
    }
}
impl<E: NGEnvironment> AssetRef<E> {
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
}

#[async_trait]
pub trait AssetInterface<E: NGEnvironment>: Send + Sync {
    async fn get_query(&self) -> Query;
    async fn message_receiver(&self) -> broadcast::Receiver<AssetMessage>;
}

#[async_trait]
impl<E: NGEnvironment> AssetInterface<E> for AssetRef<E> {
    async fn get_query(&self) -> Query {
        let lock = self.data.read();
        lock.await.query.clone()
    }
    async fn message_receiver(&self) -> broadcast::Receiver<AssetMessage> {
        let lock = self.data.read().await;
        lock.tx.subscribe()
    }
}

#[async_trait]
pub trait AssetManager<E: NGEnvironment>: Send + Sync {
    type Asset: AssetInterface<E>;
    async fn get_asset_if_exists(&self, query: &Query) -> Result<Self::Asset, Error>;
    async fn get_asset(&self, query: Query) -> Result<Self::Asset, Error>;
    async fn assets_list(&self) -> Result<Vec<Query>, Error>;
    async fn contains_asset(&self, query: &Query) -> Result<bool, Error>;
}

#[async_trait]
pub trait AssetStore<E: NGEnvironment>: Send + Sync {
    type Asset: AssetInterface<E>;
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

pub struct EnvAssetStore<E: NGEnvironment> {
    envref: NGEnvRef<E>,
    assets: scc::HashMap<Key, AssetRef<E>>,
}

impl<E: NGEnvironment> EnvAssetStore<E> {
    pub fn new(envref: NGEnvRef<E>) -> Self {
        EnvAssetStore { envref , assets: scc::HashMap::new() }
    }
}

#[async_trait]
impl<E: NGEnvironment> AssetStore<E> for EnvAssetStore<E> {
    type Asset = AssetRef<E>;

    async fn get(&self, key: &Key) -> Result<Self::Asset, Error> {

        let entry = self.assets.entry_async(key.clone()).await
            .or_insert_with(
                || AssetRef::<E>::new_from_query(key.clone().into())
            );
        
        Ok(
            entry.get().clone()
        )
    }

    async fn create(&self, key: &Key) -> Result<Self::Asset, Error> {
        self.get(key).await
    }

    async fn remove(&self, key: &Key) -> Result<(), Error> {
        Ok(())
    }

    async fn contains(&self, key: &Key) -> Result<bool, Error> {
        let store = self.envref.get_async_store().await;
        store.contains(key).await
    }

    async fn keys(&self) -> Result<Vec<Key>, Error> {
        let store = self.envref.get_async_store().await;
        store.keys().await
    }

    async fn listdir(&self, key: &Key) -> Result<Vec<String>, Error> {
        let store = self.envref.get_async_store().await;
        store.listdir(key).await
    }

    async fn listdir_keys(&self, key: &Key) -> Result<Vec<Key>, Error> {
        let store = self.envref.get_async_store().await;
        store.listdir_keys(key).await
    }

    async fn listdir_keys_deep(&self, key: &Key) -> Result<Vec<Key>, Error> {
        let store = self.envref.get_async_store().await;
        store.listdir_keys_deep(key).await
    }

    async fn makedir(&self, key: &Key) -> Result<Self::Asset, Error> {
        //let store = self.envref.get_async_store().await;
        //store.makedir(key).await
        Err(Error::general_error(format!(
            "makedir not implemented for EnvAssetStore"
        )))
    }
}

/*
pub struct SccHashMapAssetStore {
    store: scc::HashMap<Key, Asset>,
}

impl SccHashMapAssetStore {
    pub fn new() -> Self {
        SccHashMapAssetStore {
            store: scc::HashMap::new(),
        }
    }
}

impl AssetStore for SccHashMapAssetStore {
    async fn get(&self, key: &Key) -> Result<Asset, Error> {
        self.store.get(key).ok_or(Error::general_error(format!("Key {key} not found")))
    }

    async fn create(&self, key: &Key) -> Result<Asset, Error> {
        let asset = Asset { query: Query::new() };
        self.store.insert(key.clone(), asset.clone());
        Ok(asset)
    }

    async fn remove(&self, key: &Key) -> Result<(), Error> {
        self.store.remove(key);
        Ok(())
    }

    async fn contains(&self, key: &Key) -> Result<bool, Error> {
        Ok(self.store.contains_key(key))
    }

    async fn keys(&self) -> Result<Vec<Key>, Error> {
        Ok(self.store.keys().cloned().collect())
    }

    async fn listdir(&self, _key: &Key) -> Result<Vec<String>, Error> {
        Ok(vec![])
    }

    async fn listdir_keys(&self, _key: &Key) -> Result<Vec<Key>, Error> {
        Ok(vec![])
    }

    async fn listdir_keys_deep(&self, _key: &Key) -> Result<Vec<Key>, Error> {
        Ok(vec![])
    }

    async fn makedir(&self, _key: &Key) -> Result<Asset, Error> {
        Ok(Asset { query: Query::new() })
    }
}
*/

#[async_trait]
pub trait AsyncAssets<E: NGEnvironment>: Send + Sync {
    async fn get(&self, key: &Key) -> Result<(Vec<u8>, Metadata), Error>;
    async fn get_state(&self, key: &Key) -> Result<State<E::Value>, Error>;
    async fn get_bytes(&self, key: &Key) -> Result<Vec<u8>, Error>;
    /// Return names inside a directory specified by key.
    /// To get a key, names need to be joined with the key (key/name).
    /// Complete keys can be obtained with the listdir_keys method.
    async fn listdir(&self, _key: &Key) -> Result<Vec<String>, Error>;
}

pub struct DefaultAssets<E: NGEnvironment, ARP: AsyncRecipeProvider> {
    pub(crate) envref: NGEnvRef<E>,
    recipe_provider: ARP,
}

impl<E: NGEnvironment, ARP: AsyncRecipeProvider> DefaultAssets<E, ARP> {
    pub fn new(envref: NGEnvRef<E>, recipe_provider: ARP) -> Self {
        DefaultAssets {
            envref,
            recipe_provider,
        }
    }
}

// TODO: This whole think is a mess. Asset needs to be properly implemented to be able to handle concurrent access
// Asset needs some locking or transaction-like processing
#[async_trait]
impl<E: NGEnvironment, ARP: AsyncRecipeProvider> AsyncAssets<E> for DefaultAssets<E, ARP> {
    async fn get(&self, key: &Key) -> Result<(Vec<u8>, Metadata), Error> {
        let store = self.envref.get_async_store().await;
        match store.get(key).await {
            Ok((data, metadata)) => Ok((data, metadata)),
            Err(e) => {
                let plan = self.recipe_provider.recipe_plan(key).await.map_err(|e2| {
                    Error::general_error(format!("Asset {key} not found ({e}, {e2})"))
                })?;
                let state = crate::interpreter::ngi::evaluate_plan(
                    plan,
                    self.envref.clone(),
                    Some(key.parent()),
                )
                .await?;
                let data = state.as_bytes()?;
                store.set(key, &data, &state.metadata).await?;
                Ok((data, (*state.metadata).clone()))
            }
        }
    }

    async fn get_state(&self, key: &Key) -> Result<State<E::Value>, Error> {
        let store = self.envref.get_async_store().await;
        match store.get(key).await {
            // TODO: Handle the case with metadata without data
            Ok((data, metadata)) => {
                let type_identifier = metadata.type_identifier()?;
                let value = E::Value::deserialize_from_bytes(
                    &data,
                    &type_identifier,
                    &metadata.get_data_format(),
                )?;
                return Ok(State::from_value_and_metadata(value, Arc::new(metadata)));
            }
            Err(e) => {
                let plan = self.recipe_provider.recipe_plan(key).await.map_err(|e2| {
                    Error::general_error(
                        format!("Asset {key} not found ({e}, {e2})"), // TODO: make own error type
                    )
                })?;
                let state = crate::interpreter::ngi::evaluate_plan(
                    plan,
                    self.envref.clone(),
                    Some(key.parent()),
                )
                .await?;
                let data = state.as_bytes()?;
                store.set(key, &data, &state.metadata).await?;
                return Ok(state);
            }
        }
    }

    async fn get_bytes(&self, key: &Key) -> Result<Vec<u8>, Error> {
        let store = self.envref.get_async_store().await;
        match store.get_bytes(key).await {
            // TODO: Handle the case with metadata without data
            Ok(data) => {
                return Ok(data);
            }
            Err(e) => {
                let plan = self.recipe_provider.recipe_plan(key).await.map_err(|e2| {
                    Error::general_error(format!("Asset {key} not found ({e}, {e2})"))
                })?;
                let state = crate::interpreter::ngi::evaluate_plan(
                    plan,
                    self.envref.clone(),
                    Some(key.parent()),
                )
                .await?;
                let data = state.as_bytes()?;
                store.set(key, &data, &state.metadata).await?;
                return Ok(data);
            }
        }
    }

    async fn listdir(&self, key: &Key) -> Result<Vec<String>, Error> {
        let store = self.envref.get_async_store().await;
        let mut dir = store.listdir(key).await?;
        for resourcename in self.recipe_provider.assets_with_recipes(key).await? {
            if !dir.contains(&resourcename.name) {
                dir.push(resourcename.name);
            }
        }
        Ok(dir)
    }
}
