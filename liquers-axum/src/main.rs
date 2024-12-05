use std::sync::{Arc};
use tokio::sync::{RwLock};

use axum::http::{header, StatusCode};
use axum::response::Response;
use axum::body::Body;
use axum::{routing::get, Json, Router};
use axum::response::IntoResponse;
use liquers_core::store::{FileStore, Store, AsyncStore, AsyncStoreWrapper};
use liquers_core::parse::{parse_query, parse_key};
use liquers_core::query::{Key, Query};
use liquers_core::metadata::{Metadata, MetadataRecord};
use axum::extract::{Path, State};
use async_trait::async_trait;
use liquers_core::error::Error;
//use futures::lock::Mutex;

/*
#[cfg(feature = "async_store")]
#[async_trait]
pub trait AsyncStore: Send + Sync{
    /// Get store name
    fn store_name(&self) -> String {
        format!("{} Store", self.key_prefix())
    }

    /// Key prefix common to all keys in this store.
    fn key_prefix(&self) -> Key {
        Key::new()
    }

    /// Create default metadata object for a given key
    fn default_metadata(&self, _key: &Key, _is_dir: bool) -> MetadataRecord {
        MetadataRecord::new()
    }

    /// Finalize metadata before storing - when data is available
    /// This can't be a directory
    fn finalize_metadata(
        &self,
        metadata: Metadata,
        _key: &Key,
        _data: &[u8],
        _update: bool,
    ) -> Metadata {
        metadata
    }

    /// Finalize metadata before storing - when data is not available
    fn finalize_metadata_empty(
        &self,
        metadata: Metadata,
        _key: &Key,
        _is_dir: bool,
        _update: bool,
    ) -> Metadata {
        metadata
    }

    /// Get data asynchronously
    async fn get(&self, key: &Key) -> Result<(Vec<u8>, Metadata), Error>;

    /// Get data as bytes
    async fn get_bytes(&self, key: &Key) -> Result<Vec<u8>, Error> {
        self.get(key).await.map(|(data, _)| data)
    }

    /// Get metadata
    async fn get_metadata(&self, key: &Key) -> Result<Metadata, Error> {
        self.get(key).await.map(|(_, metadata)| metadata)
    }

    /// Store data and metadata.
    async fn set(&mut self, key: &Key, _data: &[u8], _metadata: &Metadata) -> Result<(), Error> {
        Err(Error::key_not_supported(key, &self.store_name()))
    }

    /// Store metadata only
    async fn set_metadata(&mut self, key: &Key, metadata: &Metadata) -> Result<(), Error>;

    /// Remove data and metadata associated with the key
    async fn remove(&mut self, key: &Key) -> Result<(), Error> {
        Err(Error::key_not_supported(key, &self.store_name()))
    }

    /// Remove directory.
    /// The key must be a directory.
    /// It depends on the underlying store whether the directory must be empty.    
    async fn removedir(&mut self, key: &Key) -> Result<(), Error> {
        Err(Error::key_not_supported(key, &self.store_name()))
    }

    /// Returns true if store contains the key.
    async fn contains(&self, _key: &Key) -> Result<bool, Error> {
        Ok(false)
    }

    /// Returns true if key points to a directory.
    async fn is_dir(&self, _key: &Key) -> Result<bool, Error> {
        Ok(false)
    }

    /// List or iterator of all keys
    async fn keys(&self) -> Result<Vec<Key>, Error> {
        let mut keys = self.listdir_keys_deep(&self.key_prefix()).await?;
        keys.push(self.key_prefix().to_owned());
        Ok(keys)
    }

    /// Return names inside a directory specified by key.
    /// To get a key, names need to be joined with the key (key/name).
    /// Complete keys can be obtained with the listdir_keys method.
    async fn listdir(&self, _key: &Key) -> Result<Vec<String>, Error> {
        Ok(vec![])
    }

    /// Return keys inside a directory specified by key.
    /// Only keys present directly in the directory are returned,
    /// subdirectories are not traversed.
    async fn listdir_keys(&self, key: &Key) -> Result<Vec<Key>, Error> {
        let names = self.listdir(key).await?;
        Ok(names.iter().map(|x| key.join(x)).collect())
    }

    /// Return keys inside a directory specified by key.
    /// Keys directly in the directory are returned,
    /// as well as in all the subdirectories.
    async fn listdir_keys_deep(&self, key: &Key) -> Result<Vec<Key>, Error> {
        let keys = self.listdir_keys(key).await?;
        let mut keys_deep = keys.clone();
        for sub_key in keys {
            if self.is_dir(&key).await? {
                let sub = self.listdir_keys_deep(&sub_key).await?;
                keys_deep.extend(sub.into_iter());
            }
        }
        Ok(keys_deep)
    }

    /// Make a directory
    async fn makedir(&self, key: &Key) -> Result<(), Error> {
        Err(Error::key_not_supported(key, &self.store_name()))
    }

    // TODO: implement openbin
    /*
    def openbin(self, key, mode="r", buffering=-1):
        """Return a file handle.
        This is not necessarily always well supported, but it is required to support PyFilesystem2."""
        raise KeyNotSupportedStoreException(key=key, store=self)
    */

    /// Returns true when this store supports the supplied key.
    /// This allows layering Stores, e.g. by with_overlay, with_fallback
    /// and store selectively certain data (keys) in certain stores.
    fn is_supported(&self, _key: &Key) -> bool {
        false
    }
}

