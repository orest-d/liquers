---
id: ASSETS-API-ADMIN-OPERATIONS
kind: feature
title: Manager status and a guarded remove have no assets API endpoint
status: draft
priority: P3
complexity: L
area: [axum, core/assets]
design: assets-api-admin-operations
created: 2026-09-27
github:
---
## Problem

`specs/design/axum-assets-endpoints/` inventoried every `AssetManager` method and exposes only the
documented assets endpoints plus what the agent memory MVP requires or can use. These
client-meaningful operations were deliberately left without an endpoint:

| Operation | Endpoint it would get |
|---|---|
| `eval_mode`, `is_started` | `GET manager` |
| a guarded `remove` refusing `Source` / `Override` (for cache-clearing clients) | `POST remove_cached` |

## Impact

Low. Manager status is a deployment detail; a cache-clearing client can check the status with
`GET info` before calling `DELETE`, but that races a concurrent write — a `Source` written in
between is deleted — which is what `remove_cached` would close. (`GET remove` was brought into scope by the design's Q25, opt-in as in the Store API.)

## Expected behaviour

Add endpoints as concrete use cases appear, each following the conventions the design above sets:
the §3 envelope, key-only for mutations, refusals as `NotSupported` or 409, and mutations behind
`AssetsApiBuilder::read_only()`.

## Discovery

Scoping decision in Phase 1 of `specs/design/axum-assets-endpoints/`, 2026-09-27: the user asked
that new endpoints be justified by the agent memory MVP.

## Status at 18732dd (2026-10-20)

Still missing: `liquers-axum/src/assets/builder.rs` registers neither route, and `AssetManager`
(`liquers-core/src/assets.rs`) has `remove`, `eval_mode` and `is_started` but no guarded removal.

Problem example: `GET key/info/notes/a.summary.md` answers `Ready`; a user then `POST`s
`key/data/notes/a.summary.md` (now `Override`); the client's `DELETE key/data/notes/a.summary.md`
deletes the user's override. Expected: a removal that refuses a `Source` / `Override` atomically
(409), and `GET manager` answering `{"eval_mode": "Queued", "is_started": true}`.

Re-sized `M` → `L` by the bulk design (`specs/design/assets-api-admin-operations/`): the guard must
be decided under the manager's key-mutation lock, which `remove` takes and which is not reentrant,
so it needs a new provided `AssetManager` method in `liquers-core` beside the `liquers-axum` routes.

