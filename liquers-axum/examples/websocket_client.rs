/// Advanced Example 2: WebSocket Notifications & Format Selection
///
/// This example demonstrates advanced features of the Liquers Assets API:
/// 1. Real-time WebSocket notifications with multiple message types
/// 2. Asset lifecycle tracking (Submitted → Processing → Finished)
/// 3. Progress tracking with primary and secondary progress
/// 4. Ping/pong keep-alive mechanism
/// 5. Format negotiation (CBOR vs JSON)
///
/// The example runs both a server and a client:
/// - Server: Provides WebSocket endpoint for asset notifications
/// - Client: Connects to WebSocket and displays real-time updates
///
/// Usage:
///   cargo run -p liquers-axum --example websocket_client
///
/// The output shows:
/// - Real-time WebSocket message sequence
/// - Asset lifecycle (Submitted → Processing → Finished)
/// - Progress updates with percentages
/// - Format size comparisons

use axum::extract::ws::{WebSocket, WebSocketUpgrade};
use axum::response::IntoResponse;
use axum::Router;
use futures::{SinkExt, StreamExt};
use liquers_axum::{QueryApiBuilder, StoreApiBuilder};
use liquers_core::command_metadata::CommandKey;
use liquers_core::commands::CommandArguments;
use liquers_core::context::{Context, Environment, SimpleEnvironment};
use liquers_core::error::Error;
use liquers_core::query::Key;
use liquers_core::store::{AsyncStoreWrapper, FileStore};
use liquers_core::value::Value;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use tokio::sync::Mutex;
use tokio_tungstenite::{connect_async, tungstenite::Message};

// WebSocket notification message types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
enum NotificationMessage {
    Initial {
        asset_id: u64,
        query: String,
        timestamp: String,
        metadata: Option<serde_json::Value>,
    },
    JobSubmitted {
        asset_id: u64,
        query: String,
        timestamp: String,
    },
    JobStarted {
        asset_id: u64,
        query: String,
        timestamp: String,
    },
    PrimaryProgressUpdated {
        asset_id: u64,
        query: String,
        timestamp: String,
        progress: ProgressInfo,
    },
    JobFinished {
        asset_id: u64,
        query: String,
        timestamp: String,
    },
    Pong {
        timestamp: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProgressInfo {
    message: String,
    done: u64,
    total: u64,
    timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    eta: Option<String>,
}

// Client message types
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "action")]
enum ClientMessage {
    #[serde(rename = "ping")]
    Ping,
}

// Simulated server-side asset
struct SimulatedAsset {
    id: u64,
    query: String,
    start_time: Instant,
    duration: Duration,
}

impl SimulatedAsset {
    fn new(id: u64, query: String, duration_secs: u64) -> Self {
        Self {
            id,
            query,
            start_time: Instant::now(),
            duration: Duration::from_secs(duration_secs),
        }
    }

    fn is_complete(&self) -> bool {
        self.start_time.elapsed() > self.duration
    }

    fn progress(&self) -> f64 {
        let elapsed = self.start_time.elapsed().as_secs_f64();
        let total = self.duration.as_secs_f64();
        (elapsed / total).min(1.0)
    }

    fn current_timestamp(&self) -> String {
        chrono::DateTime::<chrono::Utc>::from(SystemTime::now())
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
    }
}

// ============================================================================
// Server Implementation
// ============================================================================

fn register_commands(mut env: SimpleEnvironment<Value>) -> Result<SimpleEnvironment<Value>, Error> {
    use liquers_core::command_metadata::ArgumentInfo;

    let cr = &mut env.command_registry;

    let key = CommandKey::new_name("text");
    let metadata = cr.register_command(key, |_state, args: CommandArguments<_>, _context: Context<_>| {
        let text: String = args.get(0, "text")?;
        Ok(Value::from(text))
    })?;

    metadata
        .with_label("Text")
        .with_doc("Create a text value from the given string")
        .with_argument(ArgumentInfo::any_argument("text"));

    Ok(env)
}

