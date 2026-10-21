---
id: DOC-03-ENTRY-POINT-TABLE-SAYS-APPLY-SCHEDULES
kind: issue
title: DOC-03's entry-point table says apply schedules its asset and takes no payload
status: draft
priority: P3
complexity: S
area: [docs, core/assets]
design:
created: 2026-10-20
github:
---
## Problem

`specs/reference/api/DOC_03_ASSETS_EXECUTION_LIFECYCLE.md` §Public entry-point contract lists
`AssetManager::apply` with payload "No" and, on the queued manager, "Schedule ad-hoc asset"; the
paragraph under the table repeats that `apply` returns "a handle after scheduling". At HEAD
`AssetManager::apply(recipe, to, payload: Option<E::Payload>)` evaluates before returning on both
managers and consumes no job-queue slot (`liquers-core/src/assets.rs`, trait doc at ~6007 and
`DefaultAssetManager::apply` at ~8787: "runs inline in the caller's future rather than taking a
job-queue slot"). The same document's later section already says `apply` absorbed
`apply_immediately`, so the table contradicts its own text. The following-handle variant is
`apply_queued`, which the table does not list.

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
