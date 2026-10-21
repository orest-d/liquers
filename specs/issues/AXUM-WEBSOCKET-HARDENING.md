---
id: AXUM-WEBSOCKET-HARDENING
kind: issue
title: WebSocket endpoint is not hardened
status: accepted
priority: P3
complexity: M
area: [axum]
design: axum-websocket-connection-limits
created: 2026-08-08
github:
---
## Problem

WP-16 paired two things: `listdir_keys(_deep)` routes, and hardening the WebSocket endpoint. The
routes exist — `listdir_keys` is in use at `liquers-axum/src/store/handlers.rs:246`, `:347`,
`:349` — so only the hardening half remains.

What "hardening" covers was not enumerated: connection limits, message-size caps, timeouts, and
behaviour when a client disconnects mid-evaluation are the obvious candidates.

## Impact

An endpoint that holds a connection for the length of an evaluation is exposed to slow-client and
unbounded-message failure modes. Scope it before scheduling.

## Expected behaviour

Stated limits, enforced, with tests for the disconnect-mid-evaluation case.

## Discovery

Migration triage, 2026-08-08. Source: work package WP-16. Verified against HEAD: the listdir half is implemented; the hardening half is unscoped. See `specs/archive/2026-08-08-docs-migration-plan.md` §4.0c.

## Progress

Progress 2026-09-28 (`design/axum-assets-endpoints/`, I6): `WebSocketLimits` —
`max_message_size` (default 64 KiB; a larger message closes the connection) and
`max_subscriptions` per connection (default 256; beyond it an `Error` reply); a bounded outgoing
queue that drops intermediate notifications when full but always delivers replies and terminal
notifications; disconnect aborts every subscription task without cancelling the evaluations.
Evidence: AWS12a, AWS12b, AWS13. Still open: connection-count limits, idle timeouts and
server-initiated keep-alive, and a slow-consumer policy beyond the bounded queue.

## Status at 18732dd (2026-10-20)

Still reproduces for the open half. `liquers-axum/src/assets/websocket.rs`: `WebSocketLimits` has
only `max_message_size` and `max_subscriptions`; `handle_socket` reads with no deadline and sends no
Ping; `writer_task` awaits `sink.send` with no deadline, so a client that stops reading stalls its
writer, then `Session::reply` and terminal notifications (`tx.send(..).await`), and the connection
is never ended. Nothing counts connections.

Problem example: subscribe on `ws/q/make_text/upper` and stop reading; once the socket buffer is
full the connection, its 64-slot queue and its subscription tasks are held indefinitely. Expected:
server pings, an idle close (1001), a write deadline, and a connection cap (close 1013), each with a
stated default — designed in `specs/design/axum-websocket-connection-limits/` (the origin design,
`axum-assets-endpoints`, is implemented and frozen).

