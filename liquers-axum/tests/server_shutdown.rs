//! Graceful shutdown (`specs/design/axum-server-assembly/`, Phase 3 test 8, AC-9): drain, abort
//! at the deadline, the port freed, WebSockets closed with 1001.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::StreamExt;
use liquers_axum::{LiquersServer, LiquersServerBuilder, ServerConfig};
use liquers_core::command_metadata::CommandKey;
use liquers_core::context::{EnvRef, Environment, SimpleEnvironment};
use liquers_core::query::Key;
use liquers_core::store::AsyncMemoryStore;
use liquers_core::value::Value;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::Message;

type Env = SimpleEnvironment<Value>;

/// A `slow` command that waits for its gate (at most 3 s), and counts its starts.
struct Gate {
    open: AtomicBool,
    started: AtomicUsize,
}

fn env_with_gate(gate: Arc<Gate>) -> EnvRef<Env> {
    let mut env = Env::new();
    env.command_registry
        .register_command(CommandKey::new_name("slow"), move |_, _, _| {
            gate.started.fetch_add(1, Ordering::SeqCst);
            let start = Instant::now();
            while !gate.open.load(Ordering::SeqCst) && start.elapsed() < Duration::from_secs(3) {
                std::thread::sleep(Duration::from_millis(10));
            }
            Ok(Value::from("done"))
        })
        .unwrap();
    env.command_registry
        .register_command(CommandKey::new_name("hello"), |_, _, _| Ok(Value::from("Hello")))
        .unwrap();
    env.with_async_store(Box::new(AsyncMemoryStore::new(&Key::new())));
    env.to_ref()
}

fn gate() -> Arc<Gate> {
    Arc::new(Gate { open: AtomicBool::new(false), started: AtomicUsize::new(0) })
}

async fn serve(envref: EnvRef<Env>) -> LiquersServer {
    LiquersServerBuilder::<Env>::from_config(ServerConfig { port: 0, ..ServerConfig::default() })
        .with_stop_deadline(Duration::from_millis(500))
        .bind(envref)
        .await
        .unwrap()
}

async fn wait_started(gate: &Gate) {
    let start = Instant::now();
    while gate.started.load(Ordering::SeqCst) == 0 {
        assert!(start.elapsed() < Duration::from_secs(5), "the slow command never started");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn slow_request_completes_within_deadline() {
    let gate = gate();
    let server = Arc::new(serve(env_with_gate(gate.clone())).await);
    let url = format!("http://{}/liquer/q/slow", server.local_addr());
    let request = tokio::spawn(async move { reqwest::get(url).await?.text().await });
    wait_started(&gate).await;
    let stopping = {
        let server = server.clone();
        tokio::spawn(async move { server.shutdown(Duration::from_secs(5)).await })
    };
    tokio::time::sleep(Duration::from_millis(200)).await;
    gate.open.store(true, Ordering::SeqCst);
    assert_eq!(request.await.unwrap().unwrap(), "done", "the in-flight request finishes");
    tokio::time::timeout(Duration::from_secs(5), stopping).await.unwrap().unwrap();
    assert!(!server.is_running());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn slow_request_is_aborted_at_deadline() {
    let gate = gate();
    let server = serve(env_with_gate(gate.clone())).await;
    let url = format!("http://{}/liquer/q/slow", server.local_addr());
    let request = tokio::spawn(async move { reqwest::get(url).await?.text().await });
    wait_started(&gate).await;
    let start = Instant::now();
    server.shutdown(Duration::from_millis(300)).await;
    assert!(start.elapsed() < Duration::from_secs(2), "shutdown waited {:?}", start.elapsed());
    assert!(request.await.unwrap().is_err(), "the request was cut off");
    gate.open.store(true, Ordering::SeqCst);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn new_connections_refused_and_port_free() {
    let server = serve(env_with_gate(gate())).await;
    let addr = server.local_addr();
    assert_eq!(reqwest::get(format!("http://{addr}/liquer/q/hello")).await.unwrap().text().await.unwrap(), "Hello");
    server.shutdown(Duration::from_secs(1)).await;
    assert!(reqwest::get(format!("http://{addr}/liquer/q/hello")).await.is_err());
    std::net::TcpListener::bind(addr).expect("the port is free once shutdown returns");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shutdown_is_idempotent() {
    let server = serve(env_with_gate(gate())).await;
    server.shutdown(Duration::from_secs(1)).await;
    let start = Instant::now();
    server.shutdown(Duration::from_secs(1)).await;
    assert!(start.elapsed() < Duration::from_millis(100));
    server.stopped().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dropped_handle_keeps_serving() {
    let server = serve(env_with_gate(gate())).await;
    let addr = server.local_addr();
    let stop = server.stop_token();
    drop(server);
    assert_eq!(reqwest::get(format!("http://{addr}/liquer/q/hello")).await.unwrap().text().await.unwrap(), "Hello");
    stop.cancel();
    let start = Instant::now();
    loop {
        if std::net::TcpListener::bind(addr).is_ok() {
            break;
        }
        assert!(start.elapsed() < Duration::from_secs(5), "the stop token did not stop the server");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn websocket_gets_close_frame() {
    let server = Arc::new(serve(env_with_gate(gate())).await);
    let url = format!("ws://{}/liquer/api/assets/ws/q", server.local_addr());
    let (mut socket, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    let stopping = {
        let server = server.clone();
        tokio::spawn(async move { server.shutdown(Duration::from_secs(5)).await })
    };
    let frame = tokio::time::timeout(Duration::from_secs(3), socket.next())
        .await
        .expect("a message before the timeout")
        .expect("the stream is open")
        .expect("a frame");
    match frame {
        Message::Close(Some(close)) => {
            assert_eq!(close.code, CloseCode::Away);
            assert_eq!(&*close.reason, "server shutting down");
        }
        other => panic!("expected a Close frame, got {other:?}"),
    }
    let start = Instant::now();
    tokio::time::timeout(Duration::from_secs(5), stopping).await.unwrap().unwrap();
    assert!(start.elapsed() < Duration::from_secs(3));
}
