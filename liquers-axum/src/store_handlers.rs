use std::sync::Arc;

use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, Response, StatusCode},
    response::IntoResponse,
    Error,
};
use liquers_core::{
    context::{Environment, NGEnvironment},
    metadata::{Metadata, MetadataRecord},
    parse::parse_key,
};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::{
    environment::ServerEnvRef,
    utils::{CoreError, DataResultWrapper},
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum StoreResultStatus {
    #[serde(rename = "OK")]
    Ok,
    #[serde(rename = "ERROR")]
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoreResult<T: Serialize> {
    status: StoreResultStatus,
    result: Option<T>,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    query: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(default)]
    error: Option<liquers_core::error::Error>,
}

impl<T> StoreResult<T>
where
    T: Serialize,
{
    pub fn with_key(mut self, key: String) -> Self {
        self.key = Some(key);
        self
    }
}

impl<T> From<Result<T, liquers_core::error::Error>> for StoreResult<T>
where
    T: Serialize,
{
    fn from(result: Result<T, liquers_core::error::Error>) -> Self {
        match result {
            Ok(x) => StoreResult {
                status: StoreResultStatus::Ok,
                result: Some(x),
                message: "OK".to_string(),
                query: None,
                key: None,
                error: None,
            },
            Err(e) => StoreResult {
                status: StoreResultStatus::Error,
                result: None,
                message: e.to_string(),
                query: e.query.clone(),
                key: e.key.clone(),
                error: Some(e),
            },
        }
    }
}

impl<T> IntoResponse for StoreResult<T>
where
    T: Serialize,
{
    fn into_response(self) -> Response<Body> {
        match serde_json::to_string_pretty(&self) {
            Ok(json) => Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "application/json")
                .body(json.into())
                .unwrap(),
            Err(e) => Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .header(header::CONTENT_TYPE, "text/plain")
                .body(e.to_string().into())
                .unwrap(),
        }
    }
}

#[axum::debug_handler]
pub async fn store_data_handler(
    Path(query): Path<String>,
    State(env): State<ServerEnvRef>,
) -> Response<Body> {
    let store = env.0.read().await.get_async_store();
    match parse_key(&query) {
        Ok(key) => DataResultWrapper(store.get(&key).await).into_response(),
        Err(e) => CoreError(e).into_response(),
    }
}

#[axum::debug_handler]
pub async fn web_handler(
    Path(query): Path<String>,
    State(env): State<ServerEnvRef>,
) -> Response<Body> {
    let store = env.0.read().await.get_async_store();
    match parse_key(&query) {
        // TODO: handle directory and nicer error
        Ok(key) => DataResultWrapper(store.get(&key).await).into_response(),
        Err(e) => CoreError(e).into_response(),
    }
}

#[axum::debug_handler]
pub async fn store_metadata_handler(
    Path(query): Path<String>,
    State(env): State<ServerEnvRef>,
) -> Response<Body> {
    let store = env.0.read().await.get_async_store();
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

#[axum::debug_handler]
pub async fn upload_handler(
    Path(query): Path<String>,
    //State(env): State<SharedEnvironment>,
) -> Response<Body> {
    //let store = env.read().await.get_async_store();
    match parse_key(&query) {
        // TODO: handle directory and nicer error
        Ok(key) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/html")
            .body(
                format!(
                    "<!DOCTYPE html>
                    <html>
                    <head>
                        <title>Upload File {key}</title>
                    </head>
                    <body>
                        <h1>Upload to {key}</h1>
                        <form method=\"post\" enctype=\"multipart/form-data\">
                        <input type=\"file\" name=\"file\"/>
                        <input type=\"submit\" value=\"Upload\"/>
                        </form>
                    </body>
                    "
                )
                .into(),
            )
            .unwrap(),
        Err(e) => CoreError(e).into_response(),
    }
}

#[axum::debug_handler]
pub async fn remove_handler(
    Path(query): Path<String>,
    State(env): State<ServerEnvRef>,
) -> Response<Body> {
    let store = env.0.read().await.get_async_store();
    match parse_key(&query) {
        Ok(key) => StoreResult::from(store.remove(&key).await)
            .with_key(key.encode())
            .into_response(),
        Err(e) => CoreError(e).into_response(),
    }
}

#[axum::debug_handler]
pub async fn removedir_handler(
    Path(query): Path<String>,
    State(env): State<ServerEnvRef>,
) -> Response<Body> {
    let store = env.0.read().await.get_async_store();
    match parse_key(&query) {
        Ok(key) => StoreResult::from(store.removedir(&key).await)
            .with_key(key.encode())
            .into_response(),
        Err(e) => CoreError(e).into_response(),
    }
}

#[axum::debug_handler]
pub async fn contains_handler(
    Path(query): Path<String>,
    State(env): State<ServerEnvRef>,
) -> Response<Body> {
    let store = env.0.read().await.get_async_store();
    match parse_key(&query) {
        Ok(key) => StoreResult::from(store.contains(&key).await)
            .with_key(key.encode())
            .into_response(),
        Err(e) => CoreError(e).into_response(),
    }
}

#[axum::debug_handler]
pub async fn is_dir_handler(
    Path(query): Path<String>,
    State(env): State<ServerEnvRef>,
) -> Response<Body> {
    let store = env.0.read().await.get_async_store();
    match parse_key(&query) {
        Ok(key) => StoreResult::from(store.is_dir(&key).await)
            .with_key(key.encode())
            .into_response(),
        Err(e) => CoreError(e).into_response(),
    }
}

#[axum::debug_handler]
pub async fn keys_handler(State(env): State<ServerEnvRef>) -> Response<Body> {
    let store = env.0.read().await.get_async_store();
    StoreResult::from(store.keys().await.map(|keys| keys.iter().map(|k| k.encode()).collect::<Vec<_>>()))
        .with_key("".to_string())
        .into_response()
    .into_response()
}

#[axum::debug_handler]
pub async fn listdir_handler(
    Path(query): Path<String>,
    State(env): State<ServerEnvRef>,
) -> Response<Body> {
    let store = env.0.read().await.get_async_store();
    match parse_key(&query) {
        Ok(key) => StoreResult::from(store.listdir(&key).await)
            .with_key(key.encode())
            .into_response(),
        Err(e) => CoreError(e).into_response(),
    }
}

#[axum::debug_handler]
pub async fn makedir_handler(
    Path(query): Path<String>,
    State(env): State<ServerEnvRef>,
) -> Response<Body> {
    let store = env.0.read().await.get_async_store();
    match parse_key(&query) {
        Ok(key) => StoreResult::from(store.makedir(&key).await)
            .with_key(key.encode())
            .into_response(),
        Err(e) => CoreError(e).into_response(),
    }
}
