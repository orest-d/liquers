//! One Liquers server for every host: the standard APIs, the web folder, the index page and a
//! graceful shutdown.
//!
//! The API builders ([`QueryApiBuilder`](crate::QueryApiBuilder) and the others) each produce a
//! router a host composes by hand. This module assembles them under one prefix, adds the web
//! folder and the index redirect ([`crate::web`]), and runs the result in its own accept loop so
//! the server can stop in two stages: stop accepting, let requests finish up to a deadline, then
//! abort the rest. It is what `liquers-py`'s `env.serve()` starts, and what a Rust program starts
//! from the `server` section of a configuration document.
//!
//! # Main components
//!
//! - [`ServerConfig`], [`Api`], [`WebSetting`] (`config.rs`): the `server` section. Defaults:
//!   `127.0.0.1:8080`, prefix `/liquer`, all four APIs, writes enabled, web folder `web` on.
//!   [`ServerConfig::from_document`] takes it from a
//!   [`ConfigDocument`](liquers_core::environment_config::ConfigDocument).
//! - [`LiquersServerBuilder`] (`builder.rs`): `new(prefix)` or `from_config`, the switches
//!   `read_only`, `without_api`, `without_websocket`, `with_query_timeout`, `with_web`,
//!   `without_index`, `with_stop_deadline`; [`LiquersServerBuilder::router`] for a host that runs
//!   its own loop and [`LiquersServerBuilder::bind`] to listen.
//! - [`EnvSource`] (`source.rs`): the environment per request. A fixed `EnvRef` is one; a host
//!   whose environment is rebuilt hands out its current build, and the stated router is cached per
//!   `EnvRef`.
//! - [`LiquersServer`] and [`ServerStop`] (`running.rs`): `local_addr`, `is_running`, `stopped`,
//!   `shutdown(deadline)` (idempotent; the port is free when it returns) and `stop_token`. The
//!   loop serves each connection with hyper-util's connection builder under a graceful watcher;
//!   WebSocket sessions are tracked and receive `Close(1001)`. Dropping the handle keeps the
//!   server running.
//!
//! The APIs mount at `{prefix}/q`, `{prefix}/api/store`, `{prefix}/api/assets`,
//! `{prefix}/api/recipes` and `{prefix}/web`; `read_only` leaves the store API and the assets
//! mutation routes out.
//!
//! # Where it is used
//!
//! - `liquers-py`'s `Environment.serve` (`liquers-py/src/server.rs`), with a source that hands out
//!   the Python environment's current build.
//! - `examples/basic_server.rs`: a Rust program serving a file store.
//! - `tests/server_config_document.rs`: a Rust host configured from one document.
//!
//! Any other host that wants the standard server uses it the same way.
//!
//! # Related concepts
//!
//! - The API builders ([`crate::QueryApiBuilder`], [`crate::AssetsApiBuilder`], …) make one
//!   router each and run nothing; this module composes them and owns the loop.
//! - [`crate::web`] holds the web-folder and index handlers this module mounts.
//! - [`ConfigDocument`](liquers_core::environment_config::ConfigDocument) is the whole document;
//!   [`ServerConfig`] is only its `server` section.
//!
//! The server has no authentication: `host`, `read_only` and `web` are security-sensitive
//! settings (`specs/reference/WEB_API_SPECIFICATION.md` §Server assembly).
//!
//! # Examples
//!
//! ```
//! # use std::time::Duration;
//! use liquers_axum::server::{LiquersServerBuilder, ServerConfig};
//! use liquers_core::{context::{Environment, SimpleEnvironment}, value::Value};
//! # use liquers_core::environment_config::ConfigDocument;
//! # tokio::runtime::Runtime::new().unwrap().block_on(async {
//! let mut doc = ConfigDocument::from_yaml("server: {port: 0, apis: [query, assets]}\n")?;
//! let config = ServerConfig::from_document(&mut doc)?;
//! doc.finish()?;                                    // an unknown section would fail here
//! let envref = SimpleEnvironment::<Value>::new().to_ref();
//! let server = LiquersServerBuilder::from_config(config).bind(envref).await?;
//! assert_ne!(server.local_addr().port(), 0);        // port 0: a free port, reported here
//! server.shutdown(Duration::from_secs(5)).await;    // the port is free again
//! # Ok::<(), liquers_core::error::Error>(()) }).unwrap();
//! ```
//!
//! More: `liquers-axum/tests/server_assembly.rs` (APIs, binding, sources),
//! `tests/server_shutdown.rs` (the two-stage stop), `tests/server_config_document.rs` (a Rust host
//! from one file), and `examples/basic_server.rs`.
mod builder;
mod config;
mod running;
mod source;

pub use builder::LiquersServerBuilder;
pub use config::{Api, ServerConfig, WebSetting};
pub use running::{LiquersServer, ServerStop};
pub use source::EnvSource;