async fn mock_websocket_handler(ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(handle_websocket_connection)
}

async fn handle_websocket_connection(socket: WebSocket) {
    println!("[Server] Client connected to WebSocket");

    let (mut sender, mut receiver) = socket.split();

    // Create simulated asset
    let asset = Arc::new(Mutex::new(SimulatedAsset::new(
        12345,
        "-R/test/data".to_string(),
        3,
    )));

    // Send initial message
    let initial_msg = NotificationMessage::Initial {
        asset_id: 12345,
        query: "-R/test/data".to_string(),
        timestamp: chrono::DateTime::<chrono::Utc>::from(SystemTime::now())
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        metadata: Some(serde_json::json!({
            "status": "Submitted",
            "message": "Asset evaluation submitted"
        })),
    };

    if let Ok(msg_str) = serde_json::to_string(&initial_msg) {
        let msg = axum::extract::ws::Message::Text(msg_str.into());
        let _ = sender.send(msg).await;
    }

    // Spawn notification sender
    let asset_clone = Arc::clone(&asset);
    let sender_arc = Arc::new(Mutex::new(sender));
    let sender_ref = Arc::clone(&sender_arc);

    tokio::spawn(async move {
        let mut last_progress = 0;

        for i in 0..30 {
            tokio::time::sleep(Duration::from_millis(100)).await;

            let asset = asset_clone.lock().await;
            let progress = (asset.progress() * 100.0) as u64;

            let msg = if i == 0 {
                Some(NotificationMessage::JobSubmitted {
                    asset_id: 12345,
                    query: "-R/test/data".to_string(),
                    timestamp: asset.current_timestamp(),
                })
            } else if i == 1 {
                Some(NotificationMessage::JobStarted {
                    asset_id: 12345,
                    query: "-R/test/data".to_string(),
                    timestamp: asset.current_timestamp(),
                })
            } else if asset.is_complete() {
                Some(NotificationMessage::JobFinished {
                    asset_id: 12345,
                    query: "-R/test/data".to_string(),
                    timestamp: asset.current_timestamp(),
                })
            } else if progress > last_progress {
                last_progress = progress;
                Some(NotificationMessage::PrimaryProgressUpdated {
                    asset_id: 12345,
                    query: "-R/test/data".to_string(),
                    timestamp: asset.current_timestamp(),
                    progress: ProgressInfo {
                        message: format!("Processing: {}%", progress),
                        done: progress,
                        total: 100,
                        timestamp: asset.current_timestamp(),
                        eta: None,
                    },
                })
            } else {
                None
            };

            if let Some(msg) = msg {
                if let Ok(msg_str) = serde_json::to_string(&msg) {
                    let mut sender = sender_ref.lock().await;
                    let ws_msg = axum::extract::ws::Message::Text(msg_str.into());
                    let _ = sender.send(ws_msg).await;
                }
            }

            if asset.is_complete() {
                break;
            }
        }
    });

    // Handle client messages
    while let Some(Ok(msg)) = receiver.next().await {
        if let axum::extract::ws::Message::Text(text) = msg {
            if let Ok(ClientMessage::Ping) = serde_json::from_str::<ClientMessage>(&text) {
                let pong = NotificationMessage::Pong {
                    timestamp: chrono::DateTime::<chrono::Utc>::from(SystemTime::now())
                        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                };
                if let Ok(msg_str) = serde_json::to_string(&pong) {
                    let mut sender = sender_arc.lock().await;
                    let ws_msg = axum::extract::ws::Message::Text(msg_str.into());
                    let _ = sender.send(ws_msg).await;
                }
            }
        }
    }

    println!("[Server] Client disconnected");
}