#[cfg(feature = "async_store")]
pub struct AsyncStoreWrapper<T: Store>(pub Arc<Mutex<T>>);

impl<T:Store + Clone> Clone for AsyncStoreWrapper<T> {
    fn clone(&self) -> Self {
        AsyncStoreWrapper(self.0.clone())
    }
}
*/

pub struct CoreError(liquers_core::error::Error);

impl From<liquers_core::error::Error> for CoreError {
    fn from(e: liquers_core::error::Error) -> Self {
        CoreError(e)
    }
}

impl IntoResponse for CoreError {
    fn into_response(self) -> Response<Body> {
        // TODO: make error specific response
        Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .header(header::CONTENT_TYPE, "text/plain".to_owned())
            .body(format!("Error: {}", self.0).into())
            .unwrap()
    }
}

pub struct DataResultWrapper(Result<(Vec<u8>, Metadata), liquers_core::error::Error>);

impl From<Result<(Vec<u8>, Metadata), liquers_core::error::Error>> for DataResultWrapper {
    fn from(r: Result<(Vec<u8>, Metadata), liquers_core::error::Error>) -> Self {
        DataResultWrapper(r)
    }
}

impl IntoResponse for DataResultWrapper {
    fn into_response(self) -> Response<Body> {
        match self.0 {
            Ok((data, metadata)) => Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, metadata.get_media_type())
                .body(data.into())
                .unwrap(),
            Err(e) => CoreError(e).into_response(),
        }
    }
}


#[cfg(feature = "async_store")]
#[async_trait]
pub trait AsyncStoreTest: Send + Sync{
    async fn get(&self, key: &Key) -> Result<(Vec<u8>, Metadata), Error>{
        Ok((format!("Hello {key}").into_bytes(), Metadata::new()))
    }
    async fn set(&mut self, key: &Key, data: Vec<u8>) -> Result<(), Error>;
}
pub struct AsyncStoreTest1{
    some_data: String
}

#[async_trait]
impl AsyncStoreTest for AsyncStoreTest1{
    async fn get(&self, key: &Key) -> Result<(Vec<u8>, Metadata), Error>{
        Ok((format!("Hello1 {0} {key}", self.some_data).into_bytes(), Metadata::new()))
    }
    async fn set(&mut self, key: &Key, data: Vec<u8>) -> Result<(), Error>{
        self.some_data = format!("Some data: {}", String::from_utf8(data).map_err(|e| Error::general_error(format!("Error converting data to string: {}", e)))?);
        Ok(())
    }
}    


#[axum::debug_handler]
async fn test(Path(query): Path<String>) -> Response<Body> {
    let mut store = AsyncStoreTest1{some_data: "Nothing important".to_string()};


    match parse_key(&query){
        Ok(key) => {
//                DataResultWrapper(Err(Error::general_error(format!("Just testing {key}")))).into_response()
                let d=DataResultWrapper(store.get(&key).await);
                store.set(&key, "Changed".as_bytes().to_vec()).await.unwrap();
                d.into_response()

        },
        Err(e) => CoreError(e).into_response()
    }
}


/// Get data from store. Equivalent to Store.get_bytes.
/// Content type (MIME) is obtained from the metadata.
//async fn store_get<S: AsyncStore>(
//#[axum::debug_handler]
async fn store_get(
        //State(store): State<Arc<RwLock<S>>>,
    Path(query): Path<String>,
) -> impl IntoResponse {
//    let store = AsyncStoreWrapper(FileStore::new(".", &Key::new()));
//    let store = Arc::new(RwLock::new(store));
//    let st = store.read();
//    if let Ok(store) = st {
      {
        let store = AsyncStoreWrapper(FileStore::new(".", &Key::new()));
        let key = parse_key(query).unwrap();
        let data = store.get(&key).await;
        if let Ok((data, metadata)) = data {
            return (
                axum::http::StatusCode::OK,
                [(header::CONTENT_TYPE, metadata.get_media_type())],
                data,
            );
        } else {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                [(header::CONTENT_TYPE, "text/plain".to_owned())],
                format!("Error reading store: {}", data.err().unwrap()).into(),
            );
        }
    }
    /*
    else {
        return (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            [(header::CONTENT_TYPE, "text/plain".to_owned())],
            format!("Error accessing store: {}", st.err().unwrap()).into(),
        );
    }
    */
}

#[tokio::main]
async fn main() {
    // build our application with a single route

    let store = AsyncStoreWrapper(FileStore::new(".", &Key::new()));
    let shared_state:Arc<RwLock<_>> = Arc::new(RwLock::new(store));

    let app = Router::new()
        .route("/", get(|| async { "Hello, World!" }))
        //.route("/liquer/q/*query", get(evaluate_query))
        //.route("/liquer/submit/*query", get(submit_query))
        .route("/liquer/store/data/*query", get(test))
        //.route("/liquer/web/*query", get(web_store_get))
        //.route("/liquer/store/upload/*query", get(store_upload_get))
        .with_state(shared_state);

    // run it with hyper on localhost:3000
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
