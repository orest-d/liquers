//! Assets API WebSocket: real-time asset notifications.
//!
//! Two endpoints, mirroring the REST families (`specs/design/axum-assets-endpoints/
//! phase2-architecture.md`, "WebSocket Notifications"):
//!
//! - `{ws}/q` and `{ws}/q/{*query}`: subscriptions are **queries** (`parse_query`), requested
//!   with `AssetManager::get_asset`;
//! - `{ws}/key` and `{ws}/key/{*key}`: subscriptions are **keys** (`parse_key`), requested with
//!   `AssetManager::get`.
//!
//! A path in the URL is subscribed on connect. Client messages are snake_case JSON
//! (`{"action":"subscribe","query":"…"}`); a message that cannot be handled gets an `Error`
//! reply. Every notification carries an [`AssetInfo`] re-read after the change, because the core
//! channel is a `tokio::sync::watch` that keeps only the latest message.
//!
//! A subscription follows one asset and **ends** with that asset's lifecycle: after the
//! notification whose snapshot status is `Error`, `Cancelled`, `Expired` or `Volatile`, or after
//! `Removed`. Subscribing again requests a fresh asset.

use std::collections::HashMap;

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    response::Response,
    Extension,
};
use futures::{
    sink::SinkExt,
    stream::{SplitSink, StreamExt},
};
use liquers_core::{
    assets::{AssetManager, AssetNotificationMessage, AssetRef},
    context::{EnvRef, Environment},
    error::{Error, ErrorType},
    metadata::{AssetInfo, ProgressEntry, Status},
    parse::{parse_key, parse_query},
    query::Key,
};
use serde::{Deserialize, Serialize};
use tokio::{sync::mpsc, task::JoinHandle};

use crate::api_core::{error::error_to_detail, ErrorDetail};

/// Limits of the WebSocket endpoints (`AssetsApiBuilder::with_websocket_limits`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WebSocketLimits {
    /// Largest client message accepted, in bytes; a larger one closes the connection.
    pub max_message_size: usize,
    /// Most subscriptions one socket may hold at once; a `subscribe` beyond it gets an `Error`.
    pub max_subscriptions: usize,
}

impl Default for WebSocketLimits {
    fn default() -> Self {
        WebSocketLimits {
            max_message_size: 64 * 1024,
            max_subscriptions: 256,
        }
    }
}

/// Capacity of a socket's outgoing queue. An intermediate notification that finds it full is
/// dropped (the next one carries a fresh snapshot anyway); replies and terminal notifications wait.
const OUTGOING_QUEUE: usize = 64;

/// Client messages. The address field names what the endpoint parses: `query` on `ws/q`, `key`
/// on `ws/key`; the other one is an error.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ClientMessage {
    /// Request the asset and follow its notifications.
    Subscribe {
        #[serde(default)]
        query: Option<String>,
        #[serde(default)]
        key: Option<String>,
    },
    /// Stop following an asset.
    Unsubscribe {
        #[serde(default)]
        query: Option<String>,
        #[serde(default)]
        key: Option<String>,
    },
    /// Stop following every asset.
    UnsubscribeAll,
    /// Answered with `Pong`.
    Ping,
}

/// What every asset notification carries.
#[derive(Debug, Clone, Serialize)]
pub struct Head {
    pub asset_id: u64,
    /// The query as subscribed (query subscriptions).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    /// The key as subscribed (key subscriptions), or the asset's key if it is keyed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    pub timestamp: String,
    /// The asset's state re-read after the change.
    pub info: Option<AssetInfo>,
}

