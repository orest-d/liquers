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

- **Readiness:** `needs-decision`: Phase 1 written; questions 2 and 3 wait for the Phase 1 gate.
- **Automatic fixing:** `not-eligible`: new public API of `liquers-axum`, complexity `L`.
- **Leading issue:** question 2, whether web content may be computed. Source issues
  `AXUM-CANNOT-SERVE-STATIC-WEB-CONTENT` (P2 M, filed with this design) and
  `AXUM-HAS-NO-SERVER-WITH-GRACEFUL-SHUTDOWN` (P2 M).
- **Explanation:** the four route builders exist (`liquers-axum/src/*/builder.rs`), each a
  `Router<EnvRef<E>>`; `examples/basic_server.rs` merges them and calls a bare `axum::serve`, which
  cannot stop on request, and the assets WebSocket tasks are spawned apart from it
  (`assets/websocket.rs`). Nothing serves a store folder as a website or answers `/`.
- **Open questions:**
  1. **Proposed resolution: one builder and its settings.** `LiquersServerBuilder::new(prefix)` with
     switches for each API (as the builders have them: `read_only`, `without_websocket`, timeouts),
     `web(root_key)` and `index(target)`, built from a serde `ServerConfig` (host, port, prefix,
     apis, read_only, web, index) that any host can read from its configuration document. `bind`
     returns a running `LiquersServer` with its bound address and `shutdown(deadline)`.
  2. **Open design: web content as stored files or as assets.** `GET {prefix}/web/{path}` answers
     with store key `web/{path}`: a path ending in `/`, or naming a folder, gets its `index.html`
     (LiQuer's rule); `Content-Type` comes from the metadata's media type; `GET` and `HEAD` only.
     Recommended: read it through the asset manager, as `GET …/api/assets/key/data/…` does, so a
     stored file is returned as is and a recipe in `web/` (a page rendered from a template, a chart)
     is computed once and cached: "everything is an asset". Alternative: stored bytes only, as LiQuer
     did, which never computes and is simpler to cache.
  3. **Open design: the index page.** Recommended: `GET /` and `GET /index.html` redirect to
     `{prefix}/web/` (so a web app's relative links resolve under `web/`), and `index` may name another
     path or a query (`{prefix}/q/…`) to redirect to. When the target does not exist, a small built-in
     page names the server version and links the API roots instead of a bare 404. Alternative: serve
     the index content at `/` itself, without a redirect.
  4. **Proposed resolution: the web folder is part of the store.** `web` is a key prefix of the
     environment's store, not a second store: a configuration mount puts a filesystem folder there
     (`prefix: web`, `type: filesystem`), several mounts can share it (`web/app1`, `web/app2`), and its
     content is also reachable through the store and assets APIs. Whether it is writable is the
     mount's `read_only`. No mount at `web` means the route answers 404.
  5. **Proposed resolution: shutdown in two stages.** The server owns its accept loop: on
     `shutdown(deadline)` it stops accepting, lets in-flight requests and WebSocket tasks finish
     (they receive a cancellation signal and close their sockets), and aborts what is left at the
     deadline; the port is free when `shutdown` returns.
  6. **Proposed resolution: caching headers.** Web responses carry `ETag` and `Last-Modified` from
     the metadata, and a conditional `GET` answers 304, so a browser does not re-download an app.

## Problem Example

```yaml
store:
  stores:
    - type: filesystem
      prefix: web
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
  WHEN `web/app.js` is stored with media type `text/javascript` and `GET {prefix}/web/app.js` arrives
  THEN the response is its bytes with that `Content-Type`, and a missing key answers 404
- **AC-2** A folder serves its index.html
  WHEN `GET {prefix}/web/` or `GET {prefix}/web/docs` names a folder holding `index.html`
  THEN that file is returned
- **AC-3** The index page
  WHEN `GET /` or `GET /index.html` arrives
  THEN the response redirects to `{prefix}/web/`, or to the configured `index` target; with no
  target present, the built-in page is returned (question 3)
- **AC-4** Computed web content (question 2)
  WHEN `web/report.html` is a recipe and is requested twice
  THEN it is computed once and both responses return the same page
- **AC-5** Conditional requests
  WHEN a request repeats with the `ETag` it received
  THEN the response is 304 with no body
- **AC-6** One builder for the standard APIs
  WHEN a host builds the server from a `ServerConfig` that disables the store API and sets
  `read_only`
  THEN no store route exists and every assets mutation is refused, the others answering as today
- **AC-7** Bound address and startup errors
  WHEN the server binds port 0, or a port already in use
  THEN it reports the bound port, or returns a typed `Error` with nothing left listening
- **AC-8** Graceful shutdown within a deadline
  WHEN `shutdown(d)` is called with a slow request and an open WebSocket in flight
  THEN new connections are refused, the request completes if it ends within `d`, the WebSocket is
  closed, everything is gone after `d`, and the port is free

Non-goals: authentication and TLS; directory listings over `web/`; Python handlers; following an
environment rebuild (the host gives the server its environment; `liquers-py-web-server` question 4).

## Core Interactions

Web/API: a new server builder and the `web` and index routes. Store and assets: `web/` keys read
through the asset manager. Configuration: a `server` section.

## Crate Placement

`liquers-axum` only (core and store dependencies, unchanged): a new `server` module and a `web`
module; `examples/basic_server.rs` moves onto it.

## Documentation Intent

- Reference: `WEB_API_SPECIFICATION.md` gains the web and index routes and the server builder.
- Guide: `WEB_API_GUIDE.md`, "Start a server" and "Serve a web application from the store".
- Other documents: `liquers-axum/README.md`; `specs/README.md`.
- Documents to update: the serving chapters `docs_index.py chapters-for` lists; the server settings
  join the security guideline (`SECURITY-SENSITIVE-SETTINGS-ARE-NOT-DOCUMENTED`).
- API docs: the module docs of the new `liquers-axum/src/server` and `web` modules and of
  `liquers-axum/src/lib.rs`.

## Design Dependencies

- `liquers-py-web-server`: **required-by**.
- `http-accept-negotiation`, `http-request-body-limit`, `axum-websocket-connection-limits`,
  `access-policy-at-entry-points` (in review): **overlap**; their settings become builder settings.
- `web-api-spec-read-only-mount` (complete): the way to make `web/` read-only.

## References

`liquers-axum/src/*/builder.rs`, `examples/basic_server.rs`, `assets/websocket.rs`;
`reference/WEB_API_SPECIFICATION.md`; LiQuer `liquer/server/blueprint.py` (`/`, `/index.html`,
`/web/<path>`).
