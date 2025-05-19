use std::sync::Arc;

use async_trait::async_trait;
use nom::Err;
use scc;

use crate::{
    context::{NGEnvRef, NGEnvironment}, error::Error, interpreter::NGPlanInterpreter, metadata::Metadata, query::{Key, Query}, recipes::AsyncRecipeProvider, state::State, store::AsyncStore, value::{DefaultValueSerializer, ValueInterface}
};


pub struct Asset<E: NGEnvironment> {
    pub query: Query,
    _marker: std::marker::PhantomData<E>,
}

pub trait AssetInterface<E: NGEnvironment> {
    fn is_dir(&self) -> bool;
}

impl<E: NGEnvironment> AssetInterface<E> for Asset<E> {
    fn is_dir(&self) -> bool {
        false
    }
}

#[async_trait]
pub trait AssetManager<E: NGEnvironment> : Send + Sync {
    type Asset: AssetInterface<E>;
    async fn get_asset(&self, query:&Query) -> Result<Self::Asset, Error>;
    async fn create_asset(&self, query:Query) -> Result<Self::Asset, Error>;
    async fn assets_list(&self) -> Result<Vec<Query>, Error>;
    async fn contains_asset(&self, query:&Query) -> Result<bool, Error>;
}

#[async_trait]
pub trait AssetStore<E: NGEnvironment> : Send + Sync {
    type Asset: AssetInterface<E>;
    async fn get(&self, key:&Key) -> Result<Self::Asset, Error>;
    async fn create(&self, key:&Key) -> Result<Self::Asset, Error>;
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

struct EnvAssetStore<E: NGEnvironment> {
    envref: NGEnvRef<E>,
}
impl<E: NGEnvironment> EnvAssetStore<E> {
    pub fn new(envref: NGEnvRef<E>) -> Self {
        EnvAssetStore { envref }
    }
}

#[async_trait]
impl<E:NGEnvironment> AssetStore<E> for EnvAssetStore<E> {
    type Asset = Asset<E>;

    async fn get(&self, key: &Key) -> Result<Self::Asset, Error> {
        Ok(Asset::<E>{
            query: key.clone().into(),
            _marker: std::marker::PhantomData
        })
    }

    async fn create(&self, key: &Key) -> Result<Self::Asset, Error> {
        Ok(Asset::<E>{
            query: key.clone().into(),
            _marker: std::marker::PhantomData
        })
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

    async fn makedir(&self, key: &Key) -> Result<Asset<E>, Error> {
        //let store = self.envref.get_async_store().await;
        //store.makedir(key).await
        Err(Error::general_error(format!("makedir not implemented for EnvAssetStore")))
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
pub trait AsyncAssets<E: NGEnvironment> : Send + Sync {
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
impl <E: NGEnvironment, ARP: AsyncRecipeProvider> AsyncAssets<E> for DefaultAssets<E, ARP> {
    async fn get(&self, key: &Key) -> Result<(Vec<u8>, Metadata), Error> {
        let store = self.envref.get_async_store().await;
        match store.get(key).await {
            Ok((data, metadata)) => Ok((data, metadata)),
            Err(e) => {
                let plan = self.recipe_provider.recipe_plan(key).await.map_err(|e2| Error::general_error(
                    format!("Asset {key} not found ({e}, {e2})")
                ))?;
                let state = crate::interpreter::ngi::evaluate_plan(plan, self.envref.clone(), Some(key.parent())).await?;
                let data = state.as_bytes()?;
                store.set(key, &data, &state.metadata).await?;
                Ok((data, (*state.metadata).clone()))
            }
        }
    }

    async fn get_state(&self, key: &Key) -> Result<State<E::Value>, Error> {
        let store = self.envref.get_async_store().await;
        match store.get(key).await{
            // TODO: Handle the case with metadata without data
            Ok((data, metadata)) => {
                let type_identifier = metadata.type_identifier()?;
                let value = E::Value::deserialize_from_bytes(&data, &type_identifier, &metadata.get_data_format())?;
                return Ok(State::from_value_and_metadata(value, Arc::new(metadata)));
            }
            Err(e) => {
                let plan = self.recipe_provider.recipe_plan(key).await.map_err(|e2| Error::general_error(
                    format!("Asset {key} not found ({e}, {e2})") // TODO: make own error type
                ))?;
                let state = crate::interpreter::ngi::evaluate_plan(plan, self.envref.clone(), Some(key.parent())).await?;
                let data = state.as_bytes()?;
                store.set(key, &data, &state.metadata).await?;
                return Ok(state);
            }
        }
    }
    
    async fn get_bytes(&self, key: &Key) -> Result<Vec<u8>, Error> {
        let store = self.envref.get_async_store().await;
        match store.get_bytes(key).await{
            // TODO: Handle the case with metadata without data
            Ok(data) => {
                return Ok(data);
            }
            Err(e) => {
                let plan = self.recipe_provider.recipe_plan(key).await.map_err(|e2| Error::general_error(
                    format!("Asset {key} not found ({e}, {e2})")
                ))?;
                let state = crate::interpreter::ngi::evaluate_plan(plan, self.envref.clone(), Some(key.parent())).await?;
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
            if !dir.contains(&resourcename.name){
                dir.push(resourcename.name);
            }
        }
        Ok(dir)

    }
}