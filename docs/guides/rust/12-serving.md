---
title: Serving and embedding
status: ready
sources: [guides/WEB_API_GUIDE.md, reference/WEB_API_SPECIFICATION.md, reference/ENVIRONMENT_CONFIG.md]
reviewed: 2026-11-10
---
# 12. Serving and embedding

PURPOSE: The reader can make Liquers available to other programs: over HTTP, or embedded in an application.

A Liquers environment can answer other programs over HTTP: a browser, a script with `curl`, a
service in another language. The `liquers-axum` crate holds the standard server. You give it an
environment and its settings, and it serves queries, the store, assets and recipes under one path,
plus a folder of web pages.

## 12.1 Starting the standard server

Add `liquers-axum` to your dependencies, and `tokio` with the feature `signal` for the Ctrl-C
handling of this example:

```rust
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
```

`bind` listens and returns at once; the server runs on the tokio runtime until you stop it, and
dropping the `LiquersServer` handle does not stop it. Open
`http://127.0.0.1:8080/liquer/q/<query>` to evaluate a query. `ServerConfig::default()` is the
default settings of 12.2; set fields to change them, for example
`ServerConfig { port: 3000, ..Default::default() }`.

The example builds the smallest environment `liquers-core` offers, with a directory as its
store. The server works with any environment. In a real program you pass the one you built in
chapter 8, with your commands and the library's: `.bind(library_environment(store)?)`.

## 12.2 Configuring the server from a file

The server's settings are the `server` section of the same configuration file that sets up the
store, recipes and assets ([8. The environment](08-environment.md)). Read the file, take the
section, check that nothing is left over, and bind:

```rust
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
```

The example reads the document from text, binds to a free port and then acts as its own client,
which is also how you test a server (chapter 14).

{{> ../_shared/serving.md}}

## 12.3 What a server offers

| Interface | Path with the default prefix | What it does |
|---|---|---|
| Query API | `/liquer/q/<query>` | evaluates a query and returns the value |
| Store API | `/liquer/api/store/…` | reads and writes files as they are stored |
| Assets API | `/liquer/api/assets/…` | evaluates, watches and manages results; submits long computations |
| Recipes API | `/liquer/api/recipes/…` | reads the recipes |

A query goes into the URL as it is. This is the reason queries are written with URL-safe
characters only. A client that builds a query from values encodes it as section 3.9 shows, and
needs no further URL quoting.

**Responses.** A route that returns data returns the bytes themselves, with the media type in
`Content-Type` and the status of the asset in the header `X-Liquers-Status`. The file name at
the end of the query chooses the format. Without one the value is sent as
`application/octet-stream`, so end a query for a server with a file name. Every other route
returns a JSON envelope with a `status` of `OK` or `ERROR`, a `message` and a `result`.

**Errors.** A failed request has an HTTP error status and a JSON body whose `error.type` is one
of the types of chapter 15. The usual codes are 400 for a malformed query, 404 for something that
does not exist, 409 for an operation that does not fit the state of the asset, 422 for a value
that cannot be converted to the requested format, and 500 for a failing command.

**Long evaluations.** A request to the Query API waits for the result, up to a limit that the
server sets, 30 seconds by default (`with_query_timeout`). For anything longer, submit the query
through the Assets API, which answers at once, and then ask for its status or follow it over the
WebSocket (section 12.6).

## 12.4 Inside your own application

The builder can also start from a prefix and be adjusted in code: `read_only()`,
`without_api(Api::Recipes)`, `without_websocket()`, `with_query_timeout(…)`, `with_web(…)`,
`without_index()`. When your program already runs an Axum application, `router()` returns the
routes as an `axum::Router` to merge into it, and you serve it yourself:

```rust
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
```

The router's state is the `EnvRef`. Your own handlers can use the same environment: evaluate a
query, read the store, or do something that is not a query at all. Authentication, rate limits
and logging are layers of your Axum application; the Liquers routes are ordinary routes under
them.

## 12.5 Stopping the server

`server.shutdown(deadline)` stops in two steps. The server stops accepting connections at once;
requests already running get up to `deadline` to finish, and are cut off after it. A client
following an asset over the WebSocket receives a close message saying the server is shutting down.
When `shutdown` returns, the port is free for another server. Calling it again does nothing.

`server.stopped().await` waits until the server has stopped, which suits a task that only needs to
know. `server.stop_token()` gives a `CancellationToken` that stops the server when cancelled, for
code that has no access to the handle.

## 12.6 Live updates

A client that wants to follow an evaluation does not have to ask again and again. The Assets API
has a WebSocket, at `{prefix}/api/assets/ws/q` for queries and `…/ws/key` for keys. The client
sends JSON messages:

```json
{"action": "subscribe", "query": "-R/data/sales.csv/-/ns-rec/to_record/nrows"}
{"action": "subscribe", "key": "data/eu.csv"}
{"action": "unsubscribe", "query": "-R/data/sales.csv/-/ns-rec/to_record/nrows"}
{"action": "cancel", "subscription_id": "9d538916-…"}
{"action": "ping"}
```

Subscribing asks for the asset, as `evaluate` does, so it also starts the evaluation. The server
then sends a notification for each change. Each has a `type`, one of the
`AssetNotificationMessage` kinds of section 7.5 (`JobStarted`, `StatusChanged`, `LogMessage`,
`PrimaryProgressUpdated`, `JobFinished`, …), and an `info` object with the state of the asset at
that moment.

Rely on `info.status` and not on the order of the types: changes that follow each other quickly
can be merged into one notification. A subscription follows one asset. It ends when the asset
ends in `Error`, `Cancelled` or `Expired`, or is removed or replaced; subscribe again to follow
its successor. Notifications are a convenience and not a guarantee. For a definitive answer, ask
the Assets API for the asset's info.

A client in Rust is the example `websocket_client` of `liquers-axum`.

## 12.7 Embedding without a server

Nothing in this guide before this chapter needed a network. An application that only wants
named, cached, traceable results, a command-line tool, a desktop program, a batch job, builds an
environment and evaluates queries, as chapters 1 to 10 show. Two points are worth repeating for
that case:

- Build the environment once and keep the `EnvRef`. The cache lives in it.
- Point the store at durable storage (chapter 6), and results survive the process: the next run
  finds them, checks that their inputs have not changed, and reuses them.

Several programs can share one store, the same folder or the same bucket. Each then reads what
the others have written, as files with their metadata. A Rust server can serve results that a
Python program computed, although it cannot run the Python functions. Each process keeps its own
view of what it has read, and notices a change made by another process when it next reads the
file (chapter 7).

The same in Python: [12. Serving and embedding](../python/12-serving.md).
