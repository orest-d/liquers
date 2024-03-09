#![allow(unused_imports)]
#![allow(dead_code)]

use chrono::format;

use crate::error::Error;
use crate::state::State;
use crate::value::ValueInterface;
use std::collections::HashMap;
use std::marker::PhantomData;
use std::sync::Arc;

use crate::metadata::Metadata;
use crate::query::Query;

/// Definition of Cache interface for binary data
/// Cache is meant to temporarily store results of queries as values.
/// Unlike the complete cache, binary cache does not expose access to the values or states, but only the metadata and serialized values.
/// Unlike Store, Cache is not meant to be a permanent storage, but rather a temporary storage for the results of queries.
/// Store uses Key as a key, while Cache uses a Query.
/// Primary use of Cache is accelerating the evaluation of queries and making short-lived results available via web API.
/// Binary cache interface is enough to implement the cache web API.
pub trait BinCache {
    /// Clean the cache
    /// Empties all the data in the cache
    fn clear(&mut self);
    /// Get a serialized state associated with the key (Query)
    fn get_binary(&self, query: &Query) -> Option<Vec<u8>>;
    /// Get metadata associated with the key
    fn get_metadata(&self, query: &Query) -> Option<Arc<Metadata>>;
    /// Set a state associated with the key
    fn set_binary(&mut self, data: &[u8], metadata: &Metadata) -> Result<(), Error>;
    /// Set metadata associated with the key
    fn set_metadata(&mut self, metadata: &Metadata) -> Result<(), Error>;
    /// Remove a state associated with the key
    fn remove(&mut self, query: &Query) -> Result<(), Error>;
    /// Check whether cache contains the key
    fn contains(&self, query: &Query) -> bool;
    /// List of cached keys
    fn keys(&self) -> Vec<Query>;
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct NoBinCache;

impl BinCache for NoBinCache {
    fn clear(&mut self) {}

    fn get_binary(&self, _query: &Query) -> Option<Vec<u8>> {
        None
    }

    fn get_metadata(&self, _query: &Query) -> Option<Arc<Metadata>> {
        None
    }

    fn set_binary(&mut self, _data: &[u8], metadata: &Metadata) -> Result<(), Error> {
        Err(Error::cache_not_supported().with_query(&metadata.query()?))
    }

    fn set_metadata(&mut self, metadata: &Metadata) -> Result<(), Error> {
        Err(Error::cache_not_supported().with_query(&metadata.query()?))
    }

    fn remove(&mut self, query: &Query) -> Result<(), Error> {
        Err(Error::cache_not_supported().with_query(query))
    }

    fn contains(&self, _query: &Query) -> bool {
        false
    }

    fn keys(&self) -> Vec<Query> {
        Vec::new()
    }
}

#[derive(Debug, Clone)]
pub struct MemoryBinCache(HashMap<Query, (Arc<Metadata>, Option<Vec<u8>>)>);

impl MemoryBinCache {
    pub fn new() -> Self {
        MemoryBinCache(HashMap::new())
    }
}

impl BinCache for MemoryBinCache {
    fn clear(&mut self) {
        self.0.clear();
    }

    fn get_metadata(&self, query: &Query) -> Option<Arc<Metadata>> {
        if let Some((metadata, _)) = self.0.get(query) {
            Some(metadata.clone())
        } else {
            None
        }
    }

    fn set_metadata(&mut self, metadata: &Metadata) -> Result<(), Error> {
        let query = metadata.query()?;
        if let Some((am, _)) = self.0.get_mut(&query) {
            *am = Arc::new(metadata.clone());
        } else {
            self.0.insert(query, (Arc::new(metadata.clone()), None));
        }
        Ok(())
    }

    fn set_binary(&mut self, data: &[u8], metadata: &Metadata) -> Result<(), Error> {
        let query = metadata.query()?;
        if let Some((am, d)) = self.0.get_mut(&query) {
            *am = Arc::new(metadata.clone());
            *d = Some(data.to_vec());
        } else {
            self.0
                .insert(query, (Arc::new(metadata.clone()), Some(data.to_vec())));
        }
        Ok(())
    }

    fn remove(&mut self, query: &Query) -> Result<(), Error> {
        self.0.remove(query);
        Ok(())
    }

    fn contains(&self, query: &Query) -> bool {
        self.0.contains_key(query)
    }

    fn keys(&self) -> Vec<Query> {
        self.0.keys().cloned().collect()
    }

    fn get_binary(&self, query: &Query) -> Option<Vec<u8>> {
        if let Some((_, data)) = self.0.get(query) {
            data.clone()
        } else {
            None
        }
    }
}

pub trait Cache<V:ValueInterface>:BinCache{
    fn get(&self, query:&Query)->Result<State<V>,Error>;
    fn set(&mut self, state:State<V>)->Result<(),Error>;
}

pub struct NoCache<V:ValueInterface>(PhantomData<V>);

impl<V:ValueInterface> NoCache<V>{
    pub fn new()->Self{
        NoCache(PhantomData::default())
    }
}

impl <V:ValueInterface> BinCache for NoCache<V>{
    fn clear(&mut self) {}
    fn get_binary(&self, _query: &Query) -> Option<Vec<u8>> {None}
    fn get_metadata(&self, _query: &Query) -> Option<Arc<Metadata>> {None}
    fn set_binary(&mut self, _data: &[u8], _metadata: &Metadata) -> Result<(), Error> {Err(Error::cache_not_supported())}
    fn set_metadata(&mut self, _metadata: &Metadata) -> Result<(), Error> {Err(Error::cache_not_supported())}
    fn remove(&mut self, _query: &Query) -> Result<(), Error> {Err(Error::cache_not_supported())}
    fn contains(&self, _query: &Query) -> bool {false}
    fn keys(&self) -> Vec<Query> {Vec::new()}
}

impl<V:ValueInterface> Cache<V> for NoCache<V>{
    fn get(&self, _query:&Query)->Result<State<V>,Error> {
        Err(Error::cache_not_supported())
    }
    fn set(&mut self, _state:State<V>)->Result<(),Error> {
        Err(Error::cache_not_supported())
    }
}

pub struct SerializingCache<V:ValueInterface,BC:BinCache>(BC,PhantomData<V>);

impl<V:ValueInterface, BC:BinCache> BinCache for SerializingCache<V, BC>{
    fn clear(&mut self) {
        self.0.clear()
    }

