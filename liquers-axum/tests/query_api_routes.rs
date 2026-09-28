//! Query API routes (`QueryApiBuilder`), including `with_timeout`. Design:
//! `specs/design/axum-assets-endpoints/` (Phase 3, QAR01–QAR07).

use axum::{body::{to_bytes, Body}, http::{Request, StatusCode}};
use liquers_axum::query::QueryApiBuilder;
use liquers_core::{
    command_metadata::CommandKey,
    context::{Environment, EnvRef, SimpleEnvironment},
    value::Value,
};
use tower::ServiceExt;

fn env_with_commands() -> EnvRef<SimpleEnvironment<Value>> {
    let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
    env.command_registry
        .register_command(CommandKey::new_name("make_text"), |_, _, _| Ok(Value::from("generated")))
        .unwrap();
    env.to_ref()
}

async fn send(app: axum::Router, method: &str, uri: &str) -> (StatusCode, serde_json::Value) {
    let resp = app.oneshot(Request::builder().method(method).uri(uri).body(Body::empty()).unwrap()).await.unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null))
}

#[tokio::test]
async fn qar01_get_query_success() {
    let app = QueryApiBuilder::<SimpleEnvironment<Value>>::new("/q").build().with_state(env_with_commands());
    let resp = app
        .oneshot(Request::builder().method("GET").uri("/q/make_text").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    assert_eq!(body.as_ref(), b"generated");
}

#[tokio::test]
async fn qar02_post_query_success() {
    let app = QueryApiBuilder::<SimpleEnvironment<Value>>::new("/q").build().with_state(env_with_commands());
    let resp = app
        .oneshot(Request::builder().method("POST").uri("/q/make_text").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn qar03_get_parse_error_400() {
    let app = QueryApiBuilder::<SimpleEnvironment<Value>>::new("/q").build().with_state(env_with_commands());
    let (status, json) = send(app, "GET", "/q/-R/").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["error"]["type"], "ParseError");
}

#[tokio::test]
async fn qar04_get_unregistered_command_error() {
    let app = QueryApiBuilder::<SimpleEnvironment<Value>>::new("/q").build().with_state(env_with_commands());
    let (status, json) = send(app, "GET", "/q/no_such_command_registered").await;
    assert_ne!(status, StatusCode::OK);
    assert!(json["error"]["type"].is_string(), "an unregistered action must produce an error envelope");
}

/// A blocking command, so the queued manager's worker needs its own thread.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn qar05_with_timeout_message_points_to_assets_api() {
    let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
    env.command_registry
        .register_command(CommandKey::new_name("slow"), |_, _, _| {
            std::thread::sleep(std::time::Duration::from_millis(500));
            Ok(Value::from("done"))
        })
        .unwrap();
    let app = QueryApiBuilder::<SimpleEnvironment<Value>>::new("/q")
        .with_timeout(std::time::Duration::from_millis(100))
        .build()
        .with_state(env.to_ref());
    let (status, json) = send(app, "GET", "/q/slow").await;
    assert_ne!(status, StatusCode::OK, "the 100ms timeout must fire before the 500ms sleep finishes");
    let msg = json["error"]["message"].as_str().unwrap_or("").to_lowercase();
    assert!(msg.contains("q/submit") || msg.contains("q/info"), "timeout message points to the long-running path");
}

#[tokio::test]
async fn qar06_default_timeout_unchanged_for_fast_query() {
    // Regression: QueryApiBuilder::new(...).build() with no with_timeout() call must keep behaving
    // as it does today (30s default) for a query that finishes immediately.
    let app = QueryApiBuilder::<SimpleEnvironment<Value>>::new("/q").build().with_state(env_with_commands());
    let (status, _) = send(app, "GET", "/q/make_text").await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn qar07_builder_custom_path() {
    let app = QueryApiBuilder::<SimpleEnvironment<Value>>::new("/custom/q").build().with_state(env_with_commands());
    let (status, _) = send(app, "GET", "/custom/q/make_text").await;
    assert_eq!(status, StatusCode::OK);
}
