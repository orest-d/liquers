use std::sync::{Arc};
mod environment;
use crate::environment::ServerEnvironment;
use liquers_core::value::Value;
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


//use scc::HashMap;


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




#[axum::debug_handler]
async fn test(Path(query): Path<String>, State(store): State<Arc<Box<dyn AsyncStore>>>) -> Response<Body> {

    match parse_key(&query){
        Ok(key) => {
            match store.get(&key).await{
                Ok((data, metadata)) => {
                    println!("Metadata: {:?}", metadata);
                    return Response::builder()
                        .status(StatusCode::OK)
                        .header(header::CONTENT_TYPE, metadata.get_media_type())
                        .body(data.into())
                        .unwrap();
                },
                Err(e) => CoreError(e).into_response()            
            }
        },
        Err(e) => CoreError(e).into_response()
    }
}



#[tokio::main]
async fn main() {


    //let hashmaptest: Arc<HashMap<String, String>> = Arc::new(HashMap::new());
    // build our application with a single route

    let env:ServerEnvironment<Value> = ServerEnvironment::new();
    let store:Arc<Box<dyn AsyncStore>> = Arc::new(Box::new(AsyncStoreWrapper(FileStore::new(".", &Key::new()))));

    let app = Router::new()
        .route("/", get(|| async { "Hello, World!" }))
        //.route("/liquer/q/*query", get(evaluate_query))
        //.route("/liquer/submit/*query", get(submit_query))
        .route("/liquer/store/data/*query", get(test))
        //.route("/liquer/web/*query", get(web_store_get))
        //.route("/liquer/store/upload/*query", get(store_upload_get))
        .with_state(store);

    // run it with hyper on localhost:3000
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
