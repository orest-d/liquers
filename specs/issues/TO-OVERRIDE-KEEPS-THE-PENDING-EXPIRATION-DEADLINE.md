---
id: TO-OVERRIDE-KEEPS-THE-PENDING-EXPIRATION-DEADLINE
kind: issue
title: A value pinned with to_override still expires at the deadline its recipe scheduled
status: draft
priority: P3
complexity: S
area: [core/assets]
design: override-ignores-recipe-deadline
created: 2026-10-20
github:
---
## Problem

`AssetRef::run` schedules a finite expiration deadline with the expiration monitor once a run
finishes (`liquers-core/src/assets.rs`, `AssetRef::run` → `schedule_expiration` → `track_expiration`).
`AssetRef::to_override` turns a live `Ready` asset into `Override` in place — the
`key/override` route "pins the current value" (`WEB_API_SPECIFICATION.md` §5.4) — but never calls
`untrack_expiration`. When the deadline fires, the monitor calls `expire_with_reason`, and
`mark_expired_status` flips `Ready | Override` to `Expired`, persists it, and the monitor
evicts the entry; the next `get` recomputes the recipe and the pinned value is gone.

By contrast an `Override` installed with `set_state` / `set_binary` is a new asset that is never
tracked, so it never expires by time. The same status therefore has two lifetimes depending on how
it was reached. Not verified by a test: found by reading the code.

Only the queued manager is affected: the inline manager's lazy deadline check applies to
`Status::Ready` only, so there the pin already survives — the two managers disagree.

## Problem Example

Recipe `data/t.txt` = `make_text/t.txt` with `expires: in 2 s`, queued manager:
`get(data/t.txt)` (`Ready`, deadline scheduled), `to_override(data/t.txt)` (`Override`), wait 2 s,
`get(data/t.txt)`. Today the monitor expired and evicted the pinned asset and the recipe runs again;
on `ImmediateEnvironment` the pinned value is served. Expected (pending the decision below): the
pinned value on both managers. (Line references re-checked at `cd6d5da`, 2026-10-26.)

## Impact

A user who pins a value (for example a stale value recovered with `get_any_status`, or a `Ready`
value to freeze it) loses the pin at the original deadline. A non-serializable value pinned this
way is lost outright.

## Expected behaviour

Probably: `to_override` untracks the asset's pending deadline (both the live-asset branch of
`DefaultAssetManager::to_override` and `AssetRef::to_override`), so an `Override` expires only by
an explicit `expire` or a dependency cascade, as one installed by `set_state` does. If deadline
expiry of an `Override` is intended, `ASSETS.md` should say so and `set_state` should track a
deadline too. Needs a maintainer decision on which.

## Discovery

Bulk-design triage of `ASSETS-IMPROVEMENTS` (sticky eviction of `Source` / `Override`),
2026-10-20, at commit 18732dd.
