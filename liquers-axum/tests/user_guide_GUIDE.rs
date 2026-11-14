//! The examples of chapter 12 of the Rust user guide (`docs/guides/rust/12-serving.md`),
//! compiled and, where they do not wait for Ctrl-C, run.
//!
//! Every ```rust block of the guide is a contiguous slice of this file or of
//! `liquers-lib/tests/user_guide_GUIDE.rs`; `liquers-docs/tests/rust_guide_examples.rs` checks
//! that. Change an example here and in the chapter together.
#![allow(dead_code, unused_imports)]

/// Section 12.1: a complete program. Compiled only: it serves until Ctrl-C.
mod ch12_server {
use std::time::Duration;

use liquers_axum::{LiquersServerBuilder, ServerConfig};
use liquers_core::{
    context::{Environment, SimpleEnvironment},
    error::Error,
    query::Key,
    store::AsyncFileStore,
    value::Value,
};

#[tokio::main]
async fn main() -> Result<(), Error> {
    let mut env = SimpleEnvironment::<Value>::new();
    env.with_async_store(Box::new(AsyncFileStore::new("./data", &Key::new())));
    let server = LiquersServerBuilder::from_config(ServerConfig::default())
        .bind(env.to_ref())
        .await?;
    eprintln!("listening on http://{}", server.local_addr()); // http://127.0.0.1:8080
    let _ = tokio::signal::ctrl_c().await;
    server.shutdown(Duration::from_secs(5)).await;
    Ok(())
}
}

use std::time::Duration;

use liquers_axum::{Api, LiquersServerBuilder, ServerConfig};
use liquers_core::{
    command_metadata::CommandKey,
    context::{Environment, GenericEnvironment},
    environment_builder::{EnvironmentBuilder, Queued},
    environment_config::ConfigDocument,
    error::{Error, ErrorType},
    store_factory::default_store_factory,
    value::Value,
};

type Env = GenericEnvironment<Value, (), Queued>;

#[tokio::test]
async fn ch12_requests() -> Result<(), Error> {
    let http = |e: reqwest::Error| Error::from_error(ErrorType::General, e);

    let mut doc = ConfigDocument::from_yaml(
        "
store:
  stores:
    - {type: memory, prefix: data}
server:
  port: 0            # a free port
  read_only: true
",
    )?;
    let server_config = ServerConfig::from_document(&mut doc)?; // the `server` section
    let config = doc.finish()?; // an unknown section, such as `sever:`, is an error here

    let mut builder =
        EnvironmentBuilder::<Value>::new().with_config(config, Box::new(default_store_factory()));
    builder
        .command_registry
        .register_command(CommandKey::new_name("hello"), |_, _, _| Ok(Value::from("world")))?;
    let env = builder.build()?;

    let server = LiquersServerBuilder::<Env>::from_config(server_config).bind(env).await?;
    let base = format!("http://{}/liquer", server.local_addr());

    // The query API: the value itself, with its media type.
    let response = reqwest::get(format!("{base}/q/hello/greeting.txt")).await.map_err(http)?;
    assert_eq!(response.headers()["content-type"], "text/plain");
    assert_eq!(response.text().await.map_err(http)?, "world");

    // A failing query: an HTTP error status and a JSON description.
    let response = reqwest::get(format!("{base}/q/no_such_command")).await.map_err(http)?;
    assert!(response.status().is_client_error() || response.status().is_server_error());
    let body: serde_json::Value = response.json().await.map_err(http)?;
    assert_eq!(body["error"]["type"], "ActionNotRegistered");

    server.shutdown(Duration::from_secs(1)).await;
    Ok(())
}

#[tokio::test]
async fn ch12_router() -> Result<(), Error> {
    use axum::{routing::get, Router};

    let mut builder = EnvironmentBuilder::<Value>::new();
    builder
        .command_registry
        .register_command(CommandKey::new_name("hello"), |_, _, _| Ok(Value::from("world")))?;
    let env = builder.build()?;

    // The Liquers routes under /data, next to routes of your own.
    let liquers = LiquersServerBuilder::<Env>::new("/data")
        .read_only()
        .without_api(Api::Recipes)
        .router();
    let app: Router = Router::new()
        .route("/health", get(|| async { "ok" }))
        .merge(liquers.with_state(env));

    let io = |e: std::io::Error| Error::from_error(ErrorType::General, e);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.map_err(io)?;
    let address = listener.local_addr().map_err(io)?;
    tokio::spawn(async move { axum::serve(listener, app).await });

    let http = |e: reqwest::Error| Error::from_error(ErrorType::General, e);
    let text = reqwest::get(format!("http://{address}/data/q/hello")).await.map_err(http)?;
    assert_eq!(text.text().await.map_err(http)?, "world");
    Ok(())
}
