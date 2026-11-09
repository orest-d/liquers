# Phase 4: Implementation Plan — axum-server-assembly

## Overview

The server is built bottom-up in `liquers-axum`: settings, the web route, the assembled router, the
accept loop, then the WebSocket stop. It lands in commit group A on branch
`claude/project-thread-5j5pvg` (PR #264), after `configuration-document` Step 1 (the core
`ConfigDocument` its `ServerConfig::from_document` reads). Tests need `libssl-dev` (the existing
`reqwest` dev-dependency). No issue must be fixed first.

## Progress

- [ ] Step 1: dependencies and `ServerConfig`
- [ ] Step 2: the web and index routes
- [ ] Step 3: `EnvSource` and the assembled router
- [ ] Step 4: the accept loop, `bind` and `LiquersServer`
- [ ] Step 5: WebSocket stop and the Close frame
- [ ] Step 6: `server_config_document.rs` and the example
- [ ] Step 7: documentation and module docs

## Implementation Steps

### Step 1: dependencies and `ServerConfig`

- Files / symbols: `liquers-axum/Cargo.toml` (`hyper-util` with `server-auto`,
  `server-graceful`, `tokio`; `hyper`; `tokio-util` with `rt`; `arc-swap`); new
  `src/server/{mod,config}.rs` — `ServerConfig`, `Api`, `WebSetting`, `impl Default` for both,
  `from_document`.
- Change: as Phase 2 Interfaces; prefix normalized (trailing `/` stripped, a leading `/`
  required); `web` root parsed as a `Key`.
- Depends on: `configuration-document` Step 1.
- Proof: `cargo test -p liquers-axum --lib server::` — `web_setting_forms`,
  `unknown_key_is_refused`, `defaults_match_the_documented_ones`, `bad_prefix_or_web_key_is_refused`.
- Rollback: revert; nothing else uses the module.

### Step 2: the web and index routes

- Files / symbols: new `src/web/mod.rs` — `web_routes`, `index_routes`, `web_file`,
  `content_type_for`, the ETag and `If-None-Match` helpers.
- Change: Phase 2 §Web Endpoints; errors through `error_to_status_code` / `error_to_detail`,
  responses through `build_or_500`.
- Depends on: Step 1 (for `WebSetting` only).
- Proof: `cargo test -p liquers-axum --test server_web` (routers stated with a memory-store
  environment, served by `axum::serve` until Step 4 exists, then by `LiquersServer`): every test of
  Phase 3's `server_web.rs` group.
- Rollback: revert; the routes are not mounted anywhere yet.

### Step 3: `EnvSource` and the assembled router

- Files / symbols: new `src/server/{source,builder}.rs` — `EnvSource`, its impl for `EnvRef`,
  `LiquersServerBuilder` and its switches, `router()`.
- Change: `router()` merges `QueryApiBuilder` (`{prefix}/q`), `StoreApiBuilder`
  (`{prefix}/api/store`), `AssetsApiBuilder` (`{prefix}/api/assets`, `read_only` and
  `without_websocket` passed on), `RecipesApiBuilder` (`{prefix}/api/recipes`), `web_routes` and
  `index_routes` per the settings; `read_only` leaves the store API out.
- Depends on: Steps 1–2.
- Proof: `cargo test -p liquers-axum --test server_assembly config_without_store_api_and_read_only`
  (through `router().with_state`).
- Rollback: revert.

### Step 4: the accept loop, `bind` and `LiquersServer`

- Files / symbols: new `src/server/running.rs` — `LiquersServer`, the loop task, `ServerStop`
  (token and `TaskTracker`), the per-request service with the `ArcSwap` cache.
- Change: Phase 2 §Concurrency; `bind` binds `host:port` (errors per Phase 2) before spawning.
- Depends on: Step 3.
- Proof: `cargo test -p liquers-axum --test server_assembly` (all) and `--test server_shutdown
  slow_request_completes_within_deadline slow_request_is_aborted_at_deadline
  new_connections_refused_and_port_free shutdown_is_idempotent dropped_handle_keeps_serving`.
- Rollback: revert; hosts keep composing routers by hand.

### Step 5: WebSocket stop and the Close frame

- Files / symbols: `src/assets/websocket.rs` — `Outgoing` enum on the writer channel (line 796),
  `writer_task` (line 507), `handle_socket` (line 788) reading `Option<Extension<ServerStop>>`.
- Change: Phase 2 §Integration Points; with no extension, behaviour is unchanged.
- Depends on: Step 4.
- Proof: `cargo test -p liquers-axum --test server_shutdown websocket_gets_close_frame`, and the
  existing `--test assets_websocket --test argument_overrides_ws` unchanged.
- Rollback: revert this step only; shutdown then aborts sockets at the deadline without a Close.

### Step 6: `server_config_document.rs` and the example

- Files / symbols: new `tests/server_config_document.rs` (`configuration-document` Phase 3, Test 8);
  `examples/basic_server.rs` onto the builder, Ctrl-C → `shutdown(5 s)`.
- Depends on: Steps 1–5.
- Proof: `cargo test -p liquers-axum --test server_config_document`; `cargo build -p liquers-axum
  --example basic_server`.
- Rollback: revert; the example keeps its hand-built routers.

### Step 7: documentation and module docs

- Files: `specs/reference/WEB_API_SPECIFICATION.md` (§Server assembly, §Web folder, §Index page;
  History, `reviewed:`), `specs/guides/WEB_API_GUIDE.md`, `liquers-axum/README.md`,
  `docs/guides/rust/12-serving.md`, `docs/guides/web/12-serving.md` (with the read-only `web` mount
  and the stored-page warning, Decision 3); module docs of `src/server/mod.rs`, `src/web/mod.rs`,
  `src/lib.rs`, `src/assets/websocket.rs`.
- Depends on: Steps 1–6.
- Proof: `python3 scripts/check_api_docs.py liquers-axum/src/server/mod.rs liquers-axum/src/web/mod.rs
  liquers-axum/src/lib.rs liquers-axum/src/assets/websocket.rs`; `cargo run -p liquers-docs -- check`;
  `python3 scripts/docs_index.py --check`.
- Rollback: revert.

## Testing Plan

Each step's proof as it lands; at the end `cargo test -p liquers-axum` (all suites, existing ones
unchanged) and `cargo test -p liquers-lib --lib --tests` (unaffected, but it shares the core change
of `configuration-document` Step 1). No `cfg(feature)` changes in this crate.

## Rollback Plan

Steps 1–4 are additive; reverting them in reverse order leaves the crate as today. Step 5 is the
only change to existing code and reverts alone.

## Documentation Updates

As Step 7; `specs/index.csv` regenerated. User-guide chapters follow
`specs/guides/USER_DOCS_EDITING_GUIDE.md`.

## Phase 5 Entry Criteria

- [ ] Implementation finished and validated
- [ ] User and review comments answered
- [ ] Documentation checkable against implemented and tested behaviour
