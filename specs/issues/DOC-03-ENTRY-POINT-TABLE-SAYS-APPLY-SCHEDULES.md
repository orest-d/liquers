---
id: DOC-03-ENTRY-POINT-TABLE-SAYS-APPLY-SCHEDULES
kind: issue
title: DOC-03's entry-point table says apply schedules its asset and takes no payload
status: closed
priority: P3
complexity: S
area: [docs, core/assets]
design: doc-03-apply-entry-points
created: 2026-10-20
github:
---
## Problem

`specs/reference/api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md` §Public entry-point contract lists
`AssetManager::apply` with payload "No" and, on the queued manager, "Schedule ad-hoc asset"; the
paragraph under the table repeats that `apply` returns "a handle after scheduling". At HEAD
`AssetManager::apply(recipe, to, payload: Option<E::Payload>)` evaluates before returning on both
managers and consumes no job-queue slot (`liquers-core/src/assets.rs`, the `AssetManager::apply` trait doc and
`DefaultAssetManager::apply`: "runs inline in the caller's future rather than taking a
job-queue slot"). The same document's later section already says `apply` absorbed
`apply_immediately`, so the table contradicts its own text. The following-handle variant is
`apply_queued`, which the table does not list.

## Problem Example

A reader wants the result of a recipe applied to a state. DOC-03's table says `apply` on the queued
manager "Schedule[s] ad-hoc asset", so they write `manager.apply(recipe, state, None).await?` and
then wait on the handle; or, wanting a handle to follow a long run, they pick `apply` and get a
finished asset instead. At HEAD (`cd6d5da`, re-checked 2026-10-26) `apply` has already evaluated
when it returns; the followable variant is `apply_queued`.

## Impact

A reader choosing between `apply`, `apply_queued` and `get_asset` from DOC-03 picks the wrong one,
and may wrap `apply` in a wait it does not need.

## Expected behaviour

The table rows for `apply` (payload optional, evaluates before return on both managers) and a row
for `apply_queued` (queued handle, takes a slot, not from inside an evaluation —
`ASSET_LIFECYCLE.md` §entry points), with the paragraph below corrected; a History row and
`reviewed:` bump.

## Discovery

Bulk-design triage of `EXTENDED-FAST-TRACK`, 2026-10-20, checking its `apply()` items against
HEAD (commit 18732dd).

## Resolution

Fixed 2026-10-27 by `design/doc-03-apply-entry-points/`. Fixed by `design/doc-03-apply-entry-points/` (2026-10-27): DOC-03 §Public entry-point contract describes `apply` (evaluates before return, optional payload), `apply_queued` and the deprecated `apply_immediately` as implemented.
