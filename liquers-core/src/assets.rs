use std::sync::Arc;

use async_trait::async_trait;

use crate::{
    context::{NGEnvRef, NGEnvironment}, error::Error, interpreter::NGPlanInterpreter, metadata::Metadata, query::Key, recipes::AsyncRecipeProvider, state::State, store::AsyncStore, value::DefaultValueSerializer
};

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
                let state = NGPlanInterpreter::run_plan(plan, self.envref.clone()).await?;
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
                let state = NGPlanInterpreter::run_plan(plan, self.envref.clone()).await?;
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
                let state = NGPlanInterpreter::run_plan(plan, self.envref.clone()).await?;
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