---
title: Serving and embedding
status: draft
sources: [guides/WEB_API_GUIDE.md, reference/WEB_API_SPECIFICATION.md, reference/ENVIRONMENT_CONFIG.md]
reviewed:
---
# 12. Serving and embedding

PURPOSE: The reader can make Liquers available to other programs: over HTTP, or embedded in an application.

A Liquers environment can answer other programs over HTTP: a browser, a script with `curl`, a
service in another language. The `liquers-axum` crate holds the standard server. You give it an
environment and its settings, and it serves queries, the store, assets and recipes under one path,
plus a folder of web pages.

## 12.1 Starting the standard server

```rust
use std::time::Duration;
use liquers_axum::{LiquersServerBuilder, ServerConfig};
use liquers_core::{context::{Environment, SimpleEnvironment}, query::Key,
                   store::AsyncFileStore, value::Value};

#[tokio::main]
async fn main() -> Result<(), liquers_core::error::Error> {
    let mut env = SimpleEnvironment::<Value>::new();
    env.with_async_store(Box::new(AsyncFileStore::new("./data", &Key::new())));
    let server = LiquersServerBuilder::from_config(ServerConfig::default())
        .bind(env.to_ref())
        .await?;
    eprintln!("listening on http://{}", server.local_addr());   // http://127.0.0.1:8080
    let _ = tokio::signal::ctrl_c().await;
    server.shutdown(Duration::from_secs(5)).await;
    Ok(())
}
```

`bind` listens and returns at once; the server runs on the tokio runtime until you stop it, and
dropping the `LiquersServer` handle does not stop it. Open
`http://127.0.0.1:8080/liquer/q/<query>` to evaluate a query. `ServerConfig::default()` is the
default settings of 12.2; set fields to change them, for example
`ServerConfig { port: 3000, ..Default::default() }`.

The builder can also start from a prefix (`LiquersServerBuilder::new("/liquer")`) and be adjusted
in code: `read_only()`, `without_api(Api::Recipes)`, `without_websocket()`,
`with_query_timeout(…)`, `with_web(…)`, `without_index()`. When your program already runs an axum
application, `router()` returns the routes as an `axum::Router` to merge into it, and you serve it
yourself.

## 12.2 Configuring the server from a file

The server's settings are the `server` section of the same configuration file that sets up the
store, recipes and assets ([8. The environment](08-environment.md)). Read the file, take the
section, and check that nothing is left over:

```rust
use liquers_axum::{LiquersServerBuilder, ServerConfig};
use liquers_core::environment_config::ConfigDocument;

let mut doc = ConfigDocument::load(std::path::Path::new("liquers.yaml"))?;
let server_config = ServerConfig::from_document(&mut doc)?;
let environment = doc.finish()?;     // an unknown section, such as `sever:`, is an error here
let envref = /* an EnvironmentBuilder with_config(environment, …), commands, build() */;
let server = LiquersServerBuilder::from_config(server_config).bind(envref).await?;
```

{{> ../_shared/serving.md}}

## 12.3 Stopping the server

`server.shutdown(deadline)` stops in two steps. The server stops accepting connections at once;
requests already running get up to `deadline` to finish, and are cut off after it. A client
following an asset over the WebSocket receives a close message saying the server is shutting down.
When `shutdown` returns, the port is free for another server. Calling it again does nothing.

`server.stopped().await` waits until the server has stopped, which suits a task that only needs to
know. `server.stop_token()` gives a `CancellationToken` that stops the server when cancelled, for
code that has no access to the handle.

## 12.4 Live updates

TBD: 12.4 Live updates: the WebSocket event stream.
SOURCE: guides/WEB_API_GUIDE.md §8

The same in Python: [12. Serving and embedding](../python/12-serving.md).
