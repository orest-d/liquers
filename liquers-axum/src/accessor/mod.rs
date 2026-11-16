//! The accessor API: read, conditionally write and watch a value through a settable accessor.
//!
//! Each route takes a query in its path. The query is evaluated; when its value is an accessor
//! (an `acc` command such as `-R-key/config.yaml/-/ns-acc/accessor/field-title`), that accessor is
//! used, otherwise a read-only accessor of the query's own asset. A write is a compare-and-set on
//! the accessor's token, which travels as the HTTP `ETag`: `If-Match` sets a value that exists,
//! `If-None-Match: *` creates one, and a token that no longer matches answers `412` with the
//! current `ETag`.
//!
//! The API is opt-in: in a server it is `Api::Accessors`, mounted at `{prefix}/api/acc` only when
//! `apis` lists `accessors`, and `bind` refuses it for an environment whose value type has no
//! accessors (no `Accessor` type in its registry).
//!
//! # Main components
//!
//! - [`AccessorApiBuilder`]: the router, with `read_only()` (no `PUT`) and `without_websocket()`.
//! - [`handlers`]: `GET`/`PUT {base}/q/{*query}`, `GET {base}/info/…`, `log/…?since=n`,
//!   `describe/…`.
//! - [`websocket`]: `GET {base}/ws/{*query}`, the accessor's events as JSON frames.
//!
//! # Where it is used
//!
//! - [`LiquersServerBuilder`](crate::LiquersServerBuilder) mounts it for `Api::Accessors`.
//! - `tests/accessor_api.rs`.
//!
//! # Related concepts
//!
//! - `liquers_core::accessor`: `ValueAccessor`, its tokens (`Fingerprint::token`) and events.
//! - `liquers-lib`'s `acc` commands make the accessors these routes serve.
//! - [`crate::assets`]: the asset routes and socket, whose connection handling this follows.
//!
//! # Examples
//!
//! ```
//! use liquers_axum::accessor::AccessorApiBuilder;
//! use liquers_core::{context::{Environment, SimpleEnvironment}, value::Value};
//!
//! # let runtime = tokio::runtime::Runtime::new().unwrap();
//! # let _inside = runtime.enter();   // an environment starts its manager on a runtime
//! let envref = SimpleEnvironment::<Value>::new().to_ref();
//! // GET/PUT /api/acc/q/{*query}, /api/acc/info/…, /api/acc/ws/… — no PUT when read_only().
//! let app: axum::Router = AccessorApiBuilder::<SimpleEnvironment<Value>>::new("/api/acc")
//!     .build()
//!     .with_state(envref);
//! # drop(app);
//! ```
//!
//! More: `tests/accessor_api.rs`.

pub mod handlers;
pub mod websocket;

use std::marker::PhantomData;

use axum::{routing::get, Router};
use liquers_core::context::{EnvRef, Environment};

/// Builder of the accessor routes under one base path.
pub struct AccessorApiBuilder<E: Environment> {
    base_path: String,
    read_only: bool,
    websocket: bool,
    _phantom: PhantomData<E>,
}

impl<E: Environment> AccessorApiBuilder<E> {
    /// The routes under `base_path` (`{base}/q/…`, `info/…`, `log/…`, `describe/…`, `ws/…`).
    pub fn new(base_path: impl Into<String>) -> Self {
        AccessorApiBuilder {
            base_path: base_path.into(),
            read_only: false,
            websocket: true,
            _phantom: PhantomData,
        }
    }

    /// Leave out `PUT {base}/q/…`, so a `PUT` answers axum's own 405.
    pub fn read_only(mut self) -> Self {
        self.read_only = true;
        self
    }

    /// Leave out the WebSocket at `{base}/ws/…`.
    pub fn without_websocket(mut self) -> Self {
        self.websocket = false;
        self
    }

    /// The router.
    pub fn build(self) -> Router<EnvRef<E>> {
        let b = self.base_path.trim_end_matches('/').to_string();
        let mut q = get(handlers::get_handler::<E>);
        if !self.read_only {
            q = q.put(handlers::put_handler::<E>);
        }
        let mut router = Router::new()
            .route(&format!("{b}/q/{{*query}}"), q)
            .route(&format!("{b}/info/{{*query}}"), get(handlers::info_handler::<E>))
            .route(&format!("{b}/log/{{*query}}"), get(handlers::log_handler::<E>))
            .route(
                &format!("{b}/describe/{{*query}}"),
                get(handlers::describe_handler::<E>),
            );
        if self.websocket {
            router = router.route(&format!("{b}/ws/{{*query}}"), get(websocket::ws_handler::<E>));
        }
        router
    }
}
