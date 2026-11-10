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
//!
//! **Argument overrides** (`specs/design/axum-query-named-arguments/`): on `ws/q`, a `subscribe`
//! with a non-empty `arguments` object evaluates the query with them, ad hoc, as an **unmanaged**
//! asset — one the asset manager does not hold, so nothing but this subscription can see it.
//! Every notification says whether its asset is managed. An unmanaged subscription is named by a
//! `subscription_id` (a UUID, valid on this connection only), by which it can be unsubscribed or
//! cancelled; a managed one is addressed by its query or key as before. The id is acknowledged
//! at once in a `Subscribed` message (echoing the subscribe's optional `request_id`), before any
//! notification. `arguments` absent or `{}` is an ordinary subscription.
//!
//! Under a running [`LiquersServer`](crate::server::LiquersServer) each session also carries a
//! [`ServerStop`]: the server waits for it while stopping, and the session answers the stop with
//! `Close(1001 Going Away)`, even while it waits on a client that stopped reading. Served by a
//! host's own loop, a session runs until its client goes.
//!
//! # Main components
//!
//! - [`ws_query_handler`], [`ws_query_path_handler`]: the `{ws}/q` endpoints.
//! - [`ws_key_handler`], [`ws_key_path_handler`]: the `{ws}/key` endpoints.
//! - [`WebSocketLimits`]: the largest client message and the subscriptions per connection.
//!
//! # Where it is used
//!
//! - [`AssetsApiBuilder::build`](crate::AssetsApiBuilder::build) mounts the four handlers.
//! - `tests/assets_websocket.rs` and `tests/argument_overrides_ws.rs`.
//! - `tests/server_shutdown.rs` (`websocket_gets_close_frame`).
//!
//! # Related concepts
//!
//! - The REST `q/info` and `key/info` routes answer once; a subscription follows the asset.
//! - `AssetManager` notifications are the source; this module only forwards them.
//!
//! # Examples
//!
//! ```
//! use liquers_axum::AssetsApiBuilder;
//! use liquers_core::{context::{Environment, SimpleEnvironment}, value::Value};
//!
//! # let runtime = tokio::runtime::Runtime::new().unwrap();
//! # let _inside = runtime.enter();   // an environment starts its manager on a runtime
//! let envref = SimpleEnvironment::<Value>::new().to_ref();
//! // `/api/assets/ws/q` and `/api/assets/ws/key` are among the routes.
//! let app: axum::Router = AssetsApiBuilder::<SimpleEnvironment<Value>>::new("/api/assets")
//!     .build()
//!     .with_state(envref);
//! # drop(app);
//! ```
//!
//! More: `tests/assets_websocket.rs` (AWS01–AWS14b).

use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, OnceLock,
};

