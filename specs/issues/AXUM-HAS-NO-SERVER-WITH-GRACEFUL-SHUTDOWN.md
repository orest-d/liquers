---
id: AXUM-HAS-NO-SERVER-WITH-GRACEFUL-SHUTDOWN
kind: feature
title: liquers-axum has no server assembly that can shut down gracefully
status: closed
priority: P2
complexity: M
area: [axum]
design: axum-server-assembly
created: 2026-11-06
github:
---

## Problem

`liquers-axum` offers route builders (`QueryApiBuilder`, `StoreApiBuilder`, `AssetsApiBuilder`,
`RecipesApiBuilder`) and leaves the server to the caller. `liquers-axum/examples/basic_server.rs`
merges them by hand and ends in a bare `axum::serve(listener, app).await`; nothing in the
repository calls `with_graceful_shutdown`. Aborting a serve future does not end connections
already spawned, and the assets WebSocket tasks are spawned separately
(`liquers-axum/src/assets/websocket.rs`, `on_upgrade` and `tokio::spawn`), so they outlive it.

## Impact

A host that must stop a server on request (liquers-py's `server.shutdown()`, a test, a CLI on a
signal) cannot do so within a deadline, and each host would re-compose the routers itself.

## Expected behaviour

One public builder that composes the standard routers under a prefix with their options, binds,
reports the bound address, and stops in two stages: stop accepting and let in-flight requests and
WebSocket tasks finish, then abort what is left at a deadline. The example uses it.

## Discovery

`liquers-py-web-server` Phase 1 review, 2026-11-06.

## Resolution

Closed 2026-11-09 by `axum-server-assembly` (PR #264). `LiquersServerBuilder` binds a
`LiquersServer` whose `shutdown(deadline)` stops accepting at once, lets requests and WebSockets
finish up to the deadline, sends each WebSocket `Close(1001)`, aborts the rest, and frees the port
before it returns; it is idempotent, and `stopped()` / `stop_token()` let other code wait or stop.
Evidence: `liquers-axum/tests/server_shutdown.rs`, `server_assembly.rs`. Reference:
`WEB_API_SPECIFICATION.md` §8.1, §8.2.
