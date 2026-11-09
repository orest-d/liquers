//! The Liquers HTTP API for axum: query, store, assets and recipes routers, and a server that
//! assembles them.
//!
//! Each API is a builder that produces an axum `Router<EnvRef<E>>`; [`LiquersServerBuilder`] mounts
//! them under one prefix with the store's web folder and runs them with a graceful shutdown.
//!
//! # Main components
//!
//! - [`LiquersServerBuilder`], [`ServerConfig`], [`LiquersServer`] ([`server`]): the assembled
//!   server and its `server` configuration section.
//! - [`QueryApiBuilder`] ([`query`]): `GET/POST {base}/{*query}`, evaluate a query.
//! - [`StoreApiBuilder`] ([`store`]): raw store access.
//! - [`AssetsApiBuilder`] ([`assets`]): assets, their state and WebSocket notifications.
//! - [`RecipesApiBuilder`] ([`recipes`]): the recipes the providers know.
//! - [`web`]: the web folder and the index redirect.
//!
//! # Where it is used
//!
//! - `liquers-py` (`Environment.serve`), through [`LiquersServerBuilder`].
//! - `examples/basic_server.rs`.
//! - The integration tests in `liquers-axum/tests/`, one suite per API and three for the server.
//!
//! # Related concepts
//!
//! - `liquers-core`'s `Environment` and `EnvRef` are what every router is stated with; this crate
//!   adds HTTP and nothing to evaluation.
//! - `liquers-web` exposes the same environment to JavaScript in a browser, without HTTP.
//!
//! # Examples
//!
//! ```
//! use liquers_axum::LiquersServerBuilder;
//! use liquers_core::{context::{Environment, SimpleEnvironment}, value::Value};
//!
//! # let runtime = tokio::runtime::Runtime::new().unwrap();
//! # let _inside = runtime.enter();   // an environment starts its manager on a runtime
//! let envref = SimpleEnvironment::<Value>::new().to_ref();
//! let app: axum::Router = LiquersServerBuilder::<SimpleEnvironment<Value>>::new("/liquer")
//!     .router()
//!     .with_state(envref);     // serve it with `axum::serve`, or `bind` the builder instead
//! # drop(app);
//! ```
//!
//! More: `examples/basic_server.rs`, `specs/guides/WEB_API_GUIDE.md`, and the suites in `tests/`.

pub mod api_core;
pub mod assets;
pub mod axum_integration;
pub mod query;
pub mod recipes;
pub mod server;
pub mod store;
pub mod web;

// Re-exports for convenience
pub use api_core::{ApiResponse, BinaryResponse, DataEntry, ErrorDetail, SerializationFormat};
pub use assets::AssetsApiBuilder;
pub use query::QueryApiBuilder;
pub use recipes::RecipesApiBuilder;
pub use server::{Api, EnvSource, LiquersServer, LiquersServerBuilder, ServerConfig, ServerStop, WebSetting};
pub use store::StoreApiBuilder;

// Re-export EnvRef from liquers-core
pub use liquers_core::context::EnvRef;
