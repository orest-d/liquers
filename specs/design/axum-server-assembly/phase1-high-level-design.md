# Phase 1: High-Level Design — axum-server-assembly

## Purpose

Give `liquers-axum` one server that a host starts instead of composing routers by hand: the standard
APIs under a prefix, static web content served from a predefined `web` folder of the store, an index
page at `/`, and a shutdown that completes within a deadline. LiQuer, the Python predecessor, served
`/liquer/web/…` from a `web` mount point of its store and redirected `/` and `/index.html` to its
start page (`liquer/server/blueprint.py`, `liquer/store.py` `get_web_store`); the Rust server has
neither. It serves `VISION.md`'s data services ("reports, charts, extracts and dashboards become
links") and is the prerequisite of [`liquers-py-web-server`](../liquers-py-web-server/DESIGN.md)
(M7); a CLI server and `WEBAPI` (M8) reuse it.

## Design Readiness

- **Readiness:** `needs-decision`: Phase 1 written and reviewed twice; Orest decided questions 2
  and 3 on 2026-11-06 (Decisions 1 and 2); approved 2026-11-07 ("proceed"). Phase 2 (2026-11-07) adds: the `EnvSource` (see Non-goals), a
  server that keeps running when its handle is dropped (question 7), and `Cache-Control: no-cache`.
- **Automatic fixing:** `not-eligible`: new public API of `liquers-axum`, complexity `L`.
- **Leading issue:** `AXUM-CANNOT-SERVE-STATIC-WEB-CONTENT` (P2 L). Source issues
  `AXUM-CANNOT-SERVE-STATIC-WEB-CONTENT` and
  `AXUM-HAS-NO-SERVER-WITH-GRACEFUL-SHUTDOWN` (P2 M), merged into this design on 2026-11-06 because
  both change the same site (overlap test T3): the builder and its `ServerConfig`, and
  `examples/basic_server.rs`. If question 2 or 3 holds up M7, the shutdown half can be split off.
- **Decisions:**
  1. Orest, 2026-11-06, on question 2: "For now it is acceptable store-only reads, later it can be
     configurable."
  2. Orest, 2026-11-06, on question 3: "for now redirect only is fine, later other solutions might
     be added - e.g. a) default recipes for generating responses, e.g. index.html b) configurable
     redirect c) response Value that would allow to create a redirect in commands."
  3. Orest, 2026-11-06, on question 6: "the security section should explicitly warn about this and
     show how to configure /web store as read only. In future versions it may be configured as read
     only by default."
  4. Orest, 2026-11-07: one configuration file for every aspect, assembled from per-section structs
     (`liquers-py-web-server` Decision 4); `ServerConfig` reads its section from core's
     `ConfigDocument` (Phase 2).
- **Explanation:** the four route builders exist (`liquers-axum/src/*/builder.rs`), each a
  `Router<EnvRef<E>>`; `examples/basic_server.rs` merges them and calls a bare `axum::serve`. axum
  0.8's graceful shutdown has no deadline, its connection tasks are detached, and the assets
  WebSocket tasks are spawned at upgrade (`assets/websocket.rs`), outside it. `MetadataRecord` has
  no ETag or modification time: those exist only in a store's `DataFacts`, and only where the
  backend reports them. Nothing serves a store folder as a website or answers `/`.
