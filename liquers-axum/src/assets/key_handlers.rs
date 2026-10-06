//! Assets API, key family (`{base}/key/…`) and admin routes (`{base}/admin/…`): paths are parsed
//! with `parse_key`, so they take a bare key (`notes/a.txt`, no `-R/`).
//!
//! Three access modes (`specs/design/axum-assets-endpoints/phase2-architecture.md`, "Access
//! Modes"): (a) `data`/`entry` request and wait; (b) `submit` requests and returns at once; (c)
//! `info`/`metadata`/`version`/`contains`/`recover`/`listdir` observe and **never** start an
//! evaluation. The mutations go through the `AssetManager`'s status-aware operations.

use std::collections::HashMap;

use axum::{
    body::Bytes,
    extract::{Path, Query as AxumQuery, State},
    http::HeaderMap,
    response::Response,
};
use liquers_core::{
    assets::AssetManager,
    context::{EnvRef, Environment},
    error::{Error, ErrorType},
    query::{Key, Query},
};

use super::common::{
    asset_bytes, asset_entry, created, entry_response, error_response, key_from_path,
    key_metadata, key_version, ok, ok_key, status_after_remove, submit_key, AssetListing,
    AuditResult, CanMakeResult, ContainsResult, DescriptionRequest, KeyListing, RemoveDirResult, RemoveResult,
};
use super::value_description::ValueDescription;
use crate::api_core::{
    format::{deserialize_data_entry, format_from_content_type},
    SerializationFormat,
};

macro_rules! parse_key_or_return {
    ($path:expr) => {
        match key_from_path(&$path) {
            Ok(key) => key,
            Err(e) => return error_response(&e, "Failed to parse key"),
        }
    };
}

/// The key's `AssetInfo` after a mutation, as a 200.
async fn info_after<E: Environment>(env: &EnvRef<E>, key: &Key, message: &str) -> Response {
    match env.get_asset_manager().get_asset_info(key).await {
        Ok(info) => ok_key(info, message, key),
        Err(e) => error_response(&e, "Failed to describe the asset"),
    }
}

/// `" (ignored: a, b)"`, or nothing.
fn ignored_suffix(what: &str, names: &[String]) -> String {
    if names.is_empty() {
        String::new()
    } else {
        format!("; ignored {}: {}", what, names.join(", "))
    }
}

// ---------------------------------------------------------------- mode (a): request and wait

/// `GET key/data/{*key}` — evaluate if needed, wait, serve the bytes.
pub async fn key_get_data_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(key_path): Path<String>,
) -> Response {
    let key = parse_key_or_return!(key_path);
    match env.get_asset_manager().get(&key).await {
        Ok(asset) => asset_bytes(&asset).await,
        Err(e) => error_response(&e, "Failed to get asset"),
    }
}

/// `GET key/entry/{*key}` — as `data`, as a negotiated `DataEntry`.
pub async fn key_get_entry_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(key_path): Path<String>,
    headers: HeaderMap,
    AxumQuery(params): AxumQuery<HashMap<String, String>>,
) -> Response {
    let key = parse_key_or_return!(key_path);
    match env.get_asset_manager().get(&key).await {
        Ok(asset) => asset_entry(&asset, &headers, &params).await,
        Err(e) => error_response(&e, "Failed to get asset"),
    }
}

// ---------------------------------------------------------------- mode (b): submit

/// `POST|GET key/submit/{*key}` — request the evaluation, answer with `AssetInfo` at once.
pub async fn key_submit_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(key_path): Path<String>,
) -> Response {
    let key = parse_key_or_return!(key_path);
    match submit_key(&env, &key).await {
        Ok(info) => ok_key(info, "Submitted", &key),
        Err(e) => error_response(&e, "Failed to submit"),
    }
}

// ---------------------------------------------------------------- mode (c): observe

/// `GET key/info/{*key}` — the live asset, else the stored entry, else the recipe.
pub async fn key_info_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(key_path): Path<String>,
) -> Response {
    let key = parse_key_or_return!(key_path);
    match env.get_asset_manager().get_asset_info(&key).await {
        Ok(info) => ok_key(info, "Asset info", &key),
        Err(e) => error_response(&e, "Asset info not available"),
    }
}

/// `GET key/metadata/{*key}` — the live, stored or recipe metadata record.
pub async fn key_get_metadata_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(key_path): Path<String>,
) -> Response {
    let key = parse_key_or_return!(key_path);
    match key_metadata(&env, &key).await {
        Ok(metadata) => ok_key(metadata, "Asset metadata", &key),
        Err(e) => error_response(&e, "Asset metadata not available"),
    }
}