/// Server messages: one per `AssetNotificationMessage`, plus the protocol replies.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum NotificationMessage {
    Initial {
        #[serde(flatten)]
        head: Head,
    },
    JobSubmitted {
        #[serde(flatten)]
        head: Head,
    },
    JobStarted {
        #[serde(flatten)]
        head: Head,
    },
    StatusChanged {
        #[serde(flatten)]
        head: Head,
        status: Status,
    },
    ValueProduced {
        #[serde(flatten)]
        head: Head,
    },
    ErrorOccurred {
        #[serde(flatten)]
        head: Head,
        error: ErrorDetail,
    },
    /// A log entry was added; its text is in `info.message`.
    LogMessage {
        #[serde(flatten)]
        head: Head,
    },
    PrimaryProgressUpdated {
        #[serde(flatten)]
        head: Head,
        progress: ProgressEntry,
    },
    SecondaryProgressUpdated {
        #[serde(flatten)]
        head: Head,
        progress: ProgressEntry,
    },
    JobFinished {
        #[serde(flatten)]
        head: Head,
    },
    Expired {
        #[serde(flatten)]
        head: Head,
    },
    /// The asset was removed or replaced; the subscription ends.
    Removed {
        #[serde(flatten)]
        head: Head,
    },
    Pong {
        timestamp: String,
    },
    UnsubscribedAll {
        timestamp: String,
    },
    Error {
        timestamp: String,
        error: ErrorDetail,
    },
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn error_message(error: &Error) -> NotificationMessage {
    NotificationMessage::Error {
        timestamp: now(),
        error: error_to_detail(error),
    }
}

fn parameter_error(message: String) -> NotificationMessage {
    error_message(&Error::from_error(ErrorType::ParameterError, message))
}

/// Which object an endpoint's subscriptions address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Family {
    Query,
    Key,
}

impl Family {
    fn field(self) -> &'static str {
        match self {
            Family::Query => "query",
            Family::Key => "key",
        }
    }

    fn endpoint(self) -> &'static str {
        match self {
            Family::Query => "ws/q",
            Family::Key => "ws/key",
        }
    }

    /// The address this family takes from a message, or the error reply.
    fn address(
        self,
        query: Option<String>,
        key: Option<String>,
    ) -> Result<String, NotificationMessage> {
        let (wanted, other) = match self {
            Family::Query => (query, key),
            Family::Key => (key, query),
        };
        if other.is_some() {
            return Err(parameter_error(format!(
                "{} subscriptions are addressed by `{}`",
                self.endpoint(),
                self.field()
            )));
        }
        wanted.ok_or_else(|| {
            parameter_error(format!("missing `{}` for {}", self.field(), self.endpoint()))
        })
    }
}

/// The asset a subscription follows, and how it was addressed.
struct Subscribed<E: Environment> {
    asset: AssetRef<E>,
    query: Option<String>,
    key: Option<Key>,
}

/// Request the addressed asset: `get_asset` for a query, `get` for a key (which must be stored or
/// declared by a recipe, as for `key/submit`).
async fn request<E: Environment>(
    env: &EnvRef<E>,
    family: Family,
    address: &str,
) -> Result<Subscribed<E>, Error> {
    let manager = env.get_asset_manager();
    match family {
        Family::Query => {
            let query = parse_query(address)?;
            let asset = match query.key() {
                Some(key) => {
                    if !manager.contains(&key).await? {
                        return Err(Error::key_not_found(&key));
                    }
                    manager.get(&key).await?
                }
                None => manager.get_asset(&query).await?,
            };
            Ok(Subscribed {
                asset,
                query: Some(address.to_string()),
                key: None,
            })
        }
        Family::Key => {
            let key = parse_key(address)?;
            if !manager.contains(&key).await? {
                return Err(Error::key_not_found(&key));
            }
            let asset = manager.get(&key).await?;
            Ok(Subscribed {
                asset,
                query: None,
                key: Some(key),
            })
        }
    }
}

/// Whether a snapshot status ends the asset's lifecycle for a subscriber.
fn is_terminal(status: Status) -> bool {
    match status {
        Status::Error | Status::Cancelled | Status::Expired | Status::Volatile => true,
        Status::None
        | Status::Directory
        | Status::Recipe
        | Status::Submitted
        | Status::Dependencies
        | Status::Processing
        | Status::Partial
        | Status::Storing
        | Status::Ready
        | Status::Source
        | Status::Override => false,
    }
}

