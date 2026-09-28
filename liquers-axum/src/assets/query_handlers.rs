//! Assets API, query family (`{base}/q/…`): paths are parsed with `parse_query`, so any query —
//! keyed (`-R/notes/a.txt`) or not (`make_text/upper`) — is addressable.
//!
//! Three access modes (`specs/design/axum-assets-endpoints/phase2-architecture.md`, "Access
//! Modes"): (a) `data`/`entry` request and wait; (b) `submit` requests and returns at once; (c)
//! `info`/`metadata`/`version` observe and **never** start an evaluation. A pure-key query on an
//! observe route is answered like the key family, since a stored key has no live asset to find.

use std::collections::HashMap;

use axum::{
    extract::{Path, Query as AxumQuery, State},
    http::HeaderMap,
    response::Response,
};
use liquers_core::{
    assets::AssetManager,
    context::{EnvRef, Environment},
    metadata::Version,
};

use super::common::{
    asset_bytes, asset_entry, error_response, key_metadata, key_version, metadata_json,
    not_requested, ok_query, query_from_path, submit_key, VersionResult,
};

/// `GET q/data/{*query}` — mode (a): evaluate if needed, wait, serve the bytes.
pub async fn q_get_data_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(query_path): Path<String>,
) -> Response {
    let query = match query_from_path(&query_path) {
        Ok(query) => query,
        Err(e) => return error_response(&e, "Failed to parse query"),
    };
    match env.get_asset_manager().get_asset(&query).await {
        Ok(asset) => asset_bytes(&asset).await,
        Err(e) => error_response(&e, "Failed to get asset"),
    }
}

/// `GET q/entry/{*query}` — mode (a), as a negotiated `DataEntry`.
pub async fn q_get_entry_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(query_path): Path<String>,
    headers: HeaderMap,
    AxumQuery(params): AxumQuery<HashMap<String, String>>,
) -> Response {
    let query = match query_from_path(&query_path) {
        Ok(query) => query,
        Err(e) => return error_response(&e, "Failed to parse query"),
    };
    match env.get_asset_manager().get_asset(&query).await {
        Ok(asset) => asset_entry(&asset, &headers, &params).await,
        Err(e) => error_response(&e, "Failed to get asset"),
    }
}

/// `POST|GET q/submit/{*query}` — mode (b): request the evaluation and answer with the asset's
/// `AssetInfo` at once. A pure-key query must name a stored or recipe-declared key.
pub async fn q_submit_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(query_path): Path<String>,
) -> Response {
    let query = match query_from_path(&query_path) {
        Ok(query) => query,
        Err(e) => return error_response(&e, "Failed to parse query"),
    };
    let info = match query.key() {
        Some(key) => submit_key(&env, &key).await,
        None => match env.get_asset_manager().get_asset(&query).await {
            Ok(asset) => asset.get_asset_info().await,
            Err(e) => Err(e),
        },
    };
    match info {
        Ok(info) => ok_query(info, "Submitted", &query),
        Err(e) => error_response(&e, "Failed to submit"),
    }
}

/// `GET q/info/{*query}` — mode (c): the cached asset's current `AssetInfo`, or 404
/// `NotAvailable` when nobody has requested the query. Never evaluates.
pub async fn q_info_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(query_path): Path<String>,
) -> Response {
    let query = match query_from_path(&query_path) {
        Ok(query) => query,
        Err(e) => return error_response(&e, "Failed to parse query"),
    };
    let manager = env.get_asset_manager();
    let info = match query.key() {
        Some(key) => manager.get_asset_info(&key).await,
        None => match manager.lookup_query_asset(&query) {
            Some(asset) => asset.get_asset_info().await,
            None => Err(not_requested(&query)),
        },
    };
    match info {
        Ok(info) => ok_query(info, "Asset info", &query),
        Err(e) => error_response(&e, "Asset info not available (submit the query first)"),
    }
}

/// `GET q/metadata/{*query}` — mode (c): the cached asset's metadata record. Never evaluates.
pub async fn q_get_metadata_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(query_path): Path<String>,
) -> Response {
    let query = match query_from_path(&query_path) {
        Ok(query) => query,
        Err(e) => return error_response(&e, "Failed to parse query"),
    };
    let metadata = match query.key() {
        Some(key) => key_metadata(&env, &key).await,
        None => match env.get_asset_manager().lookup_query_asset(&query) {
            Some(asset) => asset.get_metadata().await.map(|m| metadata_json(&m)),
            None => Err(not_requested(&query)),
        },
    };
    match metadata {
        Ok(metadata) => ok_query(metadata, "Asset metadata", &query),
        Err(e) => error_response(&e, "Asset metadata not available (submit the query first)"),
    }
}

/// `GET q/version/{*query}` — mode (c): the version, all zeros until the asset has finished.
pub async fn q_version_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(query_path): Path<String>,
) -> Response {
    let query = match query_from_path(&query_path) {
        Ok(query) => query,
        Err(e) => return error_response(&e, "Failed to parse query"),
    };
    let version = match query.key() {
        Some(key) => key_version(&env, &key).await,
        None => match env.get_asset_manager().lookup_query_asset(&query) {
            Some(asset) => asset.get_metadata().await.map(|m| VersionResult {
                version: m.version().unwrap_or_else(Version::unknown),
            }),
            None => Err(not_requested(&query)),
        },
    };
    match version {
        Ok(version) => ok_query(version, "Asset version", &query),
        Err(e) => error_response(&e, "Asset version not available (submit the query first)"),
    }
}

/// `POST q/cancel/{*query}` (and, with `with_destructive_gets()`, `GET`): cancel the cached asset.
/// Looks the asset up without creating one, so it never starts the evaluation it cancels.
pub async fn q_cancel_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(query_path): Path<String>,
) -> Response {
    let query = match query_from_path(&query_path) {
        Ok(query) => query,
        Err(e) => return error_response(&e, "Failed to parse query"),
    };
    let Some(asset) = env.get_asset_manager().lookup_query_asset(&query) else {
        return error_response(&not_requested(&query), "Nothing to cancel (submit the query first)");
    };
    let result = match asset.cancel().await {
        Ok(()) => asset.get_asset_info().await,
        Err(e) => Err(e),
    };
    match result {
        Ok(info) => ok_query(info, "Cancel requested", &query),
        Err(e) => error_response(&e, "Failed to cancel"),
    }
}
