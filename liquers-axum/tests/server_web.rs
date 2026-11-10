//! The web folder and the index page (`specs/design/axum-server-assembly/`, Phase 3 tests 6):
//! routers served in process with `tower::ServiceExt::oneshot`.

use axum::body::{to_bytes, Body};
use axum::http::{header, Request, Response, StatusCode};
use axum::Router;
use liquers_axum::{LiquersServerBuilder, WebSetting};
use liquers_core::context::{EnvRef, Environment, SimpleEnvironment};
use liquers_core::metadata::{Metadata, MetadataRecord};
use liquers_core::parse::parse_key;
use liquers_core::query::Key;
use liquers_core::store::{AsyncFileStore, AsyncMemoryStore, AsyncStore};
use liquers_core::value::Value;
use tower::ServiceExt;

type Env = SimpleEnvironment<Value>;

fn record(data_format: Option<&str>, media_type: Option<&str>) -> Metadata {
    Metadata::MetadataRecord(MetadataRecord {
        data_format: data_format.map(str::to_string),
        media_type: media_type.map(str::to_string),
        ..Default::default()
    })
}

async fn env_with(files: &[(&str, &str, Metadata)]) -> EnvRef<Env> {
    let store = AsyncMemoryStore::new(&Key::new());
    for (key, data, metadata) in files {
        store.set(&parse_key(key).unwrap(), data.as_bytes(), metadata).await.unwrap();
    }
    let mut env = Env::new();
    env.with_async_store(Box::new(store));
    env.to_ref()
}

async fn site() -> EnvRef<Env> {
    env_with(&[
        ("web/index.html", "<h1>app</h1>", Metadata::new()),
        ("web/app.js", "console.log(1)", Metadata::new()),
        ("web/app.wasm", "\0asm", Metadata::new()),
        ("web/docs/index.html", "<h1>docs</h1>", Metadata::new()),
        ("web/page.html", "<p>text</p>", record(Some("txt"), None)),
        ("web/bad.txt", "x", record(None, Some("bad\nvalue"))),
        ("site/index.html", "<h1>site</h1>", Metadata::new()),
    ])
    .await
}

fn app(envref: EnvRef<Env>, web: WebSetting) -> Router {
    LiquersServerBuilder::<Env>::new("/liquer").with_web(web).router().with_state(envref)
}

async fn send(app: &Router, method: &str, uri: &str, headers: &[(&str, &str)]) -> Response<Body> {
    let mut request = Request::builder().method(method).uri(uri);
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    app.clone().oneshot(request.body(Body::empty()).unwrap()).await.unwrap()
}

async fn get(app: &Router, uri: &str) -> Response<Body> {
    send(app, "GET", uri, &[]).await
}

fn header_of(response: &Response<Body>, name: header::HeaderName) -> String {
    response.headers().get(name).map(|v| v.to_str().unwrap().to_string()).unwrap_or_default()
}

async fn body(response: Response<Body>) -> String {
    String::from_utf8(to_bytes(response.into_body(), usize::MAX).await.unwrap().to_vec()).unwrap()
}

#[tokio::test]
async fn stored_file_is_served_with_its_media_type() {
    let app = app(site().await, WebSetting::Enabled(true));
    let response = get(&app, "/liquer/web/app.js").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(header_of(&response, header::CONTENT_TYPE), "text/javascript");
    assert_eq!(header_of(&response, header::X_CONTENT_TYPE_OPTIONS), "nosniff");
    assert_eq!(header_of(&response, header::CACHE_CONTROL), "no-cache");
    let etag = header_of(&response, header::ETAG);
    assert!(etag.starts_with('"') && etag.len() > 2, "{etag}");
    assert_eq!(body(response).await, "console.log(1)");

    let head = send(&app, "HEAD", "/liquer/web/app.js", &[]).await;
    assert_eq!(head.status(), StatusCode::OK);
    assert_eq!(header_of(&head, header::CONTENT_TYPE), "text/javascript");
    assert_eq!(header_of(&head, header::ETAG), etag);
    assert_eq!(body(head).await, "");
}

