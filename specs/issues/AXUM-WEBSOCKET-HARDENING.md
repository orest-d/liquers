---
id: AXUM-WEBSOCKET-HARDENING
kind: issue
title: WebSocket endpoint is not hardened
status: draft
priority: P3
complexity: M
area: [axum]
design: 
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