use axum::{
    extract::{
        ws::{close_code, CloseFrame, Message, WebSocket, WebSocketUpgrade},
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
    assets::{AssetManager, AssetNotificationMessage, AssetRef, EvalMode},
    context::{EnvRef, Environment},
    error::{Error, ErrorType},
    metadata::{AssetInfo, ProgressEntry, Status},
    parse::{parse_key, parse_query},
    query::{Key, Query},
    state::State as AssetState,
};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::{
    sync::{mpsc, oneshot},
    task::JoinHandle,
};

use crate::server::ServerStop;
use tokio_util::sync::CancellationToken;

use crate::api_core::{
    error::error_to_detail,
    overrides::{OverrideRequest, RouteOptions},
    ErrorDetail,
};
use crate::assets::common::is_managed;

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
    /// Request the asset and follow its notifications. A non-empty `arguments` object (`ws/q`
    /// only) holds argument overrides of the query's last action; the evaluation is then
    /// unmanaged and named by the `subscription_id` its notifications carry.
    Subscribe {
        #[serde(default)]
        query: Option<String>,
        #[serde(default)]
        key: Option<String>,
        #[serde(default)]
        arguments: Option<serde_json::Map<String, serde_json::Value>>,
        /// Echoed in the `Subscribed` acknowledgement of an override subscription, so a client
        /// can tell which `subscribe` an id answers.
        #[serde(default)]
        request_id: Option<String>,
    },
    /// Stop following an asset: by its address, or an unmanaged one by its `subscription_id`.
    Unsubscribe {
        #[serde(default)]
        query: Option<String>,
        #[serde(default)]
        key: Option<String>,
        #[serde(default)]
        subscription_id: Option<String>,
    },
    /// Cancel the evaluation of an unmanaged subscription (the only way to reach it).
    Cancel { subscription_id: String },
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
    pub info: Option<Box<AssetInfo>>,
    /// Whether the asset is managed — held by the asset manager under its address.
    pub managed: bool,
    /// The subscription's id, for an unmanaged asset only.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subscription_id: Option<String>,
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
    /// Sent at once for a `subscribe` with `arguments`, before any notification of it: the id
    /// of the unmanaged subscription, by which it can be cancelled even while its evaluation is
    /// still being set up.
    Subscribed {
        timestamp: String,
        subscription_id: String,
        query: String,
        managed: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        request_id: Option<String>,
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
        /// The unmanaged subscription the error belongs to, if any.
        #[serde(skip_serializing_if = "Option::is_none")]
        subscription_id: Option<String>,
    },
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn error_message(error: &Error) -> NotificationMessage {
    error_message_for(error, None)
}

fn error_message_for(error: &Error, subscription_id: Option<String>) -> NotificationMessage {
    NotificationMessage::Error {
        timestamp: now(),
        error: error_to_detail(error),
        subscription_id,
    }
}

/// The one answer to an id that does not name an unmanaged subscription of this connection: it
/// says nothing about other connections, or about whether the id exists elsewhere.
fn unknown_subscription_id() -> NotificationMessage {
    parameter_error("unknown subscription id".to_string())
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

    /// The address this family takes from a message.
    fn address(self, query: Option<String>, key: Option<String>) -> Result<String, Error> {
        let (wanted, other) = match self {
            Family::Query => (query, key),
            Family::Key => (key, query),
        };
        if other.is_some() {
            return Err(Error::from_error(
                ErrorType::ParameterError,
                format!(
                    "{} subscriptions are addressed by `{}`",
                    self.endpoint(),
                    self.field()
                ),
            ));
        }
        wanted.ok_or_else(|| {
            Error::from_error(
                ErrorType::ParameterError,
                format!("missing `{}` for {}", self.field(), self.endpoint()),
            )
        })
    }
}

/// The asset a subscription follows, and how it was addressed.
struct Subscribed<E: Environment> {
    asset: AssetRef<E>,
    query: Option<String>,
    key: Option<Key>,
}

/// How a subscription is reported: whether its asset is managed, and its id when it is not.
#[derive(Clone)]
struct Reporting {
    managed: bool,
    subscription_id: Option<String>,
}

/// Evaluate `address` (a query) with argument overrides, ad hoc: the unmanaged asset is returned
/// without waiting for it (`AssetManager::apply_queued`).
async fn request_overridden<E: Environment>(
    env: &EnvRef<E>,
    address: &str,
    overrides: &OverrideRequest,
) -> Result<Subscribed<E>, Error> {
    let query = parse_query(address)?;
    let recipe = overrides.to_recipe(&query, env.get_command_metadata_registry())?;
    let asset = env
        .get_asset_manager()
        .apply_queued(recipe, AssetState::new())
        .await?;
    Ok(Subscribed {
        asset,
        query: Some(address.to_string()),
        key: None,
    })
}

/// The query whose address `subscribed` was requested under, for the managed check.
fn subscribed_query<E: Environment>(subscribed: &Subscribed<E>) -> Option<Query> {
    match (&subscribed.key, &subscribed.query) {
        (Some(key), _) => Some(Query::from(key)),
        (None, Some(query)) => parse_query(query).ok(),
        (None, None) => None,
    }
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
                    if !manager.can_make(&key).await? {
                        return Err(Error::asset_not_found(&key));
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
            if !manager.can_make(&key).await? {
                return Err(Error::asset_not_found(&key));
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
    reporting: Reporting,
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
        info: info.map(Box::new),
        managed: reporting.managed,
        subscription_id: reporting.subscription_id.clone(),
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

/// Own the socket's sink and write every queued message to it. A frame on `close` (a server
/// stopping) is sent as the last message.
async fn writer_task(
    mut sink: SplitSink<WebSocket, Message>,
    mut rx: mpsc::Receiver<NotificationMessage>,
    mut close: oneshot::Receiver<CloseFrame>,
) {
    let mut close_open = true;
    loop {
        tokio::select! {
            biased;
            frame = &mut close, if close_open => match frame {
                Ok(frame) => {
                    let _ = sink.send(Message::Close(Some(frame))).await;
                    break;
                }
                Err(_) => close_open = false,
            },
            message = rx.recv() => {
                let Some(message) = message else { break };
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
        }
    }
    let _ = sink.close().await;
}

/// A running subscription, as its session holds it.
struct SubscriptionHandle<E: Environment> {
    task: JoinHandle<()>,
    /// Set by the task once the asset has been requested.
    asset: Arc<OnceLock<AssetRef<E>>>,
    /// Set by the task (at once for an override subscription); an id is usable only when false.
    managed: Arc<OnceLock<bool>>,
    /// A `cancel` that arrived before the asset existed; the task applies it once it has it.
    cancel_requested: Arc<AtomicBool>,
}

impl<E: Environment> SubscriptionHandle<E> {
    fn is_unmanaged(&self) -> bool {
        self.managed.get() == Some(&false)
    }
}

/// One socket's subscriptions, by subscription id; subscriptions without overrides are also
/// indexed by address, so re-subscribing to an address replaces the previous one.
struct Session<E: Environment> {
    env: EnvRef<E>,
    family: Family,
    limits: WebSocketLimits,
    tx: mpsc::Sender<NotificationMessage>,
    subscriptions: HashMap<String, SubscriptionHandle<E>>,
    by_address: HashMap<String, String>,
}

impl<E: Environment> Session<E> {
    async fn reply(&self, message: NotificationMessage) {
        let _ = self.tx.send(message).await;
    }

    fn prune(&mut self) {
        self.subscriptions.retain(|_, handle| !handle.task.is_finished());
        let live = &self.subscriptions;
        self.by_address.retain(|_, id| live.contains_key(id));
    }

    /// The argument overrides of a `subscribe`; `None` when there are none (absent or `{}`).
    fn overrides(
        &self,
        arguments: Option<serde_json::Map<String, serde_json::Value>>,
    ) -> Result<Option<OverrideRequest>, Error> {
        let Some(arguments) = arguments.filter(|a| !a.is_empty()) else {
            return Ok(None);
        };
        match self.family {
            Family::Query => {}
            Family::Key => {
                return Err(Error::from_error(
                    ErrorType::ParameterError,
                    "ws/key subscriptions take no arguments: a key's asset is defined by its \
                     recipe; use ws/q"
                        .to_string(),
                ))
            }
        }
        let body = serde_json::to_vec(&serde_json::Value::Object(arguments))
            .map_err(|e| Error::from_error(ErrorType::ParameterError, e.to_string()))?;
        let overrides = OverrideRequest::parse(Vec::new(), &body, RouteOptions::None)?;
        Ok((!overrides.is_empty()).then_some(overrides))
    }

    async fn subscribe(
        &mut self,
        address: String,
        arguments: Option<serde_json::Map<String, serde_json::Value>>,
        request_id: Option<String>,
    ) {
        self.prune();
        let overrides = match self.overrides(arguments) {
            Ok(overrides) => overrides,
            Err(e) => return self.reply(error_message(&e)).await,
        };
        let replacing = overrides.is_none() && self.by_address.contains_key(&address);
        if !replacing && self.subscriptions.len() >= self.limits.max_subscriptions {
            self.reply(parameter_error(format!(
                "subscription limit reached ({} per connection)",
                self.limits.max_subscriptions
            )))
            .await;
            return;
        }
        let id = uuid::Uuid::new_v4().to_string();
        let asset = Arc::new(OnceLock::new());
        let managed = Arc::new(OnceLock::new());
        let cancel_requested = Arc::new(AtomicBool::new(false));
        if overrides.is_some() {
            // Known up front: an override subscription is unmanaged, so its id works at once —
            // and is acknowledged now, before the task can send any notification of it.
            let _ = managed.set(false);
            self.reply(NotificationMessage::Subscribed {
                timestamp: now(),
                subscription_id: id.clone(),
                query: address.clone(),
                managed: false,
                request_id,
            })
            .await;
        }
        // The request runs in the subscription's own task: with an inline manager
        // (`EvalMode::Inline`) `get`/`get_asset`/`apply_queued` evaluate before returning, and the
        // session must keep answering `ping`, `unsubscribe` and `cancel` meanwhile.
        let env = self.env.clone();
        let family = self.family;
        let tx = self.tx.clone();
        let target = address.clone();
        let (task_asset, task_managed, task_cancel, task_id) =
            (asset.clone(), managed.clone(), cancel_requested.clone(), id.clone());
        let task = tokio::spawn(async move {
            let overridden = overrides.is_some();
            let requested = match &overrides {
                Some(overrides) => request_overridden(&env, &target, overrides).await,
                None => request(&env, family, &target).await,
            };
            let subscribed = match requested {
                Ok(subscribed) => subscribed,
                Err(e) => {
                    let id = overridden.then(|| task_id.clone());
                    let _ = tx.send(error_message_for(&e, id)).await;
                    return;
                }
            };
            let _ = task_asset.set(subscribed.asset.clone());
            let is_managed = match (overridden, subscribed_query(&subscribed)) {
                (true, _) | (false, None) => false,
                (false, Some(query)) => is_managed(&env, &query, &subscribed.asset, false).await,
            };
            let _ = task_managed.set(is_managed);
            if task_cancel.load(Ordering::SeqCst) {
                let _ = subscribed.asset.cancel().await;
            }
            let reporting = Reporting {
                managed: is_managed,
                subscription_id: (!is_managed).then_some(task_id),
            };
            subscription_task(env, subscribed, reporting, tx).await;
        });
        let handle = SubscriptionHandle {
            task,
            asset,
            managed,
            cancel_requested,
        };
        if handle.managed.get().is_none() {
            if let Some(previous) = self.by_address.insert(address, id.clone()) {
                if let Some(previous) = self.subscriptions.remove(&previous) {
                    previous.task.abort();
                }
            }
        }
        self.subscriptions.insert(id, handle);
    }

    fn unsubscribe(&mut self, address: &str) {
        if let Some(id) = self.by_address.remove(address) {
            if let Some(handle) = self.subscriptions.remove(&id) {
                handle.task.abort();
            }
        }
    }

    async fn unsubscribe_id(&mut self, id: &str) {
        match self.subscriptions.get(id) {
            Some(handle) if handle.is_unmanaged() => {
                if let Some(handle) = self.subscriptions.remove(id) {
                    handle.task.abort();
                }
                self.by_address.retain(|_, v| v != id);
            }
            Some(_) | None => self.reply(unknown_subscription_id()).await,
        }
    }

    async fn cancel(&mut self, id: &str) {
        let Some(handle) = self.subscriptions.get(id).filter(|h| h.is_unmanaged()) else {
            return self.reply(unknown_subscription_id()).await;
        };
        match handle.asset.get() {
            Some(asset) => {
                if let Err(e) = asset.cancel().await {
                    self.reply(error_message_for(&e, Some(id.to_string()))).await;
                }
            }
            None => {
                // The asset does not exist yet: the task cancels it as soon as it has it. An
                // inline manager is evaluating inside the task (`apply_queued` returns only after
                // the evaluation), so stopping the task is what stops it there.
                handle.cancel_requested.store(true, Ordering::SeqCst);
                match self.env.get_asset_manager().eval_mode() {
                    EvalMode::Inline => handle.task.abort(),
                    EvalMode::Queued => {}
                }
            }
        }
    }

    fn unsubscribe_all(&mut self) {
        for (_, handle) in self.subscriptions.drain() {
            handle.task.abort();
        }
        self.by_address.clear();
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
            ClientMessage::Subscribe {
                query,
                key,
                arguments,
                request_id,
            } => match self.family.address(query, key) {
                Ok(address) => self.subscribe(address, arguments, request_id).await,
                Err(e) => self.reply(error_message(&e)).await,
            },
            ClientMessage::Unsubscribe {
                query: None,
                key: None,
                subscription_id: Some(id),
            } => self.unsubscribe_id(&id).await,
            ClientMessage::Unsubscribe {
                query,
                key,
                subscription_id: None,
            } => match self.family.address(query, key) {
                Ok(address) => self.unsubscribe(&address),
                Err(e) => self.reply(error_message(&e)).await,
            },
            ClientMessage::Unsubscribe {
                subscription_id: Some(_),
                ..
            } => {
                self.reply(parameter_error(
                    "unsubscribe by `subscription_id` or by address, not both".to_string(),
                ))
                .await
            }
            ClientMessage::Cancel { subscription_id } => self.cancel(&subscription_id).await,
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
///
/// Under a [`ServerStop`] (a session of a running `LiquersServer`) the session is tracked for its
/// life, and when the server stops it sends `Close(1001 Going Away)` and ends, waiting up to 1 s
/// for the frame to be written. The stop interrupts whatever the session is doing, including a
/// reply waiting for room in the outgoing queue, so a client that stops reading cannot keep it
/// alive past the server's deadline. Without one (a router served by the host's own loop) it runs
/// until the client goes.
async fn handle_socket<E: Environment>(
    socket: WebSocket,
    env: EnvRef<E>,
    family: Family,
    limits: WebSocketLimits,
    initial: Option<String>,
    stop: Option<ServerStop>,
) {
    let _tracked = stop.as_ref().map(|stop| stop.tracker().token());
    let stop_token = stop.as_ref().map(|stop| stop.token().clone());
    let (sink, mut stream) = socket.split();
    let (tx, rx) = mpsc::channel(OUTGOING_QUEUE);
    let (close_tx, close_rx) = oneshot::channel();
    let mut writer = tokio::spawn(writer_task(sink, rx, close_rx));
    let mut session = Session {
        env,
        family,
        limits,
        tx,
        subscriptions: HashMap::new(),
        by_address: HashMap::new(),
    };
    let mut stopping = false;
    if let Some(address) = initial {
        let subscribed = session.subscribe(address, None, None);
        stopping = until_stopped(stop_token.as_ref(), subscribed).await.is_none();
    }
    while !stopping {
        let Some(incoming) = until_stopped(stop_token.as_ref(), stream.next()).await else {
            stopping = true;
            break;
        };
        let Some(incoming) = incoming else { break };
        // Handling a message can wait on the outgoing queue (a client that stops reading fills
        // it), so it is raced against the stop too: a stopping server never waits on a client.
        let handled = async {
            match incoming {
                Ok(Message::Text(text)) => {
                    session.handle_text(text.as_str()).await;
                    true
                }
                Ok(Message::Binary(_)) => {
                    session
                        .reply(parameter_error(
                            "binary messages are not supported; send JSON text".to_string(),
                        ))
                        .await;
                    true
                }
                Ok(Message::Ping(_)) | Ok(Message::Pong(_)) => true,
                Ok(Message::Close(_)) => false,
                // A protocol error, or a message over `max_message_size`: the connection is
                // unusable.
                Err(_) => false,
            }
        };
        match until_stopped(stop_token.as_ref(), handled).await {
            Some(true) => {}
            Some(false) => break,
            None => stopping = true,
        }
    }
    session.unsubscribe_all();
    drop(session);
    if stopping {
        let _ = close_tx.send(CloseFrame {
            code: close_code::AWAY,
            reason: "server shutting down".into(),
        });
        if tokio::time::timeout(Duration::from_secs(1), &mut writer).await.is_err() {
            writer.abort();
        }
    } else {
        writer.abort();
    }
}

/// Runs `work` to completion, or until `stop` is cancelled (`None`). Without a stop token the
/// work always completes.
async fn until_stopped<T>(stop: Option<&CancellationToken>, work: impl std::future::Future<Output = T>) -> Option<T> {
    match stop {
        Some(token) => tokio::select! {
            output = work => Some(output),
            _ = token.cancelled() => None,
        },
        None => Some(work.await),
    }
}

fn upgrade<E: Environment>(
    ws: WebSocketUpgrade,
    env: EnvRef<E>,
    family: Family,
    limits: WebSocketLimits,
    initial: Option<String>,
    stop: Option<Extension<ServerStop>>,
) -> Response {
    let stop = stop.map(|Extension(stop)| stop);
    ws.max_message_size(limits.max_message_size)
        .on_upgrade(move |socket| handle_socket(socket, env, family, limits, initial, stop))
}

/// `GET {ws}/q` — query subscriptions.
pub async fn ws_query_handler<E: Environment>(
    ws: WebSocketUpgrade,
    State(env): State<EnvRef<E>>,
    Extension(limits): Extension<WebSocketLimits>,
    stop: Option<Extension<ServerStop>>,
) -> Response {
    upgrade(ws, env, Family::Query, limits, None, stop)
}

/// `GET {ws}/q/{*query}` — query subscriptions, subscribing to the path on connect.
pub async fn ws_query_path_handler<E: Environment>(
    ws: WebSocketUpgrade,
    Path(query): Path<String>,
    State(env): State<EnvRef<E>>,
    Extension(limits): Extension<WebSocketLimits>,
    stop: Option<Extension<ServerStop>>,
) -> Response {
    upgrade(ws, env, Family::Query, limits, Some(query), stop)
}

/// `GET {ws}/key` — key subscriptions.
pub async fn ws_key_handler<E: Environment>(
    ws: WebSocketUpgrade,
    State(env): State<EnvRef<E>>,
    Extension(limits): Extension<WebSocketLimits>,
    stop: Option<Extension<ServerStop>>,
) -> Response {
    upgrade(ws, env, Family::Key, limits, None, stop)
}

/// `GET {ws}/key/{*key}` — key subscriptions, subscribing to the path on connect.
pub async fn ws_key_path_handler<E: Environment>(
    ws: WebSocketUpgrade,
    Path(key): Path<String>,
    State(env): State<EnvRef<E>>,
    Extension(limits): Extension<WebSocketLimits>,
    stop: Option<Extension<ServerStop>>,
) -> Response {
    upgrade(ws, env, Family::Key, limits, Some(key), stop)
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
            ClientMessage::Subscribe { query: None, key: Some(_), .. }
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
            managed: true,
            subscription_id: None,
        };
        let json = serde_json::to_value(convert_notification(
            head,
            AssetNotificationMessage::Removed,
        ))
        .unwrap();
        assert_eq!(json["type"], "Removed");
        assert_eq!(json["asset_id"], 7);
        assert_eq!(json["query"], "make_text");
        assert_eq!(json["managed"], true);
        assert!(json.get("subscription_id").is_none(), "a managed subscription shows no id");
    }
}