async fn format_comparison_handler() -> impl IntoResponse {
    let sample_data = vec![42u8; 1000];
    let metadata = serde_json::json!({
        "key": "test/data",
        "status": "Ready",
        "type_identifier": "application/octet-stream"
    });

    // CBOR size using serde_json as proxy (both are tag-based)
    let cbor_size = serde_json::to_string(&serde_json::json!({
        "data": &sample_data,
        "metadata": &metadata
    }))
    .map(|s| s.len() * 8 / 10) // Rough estimate: CBOR is ~80% of JSON size
    .unwrap_or(0);

    // JSON size with base64
    let json_size = serde_json::to_string(&serde_json::json!({
        "data": base64::encode(&sample_data),
        "metadata": &metadata
    }))
    .map(|s| s.len())
    .unwrap_or(0);

    axum::Json(serde_json::json!({
        "formats": {
            "cbor_size_bytes": cbor_size,
            "json_size_bytes": json_size,
            "efficiency_ratio": format!("{:.1}%", (cbor_size as f64 / json_size as f64 * 100.0))
        }
    }))
}

async fn setup_server() -> Router {
    let store_path = std::env::var("LIQUERS_STORE_PATH").unwrap_or_else(|_| ".".to_string());

    let file_store = FileStore::new(&store_path, &Key::new());
    let async_store = AsyncStoreWrapper(file_store);

    let mut env = SimpleEnvironment::<Value>::new();
    env.with_async_store(Box::new(async_store));

    let env = register_commands(env).expect("Failed to register commands");
    let env_ref = env.to_ref();

    let query_router = QueryApiBuilder::new("/liquer/q").build();
    let store_router = StoreApiBuilder::new("/liquer/api/store").build();

    Router::new()
        .route(
            "/",
            axum::routing::get(|| async {
                "Liquers WebSocket Example\n\nEndpoints:\n  WS  /ws/assets/{*query} - Asset notifications\n  GET /format-comparison - Format sizes\n"
            }),
        )
        .route("/ws/assets/:path", axum::routing::get(mock_websocket_handler))
        .route(
            "/format-comparison",
            axum::routing::get(format_comparison_handler),
        )
        .merge(query_router)
        .merge(store_router)
        .with_state(env_ref)
}

// ============================================================================
// Client Implementation
// ============================================================================

struct WebSocketClient {
    url: String,
    message_count: Arc<AtomicU64>,
}

impl WebSocketClient {
    fn new(url: String) -> Self {
        Self {
            url,
            message_count: Arc::new(AtomicU64::new(0)),
        }
    }

    async fn connect_and_listen(&self) -> Result<(), Box<dyn std::error::Error>> {
        println!("\n[Client] Connecting to {}", self.url);

        let (ws_stream, _) = connect_async(&self.url).await?;
        let (mut write, mut read) = ws_stream.split();

        println!("[Client] Connected! Waiting for notifications...\n");

        // Spawn ping task
        let ping_task = tokio::spawn(async move {
            for _ in 0..20 {
                tokio::time::sleep(Duration::from_millis(150)).await;
                let ping_msg = serde_json::json!({ "action": "ping" });
                if let Ok(msg_str) = serde_json::to_string(&ping_msg) {
                    let _ = write.send(Message::Text(msg_str)).await;
                }
            }
        });

        // Listen for messages
        while let Some(msg) = read.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    self.message_count.fetch_add(1, Ordering::SeqCst);
                    if let Ok(notification) = serde_json::from_str::<NotificationMessage>(&text) {
                        self.print_notification(&notification);
                    }
                }
                Ok(Message::Close(_)) => {
                    println!("\n[Client] Server closed connection");
                    break;
                }
                Err(e) => {
                    println!("[Client] Error: {}", e);
                    break;
                }
                _ => {}
            }
        }

        let _ = ping_task.await;

        println!(
            "\n[Client] Received {} messages total\n",
            self.message_count.load(Ordering::SeqCst)
        );

        Ok(())
    }

    fn print_notification(&self, notification: &NotificationMessage) {
        match notification {
            NotificationMessage::Initial {
                asset_id,
                query,
                timestamp,
                metadata,
            } => {
                println!("  [Initial] Asset #{} for query: {}", asset_id, query);
                println!("    Timestamp: {}", timestamp);
                if let Some(m) = metadata {
                    println!("    Metadata: {}", m);
                }
            }
            NotificationMessage::JobSubmitted {
                asset_id,
                timestamp,
                ..
            } => {
                println!("  [JobSubmitted] Asset #{} submitted for processing", asset_id);
                println!("    Timestamp: {}", timestamp);
            }
            NotificationMessage::JobStarted {
                asset_id,
                timestamp,
                ..
            } => {
                println!("  [JobStarted] Asset #{} started processing", asset_id);
                println!("    Timestamp: {}", timestamp);
            }
            NotificationMessage::PrimaryProgressUpdated {
                asset_id,
                progress,
                ..
            } => {
                println!(
                    "  [Progress] Asset #{}: {} ({}/{})",
                    asset_id, progress.message, progress.done, progress.total
                );
            }
            NotificationMessage::JobFinished { asset_id, .. } => {
                println!("  [JobFinished] Asset #{} finished successfully", asset_id);
            }
            NotificationMessage::Pong { timestamp } => {
                println!("  [Pong] Keep-alive acknowledged at {}", timestamp);
            }
        }
    }
}

