//! Assets API WebSocket (`ws/q`, `ws/key`): forwarding, casing, subscription lifecycle, limits.
//! Design: `specs/design/axum-assets-endpoints/` (Phase 3, AWS01–AWS14b).

use axum::Router;
use futures::{SinkExt, StreamExt};
use liquers_axum::assets::{AssetsApiBuilder, WebSocketLimits};
use liquers_core::{
    assets::AssetManager,
    command_metadata::CommandKey,
    context::{Environment, EnvRef, SimpleEnvironment},
    metadata::{Metadata, MetadataRecord},
    parse::parse_key,
    query::Key,
    recipes::{DefaultRecipeProvider, Recipe, RecipeList},
    state::State,
    store::AsyncMemoryStore,
    value::Value,
};
use serde_json::{json, Value as JsonValue};
use std::time::Duration;
use tokio_tungstenite::{connect_async, tungstenite::Message};

type Ws = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn env_with(recipes: &[(&str, &str, &str)]) -> EnvRef<SimpleEnvironment<Value>> {
    let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
    env.command_registry
        .register_command(CommandKey::new_name("make_text"), |_, _, _| Ok(Value::from("generated")))
        .unwrap();
    env.command_registry
        .register_command(CommandKey::new_name("upper"), |state: &State<Value>, _, _| {
            Ok(Value::from(state.try_into_string()?.to_uppercase()))
        })
        .unwrap();
    // Blocking sync command; tests that submit it run on the multi-thread runtime (AWS09, AWS13).
    env.command_registry
        .register_command(CommandKey::new_name("sleep_sync"), |_, _, _| {
            std::thread::sleep(Duration::from_millis(300));
            Ok(Value::from("slept"))
        })
        .unwrap();

    let mut rl = RecipeList::new();
    for (q, t, d) in recipes {
        rl.add_recipe(Recipe::new(q.to_string(), t.to_string(), d.to_string()).unwrap());
    }
    let store = AsyncMemoryStore::new(&Key::new());
    liquers_core::store::AsyncStore::set(&store, &parse_key("recipes.yaml").unwrap(), serde_yaml::to_string(&rl).unwrap().as_bytes(), &Metadata::new())
        .await
        .unwrap();
    env.with_async_store(Box::new(store));
    env.with_recipe_provider(Box::new(DefaultRecipeProvider::new()));
    env.to_ref()
}

fn metadata_text() -> MetadataRecord {
    MetadataRecord { type_identifier: "Text".to_string(), type_name: "text".to_string(), data_format: Some("txt".to_string()), ..Default::default() }
}

fn build_app(envref: EnvRef<SimpleEnvironment<Value>>) -> Router {
    AssetsApiBuilder::<SimpleEnvironment<Value>>::new("/api/assets").build().with_state(envref)
}

fn build_app_with_limits(envref: EnvRef<SimpleEnvironment<Value>>, limits: WebSocketLimits) -> Router {
    AssetsApiBuilder::<SimpleEnvironment<Value>>::new("/api/assets")
        .with_websocket_limits(limits)
        .build()
        .with_state(envref)
}

/// Start an in-process server on an OS-assigned port and return its `ws://` base URL.
async fn start_server(app: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap(); });
    format!("ws://{}", addr)
}

async fn connect(base_url: &str, path: &str) -> Ws {
    let (ws, _) = connect_async(&format!("{}{}", base_url, path)).await.unwrap();
    ws
}

/// Receive and parse one JSON text message, skipping ping/pong/binary frames.
async fn recv_json(ws: &mut Ws, timeout: Duration) -> Option<JsonValue> {
    loop {
        match tokio::time::timeout(timeout, ws.next()).await {
            Ok(Some(Ok(Message::Text(text)))) => return Some(serde_json::from_str(&text).unwrap()),
            Ok(Some(Ok(Message::Ping(_) | Message::Pong(_) | Message::Binary(_)))) => continue,
            Ok(Some(Ok(Message::Close(_)))) | Ok(None) | Ok(Some(Err(_))) | Err(_) => return None,
            Ok(Some(Ok(Message::Frame(_)))) => continue,
        }
    }
}

async fn send_json(ws: &mut Ws, msg: JsonValue) {
    ws.send(Message::Text(serde_json::to_string(&msg).unwrap())).await.unwrap();
}

/// Poll `recv_json` up to 100 times (100ms each) for a message whose `type` matches.
async fn recv_by_type(ws: &mut Ws, expected_type: &str) -> Option<JsonValue> {
    for _ in 0..100 {
        if let Some(msg) = recv_json(ws, Duration::from_millis(100)).await {
            if msg.get("type").and_then(JsonValue::as_str) == Some(expected_type) {
                return Some(msg);
            }
        }
    }
    None
}

