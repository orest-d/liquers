use std::sync::Arc;

use crate::{
    context::{NGEnvRef, NGEnvironment},
    error::Error,
    query::Key,
    recipes::AsyncRecipeProvider,
    state::State,
    store::AsyncStore,
    value::DefaultValueSerializer
};

pub trait AsyncAssets<E: NGEnvironment> {
    async fn get_state(&self, key: &Key) -> Result<State<E::Value>, Error>;
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
impl <E: NGEnvironment, ARP: AsyncRecipeProvider> AsyncAssets<E> for DefaultAssets<E, ARP> {
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
                    format!("Asset {key} not found ({e}, {e2})")
                ))?;
                let env = self.envref.0.read().await;
                //env.evaluate(query)
                //let state = State::new(E::Value::from_plan(&plan));
                let state = State::new();
                let data = state.as_bytes()?;
                store.set(key, &data, &state.metadata).await?;
                return Ok(state);
            }
        }
    }
}