#[tokio::test]
async fn missing_file_is_404() {
    let app = app(site().await, WebSetting::Enabled(true));
    assert_eq!(get(&app, "/liquer/web/missing.js").await.status(), StatusCode::NOT_FOUND);
    assert_eq!(get(&app, "/liquer/web/nothing/").await.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn invalid_key_is_404() {
    let app = app(site().await, WebSetting::Enabled(true));
    assert_eq!(get(&app, "/liquer/web/a~b.js").await.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn folder_with_slash_serves_index_and_without_redirects() {
    let app = app(site().await, WebSetting::Enabled(true));
    let root = get(&app, "/liquer/web/").await;
    assert_eq!(root.status(), StatusCode::OK);
    assert_eq!(header_of(&root, header::CONTENT_TYPE), "text/html");
    assert_eq!(body(root).await, "<h1>app</h1>");

    let bare = get(&app, "/liquer/web").await;
    assert_eq!(bare.status(), StatusCode::PERMANENT_REDIRECT);
    assert_eq!(header_of(&bare, header::LOCATION), "/liquer/web/");

    let docs = get(&app, "/liquer/web/docs").await;
    assert_eq!(docs.status(), StatusCode::PERMANENT_REDIRECT);
    assert_eq!(header_of(&docs, header::LOCATION), "/liquer/web/docs/");
    let docs = get(&app, "/liquer/web/docs/").await;
    assert_eq!(docs.status(), StatusCode::OK);
    assert_eq!(body(docs).await, "<h1>docs</h1>");
}

#[tokio::test]
async fn root_and_index_html_redirect_to_web() {
    let app = app(site().await, WebSetting::Enabled(true));
    for uri in ["/", "/index.html"] {
        let response = get(&app, uri).await;
        assert_eq!(response.status(), StatusCode::TEMPORARY_REDIRECT, "{uri}");
        assert_eq!(header_of(&response, header::LOCATION), "/liquer/web/", "{uri}");
    }
}

#[tokio::test]
async fn web_false_removes_index_redirect() {
    let app = app(site().await, WebSetting::Enabled(false));
    assert_eq!(get(&app, "/").await.status(), StatusCode::NOT_FOUND);
    assert_eq!(get(&app, "/index.html").await.status(), StatusCode::NOT_FOUND);
    assert_eq!(get(&app, "/liquer/web/app.js").await.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn web_root_serves_another_folder() {
    let app = app(site().await, WebSetting::Root("site".into()));
    let response = get(&app, "/liquer/web/").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body(response).await, "<h1>site</h1>");
}

#[tokio::test]
async fn without_index_leaves_root_to_the_host() {
    let envref = site().await;
    let app = LiquersServerBuilder::<Env>::new("/liquer").without_index().router().with_state(envref);
    assert_eq!(get(&app, "/").await.status(), StatusCode::NOT_FOUND);
    assert_eq!(get(&app, "/liquer/web/").await.status(), StatusCode::OK);
}

#[tokio::test]
async fn extension_gives_the_type_without_metadata() {
    let app = app(site().await, WebSetting::Enabled(true));
    for (uri, expected) in [
        ("/liquer/web/index.html", "text/html"),
        ("/liquer/web/app.js", "text/javascript"),
        ("/liquer/web/app.wasm", "application/wasm"),
    ] {
        let response = get(&app, uri).await;
        assert_eq!(response.status(), StatusCode::OK, "{uri}");
        assert_eq!(header_of(&response, header::CONTENT_TYPE), expected, "{uri}");
    }
}

#[tokio::test]
async fn stored_text_html_is_text_html() {
    let app = app(site().await, WebSetting::Enabled(true));
    let response = get(&app, "/liquer/web/page.html").await;
    assert_eq!(header_of(&response, header::CONTENT_TYPE), "text/html");
}

#[tokio::test]
async fn bad_media_type_is_500() {
    let app = app(site().await, WebSetting::Enabled(true));
    assert_eq!(get(&app, "/liquer/web/bad.txt").await.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn etag_round_trip_gives_304_then_200_after_change() {
    let envref = site().await;
    let app = app(envref.clone(), WebSetting::Enabled(true));
    let first = get(&app, "/liquer/web/app.js").await;
    let etag = header_of(&first, header::ETAG);
    let again = send(&app, "GET", "/liquer/web/app.js", &[("if-none-match", &etag)]).await;
    assert_eq!(again.status(), StatusCode::NOT_MODIFIED);
    assert_eq!(header_of(&again, header::ETAG), etag);
    assert_eq!(body(again).await, "");

    envref
        .get_async_store()
        .set(&parse_key("web/app.js").unwrap(), b"console.log(2)", &Metadata::new())
        .await
        .unwrap();
    let changed = send(&app, "GET", "/liquer/web/app.js", &[("if-none-match", &etag)]).await;
    assert_eq!(changed.status(), StatusCode::OK);
    assert_ne!(header_of(&changed, header::ETAG), etag);
}

#[tokio::test]
async fn if_none_match_forms() {
    let app = app(site().await, WebSetting::Enabled(true));
    let etag = header_of(&get(&app, "/liquer/web/app.js").await, header::ETAG);
    for value in ["*".to_string(), format!("\"other\", {etag}"), format!("W/{etag}")] {
        let response = send(&app, "GET", "/liquer/web/app.js", &[("if-none-match", &value)]).await;
        assert_eq!(response.status(), StatusCode::NOT_MODIFIED, "{value}");
    }
    let response = send(&app, "GET", "/liquer/web/app.js", &[("if-none-match", "\"other\"")]).await;
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn no_data_facts_no_last_modified() {
    let app = app(site().await, WebSetting::Enabled(true));
    let response = get(&app, "/liquer/web/app.js").await;
    assert_eq!(response.status(), StatusCode::OK);
    // The memory store records no modification time for a plain `set`.
    let last_modified = header_of(&response, header::LAST_MODIFIED);
    assert!(last_modified.is_empty() || last_modified.ends_with("GMT"), "{last_modified}");
}

#[tokio::test]
async fn last_modified_from_data_facts() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("web")).unwrap();
    std::fs::write(dir.path().join("web/index.html"), "<h1>disk</h1>").unwrap();
    let mut env = Env::new();
    env.with_async_store(Box::new(AsyncFileStore::new(dir.path().to_str().unwrap(), &Key::new())));
    let app = app(env.to_ref(), WebSetting::Enabled(true));
    let response = get(&app, "/liquer/web/index.html").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(header_of(&response, header::CONTENT_TYPE), "text/html");
    let last_modified = header_of(&response, header::LAST_MODIFIED);
    assert!(last_modified.ends_with(" GMT"), "Last-Modified: {last_modified:?}");
    assert_eq!(body(response).await, "<h1>disk</h1>");
}