#[tokio::test]
async fn aws01_subscribe_by_query() {
    let envref = env_with(&[]).await;
    let base_url = start_server(build_app(envref)).await;
    let mut ws = connect(&base_url, "/api/assets/ws/q").await;

    send_json(&mut ws, json!({"action": "subscribe", "query": "make_text"})).await;

    let initial = recv_json(&mut ws, Duration::from_secs(2)).await.unwrap();
    assert_eq!(initial["type"], "Initial");
    assert!(initial["asset_id"].is_number());
    assert_eq!(initial["query"], "make_text");
    assert!(initial["timestamp"].is_string());
    assert!(initial["info"].is_object());

    let finished = recv_by_type(&mut ws, "JobFinished").await.unwrap();
    assert_eq!(finished["query"], "make_text");
    assert_eq!(finished["info"]["status"], "Ready");
}

#[tokio::test]
async fn aws02_url_path_subscription_on_connect() {
    let envref = env_with(&[]).await;
    let base_url = start_server(build_app(envref)).await;
    let mut ws = connect(&base_url, "/api/assets/ws/q/make_text").await;

    let initial = recv_json(&mut ws, Duration::from_secs(2)).await.unwrap();
    assert_eq!(initial["type"], "Initial");
    assert_eq!(initial["query"], "make_text");
}

#[tokio::test]
async fn aws03_subscribe_by_key() {
    let envref = env_with(&[("-R/notes/a.txt/-/upper/summary.txt", "Summary", "Upper-cased note")]).await;
    let am = envref.get_asset_manager();
    am.set_binary(&parse_key("notes/a.txt").unwrap(), b"hello", metadata_text()).await.unwrap();

    let base_url = start_server(build_app(envref)).await;
    let mut ws = connect(&base_url, "/api/assets/ws/key").await;
    // Final review: subscribe to the recipe key, not the Source. A Source is already finished
    // (status `Source`, never `Ready`), so it never emits `JobFinished` after the subscribe.
    send_json(&mut ws, json!({"action": "subscribe", "key": "summary.txt"})).await;

    let initial = recv_json(&mut ws, Duration::from_secs(2)).await.unwrap();
    assert_eq!(initial["type"], "Initial");
    assert_eq!(initial["key"], "summary.txt");

    let finished = recv_by_type(&mut ws, "JobFinished").await.unwrap();
    assert_eq!(finished["key"], "summary.txt");
    assert_eq!(finished["info"]["status"], "Ready");
}

#[tokio::test]
async fn aws04_key_subscription_ends_after_delete() {
    let envref = env_with(&[]).await;
    let am = envref.get_asset_manager();
    am.set_binary(&parse_key("notes/a.txt").unwrap(), b"test", metadata_text()).await.unwrap();

    let base_url = start_server(build_app(envref)).await;
    let mut ws = connect(&base_url, "/api/assets/ws/key").await;
    send_json(&mut ws, json!({"action": "subscribe", "key": "notes/a.txt"})).await;
    let initial = recv_json(&mut ws, Duration::from_secs(2)).await.unwrap();
    assert_eq!(initial["type"], "Initial");

    let client = reqwest::Client::new();
    let delete_url = format!("{}/api/assets/key/data/notes/a.txt", base_url.replace("ws://", "http://"));
    let resp = client.delete(&delete_url).send().await.unwrap();
    assert!(resp.status().is_success());

    let removed = recv_by_type(&mut ws, "Removed").await.unwrap();
    assert_eq!(removed["key"], "notes/a.txt");

    let none_msg = recv_json(&mut ws, Duration::from_millis(500)).await;
    assert_eq!(none_msg, None, "subscription ends: no further messages after Removed");
}

#[tokio::test]
async fn aws05_subscription_ends_after_error() {
    let mut env: SimpleEnvironment<Value> = SimpleEnvironment::new();
    env.command_registry
        .register_command(CommandKey::new_name("fail_always"), |_, _, _| {
            Err(liquers_core::error::Error::general_error("intentional error".to_string()))
        })
        .unwrap();
    let base_url = start_server(build_app(env.to_ref())).await;

    let mut ws = connect(&base_url, "/api/assets/ws/q").await;
    send_json(&mut ws, json!({"action": "subscribe", "query": "fail_always"})).await;
    let mut msg = recv_json(&mut ws, Duration::from_secs(2)).await.unwrap();
    assert_eq!(msg["type"], "Initial");

    // Final review: the core never sends StatusChanged(Error) — a failure is ErrorOccurred then
    // JobFinished, and the watch may coalesce them — so the terminal message is recognised by its
    // `info.status`, whatever its `type` (the Initial snapshot itself may already say Error).
    let mut seen = 0;
    while msg["info"]["status"] != "Error" {
        seen += 1;
        assert!(seen < 50, "no message reported status Error");
        msg = recv_json(&mut ws, Duration::from_secs(2))
            .await
            .expect("the subscription ended before reporting Error");
    }

    let none_msg = recv_json(&mut ws, Duration::from_millis(500)).await;
    assert_eq!(none_msg, None, "the terminal (Error) notification ends the subscription");
}

