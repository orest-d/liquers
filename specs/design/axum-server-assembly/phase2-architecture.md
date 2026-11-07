# Phase 2: Solution & Architecture — axum-server-assembly

## Overview

A new `server` module assembles the four builders, a new `web` module and the index routes into one
`Router<EnvRef<E>>`, states it per request from an `EnvSource` (a fixed `EnvRef`, or a host's
current build), and runs it in its own accept loop: hyper-util's connection builder per socket in a
`JoinSet`, upgraded WebSocket sessions in a `TaskTracker`, a graceful drain, and an abort at the
deadline. Rejected: `axum::serve(…).with_graceful_shutdown` (no deadline); `tower_http`'s `ServeDir`
(filesystem only); `with_state` per request (the stated router is cached per `EnvRef`). The source
refines a Phase 1 non-goal (recorded there): the server rebuilds nothing, but a host whose
environment is rebuilt (liquers-py) hands it a source instead of one `EnvRef`.

## Known-Issue Preflight

| Issue | Status | Priority | Impact on this design | Fix first? | Blocking? | Action |
|---|---|---|---|---|---|---|
| `AXUM-WEBSOCKET-HARDENING` | accepted | P3 | same session code as the Close frame | no | no | keep the token apart from its limits |
| `ASSETS-IMPROVEMENTS` (body limit) | accepted | P2 | its limit becomes a builder setting | no | no | `http-request-body-limit` |
| `KEY-NAMES-CANNOT-HOLD-COMMON-WEB-FILENAMES` | draft | P3 | such paths are 404 | no | no | documented |
| `CORE-SESSION-AND-KEY-ACL` | accepted | P2 | no authentication | no | no | security guideline |
| `SECURITY-SENSITIVE-SETTINGS-ARE-NOT-DOCUMENTED` | draft | P2 | question 6's risks | no | no | docs design, with Phase 5 |
| `WEB-FOLDER-SERVES-STORED-FILES-ONLY`, `INDEX-PAGE-IS-A-FIXED-REDIRECT` | draft | P3 | later `web` and index settings | no | no | separable handlers |
| `ERROR-TYPES-TOO-COARSE-AND-GENERAL-OVERUSED` | draft | P2 | bind errors are `General` | no | no | none now |
| `REGISTRIES-NOT-PUBLISHED-ACROSS-REALMS` | draft | P3 | a fifth API | no | no | `Api` is `#[non_exhaustive]` |
| `ASSET-MANAGER-GET-PRINTS-DEBUG-OUTPUT` | draft | P3 | stderr noise per request | no | no | its own PR |

## Interfaces

