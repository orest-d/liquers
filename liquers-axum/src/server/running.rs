//! The running server: the accept loop, [`LiquersServer`] and [`ServerStop`]. Documented with the
//! rest of the server in [`crate::server`].

use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwapOption;
use axum::body::Body;
use axum::http::{Request, Response, StatusCode};
use axum::response::IntoResponse;
use axum::Router;
use hyper::body::Incoming;
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto;
use hyper_util::server::graceful::GracefulShutdown;
use liquers_core::context::{EnvRef, Environment};
use tokio::net::TcpListener;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;
use tower::ServiceExt;

use super::source::EnvSource;
use crate::api_core::{error_to_detail, ApiResponse};

/// The stop signal of a running server, as a request extension: WebSocket sessions hold a
/// tracker token for their life and close with `1001 Going Away` when the token is cancelled.
/// Without a server (a router served by the host's own loop) the extension is absent.
#[derive(Clone, Debug)]
pub struct ServerStop {
    token: CancellationToken,
    tracker: TaskTracker,
}

impl ServerStop {
    /// Cancelled when the server starts stopping.
    pub fn token(&self) -> &CancellationToken {
        &self.token
    }

    /// The sessions the server waits for (up to its deadline) before it reports itself stopped.
    pub fn tracker(&self) -> &TaskTracker {
        &self.tracker
    }
}

/// A server listening on its address, until [`Self::shutdown`] or its [`Self::stop_token`] stops it.
///
/// Dropping the handle does not stop the server: it keeps serving in the background, and the
/// stop token (taken before the drop) or the end of the runtime stops it.
pub struct LiquersServer {
    addr: SocketAddr,
    stop: CancellationToken,
    deadline: tokio::sync::watch::Sender<Duration>,
    finished: CancellationToken,
}

impl std::fmt::Debug for LiquersServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LiquersServer")
            .field("addr", &self.addr)
            .field("running", &self.is_running())
            .finish()
    }
}

impl LiquersServer {
    /// The address the server listens on (with the real port when it was bound to port 0).
    pub fn local_addr(&self) -> SocketAddr {
        self.addr
    }

    /// Whether the accept loop still runs: false once it has stopped and released everything.
    pub fn is_running(&self) -> bool {
        !self.finished.is_cancelled()
    }

    /// Waits until the server has stopped.
    pub async fn stopped(&self) {
        self.finished.cancelled().await
    }

    /// Stops the server: no new connections (the port is free when this returns), in-flight
    /// requests may finish, WebSockets get a Close frame, and whatever still runs at `deadline` is
    /// aborted (a request's evaluation itself is not cancelled). Idempotent: a second call waits
    /// for the first stop.
    pub async fn shutdown(&self, deadline: Duration) {
        if !self.stop.is_cancelled() {
            self.deadline.send_replace(deadline);
            self.stop.cancel();
        }
        self.finished.cancelled().await
    }

    /// A token that stops the server when cancelled, with the builder's default deadline. Lets
    /// code that does not hold the server stop it.
    pub fn stop_token(&self) -> CancellationToken {
        self.stop.clone()
    }

    pub(crate) async fn start<E: Environment>(
        listener: TcpListener,
        router: Router<EnvRef<E>>,
        source: Arc<dyn EnvSource<E>>,
        default_deadline: Duration,
    ) -> Result<LiquersServer, liquers_core::error::Error> {
        let addr = listener.local_addr().map_err(|e| {
            liquers_core::error::Error::general_error(format!("cannot read the bound address: {e}"))
        })?;
        let stop = CancellationToken::new();
        let finished = CancellationToken::new();
        let (deadline, deadline_rx) = tokio::sync::watch::channel(default_deadline);
        let server_stop = ServerStop {
            token: CancellationToken::new(),
            tracker: TaskTracker::new(),
        };
        let accept = AcceptLoop {
            listener,
            service: Arc::new(ServiceState {
                router,
                source,
                cache: ArcSwapOption::empty(),
                stop: server_stop.clone(),
            }),
            stop: stop.clone(),
            finished: finished.clone(),
            deadline: deadline_rx,
            server_stop,
        };
        tokio::spawn(accept.run());
        Ok(LiquersServer { addr, stop, deadline, finished })
    }
}

struct ServiceState<E: Environment> {
    router: Router<EnvRef<E>>,
    source: Arc<dyn EnvSource<E>>,
    cache: ArcSwapOption<(EnvRef<E>, Router)>,
    stop: ServerStop,
}

impl<E: Environment> ServiceState<E> {
    async fn call(self: Arc<Self>, request: Request<Incoming>) -> Response<Body> {
        let env = match self.source.current().await {
            Ok(env) => env,
            Err(e) => {
                let mut response = ApiResponse::<()>::error(error_to_detail(&e), "No environment")
                    .into_response();
                *response.status_mut() = StatusCode::SERVICE_UNAVAILABLE;
                return response;
            }
        };
        let cached = self.cache.load_full();
        let router = match cached {
            Some(entry) if Arc::ptr_eq(&entry.0 .0, &env.0) => entry.1.clone(),
            Some(_) | None => {
                let stated = self.router.clone().with_state(env.clone());
                self.cache.store(Some(Arc::new((env, stated.clone()))));
                stated
            }
        };
        let mut request = request.map(Body::new);
        request.extensions_mut().insert(self.stop.clone());
        match router.oneshot(request).await {
            Ok(response) => response,
            Err(never) => match never {},
        }
    }
}

struct AcceptLoop<E: Environment> {
    listener: TcpListener,
    service: Arc<ServiceState<E>>,
    stop: CancellationToken,
    finished: CancellationToken,
    deadline: tokio::sync::watch::Receiver<Duration>,
    server_stop: ServerStop,
}

impl<E: Environment> AcceptLoop<E> {
    async fn run(self) {
        let AcceptLoop { listener, service, stop, finished, deadline, server_stop } = self;
        let builder = auto::Builder::new(TokioExecutor::new());
        let graceful = GracefulShutdown::new();
        let mut connections: JoinSet<()> = JoinSet::new();
        loop {
            tokio::select! {
                _ = stop.cancelled() => break,
                Some(_) = connections.join_next(), if !connections.is_empty() => {}
                accepted = listener.accept() => match accepted {
                    Ok((stream, _peer)) => {
                        let service = service.clone();
                        let hyper_service = hyper::service::service_fn(move |request| {
                            let service = service.clone();
                            async move { Ok::<_, Infallible>(service.call(request).await) }
                        });
                        let connection = builder
                            .serve_connection_with_upgrades(TokioIo::new(stream), hyper_service)
                            .into_owned();
                        let watched = graceful.watch(connection);
                        connections.spawn(async move {
                            let _ = watched.await;
                        });
                    }
                    Err(e) => {
                        eprintln!("liquers server: accept failed: {e}");
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    }
                },
            }
        }
        // Stop accepting: the port is free from here.
        drop(listener);
        let deadline = *deadline.borrow();
        server_stop.token.cancel();
        server_stop.tracker.close();
        let drain = async {
            graceful.shutdown().await;
            while connections.join_next().await.is_some() {}
            server_stop.tracker.wait().await;
        };
        if tokio::time::timeout(deadline, drain).await.is_err() {
            connections.abort_all();
            while connections.join_next().await.is_some() {}
        }
        service.cache.store(None);
        finished.cancel();
    }
}
