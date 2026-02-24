//! WebSocket Handler - Real-time asset notifications via WebSocket
//!
//! Part of the Assets API implementation.
//! See specs/axum-assets-recipes-api/phase2-architecture.md for specifications.

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    response::Response,
};
use futures::{sink::SinkExt, stream::StreamExt};
use liquers_core::{
    assets::{AssetManager, AssetNotificationMessage},
    context::{EnvRef, Environment},
    parse::parse_query,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::api_core::error::error_to_detail;

/// Client messages sent from WebSocket clients
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action")]
pub enum ClientMessage {
    Subscribe { query: String },
    Unsubscribe { query: String },
    UnsubscribeAll,
    Ping,
}

/// Server notification messages sent to WebSocket clients
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum NotificationMessage {
    Initial {
        asset_id: u64,
        query: String,
        timestamp: String,
        metadata: Option<serde_json::Value>,
    },
    StatusChanged {
        asset_id: u64,
        query: String,
        status: String,
        timestamp: String,
    },
    ProgressUpdated {
        asset_id: u64,
        query: String,
        primary_progress: Option<f64>,
        secondary_progress: Option<f64>,
        timestamp: String,
    },
    ValueProduced {
        asset_id: u64,
        query: String,
        timestamp: String,
    },
    ErrorOccurred {
        asset_id: u64,
        query: String,
        error: String,
        timestamp: String,
    },
    RecipeDetected {
        asset_id: u64,
        query: String,
        recipe_query: String,
        timestamp: String,
    },
    Submitted {
        asset_id: u64,
        query: String,
        timestamp: String,
    },
    Processing {
        asset_id: u64,
        query: String,
        timestamp: String,
    },
    Ready {
        asset_id: u64,
        query: String,
        timestamp: String,
    },
    Cancelled {
        asset_id: u64,
        query: String,
        timestamp: String,
    },
    Pong {
        timestamp: String,
    },
    UnsubscribedAll {
        timestamp: String,
    },
}

impl NotificationMessage {
    /// Get current timestamp in ISO 8601 format
    fn now() -> String {
        chrono::Utc::now().to_rfc3339()
    }
}

/// WebSocket endpoint for real-time asset notifications
pub async fn websocket_handler<E: Environment>(
    ws: WebSocketUpgrade,
    Path(_query_path): Path<String>,
    State(env): State<EnvRef<E>>,
) -> Response {
    // Upgrade WebSocket connection and spawn handler task
    ws.on_upgrade(move |socket| handle_socket(socket, env))
}

/// Handle WebSocket connection
async fn handle_socket<E: Environment>(socket: WebSocket, env: EnvRef<E>) {
    let (mut sender, mut receiver) = socket.split();

    // Subscription tracking - maps query string to AssetRef
    let subscriptions: Arc<RwLock<HashMap<String, Arc<dyn std::any::Any + Send + Sync>>>> =
        Arc::new(RwLock::new(HashMap::new()));

    // Process incoming messages
    while let Some(msg) = receiver.next().await {
        let msg = match msg {
            Ok(m) => m,
            Err(e) => {
                eprintln!("WebSocket error: {}", e);
                break;
            }
        };

        match msg {
            Message::Text(text) => {
                // Parse client message
                let client_msg: ClientMessage = match serde_json::from_str(&text) {
                    Ok(m) => m,
                    Err(e) => {
                        eprintln!("Failed to parse client message: {}", e);
                        continue;
                    }
                };

                // Handle client message
                match client_msg {
                    ClientMessage::Subscribe { query } => {
                        handle_subscribe(&query, &env, &subscriptions, &mut sender).await;
                    }
                    ClientMessage::Unsubscribe { query } => {
                        handle_unsubscribe(&query, &subscriptions).await;
                    }
                    ClientMessage::UnsubscribeAll => {
                        handle_unsubscribe_all(&subscriptions, &mut sender).await;
                    }
                    ClientMessage::Ping => {
                        handle_ping(&mut sender).await;
                    }
                }
            }
            Message::Close(_) => {
                break;
            }
            _ => {
                // Ignore binary, ping, pong
            }
        }
    }
}

/// Handle Subscribe message
async fn handle_subscribe<E: Environment>(
    query_str: &str,
    env: &EnvRef<E>,
    subscriptions: &Arc<RwLock<HashMap<String, Arc<dyn std::any::Any + Send + Sync>>>>,
    sender: &mut futures::stream::SplitSink<WebSocket, Message>,
) {
    // Parse query
    let query = match parse_query(query_str) {
        Ok(q) => q,
        Err(e) => {
            let error_msg = NotificationMessage::ErrorOccurred {
                asset_id: 0,
                query: query_str.to_string(),
                error: format!("Failed to parse query: {}", e),
                timestamp: NotificationMessage::now(),
            };
            let _ = send_notification(sender, &error_msg).await;
            return;
        }
    };

    // Get AssetManager
    let asset_manager = env.get_asset_manager();

    // Get or create asset
    let asset_ref = match (**asset_manager).get_asset(&query).await {
        Ok(ar) => ar,
        Err(e) => {
            let error_detail = error_to_detail(&e);
            let error_msg = NotificationMessage::ErrorOccurred {
                asset_id: 0,
                query: query_str.to_string(),
                error: error_detail.message,
                timestamp: NotificationMessage::now(),
            };
            let _ = send_notification(sender, &error_msg).await;
            return;
        }
    };

    let asset_id = asset_ref.id();

    // Store subscription (type-erased)
    let type_erased: Arc<dyn std::any::Any + Send + Sync> = Arc::new(asset_ref.clone());
    subscriptions
        .write()
        .await
        .insert(query_str.to_string(), type_erased);

    // Send initial message
    let initial_msg = NotificationMessage::Initial {
        asset_id,
        query: query_str.to_string(),
        timestamp: NotificationMessage::now(),
        metadata: None,
    };
    let _ = send_notification(sender, &initial_msg).await;

    // Note: WebSocket notification forwarding would require splitting the socket
    // and passing the sender to a background task. This is a simplified implementation
    // that subscribes but doesn't forward notifications in real-time.
    // A full implementation would need to restructure the handle_socket function.
}

/// Convert AssetNotificationMessage to NotificationMessage
fn convert_notification(
    asset_id: u64,
    query: &str,
    notification: AssetNotificationMessage,
) -> NotificationMessage {
    let timestamp = NotificationMessage::now();

    match notification {
        AssetNotificationMessage::Initial => NotificationMessage::Initial {
            asset_id,
            query: query.to_string(),
            timestamp,
            metadata: None,
        },
        AssetNotificationMessage::JobSubmitted => NotificationMessage::Submitted {
            asset_id,
            query: query.to_string(),
            timestamp,
        },
        AssetNotificationMessage::JobStarted => NotificationMessage::Processing {
            asset_id,
            query: query.to_string(),
            timestamp,
        },
        AssetNotificationMessage::StatusChanged(status) => NotificationMessage::StatusChanged {
            asset_id,
            query: query.to_string(),
            status: format!("{:?}", status),
            timestamp,
        },
        AssetNotificationMessage::ValueProduced => NotificationMessage::ValueProduced {
            asset_id,
            query: query.to_string(),
            timestamp,
        },
        AssetNotificationMessage::ErrorOccurred(error) => NotificationMessage::ErrorOccurred {
            asset_id,
            query: query.to_string(),
            error: error.to_string(),
            timestamp,
        },
        AssetNotificationMessage::LogMessage => {
            // No corresponding NotificationMessage variant for LogMessage
            // Return a generic StatusChanged instead
            NotificationMessage::StatusChanged {
                asset_id,
                query: query.to_string(),
                status: "Logging".to_string(),
                timestamp,
            }
        }
        AssetNotificationMessage::PrimaryProgressUpdated(progress) => {
            NotificationMessage::ProgressUpdated {
                asset_id,
                query: query.to_string(),
                primary_progress: if progress.total > 0 {
                    Some(progress.done as f64 / progress.total as f64)
                } else {
                    None
                },
                secondary_progress: None,
                timestamp,
            }
        }
        AssetNotificationMessage::SecondaryProgressUpdated(progress) => {
            NotificationMessage::ProgressUpdated {
                asset_id,
                query: query.to_string(),
                primary_progress: None,
                secondary_progress: if progress.total > 0 {
                    Some(progress.done as f64 / progress.total as f64)
                } else {
                    None
                },
                timestamp,
            }
        }
        AssetNotificationMessage::JobFinished => NotificationMessage::Ready {
            asset_id,
            query: query.to_string(),
            timestamp,
        },
        AssetNotificationMessage::Expired => NotificationMessage::StatusChanged {
            asset_id,
            query: query.to_string(),
            status: "Expired".to_string(),
            timestamp,
        },
    }
}

/// Handle Unsubscribe message
async fn handle_unsubscribe(
    query: &str,
    subscriptions: &Arc<RwLock<HashMap<String, Arc<dyn std::any::Any + Send + Sync>>>>,
) {
    subscriptions.write().await.remove(query);
}

/// Handle UnsubscribeAll message
async fn handle_unsubscribe_all(
    subscriptions: &Arc<RwLock<HashMap<String, Arc<dyn std::any::Any + Send + Sync>>>>,
    sender: &mut futures::stream::SplitSink<WebSocket, Message>,
) {
    subscriptions.write().await.clear();

    let msg = NotificationMessage::UnsubscribedAll {
        timestamp: NotificationMessage::now(),
    };
    let _ = send_notification(sender, &msg).await;
}

/// Handle Ping message
async fn handle_ping(sender: &mut futures::stream::SplitSink<WebSocket, Message>) {
    let msg = NotificationMessage::Pong {
        timestamp: NotificationMessage::now(),
    };
    let _ = send_notification(sender, &msg).await;
}

/// Send notification message to WebSocket client
async fn send_notification(
    sender: &mut futures::stream::SplitSink<WebSocket, Message>,
    msg: &NotificationMessage,
) -> Result<(), Box<dyn std::error::Error>> {
    let json = serde_json::to_string(msg)?;
    sender.send(Message::Text(json.into())).await?;
    Ok(())
}
