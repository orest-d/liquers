//! Helpers and response types shared by the two Assets API families.
//!
//! See `specs/design/axum-assets-endpoints/phase2-architecture.md`: "Access Modes" and "Web
//! Endpoints". Every status output is an [`ApiResponse`]; only the value transfers (`data`,
//! `entry`, `recover`) are not.

use axum::{
    http::{header::CONTENT_TYPE, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use liquers_core::{
    assets::{AssetManager, AssetRef, AuditReport},
    context::{EnvRef, Environment},
    error::{Error, ErrorType},
    metadata::{AssetInfo, Metadata, MetadataRecord, Status, Version},
    parse::{parse_key, parse_query},
    query::{Key, Query},
    state::State,
};
use serde::{Deserialize, Serialize};

use crate::api_core::{
    error::error_to_detail,
    overrides::OverrideRequest,
    format::{negotiate_format, serialize_data_entry},
    ApiResponse, BinaryResponse, DataEntry, SerializationFormat,
};

/// `GET key/listdir`: the assets directly in a directory.
#[derive(Debug, Clone, Serialize)]
pub struct AssetListing {
    pub assets: Vec<AssetInfo>,
}

/// `GET key/listdir?deep=true`: every key under a directory.
#[derive(Debug, Clone, Serialize)]
pub struct KeyListing {
    pub keys: Vec<String>,
}

/// `DELETE key/data|entry`, `GET key/remove`: what the key is after the removal.
#[derive(Debug, Clone, Serialize)]
pub struct RemoveResult {
    pub removed: bool,
    /// `Recipe` when a recipe still describes the key, otherwise `None`.
    pub new_status: Status,
}

/// `DELETE|GET key/removedir`.
#[derive(Debug, Clone, Serialize)]
pub struct RemoveDirResult {
    pub removed: bool,
}

/// `GET key/contains`.
#[derive(Debug, Clone, Serialize)]
pub struct ContainsResult {
    pub contains: bool,
}

/// `GET key/can_make`.
#[derive(Debug, Clone, Serialize)]
pub struct CanMakeResult {
    pub can_make: bool,
}

/// `GET key/version`, `GET q/version`: 32 hex digits; all zeros means unknown.
#[derive(Debug, Clone, Serialize)]
pub struct VersionResult {
    pub version: Version,
}

/// `admin/audit`: a projection of [`AuditReport`], which has no serde contract of its own.
#[derive(Debug, Clone, Serialize)]
pub struct AuditResult {
    pub checked: Vec<String>,
    pub expired: Vec<String>,
}

impl From<AuditReport> for AuditResult {
    fn from(report: AuditReport) -> Self {
        AuditResult {
            checked: report
                .checked
                .iter()
                .map(|key| key.as_str().to_string())
                .collect(),
            expired: report
                .expired
                .iter()
                .map(|key| key.as_str().to_string())
                .collect(),
        }
    }
}

/// `POST key/description` body, or `GET key/description` query parameters.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct DescriptionRequest {
    pub title: Option<String>,
    pub description: Option<String>,
}

/// An error as an [`ApiResponse`]; the HTTP status follows the error type.
pub(crate) fn error_response(e: &Error, message: impl Into<String>) -> Response {
    ApiResponse::<()>::error(error_to_detail(e), message).into_response()
}

fn with_address<T: Serialize>(
    mut response: ApiResponse<T>,
    key: Option<&Key>,
    query: Option<&Query>,
) -> ApiResponse<T> {
    if let Some(key) = key {
        response.key = Some(key.encode());
        response.query = Some(Query::from(key).encode());
    }
    if let Some(query) = query {
        response.query = Some(query.encode());
    }
    response
}

/// 200 with `result`, naming the key it answers for.
pub(crate) fn ok_key<T: Serialize>(result: T, message: impl Into<String>, key: &Key) -> Response {
    with_address(ApiResponse::ok(result, message), Some(key), None).into_response()
}

/// 200 with `result`, naming the query it answers for.
pub(crate) fn ok_query<T: Serialize>(
    result: T,
    message: impl Into<String>,
    query: &Query,
) -> Response {
    with_address(ApiResponse::ok(result, message), None, Some(query)).into_response()
}

/// 200 with `result`, for a route that addresses no key or query (`admin/…`).
pub(crate) fn ok<T: Serialize>(result: T, message: impl Into<String>) -> Response {
    ApiResponse::ok(result, message).into_response()
}

/// 201 with `result`.
pub(crate) fn created<T: Serialize>(result: T, message: impl Into<String>, key: &Key) -> Response {
    (
        StatusCode::CREATED,
        with_address(ApiResponse::ok(result, message), Some(key), None),
    )
        .into_response()
}

/// The key family parses its path with `parse_key`. A path that looks like a query (it starts
/// with `-`, as `-R/…` does) gets a hint pointing at the query family.
pub(crate) fn key_from_path(path: &str) -> Result<Key, Error> {
    parse_key(path).map_err(|mut e| {
        if path.starts_with('-') {
            e.message = format!(
                "{}; key routes take a bare key (notes/a.txt) — use /q/ for a query",
                e.message
            );
        }
        e
    })
}

/// The query family parses its path with `parse_query`.
pub(crate) fn query_from_path(path: &str) -> Result<Query, Error> {
    parse_query(path)
}

/// The observe routes of the query family answer 404 `NotAvailable` for a query nobody has
/// requested (or whose asset has been evicted).
pub(crate) fn not_requested(query: &Query) -> Error {
    Error::from_error(
        ErrorType::NotAvailable,
        format!(
            "Query '{}' is not cached: submit it first (q/submit), then poll q/info",
            query.encode()
        ),
    )
    .with_query(query)
}

/// A metadata document as JSON: the record, or a legacy document as it is.
pub(crate) fn metadata_json(metadata: &Metadata) -> serde_json::Value {
    match metadata {
        Metadata::MetadataRecord(record) => {
            serde_json::to_value(record).unwrap_or(serde_json::Value::Null)
        }
        Metadata::LegacyMetadata(value) => value.clone(),
    }
}

/// Mode (a): wait for the value and serve its bytes, with the metadata in headers. Every value
/// type goes through `AssetRef::get_binary`, as the Query API's `GET /q/` does.
pub(crate) async fn asset_bytes<E: Environment>(asset: &AssetRef<E>) -> Response {
    match asset.get_binary().await {
        Ok((data, metadata)) => BinaryResponse {
            data: (*data).clone(),
            metadata: (*metadata).clone(),
        }
        .into_response(),
        Err(e) => error_response(&e, "Failed to retrieve asset value"),
    }
}

/// A negotiated `DataEntry`: the resolved `_entry_format` first, then `Accept`, then CBOR. Each
/// caller resolves the option itself (`entry_format_param`, or the override parser on `q/entry`).
pub(crate) fn entry_response(
    data: &[u8],
    metadata: &Metadata,
    headers: &HeaderMap,
    entry_format: Option<SerializationFormat>,
) -> Response {
    let entry = DataEntry {
        data: data.to_vec(),
        metadata: metadata_json(metadata),
    };
    let format = negotiate_format(entry_format, headers);
    match serialize_data_entry(&entry, format) {
        Ok(bytes) => {
            let mut response_headers = HeaderMap::new();
            response_headers.insert(CONTENT_TYPE, HeaderValue::from_static(format.mime_type()));
            (response_headers, bytes).into_response()
        }
        Err(e) => error_response(
            &Error::from_error(ErrorType::SerializationError, e),
            "Failed to serialize entry",
        ),
    }
}

/// Mode (a) for `entry`: wait for the value, then negotiate the entry.
pub(crate) async fn asset_entry<E: Environment>(
    asset: &AssetRef<E>,
    headers: &HeaderMap,
    entry_format: Option<SerializationFormat>,
) -> Response {
    match asset.get_binary().await {
        Ok((data, metadata)) => entry_response(&data, &metadata, headers, entry_format),
        // The entry route keeps the log of a failure: the asset's metadata travels with the error.
        Err(e) => {
            let metadata = asset.get_metadata().await.ok().map(|m| metadata_json(&m));
            error_with_metadata(&e, metadata, "Failed to retrieve asset value")
        }
    }
}

/// Mode (c) for a key: the live asset's metadata, else the stored, else the recipe's. Never
/// evaluates.
pub(crate) async fn key_metadata<E: Environment>(
    env: &EnvRef<E>,
    key: &Key,
) -> Result<serde_json::Value, Error> {
    let manager = env.get_asset_manager();
    if let Some(asset) = manager.lookup_key_asset(key) {
        return Ok(metadata_json(&asset.get_metadata().await?));
    }
    let store = env.get_async_store();
    if store.contains(key).await? {
        return Ok(metadata_json(&store.get_metadata(key).await?));
    }
    let provider = env.get_recipe_provider();
    if provider.can_make(key, env.clone()).await? {
        let info = provider.get_asset_info(key, env.clone()).await?;
        let record = MetadataRecord::from(info);
        return Ok(serde_json::to_value(&record).unwrap_or(serde_json::Value::Null));
    }
    Err(Error::key_not_found(key))
}

/// Mode (c) for a key: `AssetManager::version`, with `Version::unknown()` (all zeros) when the
/// key has no version. A store error stays an error.
pub(crate) async fn key_version<E: Environment>(
    env: &EnvRef<E>,
    key: &Key,
) -> Result<VersionResult, Error> {
    let version = env.get_asset_manager().version(key).await?;
    Ok(VersionResult { version })
}

/// Mode (b) for a key: refuse a key that is neither stored nor declared by a recipe (a `get`
/// would map a fresh asset for any key and fail later), then request it without waiting.
pub(crate) async fn submit_key<E: Environment>(
    env: &EnvRef<E>,
    key: &Key,
) -> Result<AssetInfo, Error> {
    submit_key_asset(env, key).await?.get_asset_info().await
}

/// [`submit_key`], returning the requested asset.
pub(crate) async fn submit_key_asset<E: Environment>(
    env: &EnvRef<E>,
    key: &Key,
) -> Result<AssetRef<E>, Error> {
    let manager = env.get_asset_manager();
    if !manager.can_make(key).await? {
        return Err(Error::key_not_found(key));
    }
    manager.get(key).await
}

// ------------------------------------------------- argument overrides and managed assets

/// The header telling whether the asset behind an answer is **managed** — held by the asset
/// manager, so it can be found again by its query or key — or unmanaged (evaluated with argument
/// overrides, volatile, from a `cached: false` command, or not retained by the cache strategy).
/// `specs/design/axum-query-named-arguments/`.
pub const MANAGED_HEADER: &str = "x-liquers-managed";

/// The note on a `q/submit` answer about an unmanaged asset.
pub const UNMANAGED_NOTE: &str = "This evaluation is unmanaged: its status and result cannot be \
     read later. Use q/data, or WebSocket subscribe with arguments, to get a result.";

/// The asset a query-family request asked for, and whether argument overrides made it ad hoc.
pub(crate) struct Requested<E: Environment> {
    pub asset: AssetRef<E>,
    pub overridden: bool,
}

/// Request `query` with `overrides`. With none, this is the route's ordinary request
/// (`AssetManager::get_asset`). With any, the query is evaluated ad hoc —
/// `AssetManager::apply_queued` of a recipe carrying them — after the overrides were validated,
/// so a naming or conversion error fails before anything runs.
pub(crate) async fn request_with_overrides<E: Environment>(
    env: &EnvRef<E>,
    query: &Query,
    overrides: &OverrideRequest,
) -> Result<Requested<E>, Error> {
    let manager = env.get_asset_manager();
    if overrides.is_empty() {
        return Ok(Requested {
            asset: manager.get_asset(query).await?,
            overridden: false,
        });
    }
    let recipe = overrides.to_recipe(query, env.get_command_metadata_registry())?;
    Ok(Requested {
        asset: manager.apply_queued(recipe, State::new()).await?,
        overridden: true,
    })
}

/// Whether `asset` is managed: not evaluated with overrides, held by the manager under its
/// address (the key map for a pure-key query, else the query map), and not volatile — a volatile
/// asset that is still mapped is evicted by the next request. Read when answering.
pub(crate) async fn is_managed<E: Environment>(
    env: &EnvRef<E>,
    query: &Query,
    asset: &AssetRef<E>,
    overridden: bool,
) -> bool {
    if overridden {
        return false;
    }
    let manager = env.get_asset_manager();
    let mapped = match query.key() {
        Some(key) => manager.lookup_key_asset(&key),
        None => manager.lookup_query_asset(query),
    };
    mapped.is_some_and(|m| m.id() == asset.id()) && !asset.is_volatile().await
}

/// `response` with the [`MANAGED_HEADER`].
pub(crate) fn with_managed(mut response: Response, managed: bool) -> Response {
    response.headers_mut().insert(
        MANAGED_HEADER,
        HeaderValue::from_static(if managed { "true" } else { "false" }),
    );
    response
}

/// The metadata of the asset the manager holds under `query`'s address, if any — what a failed
/// request can still report when the manager returned an error instead of the asset.
pub(crate) async fn mapped_metadata<E: Environment>(
    env: &EnvRef<E>,
    query: &Query,
) -> Option<serde_json::Value> {
    let manager = env.get_asset_manager();
    let asset = match query.key() {
        Some(key) => manager.lookup_key_asset(&key),
        None => manager.lookup_query_asset(query),
    }?;
    asset.get_metadata().await.ok().map(|m| metadata_json(&m))
}

/// An error envelope carrying the failed asset's metadata (its log included) in
/// `error.metadata`.
pub(crate) fn error_with_metadata(
    e: &Error,
    metadata: Option<serde_json::Value>,
    message: impl Into<String>,
) -> Response {
    let mut detail = error_to_detail(e);
    detail.metadata = metadata;
    ApiResponse::<()>::error(detail, message).into_response()
}

/// The `q/submit` answer: the asset's state, whether it is managed, and — when it is not — a
/// note that nothing can read it later.
#[derive(Debug, Clone, Serialize)]
pub struct SubmitResult {
    #[serde(flatten)]
    pub info: AssetInfo,
    pub managed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// What a key is after `remove`: `Recipe` if something still describes it, otherwise `None`.
pub(crate) async fn status_after_remove<E: Environment>(env: &EnvRef<E>, key: &Key) -> Status {
    match env.get_asset_manager().get_asset_info(key).await {
        Ok(info) => info.status,
        Err(_) => Status::None,
    }
}