/// Convert a core notification, with its head, to the wire message. Exhaustive.
fn convert_notification(head: Head, notification: AssetNotificationMessage) -> NotificationMessage {
    match notification {
        AssetNotificationMessage::Initial => NotificationMessage::Initial { head },
        AssetNotificationMessage::JobSubmitted => NotificationMessage::JobSubmitted { head },
        AssetNotificationMessage::JobStarted => NotificationMessage::JobStarted { head },
        AssetNotificationMessage::StatusChanged(status) => {
            NotificationMessage::StatusChanged { head, status }
        }
        AssetNotificationMessage::ValueProduced => NotificationMessage::ValueProduced { head },
        AssetNotificationMessage::ErrorOccurred(error) => NotificationMessage::ErrorOccurred {
            head,
            error: error_to_detail(&error),
        },
        AssetNotificationMessage::LogMessage => NotificationMessage::LogMessage { head },
        AssetNotificationMessage::PrimaryProgressUpdated(progress) => {
            NotificationMessage::PrimaryProgressUpdated { head, progress }
        }
        AssetNotificationMessage::SecondaryProgressUpdated(progress) => {
            NotificationMessage::SecondaryProgressUpdated { head, progress }
        }
        AssetNotificationMessage::JobFinished => NotificationMessage::JobFinished { head },
        AssetNotificationMessage::Expired => NotificationMessage::Expired { head },
        AssetNotificationMessage::Removed => NotificationMessage::Removed { head },
    }
}

/// Follow one asset until its lifecycle ends, forwarding each change with a fresh snapshot.
async fn subscription_task<E: Environment>(
    env: EnvRef<E>,
    subscribed: Subscribed<E>,
    tx: mpsc::Sender<NotificationMessage>,
) {
    let Subscribed { asset, query, key } = subscribed;
    // Subscribe before taking the snapshot, so a change in between still wakes the loop.
    let mut rx = asset.subscribe_to_notifications().await;
    let latest = rx.borrow_and_update().clone();
    let key_text = match &key {
        Some(key) => Some(key.encode()),
        None => asset.key().await.map(|k| k.encode()),
    };
    let head = |info: Option<AssetInfo>| Head {
        asset_id: asset.id(),
        query: query.clone(),
        key: key_text.clone(),
        timestamp: now(),
        info,
    };

    let info = asset.get_asset_info().await.ok();
    let terminal = info.as_ref().is_some_and(|i| is_terminal(i.status));
    if tx
        .send(NotificationMessage::Initial { head: head(info) })
        .await
        .is_err()
        || terminal
    {
        return;
    }
    // The watch keeps only the latest message, and the subscriber has not seen it: replay it, so
    // an asset that finished before the subscribe still reports how it finished.
    let mut pending = match latest {
        AssetNotificationMessage::Initial => None,
        other => Some(other),
    };

    loop {
        let notification = match pending.take() {
            Some(notification) => notification,
            None => {
                if rx.changed().await.is_err() {
                    return; // the asset is gone
                }
                rx.borrow_and_update().clone()
            }
        };
        let removed = matches!(notification, AssetNotificationMessage::Removed);
        let info = if removed {
            // The asset is being unmapped; describe the key as it is now, without evaluating.
            match &key {
                Some(key) => env.get_asset_manager().get_asset_info(key).await.ok(),
                None => asset.get_asset_info().await.ok(),
            }
        } else {
            asset.get_asset_info().await.ok()
        };
        let terminal = removed || info.as_ref().is_some_and(|i| is_terminal(i.status));
        let message = convert_notification(head(info), notification);
        if terminal {
            let _ = tx.send(message).await;
            return;
        }
        match tx.try_send(message) {
            Ok(()) | Err(mpsc::error::TrySendError::Full(_)) => {}
            Err(mpsc::error::TrySendError::Closed(_)) => return,
        }
    }
}

