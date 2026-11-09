//! The web folder and the index page: static files served from the store.
//!
//! A Liquers server can serve a web application stored like any other data. `GET
//! {prefix}/web/{path}` answers with the bytes stored at `<root>/{path}` (root `web` by default,
//! [`WebSetting`](crate::server::WebSetting)), and `/` and `/index.html` redirect a browser there.
//! The route reads stored files only: a key with no stored data is 404 even when a recipe could
//! make it (`WEB-FOLDER-SERVES-STORED-FILES-ONLY`).
//!
//! # Main components
//!
//! - [`web_routes`]: `{prefix}/web` (308 to `{prefix}/web/`), `{prefix}/web/` and
//!   `{prefix}/web/{*path}`. A folder is answered with its `index.html` when the path ends with
//!   `/`, and with a 308 to the slash form when it does not.
//! - [`index_routes`]: `/` and `/index.html`, a 307 to `{prefix}/web/` (`INDEX-PAGE-IS-A-FIXED-REDIRECT`).
//! - The response: `Content-Type` from the metadata's declared media type, else the key's
//!   extension, else the stored data format (a text stored from Python records `txt`, so the
//!   extension comes before it); `X-Content-Type-Options: nosniff`; `Cache-Control: no-cache`;
//!   an `ETag` (the content hash) with `If-None-Match` answered by 304; `Last-Modified` when the
//!   store reports a modification time.
//!
//! # Where it is used
//!
//! - [`LiquersServerBuilder::router`](crate::server::LiquersServerBuilder::router) mounts both
//!   while `web` is on (and `index_routes` unless `without_index`).
//! - `liquers-py`'s `env.serve()`, through that builder.
//! - `tests/server_web.rs`.
//!
//! A host composing its own router can merge them directly.
//!
//! # Related concepts
//!
//! - The store API (`{prefix}/api/store/data/…`) also returns stored bytes, with the store's own
//!   content-type rule and no caching headers; the web route is for browsers.
//! - The query API (`{prefix}/q/…`) evaluates; the web route never does.
//!
//! Pages served from a writable store can be replaced through the store API: a stored page is a
//! stored-XSS risk, so a deployment mounts its `web` folder read-only (`read_only: true` on that
//! mount). See `specs/reference/WEB_API_SPECIFICATION.md` §Web folder.
//!
//! # Examples
//!
//! ```
//! use axum::{body::Body, http::Request};
//! use liquers_axum::web::{index_routes, web_routes};
//! use liquers_core::{context::{Environment, SimpleEnvironment}, parse::parse_key, value::Value};
//! use tower::ServiceExt;
//!
//! # tokio::runtime::Runtime::new().unwrap().block_on(async {
//! let envref = SimpleEnvironment::<Value>::new().to_ref();
//! let app = web_routes::<SimpleEnvironment<Value>>("/liquer", parse_key("web").unwrap())
//!     .merge(index_routes::<SimpleEnvironment<Value>>("/liquer"))
//!     .with_state(envref);
//! let response = app.oneshot(Request::get("/").body(Body::empty()).unwrap()).await.unwrap();
//! assert_eq!(response.headers()["location"], "/liquer/web/");
//! # });
//! ```
//!
//! More: `liquers-axum/tests/server_web.rs` (content types, folders, ETag, Last-Modified).
use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, HeaderMap, HeaderValue, Response, StatusCode, Uri},
    response::IntoResponse,
    routing::get,
    Extension, Router,
};
use liquers_core::context::{EnvRef, Environment};
use liquers_core::error::Error;
use liquers_core::media_type::file_extension_to_media_type;
use liquers_core::metadata::{Metadata, Version};
use liquers_core::parse::parse_key;
use liquers_core::query::Key;

use crate::api_core::{error_to_detail, ApiResponse};
use crate::axum_integration::build_or_500;

#[derive(Clone)]
struct WebFolder {
    root: Key,
}

/// The web folder's routes under `prefix` (no trailing `/`; `""` for the root), serving store
/// folder `root`.
pub fn web_routes<E: Environment>(prefix: &str, root: Key) -> Router<EnvRef<E>> {
    let prefix = prefix.trim_end_matches('/');
    let base = format!("{prefix}/web");
    let folder = WebFolder { root };
    let slash = format!("{base}/");
    Router::new()
        .route(&base, get(move || async move { redirect(StatusCode::PERMANENT_REDIRECT, &slash) }))
        .route(&format!("{base}/"), get(web_root_handler::<E>))
        .route(&format!("{base}/{{*path}}"), get(web_path_handler::<E>))
        .layer(Extension(folder))
}

/// `/` and `/index.html`, redirecting (307) to `{prefix}/web/`.
pub fn index_routes<E: Environment>(prefix: &str) -> Router<EnvRef<E>> {
    let target = format!("{}/web/", prefix.trim_end_matches('/'));
    let again = target.clone();
    Router::new()
        .route("/", get(move || async move { redirect(StatusCode::TEMPORARY_REDIRECT, &target) }))
        .route(
            "/index.html",
            get(move || async move { redirect(StatusCode::TEMPORARY_REDIRECT, &again) }),
        )
}

fn redirect(status: StatusCode, location: &str) -> Response<Body> {
    build_or_500(
        Response::builder().status(status).header(header::LOCATION, location),
        Body::empty(),
    )
}

async fn web_root_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Extension(folder): Extension<WebFolder>,
    uri: Uri,
    headers: HeaderMap,
) -> Response<Body> {
    web_file(env, &folder, "", &uri, &headers).await
}

