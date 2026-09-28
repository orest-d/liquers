/// Example: Assets API WebSocket notifications and entry format negotiation
///
/// Runs a server with the real Assets API (`AssetsApiBuilder`) and a client that:
/// 1. subscribes to a **query** on `ws/q` and prints every notification until the evaluation
///    finishes (each notification carries an `info` snapshot of the asset);
/// 2. writes a note with `POST key/data`, subscribes to its **key** on `ws/key`, deletes it with
///    `DELETE key/data`, and receives `Removed`, which ends the subscription;
/// 3. sends `ping` and receives `Pong`;
/// 4. fetches the same value with `GET q/entry` as CBOR and as JSON and compares the sizes.
///
/// Usage:
///   cargo run -p liquers-axum --example websocket_client
///
/// Protocol: `specs/reference/WEB_API_SPECIFICATION.md` §5 (WebSocket).
use axum::Router;
use futures::{SinkExt, StreamExt};
use liquers_axum::{AssetsApiBuilder, QueryApiBuilder};
use liquers_core::command_metadata::CommandKey;
use liquers_core::context::{Environment, SimpleEnvironment};
use liquers_core::query::Key;
use liquers_core::state::State;
use liquers_core::store::AsyncMemoryStore;
use liquers_core::value::Value;
use serde_json::{json, Value as JsonValue};
use std::time::Duration;
use tokio_tungstenite::{connect_async, tungstenite::Message};

type Ws =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

fn setup_server() -> Router {
    let mut env = SimpleEnvironment::<Value>::new();
    env.with_async_store(Box::new(AsyncMemoryStore::new(&Key::new())));
    env.command_registry
        .register_command(CommandKey::new_name("hello"), |_, _, _| {
            Ok(Value::from("hello, websocket"))
        })
        .expect("register hello");
    env.command_registry
        .register_command(
            CommandKey::new_name("slow_upper"),
            |state: &State<Value>, _, _| {
                // A blocking command, to give the evaluation a visible duration.
                std::thread::sleep(Duration::from_millis(300));
                Ok(Value::from(state.try_into_string()?.to_uppercase()))
            },
        )
        .expect("register slow_upper");
    let envref = env.to_ref();

    AssetsApiBuilder::new("/liquer/api/assets")
        .build()
        .merge(QueryApiBuilder::new("/liquer/q/").build())
        .with_state(envref)
}

async fn send(ws: &mut Ws, message: JsonValue) {
    ws.send(Message::Text(message.to_string()))
        .await
        .expect("send");
}

/// The next JSON message, or `None` on close or after `timeout`.
async fn next_json(ws: &mut Ws, timeout: Duration) -> Option<JsonValue> {
    loop {
        match tokio::time::timeout(timeout, ws.next()).await {
            Ok(Some(Ok(Message::Text(text)))) => return serde_json::from_str(&text).ok(),
            Ok(Some(Ok(Message::Ping(_) | Message::Pong(_) | Message::Binary(_))))
            | Ok(Some(Ok(Message::Frame(_)))) => continue,
            Ok(Some(Ok(Message::Close(_)))) | Ok(Some(Err(_))) | Ok(None) | Err(_) => {
                return None
            }
        }
    }
}

fn print_notification(message: &JsonValue) {
    let kind = message["type"].as_str().unwrap_or("?");
    let address = message["query"]
        .as_str()
        .or_else(|| message["key"].as_str())
        .unwrap_or("");
    let status = message["info"]["status"].as_str().unwrap_or("-");
    println!("  {kind:<24} {address:<28} status: {status}");
}

async fn follow_a_query(ws_base: &str) {
    println!("\n1. Query subscription on ws/q");
    let (mut ws, _) = connect_async(format!("{ws_base}/ws/q")).await.expect("connect");
    send(&mut ws, json!({"action": "subscribe", "query": "hello/slow_upper"})).await;
    while let Some(message) = next_json(&mut ws, Duration::from_secs(5)).await {
        print_notification(&message);
        if message["type"] == "JobFinished" {
            break;
        }
    }
}

async fn follow_a_key(ws_base: &str, http_base: &str) {
    println!("\n2. Key subscription on ws/key, ended by a DELETE");
    let client = reqwest::Client::new();
    client
        .post(format!(
            "{http_base}/key/data/notes/a.txt?type_identifier=Text&title=Note"
        ))
        .body("remember this")
        .send()
        .await
        .expect("POST key/data");

    let (mut ws, _) = connect_async(format!("{ws_base}/ws/key/notes/a.txt"))
        .await
        .expect("connect");
    if let Some(initial) = next_json(&mut ws, Duration::from_secs(5)).await {
        print_notification(&initial);
    }
    client
        .delete(format!("{http_base}/key/data/notes/a.txt"))
        .send()
        .await
        .expect("DELETE key/data");
    while let Some(message) = next_json(&mut ws, Duration::from_secs(2)).await {
        print_notification(&message);
        if message["type"] == "Removed" {
            println!("  (the subscription has ended; subscribe again to follow a new asset)");
            break;
        }
    }

    send(&mut ws, json!({"action": "ping"})).await;
    if let Some(pong) = next_json(&mut ws, Duration::from_secs(2)).await {
        println!("\n3. Ping → {}", pong["type"]);
    }
}

async fn compare_entry_formats(http_base: &str) {
    println!("\n4. GET q/entry format negotiation");
    let client = reqwest::Client::new();
    for format in ["cbor", "json"] {
        let response = client
            .get(format!("{http_base}/q/entry/hello?format={format}"))
            .send()
            .await
            .expect("GET q/entry");
        let content_type = response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("?")
            .to_string();
        let size = response.bytes().await.map(|b| b.len()).unwrap_or(0);
        println!("  {format:<5} {content_type:<20} {size} bytes");
    }
}

#[tokio::main]
async fn main() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("address");
    tokio::spawn(async move {
        axum::serve(listener, setup_server()).await.expect("serve");
    });

    let http_base = format!("http://{addr}/liquer/api/assets");
    let ws_base = format!("ws://{addr}/liquer/api/assets");
    println!("Assets API on {http_base}, WebSocket on {ws_base}/ws/q and {ws_base}/ws/key");

    follow_a_query(&ws_base).await;
    follow_a_key(&ws_base, &http_base).await;
    compare_entry_formats(&http_base).await;
}