/// Own the socket's sink and write every queued message to it.
async fn writer_task(
    mut sink: SplitSink<WebSocket, Message>,
    mut rx: mpsc::Receiver<NotificationMessage>,
) {
    while let Some(message) = rx.recv().await {
        let text = match serde_json::to_string(&message) {
            Ok(text) => text,
            Err(e) => {
                eprintln!("WebSocket: cannot serialize a notification: {e}");
                continue;
            }
        };
        if sink.send(Message::Text(text.into())).await.is_err() {
            break;
        }
    }
    let _ = sink.close().await;
}

/// One socket's subscriptions, keyed by address.
struct Session<E: Environment> {
    env: EnvRef<E>,
    family: Family,
    limits: WebSocketLimits,
    tx: mpsc::Sender<NotificationMessage>,
    subscriptions: HashMap<String, JoinHandle<()>>,
}

impl<E: Environment> Session<E> {
    async fn reply(&self, message: NotificationMessage) {
        let _ = self.tx.send(message).await;
    }

    fn prune(&mut self) {
        self.subscriptions.retain(|_, task| !task.is_finished());
    }

    async fn subscribe(&mut self, address: String) {
        self.prune();
        if !self.subscriptions.contains_key(&address)
            && self.subscriptions.len() >= self.limits.max_subscriptions
        {
            self.reply(parameter_error(format!(
                "subscription limit reached ({} per connection)",
                self.limits.max_subscriptions
            )))
            .await;
            return;
        }
        match request(&self.env, self.family, &address).await {
            Ok(subscribed) => {
                let task = tokio::spawn(subscription_task(
                    self.env.clone(),
                    subscribed,
                    self.tx.clone(),
                ));
                if let Some(previous) = self.subscriptions.insert(address, task) {
                    previous.abort();
                }
            }
            Err(e) => self.reply(error_message(&e)).await,
        }
    }

    fn unsubscribe(&mut self, address: &str) {
        if let Some(task) = self.subscriptions.remove(address) {
            task.abort();
        }
    }

    fn unsubscribe_all(&mut self) {
        for (_, task) in self.subscriptions.drain() {
            task.abort();
        }
    }

    async fn handle_text(&mut self, text: &str) {
        let message = match serde_json::from_str::<ClientMessage>(text) {
            Ok(message) => message,
            Err(e) => {
                self.reply(parameter_error(format!("invalid client message: {e}")))
                    .await;
                return;
            }
        };
        match message {
            ClientMessage::Subscribe { query, key } => match self.family.address(query, key) {
                Ok(address) => self.subscribe(address).await,
                Err(reply) => self.reply(reply).await,
            },
            ClientMessage::Unsubscribe { query, key } => match self.family.address(query, key) {
                Ok(address) => self.unsubscribe(&address),
                Err(reply) => self.reply(reply).await,
            },
            ClientMessage::UnsubscribeAll => {
                self.unsubscribe_all();
                self.reply(NotificationMessage::UnsubscribedAll { timestamp: now() })
                    .await;
            }
            ClientMessage::Ping => {
                self.reply(NotificationMessage::Pong { timestamp: now() })
                    .await;
            }
        }
    }
}

/// Serve one connection until the client disconnects. Disconnecting aborts every subscription
/// task; it does not cancel the evaluations, which other clients may share.
async fn handle_socket<E: Environment>(
    socket: WebSocket,
    env: EnvRef<E>,
    family: Family,
    limits: WebSocketLimits,
    initial: Option<String>,
) {
    let (sink, mut stream) = socket.split();
    let (tx, rx) = mpsc::channel(OUTGOING_QUEUE);
    let writer = tokio::spawn(writer_task(sink, rx));
    let mut session = Session {
        env,
        family,
        limits,
        tx,
        subscriptions: HashMap::new(),
    };
    if let Some(address) = initial {
        session.subscribe(address).await;
    }
    while let Some(incoming) = stream.next().await {
        match incoming {
            Ok(Message::Text(text)) => session.handle_text(text.as_str()).await,
            Ok(Message::Binary(_)) => {
                session
                    .reply(parameter_error(
                        "binary messages are not supported; send JSON text".to_string(),
                    ))
                    .await
            }
            Ok(Message::Ping(_)) | Ok(Message::Pong(_)) => {}
            Ok(Message::Close(_)) => break,
            // A protocol error, or a message over `max_message_size`: the connection is unusable.
            Err(_) => break,
        }
    }
    session.unsubscribe_all();
    drop(session);
    writer.abort();
}