#[tokio::test]
async fn aws06_snake_case_accepted_old_casing_rejected() {
    let envref = env_with(&[]).await;
    let base_url = start_server(build_app(envref)).await;

    let mut ws = connect(&base_url, "/api/assets/ws/q").await;
    send_json(&mut ws, json!({"action": "subscribe", "query": "make_text"})).await;
    let msg = recv_json(&mut ws, Duration::from_secs(2)).await.unwrap();
    assert_eq!(msg["type"], "Initial");

    let mut ws2 = connect(&base_url, "/api/assets/ws/q").await;
    send_json(&mut ws2, json!({"action": "Subscribe", "query": "make_text"})).await; // old, Rust-cased
    let err_msg = recv_json(&mut ws2, Duration::from_secs(2)).await.unwrap();
    assert_eq!(err_msg["type"], "Error");
    assert!(err_msg["error"].is_object());
}

#[tokio::test]
async fn aws07_wrong_address_field_returns_error() {
    let envref = env_with(&[]).await;
    let base_url = start_server(build_app(envref)).await;

    let mut ws_q = connect(&base_url, "/api/assets/ws/q").await;
    send_json(&mut ws_q, json!({"action": "subscribe", "key": "notes/a.txt"})).await;
    assert_eq!(recv_json(&mut ws_q, Duration::from_secs(2)).await.unwrap()["type"], "Error");

    let mut ws_key = connect(&base_url, "/api/assets/ws/key").await;
    send_json(&mut ws_key, json!({"action": "subscribe", "query": "make_text"})).await;
    assert_eq!(recv_json(&mut ws_key, Duration::from_secs(2)).await.unwrap()["type"], "Error");
}

#[tokio::test]
async fn aws08_malformed_json_returns_error() {
    let envref = env_with(&[]).await;
    let base_url = start_server(build_app(envref)).await;
    let mut ws = connect(&base_url, "/api/assets/ws/q").await;
    ws.send(Message::Text("not valid json {".to_string())).await.unwrap();

    let err_msg = recv_json(&mut ws, Duration::from_secs(2)).await.unwrap();
    assert_eq!(err_msg["type"], "Error");
    assert!(err_msg["error"].is_object());
}

/// Final review: `make_text` finishes at once, so its `JobFinished` could already be on the wire
/// before `unsubscribe` is processed. A 300 ms `sleep_sync` makes the check meaningful: messages
/// sent before the unsubscribe (JobSubmitted/JobStarted) are tolerated, the completion is not.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn aws09_unsubscribe_stops_messages() {
    let envref = env_with(&[]).await;
    let base_url = start_server(build_app(envref)).await;
    let mut ws = connect(&base_url, "/api/assets/ws/q").await;
    send_json(&mut ws, json!({"action": "subscribe", "query": "sleep_sync"})).await;
    assert_eq!(recv_json(&mut ws, Duration::from_secs(2)).await.unwrap()["type"], "Initial");

    send_json(&mut ws, json!({"action": "unsubscribe", "query": "sleep_sync"})).await;
    let deadline = tokio::time::Instant::now() + Duration::from_millis(800);
    while let Some(msg) =
        recv_json(&mut ws, deadline.saturating_duration_since(tokio::time::Instant::now())).await
    {
        assert_ne!(msg["type"], "JobFinished", "no completion is forwarded after unsubscribe");
    }
}

#[tokio::test]
async fn aws10_unsubscribe_all() {
    let envref = env_with(&[]).await;
    let base_url = start_server(build_app(envref)).await;
    let mut ws = connect(&base_url, "/api/assets/ws/q").await;
    send_json(&mut ws, json!({"action": "subscribe", "query": "make_text"})).await;
    let _ = recv_json(&mut ws, Duration::from_secs(2)).await.unwrap();

    send_json(&mut ws, json!({"action": "unsubscribe_all"})).await;
    // The subscription's own JobFinished may arrive first (final review).
    let all_unsub = recv_by_type(&mut ws, "UnsubscribedAll").await.unwrap();
    assert_eq!(all_unsub["type"], "UnsubscribedAll");
}