```rust
// server/config.rs — sync (pure data)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ServerConfig {
    #[serde(default = "default_host")] pub host: String,       // 127.0.0.1
    #[serde(default = "default_port")] pub port: u16,          // 8080
    #[serde(default = "default_prefix")] pub prefix: String,   // /liquer; a trailing `/` is stripped
    #[serde(default = "all_apis")] pub apis: Vec<Api>,         // `apis: [query, assets]`
    #[serde(default)] pub read_only: bool,
    #[serde(default)] pub web: WebSetting,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")] #[non_exhaustive]
pub enum Api { Query, Store, Assets, Recipes }
/// `web: true` or absent → root `web`; `web: false` → off (and the index redirect too);
/// `web: site` → root `site`. Serializes back to the same form.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum WebSetting { Enabled(bool), Root(String) }           // Default: Enabled(true)
impl ServerConfig {
    /// Takes the `server` section of core's `ConfigDocument` (one file for every section,
    /// `liquers-py-web-server` §Configuration Document); absent → defaults.
    pub fn from_document(doc: &mut ConfigDocument) -> Result<Self, Error>;
}

// server/source.rs — async: a host may need a blocking rebuild
#[async_trait]
pub trait EnvSource<E: Environment>: Send + Sync + 'static {
    async fn current(&self) -> Result<EnvRef<E>, Error>;
}
impl<E: Environment> EnvSource<E> for EnvRef<E> { /* a clone */ }

// server/builder.rs — sync builder, async bind (I/O)
impl<E: Environment> LiquersServerBuilder<E> {
    pub fn new(prefix: impl Into<String>) -> Self;
    pub fn from_config(config: ServerConfig) -> Self;
    pub fn read_only(self) -> Self;
    pub fn without_api(self, api: Api) -> Self;
    pub fn without_websocket(self) -> Self;
    pub fn with_query_timeout(self, timeout: Duration) -> Self;
    pub fn with_web(self, web: WebSetting) -> Self;
    pub fn with_stop_deadline(self, deadline: Duration) -> Self; // default 5 s, for stop_token()
    pub fn router(&self) -> Router<EnvRef<E>>;                  // hosts that serve it themselves
    pub async fn bind(self, source: impl EnvSource<E>) -> Result<LiquersServer, Error>;
}

// server/running.rs
pub struct LiquersServer {
    addr: SocketAddr,
    stop: CancellationToken,
    deadline: tokio::sync::watch::Sender<Duration>, // read by the loop when `stop` fires
    finished: CancellationToken,                    // cancelled when the loop has released all
}
impl LiquersServer {
    pub fn local_addr(&self) -> SocketAddr;
    pub fn is_running(&self) -> bool;
    pub async fn stopped(&self);                    // awaits `finished`
    pub async fn shutdown(&self, deadline: Duration); // sets deadline, cancels stop, awaits finished
    pub fn stop_token(&self) -> CancellationToken;  // stop from elsewhere, with the default deadline
}

// web/mod.rs — async handlers
pub fn web_routes<E: Environment>(prefix: &str, root: Key) -> Router<EnvRef<E>>;
pub fn index_routes<E: Environment>(prefix: &str) -> Router<EnvRef<E>>;
// axum_integration.rs — shared with BinaryResponse
pub(crate) fn content_type_for(metadata: &Metadata, key: &Key) -> String;
```

`shutdown` takes `&self` and is idempotent. `read_only` keeps today's meaning per API:
`AssetsApiBuilder::read_only`, and the store API left out. Commands, ExtValue variants: none.

## Integration Points

- New `liquers-axum/src/server/{mod,config,source,builder,running}.rs`, `src/web/mod.rs`; `lib.rs`
  re-exports them. `Cargo.toml`: `hyper-util` (`server-auto`, `server-graceful`, `tokio`), `hyper`,
  `tokio-util` (`rt`); all in the lock file.
- `assets/websocket.rs`: the writer channel (line 797) carries `enum Outgoing { Notify(…),
  Close(CloseFrame) }`. `handle_socket` (line 788) reads `Option<Extension<ServerStop>>` (token and
  tracker), holds `tracker.token()` for its life, and on cancel sends `Close(1001)`, drops the
  sender and awaits the writer for up to 1 s instead of aborting it (line 827). The subscription
  tasks (line 636) are aborted as on disconnect. Without a server nothing changes.
- `axum_integration.rs`: `BinaryResponse` uses `content_type_for`; the web handler uses
  `build_or_500` (line 15) and `api_core::error::{error_to_status_code, error_to_detail}`.
- `examples/basic_server.rs`: the builder, Ctrl-C → `shutdown(5 s)`.

A Rust host reads one file, takes `server`, builds the environment from the rest and binds:

```rust
let mut doc = ConfigDocument::load(Path::new("liquers.yaml"))?;
let server = ServerConfig::from_document(&mut doc)?;
let (environment, _) = doc.finish()?;
let mut builder = default_environment_builder::<Value, ()>()
    .with_config(environment, Box::new(default_store_factory()));
register_my_commands(&mut builder.command_registry)?;
let running = LiquersServerBuilder::from_config(server).bind(builder.build()?).await?;
tokio::signal::ctrl_c().await.ok();
running.shutdown(Duration::from_secs(5)).await;
```

Or with no file: `LiquersServerBuilder::new("/api").read_only().without_api(Api::Recipes)
.bind(envref).await?`.

## Web Endpoints