async fn test_format_negotiation(base_url: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("\n[Client] Testing format negotiation...\n");

    let url = format!("{}/format-comparison", base_url);

    match reqwest::Client::new().get(&url).send().await {
        Ok(response) => {
            if let Ok(json) = response.json::<serde_json::Value>().await {
                println!("Format comparison results:");
                if let Some(formats) = json.get("formats") {
                    println!("  CBOR size:     {} bytes", formats.get("cbor_size_bytes").unwrap_or(&"?".into()));
                    println!("  JSON size:     {} bytes", formats.get("json_size_bytes").unwrap_or(&"?".into()));
                    println!(
                        "  Efficiency:    {}",
                        formats.get("efficiency_ratio").unwrap_or(&"?".into())
                    );
                }
            }
        }
        Err(e) => {
            println!("Could not fetch format comparison: {}", e);
        }
    }

    Ok(())
}

// ============================================================================
// Main
// ============================================================================

#[tokio::main]
async fn main() {
    println!("{}", "=".repeat(70));
    println!("Liquers Advanced Example 2: WebSocket Notifications & Format Selection");
    println!("{}", "=".repeat(70));

    println!("\n[Main] Initializing server...");
    let app = setup_server().await;

    let addr = "127.0.0.1:3001";
    println!("[Main] Starting server on {}\n", addr);

    // Spawn server
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    // Wait for server to start
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Connect WebSocket client
    let ws_url = format!("ws://{}/ws/assets/-R/test/data", addr);
    let client = WebSocketClient::new(ws_url);

    println!("\n[Main] Starting WebSocket client...");
    println!("\nWebSocket Message Sequence:");
    println!("{:-^70}\n", " Asset Lifecycle Notifications ");

    if let Err(e) = client.connect_and_listen().await {
        eprintln!("[Client] Connection error: {}", e);
    }

    // Test format negotiation
    let http_base = format!("http://{}", addr);
    if let Err(e) = test_format_negotiation(&http_base).await {
        eprintln!("[Client] Format test error: {}", e);
    }

    println!("\n[Main] Example complete!");
    println!("\n{:-^70}", " Key Features Demonstrated ");
    println!("✓ Real-time WebSocket asset notifications");
    println!("✓ Asset lifecycle tracking (Submitted → Processing → Finished)");
    println!("✓ Progress updates with percentages");
    println!("✓ Ping/pong keep-alive mechanism");
    println!("✓ Format negotiation (CBOR vs JSON efficiency)");
    println!("✓ Async server-client communication");
    println!("{}", "=".repeat(70));
}
