---
id: ASSET-EVENTS-ARE-NOT-DELIVERED-OUT-OF-PROCESS
kind: feature
title: Manager-level asset events cannot be followed from another process
status: draft
priority: P3
complexity: M
area: [axum]
design:
created: 2026-10-21
github:
---

## Problem

`external-index-sync` designs an in-process, scoped event stream on the asset manager
(`AssetManager::subscribe_asset_events`). An observer in another process (an index feeder, a cache
warmer, a second Liquers instance) has no way to receive it. The assets WebSocket in
`liquers-axum/src/assets/websocket.rs` follows individual assets only: it maps the per-asset
`AssetNotificationMessage::Expired`.

## Impact

Out-of-process observers poll, so an external view is as stale as its polling interval.
Reconciliation stays correct either way. Workaround: an in-process bridge written by the host.

## Expected behaviour

A WebSocket message (or route) that subscribes to a scope (`Under(key)` or all) and forwards
`AssetEvent`s as JSON, including the `Lagged(n)` signal, so the client knows to reconcile. It needs
a serde form of `AssetEvent` and must respect the connection limits of
`axum-websocket-connection-limits`.

## Discovery

Split off on 2026-10-21 from `ASSET-EXPIRATION-EVENTS-CANNOT-BE-OBSERVED-EXCEPT-PER-ASSET` (its
question on in-process versus out-of-process delivery) during bulk design of `external-index-sync`.
Depends on that design being implemented.