/// `GET key/version/{*key}` — all zeros when the key has no version.
pub async fn key_version_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(key_path): Path<String>,
) -> Response {
    let key = parse_key_or_return!(key_path);
    match key_version(&env, &key).await {
        Ok(version) => ok_key(version, "Asset version", &key),
        Err(e) => error_response(&e, "Failed to read the version"),
    }
}

/// `GET key/contains/{*key}` — stored, or listed by the recipe provider.
pub async fn key_contains_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(key_path): Path<String>,
) -> Response {
    let key = parse_key_or_return!(key_path);
    match env.get_asset_manager().contains(&key).await {
        Ok(contains) => ok_key(ContainsResult { contains }, "Contains", &key),
        Err(e) => error_response(&e, "Failed to check the key"),
    }
}

/// `GET key/can_make/{*key}` — stored, or producible by the recipe provider (listed or not).
pub async fn key_can_make_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(key_path): Path<String>,
) -> Response {
    let key = parse_key_or_return!(key_path);
    match env.get_asset_manager().can_make(&key).await {
        Ok(can_make) => ok_key(CanMakeResult { can_make }, "CanMake", &key),
        Err(e) => error_response(&e, "Failed to check the key"),
    }
}

/// `GET key/recover/{*key}` — the last known value whatever its status (an `Expired` one
/// included), as a negotiated `DataEntry`. Never evaluates.
pub async fn key_recover_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(key_path): Path<String>,
    headers: HeaderMap,
    AxumQuery(params): AxumQuery<HashMap<String, String>>,
) -> Response {
    let key = parse_key_or_return!(key_path);
    match env.get_asset_manager().get_binary_any_status(&key).await {
        Ok(Some((data, metadata))) => entry_response(&data, &metadata, &headers, &params),
        Ok(None) => error_response(&Error::key_not_found(&key), "No value to recover"),
        Err(e) => error_response(&e, "Failed to recover the value"),
    }
}

async fn listdir<E: Environment>(
    env: &EnvRef<E>,
    key: &Key,
    params: &HashMap<String, String>,
) -> Response {
    let manager = env.get_asset_manager();
    let deep = params
        .get("deep")
        .map(|value| value == "true" || value == "1")
        .unwrap_or(false);
    if deep {
        match manager.listdir_keys_deep(key).await {
            Ok(keys) => ok_key(
                KeyListing {
                    keys: keys.iter().map(|k| k.encode()).collect(),
                },
                "Directory keys",
                key,
            ),
            Err(e) => error_response(&e, "Failed to list the directory"),
        }
    } else {
        match manager.listdir_asset_info(key).await {
            Ok(assets) => ok_key(AssetListing { assets }, "Directory listing", key),
            Err(e) => error_response(&e, "Failed to list the directory"),
        }
    }
}

/// `GET key/listdir/{*key}` (`?deep=true` for every key under it).
pub async fn key_listdir_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(key_path): Path<String>,
    AxumQuery(params): AxumQuery<HashMap<String, String>>,
) -> Response {
    let key = parse_key_or_return!(key_path);
    listdir(&env, &key, &params).await
}

/// `GET key/listdir` — the root directory.
pub async fn key_listdir_root_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    AxumQuery(params): AxumQuery<HashMap<String, String>>,
) -> Response {
    listdir(&env, &Key::new(), &params).await
}

// ---------------------------------------------------------------- mutations

async fn write_value<E: Environment>(
    env: &EnvRef<E>,
    key: &Key,
    description: ValueDescription,
    data: &[u8],
    message: String,
) -> Response {
    let manager = env.get_asset_manager();
    let registry = env.get_type_registry();
    // `AssetInfo.data_format` is the *effective* format, which may be derived from the filename
    // (`a.txt` → `txt`) rather than declared. Inheriting one the type cannot be written in would
    // refuse a plain rewrite of the same key with a 422, so such a format is not inherited.
    let previous = manager.get_asset_info(key).await.ok().map(|mut info| {
        let unsupported = info
            .data_format
            .as_deref()
            .is_some_and(|format| !registry.supports_data_format(&info.type_identifier, format));
        if unsupported {
            info.data_format = None;
        }
        info
    });
    let record = match description
        .or_previous(previous.as_ref())
        .into_metadata_record(registry)
    {
        Ok(record) => record,
        Err(e) => return error_response(&e, "Invalid value description"),
    };
    if let Err(e) = manager.set_binary(key, data, record).await {
        return error_response(&e, "Failed to set the value");
    }
    match manager.get_asset_info(key).await {
        Ok(info) => created(info, message, key),
        Err(e) => error_response(&e, "Value set, but it cannot be described"),
    }
}

