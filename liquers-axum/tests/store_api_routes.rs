//! Store API routes (`StoreApiBuilder`). Design: `specs/design/axum-assets-endpoints/`
//! (Phase 3, SAR01–SAR16).

use axum::{body::Body, body::to_bytes, http::{Request, StatusCode}};
use liquers_axum::store::StoreApiBuilder;
use liquers_core::{
    context::{EnvRef, Environment, SimpleEnvironment},
    metadata::Metadata,
    parse::parse_key,
    query::Key,
    store::{AsyncMemoryStore, AsyncStore},
    value::Value,
};
use tower::ServiceExt;

/// The environment and its store (`AsyncMemoryStore` is not `Clone`, so the store is read back
/// from the environment).
fn env_with_store() -> (EnvRef<SimpleEnvironment<Value>>, std::sync::Arc<dyn AsyncStore>) {
    let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
    env.with_async_store(Box::new(AsyncMemoryStore::new(&Key::new())));
    let envref = env.to_ref();
    let store = envref.get_async_store();
    (envref, store)
}

fn build_app(envref: EnvRef<SimpleEnvironment<Value>>) -> axum::Router {
    StoreApiBuilder::<SimpleEnvironment<Value>>::new("/api/store").build().with_state(envref)
}

fn build_app_with_gets(envref: EnvRef<SimpleEnvironment<Value>>) -> axum::Router {
    StoreApiBuilder::<SimpleEnvironment<Value>>::new("/api/store")
        .with_destructive_gets()
        .build()
        .with_state(envref)
}

async fn send(app: axum::Router, method: &str, uri: &str, body: Body) -> (StatusCode, serde_json::Value) {
    let resp = app.oneshot(Request::builder().method(method).uri(uri).body(body).unwrap()).await.unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null))
}

async fn send_raw(app: axum::Router, method: &str, uri: &str, body: Body) -> (StatusCode, Vec<u8>) {
    let resp = app.oneshot(Request::builder().method(method).uri(uri).body(body).unwrap()).await.unwrap();
    let status = resp.status();
    (status, to_bytes(resp.into_body(), usize::MAX).await.unwrap().to_vec())
}