| Route (GET; axum answers HEAD from it) | Response |
|---|---|
| `/`, `/index.html` (only when `web` is on) | 307 `Location: {prefix}/web/` |
| `{prefix}/web` | 308 to `{prefix}/web/` |
| `{prefix}/web/`, `{prefix}/web/{*path}` | `<root>/{path}`, a folder's `index.html` with a trailing `/`, 308 to the slash form without |

`web_file` parses `<root>/{path}` (invalid → 404), checks `store.is_dir`, then `store.get(key)` for
bytes and metadata, as `get_data_handler` does. Errors map with `error_to_status_code`
(`KeyNotFound`, `KeyNotSupported` → 404). Content type: the metadata's unless empty or
`application/octet-stream`, else `file_extension_to_media_type`; `nosniff`. `ETag` is
`Version::from_content`; `If-None-Match` matches a list, `*` and `W/` tags → 304. `Last-Modified`
when `store.data_facts` reports `modified` (an RFC 3339 `BackendTime`, reformatted as an HTTP date
with chrono); an `Err` from `data_facts` counts as none. `Cache-Control: no-cache` (new in Phase 2).

## Concurrency

One loop task `select!`s on `accept()`, `join_next()` (reaping finished connections) and
`stop.cancelled()`. Each connection is served by `auto::Builder::serve_connection_with_upgrades`
and watched by `GracefulShutdown`. Per request the service calls `source.current()`, compares it
with the cached `(EnvRef, Router)` in an `ArcSwap` by `Arc::ptr_eq`, re-states the router on a
change, and inserts `ServerStop` into the request's extensions; no lock is held across an `.await`.
A WebSocket keeps the build it upgraded with. On `stop`: accepting ends and the listener drops (the
port is free), the graceful watcher and the WebSocket token fire, the loop waits for the `JoinSet`
and `tracker.wait()` together up to the deadline, then aborts what is left and awaits it, and
cancels `finished`. An accept error is logged and the loop sleeps 100 ms. Dropping
`LiquersServer` detaches the task without stopping it (a Phase 2 addition, recorded in Phase 1).

## Error Handling

| Failure | Constructor |
|---|---|
| bind fails (in use, permission, bad host) | `Error::from_error(ErrorType::General, io_error)` naming the address |
| unknown key, wrong type, bad `prefix` or `web` key | `Error::configuration_error(…)` |
| `source.current()` fails | 503 with `error_to_detail` |
| a store error | `error_to_status_code` with `error_to_detail` |

## Relevant Commands

None; no namespace is involved.

## Documentation Architecture

| Path | Kind | Change |
|---|---|---|
| `specs/reference/WEB_API_SPECIFICATION.md` | reference | §Server assembly, §Web folder, §Index page |
| `specs/guides/WEB_API_GUIDE.md` | guide | "Start a server", "Serve a web application from the store" |
| `liquers-axum/README.md` | readme | builder example |
| `docs/guides/{rust,web}/12-serving.md` | user guide | web folder, index, read-only `web` mount |
| `liquers-axum/src/server/mod.rs`, `src/web/mod.rs` | API doc | all five parts; submodules documented in `mod.rs` |
| `liquers-axum/src/lib.rs`, `src/assets/websocket.rs` | API doc | components: the server; the stop token |

`affects_docs` lists both chapters. `specs/README.md` keeps its link. Question 6's risks go to the
security guideline (`SECURITY-SENSITIVE-SETTINGS-ARE-NOT-DOCUMENTED`).

## Risks

| Assessment | Finding |
|---|---|
| Files likely to change | the new modules, `assets/websocket.rs`, `axum_integration.rs`, `lib.rs`, `Cargo.toml`, the example |
| Crates and workflows affected | `liquers-axum`; its CI job (its tests need `libssl-dev`, unchanged) |
| Existing tests likely to change | none: WebSocket tests run without a server. Recovery: revert the PR |
| New validation | `tests/server.rs` (AC-1–3, AC-5–9): ephemeral port, reqwest, tokio-tungstenite |
| Compatibility / data / concurrency / performance / security | additive; one `ArcSwap` load per request; files are buffered whole (streaming is a non-goal); stored XSS documented |
| Certainty and open questions | high for routes; medium for the Close frame under `TaskTracker`, which AC-9's test checks first |