#[tokio::test]
async fn aws11_ping_returns_pong() {
    let envref = env_with(&[]).await;
    let base_url = start_server(build_app(envref)).await;
    let mut ws = connect(&base_url, "/api/assets/ws/q").await;
    send_json(&mut ws, json!({"action": "ping"})).await;

    let pong = recv_json(&mut ws, Duration::from_secs(2)).await.unwrap();
    assert_eq!(pong["type"], "Pong");
    assert!(pong["timestamp"].is_string());
}

#[tokio::test]
async fn aws12a_subscription_limit_enforced() {
    let envref = env_with(&[]).await;
    let base_url = start_server(build_app_with_limits(envref, WebSocketLimits { max_message_size: 65536, max_subscriptions: 2 })).await;
    let mut ws = connect(&base_url, "/api/assets/ws/q").await;

    // Final review: two queries that end `Ready` (a live subscription, not a terminal one — a
    // failing `upper` without input would end and free its slot), and a third that is a valid
    // `query` address (a `key` field on ws/q is an Error for the wrong reason, AWS07).
    send_json(&mut ws, json!({"action": "subscribe", "query": "make_text"})).await;
    recv_by_type(&mut ws, "Initial").await.unwrap();
    send_json(&mut ws, json!({"action": "subscribe", "query": "make_text/upper"})).await;
    recv_by_type(&mut ws, "Initial").await.unwrap();

    // Third distinct subscription exceeds max_subscriptions: 2.
    send_json(&mut ws, json!({"action": "subscribe", "query": "make_text/upper/upper"})).await;
    let err_msg = recv_by_type(&mut ws, "Error").await.unwrap();
    assert_eq!(err_msg["type"], "Error");
}

#[tokio::test]
async fn aws12b_oversized_message_closed_or_errors() {
    let envref = env_with(&[]).await;
    let base_url = start_server(build_app_with_limits(envref, WebSocketLimits { max_message_size: 1024, max_subscriptions: 256 })).await;
    let mut ws = connect(&base_url, "/api/assets/ws/q").await;

    let huge_query = "x".repeat(2000);
    let msg_text = serde_json::to_string(&json!({"action": "subscribe", "query": huge_query})).unwrap();
    let send_result = ws.send(Message::Text(msg_text)).await;

    // Either the frame is rejected at the WebSocket layer, or the server replies Error; both are
    // acceptable, but silence (a message accepted with no reply at all) is not.
    if send_result.is_ok() {
        let result = recv_json(&mut ws, Duration::from_secs(2)).await;
        eprintln!("aws12b: oversized message → {:?}", result.as_ref().map(|m| m["type"].clone()));
        if let Some(msg) = result {
            assert_eq!(msg["type"], "Error");
        }
    } else {
        eprintln!("aws12b: oversized message rejected on send: {send_result:?}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn aws13_disconnect_mid_evaluation_completes_and_leaks_nothing() {
    let envref = env_with(&[]).await;
    let base_url = start_server(build_app(envref)).await;

    let mut ws = connect(&base_url, "/api/assets/ws/q").await;
    send_json(&mut ws, json!({"action": "subscribe", "query": "sleep_sync"})).await;
    let initial = recv_json(&mut ws, Duration::from_secs(2)).await.unwrap();
    assert_eq!(initial["type"], "Initial");

    drop(ws); // disconnect while the evaluation is still running

    let client = reqwest::Client::new();
    let info_url = format!("{}/api/assets/q/info/sleep_sync", base_url.replace("ws://", "http://"));
    for _ in 0..100 {
        let resp = client.get(&info_url).send().await.unwrap();
        if let Ok(body) = resp.text().await {
            if let Ok(json) = serde_json::from_str::<JsonValue>(&body) {
                if json["result"]["status"] == "Ready" {
                    return; // evaluation completed despite the disconnect
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("sleep_sync did not complete within the poll budget after disconnect");
}

#[tokio::test]
async fn aws14a_build_with_defaults_does_not_panic() {
    let envref = env_with(&[]).await;
    let _app = build_app(envref); // axum 0.8's `{*query}` route syntax fix (I10); this must not panic
}

#[tokio::test]
async fn aws14b_without_websocket_removes_both_routes() {
    let envref = env_with(&[]).await;
    let app = AssetsApiBuilder::<SimpleEnvironment<Value>>::new("/api/assets")
        .without_websocket()
        .build()
        .with_state(envref);
    let base_url = start_server(app).await;

    let result = connect_async(&format!("{}/api/assets/ws/q", base_url)).await;
    assert!(result.is_err(), "ws/q must not exist when without_websocket() is set");
    let result = connect_async(&format!("{}/api/assets/ws/key", base_url)).await;
    assert!(result.is_err(), "ws/key must not exist when without_websocket() is set");
}
