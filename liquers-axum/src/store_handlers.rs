use std::sync::Arc;

use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, Response, StatusCode},
    response::IntoResponse,
};
use liquers_core::{
    context::Environment,
    metadata::{Metadata, MetadataRecord},
    parse::parse_key,
};
use tokio::sync::RwLock;

use crate::{
    environment::SharedEnvironment,
    utils::{CoreError, DataResultWrapper},
};

#[axum::debug_handler]
pub async fn store_data_handler(
    Path(query): Path<String>,
    State(env): State<SharedEnvironment>,
) -> Response<Body> {
    let store = env.read().await.get_async_store();
    match parse_key(&query) {
        Ok(key) => DataResultWrapper(store.get(&key).await).into_response(),
        Err(e) => CoreError(e).into_response(),
    }
}

#[axum::debug_handler]
pub async fn store_metadata_handler(
    Path(query): Path<String>,
    State(env): State<SharedEnvironment>,
) -> Response<Body> {
    let store = env.read().await.get_async_store();
    match parse_key(&query) {
        Ok(key) => match store.get_metadata(&key).await {
            Ok(Metadata::MetadataRecord(metadata)) => Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&metadata).unwrap().into())
                .unwrap(),
            Ok(Metadata::LegacyMetadata(metadata)) => Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "application/json")
                .body(serde_json::to_string(&metadata).unwrap().into())
                .unwrap(),
            Err(e) => CoreError(e).into_response(),
        },
        Err(e) => CoreError(e).into_response(),
    }
}
