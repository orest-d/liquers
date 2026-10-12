//! Assets API, query family (`{base}/q/…`): paths are parsed with `parse_query`, so any query —
//! keyed (`-R/notes/a.txt`) or not (`make_text/upper`) — is addressable.
//!
//! Three access modes (`specs/design/axum-assets-endpoints/phase2-architecture.md`, "Access
//! Modes"): (a) `data`/`entry` request and wait; (b) `submit` requests and returns at once; (c)
//! `info`/`metadata`/`version` observe and **never** start an evaluation. A pure-key query on an
//! observe route is answered like the key family, since a stored key has no live asset to find.


use axum::{
    body::Bytes,
    extract::{Path, Query as AxumQuery, State},
    http::HeaderMap,
    response::Response,
};
use liquers_core::{
    assets::AssetManager,
    context::{EnvRef, Environment},
    metadata::Version,
};

use crate::api_core::overrides::{OverrideRequest, RouteOptions};

use super::common::{
    asset_bytes, asset_entry, error_response, error_with_metadata, is_managed, key_metadata,
    key_version, mapped_metadata, metadata_json, not_requested, ok_query, query_from_path,
    request_with_overrides, submit_key_asset, with_managed, SubmitResult, VersionResult,
    UNMANAGED_NOTE,
};

/// The query and argument overrides of a query-family request, or the 400 answer.
fn parse_request(
    query_path: &str,
    pairs: Vec<(String, String)>,
    body: &[u8],
    options: RouteOptions,
) -> Result<(liquers_core::query::Query, OverrideRequest), Response> {
    let query = query_from_path(query_path).map_err(|e| error_response(&e, "Failed to parse query"))?;
    let overrides = OverrideRequest::parse(pairs, body, options)
        .map_err(|e| error_response(&e.with_query(&query), "Invalid argument overrides"))?;
    Ok((query, overrides))
}

/// `GET|POST q/data/{*query}` — mode (a): evaluate if needed, wait, serve the bytes. Argument
/// overrides come from the query string and (POST) a JSON object body; with any, the value is
/// evaluated ad hoc as an unmanaged asset (`x-liquers-managed: false`).
pub async fn q_get_data_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(query_path): Path<String>,
    AxumQuery(pairs): AxumQuery<Vec<(String, String)>>,
    body: Bytes,
) -> Response {
    let (query, overrides) = match parse_request(&query_path, pairs, &body, RouteOptions::None) {
        Ok(parsed) => parsed,
        Err(response) => return response,
    };
    match request_with_overrides(&env, &query, &overrides).await {
        Ok(requested) => {
            let response = asset_bytes(&requested.asset).await;
            let managed = is_managed(&env, &query, &requested.asset, requested.overridden).await;
            with_managed(response, managed)
        }
        Err(e) => error_response(&e, "Failed to get asset"),
    }
}

/// `GET|POST q/entry/{*query}` — mode (a), as a negotiated `DataEntry` (`_entry_format`, then
/// `Accept`, then CBOR). Overrides as for `data`; a failure carries the asset's metadata.
pub async fn q_get_entry_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(query_path): Path<String>,
    headers: HeaderMap,
    AxumQuery(pairs): AxumQuery<Vec<(String, String)>>,
    body: Bytes,
) -> Response {
    let (query, overrides) = match parse_request(&query_path, pairs, &body, RouteOptions::Entry) {
        Ok(parsed) => parsed,
        Err(response) => return response,
    };
    match request_with_overrides(&env, &query, &overrides).await {
        Ok(requested) => {
            let response = asset_entry(&requested.asset, &headers, overrides.entry_format).await;
            let managed = is_managed(&env, &query, &requested.asset, requested.overridden).await;
            with_managed(response, managed)
        }
        // An inline manager returns the evaluation's error rather than the asset; a managed asset
        // can still be found under its address for its log.
        Err(e) => {
            let metadata = if overrides.is_empty() {
                mapped_metadata(&env, &query).await
            } else {
                None
            };
            error_with_metadata(&e, metadata, "Failed to get asset")
        }
    }
}

/// `POST|GET q/submit/{*query}` — mode (b): request the evaluation and answer with the asset's
/// `AssetInfo` at once, plus whether it is managed. A pure-key query must name a stored or
/// recipe-declared key. With argument overrides the evaluation is unmanaged: it runs, but no
/// route can read it later — the note says so (useful for a query run for its side effect).
pub async fn q_submit_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(query_path): Path<String>,
    AxumQuery(pairs): AxumQuery<Vec<(String, String)>>,
    body: Bytes,
) -> Response {
    let (query, overrides) = match parse_request(&query_path, pairs, &body, RouteOptions::None) {
        Ok(parsed) => parsed,
        Err(response) => return response,
    };
    let requested = if overrides.is_empty() {
        match query.key() {
            Some(key) => submit_key_asset(&env, &key).await.map(|asset| (asset, false)),
            None => env
                .get_asset_manager()
                .get_asset(&query)
                .await
                .map(|asset| (asset, false)),
        }
    } else {
        request_with_overrides(&env, &query, &overrides)
            .await
            .map(|r| (r.asset, r.overridden))
    };
    let (asset, overridden) = match requested {
        Ok(requested) => requested,
        Err(e) => return error_response(&e, "Failed to submit"),
    };
    let managed = is_managed(&env, &query, &asset, overridden).await;
    match asset.get_asset_info().await {
        Ok(info) => {
            let result = SubmitResult {
                info,
                managed,
                note: (!managed).then(|| UNMANAGED_NOTE.to_string()),
            };
            with_managed(ok_query(result, "Submitted", &query), managed)
        }
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