/// `POST key/data/{*key}` — write raw bytes as a `Source` (or an `Override` of a recipe key). The
/// query parameters `type_identifier`, `data_format`, `media_type`, `title` and `description`
/// describe the value; any other parameter is ignored and named in the message.
pub async fn key_post_data_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(key_path): Path<String>,
    AxumQuery(params): AxumQuery<HashMap<String, String>>,
    body: Bytes,
) -> Response {
    let key = parse_key_or_return!(key_path);
    let (description, ignored) = ValueDescription::from_params(&params);
    let message = format!("Value set{}", ignored_suffix("parameters", &ignored));
    write_value(&env, &key, description, &body, message).await
}

/// `POST key/entry/{*key}` — write a `DataEntry`. Only the five `ValueDescription` fields of its
/// metadata are taken; the others are dropped and named in the message.
pub async fn key_post_entry_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(key_path): Path<String>,
    headers: HeaderMap,
    AxumQuery(params): AxumQuery<HashMap<String, String>>,
    body: Bytes,
) -> Response {
    let key = parse_key_or_return!(key_path);
    let format = match params.get("format").map(|s| s.to_lowercase()).as_deref() {
        Some("cbor") => SerializationFormat::Cbor,
        Some("bincode") => SerializationFormat::Bincode,
        Some("json") => SerializationFormat::Json,
        Some(_) | None => format_from_content_type(&headers).unwrap_or(SerializationFormat::Cbor),
    };
    let entry = match deserialize_data_entry(&body, format) {
        Ok(entry) => entry,
        Err(e) => {
            let e = Error::from_error(
                ErrorType::ParameterError,
                format!("Cannot decode the entry: {e}"),
            )
            .with_key(&key);
            return error_response(&e, "Invalid entry");
        }
    };
    let (description, dropped) = match ValueDescription::from_json(&entry.metadata) {
        Ok(parsed) => parsed,
        Err(e) => return error_response(&e, "Invalid entry metadata"),
    };
    let message = format!("Value set{}", ignored_suffix("metadata fields", &dropped));
    write_value(&env, &key, description, &entry.data, message).await
}

/// `POST key/metadata/{*key}` — always refused: asset metadata is owned by the asset manager.
pub async fn key_post_metadata_handler<E: Environment>(
    State(_env): State<EnvRef<E>>,
    Path(key_path): Path<String>,
) -> Response {
    let key = parse_key_or_return!(key_path);
    let e = Error::not_supported(
        "Asset metadata is owned by the asset manager; use POST description for a Source asset's \
         title and description"
            .to_string(),
    )
    .with_key(&key);
    error_response(&e, "Metadata cannot be set")
}

/// `DELETE key/data|entry/{*key}`, `GET key/remove/{*key}` — status-aware removal
/// (`AssetManager::remove`). A directory is refused with 409.
pub async fn key_remove_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(key_path): Path<String>,
) -> Response {
    let key = parse_key_or_return!(key_path);
    match env.get_asset_manager().remove(&key).await {
        Ok(()) => {
            let new_status = status_after_remove(&env, &key).await;
            ok_key(
                RemoveResult {
                    removed: true,
                    new_status,
                },
                "Removed",
                &key,
            )
        }
        Err(mut e) => {
            if e.error_type == ErrorType::StatusConflict {
                e.message = format!("{}; use key/removedir to remove a directory", e.message);
            }
            error_response(&e, "Failed to remove")
        }
    }
}

/// `DELETE|GET key/removedir/{*key}` — remove a directory and everything in it.
pub async fn key_removedir_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(key_path): Path<String>,
) -> Response {
    let key = parse_key_or_return!(key_path);
    match env.get_asset_manager().removedir(&key).await {
        Ok(()) => ok_key(RemoveDirResult { removed: true }, "Directory removed", &key),
        Err(e) => error_response(&e, "Failed to remove the directory"),
    }
}

