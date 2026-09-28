---
id: ASSETS-API-ADMIN-OPERATIONS
kind: feature
title: Manager status and a guarded remove have no assets API endpoint
status: draft
priority: P3
complexity: M
area: [axum, core/assets]
design: 
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
