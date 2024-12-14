pub mod environment;
pub mod store_handlers;
pub mod utils;

use std::sync::{Arc};
use crate::environment::ServerEnvironment;
use liquers_core::context::Environment;
use liquers_core::value::Value;
use tokio::sync::{RwLock};

use axum::http::{header, StatusCode};
use axum::response::Response;
use axum::body::Body;
use axum::{routing::get, Router};
use axum::response::IntoResponse;
use liquers_core::store::{FileStore, AsyncStoreWrapper};
use liquers_core::query::Key;
use axum::extract::{Path, State};


use crate::store_handlers::*;
use crate::utils::*;




#[tokio::main]
async fn main() {


    //let hashmaptest: Arc<HashMap<String, String>> = Arc::new(HashMap::new());
    // build our application with a single route

    let mut env:ServerEnvironment<Value> = ServerEnvironment::new();
    env.with_async_store(Box::new(AsyncStoreWrapper(FileStore::new(".", &Key::new()))));
    let state: Arc<RwLock<ServerEnvironment<Value>>> =Arc::new(RwLock::new(env));

//    let store:Arc<Box<dyn AsyncStore>> = Arc::new(Box::new(AsyncStoreWrapper(FileStore::new(".", &Key::new()))));

    let app = Router::new()
        .route("/", get(|| async { "Hello, World!" }))
        //.route("/liquer/q/*query", get(evaluate_query))
        //.route("/liquer/submit/*query", get(submit_query))
        .route("/liquer/store/data/*query", get(crate::store_handlers::store_data_handler))
        .route("/liquer/store/metadata/*query", get(crate::store_handlers::store_metadata_handler))
        //.route("/liquer/web/*query", get(web_store_get))
        //.route("/liquer/store/upload/*query", get(store_upload_get))
        .with_state(state);

    // run it with hyper on localhost:3000
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