/// `PUT|GET key/makedir/{*key}` — 201 with the directory's `AssetInfo`.
pub async fn key_makedir_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(key_path): Path<String>,
) -> Response {
    let key = parse_key_or_return!(key_path);
    let info = match env.get_asset_manager().makedir(&key).await {
        Ok(asset) => asset.get_asset_info().await,
        Err(e) => Err(e),
    };
    match info {
        Ok(info) => created(info, "Directory created", &key),
        Err(e) => error_response(&e, "Failed to create the directory"),
    }
}

/// `POST key/description/{*key}` (JSON body), `GET key/description/{*key}?title=&description=` —
/// set the title and/or description of a `Source`. Body fields win over query parameters.
pub async fn key_description_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(key_path): Path<String>,
    AxumQuery(params): AxumQuery<DescriptionRequest>,
    body: Bytes,
) -> Response {
    let key = parse_key_or_return!(key_path);
    let from_body = if body.is_empty() {
        DescriptionRequest::default()
    } else {
        match serde_json::from_slice::<DescriptionRequest>(&body) {
            Ok(request) => request,
            Err(e) => {
                let e = Error::from_error(
                    ErrorType::ParameterError,
                    format!("Description body must be a JSON object with title/description: {e}"),
                )
                .with_key(&key);
                return error_response(&e, "Invalid description");
            }
        }
    };
    let title = from_body.title.or(params.title);
    let description = from_body.description.or(params.description);
    if let Err(e) = env
        .get_asset_manager()
        .set_description(&key, title, description)
        .await
    {
        return error_response(&e, "Failed to set the description");
    }
    info_after(&env, &key, "Description set").await
}

/// `POST|GET key/expire/{*key}` — expire and cascade to dependents.
pub async fn key_expire_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(key_path): Path<String>,
) -> Response {
    let key = parse_key_or_return!(key_path);
    if let Err(e) = env.get_asset_manager().expire(&key).await {
        return error_response(&e, "Failed to expire");
    }
    info_after(&env, &key, "Expired").await
}

/// `POST|GET key/override/{*key}` — pin the current value as `Override`; a `Source` is unchanged.
pub async fn key_override_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(key_path): Path<String>,
) -> Response {
    let key = parse_key_or_return!(key_path);
    if let Err(e) = env.get_asset_manager().to_override(&key).await {
        return error_response(&e, "Failed to override");
    }
    info_after(&env, &key, "Override set").await
}

/// `POST|GET key/cancel/{*key}` — cancel the live asset. Looks it up without creating one, so it
/// never starts the evaluation it cancels; 404 when no live asset holds the key.
pub async fn key_cancel_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(key_path): Path<String>,
) -> Response {
    let key = parse_key_or_return!(key_path);
    let Some(asset) = env.get_asset_manager().lookup_key_asset(&key) else {
        let e = Error::from_error(
            ErrorType::NotAvailable,
            format!("No live asset for '{}': nothing to cancel", key.encode()),
        )
        .with_key(&key);
        return error_response(&e, "Nothing to cancel");
    };
    let result = match asset.cancel().await {
        Ok(()) => asset.get_asset_info().await,
        Err(e) => Err(e),
    };
    match result {
        Ok(info) => ok_key(info, "Cancel requested", &key),
        Err(e) => error_response(&e, "Failed to cancel"),
    }
}

// ---------------------------------------------------------------- admin

/// `POST|GET admin/audit/{*key}` — verify the recorded dependency versions reachable from a key.
pub async fn key_audit_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(key_path): Path<String>,
) -> Response {
    let key = parse_key_or_return!(key_path);
    match env
        .get_asset_manager()
        .trigger_dependency_audit(&Query::from(&key))
        .await
    {
        Ok(report) => ok_key(AuditResult::from(report), "Audit finished", &key),
        Err(e) => error_response(&e, "Audit failed"),
    }
}

/// `POST|GET admin/audit` — audit every gap the dependency graph knows of.
pub async fn audit_all_handler<E: Environment>(State(env): State<EnvRef<E>>) -> Response {
    match env
        .get_asset_manager()
        .trigger_dependency_audit_all_registered()
        .await
    {
        Ok(report) => ok(AuditResult::from(report), "Audit finished"),
        Err(e) => error_response(&e, "Audit failed"),
    }
}

/// `POST|GET admin/refresh_command_versions` — re-register command versions and expire what
/// changed.
pub async fn refresh_command_versions_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
) -> Response {
    match env
        .get_asset_manager()
        .refresh_command_versions_and_expire()
        .await
    {
        Ok(()) => ok((), "Command versions refreshed"),
        Err(e) => error_response(&e, "Failed to refresh command versions"),
    }
}