- **Open questions:**
  1. **Proposed resolution: one builder and its settings.** `LiquersServerBuilder::new(prefix)` with
     the builders' switches (`read_only`, `without_websocket`, timeouts), an extensible set of APIs
     (later ones such as registry publication plug in), `web` and `ServerConfig` (serde,
     in `liquers-axum`, `deny_unknown_fields`) reads the `server` section of a configuration
     document (`ServerConfig::from_yaml`, `from_toml`; Phase 2); core's `EnvironmentConfig` is not changed. Keys:
     `host`, `port`, `prefix`, `apis`, `read_only`, `web` (the root key, default `web`; `false`
     turns the route off). `bind` returns a running `LiquersServer` with its bound address
     and `shutdown(deadline)`. No CORS layer, deliberately, as today.
  2. **Decided (Decision 1): web content is read from the store only.** `GET {prefix}/web/{path}`
     answers with the stored bytes of key `<root>/{path}`, read from the store on each request
     (`get_data` with its metadata), so an edited file is served at once and nothing stays in
     memory; a key that is not stored is 404, without touching the asset manager, even when a
     recipe could make it. Serving computed content may become a setting later
     (`WEB-FOLDER-SERVES-STORED-FILES-ONLY`, filed with this decision).
  3. **Decided (Decision 2): the index page is a fixed redirect.** `GET /` and `GET /index.html`
     redirect to `{prefix}/web/`, so a web app's relative links resolve under `web/`; when
     `web/index.html` does not exist, that request is an ordinary 404. Later options (default
     recipes that generate responses such as `index.html`, a configurable redirect, a response
     value with which a command redirects) are recorded in `INDEX-PAGE-IS-A-FIXED-REDIRECT`.
  4. **Proposed resolution: the web folder is part of the store.** A configuration mount puts a
     filesystem folder at `web` (`prefix: web`, `type: filesystem`), several mounts can share it
     (`web/app1`, `web/app2`), and the store and assets APIs reach it too. Whether it is writable is
     the mount's `read_only`: a store can be made read-only only per mount, or as a whole with the
     router's `read_only` (`STORE_CONFIG_FSD.md`), not per key by a top-level `store_config` spec
     (`STORE-CONFIG-SPEC-CANNOT-SCOPE-HANDLERS-TO-A-KEY`, design `store-config-key-scoped-handlers`).
     The docs show the read-only `web` mount (question 6, Decision 3).
  5. **Proposed resolution: HTTP details of the web route.** `Content-Type` from the metadata's
     media type, else from the key's extension (`liquers-core/src/media_type.rs`: `.html`, `.js`,
     `.css`, `.wasm`, `.svg`, …), else `application/octet-stream`, with
     `X-Content-Type-Options: nosniff`. A folder addressed with a trailing slash gives its
     `index.html`; without one it is redirected (308) to the slash form, so relative links work.
     `{prefix}/web` and `{prefix}/web/` have their own routes. `ETag` is the content hash
     (`Version::from_content`), so it exists whatever the backend reports; `Last-Modified`
     only when the store's `data_facts` reports one; a matching `If-None-Match` answers 304. `HEAD`
     is answered like `GET` without the body. A path that is not a valid absolute key is 404. Key
     names allow letters, digits, `_`, `.` and `-`, so some bundler filenames (`chunk~x.js`,
     `[slug]`, `@scope`) cannot be served (`KEY-NAMES-CANNOT-HOLD-COMMON-WEB-FILENAMES`, filed with
     this design).
  6. **Proposed resolution: name the risks, for the security guideline.** With writes enabled, any
     client that reaches the server can store `web/x.html`, and the page then runs on the same
     origin as the write APIs (stored cross-site scripting); a query `GET` can run a command that
     writes, whatever `read_only` says; under question 2 an anonymous `GET` can start a
     computation. Each goes into `SECURITY-SENSITIVE-SETTINGS-ARE-NOT-DOCUMENTED`, with the remedy
     (a read-only `web` mount, `127.0.0.1`, a proxy). The security section warns about the first
     explicitly and shows the read-only `web` mount (Decision 3); a later version may make `web`
     read-only by default.
  7. **Proposed resolution: shutdown in two stages.** The server owns its accept loop
     (`hyper-util`'s connection builder and graceful watcher, connections in a `JoinSet`): on
     `shutdown(deadline)` it stops accepting, lets in-flight requests finish, signals WebSocket
     tasks through a cancellation token so they send a Close frame, and aborts what is left at the
     deadline; the port is free when `shutdown` returns. Aborting a request does not cancel an
     evaluation it started: draining or cancelling the environment is the host's choice
     (`environment-orderly-shutdown`). Dropping the handle does not stop the server (added in Phase 2,
     for `liquers-py-web-server` Decision 2).

## Problem Example

```yaml
store:
  stores:
    - type: filesystem
      prefix: web
      read_only: true
      config: { path: ./site }          # ./site/index.html, ./site/app.js
server:
  port: 8080
```

**Today** a host must merge four routers itself; `GET /liquer/web/` is 404, `GET /` is 404, and
stopping the server means dropping a future while connections keep running. **After**
`LiquersServer` started from this configuration answers `GET /` with a redirect to `/liquer/web/`,
which returns `./site/index.html` as `text/html`; `GET /liquer/web/app.js` returns the script; and
`shutdown(5 s)` returns with the port free.

## Scope and Acceptance Criteria

- **AC-1** Static content from the web folder
  WHEN `GET {prefix}/web/app.js` names a stored file
  THEN the response is its current bytes with the metadata's media type, and a missing key is 404
- **AC-2** Folders
  WHEN `GET {prefix}/web/` or `GET {prefix}/web/docs/` names a folder holding `index.html`, and
  `GET {prefix}/web/docs` omits the slash
  THEN the first two return that `index.html`, and the third redirects (308) to `…/docs/`
- **AC-3** The index page
  WHEN `GET /` or `GET /index.html` arrives
  THEN it redirects to `{prefix}/web/` (Decision 2)
- ~~**AC-4** Computed web content~~ — retired 2026-11-06 by Decision 1: a key that is not stored
  is 404 (`WEB-FOLDER-SERVES-STORED-FILES-ONLY`)
- **AC-5** Content types without metadata
  WHEN a file with no media type in its metadata (a plain filesystem mount, or text stored from a
  binding) is requested as `index.html`, `app.js` or `app.wasm`
  THEN it is served as `text/html`, `text/javascript` or `application/wasm`, with `nosniff`
- **AC-6** Conditional requests
  WHEN a request for a stored page repeats with the `ETag` it received
  THEN the response is 304 with no body, and after the file changes it is 200 with the new bytes
- **AC-7** One builder for the standard APIs
  WHEN a host builds the server from a `ServerConfig` that disables the store API and sets
  `read_only`
  THEN no store route exists and every assets mutation is refused, the others answering as today
- **AC-8** Bound address and startup errors
  WHEN the server binds port 0, or a port already in use, or a `server` section has an unknown key
  THEN it reports the bound port, or returns a typed `Error` with nothing left listening
- **AC-9** Graceful shutdown within a deadline
  WHEN `shutdown(d)` is called with a slow request and an open WebSocket in flight
  THEN new connections are refused, the request completes if it ends within `d`, the WebSocket gets
  a Close frame, everything is gone after `d`, and the port is free

Non-goals: authentication, TLS and CORS; directory listings over `web/`; range requests and
streaming (`store-streaming-read-write`); Python handlers; rebuilding an environment (the server
rebuilds nothing; since Phase 2 a host may hand it an `EnvSource` that returns its current build,
`liquers-py-web-server` question 4).

## Core Interactions

Web/API: a server builder, the `web` and index routes, the accept loop. Store: `web/` keys
read from the store. Configuration: a `server` section.

## Crate Placement

`liquers-axum` only (core and store dependencies, plus `hyper-util` and `tokio-util`, already in the
lock file): new `server` and `web` modules; `assets/websocket.rs` takes the cancellation token;
`examples/basic_server.rs` moves onto the builder.

## Documentation Intent

- Reference: `WEB_API_SPECIFICATION.md` gains the web and index routes and the server builder.
- Guide: `WEB_API_GUIDE.md`, "Start a server" and "Serve a web application from the store".
- Other documents: `liquers-axum/README.md`; `specs/README.md`.
- Documents to update: user guide chapters `docs/guides/{rust,python,web}/12-serving.md`; the
  risks of question 6 go to the security guideline and the guides' security chapter
  (`SECURITY-SENSITIVE-SETTINGS-ARE-NOT-DOCUMENTED`).
- API docs: the module docs of the new `liquers-axum/src/server` and `web` modules and of
  `liquers-axum/src/lib.rs`.

## Design Dependencies

- `configuration-document` (2026-11-07, Decision 4): **requires**; the `server` section is read from it.
- `liquers-py-web-server`: **required-by**.
- `environment-orderly-shutdown` (in review): **overlaps** question 7, the server stop it names.
- `http-request-body-limit` (in review): **overlaps**; it also edits `examples/basic_server.rs`,
  so whichever lands second rebases. `http-accept-negotiation`, `axum-websocket-connection-limits`,
  `asset-events-over-websocket` (a second WebSocket shutdown must close), `unmanaged-asset-registry`,
  `registry-publication` (a fifth API), `access-policy-at-entry-points` (in review): **overlap**;
  their settings and routes become builder settings.
- `store-streaming-read-write`, `value-serialization-streaming` (in review): **overlap**; web
  responses are buffered whole until they land.
- `web-api-spec-read-only-mount` (complete): the way to make `web/` read-only.

## References

`liquers-axum/src/*/builder.rs`, `assets/common.rs`, `assets/websocket.rs`, `axum_integration.rs`;
`liquers-core/src/media_type.rs`, `metadata.rs` `Version::from_content`, `store.rs` `DataFacts`,
`environment_config.rs`; `reference/STORE_SEMANTICS.md`; LiQuer `liquer/server/blueprint.py`.
