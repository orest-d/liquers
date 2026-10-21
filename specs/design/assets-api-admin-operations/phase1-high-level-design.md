# Phase 1: High-Level Design — Manager status and a guarded removal

## Purpose

Two asset-manager operations that `axum-assets-endpoints` inventoried and deferred: reading the
manager's mode and readiness over HTTP, and removing a key's value **only if it is computed**, so a
cache-clearing client can never delete a user's data. Serves VISION "Server / data service" and
"Cache invalidation and freshness": a client frees derived results without the race that today
turns "clear the cache" into "maybe delete a source".

## Problem Example

A maintenance script clears derived summaries under `notes/` on a server at HEAD:

```text
GET    /liquer/api/assets/key/info/notes/a.summary.md   → {"status":"Ready", …}   (computed: safe)
                         ← meanwhile a user POSTs key/data/notes/a.summary.md: it becomes Override
DELETE /liquer/api/assets/key/data/notes/a.summary.md   → {"removed":true,"new_status":"Recipe"}
```

The user's override is deleted: `DELETE` removes whatever the key holds, and the check before it
races any write. Should be: `POST key/remove_computed/notes/a.summary.md` drops the value only if
it is computed at the moment of removal, under the manager's key lock, and otherwise answers 409
`StatusConflict` ("asset status is Override") with the override untouched. Separately, a client
cannot tell whether the server's manager queues or evaluates inline, which changes what `submit`
answers (§5.2): `GET manager` answers `{"eval_mode":"Queued","is_started":true}`.

## Scope and Acceptance Criteria

- **AC-1** Computed value removed
  WHEN `POST key/remove_computed/{key}` targets a recipe key whose value is computed (`Ready`, `Expired`, `Error`, `Cancelled`, `Volatile`, `Partial` or in flight)
  THEN the value is dropped exactly as `DELETE key/data` drops it (`Recipe` kept with its version, dependents not expired) and the answer is `{"removed": true, "new_status": "Recipe"}`
- **AC-2** User value refused
  WHEN the key's decisive status is `Source` or `Override`, or the key has no recipe
  THEN the answer is 409 `StatusConflict` naming the status and `remove_computed`, and the stored value, its dependents and any live asset are unchanged
- **AC-3** No race with a write
  WHEN a `POST key/data` and a `remove_computed` of the same key run concurrently
  THEN either the write lands after the removal (the key holds the user value) or the removal sees the user value and refuses; a user value is never deleted
- **AC-4** Nothing to remove
  WHEN the key is a recipe key with no value
  THEN the answer is 200 `{"removed": true, "new_status": "Recipe"}` and nothing is evaluated
- **AC-5** Directory and unknown keys
  WHEN the key is a directory, or neither stored, live nor recipe-declared
  THEN 409 `StatusConflict` (pointing at `key/removedir`), or 404 `KeyNotFound`, as `DELETE` answers
- **AC-6** Manager status
  WHEN `GET manager` is requested
  THEN the answer is the §2.2 envelope with `result` `{"eval_mode": "Queued" | "Inline", "is_started": bool}`
- **AC-7** Route switches
  WHEN the router is built with `read_only()`
  THEN the `remove_computed` routes are absent (404/405 like the other mutations) and `GET manager` is present; `GET key/remove_computed` exists only with `with_destructive_gets()`

Non-goals: a guarded `removedir`; removing by query (`q/`) — only keyed values have a computed or
user status; exposing other manager internals (maps, queue length); a WebSocket message for it.

## Core Interactions

- Assets: a new provided `AssetManager` method beside `remove`, sharing its decision table.
- Web/API: two Assets API routes and their builder switches.

## Crate Placement

`liquers-core` (`assets.rs`): the guard must be decided under `KeyMutationAccess::key_mutation_lock`,
which `remove` takes and which is not reentrant, so it cannot be composed in `liquers-axum`.
`liquers-axum` (`assets/`): routes, handlers, result types.

## Documentation Intent

- Reference: extend `WEB_API_SPECIFICATION.md` §5.4 (route row, removal subsection), §5.5 or a new
  row for `GET manager`, §5.6 switch table; `ASSETS.md` removal section gains `remove_computed`.
- Guide: `WEB_API_GUIDE.md` §7 "Dependencies, recovery and removal" — one example.
- Other documents: none.
- Documents to update: the source issue (closed at the end).

## Design Readiness

- **Readiness:** needs-decision
- **Automatic fixing:** not-eligible — complexity `L`: a new `AssetManager` provided method in
  `liquers-core` plus new HTTP routes (rules 1, 4, 6).
- **Leading issue:** the route and method name (Q1).
- **Explanation:** the guarded removal is `remove`'s own decision table with the `Delete` branch
  refused, so it is small and its tests mirror `remove`'s; the remaining questions are naming and
  placement, which the plan carries as assumptions.
- **Open questions:**
  1. **Open design - name:** `remove_computed` for the method and `POST key/remove_computed/{*key}`
     (recommended: says what it removes and keeps the codebase's verb `remove`, which
     `axum-assets-endpoints` Phase 2 point 5 chose over "evict"/"remove_cached"), or the issue's
     `remove_cached` (but a computed value need not be cached in memory), or a flag on `DELETE`
     (`?only=computed`; hides a different contract behind the same route).
  2. **Proposed resolution - `GET manager` placement:** at the base, always present
     (recommended: it observes and changes nothing, like `info`), rather than under `admin/`,
     which `read_only()` and `with_admin(false)` remove.
  3. **Proposed resolution - a key without a recipe:** refused as a user value (recommended: with
     no recipe nothing could recompute it), not removed.

## Design Dependencies

- `axum-assets-endpoints` (implemented) — origin; its `remove` table and route-presence rule are
  reused unchanged.
- `access-policy-at-entry-points` — **overlaps**: `remove_computed` is a `Write` operation and
  `GET manager` an `Admin` read under its policy; no order.
- `axum-websocket-connection-limits`, `unmanaged-asset-registry` — **overlaps** (same router and
  builder); no order.

## Consolidated Findings

- At HEAD neither operation exists (`builder.rs` routes; `AssetManager` has `remove`,
  `eval_mode`, `is_started` but no guarded variant). The race in the problem example is real:
  `remove` decides under `key_mutation_lock`, and a client-side check cannot be inside that lock.
- `KeyMutationAccess` is public, but taking its lock in `liquers-axum` and then calling `remove`
  deadlocks (non-reentrant); the guard belongs in core.
- Implementation: `remove`'s body moves into a private generic helper taking a `RemoveGuard`; both
  methods call it, so the two tables cannot drift. `remove`'s behaviour is unchanged (its `amr*`
  tests are the regression suite).
- `EvalMode` has no serde derive; the handler maps it with an exhaustive `match` to `"Queued"` /
  `"Inline"`, so core needs no serde change.
