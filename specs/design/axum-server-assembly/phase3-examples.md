# Phase 3: Examples & Use-cases — axum-server-assembly

## Overview Table

| # | Kind | Name | Shows / checks | Scenarios |
|---|---|---|---|---|
| 1 | Example | A web application from the store | `web/` mount, `/` redirect, folders, content types | AC-1, AC-2, AC-3, AC-5 |
| 2 | Example | The server configured in code | builder switches, no file | AC-7, AC-8 |
| 3 | Example | The server from one file | `ConfigDocument` → `ServerConfig` | AC-7, AC-8 |
| 4 | Example | Embedding the router | `router()` in a host's own axum app | AC-7 |
| 5 | Example | Graceful shutdown | Ctrl-C, deadline, WebSocket Close | AC-9 |
| 6 | Test | `liquers-axum/tests/server_web.rs` | the web and index routes | AC-1, AC-2, AC-3, AC-5, AC-6 |
| 7 | Test | `liquers-axum/tests/server_assembly.rs` | APIs, binding, configuration errors | AC-7, AC-8 |
| 8 | Test | `liquers-axum/tests/server_shutdown.rs` | drain, abort at the deadline, port freed | AC-9 |
| 9 | Test | `liquers-axum/src/server/config.rs` `tests` | serde of `ServerConfig` and `WebSetting` | AC-8 |

## Example 1: a web application from the store

The store has a read-only `web` mount over `./site` (`index.html`, `app.js`, `docs/index.html`).
A browser opening the server's root lands on the app.

```text
GET /                       → 307  Location: /liquer/web/
GET /liquer/web             → 308  Location: /liquer/web/
GET /liquer/web/            → 200  text/html          ./site/index.html   ETag: "…"
GET /liquer/web/app.js      → 200  text/javascript    nosniff, Cache-Control: no-cache
GET /liquer/web/docs        → 308  Location: /liquer/web/docs/
GET /liquer/web/docs/       → 200  text/html          ./site/docs/index.html
GET /liquer/web/app.js      If-None-Match: "…"   → 304, no body
GET /liquer/web/missing.js  → 404  (a recipe for it is not run: Decision 1)
HEAD /liquer/web/app.js     → 200, headers only
```

The filesystem mount reports no media type, so the type comes from the extension (AC-5).

## Example 2: the server configured in code

```rust
let server = LiquersServerBuilder::<MyEnv>::new("/api")
    .without_api(Api::Recipes)
    .read_only()                                   // store API left out, assets mutations refused
    .with_web(WebSetting::Root("site".into()))     // GET /api/web/… reads store key site/…
    .with_query_timeout(Duration::from_secs(30))
    .bind(envref.clone())                          // a fixed EnvRef is an EnvSource
    .await?;
eprintln!("listening on {}", server.local_addr());
```

`ServerConfig { host: "0.0.0.0".into(), port: 0, ..Default::default() }` passed to `from_config`
does the same from a value; port 0 binds an ephemeral port, which `local_addr()` reports (AC-8).

## Example 3: the server from one file

The program of Phase 2 §Integration Points: `ConfigDocument::load`, `ServerConfig::from_document`,
`finish`, then `from_config(server).bind(…)`. With

```yaml
server: {port: 8000, apis: [query, assets], read_only: true, web: false}
```

there is no store or recipes route, every assets mutation is refused, and neither `/` nor
`/liquer/web/` exists (`web: false` turns the index redirect off too). A section with
`prot: 8000` fails at `from_document` with `ConfigurationError` naming `prot` (AC-8).

## Example 4: embedding the router

```rust
let liquers = LiquersServerBuilder::<MyEnv>::new("/liquer").router().with_state(envref);
let app = Router::new().route("/health", get(|| async { "ok" })).merge(liquers);
axum::serve(listener, app).await?;    // the host's own loop: no graceful deadline from us
```

## Example 5: graceful shutdown

```rust
tokio::signal::ctrl_c().await.ok();
server.shutdown(Duration::from_secs(5)).await;     // returns with the port free
```

A request still running after 5 s is aborted (its evaluation is not cancelled); an open WebSocket
receives `Close(1001)` first. `stop_token().cancel()` starts the same stop with the builder's
default deadline from code that does not hold the server.

## Edge and Error Cases

| Symptom | Expected behaviour | Test |
|---|---|---|
| port already in use | `bind` returns `Error` (General) naming the address; nothing listens | `bind_port_in_use_is_an_error` |
| `prefix: /` | routes under `/web/…`, not `//web` | `root_prefix_has_no_double_slash` |
| `GET {prefix}/web/a~b.js` (not a valid key) | 404 | `invalid_key_is_404` |
| metadata media type not a valid header value | 500 from `build_or_500`, no panic | `bad_media_type_is_500` |
| `data_facts` unsupported by the store | no `Last-Modified`, still 200 | `no_data_facts_no_last_modified` |
| `If-None-Match: *`, a list, a `W/` tag | 304 on a match | `if_none_match_forms` |
| the source fails (a host's rebuild) | 503 with the error detail | `failing_source_is_503` |
| the source returns a new `EnvRef` | later requests use it; the router is re-stated once | `new_envref_is_used` |
| dropped `LiquersServer` | still serving; `stop_token` stops it | `dropped_handle_keeps_serving` |
| `shutdown` called twice | the second returns at once | `shutdown_is_idempotent` |

## Test Plan

- Unit: `liquers-axum/src/server/config.rs` `tests`
  - `web_setting_forms` (AC-8): `true`, `false`, `site`, absent; each serializes back to its form.
  - `unknown_key_is_refused` (AC-8); `defaults_match_the_documented_ones` (AC-8).
- Integration: `liquers-axum/tests/server_web.rs` (memory store with `web/…` keys)
  - `stored_file_is_served_with_its_media_type` (AC-1); `missing_file_is_404` (AC-1).
  - `folder_with_slash_serves_index_and_without_redirects` (AC-2).
  - `root_and_index_html_redirect_to_web` (AC-3); `web_false_removes_index_redirect` (AC-3).
  - `extension_gives_the_type_without_metadata` (AC-5): `index.html`, `app.js`, `app.wasm`.
  - `etag_round_trip_gives_304_then_200_after_change` (AC-6); plus the web rows above.
- Integration: `liquers-axum/tests/server_assembly.rs`
  - `config_without_store_api_and_read_only` (AC-7); `bind_port_zero_reports_port` (AC-8);
    `bind_port_in_use_is_an_error` (AC-8); the source and handle rows above.
- Integration: `liquers-axum/tests/server_shutdown.rs` (a `slow` command waiting on a gate)
  - `slow_request_completes_within_deadline` (AC-9); `slow_request_is_aborted_at_deadline` (AC-9);
  - `websocket_gets_close_frame` (AC-9); `new_connections_refused_and_port_free` (AC-9).
- Command: `cargo test -p liquers-axum --lib server --test server_web --test server_assembly
  --test server_shutdown` (needs `libssl-dev`, as every `liquers-axum` test run).

## Learning Log

- `file_extension_to_media_type` already maps `html`, `js` (`text/javascript`), `css`, `svg` and
  `wasm` (`liquers-core/src/media_type.rs`); no table is added.
- The existing suites start servers with a bare `axum::serve` on port 0
  (`tests/assets_websocket.rs` `start_server`); the new suites use `LiquersServer` instead.