#[tokio::test]
async fn sar01_data_get_success() {
    let (envref, store) = env_with_store();
    store.set(&parse_key("data/file.txt").unwrap(), b"hello", &Metadata::new()).await.unwrap();
    let (status, body) = send_raw(build_app(envref), "GET", "/api/store/data/data/file.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, b"hello");
}

#[tokio::test]
async fn sar02_data_get_not_found() {
    let (envref, _) = env_with_store();
    let (status, json) = send(build_app(envref), "GET", "/api/store/data/nonexistent/key", Body::empty()).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(json["error"]["type"], "KeyNotFound");
}

#[tokio::test]
async fn sar03_data_put_success_then_delete() {
    let (envref, _) = env_with_store();
    let app = build_app(envref);
    let (status, json) = send(app.clone(), "PUT", "/api/store/data/data/file.txt", Body::from("hello")).await;
    assert_eq!(status, StatusCode::OK, "the spec's write verb, POST, does not exist: the code uses PUT (see Spec Divergences)");
    assert_eq!(json["status"], "OK");

    let (status, json) = send(app, "DELETE", "/api/store/data/data/file.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["status"], "OK");
}

#[tokio::test]
async fn sar04_metadata_get_after_put() {
    let (envref, _) = env_with_store();
    let app = build_app(envref);
    let (status, _) = send(app.clone(), "PUT", "/api/store/data/data/file.txt", Body::from("hello")).await;
    assert_eq!(status, StatusCode::OK);

    let (status, json) = send(app, "GET", "/api/store/metadata/data/file.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["result"].is_object());
}

#[tokio::test]
async fn sar05_entry_get_success() {
    let (envref, store) = env_with_store();
    store.set(&parse_key("data/entry.txt").unwrap(), b"content", &Metadata::new()).await.unwrap();
    let (status, _) = send(build_app(envref), "GET", "/api/store/entry/data/entry.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn sar06_listdir_returns_children() {
    let (envref, store) = env_with_store();
    store.set(&parse_key("data/a.txt").unwrap(), b"a", &Metadata::new()).await.unwrap();
    store.set(&parse_key("data/b.txt").unwrap(), b"b", &Metadata::new()).await.unwrap();
    let (status, json) = send(build_app(envref), "GET", "/api/store/listdir/data", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["result"].as_array().unwrap().len() >= 2);
}

#[tokio::test]
async fn sar07_is_dir_true_and_false() {
    let (envref, _) = env_with_store();
    let am_store = envref.get_async_store();
    am_store.makedir(&parse_key("adir").unwrap()).await.unwrap();
    let app = build_app(envref);
    let (status, json) = send(app.clone(), "GET", "/api/store/is_dir/adir", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["result"], true);

    let (status, json) = send(app, "GET", "/api/store/is_dir/no_such_thing", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["result"], false);
}

#[tokio::test]
async fn sar08_contains_true_and_false() {
    let (envref, store) = env_with_store();
    store.set(&parse_key("data/x.txt").unwrap(), b"x", &Metadata::new()).await.unwrap();
    let app = build_app(envref);
    let (status, json) = send(app.clone(), "GET", "/api/store/contains/data/x.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["result"], true);

    let (status, json) = send(app, "GET", "/api/store/contains/nowhere.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::OK, "contains on an absent key is OK, not an error");
    assert_eq!(json["result"], false);
}

#[tokio::test]
async fn sar09_keys_root_and_prefix() {
    let (envref, store) = env_with_store();
    store.set(&parse_key("data/a.txt").unwrap(), b"a", &Metadata::new()).await.unwrap();
    store.set(&parse_key("other/b.txt").unwrap(), b"b", &Metadata::new()).await.unwrap();
    let app = build_app(envref);
    let (status, json) = send(app.clone(), "GET", "/api/store/keys", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert!(json["result"].as_array().unwrap().len() >= 2);

    let (status, json) = send(app, "GET", "/api/store/keys?prefix=data", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    let keys: Vec<&str> = json["result"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
    assert!(keys.iter().all(|k| k.starts_with("data")), "prefix filter applied");
}

fn result_keys(json: &serde_json::Value) -> Vec<String> {
    json["result"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect()
}

/// `keys?prefix=` lists every key under the prefix, at any depth.
#[tokio::test]
async fn store_keys_lists_nested_keys_under_prefix() {
    let (envref, store) = env_with_store();
    store.set(&parse_key("data/a.txt").unwrap(), b"a", &Metadata::new()).await.unwrap();
    store.set(&parse_key("data/sub/b.txt").unwrap(), b"b", &Metadata::new()).await.unwrap();
    store.set(&parse_key("other/c.txt").unwrap(), b"c", &Metadata::new()).await.unwrap();
    let (status, json) = send(build_app(envref), "GET", "/api/store/keys?prefix=data", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    let keys = result_keys(&json);
    assert!(keys.contains(&"data/a.txt".to_string()), "got {keys:?}");
    assert!(keys.contains(&"data/sub/b.txt".to_string()), "got {keys:?}");
    assert!(!keys.iter().any(|k| k.starts_with("other")), "got {keys:?}");
}

/// `keys` without a prefix lists the whole store, at any depth.
#[tokio::test]
async fn store_keys_without_prefix_lists_whole_store() {
    let (envref, store) = env_with_store();
    store.set(&parse_key("data/sub/b.txt").unwrap(), b"b", &Metadata::new()).await.unwrap();
    store.set(&parse_key("other/c.txt").unwrap(), b"c", &Metadata::new()).await.unwrap();
    let (status, json) = send(build_app(envref), "GET", "/api/store/keys", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    let keys = result_keys(&json);
    assert!(keys.contains(&"data/sub/b.txt".to_string()), "got {keys:?}");
    assert!(keys.contains(&"other/c.txt".to_string()), "got {keys:?}");
}

/// `listdir` still lists only the keys directly in the directory.
#[tokio::test]
async fn store_listdir_still_lists_direct_children_only() {
    let (envref, store) = env_with_store();
    store.set(&parse_key("data/a.txt").unwrap(), b"a", &Metadata::new()).await.unwrap();
    store.set(&parse_key("data/sub/b.txt").unwrap(), b"b", &Metadata::new()).await.unwrap();
    let (status, json) = send(build_app(envref), "GET", "/api/store/listdir/data", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    let keys = result_keys(&json);
    assert!(!keys.iter().any(|k| k.ends_with("b.txt")), "got {keys:?}");
}

#[tokio::test]
async fn sar10_makedir_then_removedir() {
    let (envref, _) = env_with_store();
    let app = build_app(envref);
    let (status, _) = send(app.clone(), "PUT", "/api/store/makedir/newdir", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);

    let (status, json) = send(app.clone(), "GET", "/api/store/contains/newdir", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["result"], true);

    let (status, _) = send(app, "DELETE", "/api/store/removedir/newdir", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn sar11_upload_single_file_multipart() {
    let (envref, _) = env_with_store();
    let boundary = "----testboundary";
    let body = format!(
        "--{b}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"test.txt\"\r\nContent-Type: text/plain\r\n\r\nhello world\r\n--{b}--\r\n",
        b = boundary
    );
    let app = build_app(envref);
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/store/upload/uploads")
                .header("Content-Type", format!("multipart/form-data; boundary={}", boundary))
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["status"], "OK");
    assert!(!json["result"]["uploaded"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn sar12_get_remove_disabled_by_default() {
    let (envref, store) = env_with_store();
    store.set(&parse_key("data/x.txt").unwrap(), b"x", &Metadata::new()).await.unwrap();
    let (status, _) = send(build_app(envref), "GET", "/api/store/remove/data/x.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "GET remove route does not exist without with_destructive_gets()");
}

#[tokio::test]
async fn sar13_get_remove_enabled_with_flag() {
    let (envref, store) = env_with_store();
    store.set(&parse_key("data/x.txt").unwrap(), b"x", &Metadata::new()).await.unwrap();
    let (status, json) = send(build_app_with_gets(envref), "GET", "/api/store/remove/data/x.txt", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["status"], "OK");
}

#[tokio::test]
async fn sar14_get_makedir_enabled_with_flag() {
    let (envref, _) = env_with_store();
    let (status, _) = send(build_app_with_gets(envref), "GET", "/api/store/makedir/gdir", Body::empty()).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn sar15_get_removedir_disabled_by_default() {
    let (envref, _) = env_with_store();
    // DELETE removedir is registered on the same path, so a GET matches the path but not the
    // method: 405, as for the Assets API's GET alternatives (AAE69).
    let (status, _) = send(build_app(envref), "GET", "/api/store/removedir/somedir", Body::empty()).await;
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn sar16_builder_build_smoke_test() {
    let (envref, _) = env_with_store();
    let _app = build_app(envref); // must not panic
}