fn upgrade<E: Environment>(
    ws: WebSocketUpgrade,
    env: EnvRef<E>,
    family: Family,
    limits: WebSocketLimits,
    initial: Option<String>,
) -> Response {
    ws.max_message_size(limits.max_message_size)
        .on_upgrade(move |socket| handle_socket(socket, env, family, limits, initial))
}

/// `GET {ws}/q` — query subscriptions.
pub async fn ws_query_handler<E: Environment>(
    ws: WebSocketUpgrade,
    State(env): State<EnvRef<E>>,
    Extension(limits): Extension<WebSocketLimits>,
) -> Response {
    upgrade(ws, env, Family::Query, limits, None)
}

/// `GET {ws}/q/{*query}` — query subscriptions, subscribing to the path on connect.
pub async fn ws_query_path_handler<E: Environment>(
    ws: WebSocketUpgrade,
    Path(query): Path<String>,
    State(env): State<EnvRef<E>>,
    Extension(limits): Extension<WebSocketLimits>,
) -> Response {
    upgrade(ws, env, Family::Query, limits, Some(query))
}

/// `GET {ws}/key` — key subscriptions.
pub async fn ws_key_handler<E: Environment>(
    ws: WebSocketUpgrade,
    State(env): State<EnvRef<E>>,
    Extension(limits): Extension<WebSocketLimits>,
) -> Response {
    upgrade(ws, env, Family::Key, limits, None)
}

/// `GET {ws}/key/{*key}` — key subscriptions, subscribing to the path on connect.
pub async fn ws_key_path_handler<E: Environment>(
    ws: WebSocketUpgrade,
    Path(key): Path<String>,
    State(env): State<EnvRef<E>>,
    Extension(limits): Extension<WebSocketLimits>,
) -> Response {
    upgrade(ws, env, Family::Key, limits, Some(key))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_messages_are_snake_case() {
        let message: ClientMessage =
            serde_json::from_str(r#"{"action":"unsubscribe_all"}"#).unwrap();
        assert!(matches!(message, ClientMessage::UnsubscribeAll));
        let message: ClientMessage =
            serde_json::from_str(r#"{"action":"subscribe","key":"a/b.txt"}"#).unwrap();
        assert!(matches!(
            message,
            ClientMessage::Subscribe { query: None, key: Some(_) }
        ));
        assert!(serde_json::from_str::<ClientMessage>(r#"{"action":"Subscribe"}"#).is_err());
    }

    #[test]
    fn family_refuses_the_other_address_field() {
        assert!(Family::Query
            .address(None, Some("a.txt".to_string()))
            .is_err());
        assert!(Family::Key
            .address(Some("make_text".to_string()), None)
            .is_err());
        assert_eq!(
            Family::Key.address(None, Some("a.txt".to_string())).ok(),
            Some("a.txt".to_string())
        );
    }

    #[test]
    fn notifications_are_flat_with_a_type_tag() {
        let head = Head {
            asset_id: 7,
            query: Some("make_text".to_string()),
            key: None,
            timestamp: "t".to_string(),
            info: None,
        };
        let json = serde_json::to_value(convert_notification(
            head,
            AssetNotificationMessage::Removed,
        ))
        .unwrap();
        assert_eq!(json["type"], "Removed");
        assert_eq!(json["asset_id"], 7);
        assert_eq!(json["query"], "make_text");
    }
}
