//! [`LiquersServerBuilder`]: the assembled router and `bind`. Documented with the rest of the
//! server in [`crate::server`].

use std::marker::PhantomData;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use liquers_core::context::{EnvRef, Environment};
use liquers_core::error::Error;
use tokio::net::TcpListener;

use super::config::{normalize_prefix, Api, ServerConfig, WebSetting};
use super::running::LiquersServer;
use super::source::EnvSource;
use crate::web::{index_routes, web_routes};
use crate::{AssetsApiBuilder, QueryApiBuilder, RecipesApiBuilder, StoreApiBuilder};

/// Assembles the standard APIs, the web folder and the index page into one router, and binds it.
///
/// Starts from a [`ServerConfig`] (`from_config`, or `new(prefix)` for the defaults under another
/// prefix); the switches narrow it.
pub struct LiquersServerBuilder<E: Environment> {
    config: ServerConfig,
    websocket: bool,
    index: bool,
    query_timeout: Option<Duration>,
    stop_deadline: Duration,
    _phantom: PhantomData<E>,
}

impl<E: Environment> LiquersServerBuilder<E> {
    /// The default settings ([`ServerConfig::default`]) under `prefix`.
    pub fn new(prefix: impl Into<String>) -> Self {
        Self::from_config(ServerConfig { prefix: prefix.into(), ..ServerConfig::default() })
    }

    /// The settings of a configuration document's `server` section.
    pub fn from_config(config: ServerConfig) -> Self {
        LiquersServerBuilder {
            config,
            websocket: true,
            index: true,
            query_timeout: None,
            stop_deadline: Duration::from_secs(5),
            _phantom: PhantomData,
        }
    }

    /// The settings as they stand.
    pub fn config(&self) -> &ServerConfig {
        &self.config
    }

    /// Leave out the store API and the assets mutation routes.
    pub fn read_only(mut self) -> Self {
        self.config.read_only = true;
        self
    }

    /// Leave out one of the standard APIs.
    pub fn without_api(mut self, api: Api) -> Self {
        self.config.apis.retain(|a| *a != api);
        self
    }

    /// Leave out the assets API's WebSocket endpoints.
    pub fn without_websocket(mut self) -> Self {
        self.websocket = false;
        self
    }

    /// How long `{prefix}/q/…` waits for a value (the query API's default otherwise, 30 s).
    pub fn with_query_timeout(mut self, timeout: Duration) -> Self {
        self.query_timeout = Some(timeout);
        self
    }

    /// The web folder: on, off, or another store folder.
    pub fn with_web(mut self, web: WebSetting) -> Self {
        self.config.web = web;
        self
    }

    /// Leave out `/` and `/index.html`, for a host that serves its own.
    pub fn without_index(mut self) -> Self {
        self.index = false;
        self
    }

    /// The deadline a [`LiquersServer::stop_token`] stop uses (default 5 s).
    pub fn with_stop_deadline(mut self, deadline: Duration) -> Self {
        self.stop_deadline = deadline;
        self
    }

    /// The assembled router, for a host that serves it itself (`.with_state(envref)`). A prefix or
    /// web root that is not valid leaves its routes out and is reported by [`Self::bind`]; call
    /// [`ServerConfig::validate`] first to see the error.
    pub fn router(&self) -> Router<EnvRef<E>> {
        let prefix = normalize_prefix(&self.config.prefix).unwrap_or_default();
        let read_only = self.config.read_only;
        let mut router = Router::new();
        let mut mounted: Vec<Api> = Vec::new();
        for api in &self.config.apis {
            if mounted.contains(api) {
                continue;
            }
            mounted.push(*api);
            router = match api {
                Api::Query => {
                    let mut query = QueryApiBuilder::<E>::new(format!("{prefix}/q"));
                    if let Some(timeout) = self.query_timeout {
                        query = query.with_timeout(timeout);
                    }
                    router.merge(query.build())
                }
                Api::Store if read_only => router,
                Api::Store => router.merge(StoreApiBuilder::<E>::new(format!("{prefix}/api/store")).build()),
                Api::Assets => {
                    let mut assets = AssetsApiBuilder::<E>::new(format!("{prefix}/api/assets"));
                    if read_only {
                        assets = assets.read_only();
                    }
                    if !self.websocket {
                        assets = assets.without_websocket();
                    }
                    router.merge(assets.build())
                }
                Api::Recipes => {
                    router.merge(RecipesApiBuilder::<E>::new(format!("{prefix}/api/recipes")).build())
                }
            };
        }
        if let Ok(Some(root)) = self.config.web.root() {
            router = router.merge(web_routes::<E>(&prefix, root));
            if self.index {
                router = router.merge(index_routes::<E>(&prefix));
            }
        }
        router
    }

    /// Binds `host:port` and starts serving `source` in the background. Fails, with nothing
    /// listening, when the settings are invalid or the address cannot be bound.
    pub async fn bind(self, source: impl EnvSource<E>) -> Result<LiquersServer, Error> {
        self.config.validate()?;
        let host = self.config.host.trim();
        let address = if host.contains(':') && !host.starts_with('[') {
            format!("[{host}]:{}", self.config.port)
        } else {
            format!("{host}:{}", self.config.port)
        };
        let listener = TcpListener::bind(&address)
            .await
            .map_err(|e| Error::general_error(format!("cannot listen on {address}: {e}")))?;
        let router = self.router();
        LiquersServer::start(listener, router, Arc::new(source), self.stop_deadline).await
    }
}