async fn web_path_handler<E: Environment>(
    State(env): State<EnvRef<E>>,
    Extension(folder): Extension<WebFolder>,
    Path(path): Path<String>,
    uri: Uri,
    headers: HeaderMap,
) -> Response<Body> {
    web_file(env, &folder, &path, &uri, &headers).await
}

fn not_found(root: &Key, path: &str) -> Response<Body> {
    let error = Error::asset_not_supported(root, format!("'{path}' is not a store key"));
    ApiResponse::<()>::error(error_to_detail(&error), "Not found").into_response()
}

async fn web_file<E: Environment>(
    env: EnvRef<E>,
    folder: &WebFolder,
    path: &str,
    uri: &Uri,
    headers: &HeaderMap,
) -> Response<Body> {
    let folder_request = path.is_empty() || path.ends_with('/');
    let relative = path.trim_matches('/');
    let root = folder.root.encode();
    let joined = if relative.is_empty() { root } else { format!("{root}/{relative}") };
    let mut key = match parse_key(&joined) {
        Ok(key) => key,
        Err(_) => return not_found(&folder.root, path),
    };
    let store = env.get_async_store();
    if folder_request {
        key = key.join("index.html");
    } else if store.is_dir(&key).await.unwrap_or(false) {
        return redirect(StatusCode::PERMANENT_REDIRECT, &format!("{}/", uri.path()));
    }
    let (data, metadata) = match store.get(&key).await {
        Ok(found) => found,
        Err(e) => {
            return ApiResponse::<()>::error(error_to_detail(&e), "Failed to retrieve web file")
                .into_response()
        }
    };
    let etag = format!("\"{}\"", Version::from_content(&data));
    let mut builder = Response::builder()
        .header(header::ETAG, &etag)
        .header(header::CACHE_CONTROL, "no-cache")
        .header("x-content-type-options", "nosniff");
    if let Ok(Some(facts)) = store.data_facts(&key).await {
        if let Some(modified) = facts.modified.as_ref().and_then(|t| http_date(t.as_str())) {
            builder = builder.header(header::LAST_MODIFIED, modified);
        }
    }
    if if_none_match(headers, &etag) {
        return build_or_500(builder.status(StatusCode::NOT_MODIFIED), Body::empty());
    }
    let content_type = content_type_for(&metadata, &key);
    build_or_500(
        builder
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, content_type),
        Body::from(data),
    )
}

/// The web route's content type: a declared media type, else the key's extension, else the
/// stored data format.
fn content_type_for(metadata: &Metadata, key: &Key) -> String {
    if let Some(declared) = declared_media_type(metadata) {
        return declared;
    }
    if let Some(extension) = key.extension() {
        let extension = extension.to_ascii_lowercase();
        let from_extension = file_extension_to_media_type(&extension);
        if from_extension != "application/octet-stream" {
            return from_extension.to_string();
        }
    }
    metadata.get_media_type()
}

fn declared_media_type(metadata: &Metadata) -> Option<String> {
    match metadata {
        Metadata::MetadataRecord(record) => record.declared_media_type().map(str::to_string),
        Metadata::LegacyMetadata(serde_json::Value::Object(object)) => ["media_type", "mimetype"]
            .iter()
            .filter_map(|field| object.get(*field).and_then(|v| v.as_str()))
            .find(|value| !value.is_empty())
            .map(str::to_string),
        Metadata::LegacyMetadata(_) => None,
    }
}

/// Whether `If-None-Match` matches `etag`: `*`, or one of a list of tags, weak (`W/`) or strong.
fn if_none_match(headers: &HeaderMap, etag: &str) -> bool {
    headers
        .get_all(header::IF_NONE_MATCH)
        .iter()
        .filter_map(|value: &HeaderValue| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .map(|tag| tag.trim())
        .any(|tag| tag == "*" || tag.trim_start_matches("W/") == etag)
}

/// An RFC 3339 time as an HTTP date (`Sun, 06 Nov 1994 08:49:37 GMT`).
fn http_date(rfc3339: &str) -> Option<String> {
    chrono::DateTime::parse_from_rfc3339(rfc3339)
        .ok()
        .map(|t| t.with_timezone(&chrono::Utc).format("%a, %d %b %Y %H:%M:%S GMT").to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_type_prefers_declared_then_extension_then_format() {
        let key = parse_key("web/index.html").expect("key");
        let mut record = liquers_core::metadata::MetadataRecord::new();
        record.data_format = Some("txt".to_string());
        assert_eq!(content_type_for(&Metadata::MetadataRecord(record.clone()), &key), "text/html");
        record.media_type = Some("text/plain".to_string());
        assert_eq!(content_type_for(&Metadata::MetadataRecord(record), &key), "text/plain");
        let unknown = parse_key("web/blob.unknownext").expect("key");
        let mut record = liquers_core::metadata::MetadataRecord::new();
        record.data_format = Some("json".to_string());
        assert_eq!(content_type_for(&Metadata::MetadataRecord(record), &unknown), "application/json");
    }

    #[test]
    fn if_none_match_forms_and_http_date() {
        let mut headers = HeaderMap::new();
        headers.insert(header::IF_NONE_MATCH, HeaderValue::from_static("\"a\", W/\"b\""));
        assert!(if_none_match(&headers, "\"b\""));
        assert!(if_none_match(&headers, "\"a\""));
        assert!(!if_none_match(&headers, "\"c\""));
        headers.insert(header::IF_NONE_MATCH, HeaderValue::from_static("*"));
        assert!(if_none_match(&headers, "\"c\""));
        assert!(!if_none_match(&HeaderMap::new(), "\"c\""));
        assert_eq!(http_date("1994-11-06T08:49:37Z").as_deref(), Some("Sun, 06 Nov 1994 08:49:37 GMT"));
        assert_eq!(http_date("not a time"), None);
    }
}
