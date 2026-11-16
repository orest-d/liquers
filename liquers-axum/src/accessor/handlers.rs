//! The HTTP handlers of the accessor API: `q/` (read and conditional write), `info/`, `log/` and
//! `describe/`.
//!
//! Every handler resolves its path to an accessor first ([`resolve`]): the query's value when it
//! is one, else a read-only accessor of the query's asset. Tokens travel as quoted `ETag`s.
//!
//! # Main components
//!
//! - [`get_handler`], [`put_handler`]: `GET` and `PUT {base}/q/{*query}`.
//! - [`info_handler`], [`log_handler`], [`describe_handler`]: the observe routes.
//! - [`SetResult`]: the body of a successful `PUT`.
//!
//! # Where it is used
//!
//! - [`AccessorApiBuilder::build`](super::AccessorApiBuilder::build) mounts them.
//!
//! # Related concepts
//!
//! - `liquers_core::accessor::ValueAccessor::set_if`: the compare-and-set a `PUT` is.
//! - [`crate::assets::query_handlers`]: the asset routes these follow in shape.
//!
//! # Examples
//!
//! ```text
//! GET /liquer/api/acc/q/-R-key/config.yaml/-/ns-acc/accessor/field-title
//!   → 200 "Dashboard", ETag: "98214eb4706d2e44-ea6f…"
//! PUT …same path…  If-Match: "98214eb4706d2e44-ea6f…"  Content-Type: application/json  "Sales"
//!   → 200, the new ETag; a stale token → 412 with the current ETag
//! ```
//!
//! More: `tests/accessor_api.rs`.

use std::collections::HashMap;
use std::sync::Arc;

use axum::{
    body::Bytes,
    extract::{Path, Query as AxumQuery, State},
    http::{
        header::{CONTENT_TYPE, ETAG, IF_MATCH, IF_NONE_MATCH},
        HeaderMap, HeaderValue, StatusCode,
    },
    response::{IntoResponse, Response},
};
use liquers_core::accessor::{
    conflict_token, resolve_accessor, AccessorInfo, AccessorSnapshot, Fingerprint, SetOptions,
    ValueAccessor,
};
use liquers_core::context::{EnvRef, Environment};
use liquers_core::error::{Error, ErrorType};
use liquers_core::media_type::file_extension_to_media_type;
use liquers_core::metadata::Status;
use liquers_core::query::Query;
use liquers_core::value::{DefaultValueSerializer, ValueInterface};
use serde::Serialize;

use crate::assets::common::{error_response, error_with_metadata, ok_query, query_from_path};

/// The accessor a query names (`liquers_core::accessor::resolve_accessor`).
pub(crate) async fn resolve<E: Environment>(
    env: &EnvRef<E>,
    query: &Query,
) -> Result<Arc<dyn ValueAccessor<E::Value>>, Error> {
    resolve_accessor(env, query).await
}

/// The parsed query and its accessor, or the error answer.
async fn accessor_for<E: Environment>(
    env: &EnvRef<E>,
    path: &str,
) -> Result<(Query, Arc<dyn ValueAccessor<E::Value>>), Response> {
    let query =
        query_from_path(path).map_err(|e| error_response(&e, "Failed to parse the query"))?;
    let accessor = resolve(env, &query)
        .await
        .map_err(|e| error_response(&e, "Failed to resolve the accessor"))?;
    Ok((query, accessor))
}

/// An `AccessorInfo` as JSON, with its fingerprint also as `etag` (absent when the value does not
/// exist).
pub(crate) fn info_json(info: &AccessorInfo) -> serde_json::Value {
    let mut json = serde_json::to_value(info).unwrap_or(serde_json::Value::Null);
    if let serde_json::Value::Object(map) = &mut json {
        let etag = info
            .fingerprint
            .map(|fp| serde_json::Value::String(fp.token()))
            .unwrap_or(serde_json::Value::Null);
        map.insert("etag".into(), etag);
    }
    json
}

fn etag_header(fingerprint: &Fingerprint) -> Option<HeaderValue> {
    HeaderValue::from_str(&format!("\"{}\"", fingerprint.token())).ok()
}

fn with_etag(mut response: Response, fingerprint: Option<&Fingerprint>) -> Response {
    if let Some(value) = fingerprint.and_then(etag_header) {
        response.headers_mut().insert(ETAG, value);
    }
    response
}

