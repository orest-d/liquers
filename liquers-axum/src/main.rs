pub mod core_handlers;
pub mod environment;
pub mod store_handlers;
pub mod utils;

use crate::environment::ServerEnvironment;
use liquers_core::value::Value;
use std::sync::Arc;
use tokio::sync::RwLock;

use axum::{routing::get, Router};
use liquers_core::query::Key;
use liquers_core::store::{AsyncStoreWrapper, FileStore};

#[tokio::main]
async fn main() {
    //let hashmaptest: Arc<HashMap<String, String>> = Arc::new(HashMap::new());
    // build our application with a single route

    let mut env: ServerEnvironment<Value> = ServerEnvironment::new();
    env.with_async_store(Box::new(AsyncStoreWrapper(FileStore::new(
        ".",
        &Key::new(),
    ))));
    let state: Arc<RwLock<ServerEnvironment<Value>>> = Arc::new(RwLock::new(env));

    //    let store:Arc<Box<dyn AsyncStore>> = Arc::new(Box::new(AsyncStoreWrapper(FileStore::new(".", &Key::new()))));

    let app = Router::new()
        .route("/", get(|| async { "Hello, World!" }))
        //.route("/liquer/q/*query", get(evaluate_query))
        //.route("/liquer/submit/*query", get(submit_query))
        .route(
            "/liquer/store/data/*query",
            get(crate::store_handlers::store_data_handler),
        )
        //.route("/liquer/store/data/*query", post(crate::store_handlers::store_data_post_handler))
        .route(
            "/liquer/store/metadata/*query",
            get(crate::store_handlers::store_metadata_handler),
        )
        //.route("/liquer/store/metadata/*query", post(crate::store_handlers::store_metadata_post_handler))
        .route(
            "/liquer/store/upload/*query",
            get(crate::store_handlers::upload_handler),
        )
        //.route("/liquer/store/upload/*query", post(crate::store_handlers::upload_post_handler))
        // /api/stored_metadata/QUERY (GET) ?
        .route(
            "/liquer/store/upload/*query",
            get(crate::store_handlers::upload_handler),
        )
        .route(
            "/liquer/store/remove/*query",
            get(crate::store_handlers::remove_handler),
        )
        .route(
            "/liquer/store/removedir/*query",
            get(crate::store_handlers::remove_handler),
        )
        .route(
            "/liquer/store/contains/*query",
            get(crate::store_handlers::remove_handler),
        )
        .route(
            "/liquer/store/is_dir/*query",
            get(crate::store_handlers::is_dir_handler),
        )
        .route(
            "/liquer/store/keys",
            get(crate::store_handlers::keys_handler),
        )
        .route(
            "/liquer/store/listdir/*query",
            get(crate::store_handlers::listdir_handler),
        ) // TODO: support listdir_keys and listdir_keys_deep
        .route(
            "/liquer/store/makedir/*query",
            get(crate::store_handlers::makedir_handler),
        )
        .with_state(state);

    // run it with hyper on localhost:3000
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
