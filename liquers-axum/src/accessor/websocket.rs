//! The accessor WebSocket, `GET {base}/ws/{*query}`: one accessor per connection, its events
//! relayed as JSON text frames. Documented with the builder in [`crate::accessor`].

use std::sync::Arc;
use std::time::Duration;

use axum::{
    extract::{
        ws::{close_code, CloseFrame, Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    response::Response,
    Extension,
};
use futures::{SinkExt, StreamExt};
use liquers_core::accessor::{AccessorEvent, ValueAccessor};
use liquers_core::context::{EnvRef, Environment};
use liquers_core::value::ValueInterface;

use super::handlers::resolve;
use crate::assets::common::{error_response, query_from_path};
use crate::server::ServerStop;

/// How often a socket refreshes an accessor whose source cannot notify (the store root).
const REFRESH_INTERVAL: Duration = Duration::from_secs(1);

/// The largest client message; the socket reads none but a close.
const MAX_MESSAGE_SIZE: usize = 4096;

async fn send(socket: &mut futures::stream::SplitSink<WebSocket, Message>, event: &AccessorEvent) -> bool {
    let text = event.to_json().to_string();
    socket.send(Message::Text(text.into())).await.is_ok()
}

async fn session<V: ValueInterface>(
    socket: WebSocket,
    accessor: Arc<dyn ValueAccessor<V>>,
    stop: Option<ServerStop>,
) {
    let _tracked = stop.as_ref().map(|stop| stop.tracker().token());
    let stop_token = stop.as_ref().map(|stop| stop.token().clone());
    let stopped = async {
        match &stop_token {
            Some(token) => token.cancelled().await,
            None => futures::future::pending::<()>().await,
        }
    };
    tokio::pin!(stopped);
    let (mut sink, mut stream) = socket.split();
    let mut events = accessor.subscribe();
    let first = events.borrow_and_update().clone();
    if !send(&mut sink, &first).await {
        return;
    }
    let poll = !accessor.capabilities().notify;
    let mut tick = tokio::time::interval(REFRESH_INTERVAL);
    let close = loop {
        tokio::select! {
            changed = events.changed() => {
                if changed.is_err() {
                    break None;
                }
                let event = events.borrow_and_update().clone();
                if !send(&mut sink, &event).await {
                    return;
                }
                if event == AccessorEvent::Invalidated {
                    break Some(CloseFrame {
                        code: close_code::NORMAL,
                        reason: "the accessor is invalidated".into(),
                    });
                }
            }
            incoming = stream.next() => match incoming {
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => return,
                Some(Ok(Message::Text(_)))
                | Some(Ok(Message::Binary(_)))
                | Some(Ok(Message::Ping(_)))
                | Some(Ok(Message::Pong(_))) => {}
            },
            _ = tick.tick(), if poll => {
                if let Err(e) = accessor.refresh().await {
                    eprintln!("accessor socket: refresh failed: {e}");
                }
            }
            _ = &mut stopped => {
                break Some(CloseFrame {
                    code: close_code::AWAY,
                    reason: "server shutting down".into(),
                });
            }
        }
    };
    if let Some(frame) = close {
        let _ = tokio::time::timeout(
            Duration::from_secs(1),
            sink.send(Message::Close(Some(frame))),
        )
        .await;
    }
    let _ = sink.close().await;
}

/// `GET {base}/ws/{*query}`: resolves the accessor, then relays its events: the current one on
/// connect, then each change (`{"event": "changed", "etag": …}`, `status`, `progress`, `log`,
/// `removed`, `invalidated`). An invalidated accessor closes the socket (1000); a stopping
/// server closes it with 1001. A query that cannot be resolved is answered before the upgrade.
pub async fn ws_handler<E: Environment>(
    ws: WebSocketUpgrade,
    Path(path): Path<String>,
    State(env): State<EnvRef<E>>,
    stop: Option<Extension<ServerStop>>,
) -> Response {
    let query = match query_from_path(&path) {
        Ok(query) => query,
        Err(e) => return error_response(&e, "Failed to parse the query"),
    };
    let accessor = match resolve(&env, &query).await {
        Ok(accessor) => accessor,
        Err(e) => return error_response(&e, "Failed to resolve the accessor"),
    };
    let stop = stop.map(|Extension(stop)| stop);
    ws.max_message_size(MAX_MESSAGE_SIZE)
        .on_upgrade(move |socket| session(socket, accessor, stop))
}