fn with_status(mut response: Response, status: StatusCode) -> Response {
    *response.status_mut() = status;
    response
}

/// The part's bytes in `format` (default: the part's own data format), with its `ETag`.
fn value_response<V: ValueInterface>(
    snapshot: &AccessorSnapshot<V>,
    format: Option<&String>,
) -> Response {
    let format = format
        .cloned()
        .unwrap_or_else(|| snapshot.state.effective_data_format());
    match snapshot.state.as_bytes_with_data_format(&format) {
        Ok(bytes) => {
            let mut headers = HeaderMap::new();
            if let Ok(media) = HeaderValue::from_str(file_extension_to_media_type(&format)) {
                headers.insert(CONTENT_TYPE, media);
            }
            if let Some(etag) = etag_header(&snapshot.fingerprint) {
                headers.insert(ETAG, etag);
            }
            (headers, bytes).into_response()
        }
        Err(e) => error_response(&e, "Failed to serialize the value"),
    }
}

/// The data format of a `PUT` body: `?format=`, else the `Content-Type`, else JSON.
fn body_format(params: &HashMap<String, String>, headers: &HeaderMap) -> String {
    if let Some(format) = params.get("format") {
        return format.clone();
    }
    let content_type = headers
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.split(';').next().unwrap_or("").trim().to_ascii_lowercase())
        .unwrap_or_default();
    match content_type.as_str() {
        "text/plain" => "txt",
        "application/yaml" | "application/x-yaml" | "text/yaml" => "yaml",
        "application/octet-stream" => "b",
        _ => "json",
    }
    .to_string()
}

/// What a `PUT` expects: `Some(token)` for `If-Match`, `None` for `If-None-Match: *`.
async fn expectation<V: ValueInterface>(
    accessor: &Arc<dyn ValueAccessor<V>>,
    headers: &HeaderMap,
) -> Result<Option<Fingerprint>, Response> {
    let header = |name| headers.get(name).and_then(|v: &HeaderValue| v.to_str().ok());
    match (header(IF_MATCH), header(IF_NONE_MATCH)) {
        // `*` is "the value as it is now": read it (`info` alone may not have seen the root yet).
        (Some("*"), _) => match accessor.get().await {
            Ok(snapshot) => Ok(Some(snapshot.fingerprint)),
            Err(e) => match accessor.info().await {
                Ok(info) if info.status == Status::None => Err(with_status(
                    error_response(
                        &Error::from_error(
                            ErrorType::StatusConflict,
                            "If-Match: * needs a value that exists".to_string(),
                        ),
                        "The value does not exist",
                    ),
                    StatusCode::PRECONDITION_FAILED,
                )),
                Ok(_) | Err(_) => Err(error_response(&e, "Failed to read the value")),
            },
        },
        (Some(token), _) => Fingerprint::from_token(token)
            .map(Some)
            .map_err(|e| error_response(&e, "If-Match is not an accessor token")),
        (None, Some("*")) => Ok(None),
        (None, Some(_)) => Err(error_response(
            &Error::parse_error("If-None-Match takes only '*'".to_string()),
            "Unsupported precondition",
        )),
        (None, None) => Err(with_status(
            error_response(
                &Error::general_error(
                    "a PUT needs If-Match: <token> or If-None-Match: *".to_string(),
                ),
                "Precondition required",
            ),
            StatusCode::PRECONDITION_REQUIRED,
        )),
    }
}

/// `GET {base}/q/{*query}`: the part's value (`?format=` picks the data format), with
/// `ETag: "<token>"`. A value that does not exist answers 404 with the accessor's info in
/// `error.metadata`.
pub async fn get_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(path): Path<String>,
    AxumQuery(params): AxumQuery<HashMap<String, String>>,
) -> Response {
    let (_, accessor) = match accessor_for(&env, &path).await {
        Ok(found) => found,
        Err(response) => return response,
    };
    match accessor.get().await {
        Ok(snapshot) => value_response(&snapshot, params.get("format")),
        Err(e) => {
            let info = accessor.info().await.ok().map(|i| info_json(&i));
            error_with_metadata(&e, info, "The value cannot be read")
        }
    }
}

