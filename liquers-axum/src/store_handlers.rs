use std::sync::Arc;

use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, Response, StatusCode},
    response::IntoResponse,
};
use liquers_core::{
    context::{Environment, NGEnvironment},
    metadata::{Metadata, MetadataRecord},
    parse::parse_key,
};
use tokio::sync::RwLock;

use crate::{
    environment::ServerEnvRef,
    utils::{CoreError, DataResultWrapper},
};

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
        Ok(key) => match store.remove(&key).await {
            // TODO: convert store output to JSON
            Ok(_) => Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "text/plain")
                .body("OK".into())
                .unwrap(),
            Err(e) => CoreError(e).into_response(),
        },
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
        Ok(key) => match store.removedir(&key).await {
            // TODO: convert store output to JSON
            Ok(_) => Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "text/plain")
                .body("OK".into())
                .unwrap(),
            Err(e) => CoreError(e).into_response(),
        },
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
        Ok(key) => match store.contains(&key).await {
            // TODO: convert store output to JSON
            Ok(_) => Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "text/plain")
                .body("OK".into())
                .unwrap(),
            Err(e) => CoreError(e).into_response(),
        },
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
        Ok(key) => match store.is_dir(&key).await {
            // TODO: convert store output to JSON
            Ok(_) => Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "text/plain")
                .body("OK".into())
                .unwrap(),
            Err(e) => CoreError(e).into_response(),
        },
        Err(e) => CoreError(e).into_response(),
    }
}

#[axum::debug_handler]
pub async fn keys_handler(State(env): State<ServerEnvRef>) -> Response<Body> {
    let store = env.0.read().await.get_async_store();
    match store.keys().await {
        // TODO: convert store output to JSON
        Ok(_) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, "text/plain")
            .body("OK".into())
            .unwrap(),
        Err(e) => CoreError(e).into_response(),
    }
}

#[axum::debug_handler]
pub async fn listdir_handler(
    Path(query): Path<String>,
    State(env): State<ServerEnvRef>,
) -> Response<Body> {
    let store = env.0.read().await.get_async_store();
    match parse_key(&query) {
        Ok(key) => match store.listdir(&key).await {
            // TODO: convert store output to JSON
            Ok(_) => Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "text/plain")
                .body("OK".into())
                .unwrap(),
            Err(e) => CoreError(e).into_response(),
        },
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
        Ok(key) => match store.makedir(&key).await {
            // TODO: convert store output to JSON
            Ok(_) => Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "text/plain")
                .body("OK".into())
                .unwrap(),
            Err(e) => CoreError(e).into_response(),
        },
        Err(e) => CoreError(e).into_response(),
    }
}