    fn get_binary(&self, query: &Query) -> Option<Vec<u8>> {
        self.0.get_binary(query)
    }

    fn get_metadata(&self, query: &Query) -> Option<Arc<Metadata>> {
        self.0.get_metadata(query)
    }

    fn set_binary(&mut self, data: &[u8], metadata: &Metadata) -> Result<(), Error> {
        self.0.set_binary(data, metadata)
    }

    fn set_metadata(&mut self, metadata: &Metadata) -> Result<(), Error> {
        self.0.set_metadata(metadata)
    }

    fn remove(&mut self, query: &Query) -> Result<(), Error> {
        self.0.remove(query)
    }

    fn contains(&self, query: &Query) -> bool {
        self.0.contains(query)
    }

    fn keys(&self) -> Vec<Query> {
        self.0.keys()
    }
}

impl<V:ValueInterface, BC:BinCache> Cache<V> for SerializingCache<V, BC>{
    fn get(&self, query:&Query)->Result<State<V>,Error> {
        let b = self.get_binary(query).ok_or(Error::not_available().with_query(query))?;
        let metadata = self.get_metadata(query).ok_or(Error::not_available().with_query(query))?;
        let type_identifier = metadata.type_identifier()?;
        let extension = metadata.extension().unwrap_or("b".to_owned()); // TODO: what is the default extension ?
        let value = V::deserialize_from_bytes(&b, &type_identifier, &extension)?;
        Ok(State::from_value_and_metadata(value, metadata))
    }

    fn set(&mut self, state:State<V>)->Result<(),Error> {
        let value = state.data.as_ref();
        if let Some(extension) = state.metadata.extension(){
            let b = state.data.as_bytes(&extension)?;
            self.set_binary(&b, &state.metadata)?;
        }
        else{
            let extension = value.default_extension();
            let b = value.as_bytes(extension.as_ref())?;
            let mut metadata = state.metadata.as_ref().clone();
            metadata.set_extension(&extension)?;
            self.set_binary(&b, &metadata)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Mutex, thread, time::Duration};

    use super::*;
    use crate::parse::parse_query;

    #[test]
    fn test_no_cache() -> Result<(), Error> {
        let cache: NoBinCache = NoBinCache;
        let key = parse_query("-R/key")?;
        assert!(cache.get_metadata(&key).is_none());
        assert_eq!(cache.contains(&key), false);
        Ok(())
    }
    #[test]
    fn test_memory_cache() -> Result<(), Error> {
        let mut cache = MemoryBinCache::new();
        let key = parse_query("-R/key")?;
        assert_eq!(cache.contains(&key), false);
        cache.set_binary(
            "hello".as_bytes(),
            &Metadata::new().with_query(key.to_owned()),
        )?;
        assert_eq!(cache.contains(&key), true);
        assert_eq!(cache.get_binary(&key).is_some(), true);
        Ok(())
    }
    #[test]
    fn test_memory_cache_threaded() -> Result<(), Error> {
        let key = parse_query("-R/key")?;
        let cache = MemoryBinCache::new();
        assert!(cache.get_binary(&key).is_none());
        let cache = Arc::new(Mutex::new(cache));
        let c1 = cache.clone();
        let t1 = thread::spawn(move || {
            if let Ok(mut cache) = c1.lock() {
                let key = parse_query("-R/key").unwrap();
                cache
                    .set_binary(
                        "hello1".as_bytes(),
                        &Metadata::new().with_query(key.to_owned()),
                    )
                    .unwrap();
                assert!(cache.get_metadata(&key).unwrap().query().unwrap() == key);
                println!("T1 CACHED {:?}", cache.get_binary(&key));
            }
        });
        let c2 = cache.clone();
        let t2 = thread::spawn(move || {
            thread::sleep(Duration::from_millis(200));
            if let Ok(mut cache) = c2.lock() {
                let key = parse_query("-R/key").unwrap();
                cache
                    .set_binary(
                        "hello2".as_bytes(),
                        &Metadata::new().with_query(key.to_owned()),
                    )
                    .unwrap();
                println!("T2 CACHED {:?}", cache.get_binary(&key));
            }
        });
        t1.join().unwrap();
        if let Ok(cache) = cache.lock() {
            assert!(cache.contains(&key));
            println!("Joint t1 CACHED {:?}", cache.get_binary(&key));
            assert!(cache.get_binary(&key).is_some());
        } else {
            assert!(false);
        }
        t2.join().unwrap();
        if let Ok(cache) = cache.lock() {
            assert!(cache.contains(&key));
            println!("Joint t2 CACHED {:?}", cache.get_binary(&key));
            assert!(cache.get_binary(&key).is_some());
        } else {
            assert!(false);
        }

        Ok(())
    }
}