/// The answer to a successful `PUT`.
#[derive(Debug, Clone, Serialize)]
pub struct SetResult {
    /// The new token of the part.
    pub etag: String,
}

/// `PUT {base}/q/{*query}`: a conditional set. `If-Match: <token>` (or `*`, the value as it is)
/// sets a value that exists, `If-None-Match: *` creates one. The body is read in `?format=`, else
/// the format its `Content-Type` names, else JSON; `?type=` names its type identifier. Answers
/// 200 with the new `ETag`, 412 with the current one on a conflict, 428 without a precondition
/// and 405 for a read-only accessor.
pub async fn put_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(path): Path<String>,
    AxumQuery(params): AxumQuery<HashMap<String, String>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let (query, accessor) = match accessor_for(&env, &path).await {
        Ok(found) => found,
        Err(response) => return response,
    };
    if !accessor.capabilities().write {
        return with_status(
            error_response(
                &Error::not_supported(format!("'{}' is read-only", query.encode())),
                "The accessor is read-only",
            ),
            StatusCode::METHOD_NOT_ALLOWED,
        );
    }
    let expected = match expectation(&accessor, &headers).await {
        Ok(expected) => expected,
        Err(response) => return response,
    };
    let format = body_format(&params, &headers);
    let type_identifier = params.get("type").map(String::as_str).unwrap_or("");
    let value = match <E::Value as DefaultValueSerializer>::deserialize_from_bytes(
        &body,
        type_identifier,
        &format,
    ) {
        Ok(value) => value,
        Err(e) => {
            return with_status(
                error_response(&e, "The body cannot be read as a value"),
                StatusCode::BAD_REQUEST,
            )
        }
    };
    match accessor.set_if(value, expected, SetOptions::default()).await {
        Ok(snapshot) => with_etag(
            ok_query(
                SetResult {
                    etag: snapshot.fingerprint.token(),
                },
                "Value set",
                &query,
            ),
            Some(&snapshot.fingerprint),
        ),
        Err(e) if e.error_type == ErrorType::StatusConflict => with_etag(
            with_status(
                error_response(&e, "The value changed since the token was read"),
                StatusCode::PRECONDITION_FAILED,
            ),
            conflict_token(&e).as_ref(),
        ),
        Err(e) => error_response(&e, "Failed to set the value"),
    }
}

/// `GET {base}/info/{*query}`: the accessor's `AccessorInfo` (status, message, progress, log
/// length, `etag`). Reading it evaluates no value.
pub async fn info_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(path): Path<String>,
) -> Response {
    let (query, accessor) = match accessor_for(&env, &path).await {
        Ok(found) => found,
        Err(response) => return response,
    };
    match accessor.info().await {
        Ok(info) => with_etag(
            ok_query(info_json(&info), "Accessor info", &query),
            info.fingerprint.as_ref(),
        ),
        Err(e) => error_response(&e, "Failed to read the accessor's state"),
    }
}

/// `GET {base}/log/{*query}?since=<n>`: the log messages from index `n` (default 0).
pub async fn log_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(path): Path<String>,
    AxumQuery(params): AxumQuery<HashMap<String, String>>,
) -> Response {
    let since = match params.get("since").map(|s| s.parse::<usize>()) {
        None => 0,
        Some(Ok(n)) => n,
        Some(Err(_)) => {
            return error_response(
                &Error::parse_error("since must be a non-negative integer".to_string()),
                "Invalid since",
            )
        }
    };
    let (query, accessor) = match accessor_for(&env, &path).await {
        Ok(found) => found,
        Err(response) => return response,
    };
    match accessor.log(since).await {
        Ok(log) => ok_query(log, "Accessor log", &query),
        Err(e) => error_response(&e, "Failed to read the accessor's log"),
    }
}

/// `GET {base}/describe/{*query}`: the `PartDescription` (type, argument, command, `overridden`,
/// `read_only`).
pub async fn describe_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Path(path): Path<String>,
) -> Response {
    let (query, accessor) = match accessor_for(&env, &path).await {
        Ok(found) => found,
        Err(response) => return response,
    };
    match accessor.describe().await {
        Ok(description) => ok_query(description, "Accessor description", &query),
        Err(e) => error_response(&e, "Failed to describe the accessor"),
    }
}
