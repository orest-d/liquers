---
id: EXPIRATION-RECOVERY-WEB-API
kind: issue
title: Expiration recovery has no web API surface
status: draft
priority: P2
complexity: M
area: [axum, core/assets]
design: 
created: 2026-08-08
github:
---
Source: WP-3 `expiration-safety` (see `specs/design/expiration-safety/`) — deferred follow-up.

## Problem
WP-3 added two keyed-asset recovery operations as shared default methods on the `AssetManager<E>`
trait (`liquers-core/src/assets.rs`), inherited by both `DefaultAssetManager` and
`ImmediateAssetManager`:

- `get_any_status(key) -> Result<Option<State>, Error>` — read a keyed asset's current value
  regardless of status, **including `Status::Expired`**, without triggering evaluation (for
  inspection / download / audit of an expensive expired result).
- `to_override(key) -> Result<(), Error>` — pin a keyed asset's current value as
  `Status::Override`, preserving it without recomputation (`PersistenceStatus`-aware: no
  double-serialization).

These are only reachable in-process today. There is **no web API surface**, so a browser/HTTP
client cannot inspect an expired keyed asset or promote it to `Override` — exactly the
user-directed recovery flows the feature exists to enable. This support should be added to the web
API.

## Fix direction
Expose both operations through `liquers-axum` (the assets router, `liquers-axum/src/assets/`):
1. A **recovery read** endpoint that resolves via `AssetManager::get_any_status` instead of the
   normal `get` (which treats `Expired` as a cache miss) — returning the expired/any-status state
   (data + metadata), with a clear indication in the response/metadata that the value is expired.
   It must NOT trigger evaluation and must not be the default `get` path.
2. A **promote-to-override** endpoint (mutating; POST/PUT) calling `AssetManager::to_override(key)`
   for a keyed asset, returning the resulting `Override` status.
3. Keep these on the keyed (`&Key`) surface only — there is no query-based counterpart (mirrors the
   core API, which is keyed-only by signature).
4. Consider whether the WebSocket asset stream should surface `Status::Expired` distinctly (ties
   into the WP-2 outcome contract already used there).

## Verification
`tower::ServiceExt::oneshot` handler tests in `liquers-axum`: evaluate a keyed resource, expire it,
then (a) the recovery-read route returns the stale value with expired metadata while the normal
`get` route treats it as a cache miss / recomputes, and (b) the promote route flips the asset to
`Override` so a subsequent normal `get` serves it without recomputation.
