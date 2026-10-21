---
id: UNMANAGED-ASSET-NOT-FOLLOWABLE-ACROSS-CONNECTIONS
kind: feature
title: An unmanaged asset can be followed only by the connection that requested it
status: draft
priority: P3
complexity: M
area: [axum, core/context]
design: unmanaged-asset-registry
created: 2026-10-12
github:
---

## Problem

An **unmanaged asset** — one in neither of the asset manager's maps: evaluated with argument
overrides, volatile, from a `cached: false` command, or not retained by the cache strategy (terms
defined by `design/axum-query-named-arguments/`) — is visible only to whoever holds its `AssetRef`.
Under that design the WebSocket gives such an asset a subscription id, but the id is valid only on
the connection that requested it (decision Q15, option (a)). So:

- `q/submit` of an unmanaged asset over HTTP is fire-and-forget: its status and result cannot be
  read later, by `q/info`, by `q/data`, or by a WebSocket subscription;
- a WebSocket client that reconnects cannot resume following an evaluation it started.

## Impact

Low while the workaround holds: request the result with `q/data` (waits), or start and follow the
evaluation in one WebSocket `subscribe` with `arguments`, on one connection. A client that must
submit over HTTP and follow later, or survive a reconnect, has no route.

## Expected behaviour

A server-wide registry of unmanaged assets keyed by a server-issued UUID v4: `q/submit` returns the
id; `subscribe` / `cancel` by id work from any connection; possibly HTTP `id/info`, `id/data`,
`id/cancel`. The analysis recorded in `design/axum-query-named-arguments/phase1-high-level-design.md`
(Q10, option (b)) governs it:

- the id is a bearer capability — whoever has it reads the result and can cancel. UUID v4 defeats
  guessing, but ids leak through logs, proxies, URLs and shared links, and override values (search
  terms) can be more sensitive than the query. **Design this once sessions exist**
  (`CORE-SESSION-AND-KEY-ACL`), so an id is checked against the session that created it;
- it needs a retention policy — a TTL after the terminal status, a capacity cap against memory
  exhaustion — and must say how that squares with "volatile: used once".

## Discovery

Decided out of scope while designing `axum-query-named-arguments` (Phase 1, Q15, 2026-10-12): the
user chose connection-scoped ids, to be extended to sessions once sessions are supported.

## Status at 18732dd (2026-10-20)

Still reproduces: `SubmitResult` (`liquers-axum/src/assets/common.rs`) carries no id, and
`Session::cancel` / `unsubscribe_id` (`websocket.rs`) look only in the connection's own table.

Problem example: `POST q/submit/ns-search/search?query=liquers&limit=20` answers `managed: false`
with a note, and `q/info/ns-search/search` is 404 `NotAvailable`; nothing can read the result
later. Expected (opt-in): the submit answers an `id`, and `id/info`, `id/data`, `id/cancel` and a
WebSocket `subscribe` by `id` follow it from anywhere, with bounded retention. Designed in
`specs/design/unmanaged-asset-registry/`; its leading decision is whether ids may ship as bearer
capabilities before `CORE-SESSION-AND-KEY-ACL` (`access-policy-at-entry-points`).

