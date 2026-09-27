---
id: NO-REMOTE-STORE-OR-ASSET-MANAGER
kind: feature
title: A client environment cannot use a server's store or asset manager as its own
status: draft
priority: P2
complexity: XL
area: [axum, web, core/assets, core/store]
design:
created: 2026-09-27
github:
---
## Problem

An environment's store and asset manager are always local to it. There is no `AsyncStore` that
reads and writes a Liquers server's Store API, and no `AssetManager` that is a proxy for the asset
manager of a server (`liquers-axum`). A client environment — typically `liquers-web` in a browser —
can fetch files over HTTP (`FetchStore`, read-only, from a configured key set) but cannot:

- use a server's store as a read/write store, with listing, metadata and removal;
- resolve a key or query through the server's asset manager, sharing its cache, recipes,
  evaluation, status, progress and expiration, instead of evaluating everything locally;
- exchange assets with the server as a peer: send an entry (data + metadata + version +
  dependencies) that the sending side is authoritative for, and have the receiver keep its
  dependency graph and expiration consistent with it.

The last point needs operations the HTTP API deliberately does not offer.
`specs/design/axum-assets-endpoints/` makes asset metadata **manager-owned**: the public Assets API
accepts only a value and five descriptive fields (`type_identifier`, `data_format`, `media_type`,
`title`, `description`), because trusting client-supplied `status`, `stored`, `dependencies`,
`expiration_time` or `is_error` lets a client silently corrupt an asset (details there, Q10). A
proxy asset manager needs exactly those fields to travel, so it cannot be built on the user API.

## Impact

A browser client can only be a thin viewer over the REST API, or a fully separate environment that
duplicates the server's data and computation. Nothing in between — a local environment that
delegates some keys or all evaluation to the server — can be built without inventing an ad-hoc
protocol, and an ad-hoc protocol built on the user API would either be unable to carry manager
state or would force that API to trust it.

## Expected behaviour

- A **remote store**: an `AsyncStore` over the Store API (`liquers-web`, possibly also native),
  with capabilities declared honestly and run through the store conformance suite.
- A **remote asset manager**: an `AssetManager` in the client that forwards to a server's asset
  manager, surfacing remote status and progress (the Assets WebSocket already carries them) as
  local `AssetRef` state.
- A **clear split between two APIs**:
  - the *user API* (today's `/api/assets`, `/api/store`): safe for any client, never accepts
    manager-owned metadata;
  - a *trusted system API* between a server asset manager and its proxies: carries whole entries,
    versions, dependency records and expiration, and is only reachable by an authenticated peer.
    It should be separately mountable (its own builder, off by default) so a deployment that does
    not need it does not expose it.
- Peer identity and authorization come from `CORE-SESSION-AND-KEY-ACL`; the system API should not
  invent its own.

Open design questions: which side is authoritative for a key (server always, or per key/prefix);
conflict resolution when both sides hold a version; whether the proxy caches values locally and
how expiration invalidates that cache; whether evaluation can be split (some commands local, some
remote); reconnection and missed-event recovery on the WebSocket.

## Discovery

Raised by the user while reviewing Phase 1 of `specs/design/axum-assets-endpoints/` (2026-09-27),
as the second of two legitimate reasons to write asset metadata over HTTP. The first — editing
`title`/`description` of a user-supplied `Source` asset — is in scope of that design.
