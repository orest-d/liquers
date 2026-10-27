---
id: REMOTE-ASSET-MANAGER-AND-SYSTEM-API
kind: feature
title: A client environment cannot delegate evaluation to a server's asset manager
status: draft
priority: P2
complexity: XL
area: [axum, core/assets, web]
design: remote-asset-manager
created: 2026-10-20
github:
---

## Problem

Split off from `NO-REMOTE-STORE-OR-ASSET-MANAGER` (`design/remote-store-and-asset-manager/`), whose
slice 1 covers only the remote store. There is still no `AssetManager` that forwards to a server's
asset manager (`liquers-core/src/assets.rs` has `DefaultAssetManager` and `ImmediateAssetManager`,
both local), and no trusted system API to carry what such a proxy needs — whole entries, versions,
dependency records, expiration — which the user Assets API deliberately refuses
(`design/axum-assets-endpoints/` Q10: manager-owned metadata).

## Problem Example

A desktop client mounts the server's store at `shared` (slice 1) and asks for
`shared/report.html`, whose recipe needs Polars and an hour of CPU on the server. Today (once
slice 1 lands) the client either reads a stored copy or evaluates the server's recipe itself —
downloading the inputs and writing the result back as a client-written value — and sees none of
the server's progress or later expiration. It should ask the server's asset manager and mirror its
status, progress and expiration as a local `AssetRef`.

Design note (2026-10-26, `design/remote-asset-manager/`): under prefix authority (the server owns
every key under a delegated prefix) the client never writes manager-owned metadata, so the first
slice needs only the user Assets API, `ws/key` and the manager event stream over WebSocket; the
trusted system API is needed only for replication or split evaluation.

## Impact

A client either evaluates everything locally or is a thin REST viewer. Realm dispatch to a server
(`design/multi-realm-dispatch/`) has no evaluation to delegate to beyond `GET q/entry`, which
cannot report progress or expiration of the remote part.

## Expected behaviour

A proxy `AssetManager` surfacing remote status and progress (the Assets WebSocket already carries
them) as local `AssetRef` state, and a separately mountable, off-by-default system API reachable
only by an authenticated peer. Open, and blocking a design: which side is authoritative for a key
(server always, or per prefix); conflicts when both hold a version; local caching and how
expiration invalidates it; split evaluation; reconnection and missed-event recovery. Peer identity
comes from `CORE-SESSION-AND-KEY-ACL` (`design/access-policy-at-entry-points/`).

## Discovery

Bulk design 2026-10-20 (cluster C6), split from `NO-REMOTE-STORE-OR-ASSET-MANAGER`.
